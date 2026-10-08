//! Play-script assertions (dsl 0.22.0 §4).
//!
//! A play step MAY carry `expect: { winner, offered, notOffered, presented,
//! quests, state, facts, notFacts }` and a script MAY carry a top-level
//! `expect: { end, quests, state, facts, notFacts, transcriptContains,
//! transcriptLacks }`. The first four step keys judge a step's selection:
//! an `occasion` step's, or what an `advance:` step raised — `presented`
//! every raise in order (each midnight's `dayEnd` / `dayStart`, then the
//! slot occasion), `winner` / `offered` / `notOffered` its last raise, where
//! the clock stops (dsl 0.27.0, T3-8). The world keys (`quests`, `state`,
//! `facts`, `notFacts`, 0.23.1) judge the world right after the step
//! settled, on any step kind.
//! [`crate::play`] parses the script, calls [`validate`] on every `expect:`
//! at parse time (an unknown key or a malformed value is a usage error, exit
//! 2), walks the play, fills a [`PlayOutcome`] and hands both to [`check`].
//! Every miss names its step (and `label:`) and the actual value; `lute
//! play` exits 1 on any miss, and `lute test` reports a play with a miss as
//! FAIL.
//!
//! This module judges; it never walks. What a step presented, what was
//! eligible, and the state/facts/quests after a step and at the end are
//! exactly what the play walk recorded — there is no second model of the
//! playthrough here.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use lute_runtime::Value;
use serde_yaml::Value as Yaml;

/// The complete legal key set of a STEP `expect:`.
pub(crate) const STEP_EXPECT_KEYS: &[&str] = &[
    "clock",
    "facts",
    "notFacts",
    "notOffered",
    "offered",
    "options",
    "presented",
    "quests",
    "state",
    "winner",
];

/// The step keys that judge an occasion's selection — legal only on an
/// `occasion` or `advance` step.
const OCCASION_STEP_KEYS: &[&str] = &["notOffered", "offered", "presented", "winner"];

/// The keys that judge the world (state, facts, quest statuses, the clock)
/// — on a step, right after it settled; at the top level, at the end
/// (`clock` is a step key only).
const WORLD_KEYS: &[&str] = &["clock", "facts", "notFacts", "quests", "state"];

/// The keys a step `expect.clock` may name (dsl 0.26.0 §7, T2-5).
const CLOCK_KEYS: &[&str] = &["day", "ended", "slot", "weekday"];

/// The first occasion-only key a step `expect:` carries — a usage error on a
/// step that raises no occasion.
pub(crate) fn occasion_key(expect: &Yaml) -> Option<&'static str> {
    OCCASION_STEP_KEYS
        .iter()
        .copied()
        .find(|k| expect.get(k).is_some())
}

/// Does this step `expect:` judge the world after the step? The play then
/// snapshots it ([`WorldView`]); `facts` says whether the derived facts are
/// needed too (a fixpoint, so only on request).
pub(crate) fn wants_world(expect: &Yaml) -> Option<WorldWants> {
    WORLD_KEYS
        .iter()
        .any(|k| expect.get(k).is_some())
        .then(|| WorldWants {
            facts: expect.get("facts").is_some() || expect.get("notFacts").is_some(),
        })
}

/// What a step's world expectation needs captured.
#[derive(Clone, Copy, Debug)]
pub(crate) struct WorldWants {
    pub facts: bool,
}

pub(crate) use lute_runtime::session::{ClockView, WorldView};

/// The complete legal key set of the top-level (end-of-play) `expect:`.
pub(crate) const PLAY_EXPECT_KEYS: &[&str] = &[
    "end",
    "facts",
    "notFacts",
    "quests",
    "state",
    "transcriptContains",
    "transcriptLacks",
];

/// How a playthrough ended — what a top-level `expect.end` names (and a
/// `*.test.yaml`'s): every step played (`complete`), every step played and
/// the project's `terminal:` holds (`terminal`), an unknown halted it
/// (`incomplete`), or an error did (`error`).
pub(crate) const ENDS: &[&str] = &["complete", "terminal", "incomplete", "error"];

/// The key `expect.end` replaced; naming it is a usage error that says so.
pub(crate) const OLD_END_KEY: &str = "exit";

/// The quest lifecycle states `expect.quests` may name (dsl 0.21.0 §7a.4).
const QUEST_STATES: &[&str] = &["unset", "active", "complete", "failed"];

/// The `winner:` value that means "the occasion passed" — nothing was
/// presented (no eligible beat, or `pick: none`).
const NO_WINNER: &str = "none";

/// What one executed step did. A step with `repeat: n` yields `n` rows
/// sharing one `index`.
#[derive(Clone, Debug, Default)]
pub(crate) struct StepOutcome {
    /// 1-based script step number — the `N` of every "step N" message.
    pub index: usize,
    pub label: Option<String>,
    /// The raised occasion; for another step kind, its action (`engine`,
    /// `newRun`, `event <name>`).
    pub occasion: String,
    pub target: Option<String>,
    /// The presented beat; `None` when the occasion passed (no eligible
    /// beat, or `pick: none`).
    pub winner: Option<String>,
    /// Every eligible beat id, in presentation order.
    pub offered: Vec<String>,
    /// Every presented beat id, in presentation order (dsl 0.23.0 §3: the
    /// winner and its `also` riders, or a `select: sequence`'s beats; an
    /// `advance:`'s every raise, dsl 0.27.0 T3-8).
    pub presented: Vec<String>,
    /// dsl 0.27.0 (T3-8): on an `advance:` step, per presented beat the
    /// raise that presented it (`dayEnd at day 3 night`); empty otherwise.
    pub presented_from: Vec<String>,
    /// On an `advance:` step, per presented beat the occasion of the raise
    /// that presented it — what a keyed `presented: { <occasion>: [...] }`
    /// judges; empty otherwise.
    pub presented_occasion: Vec<String>,
    /// The world right after the step settled — captured only when the
    /// step's `expect:` judges it ([`wants_world`]).
    pub world: Option<WorldView>,
    /// dsl 0.24.0 (T3-10): per branch/hub id, every option offered at its
    /// presentations during this step (unioned) — eligible, not spent.
    pub options: BTreeMap<String, BTreeSet<String>>,
}

