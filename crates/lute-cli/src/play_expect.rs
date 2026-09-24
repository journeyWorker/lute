//! Play-script assertions (dsl 0.22.0 §4).
//!
//! A play step MAY carry `expect: { winner, offered, notOffered }` and a
//! script MAY carry a top-level `expect: { exit, quests, state, facts,
//! notFacts, transcriptContains, transcriptLacks }`. [`crate::play`] parses
//! the script, calls [`validate`] on every `expect:` at parse time (an
//! unknown key or a malformed value is a usage error, exit 2), walks the
//! play, fills a [`PlayOutcome`] and hands both to [`check`]. Every miss
//! names its step (and `label:`) and the actual value; `lute play` exits 1
//! on any miss, and `lute test` reports a play with a miss as FAIL.
//!
//! This module judges; it never walks. What a step presented, what was
//! eligible, and the final state/facts/quests are exactly what the play
//! walk recorded — there is no second model of the playthrough here.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;

use lute_trace::Value;
use serde_yaml::Value as Yaml;

/// The complete legal key set of a STEP `expect:`.
pub(crate) const STEP_EXPECT_KEYS: &[&str] = &["notOffered", "offered", "winner"];

/// The complete legal key set of the top-level (end-of-play) `expect:`.
pub(crate) const PLAY_EXPECT_KEYS: &[&str] = &[
    "exit",
    "facts",
    "notFacts",
    "quests",
    "state",
    "transcriptContains",
    "transcriptLacks",
];

/// The exits a top-level `expect.exit` may name.
const EXITS: &[&str] = &["complete", "incomplete", "error"];

/// The quest lifecycle states `expect.quests` may name (dsl 0.21.0 §7a.4).
const QUEST_STATES: &[&str] = &["unset", "active", "complete", "failed"];

/// The `winner:` value that means "the occasion passed" — nothing was
/// presented (no eligible beat, or `pick: none`).
const NO_WINNER: &str = "none";

/// What one executed occasion step did. A step with `repeat: n` yields `n`
/// rows sharing one `index`.
#[derive(Clone, Debug, Default)]
pub(crate) struct StepOutcome {
    /// 1-based script step number — the `N` of every "step N" message.
    pub index: usize,
    pub label: Option<String>,
    pub occasion: String,
    pub target: Option<String>,
    /// The presented beat; `None` when the occasion passed (no eligible
    /// beat, or `pick: none`).
    pub winner: Option<String>,
    /// Every eligible beat id, in presentation order.
    pub offered: Vec<String>,
}