/// Everything a play's expectations are judged against.
#[derive(Clone, Debug, Default)]
pub(crate) struct PlayOutcome {
    /// One row per executed step, in execution order.
    pub steps: Vec<StepOutcome>,
    /// The world the play ended in: the EFFECTIVE state (every write, else
    /// the seed, else the declared `default:`), every fact after
    /// derivation, every declared quest's status.
    pub end: WorldView,
    /// The presented content lines, one `@speaker: text` per line that
    /// played (never a skipped `when=` line, a header, a candidate, staging
    /// or a note) — what `transcriptContains` / `transcriptLacks` match, the
    /// same canonical form `lute test` matches a scene walk against.
    pub said: String,
    /// The index of each step's first line in `said` — a miss's nearest
    /// line prefers the step the needle's other lines were said in (round-5
    /// T3-16).
    pub said_steps: Vec<usize>,
    /// How the play ended ([`ENDS`]): `complete`, `terminal`, `incomplete`
    /// or `error`.
    pub ended: &'static str,
    /// The last step the play ran (or halted in); `None` when it stopped
    /// before step 1.
    pub last_step: Option<usize>,
    /// dsl 0.26.0 §7 (T3-10): `<document id>.<entry id>` -> the entry id —
    /// a step's `winner` / `offered` / `notOffered` / `presented` may name
    /// an entry by that alias.
    pub entry_aliases: BTreeMap<String, String>,
}

/// One failed expectation.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ExpectMiss {
    /// The script step the expectation sits on; `None` for the top-level
    /// (end-of-play) `expect:`.
    pub step: Option<usize>,
    pub label: Option<String>,
    /// The occasion the step raised, with its target (`talk npc.meg`);
    /// `None` at the end of the play or for a step that never ran.
    pub occasion: Option<String>,
    /// The 1-based repetition of a `repeat:` step that missed; `None` when
    /// the step ran once (or never ran).
    pub repetition: Option<usize>,
    /// The expectation, e.g. `winner`, `offered`, `state run.day`.
    pub key: String,
    pub expected: String,
    pub actual: String,
}

impl ExpectMiss {
    /// Where the miss sits: `step 3 (label) at talk npc.meg, repetition 2`
    /// or `end of play`.
    pub(crate) fn place(&self) -> String {
        let Some(n) = self.step else {
            return "end of play".to_string();
        };
        let mut s = format!("step {n}");
        if let Some(label) = &self.label {
            s.push_str(&format!(" ({label})"));
        }
        if let Some(occasion) = &self.occasion {
            s.push_str(&format!(" at {occasion}"));
        }
        if let Some(r) = self.repetition {
            s.push_str(&format!(", repetition {r}"));
        }
        s
    }

    /// The stable machine shape (`lute test --json`).
    pub(crate) fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "step": self.step,
            "label": self.label,
            "occasion": self.occasion,
            "repetition": self.repetition,
            "key": self.key,
            "expected": self.expected,
            "actual": self.actual,
            "evidence": "witnessed",
        })
    }
}

impl fmt::Display for ExpectMiss {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}: expect {}: expected {}, actual {}",
            self.place(),
            self.key,
            self.expected,
            self.actual
        )
    }
}

// ===========================================================================
// Parse-time validation.
// ===========================================================================

/// One malformed `expect:` entry: the key path inside the `expect:` block it
/// is about (`["state", "run.day"]`; empty for the block itself), the list
/// item when it is about one, and why. The script locates it at that node.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ExpectError {
    pub keys: Vec<String>,
    pub item: Option<usize>,
    pub msg: String,
}

impl ExpectError {
    fn at(keys: &[&str], msg: String) -> Self {
        ExpectError {
            keys: keys.iter().map(|k| k.to_string()).collect(),
            item: None,
            msg,
        }
    }

    fn item(keys: &[&str], item: usize, msg: String) -> Self {
        ExpectError {
            item: Some(item),
            ..ExpectError::at(keys, msg)
        }
    }
}

/// Validate one `expect:` block. `top_level` selects the end-of-play key set
/// ([`PLAY_EXPECT_KEYS`]) over the step set ([`STEP_EXPECT_KEYS`]). Every
/// malformed key is reported, in the block's order — each a usage error
/// naming the key (and the legal list for an unknown one); empty when the
/// block is well-formed.
pub(crate) fn validate(expect: &Yaml, top_level: bool) -> Vec<ExpectError> {
    let (legal, where_) = if top_level {
        (PLAY_EXPECT_KEYS, "top-level `expect:`")
    } else {
        (STEP_EXPECT_KEYS, "step `expect:`")
    };
    let Yaml::Mapping(m) = expect else {
        return vec![ExpectError::at(
            &[],
            format!(
                "a {where_} must be a mapping (legal keys: {})",
                legal.join(", ")
            ),
        )];
    };
    let mut out = Vec::new();
    for (k, v) in m {
        let Some(key) = k.as_str() else {
            out.push(ExpectError::at(
                &[],
                format!("a {where_} key must be a string"),
            ));
            continue;
        };
        if key == OLD_END_KEY {
            let place = if top_level {
                ""
            } else {
                " in the top-level `expect:`"
            };
            out.push(ExpectError::at(
                &[key],
                format!(
                    "`expect.{OLD_END_KEY}` is now `expect.end`{place} — write `end: <how the \
                     play ended>`, one of: {}",
                    ENDS.join(", ")
                ),
            ));
            continue;
        }
        if !legal.contains(&key) {
            let sugg = lute_manifest::suggest::did_you_mean(key, legal.iter().copied());
            let other = if top_level {
                STEP_EXPECT_KEYS.contains(&key).then_some("a step")
            } else {
                PLAY_EXPECT_KEYS.contains(&key).then_some("the top-level")
            };
            let hint = match other {
                Some(o) => format!(" (`{key}` belongs in {o} `expect:`)"),
                None if crate::testcmd::TEST_EXPECT_KEYS.contains(&key) => {
                    format!(" (`{key}` is a `*.test.yaml` expectation, not a play's)")
                }
                None => String::new(),
            };
            out.push(ExpectError::at(
                &[key],
                format!(
                    "unknown {where_} key `{key}`{sugg}{hint} (legal: {})",
                    legal.join(", ")
                ),
            ));
            continue;
        }
        out.extend(validate_value(key, v));
    }
    out
}

/// The value shape of one known key — every malformed entry of it.
fn validate_value(key: &str, v: &Yaml) -> Vec<ExpectError> {
    let one = |msg: String| vec![ExpectError::at(&[key], msg)];
    match key {
        "winner" => match scalar_text(v) {
            Some(_) => Vec::new(),
            None => one(format!(
                "`expect.winner` must be a beat id or `{NO_WINNER}`"
            )),
        },
        // A test's menu-choice map, written in a play: here `offered` is the
        // occasion's beat candidates and `options` is the menu choices.
        "offered" if v.is_mapping() => one(
            "`expect.offered` must be a list of beat ids — in a play, menu choices are \
             `options: { <branch or hub id>: [option ids] }`; `offered` lists beat candidates"
                .into(),
        ),
        // On an `advance:` step, `presented` may be keyed by the occasion
        // of the raise: `{ dayStart: [a], slotStart: [b] }`.
        "presented" if v.is_mapping() => {
            let Yaml::Mapping(m) = v else { unreachable!() };
            let mut out = Vec::new();
            for (occasion, ids) in m {
                match occasion.as_str() {
                    Some(o) => out.extend(list_errors(&[key, o], ids)),
                    None => {
                        out.extend(one("`expect.presented` keys must be occasion names".into()))
                    }
                }
            }
            out
        }
        "offered" | "notOffered" | "presented" | "transcriptContains" | "transcriptLacks" => {
            list_errors(&[key], v)
        }
        "options" => {
            let Yaml::Mapping(m) = v else {
                return one(
                    "`expect.options` must be a mapping `{ <branch or hub id>: [option ids] }`"
                        .into(),
                );
            };
            let mut out = Vec::new();
            for (id, opts) in m {
                match id.as_str() {
                    Some(id) => out.extend(list_errors(&[key, id], opts)),
                    None => {
                        out.extend(one("`expect.options` keys must be branch or hub ids".into()))
                    }
                }
            }
            out
        }
        "facts" | "notFacts" => {
            let Yaml::Sequence(items) = v else {
                return list_errors(&[key], v);
            };
            let texts: Vec<Option<String>> = items.iter().map(scalar_text).collect();
            let mut out = Vec::new();
            for (i, atom) in texts.iter().enumerate() {
                let Some(atom) = atom else {
                    out.push(ExpectError::item(
                        &[key],
                        i,
                        format!("`expect.{key}` entries must be strings"),
                    ));
                    continue;
                };
                if parse_atom(atom).is_none() {
                    let next = texts.get(i + 1).cloned().flatten();
                    out.push(ExpectError::item(
                        &[key],
                        i,
                        format!(
                            "`expect.{key}` entry `{atom}` is not a ground atom `rel(a, b)`{}",
                            lute_runtime::session::split_atom_hint(atom, next.as_deref())
                        ),
                    ));
                }
            }
            out
        }
        "end" => match scalar_text(v) {
            Some(e) if ENDS.contains(&e.as_str()) => Vec::new(),
            Some(e) => one(format!(
                "`expect.end: {e}` names no way a play ends{} (one of: {})",
                lute_manifest::suggest::did_you_mean(&e, ENDS.iter().copied()),
                ENDS.join(", ")
            )),
            None => one(format!("`expect.end` must be one of: {}", ENDS.join(", "))),
        },
        "quests" => {
            let Yaml::Mapping(m) = v else {
                return one("`expect.quests` must be a mapping `{ <quest id>: <state> }`".into());
            };
            let mut out = Vec::new();
            for (id, st) in m {
                let Some(id) = id.as_str() else {
                    out.extend(one("`expect.quests` keys must be quest ids".into()));
                    continue;
                };
                match scalar_text(st) {
                    Some(s) if QUEST_STATES.contains(&s.as_str()) => {}
                    got => out.push(ExpectError::at(
                        &[key, id],
                        format!(
                            "`expect.quests.{id}` must be one of: {}{}",
                            QUEST_STATES.join(", "),
                            got.map(|s| {
                                lute_manifest::suggest::did_you_mean(
                                    &s,
                                    QUEST_STATES.iter().copied(),
                                )
                            })
                            .unwrap_or_default()
                        ),
                    )),
                }
            }
            out
        }
        "state" => {
            let Yaml::Mapping(m) = v else {
                return one("`expect.state` must be a mapping `{ <state path>: <value> }`".into());
            };
            let mut out = Vec::new();
            for (path, want) in m {
                let Some(path) = path.as_str() else {
                    out.extend(one("`expect.state` keys must be state paths".into()));
                    continue;
                };
                if scalar_text(want).is_none() {
                    out.push(ExpectError::at(
                        &[key, path],
                        format!("`expect.state.{path}` must be a bool, number or string"),
                    ));
                }
            }
            out
        }
        "clock" => {
            let shape = "`expect.clock` must be a mapping `{ weekday: <label or number>, slot: \
                         <slot>, day: <whole number>, ended: <bool> }` (any of them)";
            let Yaml::Mapping(m) = v else {
                return one(shape.into());
            };
            if m.is_empty() {
                return one(shape.into());
            }
            let mut out = Vec::new();
            for (k, want) in m {
                let Some(k) = k.as_str() else {
                    out.extend(one(shape.into()));
                    continue;
                };
                let ok = match k {
                    "day" => want.as_i64().is_some_and(|d| d >= 1),
                    "ended" => want.as_bool().is_some(),
                    "slot" => want.as_str().is_some_and(|s| !s.trim().is_empty()),
                    "weekday" => {
                        want.as_i64().is_some_and(|d| d >= 0)
                            || want.as_str().is_some_and(|s| !s.trim().is_empty())
                    }
                    _ => {
                        out.push(ExpectError::at(
                            &[key, k],
                            format!(
                                "unknown `expect.clock` key `{k}`{} (legal: {})",
                                lute_manifest::suggest::did_you_mean(k, CLOCK_KEYS.iter().copied()),
                                CLOCK_KEYS.join(", ")
                            ),
                        ));
                        continue;
                    }
                };
                if !ok {
                    out.push(ExpectError::at(&[key, k], shape.into()));
                }
            }
            out
        }
        _ => unreachable!("validate() filtered to the legal key sets"),
    }
}

/// A list of strings: an error for a non-list, else one per entry that is
/// not a string.
fn list_errors(keys: &[&str], v: &Yaml) -> Vec<ExpectError> {
    let name = keys.join(".");
    let Yaml::Sequence(items) = v else {
        return vec![ExpectError::at(
            keys,
            format!("`expect.{name}` must be a list"),
        )];
    };
    items
        .iter()
        .enumerate()
        .filter(|(_, i)| scalar_text(i).is_none())
        .map(|(i, _)| {
            ExpectError::item(keys, i, format!("`expect.{name}` entries must be strings"))
        })
        .collect()
}