/// Everything a play's expectations are judged against.
#[derive(Clone, Debug, Default)]
pub(crate) struct PlayOutcome {
    /// One row per executed occasion step, in execution order.
    pub steps: Vec<StepOutcome>,
    /// The final EFFECTIVE state: every write, else the seed, else the
    /// declared `default:`.
    pub state: BTreeMap<String, Value>,
    /// Every ground atom that holds at the end, after derivation, rendered
    /// `rel(a, b)`.
    pub facts: BTreeSet<String>,
    /// Every declared quest → `unset | active | complete | failed`.
    pub quests: BTreeMap<String, String>,
    pub transcript: String,
    /// `complete | incomplete | error`.
    pub exit: &'static str,
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

/// Validate one `expect:` block. `top_level` selects the end-of-play key set
/// ([`PLAY_EXPECT_KEYS`]) over the step set ([`STEP_EXPECT_KEYS`]). `Err`
/// is a usage error naming the key and the legal list.
pub(crate) fn validate(expect: &Yaml, top_level: bool) -> Result<(), String> {
    let (legal, where_) = if top_level {
        (PLAY_EXPECT_KEYS, "top-level `expect:`")
    } else {
        (STEP_EXPECT_KEYS, "step `expect:`")
    };
    let Yaml::Mapping(m) = expect else {
        return Err(format!(
            "a {where_} must be a mapping (legal keys: {})",
            legal.join(", ")
        ));
    };
    for (k, v) in m {
        let Some(key) = k.as_str() else {
            return Err(format!("a {where_} key must be a string"));
        };
        if !legal.contains(&key) {
            let sugg = lute_manifest::suggest::nearest(key, legal.iter().copied(), 2)
                .map(|k| format!(" — did you mean `{k}`?"))
                .unwrap_or_default();
            let other = if top_level {
                STEP_EXPECT_KEYS.contains(&key).then_some("a step")
            } else {
                PLAY_EXPECT_KEYS.contains(&key).then_some("the top-level")
            };
            let hint = other
                .map(|o| format!(" (`{key}` belongs in {o} `expect:`)"))
                .unwrap_or_default();
            return Err(format!(
                "unknown {where_} key `{key}`{sugg}{hint} (legal: {})",
                legal.join(", ")
            ));
        }
        validate_value(key, v)?;
    }
    Ok(())
}

/// The value shape of one known key.
fn validate_value(key: &str, v: &Yaml) -> Result<(), String> {
    match key {
        "winner" => scalar_text(v)
            .map(|_| ())
            .ok_or_else(|| format!("`expect.winner` must be a beat id or `{NO_WINNER}`")),
        "offered" | "notOffered" | "transcriptContains" | "transcriptLacks" => {
            string_list(key, v).map(|_| ())
        }
        "facts" | "notFacts" => {
            for atom in string_list(key, v)? {
                parse_atom(&atom).ok_or_else(|| {
                    format!("`expect.{key}` entry `{atom}` is not a ground atom `rel(a, b)`")
                })?;
            }
            Ok(())
        }
        "exit" => match scalar_text(v) {
            Some(e) if EXITS.contains(&e.as_str()) => Ok(()),
            _ => Err(format!("`expect.exit` must be one of: {}", EXITS.join(", "))),
        },
        "quests" => {
            let Yaml::Mapping(m) = v else {
                return Err("`expect.quests` must be a mapping `{ <quest id>: <state> }`".into());
            };
            for (id, st) in m {
                let id = id
                    .as_str()
                    .ok_or("`expect.quests` keys must be quest ids")?;
                match scalar_text(st) {
                    Some(s) if QUEST_STATES.contains(&s.as_str()) => {}
                    _ => {
                        return Err(format!(
                            "`expect.quests.{id}` must be one of: {}",
                            QUEST_STATES.join(", ")
                        ))
                    }
                }
            }
            Ok(())
        }
        "state" => {
            let Yaml::Mapping(m) = v else {
                return Err("`expect.state` must be a mapping `{ <state path>: <value> }`".into());
            };
            for (path, want) in m {
                let path = path
                    .as_str()
                    .ok_or("`expect.state` keys must be state paths")?;
                if scalar_text(want).is_none() {
                    return Err(format!(
                        "`expect.state.{path}` must be a bool, number or string"
                    ));
                }
            }
            Ok(())
        }
        _ => unreachable!("validate() filtered to the legal key sets"),
    }
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
fn parse_atom(text: &str) -> Option<(String, Vec<String>)> {
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
        !s.is_empty()
            && !s.contains(|c: char| c.is_whitespace() || matches!(c, '(' | ')' | ','))
    };
    (ident(rel) && args.iter().all(|a| ident(a))).then(|| (rel.to_string(), args))
}

/// The one spelling both sides of a fact comparison agree on.
fn canonical_atom(text: &str) -> String {
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
    for (index, label, expect) in steps {
        let rows: Vec<&StepOutcome> = outcome.steps.iter().filter(|s| s.index == *index).collect();
        if rows.is_empty() {
            misses.push(ExpectMiss {
                step: Some(*index),
                label: label.clone(),
                occasion: None,
                repetition: None,
                key: "(step reached)".to_string(),
                expected: "the step to run".to_string(),
                actual: format!("not reached — the play ended {} before it", outcome.exit),
            });
            continue;
        }
        let repeated = rows.len() > 1;
        for (r, row) in rows.iter().enumerate() {
            check_step(
                row,
                label.as_ref(),
                repeated.then_some(r + 1),
                expect,
                &mut misses,
            );
        }
    }
    if let Some(top) = top {
        check_end(outcome, top, &mut misses);
    }
    misses
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
    misses: &mut Vec<ExpectMiss>,
) {
    let Yaml::Mapping(m) = expect else { return };
    let occasion = match &row.target {
        Some(t) => format!("{} {t}", row.occasion),
        None => row.occasion.clone(),
    };
    let label = label.or(row.label.as_ref());
    let mut miss = |key: &str, expected: String, actual: String| {
        misses.push(ExpectMiss {
            step: Some(row.index),
            label: label.cloned(),
            occasion: Some(occasion.clone()),
            repetition,
            key: key.to_string(),
            expected,
            actual,
        })
    };
    let actual_winner = row.winner.clone().unwrap_or_else(|| NO_WINNER.to_string());
    if let Some(want) = m.get("winner").and_then(scalar_text) {
        let holds = match (&row.winner, want.as_str()) {
            (None, NO_WINNER) => true,
            (Some(w), want) => w == want,
            (None, _) => false,
        };
        if !holds {
            miss("winner", want, actual_winner.clone());
        }
    }
    let offered: BTreeSet<&str> = row.offered.iter().map(String::as_str).collect();
    if let Some(want) = m.get("offered").and_then(|v| string_list("offered", v).ok()) {
        let missing: Vec<String> = want
            .iter()
            .filter(|w| !offered.contains(w.as_str()))
            .cloned()
            .collect();
        if !missing.is_empty() {
            miss(
                "offered",
                format!("{} among the eligible beats (missing {})", list(&want), list(&missing)),
                list(&row.offered),
            );
        }
    }
    if let Some(want) = m
        .get("notOffered")
        .and_then(|v| string_list("notOffered", v).ok())
    {
        let present: Vec<String> = want
            .iter()
            .filter(|w| offered.contains(w.as_str()))
            .cloned()
            .collect();
        if !present.is_empty() {
            miss(
                "notOffered",
                format!("none of {} eligible", list(&want)),
                format!("{} (offending {})", list(&row.offered), list(&present)),
            );
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
    if let Some(want) = m.get("exit").and_then(scalar_text) {
        if want != outcome.exit {
            miss("exit".into(), want, outcome.exit.to_string());
        }
    }
    if let Some(Yaml::Mapping(quests)) = m.get("quests") {
        for (id, want) in quests {
            let (Some(id), Some(want)) = (id.as_str(), scalar_text(want)) else {
                continue;
            };
            match outcome.quests.get(id) {
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
            let actual = outcome.state.get(path);
            if !state_matches(want, actual) {
                miss(
                    format!("state {path}"),
                    scalar_text(want).unwrap_or_default(),
                    match actual {
                        Some(v) => value_text(v),
                        None => "no value (never written, not seeded, no default)".to_string(),
                    },
                );
            }
        }
    }
    let facts: BTreeSet<String> = outcome.facts.iter().map(|f| canonical_atom(f)).collect();
    for (key, want_held) in [("facts", true), ("notFacts", false)] {
        let Some(want) = m.get(key).and_then(|v| string_list(key, v).ok()) else {
            continue;
        };
        for atom in want {
            let held = facts.contains(&canonical_atom(&atom));
            if held != want_held {
                miss(
                    key.to_string(),
                    format!("{atom} {}", if want_held { "holds" } else { "does not hold" }),
                    format!("{atom} {}", if held { "holds" } else { "does not hold" }),
                );
            }
        }
    }
    for (key, want_present) in [("transcriptContains", true), ("transcriptLacks", false)] {
        let Some(want) = m.get(key).and_then(|v| string_list(key, v).ok()) else {
            continue;
        };
        for sub in want {
            let present = outcome.transcript.contains(&sub);
            if present != want_present {
                miss(
                    key.to_string(),
                    format!("{sub:?} {}", if want_present { "present" } else { "absent" }),
                    format!("{sub:?} {}", if present { "present" } else { "absent" }),
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
        (Yaml::Number(w), Some(Value::Num(a))) => w.as_f64() == Some(*a),
        (Yaml::String(w), Some(Value::Str(a))) => w == a,
        _ => false,
    }
}

/// A value as a miss line prints it — strings quoted so `"3"` and `3` differ.
fn value_text(v: &Value) -> String {
    match v {
        Value::Bool(b) => b.to_string(),
        Value::Num(n) if n.fract() == 0.0 && n.abs() < 1e15 => format!("{}", *n as i64),
        Value::Num(n) => n.to_string(),
        Value::Str(s) => format!("{s:?}"),
        Value::Unknown => "unknown".to_string(),
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
        }
    }

    fn outcome() -> PlayOutcome {
        PlayOutcome {
            steps: vec![
                step(1, Some("hub.welcome"), &["hub.welcome", "hub.idle"]),
                step(3, None, &[]),
            ],
            state: BTreeMap::from([
                ("run.day".to_string(), Value::Num(3.0)),
                ("run.outcome".to_string(), Value::Str("fell".into())),
                ("user.met".to_string(), Value::Bool(true)),
                ("run.fog".to_string(), Value::Unknown),
            ]),
            facts: BTreeSet::from(["knows(player, oskar)".to_string(), "slew(warden)".into()]),
            quests: BTreeMap::from([
                ("caseClosed".to_string(), "complete".to_string()),
                ("side".to_string(), "unset".to_string()),
            ]),
            transcript: "Oskar: Welcome back.\n".into(),
            exit: "complete",
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
exit: complete
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
            &[(1, None, y("{offered: [hub.welcome, hub.trophy], notOffered: [hub.idle]}"))],
            None,
        );
        let keys: Vec<&str> = misses.iter().map(|m| m.key.as_str()).collect();
        assert_eq!(keys, ["offered", "notOffered"]);
        assert!(misses[0].expected.contains("missing [hub.trophy]"));
        assert_eq!(misses[0].actual, "[hub.welcome, hub.idle]");
        assert_eq!(misses[1].actual, "[hub.welcome, hub.idle] (offending [hub.idle])");
    }

    #[test]
    fn a_step_the_play_never_reached_is_a_miss() {
        let o = PlayOutcome {
            exit: "incomplete",
            ..outcome()
        };
        let misses = check(&o, &[(2, None, y("{winner: none}"))], None);
        assert_eq!(misses.len(), 1);
        assert_eq!(misses[0].step, Some(2));
        assert!(misses[0].actual.contains("incomplete"), "{}", misses[0].actual);
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
        let top = y(r#"state: { run.day: "3", user.met: false, run.fog: x, run.none: 1, run.outcome: fell }"#);
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
                ("state run.none", "no value (never written, not seeded, no default)"),
            ]
        );
    }

    #[test]
    fn end_misses_cover_exit_quests_facts_and_transcript() {
        let top = y(r#"
exit: incomplete
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
                "exit",
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
            "end of play: expect exit: expected incomplete, actual complete"
        );
    }

    #[test]
    fn validate_rejects_unknown_keys_with_the_legal_list() {
        let e = validate(&y("{winer: hub.a}"), false).unwrap_err();
        assert!(e.contains("`winer`"), "{e}");
        assert!(e.contains("did you mean `winner`"), "{e}");
        assert!(e.contains("legal: notOffered, offered, winner"), "{e}");
        let e = validate(&y("{state: {run.day: 1}}"), false).unwrap_err();
        assert!(e.contains("belongs in the top-level"), "{e}");
        let e = validate(&y("{winner: a}"), true).unwrap_err();
        assert!(e.contains("belongs in a step"), "{e}");
        assert!(validate(&y("{winner: a, offered: [a], notOffered: [b]}"), false).is_ok());
    }

    #[test]
    fn validate_rejects_malformed_values() {
        for (text, top) in [
            ("{offered: a}", false),
            ("{exit: done}", true),
            ("{quests: {q: started}}", true),
            ("{state: {run.day: [1]}}", true),
            ("{facts: ['knows(a']}", true),
            ("[winner]", false),
        ] {
            assert!(validate(&y(text), top).is_err(), "{text} should be rejected");
        }
    }
}