/// A list of strings (scalars rendered to text).
fn string_list(key: &str, v: &Yaml) -> Result<Vec<String>, String> {
    let Yaml::Sequence(items) = v else {
        return Err(format!("`expect.{key}` must be a list"));
    };
    items
        .iter()
        .map(|i| scalar_text(i).ok_or_else(|| format!("`expect.{key}` entries must be strings")))
        .collect()
}

/// A YAML scalar's literal text; `None` for null/sequence/mapping.
fn scalar_text(v: &Yaml) -> Option<String> {
    match v {
        Yaml::Bool(b) => Some(b.to_string()),
        Yaml::Number(n) => Some(n.to_string()),
        Yaml::String(s) => Some(s.clone()),
        _ => None,
    }
}

/// `rel(a, b)` / `rel` → `(rel, [a, b])`, whitespace-trimmed and quote-
/// stripped, so `knows(player,"oskar")` and `knows(player, oskar)` agree.
pub(crate) fn parse_atom(text: &str) -> Option<(String, Vec<String>)> {
    let text = text.trim();
    let (rel, args) = match text.split_once('(') {
        None => (text, Vec::new()),
        Some((rel, rest)) => {
            let inner = rest.strip_suffix(')')?;
            let args: Vec<String> = if inner.trim().is_empty() {
                Vec::new()
            } else {
                inner
                    .split(',')
                    .map(|a| a.trim().trim_matches(|c| c == '"' || c == '\'').to_string())
                    .collect()
            };
            (rel.trim(), args)
        }
    };
    let ident = |s: &str| {
        !s.is_empty() && !s.contains(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | ','))
    };
    (ident(rel) && args.iter().all(|a| ident(a))).then(|| (rel.to_string(), args))
}

/// The one spelling both sides of a fact comparison agree on.
pub(crate) fn canonical_atom(text: &str) -> String {
    match parse_atom(text) {
        Some((rel, args)) => format!("{rel}({})", args.join(", ")),
        None => text.trim().to_string(),
    }
}

// ===========================================================================
// Judging.
// ===========================================================================

/// Judge every step `expect:` (`(script step index, label, expect)`) and the
/// top-level one against what the play did. Empty = every expectation held.
/// Expectations are assumed [`validate`]d; a malformed value that slipped
/// through is skipped, never a panic.
pub(crate) fn check(
    outcome: &PlayOutcome,
    steps: &[(usize, Option<String>, Yaml)],
    top: Option<&Yaml>,
) -> Vec<ExpectMiss> {
    let mut misses = Vec::new();
    // OT-F-15: the steps whose `expect:` the play never reached — one
    // summary line, not one miss per step.
    let mut unreached: Vec<(usize, Option<&String>)> = Vec::new();
    for (index, label, expect) in steps {
        let rows: Vec<&StepOutcome> = outcome.steps.iter().filter(|s| s.index == *index).collect();
        if rows.is_empty() {
            unreached.push((*index, label.as_ref()));
            continue;
        }
        let repeated = rows.len() > 1;
        for (r, row) in rows.iter().enumerate() {
            check_step(
                row,
                label.as_ref(),
                repeated.then_some(r + 1),
                expect,
                &outcome.entry_aliases,
                &mut misses,
            );
        }
    }
    if let Some(&(first, label)) = unreached.first() {
        misses.push(unreached_miss(outcome, first, label, &unreached));
    }
    if let Some(top) = top {
        check_end(outcome, top, &mut misses);
    }
    misses
}

/// OT-F-15: the one miss for every step `expect:` the play never reached,
/// saying where and how it stopped.
fn unreached_miss(
    outcome: &PlayOutcome,
    first: usize,
    label: Option<&String>,
    unreached: &[(usize, Option<&String>)],
) -> ExpectMiss {
    const SHOWN: usize = 8;
    let expected = match unreached.len() {
        1 => "the step to run".to_string(),
        k => {
            let mut ns: Vec<String> = unreached
                .iter()
                .take(SHOWN)
                .map(|(n, _)| n.to_string())
                .collect();
            if k > SHOWN {
                ns.push("…".to_string());
            }
            format!("steps {} to run ({k} expectations)", ns.join(", "))
        }
    };
    let how = match outcome.ended {
        "complete" | "terminal" => "ended (`end: true`)",
        "incomplete" => "stopped incomplete",
        _ => "halted with an error",
    };
    let at = match outcome.last_step {
        Some(k) => format!(" at step {k}"),
        None => " before step 1".to_string(),
    };
    ExpectMiss {
        step: Some(first),
        label: label.cloned(),
        occasion: None,
        repetition: None,
        key: "(step reached)".to_string(),
        expected,
        actual: format!("not reached — the play {how}{at}"),
    }
}

/// Render a list for a miss line: `[a, b]`.
fn list(items: &[String]) -> String {
    format!("[{}]", items.join(", "))
}

fn check_step(
    row: &StepOutcome,
    label: Option<&String>,
    repetition: Option<usize>,
    expect: &Yaml,
    entry_aliases: &BTreeMap<String, String>,
    misses: &mut Vec<ExpectMiss>,
) {
    // dsl 0.26.0 §7 (T3-10): an expected id may be an entry's
    // `<doc>.<entry>` alias; it is judged as the entry id.
    let resolve = |id: String| entry_aliases.get(&id).cloned().unwrap_or(id);
    let ids = |key: &str| {
        let list = string_list(key, expect.get(key)?).ok()?;
        Some(list.into_iter().map(resolve).collect::<Vec<String>>())
    };
    let Yaml::Mapping(m) = expect else { return };
    let occasion = match &row.target {
        Some(t) => format!("{} {t}", row.occasion),
        None => row.occasion.clone(),
    };
    let label = label.or(row.label.as_ref());
    let mut miss = |key: String, expected: String, actual: String| {
        misses.push(ExpectMiss {
            step: Some(row.index),
            label: label.cloned(),
            occasion: Some(occasion.clone()),
            repetition,
            key,
            expected,
            actual,
        })
    };
    let actual_winner = row.winner.clone().unwrap_or_else(|| NO_WINNER.to_string());
    if let Some(want) = m.get("winner").and_then(scalar_text).map(resolve) {
        let holds = match (&row.winner, want.as_str()) {
            (None, NO_WINNER) => true,
            (Some(w), want) => names(want, w),
            (None, _) => false,
        };
        if !holds {
            miss("winner".into(), want, actual_winner.clone());
        }
    }
    let offered = |w: &str| row.offered.iter().any(|o| names(w, o));
    if let Some(want) = ids("offered") {
        let missing: Vec<String> = want.iter().filter(|w| !offered(w)).cloned().collect();
        if !missing.is_empty() {
            miss(
                "offered".into(),
                format!(
                    "{} among the eligible beats (missing {})",
                    list(&want),
                    list(&missing)
                ),
                list(&row.offered),
            );
        }
    }
    if let Some(want) = ids("notOffered") {
        let present: Vec<String> = want.iter().filter(|w| offered(w)).cloned().collect();
        if !present.is_empty() {
            miss(
                "notOffered".into(),
                format!("none of {} eligible", list(&want)),
                format!("{} (offending {})", list(&row.offered), list(&present)),
            );
        }
    }
    // dsl 0.23.0 §3: the exact presentation order. On an `advance:` step
    // every raise's presentations, each named by its raise (T3-8).
    if let Some(want) = ids("presented") {
        let holds = want.len() == row.presented.len()
            && want.iter().zip(&row.presented).all(|(w, a)| names(w, a));
        if !holds {
            let actual = if row.presented_from.len() == row.presented.len() {
                let tagged: Vec<String> = row
                    .presented
                    .iter()
                    .zip(&row.presented_from)
                    .map(|(id, from)| format!("{id} ({from})"))
                    .collect();
                list(&tagged)
            } else {
                list(&row.presented)
            };
            miss("presented".into(), list(&want), actual);
        }
    }
    // An `advance:` step's `presented: { <occasion>: [...] }`: per named
    // occasion, the exact order its raises in this step presented.
    if let Some(Yaml::Mapping(keyed)) = m.get("presented") {
        for (occasion, ids) in keyed {
            let (Some(occasion), Ok(want)) = (occasion.as_str(), string_list("presented", ids))
            else {
                continue;
            };
            let want: Vec<String> = want.into_iter().map(resolve).collect();
            let (got, tagged): (Vec<&String>, Vec<String>) = row
                .presented
                .iter()
                .zip(&row.presented_from)
                .zip(&row.presented_occasion)
                .filter(|(_, o)| o.as_str() == occasion)
                .map(|((id, from), _)| (id, format!("{id} ({from})")))
                .unzip();
            let holds = want.len() == got.len() && want.iter().zip(&got).all(|(w, a)| names(w, a));
            if !holds {
                miss(format!("presented {occasion}"), list(&want), list(&tagged));
            }
        }
    }
    // dsl 0.24.0 (T3-10): the options a branch/hub offered in this step, as
    // a set — `lute test`'s `offered:` for a play step.
    if let Some(Yaml::Mapping(want)) = m.get("options") {
        for (id, opts) in want {
            let (Some(id), Ok(opts)) = (id.as_str(), string_list("options", opts)) else {
                continue;
            };
            let want: BTreeSet<String> = opts.into_iter().collect();
            let shown = |s: &BTreeSet<String>| list(&s.iter().cloned().collect::<Vec<_>>());
            match row.options.get(id) {
                Some(got) if *got == want => {}
                Some(got) => miss(format!("options {id}"), shown(&want), shown(got)),
                None => miss(
                    format!("options {id}"),
                    shown(&want),
                    format!("no branch or hub `{id}` was presented in this step"),
                ),
            }
        }
    }
    // 0.23.1: the world right after this step settled.
    if wants_world(expect).is_some() {
        match &row.world {
            Some(world) => check_world(world, m, &mut miss),
            None => miss(
                "(world)".into(),
                "the world after this step".into(),
                "not captured".into(),
            ),
        }
    }
}

/// G-9: whether the expected beat `want` names `actual` — the same id, or
/// (dsl 0.27.0 §3) a bare `for` beat id naming its presentation for any
/// member (`actual` spelled `<id> for <member>`, as the transcript prints).
fn names(want: &str, actual: &str) -> bool {
    actual == want
        || actual
            .strip_prefix(want)
            .is_some_and(|rest| rest.starts_with(" for "))
}

/// Judge the world keys (`quests`, `state`, `facts`, `notFacts`, `clock`)
/// of one `expect:` against `world`.
fn check_world(
    world: &WorldView,
    m: &serde_yaml::Mapping,
    miss: &mut impl FnMut(String, String, String),
) {
    if let Some(Yaml::Mapping(want)) = m.get("clock") {
        check_clock(world.clock.as_ref(), want, miss);
    }
    if let Some(Yaml::Mapping(quests)) = m.get("quests") {
        for (id, want) in quests {
            let (Some(id), Some(want)) = (id.as_str(), scalar_text(want)) else {
                continue;
            };
            match world.quests.get(id) {
                Some(actual) if *actual == want => {}
                Some(actual) => miss(format!("quests {id}"), want, actual.clone()),
                None => miss(
                    format!("quests {id}"),
                    want,
                    "no such quest in the project".to_string(),
                ),
            }
        }
    }
    if let Some(Yaml::Mapping(state)) = m.get("state") {
        for (path, want) in state {
            let Some(path) = path.as_str() else { continue };
            let actual = world.state.get(&lute_trace::state_key(path));
            if !state_matches(want, actual) {
                miss(
                    format!("state {path}"),
                    yaml_value_text(want),
                    match actual {
                        Some(v) => value_text(v),
                        None => "no value (never written, not seeded, no default)".to_string(),
                    },
                );
            }
        }
    }
    let facts: BTreeSet<String> = world.facts.iter().map(|f| canonical_atom(f)).collect();
    for (key, want_held) in [("facts", true), ("notFacts", false)] {
        let Some(want) = m.get(key).and_then(|v| string_list(key, v).ok()) else {
            continue;
        };
        for atom in want {
            let held = facts.contains(&canonical_atom(&atom));
            if held != want_held {
                miss(
                    key.to_string(),
                    format!(
                        "{atom} {}",
                        if want_held { "holds" } else { "does not hold" }
                    ),
                    format!("{atom} {}", if held { "holds" } else { "does not hold" }),
                );
            }
        }
    }
}

/// dsl 0.26.0 §7 (T2-5): judge a step `expect.clock` — the `day`, `slot`
/// name and `weekday` (a `week.labels` label or a `clock.weekday` number)
/// where the clock stands after the step, and whether a clock that ends has
/// ended (`ended`, the `clock.ended` content reads). An `include:`d steps file states
/// the time it assumes, so a clock another area's steps pushed on fails at
/// the boundary instead of silently skipping time-gated beats.
fn check_clock(
    clock: Option<&ClockView>,
    want: &serde_yaml::Mapping,
    miss: &mut impl FnMut(String, String, String),
) {
    for (key, want) in want {
        let Some(key) = key.as_str() else { continue };
        let expected = scalar_text(want).unwrap_or_default();
        let Some(c) = clock else {
            miss(
                format!("clock {key}"),
                expected,
                "no clock position (the project declares no clock, or its day/slot name no \
                 position)"
                    .to_string(),
            );
            continue;
        };
        let (held, actual) = match key {
            "day" => (want.as_i64() == Some(c.day), c.day.to_string()),
            "slot" => (
                want.as_str() == c.slot.as_deref(),
                c.slot
                    .clone()
                    .unwrap_or_else(|| "none (a day-granular clock)".to_string()),
            ),
            "weekday" => {
                let held = match want {
                    Yaml::Number(n) => n.as_i64().is_some_and(|n| Some(n) == c.weekday),
                    Yaml::String(s) => {
                        c.weekday_label.as_deref() == Some(s.as_str())
                            || s.parse::<i64>().ok().is_some_and(|n| Some(n) == c.weekday)
                    }
                    _ => false,
                };
                let actual = match (&c.weekday_label, c.weekday) {
                    (Some(label), Some(n)) => format!("{label} ({n})"),
                    (None, Some(n)) => n.to_string(),
                    _ => "none (the clock declares no `week:`)".to_string(),
                };
                (held, actual)
            }
            "ended" => match c.ended {
                Some(ended) => (
                    want.as_bool() == Some(ended),
                    match &c.last {
                        // The last position is not the end: the clock ends
                        // on the advance that would go past it.
                        Some(at) => format!(
                            "{ended} — the clock stands at its last position ({at}) and ends on \
                             the next `advance:`"
                        ),
                        None => ended.to_string(),
                    },
                ),
                None => (
                    false,
                    "none (the clock never ends: it declares no `last:` or `days:`)".to_string(),
                ),
            },
            _ => continue,
        };
        if !held {
            miss(format!("clock {key}"), expected, actual);
        }
    }
}

fn check_end(outcome: &PlayOutcome, top: &Yaml, misses: &mut Vec<ExpectMiss>) {
    let Yaml::Mapping(m) = top else { return };
    let mut miss = |key: String, expected: String, actual: String| {
        misses.push(ExpectMiss {
            step: None,
            label: None,
            occasion: None,
            repetition: None,
            key,
            expected,
            actual,
        })
    };
    if let Some(want) = m.get("end").and_then(scalar_text) {
        if want != outcome.ended {
            miss("end".into(), want, outcome.ended.to_string());
        }
    }
    check_world(&outcome.end, m, &mut miss);
    for (key, want_present) in [("transcriptContains", true), ("transcriptLacks", false)] {
        let Some(want) = m.get(key).and_then(|v| string_list(key, v).ok()) else {
            continue;
        };
        for sub in want {
            if let Some(actual) = lute_trace::exec::record::judge(
                &outcome.said,
                &outcome.said_steps,
                &sub,
                want_present,
            ) {
                miss(
                    key.to_string(),
                    format!(
                        "{sub:?} {}",
                        if want_present { "present" } else { "absent" }
                    ),
                    actual,
                );
            }
        }
    }
}

/// Typed comparison of an expected YAML scalar against an effective value:
/// a bool matches a bool, a number a number, a string a string. `Unknown`
/// and an absent value match nothing.
fn state_matches(want: &Yaml, actual: Option<&Value>) -> bool {
    match (want, actual) {
        (Yaml::Bool(w), Some(Value::Bool(a))) => w == a,
        (Yaml::Number(w), Some(Value::Int(a))) => w.as_i64() == Some(*a),
        (Yaml::Number(w), Some(Value::Double(a))) => w.as_f64() == Some(*a),
        (Yaml::String(w), Some(Value::Str(a))) => w == a,
        _ => false,
    }
}

/// A value as a miss line prints it — strings quoted so `"3"` and `3` differ.
fn value_text(v: &Value) -> String {
    match v {
        Value::Bool(b) => b.to_string(),
        Value::Int(n) => n.to_string(),
        Value::Double(n) => n.to_string(),
        Value::Str(s) => format!("{s:?}"),
        Value::Unknown => "unknown".to_string(),
        Value::Error(e) => format!("error: {e}"),
    }
}

/// An expected YAML scalar as a miss line prints it — quoted like
/// [`value_text`], so both sides of a miss read alike.
fn yaml_value_text(v: &Yaml) -> String {
    match v {
        Yaml::String(s) => format!("{s:?}"),
        other => scalar_text(other).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn y(s: &str) -> Yaml {
        serde_yaml::from_str(s).unwrap()
    }

    fn step(index: usize, winner: Option<&str>, offered: &[&str]) -> StepOutcome {
        StepOutcome {
            index,
            label: None,
            occasion: "hubVisit".into(),
            target: None,
            winner: winner.map(str::to_string),
            offered: offered.iter().map(|s| s.to_string()).collect(),
            presented: winner.into_iter().map(str::to_string).collect(),
            presented_from: Vec::new(),
            presented_occasion: Vec::new(),
            world: None,
            options: BTreeMap::new(),
        }
    }

    fn outcome() -> PlayOutcome {
        PlayOutcome {
            steps: vec![
                step(1, Some("hub.welcome"), &["hub.welcome", "hub.idle"]),
                step(3, None, &[]),
            ],
            end: WorldView {
                state: BTreeMap::from([
                    ("run.day".to_string(), Value::Int(3)),
                    ("run.outcome".to_string(), Value::Str("fell".into())),
                    ("user.met".to_string(), Value::Bool(true)),
                    ("run.fog".to_string(), Value::Unknown),
                ]),
                facts: BTreeSet::from(["knows(player, oskar)".to_string(), "slew(warden)".into()]),
                quests: BTreeMap::from([
                    ("caseClosed".to_string(), "complete".to_string()),
                    ("side".to_string(), "unset".to_string()),
                ]),
                clock: None,
            },
            said: "@oskar: Welcome back.\n".into(),
            said_steps: vec![0, 1],
            ended: "complete",
            last_step: Some(3),
            entry_aliases: BTreeMap::new(),
        }
    }

    #[test]
    fn holding_step_and_end_expectations_report_nothing() {
        let steps = vec![
            (
                1,
                None,
                y("{winner: hub.welcome, offered: [hub.idle, hub.welcome], notOffered: [hub.trophy]}"),
            ),
            (3, None, y("{winner: none, notOffered: [hub.welcome]}")),
        ];
        let top = y(r#"
end: complete
quests: { caseClosed: complete, side: unset }
state: { run.day: 3, run.outcome: fell, user.met: true }
facts: ['knows(player,"oskar")', slew(warden)]
notFacts: [slew(oskar)]
transcriptContains: ["Welcome back."]
transcriptLacks: ["Goodbye"]
"#);
        assert_eq!(check(&outcome(), &steps, Some(&top)), Vec::new());
    }

    #[test]
    fn a_wrong_winner_names_the_step_label_and_the_actual_beat() {
        let steps = vec![(1, Some("first visit".to_string()), y("{winner: hub.idle}"))];
        let misses = check(&outcome(), &steps, None);
        assert_eq!(misses.len(), 1);
        let line = misses[0].to_string();
        assert_eq!(
            line,
            "step 1 (first visit) at hubVisit: expect winner: expected hub.idle, actual hub.welcome"
        );
    }

    #[test]
    fn winner_none_distinguishes_a_passed_occasion() {
        let misses = check(&outcome(), &[(1, None, y("{winner: none}"))], None);
        assert_eq!(misses[0].actual, "hub.welcome");
        let misses = check(&outcome(), &[(3, None, y("{winner: hub.welcome}"))], None);
        assert_eq!(misses[0].actual, "none");
    }

    #[test]
    fn offered_is_a_subset_check_and_not_offered_an_exclusion() {
        let misses = check(
            &outcome(),
            &[(
                1,
                None,
                y("{offered: [hub.welcome, hub.trophy], notOffered: [hub.idle]}"),
            )],
            None,
        );
        let keys: Vec<&str> = misses.iter().map(|m| m.key.as_str()).collect();
        assert_eq!(keys, ["offered", "notOffered"]);
        assert!(misses[0].expected.contains("missing [hub.trophy]"));
        assert_eq!(misses[0].actual, "[hub.welcome, hub.idle]");
        assert_eq!(
            misses[1].actual,
            "[hub.welcome, hub.idle] (offending [hub.idle])"
        );
    }

    #[test]
    fn steps_the_play_never_reached_are_one_summary_miss() {
        let o = PlayOutcome {
            ended: "error",
            ..outcome()
        };
        let misses = check(&o, &[(2, None, y("{winner: none}"))], None);
        assert_eq!(misses.len(), 1);
        assert_eq!(
            misses[0].to_string(),
            "step 2: expect (step reached): expected the step to run, actual not reached — \
             the play halted with an error at step 3"
        );

        let later: Vec<(usize, Option<String>, Yaml)> =
            (4..=14).map(|n| (n, None, y("{winner: none}"))).collect();
        let misses = check(&o, &later, None);
        assert_eq!(misses.len(), 1, "{misses:?}");
        assert_eq!(
            misses[0].expected,
            "steps 4, 5, 6, 7, 8, 9, 10, 11, … to run (11 expectations)"
        );
    }

    #[test]
    fn every_repetition_of_a_repeated_step_is_judged() {
        let mut o = outcome();
        o.steps = vec![
            step(1, Some("hub.welcome"), &["hub.welcome"]),
            step(1, Some("hub.idle"), &["hub.idle"]),
        ];
        let misses = check(&o, &[(1, None, y("{winner: hub.welcome}"))], None);
        assert_eq!(misses.len(), 1);
        assert_eq!(misses[0].repetition, Some(2));
        assert_eq!(misses[0].place(), "step 1 at hubVisit, repetition 2");
    }

    #[test]
    fn state_compares_typed_effective_values() {
        let top = y(
            r#"state: { run.day: "3", user.met: false, run.fog: x, run.none: 1, run.outcome: fell }"#,
        );
        let misses = check(&outcome(), &[], Some(&top));
        let got: Vec<(&str, &str)> = misses
            .iter()
            .map(|m| (m.key.as_str(), m.actual.as_str()))
            .collect();
        assert_eq!(
            got,
            [
                ("state run.day", "3"),
                ("state user.met", "true"),
                ("state run.fog", "unknown"),
                (
                    "state run.none",
                    "no value (never written, not seeded, no default)"
                ),
            ]
        );
    }

    #[test]
    fn end_misses_cover_end_quests_facts_and_transcript() {
        let top = y(r#"
end: incomplete
quests: { caseClosed: failed, ghost: active }
facts: [slew(oskar)]
notFacts: [slew(warden)]
transcriptContains: ["Goodbye"]
transcriptLacks: ["Welcome"]
"#);
        let misses = check(&outcome(), &[], Some(&top));
        let keys: Vec<&str> = misses.iter().map(|m| m.key.as_str()).collect();
        assert_eq!(
            keys,
            [
                "end",
                "quests caseClosed",
                "quests ghost",
                "facts",
                "notFacts",
                "transcriptContains",
                "transcriptLacks"
            ]
        );
        assert!(misses.iter().all(|m| m.step.is_none()));
        assert_eq!(misses[0].actual, "complete");
        assert_eq!(misses[1].actual, "complete");
        assert_eq!(
            misses[0].to_string(),
            "end of play: expect end: expected incomplete, actual complete"
        );
    }

    /// A play that ran every step without reaching the project's
    /// `terminal:` is `complete`, never `terminal` — and the reverse.
    #[test]
    fn end_tells_a_terminal_ending_from_a_complete_one() {
        let terminal = PlayOutcome {
            ended: "terminal",
            ..outcome()
        };
        let misses = check(&outcome(), &[], Some(&y("{end: terminal}")));
        assert_eq!(misses.len(), 1, "{misses:?}");
        assert_eq!(misses[0].actual, "complete");
        let misses = check(&terminal, &[], Some(&y("{end: complete}")));
        assert_eq!(misses[0].actual, "terminal");
        assert!(check(&terminal, &[], Some(&y("{end: terminal}"))).is_empty());
    }

    fn messages(errs: Vec<ExpectError>) -> Vec<String> {
        errs.into_iter().map(|e| e.msg).collect()
    }

    #[test]
    fn validate_rejects_unknown_keys_with_the_legal_list() {
        let e = &messages(validate(&y("{winer: hub.a}"), false))[0];
        assert!(e.contains("`winer`"), "{e}");
        assert!(e.contains("did you mean `winner`"), "{e}");
        assert!(
            e.contains("legal: clock, facts, notFacts, notOffered, offered, options, presented, quests, state, winner"),
            "{e}"
        );
        let e = &messages(validate(&y("{transcriptContains: [x]}"), false))[0];
        assert!(e.contains("belongs in the top-level"), "{e}");
        let e = &messages(validate(&y("{winner: a}"), true))[0];
        assert!(e.contains("belongs in a step"), "{e}");
        assert!(validate(&y("{winner: a, offered: [a], notOffered: [b]}"), false).is_empty());
    }

    /// `expect.exit` became `expect.end`: the old key is refused, naming the
    /// new one and its values; a mistyped value gets a did-you-mean.
    #[test]
    fn exit_is_refused_naming_end() {
        let e = messages(validate(&y("{exit: terminal}"), true));
        assert_eq!(e.len(), 1, "{e:?}");
        assert!(e[0].contains("`expect.exit` is now `expect.end`"), "{e:?}");
        assert!(
            e[0].contains("complete, terminal, incomplete, error"),
            "{e:?}"
        );
        let e = messages(validate(&y("{end: completed}"), true));
        assert!(e[0].contains("did you mean `complete`?"), "{e:?}");
        assert!(validate(&y("{end: terminal}"), true).is_empty());
    }

    /// Every malformed entry is reported, each at its key (and list item).
    #[test]
    fn validate_reports_every_malformed_entry_where_it_is() {
        let errs = validate(
            &y("{facts: ['knows(a', ok(b)], state: {run.day: [1], run.ok: 2}, bogus: 1}"),
            true,
        );
        let at: Vec<(Vec<String>, Option<usize>)> =
            errs.iter().map(|e| (e.keys.clone(), e.item)).collect();
        assert_eq!(
            at,
            [
                (vec!["facts".to_string()], Some(0)),
                (vec!["state".to_string(), "run.day".to_string()], None),
                (vec!["bogus".to_string()], None),
            ],
            "{errs:?}"
        );
        // A lone unbalanced atom is not blamed on YAML's comma split.
        assert!(
            errs[0].msg.contains("parentheses do not balance"),
            "{errs:?}"
        );
        let split = validate(&y("{facts: [knows(a, b)]}"), true);
        assert!(split[0].msg.contains("quote the atom"), "{split:?}");
    }

    #[test]
    fn validate_rejects_malformed_values() {
        for (text, top) in [
            ("{offered: a}", false),
            ("{end: done}", true),
            ("{quests: {q: started}}", true),
            ("{state: {run.day: [1]}}", true),
            ("{facts: ['knows(a']}", true),
            ("[winner]", false),
            ("{clock: {}}", false),
            ("{clock: {hour: 3}}", false),
            ("{clock: {day: 0}}", false),
            ("{clock: {slot: 3}}", false),
            ("{clock: {day: 2}}", true),
        ] {
            assert!(
                !validate(&y(text), top).is_empty(),
                "{text} should be rejected"
            );
        }
    }

    #[test]
    fn a_step_judges_the_world_right_after_it_settled() {
        let mut o = outcome();
        o.steps[0].world = Some(WorldView {
            state: BTreeMap::from([("run.accused".to_string(), Value::Str("b".into()))]),
            facts: BTreeSet::from(["slew(warden)".to_string()]),
            quests: BTreeMap::from([("caseClosed".to_string(), "failed".to_string())]),
            clock: None,
        });
        let holds =
            y("{quests: {caseClosed: failed}, state: {run.accused: b}, facts: [slew(warden)]}");
        assert_eq!(check(&o, &[(1, None, holds)], None), Vec::new());
        // The end world (`complete`) is not what a step judges.
        let misses = check(
            &o,
            &[(1, Some("night one".into()), y("{quests: {caseClosed: complete}, state: {run.accused: a}, notFacts: [slew(warden)]}"))],
            None,
        );
        let lines: Vec<String> = misses.iter().map(ToString::to_string).collect();
        assert_eq!(
            lines,
            [
                "step 1 (night one) at hubVisit: expect quests caseClosed: expected complete, actual failed",
                "step 1 (night one) at hubVisit: expect state run.accused: expected \"a\", actual \"b\"",
                "step 1 (night one) at hubVisit: expect notFacts: expected slew(warden) does not hold, actual slew(warden) holds",
            ]
        );
    }

    /// dsl 0.26.0 §7 (T2-5): a step `expect.clock` judges where the clock
    /// stands — a weekday by label or number, the slot name, the day.
    #[test]
    fn a_step_judges_the_clock() {
        let mut o = outcome();
        o.steps[0].world = Some(WorldView {
            clock: Some(ClockView {
                day: 5,
                slot: Some("morning".into()),
                weekday: Some(5),
                weekday_label: Some("Fri".into()),
                ended: None,
                last: None,
            }),
            ..WorldView::default()
        });
        for holds in [
            "{clock: {weekday: Fri, slot: morning, day: 5}}",
            "{clock: {weekday: 5}}",
        ] {
            assert_eq!(
                check(&o, &[(1, None, y(holds))], None),
                Vec::new(),
                "{holds}"
            );
        }
        let misses = check(
            &o,
            &[(1, None, y("{clock: {weekday: Mon, slot: night, day: 1}}"))],
            None,
        );
        let lines: Vec<String> = misses.iter().map(ToString::to_string).collect();
        assert_eq!(
            lines,
            [
                "step 1 at hubVisit: expect clock weekday: expected Mon, actual Fri (5)",
                "step 1 at hubVisit: expect clock slot: expected night, actual morning",
                "step 1 at hubVisit: expect clock day: expected 1, actual 5",
            ]
        );
        // No clock: every key misses, saying why.
        o.steps[0].world = Some(WorldView::default());
        let misses = check(&o, &[(1, None, y("{clock: {day: 1}}"))], None);
        assert_eq!(misses.len(), 1);
        assert!(
            misses[0].to_string().contains("no clock position"),
            "{}",
            misses[0]
        );
    }

    #[test]
    fn occasion_keys_are_named_for_a_non_occasion_step() {
        assert_eq!(
            occasion_key(&y("{quests: {q: active}, winner: a}")),
            Some("winner")
        );
        assert_eq!(occasion_key(&y("{quests: {q: active}}")), None);
        assert!(wants_world(&y("{winner: a}")).is_none());
        assert!(!wants_world(&y("{state: {run.x: 1}}")).unwrap().facts);
        assert!(wants_world(&y("{notFacts: [a]}")).unwrap().facts);
    }
}
