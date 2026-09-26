//! `lute play <PROJECT_DIR> --script <FILE>` — the occasion-driven reference
//! playthrough (dsl 0.21.0 §6, `docs/proposals/scenario-dsl/0.21.0.md`).
//!
//! A play script raises a sequence of **occasions**. For each one this module
//! computes the candidate **beats** (`ProjectIndex.beats` rows answering the
//! occasion, restricted by target), decides each candidate's verdict against
//! the LIVE playthrough — `once` spending, `after:` over the real `visited` /
//! `completed` / `active` sets, `when` through the reference runner's CEL
//! evaluator — orders the eligible ones by priority then index order (§4),
//! presents the winner (or the script's `pick` for a `select: all` occasion)
//! and then advances every quest lifecycle, so the next step's `when` over
//! `quest.*` and `after: completed(…)` see real progress (D-H). Then the
//! raised occasion judges the `<objective on="<occasion>">` objectives of
//! every active quest (§7a.2) — an occasion only objectives answer is a
//! legal step. A presentation's `::accept` activates its accept-driven
//! quest at that advance (§7a.3), and CEL `visited('<id>')` reads the
//! presented scenes (§7a.1). A presentation that runs `::end` still gets
//! both — the advance and the occasion's judging — before the playthrough
//! stops.
//!
//! ## Runs
//! One `lute play` invocation is one player profile. `once: run` is spent by
//! a presentation until the next `newRun` step; `once: user` stays spent for
//! the rest of the invocation (spending across separate invocations is not
//! modelled — put the runs in one script, separated by `newRun`, or start
//! from a save: `visited:` / `presented:` / `quests:` / `entriesRead:`, dsl
//! 0.22.0 §3). A `newRun` resets `run.*` state and `entry.<id>.read` flags
//! (run-tier, dsl 0.19.0 §5) to their declared defaults, run-tier facts to
//! the project's seed facts and `<quest tier="run">` quests to `unset` (dsl
//! 0.22.0 §7), then applies its long form's seed; `user.*`/`app.*` state,
//! user-tier quests, `entry.<id>.everRead`, other facts and the `visited`
//! history persist.
//!
//! ## The engine's part (dsl 0.22.0 §1, §9)
//! An `engine:` step writes what the engine owns — declared state (literal
//! or `{ add: n }`), facts of any declared base relation (reserved ones
//! included) and retractions — presents nothing, and the quest lifecycle
//! settles after it. An `event:` step fires a declared world event: active
//! quests' `<on event>` handlers run, as trace `events:` fires them.
//!
//! ## Assertions (dsl 0.22.0 §4)
//! Step and top-level `expect:` are judged by [`crate::play_expect`] against
//! the [`PlayOutcome`] the walk fills; a miss exits 1.
//!
//! ## What is reused, never re-implemented
//! - Whole-project compile + gate: [`crate::reconciled_project_results`] +
//!   [`crate::gate_for_doc`] + `lute_compile::compile_with_check`, the loop
//!   `compile --all` ([`crate::compile_all`]) runs, kept in memory.
//! - Beat table and declaration union: `lute_compile::index::build_index` —
//!   the SAME `beats` rows (and tiebreak order), rules, seed facts and
//!   relation tiers `compile --all` writes to `project.index.json`.
//! - Execution: [`lute_trace::exec::Machine`] — the walker `lute run` uses
//!   — driven by [`PlayDriver`], runs every scene beat, every entry beat
//!   (its `--entry` path: first-read effects, `entry.<id>.read`), every
//!   quest-lifecycle advance ([`Machine::advance_quests`]) and every `when`
//!   ([`Machine::eval_guard`]).
//! - Script surfaces: `state:` / `facts:` / `choose:` are parsed by the
//!   trace-mock grammar ([`lute_trace::parse_mock_yaml`]); only `steps:` is
//!   this module's own.
//!
//! ## Honesty
//! Nothing the reference runner cannot decide is decided silently: a `when`
//! that evaluates unknown and could change the outcome, an unscripted
//! choice/hub, an undecidable required quest objective, `now()` /
//! `validAt(...)`, and an unresolved plugin `bridgeResult` all halt the walk
//! incomplete (exit 3), naming what could not be decided.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_compile::Artifact;
use lute_manifest::schema::{OccasionDecl, OccasionSelect};
use lute_trace::exec::session::{
    domain_members, entry_flag, is_candidate, kind_label, parse_ground_fact, resolve_bridges,
    resolve_fact, resolve_state, seed_world, value_to_json, world_view, Candidate, ExecProject,
    Pick, PlayHalt, Played, Presented, QuestAdvance, SaveSeed, Session, StepBody, Verdict, World,
    WorldSeed, Write, Writes, QUEST_STATES,
};
use lute_trace::exec::{line_head, render_attrs, BridgeReads};
use lute_trace::{MockSet, Value};
use serde_json::{json, Value as Json};

use lute_trace::datalog::Fact;

use crate::play_expect::{ExpectMiss, PlayOutcome, StepOutcome, WorldView};

pub(crate) mod calendar;

// ===========================================================================
// Play script (`*.play.yaml`, dsl 0.21.0 §6, 0.22.0 §1–§4, §9, §10, §13).
// ===========================================================================

/// The complete legal top-level key set of a play script.
const SCRIPT_KEYS: &[&str] = &[
    "bridges",
    "choose",
    "derive",
    "entriesRead",
    "expect",
    "facts",
    "presented",
    "quests",
    "state",
    "steps",
    "visited",
];
/// The script keys that ARE trace-mock surfaces, parsed by the mock grammar.
const MOCK_SURFACES: &[&str] = &["bridges", "choose", "facts", "state"];
/// The complete legal key set of one `steps:` entry.
const STEP_KEYS: &[&str] = &[
    "advance", "bridges", "choose", "end", "engine", "event", "expect", "label", "newRun",
    "occasion", "pick", "repeat", "target",
];
/// The keys of a step that DO something — exactly one per step.
const STEP_ACTIONS: &[&str] = &["occasion", "newRun", "engine", "event", "advance", "end"];

/// One `state:` write of an `engine:` step / `newRun:` seed, as written.
enum RawWrite {
    /// A scalar literal, rendered back to text (the trace-mock idiom).
    Lit(String),
    /// `{ add: <number> }` — a numeric delta.
    Add(f64),
}

/// An `engine:` step's (or a long-form `newRun:`'s) writes, as written.
#[derive(Default)]
struct RawWrites {
    state: Vec<(String, RawWrite)>,
    facts: Vec<String>,
    retract: Vec<String>,
    /// dsl 0.26.0 §7 (T2-9): `engine: { accept: [quest ids] }` — the engine
    /// accepts these quests (a quest board, a menu) at this step.
    accept: Vec<String>,
}

/// What one `steps:` entry does.
enum StepAction {
    /// Raise `occasion` (for `target`); `pick` is the player's take on a
    /// `select: all` occasion; `choose` replaces the script's `choose:` key
    /// by key for this step's presentation (dsl 0.22.0 §2).
    Occasion {
        occasion: String,
        target: Option<String>,
        pick: Option<Pick>,
        choose: BTreeMap<String, Vec<String>>,
    },
    /// Start a new run; the long form's writes seed it (dsl 0.22.0 §1.1).
    NewRun(RawWrites),
    /// Write what the engine owns (dsl 0.22.0 §1.1).
    Engine(RawWrites),
    /// Fire a declared world event (dsl 0.22.0 §9).
    Event(String),
    /// dsl 0.24.0 §1: move the declared clock forward, settle the quests,
    /// then raise the clock's `raise` occasion (when it declares one) —
    /// `pick`/`choose` are the player's take on that occasion;
    /// `occasion_expect` the first selection key the step's `expect:`
    /// judges it by (refused, like them, when the clock raises nothing).
    Advance {
        by: AdvanceBy,
        /// dsl 0.24.0 §1 (ER N16): the `engine:` writes of the same moment,
        /// applied before the clock moves; one settle follows both.
        writes: RawWrites,
        pick: Option<Pick>,
        choose: BTreeMap<String, Vec<String>>,
        occasion_expect: Option<&'static str>,
    },
    /// `end: true` (0.23.1): the playthrough is over — later steps are
    /// skipped. A `::end` in a presentation ends only that presentation.
    End,
}

/// How far an `advance:` step moves the clock, as written.
#[derive(Clone)]
enum AdvanceBy {
    /// `slot`, `day` or a number of slots.
    By(lute_manifest::clock::Advance),
    /// dsl 0.26.0 §7 (T2-5): `{ to: <slot> }` / `{ to: { weekday, slot } }`
    /// — resolved against the clock when the step is planned. `weekday` is
    /// a `week.labels` label or a `clock.weekday` number.
    To {
        weekday: Option<String>,
        slot: Option<String>,
    },
}

/// One `steps:` entry.
struct ScriptStep {
    /// 1-based position in `steps:` — the `N` of every "step N" message.
    n: usize,
    /// `label:` (dsl 0.22.0 §13), printed in the transcript.
    label: Option<String>,
    /// `repeat:` (dsl 0.22.0 §13): how many times the step runs (≥ 1).
    repeat: usize,
    action: StepAction,
    /// `bridges:` (dsl 0.24.0 §5): answers for the plugin calls this step
    /// makes, consumed before the top-level ones.
    bridges: BTreeMap<String, Vec<lute_trace::BridgeAnswer>>,
}

/// A parsed play script.
struct PlayScript {
    /// `state:` / `facts:` / `choose:`, exactly as a trace mock carries them.
    surfaces: MockSet,
    save: SaveSeed,
    steps: Vec<ScriptStep>,
    /// Every step `expect:` (dsl 0.22.0 §4): `(step n, label, expect)`.
    step_expects: Vec<(usize, Option<String>, serde_yaml::Value)>,
    /// The top-level (end-of-play) `expect:`.
    expect: Option<serde_yaml::Value>,
    /// The top-level `derive:` key (dsl 0.22.0 §6); `None` = the default.
    derive: Option<bool>,
}

impl PlayScript {
    fn has_expect(&self) -> bool {
        self.expect.is_some() || !self.step_expects.is_empty()
    }
}

/// A YAML scalar rendered back to the text a mock literal carries.
fn scalar_text(v: &serde_yaml::Value) -> Option<String> {
    match v {
        serde_yaml::Value::Bool(b) => Some(b.to_string()),
        serde_yaml::Value::Number(n) => Some(n.to_string()),
        serde_yaml::Value::String(s) => Some(s.clone()),
        _ => None,
    }
}

/// A list of non-empty strings (`what` names it in the error).
fn string_list(v: &serde_yaml::Value, what: &str) -> Result<Vec<String>, String> {
    let serde_yaml::Value::Sequence(items) = v else {
        return Err(format!("{what} must be a list"));
    };
    items
        .iter()
        .map(|item| {
            item.as_str()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .ok_or_else(|| format!("every {what} entry must be a non-empty string"))
        })
        .collect()
}

/// A mapping whose only keys are `legal`, each an optional string list:
/// `presented: { user, run }` / `entriesRead: { run, user }`.
fn tiered_lists(v: &serde_yaml::Value, key: &str) -> Result<(Vec<String>, Vec<String>), String> {
    let serde_yaml::Value::Mapping(m) = v else {
        return Err(format!(
            "`{key}:` must be a mapping `{{ run: [...], user: [...] }}`"
        ));
    };
    let (mut run, mut user) = (Vec::new(), Vec::new());
    for (k, v) in m {
        match k.as_str() {
            Some("run") => run = string_list(v, &format!("`{key}.run`"))?,
            Some("user") => user = string_list(v, &format!("`{key}.user`"))?,
            _ => {
                return Err(format!(
                    "`{key}:` takes only `run:` and `user:` (got `{}`)",
                    k.as_str().unwrap_or("?")
                ))
            }
        }
    }
    Ok((run, user))
}

/// Parse the play script at `path` (its text already read). Total: never
/// panics; `Err` names what is wrong.
fn parse_script(text: &str, path: &Path) -> Result<PlayScript, String> {
    parse_script_with(text, path, true)
}

/// [`parse_script`]; `steps_required: false` also admits a script that is
/// only a save (seeds and no `steps:`) — what `lute calendar --script`
/// starts every cell from (dsl 0.23.0 §1).
fn parse_script_with(text: &str, path: &Path, steps_required: bool) -> Result<PlayScript, String> {
    let value: serde_yaml::Value =
        serde_yaml::from_str(text).map_err(|e| format!("malformed YAML: {e}"))?;
    let serde_yaml::Value::Mapping(top) = value else {
        return Err("a play script must be a YAML mapping with a `steps:` list".to_string());
    };
    let mut surfaces = serde_yaml::Mapping::new();
    let mut steps = None;
    let mut save = SaveSeed::default();
    let (mut expect, mut derive) = (None, None);
    for (k, v) in &top {
        let Some(key) = k.as_str() else {
            return Err("a play script's top-level keys must be strings".to_string());
        };
        match key {
            "steps" => steps = Some(v),
            _ if MOCK_SURFACES.contains(&key) => {
                surfaces.insert(k.clone(), v.clone());
            }
            "visited" => save.visited = string_list(v, "`visited:`")?,
            "presented" => (save.presented_run, save.presented_user) = tiered_lists(v, key)?,
            "entriesRead" => (save.entries_run, save.entries_user) = tiered_lists(v, key)?,
            "quests" => {
                let serde_yaml::Value::Mapping(m) = v else {
                    return Err("`quests:` must be a mapping of quest id -> status".to_string());
                };
                for (id, status) in m {
                    let (Some(id), Some(status)) = (id.as_str(), status.as_str()) else {
                        return Err(format!(
                            "`quests:` maps a quest id to one of {}",
                            QUEST_STATES.join(", ")
                        ));
                    };
                    save.quests.push((id.to_string(), status.to_string()));
                }
            }
            "expect" => {
                crate::play_expect::validate(v, true)?;
                expect = Some(v.clone());
            }
            "derive" => {
                let Some(b) = v.as_bool() else {
                    return Err("`derive:` must be `true` or `false`".to_string());
                };
                derive = Some(b);
            }
            _ => {
                return Err(format!(
                    "unknown top-level key `{key}` (legal: {})",
                    SCRIPT_KEYS.join(", ")
                ))
            }
        }
    }
    let surfaces = if surfaces.is_empty() {
        MockSet::default()
    } else {
        let text = serde_yaml::to_string(&serde_yaml::Value::Mapping(surfaces))
            .map_err(|e| format!("cannot re-read `state:`/`facts:`/`choose:`: {e}"))?;
        lute_trace::parse_mock_yaml(&text).map_err(|d| d.message)?
    };
    let items = match steps {
        Some(serde_yaml::Value::Sequence(items)) => items.as_slice(),
        Some(_) => return Err("`steps:` must be a list".to_string()),
        None if steps_required => {
            return Err("`steps:` is required — the occasions to raise, in order".to_string())
        }
        None => &[],
    };
    if items.is_empty() && steps_required {
        return Err("`steps:` is empty — there is nothing to play".to_string());
    }
    // dsl 0.24.0 §1: `- include: <file>` splices that file's steps in its
    // place; steps are numbered after the splice.
    let mut stack = vec![path.canonicalize().unwrap_or_else(|_| path.to_path_buf())];
    let items = expand_includes(items, path, &mut stack)?;
    let mut parsed = Vec::with_capacity(items.len());
    let mut step_expects = Vec::new();
    for (i, item) in items.iter().enumerate() {
        let (step, expect) = parse_step(i + 1, item)?;
        if let Some(e) = expect {
            step_expects.push((step.n, step.label.clone(), e));
        }
        parsed.push(step);
    }
    Ok(PlayScript {
        surfaces,
        save,
        steps: parsed,
        step_expects,
        expect,
        derive,
    })
}

/// dsl 0.24.0 §1: `steps` with every `- include: <file>` entry replaced by
/// that file's steps, recursively. `from` is the file `steps` came from (an
/// include resolves against its directory); `stack` is the chain of files
/// being expanded — naming one of them again is a cycle. An included file is
/// a list of steps or a mapping whose only key is `steps:`.
fn expand_includes(
    steps: &[serde_yaml::Value],
    from: &Path,
    stack: &mut Vec<PathBuf>,
) -> Result<Vec<serde_yaml::Value>, String> {
    let mut out = Vec::with_capacity(steps.len());
    for item in steps {
        let Some(target) = item.get("include") else {
            out.push(item.clone());
            continue;
        };
        let shown = from.display();
        let serde_yaml::Value::Mapping(m) = item else {
            unreachable!("`get` found a key")
        };
        if m.len() != 1 {
            return Err(format!(
                "{shown}: an `include:` step names only the file whose steps it splices in"
            ));
        }
        let Some(rel) = target.as_str().map(str::trim).filter(|s| !s.is_empty()) else {
            return Err(format!("{shown}: `include:` must name a file"));
        };
        let file = from.parent().unwrap_or(Path::new(".")).join(rel);
        let canonical = file.canonicalize().map_err(|e| {
            format!(
                "{shown}: cannot read `include: {rel}` ({}): {e}",
                file.display()
            )
        })?;
        if stack.contains(&canonical) {
            return Err(format!(
                "{shown}: `include: {rel}` is a cycle — {} is already being included",
                file.display()
            ));
        }
        let text = std::fs::read_to_string(&canonical).map_err(|e| {
            format!(
                "{shown}: cannot read `include: {rel}` ({}): {e}",
                file.display()
            )
        })?;
        let value: serde_yaml::Value = serde_yaml::from_str(&text)
            .map_err(|e| format!("{}: malformed YAML: {e}", file.display()))?;
        let included = match &value {
            serde_yaml::Value::Sequence(items) => Some(items.as_slice()),
            serde_yaml::Value::Mapping(m) if m.len() == 1 => match m.get("steps") {
                Some(serde_yaml::Value::Sequence(items)) => Some(items.as_slice()),
                _ => None,
            },
            _ => None,
        };
        let Some(included) = included else {
            return Err(format!(
                "{}: an included file is a list of steps or `steps: […]`",
                file.display()
            ));
        };
        stack.push(canonical);
        out.extend(expand_includes(included, &file, stack)?);
        stack.pop();
    }
    Ok(out)
}

/// `engine:` / long-form `newRun:` writes. `retract` says whether
/// `retract:` and `accept:` are legal (a new run's seed only adds).
fn parse_writes(
    n: usize,
    key: &str,
    v: &serde_yaml::Value,
    retract: bool,
) -> Result<RawWrites, String> {
    let legal = if retract {
        "state, facts, retract, accept"
    } else {
        "state, facts"
    };
    let serde_yaml::Value::Mapping(m) = v else {
        return Err(format!("step {n}: `{key}:` must be a mapping ({legal})"));
    };
    let mut w = RawWrites::default();
    for (k, v) in m {
        match k.as_str() {
            Some("state") => {
                let serde_yaml::Value::Mapping(paths) = v else {
                    return Err(format!(
                        "step {n}: `{key}.state` must be a mapping of path -> value"
                    ));
                };
                for (path, value) in paths {
                    let Some(path) = path.as_str() else {
                        return Err(format!("step {n}: `{key}.state` keys must be state paths"));
                    };
                    let write = match value {
                        serde_yaml::Value::Mapping(delta) => {
                            let add = match (delta.len(), delta.get("add")) {
                                (1, Some(a)) => a.as_f64(),
                                _ => None,
                            };
                            let Some(add) = add else {
                                return Err(format!(
                                    "step {n}: `{key}.state.{path}` — a delta is `{{ add: <number> }}`"
                                ));
                            };
                            RawWrite::Add(add)
                        }
                        other => RawWrite::Lit(scalar_text(other).ok_or_else(|| {
                            format!(
                                "step {n}: `{key}.state.{path}` must be a literal or `{{ add: <number> }}`"
                            )
                        })?),
                    };
                    w.state.push((path.to_string(), write));
                }
            }
            Some("facts") => w.facts = string_list(v, &format!("step {n}: `{key}.facts`"))?,
            Some("retract") if retract => {
                w.retract = string_list(v, &format!("step {n}: `{key}.retract`"))?
            }
            Some("accept") if retract => {
                w.accept = string_list(v, &format!("step {n}: `{key}.accept`"))?
            }
            other => {
                return Err(format!(
                    "step {n}: unknown `{key}:` key `{}` (legal: {legal})",
                    other.unwrap_or("?")
                ))
            }
        }
    }
    if w.state.is_empty() && w.facts.is_empty() && w.retract.is_empty() && w.accept.is_empty() {
        return Err(format!("step {n}: `{key}:` writes nothing ({legal})"));
    }
    Ok(w)
}

/// One `steps:` entry and its `expect:` (validated). Exactly one action key
/// ([`STEP_ACTIONS`]); `target`/`pick`/`choose` only beside `occasion`;
/// `label`/`repeat`/`expect` beside any (`repeat`/`expect` not beside `end`).
fn parse_step(
    n: usize,
    item: &serde_yaml::Value,
) -> Result<(ScriptStep, Option<serde_yaml::Value>), String> {
    let shape = "one of `occasion` (with `target`, `pick`, `choose`), `newRun`, `engine`, \
                 `event`, `advance` (with `pick`, `choose`, `engine`), `end` — plus \
                 `label`/`repeat`/`expect`";
    let serde_yaml::Value::Mapping(m) = item else {
        return Err(format!("step {n} must be a mapping — {shape}"));
    };
    if m.is_empty() {
        return Err(format!("step {n} is empty — {shape}"));
    }
    let non_empty = |key: &str, v: &serde_yaml::Value| {
        v.as_str()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .ok_or_else(|| format!("step {n}: `{key}` must be a non-empty string"))
    };
    let mut actions: Vec<&str> = Vec::new();
    let (mut occasion, mut target, mut pick, mut event) = (None, None, None, None);
    let (mut label, mut repeat, mut expect) = (None, 1usize, None);
    let mut choose = BTreeMap::new();
    let mut writes = None;
    let mut advance = None;
    let mut bridges = BTreeMap::new();
    let mut occasion_only: Vec<&str> = Vec::new();
    for (k, v) in m {
        let Some(key) = k.as_str() else {
            return Err(format!("step {n}: keys must be strings"));
        };
        let Some(&key) = STEP_KEYS.iter().find(|&&s| s == key) else {
            return Err(format!(
                "step {n}: unknown key `{key}` (legal: {})",
                STEP_KEYS.join(", ")
            ));
        };
        if STEP_ACTIONS.contains(&key) {
            actions.push(key);
        }
        match key {
            "occasion" => occasion = Some(non_empty(key, v)?),
            "event" => event = Some(non_empty(key, v)?),
            "label" => label = Some(non_empty(key, v)?),
            "target" => {
                occasion_only.push(key);
                target = Some(non_empty(key, v)?);
            }
            "pick" => {
                occasion_only.push(key);
                let s = non_empty(key, v)?;
                pick = Some(if s == "none" {
                    Pick::Pass
                } else {
                    Pick::Beat(s)
                });
            }
            "choose" => {
                occasion_only.push(key);
                let mut doc = serde_yaml::Mapping::new();
                doc.insert("choose".into(), v.clone());
                let text = serde_yaml::to_string(&serde_yaml::Value::Mapping(doc))
                    .map_err(|e| format!("step {n}: cannot re-read `choose:`: {e}"))?;
                choose = lute_trace::parse_mock_yaml(&text)
                    .map_err(|d| format!("step {n}: {}", d.message))?
                    .choose;
            }
            "bridges" => {
                bridges = lute_trace::parse_bridges(v).map_err(|e| format!("step {n}: {e}"))?
            }
            "expect" => {
                crate::play_expect::validate(v, false).map_err(|e| format!("step {n}: {e}"))?;
                expect = Some(v.clone());
            }
            "repeat" => {
                repeat = v
                    .as_u64()
                    .filter(|r| *r >= 1)
                    .and_then(|r| usize::try_from(r).ok())
                    .ok_or_else(|| format!("step {n}: `repeat` must be a whole number ≥ 1"))?;
            }
            "newRun" => {
                writes = Some(match v {
                    serde_yaml::Value::Bool(true) => RawWrites::default(),
                    serde_yaml::Value::Mapping(_) => parse_writes(n, key, v, false)?,
                    _ => {
                        return Err(format!(
                            "step {n}: `newRun` must be `true` or `{{ state: …, facts: … }}`"
                        ))
                    }
                });
            }
            "engine" => writes = Some(parse_writes(n, key, v, true)?),
            "advance" => {
                use lute_manifest::clock::Advance;
                let shape = "`advance` is `slot`, `day`, a number of slots, `{ to: <slot> }` \
                             or `{ to: { weekday: <label or number>, slot: <slot> } }`";
                advance = Some(match v {
                    serde_yaml::Value::String(s) if s.trim() == "slot" => {
                        AdvanceBy::By(Advance::Slots(1))
                    }
                    serde_yaml::Value::String(s) if s.trim() == "day" => {
                        AdvanceBy::By(Advance::Day)
                    }
                    serde_yaml::Value::Number(k) => {
                        match k.as_u64().and_then(|k| u32::try_from(k).ok()) {
                            Some(k) if k >= 1 => AdvanceBy::By(Advance::Slots(k)),
                            _ => {
                                return Err(format!(
                                "step {n}: `advance: {k}` — a slot count is a whole number ≥ 1 \
                                 (the clock never moves backward)"
                            ))
                            }
                        }
                    }
                    // dsl 0.26.0 §7 (T2-5): forward to the next such position.
                    serde_yaml::Value::Mapping(m) if m.len() == 1 && m.contains_key("to") => {
                        match m.get("to").unwrap_or(&serde_yaml::Value::Null) {
                            serde_yaml::Value::String(slot) if !slot.trim().is_empty() => {
                                AdvanceBy::To {
                                    weekday: None,
                                    slot: Some(slot.trim().to_string()),
                                }
                            }
                            serde_yaml::Value::Mapping(to) => {
                                let (mut weekday, mut slot) = (None, None);
                                for (k, v) in to {
                                    match (k.as_str(), scalar_text(v)) {
                                        (Some("weekday"), Some(w)) => weekday = Some(w),
                                        (Some("slot"), Some(s)) => slot = Some(s),
                                        _ => return Err(format!("step {n}: {shape}")),
                                    }
                                }
                                if weekday.is_none() && slot.is_none() {
                                    return Err(format!("step {n}: {shape}"));
                                }
                                AdvanceBy::To { weekday, slot }
                            }
                            _ => return Err(format!("step {n}: {shape}")),
                        }
                    }
                    _ => return Err(format!("step {n}: {shape}")),
                });
            }
            "end" => {
                if v != &serde_yaml::Value::Bool(true) {
                    return Err(format!(
                        "step {n}: `end` must be `true` — it ends the playthrough; later \
                         steps are skipped"
                    ));
                }
            }
            _ => unreachable!("every STEP_KEYS key is matched"),
        }
    }
    // dsl 0.24.0 §1: an `advance:` may carry the `engine:` writes of the same
    // moment (applied before the clock moves, one settle for both).
    if actions.contains(&"advance") && actions.contains(&"engine") {
        actions.retain(|a| *a != "engine");
    }
    let action = match actions.as_slice() {
        [] => return Err(format!("step {n} names no action — {shape}")),
        [a, b, ..] => {
            return Err(format!(
                "step {n}: a step raises an `occasion`, starts a `newRun`, applies `engine` \
                 writes, fires an `event`, advances the clock (`advance`, which may carry \
                 `engine` writes) or ends the playthrough (`end`) — not both `{a}` and `{b}`"
            ))
        }
        [one] => *one,
    };
    // dsl 0.24.0 §1: an `advance:` raises the clock's `raise` occasion, so
    // it takes the occasion's `pick`/`choose` and selection expectations
    // (the plan refuses them when the clock raises nothing).
    let raises = action == "occasion" || action == "advance";
    if action != "occasion" {
        let misplaced = occasion_only.iter().find(|k| !raises || **k == "target");
        if let Some(key) = misplaced {
            let only = if *key == "target" {
                "an `occasion` step"
            } else {
                "an `occasion` or `advance` step"
            };
            return Err(format!(
                "step {n}: `{key}` applies only to {only}, not `{action}`"
            ));
        }
    }
    if !raises {
        if let Some(key) = expect.as_ref().and_then(crate::play_expect::occasion_key) {
            return Err(format!(
                "step {n}: `expect.{key}` applies only to an `occasion` or `advance` step, not \
                 `{action}` (a `{action}` step may expect quests, state, facts, notFacts)"
            ));
        }
    }
    if action == "end" && (repeat != 1 || expect.is_some() || !bridges.is_empty()) {
        let key = if repeat != 1 {
            "repeat"
        } else if expect.is_some() {
            "expect"
        } else {
            "bridges"
        };
        return Err(format!(
            "step {n}: `{key}` does not apply to an `end` step — it ends the playthrough \
             (put the expectation on the step before it, or at the top level)"
        ));
    }
    let action = match action {
        "occasion" => StepAction::Occasion {
            occasion: occasion.unwrap_or_default(),
            target,
            pick,
            choose,
        },
        "event" => StepAction::Event(event.unwrap_or_default()),
        "advance" => StepAction::Advance {
            by: advance.expect("an `advance` action parsed its value"),
            writes: writes.take().unwrap_or_default(),
            pick,
            choose,
            occasion_expect: expect.as_ref().and_then(crate::play_expect::occasion_key),
        },
        "newRun" => StepAction::NewRun(writes.unwrap_or_default()),
        "end" => StepAction::End,
        _ => StepAction::Engine(writes.unwrap_or_default()),
    };
    Ok((
        ScriptStep {
            n,
            label,
            repeat,
            action,
            bridges,
        },
        expect,
    ))
}

// ===========================================================================
// Whole-project compile (the `compile --all` loop, in memory) + the index.
// ===========================================================================

/// `path` relative to `root`, forward-slash joined — the project-relative
/// artifact identity `compile_all.rs`'s private `rel_slash` uses.
fn project_rel(path: &Path, root: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let mut out = String::new();
    for c in rel.components() {
        if !out.is_empty() {
            out.push('/');
        }
        out.push_str(&c.as_os_str().to_string_lossy());
    }
    (!out.is_empty()).then_some(out)
}

/// Compile every non-component document under `project_dir` in memory with
/// the `compile --all` gate — refusing to play a project that does not
/// wholly compile — and build its index. `Err` carries the exit code after
/// the diagnostics are printed.
fn compile_project(project_dir: &Path) -> Result<ExecProject, ExitCode> {
    match crate::manifests::validate_manifests_under(project_dir) {
        Ok(mut verdicts) => {
            crate::manifests::mark_inert_under(&mut verdicts, project_dir);
            if crate::manifests::report_and_gate(&verdicts) {
                return Err(ExitCode::from(1));
            }
        }
        Err(e) => {
            eprintln!(
                "lute play: cannot walk {} for manifests: {e}",
                project_dir.display()
            );
            return Err(ExitCode::from(2));
        }
    }

    let reconciled = crate::reconciled_project_results(project_dir, None)?;
    let identity = lute_manifest::project::load_project(project_dir)
        .ok()
        .flatten()
        .map(|p| p.identity)
        .unwrap_or_default();

    let mut compiled: BTreeMap<String, Artifact> = BTreeMap::new();
    let mut occasions: BTreeMap<String, OccasionDecl> = BTreeMap::new();
    let mut failures: BTreeMap<PathBuf, String> = BTreeMap::new();
    let policy = crate::DenyPolicy::default();
    let mut world_events: BTreeSet<String> = BTreeSet::new();
    // dsl 0.26.0 §3.1: the bridge capabilities' `result:` types, per tag.
    let mut bridge_types = BridgeReads::default();
    let cache = crate::InputCache::default();
    let mut display_names: BTreeMap<String, String> = BTreeMap::new();

    for (file, base) in &reconciled.per_doc {
        if crate::compile_all::is_component_file(file) {
            continue;
        }
        let Some(rel) = project_rel(file, project_dir) else {
            eprintln!(
                "lute play: {} is not under {}",
                file.display(),
                project_dir.display()
            );
            return Err(ExitCode::from(2));
        };
        let Some(built) = crate::build_input_with(&cache, file, None, Some(project_dir), None)
        else {
            return Err(ExitCode::from(2));
        };
        built.report_project_diags();
        if built.resolve_error {
            return Err(ExitCode::from(1));
        }
        for (name, decl) in &built.input.snapshot.occasions {
            occasions
                .entry(name.clone())
                .or_insert_with(|| decl.clone());
        }
        world_events.extend(built.input.snapshot.events.keys().cloned());
        for (id, m) in
            lute_check::cast::declared_cast(&built.input.snapshot, &built.input.imports, &[])
        {
            if let Some(name) = m.name {
                display_names.entry(id).or_insert(name);
            }
        }
        bridge_types = bridge_types.with_result_types(&built.input.snapshot);
        let gate = crate::gate_for_doc(&reconciled, file, base);
        match lute_compile::compile_with_check(&built.input, gate, &identity) {
            Ok(artifact) => {
                compiled.insert(rel, artifact);
            }
            Err(diags) => {
                failures.insert(
                    file.clone(),
                    crate::render_diagnostics(file, &diags, &policy),
                );
            }
        }
    }

    if !failures.is_empty() {
        for rendered in failures.values() {
            print!("{rendered}");
        }
        eprintln!(
            "lute play: {} of {} document(s) failed to compile; refusing to play",
            failures.len(),
            failures.len() + compiled.len()
        );
        return Err(ExitCode::from(1));
    }

    ExecProject::assemble(
        &compiled,
        occasions,
        world_events,
        bridge_types,
        display_names,
    )
    .map_err(|(code, lines)| {
        for line in &lines {
            eprintln!("{line}");
        }
        ExitCode::from(code)
    })
}

/// What a planned step does.
enum Action {
    Occasion {
        occasion: String,
        target: Option<String>,
        pick: Option<Pick>,
        choose: BTreeMap<String, Vec<String>>,
    },
    NewRun(Writes),
    Engine(Writes),
    Event(String),
    /// dsl 0.24.0 §1: apply `writes` (the step's `engine:`), move the clock
    /// `by` — raising the clock's `dayEnd` / `dayStart` at every midnight it
    /// crosses — settle, then raise its `slot` occasion (when it declares
    /// one) with the step's `pick` and `choose`.
    Advance {
        by: lute_manifest::clock::Advance,
        writes: Writes,
        raise: lute_manifest::clock::RaiseMoments,
        pick: Option<Pick>,
        choose: BTreeMap<String, Vec<String>>,
    },
    /// `end: true` — the playthrough is over.
    End,
}

/// One step, validated against the project.
struct Step {
    n: usize,
    label: Option<String>,
    repeat: usize,
    action: Action,
    /// The step's own `bridges:` (dsl 0.24.0 §5), resolved against the
    /// project's plugin calls — every run of the step gets them afresh.
    bridges: BTreeMap<String, VecDeque<lute_trace::BridgeAnswer>>,
}

/// Resolve an `engine:` step's / `newRun` seed's writes. A `quest.*` path is
/// the quest runner's (its lifecycle transitions fire handlers and grants),
/// never written directly — a save's quest status is top-level `quests:`.
fn resolve_writes(p: &ExecProject, n: usize, key: &str, raw: &RawWrites) -> Result<Writes, String> {
    let mut out = Writes::default();
    for (path, write) in &raw.state {
        let at = format!("step {n}: `{key}.state.{path}`");
        if path.starts_with("quest.") {
            return Err(format!(
                "{at}: quest state is written by the quest lifecycle, not the engine — seed a \
                 save's quest status with top-level `quests:`"
            ));
        }
        let write = match write {
            RawWrite::Lit(lit) => {
                Write::Set(resolve_state(p, path, lit).map_err(|e| format!("{at} {e}"))?)
            }
            RawWrite::Add(d) => {
                let ty = p
                    .state_table
                    .get(path)
                    .map(|e| e.get("type").and_then(Json::as_str));
                let why = match ty {
                    _ if path.starts_with("scene.") => resolve_state(p, path, "").err(),
                    None if entry_flag(path).is_none() => resolve_state(p, path, "").err(),
                    Some(Some("number")) => None,
                    _ => Some("is not a `number` path, so it takes no `{ add: … }`".to_string()),
                };
                if let Some(why) = why {
                    return Err(format!("{at} {why}"));
                }
                Write::Add(*d)
            }
        };
        out.state.push((path.clone(), write));
    }
    for (list, into, what) in [
        (&raw.facts, &mut out.facts, "facts"),
        (&raw.retract, &mut out.retract, "retract"),
    ] {
        for f in list {
            into.push(
                resolve_fact(p, f)
                    .map_err(|e| format!("step {n}: `{key}.{what}` entry `{f}` {e}"))?,
            );
        }
    }
    // dsl 0.26.0 §7 (T2-9): the engine accepts an accept-driven quest.
    for id in &raw.accept {
        if !p.accept_driven.contains(id) {
            return Err(format!(
                "step {n}: `{key}.accept` names `{id}`, which is no accept-driven quest of this \
                 project (a quest with no `start`, e.g. `accept=\"external\"`)"
            ));
        }
        out.accept.push(id.clone());
    }
    Ok(out)
}

/// Every step's usage-level check, before anything plays (exit 2 on `Err`),
/// producing the plan the walk executes. An occasion exists (under
/// shape-only vocabulary: a beat answers it or an objective is judged at
/// it, dsl 0.21.0 §7a.2); `target` is given exactly when the occasion takes
/// one and lies in its domain (dsl 0.22.0 §8); `pick` is required exactly
/// for `select: all` and names a beat answering that occasion, or `none`
/// (§10); an `engine:`/`newRun` write names declared paths and relations
/// with values that fit (§1.1); an `event:` names a declared world event
/// (§9).
fn plan_steps(p: &ExecProject, steps: &[ScriptStep]) -> Result<Vec<Step>, String> {
    let mut answered: BTreeSet<&str> = p.index.beats.iter().map(|b| b.on.as_str()).collect();
    answered.extend(p.objective_occasions.iter().map(String::as_str));
    let mut plan = Vec::with_capacity(steps.len());
    for step in steps {
        let n = step.n;
        let action = match &step.action {
            StepAction::NewRun(raw) => Action::NewRun(resolve_writes(p, n, "newRun", raw)?),
            StepAction::Engine(raw) => Action::Engine(resolve_writes(p, n, "engine", raw)?),
            StepAction::End => Action::End,
            StepAction::Event(name) => {
                if lute_manifest::snapshot::BUILTIN_LIFECYCLE_EVENTS.contains(&name.as_str()) {
                    return Err(format!(
                        "step {n}: `{name}` is a quest lifecycle event — the quest runner fires it \
                         on a transition; it cannot be fired from a script"
                    ));
                }
                if !p.world_events.contains(name) {
                    let hint = if p.occasions.contains_key(name) || answered.contains(name.as_str())
                    {
                        format!(" — `{name}` is an occasion; raise it with `occasion: {name}`")
                    } else if p.world_events.is_empty() {
                        " (no plugin declares world events)".to_string()
                    } else {
                        let declared: Vec<&str> =
                            p.world_events.iter().map(String::as_str).collect();
                        format!(" (declared: {})", declared.join(", "))
                    };
                    return Err(format!(
                        "step {n}: `event: {name}` names no declared world event{hint}"
                    ));
                }
                Action::Event(name.clone())
            }
            StepAction::Occasion {
                occasion,
                target,
                pick,
                choose,
            } => {
                let pick = entry_pick(p, pick);
                plan_occasion(p, &answered, n, occasion, target.as_deref(), pick.as_ref())?;
                Action::Occasion {
                    occasion: occasion.clone(),
                    target: target.clone(),
                    pick,
                    choose: choose.clone(),
                }
            }
            StepAction::Advance {
                by,
                writes,
                pick,
                choose,
                occasion_expect,
            } => {
                let Some(clock) = &p.index.clock else {
                    return Err(format!(
                        "step {n}: `advance:` moves the declared clock, and no schema of this \
                         project declares a `clock:` (dsl 0.24.0 §1)"
                    ));
                };
                let writes = resolve_writes(p, n, "engine", writes)?;
                if let Some((path, _)) = writes
                    .state
                    .iter()
                    .find(|(path, _)| *path == clock.day || clock.slot.as_ref() == Some(path))
                {
                    return Err(format!(
                        "step {n}: `engine:` writes `{path}`, which the `advance:` beside it \
                         moves — write the clock in a step of its own, or let `advance:` move it"
                    ));
                }
                let by = resolve_advance(clock, n, by)?;
                let pick = &entry_pick(p, pick);
                let raise = clock.raises();
                if raise.slot.is_none() {
                    let key = if pick.is_some() {
                        Some("`pick`".to_string())
                    } else if !choose.is_empty() {
                        Some("`choose`".to_string())
                    } else {
                        occasion_expect.map(|k| format!("`expect.{k}`"))
                    };
                    if let Some(key) = key {
                        return Err(format!(
                            "step {n}: {key} judges the occasion an `advance:` raises where the \
                             clock stops, and the clock declares no `raise:` slot occasion — \
                             the advance presents nothing there"
                        ));
                    }
                }
                let moments = [
                    (&raise.slot, pick.as_ref()),
                    (&raise.day_start, None),
                    (&raise.day_end, None),
                ];
                for (occasion, pick) in moments {
                    let Some(occasion) = occasion else { continue };
                    if p.occasions
                        .get(occasion)
                        .is_some_and(|d| d.target.takes_target())
                    {
                        return Err(format!(
                            "step {n}: the clock raises `{occasion}`, which is declared \
                             `target: true` — an `advance:` raises it for no target"
                        ));
                    }
                    // Shape-only vocabulary: an occasion no beat answers
                    // and no objective is judged at is raised to nobody.
                    if !p.occasions.is_empty()
                        || answered.contains(occasion.as_str())
                        || pick.is_some()
                    {
                        plan_occasion(p, &answered, n, occasion, None, pick)?;
                    }
                }
                Action::Advance {
                    by,
                    writes,
                    raise,
                    pick: pick.clone(),
                    choose: choose.clone(),
                }
            }
        };
        plan.push(Step {
            n,
            label: step.label.clone(),
            repeat: step.repeat,
            action,
            bridges: resolve_bridges(p, &format!("step {n}"), &step.bridges)?,
        });
    }
    Ok(plan)
}

/// dsl 0.26.0 §7 (T3-10): a `pick:` naming an entry as `<document id>.<entry
/// id>` picks the entry.
fn entry_pick(p: &ExecProject, pick: &Option<Pick>) -> Option<Pick> {
    pick.as_ref().map(|pk| match pk {
        Pick::Beat(id) => Pick::Beat(p.entry_id(id).to_string()),
        Pick::Pass => Pick::Pass,
    })
}

/// An `advance:` against the declared clock (dsl 0.26.0 §7, T2-5): a `to:`
/// slot is one of `slots`, a `to:` weekday a `week.labels` label or a
/// `clock.weekday` number of the declared `week:`.
fn resolve_advance(
    clock: &lute_manifest::clock::ClockDecl,
    n: usize,
    by: &AdvanceBy,
) -> Result<lute_manifest::clock::Advance, String> {
    let (weekday, slot) = match by {
        AdvanceBy::By(by) => return Ok(*by),
        AdvanceBy::To { weekday, slot } => (weekday, slot),
    };
    let slot = match slot {
        None => None,
        Some(s) if clock.slot.is_none() => {
            return Err(format!(
                "step {n}: `advance: {{ to: … slot: {s} }}` — the clock is day-granular (it \
                 declares no `slots:`)"
            ))
        }
        Some(s) => Some(clock.slot_index(s).ok_or_else(|| {
            format!(
                "step {n}: `advance:` to slot `{s}` — the clock's slots are: {}",
                clock.slots.join(", ")
            )
        })?),
    };
    let weekday = match weekday {
        None => None,
        Some(wd) => {
            let Some(week) = clock.week.as_ref() else {
                return Err(format!(
                    "step {n}: `advance:` to weekday `{wd}` — the clock declares no `week:`"
                ));
            };
            let index = match week.labels.iter().position(|l| l == wd) {
                Some(i) => Some(i as i64),
                None => wd
                    .parse::<i64>()
                    .ok()
                    .filter(|i| (0..i64::from(week.length)).contains(i)),
            };
            Some(index.ok_or_else(|| {
                let labels = if week.labels.is_empty() {
                    String::new()
                } else {
                    format!(" or one of: {}", week.labels.join(", "))
                };
                format!(
                    "step {n}: `advance:` to weekday `{wd}` — a weekday is a number 0..{}{labels}",
                    week.length.saturating_sub(1)
                )
            })?)
        }
    };
    Ok(lute_manifest::clock::Advance::To { weekday, slot })
}

/// [`plan_steps`] for one occasion step.
fn plan_occasion(
    p: &ExecProject,
    answered: &BTreeSet<&str>,
    n: usize,
    occasion: &str,
    target: Option<&str>,
    pick: Option<&Pick>,
) -> Result<(), String> {
    let decl = p.occasions.get(occasion);
    if p.occasions.is_empty() {
        if !answered.contains(occasion) {
            return Err(format!(
                "step {n}: occasion `{occasion}` is answered by no beat and judges no \
                 objective in this project (no plugin declares occasions, so the beats' and \
                 objectives' `on` values are the vocabulary)"
            ));
        }
    } else if decl.is_none() {
        let declared: Vec<&str> = p.occasions.keys().map(String::as_str).collect();
        let hint = if p.world_events.contains(occasion) {
            format!(" — `{occasion}` is a world event; fire it with `event: {occasion}`")
        } else {
            String::new()
        };
        return Err(format!(
            "step {n}: occasion `{occasion}` is declared by no resolved plugin (declared: {}){hint}",
            declared.join(", ")
        ));
    }
    match (target, decl) {
        (Some(t), Some(d)) if !d.target.takes_target() => {
            return Err(format!(
                "step {n}: occasion `{occasion}` is not declared `target: true`, so it cannot \
                 be raised for `{t}`"
            ));
        }
        (None, Some(d)) if d.target.takes_target() => {
            return Err(format!(
                "step {n}: occasion `{occasion}` is declared `target: true` — name what it is \
                 raised for with `target:`"
            ));
        }
        (Some(t), Some(d)) => {
            lute_check::occasion_target_ok(d, t, &p.kinds).map_err(|e| format!("step {n}: {e}"))?;
        }
        _ => {}
    }
    let select = p.select_of(occasion);
    match (select, pick) {
        // dsl 0.24.0 (T3-10): no `pick:` is `pick: none` when the list is
        // empty; a non-empty list without one halts at the step (the offered
        // list is only known when the occasion is raised).
        (OccasionSelect::All, None) => Ok(()),
        (OccasionSelect::First | OccasionSelect::Sequence, Some(pk)) => Err(format!(
            "step {n}: `pick: {}` applies only to a `select: all` occasion; \
             `{occasion}` is `select: {}`",
            match pk {
                Pick::Beat(id) => id.as_str(),
                Pick::Pass => "none",
            },
            select.as_str()
        )),
        (OccasionSelect::All, Some(Pick::Beat(pk))) => {
            if p.index
                .beats
                .iter()
                .any(|b| &b.id == pk && is_candidate(b, occasion, target))
            {
                Ok(())
            } else {
                Err(format!(
                    "step {n}: `pick: {pk}` names no beat answering `{occasion}`{}",
                    target.map(|t| format!(" for `{t}`")).unwrap_or_default()
                ))
            }
        }
        (OccasionSelect::All, Some(Pick::Pass))
        | (OccasionSelect::First | OccasionSelect::Sequence, None) => Ok(()),
    }
}

struct StepRecord {
    n: usize,
    label: Option<String>,
    /// `(k, of)` for the k-th run of a `repeat: of` step (of > 1).
    iteration: Option<(usize, usize)>,
    body: StepBody,
    quests: Vec<QuestAdvance>,
    /// The world right after the step settled — captured only when the
    /// step's `expect:` judges it (0.23.1).
    world: Option<WorldView>,
    /// Usage notes on the step as written — e.g. an `occasion:` step raising
    /// the `dayEnd` / `dayStart` its clock's `advance:` already raises
    /// ([`clock_raised_note`]).
    notes: Vec<String>,
    /// dsl 0.25.0 §1: exclusive relations that both hold after the step
    /// (`seenAfter(elias) and fell(elias) both hold`) — each fails the play.
    exclusive: Vec<String>,
}

/// The whole playthrough: the initial quest settle, then every step, and
/// the world it ended in.
struct Playthrough {
    start: Vec<QuestAdvance>,
    steps: Vec<StepRecord>,
    /// The steps an `end: true` step left unplayed: `(n, label)`.
    skipped: Vec<(usize, Option<String>)>,
    outcome: Result<String, PlayHalt>,
    world: World,
}

fn execute(script: &PlayScript, plan: &[Step], mut s: Session<'_>) -> Playthrough {
    // dsl 0.25.0 §1 (LH N16): the script's seeded world (`state:` /
    // `facts:`, the project's seeds, and what the rules derive) must not
    // already hold exclusive relations together.
    let seeded = s.exclusive();
    if !seeded.is_empty() {
        return Playthrough {
            start: Vec::new(),
            steps: Vec::new(),
            skipped: Vec::new(),
            outcome: Err(PlayHalt::Error(format!(
                "the script's seeded world holds exclusive relations together before step 1 — \
                 {} (dsl 0.25.0 §1); fix the script's `facts:` (or the `excludes:` declaration)",
                seeded.join("; ")
            ))),
            world: s.world,
        };
    }
    let (start, halt) = s.settle();
    let mut steps = Vec::new();
    let finish = |start, steps, h: PlayHalt, world| Playthrough {
        start,
        steps,
        skipped: Vec::new(),
        outcome: Err(h),
        world,
    };
    if let Some(h) = halt {
        return finish(start, steps, h, s.world);
    }
    for (i, step) in plan.iter().enumerate() {
        if matches!(step.action, Action::End) {
            steps.push(StepRecord {
                n: step.n,
                label: step.label.clone(),
                iteration: None,
                body: StepBody::End,
                quests: Vec::new(),
                world: None,
                notes: Vec::new(),
                exclusive: Vec::new(),
            });
            let skipped: Vec<(usize, Option<String>)> = plan[i + 1..]
                .iter()
                .map(|s| (s.n, s.label.clone()))
                .collect();
            let reason = match skipped.len() {
                0 => format!("`end: true` at step {}", step.n),
                k => format!(
                    "`end: true` at step {} ({k} later step{} skipped)",
                    step.n,
                    if k == 1 { "" } else { "s" }
                ),
            };
            return Playthrough {
                start,
                steps,
                skipped,
                outcome: Ok(reason),
                world: s.world,
            };
        }
        let wants = script
            .step_expects
            .iter()
            .find(|(i, _, _)| *i == step.n)
            .and_then(|(_, _, e)| crate::play_expect::wants_world(e));
        for k in 1..=step.repeat {
            // dsl 0.24.0 §5: the step's own answers ride before the top
            // level's for exactly this run of the step.
            s.world.bridges.step = step.bridges.clone();
            let (body, quests, halt) = run_step(&mut s, step);
            let leftover = std::mem::take(&mut s.world.bridges.step);
            let halt = halt.or_else(|| unconsumed_step_bridges(step.n, &leftover));
            // A halted step (a write-time exclusive refusal included, whose
            // `✗ exclusive` already sits in the transcript) is not checked again.
            let exclusive = if halt.is_none() {
                s.exclusive()
            } else {
                Vec::new()
            };
            let halt = halt.or_else(|| {
                (!exclusive.is_empty()).then(|| {
                    PlayHalt::Error(format!(
                        "step {}: exclusive relations hold together — {} (dsl 0.25.0 §1)",
                        step.n,
                        exclusive.join("; ")
                    ))
                })
            });
            steps.push(StepRecord {
                n: step.n,
                label: step.label.clone(),
                iteration: (step.repeat > 1).then_some((k, step.repeat)),
                body,
                quests,
                world: wants.map(|wants| s.view(wants.facts)),
                notes: clock_raised_note(&s, &step.action, &plan[i + 1..])
                    .into_iter()
                    .collect(),
                exclusive,
            });
            if let Some(h) = halt {
                return finish(start, steps, h, s.world);
            }
        }
    }
    let n = steps.len();
    Playthrough {
        start,
        steps,
        skipped: Vec::new(),
        outcome: Ok(format!(
            "complete ({n} step{})",
            if n == 1 { "" } else { "s" }
        )),
        world: s.world,
    }
}

/// Summer R1: an `occasion:` step raising the `dayEnd` / `dayStart` the
/// clock's `raise:` map declares — the next `advance:` crossing that
/// midnight raises it again, so its content runs twice for one day. A note,
/// not an error: a script may mean it. Ember F7 (round-5 T3-24): only when
/// such an `advance:` comes — some `later` step, before a `newRun` / `end`,
/// moves the clock (from where it stands now, each advance from where the
/// one before it left it) across a midnight while raising that moment.
fn clock_raised_note(s: &Session<'_>, action: &Action, later: &[Step]) -> Option<String> {
    let Action::Occasion { occasion, .. } = action else {
        return None;
    };
    let clock = s.project().index.clock.as_ref()?;
    let moments = clock.raise.as_ref()?.moments();
    let (moment, when) = if moments.day_end.as_ref() == Some(occasion) {
        (
            "dayEnd",
            "at each midnight it crosses, before the clock leaves the day",
        )
    } else if moments.day_start.as_ref() == Some(occasion) {
        ("dayStart", "on each day it enters")
    } else {
        return None;
    };
    let mut at = s.clock_at()?;
    let passed = later
        .iter()
        .take_while(|s| !matches!(s.action, Action::NewRun(_) | Action::End))
        .any(|s| {
            let Action::Advance { by, raise, .. } = &s.action else {
                return false;
            };
            let raises = match moment {
                "dayEnd" => raise.day_end.as_ref(),
                _ => raise.day_start.as_ref(),
            } == Some(occasion);
            (0..s.repeat).any(|_| {
                let to = clock.advance(at, *by);
                let crossed = to.day > at.day;
                at = to;
                raises && crossed
            })
        });
    passed.then(|| {
        format!(
            "`{occasion}` is the clock's `raise: {{ {moment}: {occasion} }}` — an `advance:` \
             raises it {when}; this step raises it again, so the same day's `{occasion}` runs \
             twice once an `advance:` passes it (drop the step and let `advance:` raise it; dsl \
             0.24.0 §1)"
        )
    })
}

/// dsl 0.24.0 §5: answers a step's own `bridges:` gave that no plugin call
/// of the step consumed fail the step (exit 1), named — an answer the script
/// expected to decide something decided nothing.
fn unconsumed_step_bridges(
    n: usize,
    leftover: &BTreeMap<String, VecDeque<lute_trace::BridgeAnswer>>,
) -> Option<PlayHalt> {
    if leftover.is_empty() {
        return None;
    }
    let named: Vec<String> = leftover
        .iter()
        .map(|(tag, q)| {
            let answers: Vec<String> = q
                .iter()
                .map(|a| {
                    let fields: Vec<String> = a.iter().map(|(f, v)| format!("{f}: {v}")).collect();
                    format!("{{{}}}", fields.join(", "))
                })
                .collect();
            format!("`{tag}` {}", answers.join(" "))
        })
        .collect();
    Some(PlayHalt::Error(format!(
        "step {n}: its `bridges:` answers were not all consumed — no plugin call of the step \
         took {}",
        named.join("; ")
    )))
}

/// Run one step (one repetition): its body record, the quest advances it
/// made, and what ends the playthrough, if anything. Every step that
/// changes the world settles the quest lifecycle after it — a presentation,
/// an `engine:` write, a new run (dsl 0.22.0 §1.1) — and a raised occasion
/// or event is then answered by the quests.
fn run_step(s: &mut Session<'_>, step: &Step) -> lute_trace::exec::session::StepOutcome {
    let n = step.n;
    match &step.action {
        Action::Occasion {
            occasion,
            target,
            pick,
            choose,
        } => s.occasion(n, occasion, target, pick, choose),
        Action::NewRun(seed) => s.new_run(n, seed),
        Action::Engine(writes) => s.engine(n, writes),
        Action::Event(event) => s.event(event),
        Action::Advance {
            by,
            writes,
            raise,
            pick,
            choose,
        } => s.advance(n, *by, writes, raise, pick, choose),
        Action::End => unreachable!("`execute` ends the playthrough at an `end` step"),
    }
}

// ===========================================================================
// Rendering. Human by default, `--json` one object; both reuse the runner's
// own transcript records rather than re-deriving per-kind semantics.
// ===========================================================================

/// A menu at its presentation point: the chosen option bracketed, a spent
/// `once` option `(spent)`, an option whose guard decided false `✗`.
fn render_options(opts: &[Json], rec: &Json) -> String {
    let chosen = rec.get("chose").and_then(Json::as_str);
    let listed = |key: &str, id: &str| {
        rec.get(key)
            .and_then(Json::as_array)
            .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(id)))
    };
    opts.iter()
        .filter_map(|o| o.get("id").and_then(Json::as_str))
        .map(|id| {
            if Some(id) == chosen {
                format!("[{id}]")
            } else if listed("spent", id) {
                format!("{id}(spent)")
            } else if listed("ineligible", id) {
                format!("{id}✗")
            } else {
                id.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn str_of<'a>(rec: &'a Json, key: &str) -> &'a str {
    rec.get(key).and_then(Json::as_str).unwrap_or("")
}

/// One document's commands, by address and in stream order, and how its
/// staging prints: as authored (default) or as the lowered IR (`--ir`).
struct DocCmds<'a> {
    list: &'a [Json],
    at: BTreeMap<&'a str, usize>,
    /// addr -> the directive as authored ([`ExecProject::authored`]).
    authored: Option<&'a BTreeMap<String, String>>,
    /// `--ir`: print lowered records, injected ones included.
    ir: bool,
}

impl<'a> DocCmds<'a> {
    fn new(p: &'a ExecProject, document: &str, ir: bool) -> Self {
        let list = p
            .artifacts
            .get(document)
            .and_then(|d| d.get("commands"))
            .and_then(Json::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let at = list
            .iter()
            .enumerate()
            .filter_map(|(i, c)| c.get("addr").and_then(Json::as_str).map(|a| (a, i)))
            .collect();
        DocCmds {
            list,
            at,
            authored: p.authored.get(document),
            ir,
        }
    }

    fn get(&self, addr: &str) -> Option<&'a Json> {
        self.at.get(addr).map(|&i| &self.list[i])
    }

    /// The authored commands from `addr` on, in stream order — the
    /// compiler's injected staging bookkeeping skipped.
    fn authored_from(&self, addr: &str) -> impl Iterator<Item = &'a Json> {
        let list = self.list;
        let start = self.at.get(addr).copied().unwrap_or(list.len());
        list[start..].iter().filter(|c| !is_injected(c))
    }
}

/// A command the compiler injected (`provenance.injected`: a preload
/// lookahead, a pose reset, a `::bg` auto-hide) rather than one authored.
fn is_injected(cmd: &Json) -> bool {
    cmd.get("provenance")
        .and_then(|p| p.get("injected"))
        .and_then(Json::as_bool)
        == Some(true)
}

/// What a `when=` guard wraps: the one-arm match whose `$` test is the guard
/// itself and whose `otherwise` is empty (dsl §7.2/§7.4 line desugar, dsl
/// 0.24.0 §1 set desugar, dsl 0.26.0 §4 directive desugar). The transcript
/// shows such a match as what it guards or its skip — the synthetic match
/// is compiler plumbing.
enum Guarded<'a> {
    /// One record: a line, `::set`, `::assert`, `::retract`, `::accept` or
    /// a plugin call.
    Leaf(&'a Json),
    /// A guarded `::use`'s expansion, by the use as authored.
    Use(&'a str),
}

fn guarded<'a>(m: &Json, cmds: &DocCmds<'a>) -> Option<Guarded<'a>> {
    let [arm] = m.get("arms")?.as_array()?.as_slice() else {
        return None;
    };
    let subject = str_of(m, "subject").trim();
    let test = str_of(arm, "test").trim();
    if subject.is_empty() || (test != subject && test != format!("({subject})")) {
        return None;
    }
    let converge = str_of(m, "converge");
    let jumps_to_converge = |c: Option<&Json>| {
        c.is_some_and(|c| str_of(c, "kind") == "jump" && str_of(c, "target") == converge)
    };
    if !jumps_to_converge(cmds.authored_from(str_of(m, "otherwise")).next()) {
        return None;
    }
    if let Some(text) = cmds.authored.and_then(|a| a.get(str_of(m, "addr"))) {
        return Some(Guarded::Use(text));
    }
    let mut body = cmds.authored_from(str_of(arm, "target"));
    let leaf = body.next().filter(|c| {
        matches!(
            str_of(c, "kind"),
            "line" | "set" | "assert" | "retract" | "accept" | "plugin"
        )
    })?;
    jumps_to_converge(body.next()).then_some(Guarded::Leaf(leaf))
}

/// A skipped guarded record or `::use`, as the transcript names it.
fn skipped(g: Guarded<'_>, cmds: &DocCmds<'_>) -> String {
    let leaf = match g {
        Guarded::Use(text) => return text.to_string(),
        Guarded::Leaf(leaf) => leaf,
    };
    match str_of(leaf, "kind") {
        "set" => format!(
            "set {} {} {}",
            str_of(leaf, "path"),
            str_of(leaf, "op"),
            str_of(leaf, "value")
        ),
        "line" => format!(
            "{} \"{}\"",
            line_head(str_of(leaf, "speaker"), Some(leaf)),
            str_of(leaf, "text")
        ),
        kind @ ("assert" | "retract") => {
            let args: Vec<String> = leaf
                .get("args")
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
                .map(|a| a.as_str().map_or_else(|| a.to_string(), str::to_string))
                .collect();
            format!("{kind} {}({})", str_of(leaf, "relation"), args.join(", "))
        }
        "accept" => format!("::accept{{quest=\"{}\"}}", str_of(leaf, "quest")),
        kind => cmds
            .authored
            .and_then(|a| a.get(str_of(leaf, "addr")))
            .cloned()
            .unwrap_or_else(|| lowered(kind, Some(leaf))),
    }
}

/// A lowered record as `::<kind>{attrs}` — the `--ir` view (an injected one
/// names what injected it).
fn lowered(kind: &str, orig: Option<&Json>) -> String {
    let attrs = orig
        .map(|c| render_attrs(c, &["addr", "kind"]))
        .unwrap_or_default();
    let injected = orig
        .filter(|c| is_injected(c))
        .and_then(|c| c.get("provenance"))
        .map(|p| format!("        (injected: {})", str_of(p, "by")))
        .unwrap_or_default();
    if attrs.is_empty() {
        format!("::{kind}{injected}")
    } else {
        format!("::{kind}{{{attrs}}}{injected}")
    }
}

/// One runner transcript record -> a human line at source level, enriched
/// with the original command's authored attrs (looked up by `addr`).
/// Staging prints as authored (`::bg{…}`, not the lowered `::background`,
/// 0.23.1) unless `--ir`. `None` for a record that is not authored source:
/// a staging record the compiler injected (`provenance.injected`) — save the
/// first exit of a `::clear`, which carries `::clear` as authored (dsl
/// 0.24.0 §4) —, a
/// bundle `beat` record (the `→` line names the beat), or the synthetic
/// match of a line guard whose line played. `--json` keeps every record
/// verbatim.
fn render_record(rec: &Json, cmds: &DocCmds<'_>) -> Option<String> {
    let kind = str_of(rec, "kind");
    let orig = cmds.get(str_of(rec, "addr"));
    let authored = || {
        cmds.authored
            .filter(|_| !cmds.ir)
            .and_then(|a| a.get(str_of(rec, "addr")))
            .cloned()
    };
    Some(match kind {
        "line" => lute_trace::exec::said_line(rec, orig),
        "background" | "music" | "sfx" | "vfx" | "sprite" | "camera" | "cut" | "video" => {
            if !cmds.ir && orig.is_some_and(is_injected) {
                return authored();
            }
            authored().unwrap_or_else(|| lowered(kind, orig))
        }
        // T1-3: a directive's declared effect names the call it came from.
        "set" => {
            let effect = rec
                .get("effectOf")
                .and_then(Json::as_str)
                .map(|tag| format!("  (effect of ::{tag})"))
                .unwrap_or_default();
            format!(
                "  set {} = {}{effect}",
                str_of(rec, "path"),
                rec.get("value").map(Json::to_string).unwrap_or_default()
            )
        }
        "assert" => format!("  assert {}", str_of(rec, "fact")),
        "retract" => format!("  retract {}", str_of(rec, "pattern")),
        "choice" | "hub" => {
            let id = str_of(rec, if kind == "choice" { "branch" } else { "hub" });
            let chosen = rec.get("chose").and_then(Json::as_str);
            let opts: Vec<Json> = orig
                .and_then(|c| c.get("options"))
                .and_then(Json::as_array)
                .cloned()
                .unwrap_or_default();
            let mut label = format!("{kind} {id}");
            if let Some(prompt) = orig.and_then(|c| c.get("prompt")).and_then(Json::as_str) {
                label.push_str(&format!(" \"{prompt}\""));
            }
            if let Some(t) = orig
                .and_then(|c| c.get("timeoutSec"))
                .and_then(Json::as_u64)
            {
                label.push_str(&format!(" ({t}s)"));
            }
            let rendered = render_options(&opts, rec);
            match chosen {
                Some(c) => format!("▷ {label}: {rendered}        ← chosen: {c}"),
                None => format!("▷ {label}: {rendered}        ← INCOMPLETE (no decision)"),
            }
        }
        "match" => match orig.and_then(|m| guarded(m, cmds)) {
            // The guard held: what it guards follows as the output.
            Some(_) if str_of(rec, "result") == "arm 1" => return None,
            Some(g) => format!("  skip {} — when: false", skipped(g, cmds)),
            None => format!("  match -> {}", str_of(rec, "result")),
        },
        "barrier" => "  barrier (no real clock simulated)".to_string(),
        // 0.23.1: `::end` ends the presentation (or quest advance) it ran
        // in; the playthrough goes on with the next step.
        "end" => {
            let text = authored().unwrap_or_else(|| lowered("end", orig));
            format!("{text}        (this presentation ends; the play goes on)")
        }
        "beat" if !cmds.ir => return None,
        "plugin" => {
            // dsl 0.24.0 §5: the call's bridge answer, when it read one.
            let note = crate::runner::plugin_call_note(rec);
            let bridged = note.starts_with("(bridge");
            match authored() {
                // T1-3: a call with only declared effects — its `set`
                // records follow, each naming it.
                Some(text) if note.is_empty() => text,
                Some(text) if bridged => format!("{text}        {note}"),
                Some(text) => format!("{text}        (plugin call, not invoked)"),
                None if note.is_empty() => format!("  plugin {}", str_of(rec, "tag")),
                None => format!("  plugin {} {note}", str_of(rec, "tag")),
            }
        }
        "entry" => {
            let read = if rec.get("firstRead").and_then(Json::as_bool) == Some(true) {
                "first read"
            } else {
                "re-read: effects skipped"
            };
            format!("  entry {} ({read})", str_of(rec, "id"))
        }
        "skipped" => {
            let what = ["path", "fact", "pattern"]
                .iter()
                .find_map(|k| rec.get(*k).and_then(Json::as_str))
                .unwrap_or("");
            format!("  {} {what} (skipped: re-read)", str_of(rec, "effect"))
        }
        // dsl 0.25.0 §1: the write just before made exclusive relations hold.
        "exclusive" => format!("  ✗ exclusive: {}", str_of(rec, "text")),
        // dsl 0.24.0 (T3-11): a handler whose quest already settled.
        "handlerSkipped" => format!(
            "  <on event={}> of quest {} skipped — quest {}",
            str_of(rec, "event"),
            str_of(rec, "quest"),
            str_of(rec, "status")
        ),
        // dsl 0.24.0 §2 (ER N15): an accept the child's inactive parent spent.
        "acceptSpent" => format!(
            "  note: accept of quest {} spent — its parent quest {} is {}; an \
             `activate=\"accept\"` child activates only while its parent is active (dsl 0.24.0 §2)",
            str_of(rec, "quest"),
            str_of(rec, "parent"),
            match str_of(rec, "parentStatus") {
                "unset" => "not active yet",
                s => s,
            }
        ),
        "accept" => {
            let ignored = rec
                .get("ignored")
                .and_then(Json::as_str)
                .map(|s| format!(" ({s} — ignored)"))
                .unwrap_or_default();
            // dsl 0.24.0 §2: a queued accept applies at the next run start.
            let queued = if str_of(rec, "at") == "nextRun" {
                " (queued: applies after the next newRun)"
            } else {
                ""
            };
            let engine = if str_of(rec, "by") == "engine" {
                " (engine)"
            } else {
                ""
            };
            format!(
                "  quest {} accepted{engine}{ignored}{queued}",
                str_of(rec, "quest")
            )
        }
        // dsl 0.23.0 §2 / 0.24.0 §2.1: the objective's `by` (or `until`)
        // came true first.
        "objective" => match rec.get("failedBy").and_then(Json::as_str) {
            Some(by) if rec.get("failed").and_then(Json::as_bool) == Some(true) => format!(
                "  {}.{} failed ({by})",
                str_of(rec, "quest"),
                str_of(rec, "objective")
            ),
            _ => format!(
                "  {}.{} done",
                str_of(rec, "quest"),
                str_of(rec, "objective")
            ),
        },
        // dsl 0.24.0 §2: a failure names its reason (`failedBy`).
        "quest" => match rec.get("failedBy").and_then(Json::as_str) {
            Some(by) => format!(
                "  quest {} -> {} ({by})",
                str_of(rec, "quest"),
                str_of(rec, "state")
            ),
            None => format!(
                "  quest {} -> {}",
                str_of(rec, "quest"),
                str_of(rec, "state")
            ),
        },
        "grant" => {
            let owner = match rec.get("objective").and_then(Json::as_str) {
                Some(oid) => format!("{}.{oid}", str_of(rec, "quest")),
                None => str_of(rec, "quest").to_string(),
            };
            let reward = rec.get("reward").cloned().unwrap_or(Json::Null);
            let amount = match reward.get("amount").and_then(Json::as_i64) {
                Some(n) => n.to_string(),
                None => match (
                    reward.get("amountMin").and_then(Json::as_i64),
                    reward.get("amountMax").and_then(Json::as_i64),
                ) {
                    (Some(lo), Some(hi)) => format!("{lo}..{hi}"),
                    _ => "?".to_string(),
                },
            };
            let target = reward
                .get("target")
                .and_then(Json::as_str)
                .map(|t| format!(" -> {t}"))
                .unwrap_or_default();
            let on_failed = if rec.get("onFailed").and_then(Json::as_bool) == Some(true) {
                " (on failed)"
            } else {
                ""
            };
            // dsl 0.23.0 §8: where the grant was credited, and the new value.
            let credited = match rec.get("credited") {
                Some(c) => format!(
                    " (credits {} = {})",
                    str_of(c, "path"),
                    c.get("value").map(Json::to_string).unwrap_or_default()
                ),
                None => String::new(),
            };
            format!(
                "  grant {owner} {} {amount}{target}{on_failed}{credited}",
                str_of(&reward, "kind")
            )
        }
        _ => format!("  {kind}"),
    })
}

fn render_records(out: &mut String, p: &ExecProject, ir: bool, document: &str, records: &[Json]) {
    let cmds = DocCmds::new(p, document, ir);
    for line in records.iter().filter_map(|rec| render_record(rec, &cmds)) {
        out.push_str(&line);
        out.push('\n');
    }
}

const RULE: &str = "──────────────";

fn render_candidate(c: &Candidate) -> String {
    let read = if c.read { ", read" } else { "" };
    let also = if c.also { ", also" } else { "" };
    let head = format!(
        "{} [{}, priority {}{read}{also}]",
        c.id,
        kind_label(c.kind),
        c.priority
    );
    match &c.verdict {
        Verdict::Eligible => format!("  ✓ {head}\n"),
        Verdict::Ineligible(reason) => format!("  ✗ {head} — {reason}\n"),
        Verdict::Unknown(detail) => format!("  ? {head} — when: unknown ({detail})\n"),
    }
}

/// `pick:` as written.
fn pick_label(pick: &Pick) -> &str {
    match pick {
        Pick::Beat(id) => id,
        Pick::Pass => "none",
    }
}

/// An `engine:` / `newRun` seed write record, one human line.
fn render_write(rec: &Json) -> String {
    match str_of(rec, "kind") {
        "set" => format!(
            "  set {} = {}",
            str_of(rec, "path"),
            rec.get("value").map(Json::to_string).unwrap_or_default()
        ),
        "assert" => format!("  assert {}", str_of(rec, "fact")),
        // dsl 0.26.0 §7 (T2-9).
        "accept" => format!("  quest {} accepted (engine)", str_of(rec, "quest")),
        "acceptIgnored" => format!(
            "  note: quest {} is already {} — engine accept ignored",
            str_of(rec, "quest"),
            str_of(rec, "status")
        ),
        _ if rec.get("held").and_then(Json::as_bool) == Some(false) => {
            format!("  retract {} (did not hold)", str_of(rec, "pattern"))
        }
        _ => format!("  retract {}", str_of(rec, "pattern")),
    }
}

fn render_human(p: &ExecProject, play: &Playthrough, ir: bool) -> String {
    let mut out = String::new();
    if !play.start.is_empty() {
        out.push_str(&format!("── start {RULE}\n"));
        for q in &play.start {
            render_records(&mut out, p, ir, &q.document, &q.transcript);
        }
    }
    for s in &play.steps {
        let mut head = format!("── step {}", s.n);
        if let Some(label) = &s.label {
            head.push_str(&format!(" ({label})"));
        }
        if let Some((k, of)) = s.iteration {
            head.push_str(&format!(" [{k}/{of}]"));
        }
        match &s.body {
            StepBody::NewRun {
                writes,
                reset_quests,
                prev_run,
                unjudged,
                accepted,
            } => {
                out.push_str(&format!("{head} · new run {RULE}\n"));
                out.push_str("  run.* state, run-tier facts and once: run reset");
                if !prev_run.is_empty() {
                    out.push_str(&format!(
                        "; prev.run.* holds the ended run ({} value{})",
                        prev_run.len(),
                        if prev_run.len() == 1 { "" } else { "s" }
                    ));
                }
                out.push('\n');
                for (path, v) in prev_run {
                    out.push_str(&format!("  {path} = {}\n", value_to_json(v)));
                }
                for (id, was) in reset_quests {
                    out.push_str(&format!("  quest {id} -> unset (tier: run; was {was})\n"));
                }
                for id in unjudged {
                    out.push_str(&format!(
                        "  note: quest {id} was accepted this run and is still active with no \
                         objective done or failed — the reset discards it; a run-tier quest taken \
                         between runs is `::accept{{quest=\"{id}\" at=\"nextRun\"}}` (dsl 0.24.0 §2)\n"
                    ));
                }
                for id in accepted {
                    out.push_str(&format!("  quest {id} accepted (queued at=\"nextRun\")\n"));
                }
                for rec in writes {
                    out.push_str(&render_write(rec));
                    out.push('\n');
                }
            }
            StepBody::Engine { writes } => {
                out.push_str(&format!("{head} · engine {RULE}\n"));
                for rec in writes {
                    out.push_str(&render_write(rec));
                    out.push('\n');
                }
            }
            StepBody::Event { event } => {
                out.push_str(&format!("{head} · event {event} {RULE}\n"));
            }
            StepBody::End => {
                out.push_str(&format!("{head} · end (the playthrough ends) {RULE}\n"));
            }
            StepBody::Advance {
                by,
                from,
                to,
                writes,
                settled,
                days,
                raised,
            } => {
                out.push_str(&format!("{head} · advance {by}: {from} → {to} {RULE}\n"));
                for d in days {
                    for rec in &d.writes {
                        out.push_str(&render_write(rec));
                        out.push('\n');
                    }
                    for q in &d.settled {
                        render_records(&mut out, p, ir, &q.document, &q.transcript);
                    }
                    render_occasion_human(
                        &mut out,
                        p,
                        ir,
                        &format!("{head} · {}", d.at),
                        &d.occasion,
                    );
                    for q in &d.quests {
                        render_records(&mut out, p, ir, &q.document, &q.transcript);
                    }
                }
                if !days.is_empty() && !(writes.is_empty() && settled.is_empty()) {
                    out.push_str(&format!("{head} · {to} {RULE}\n"));
                }
                for rec in writes {
                    out.push_str(&render_write(rec));
                    out.push('\n');
                }
                for q in settled {
                    render_records(&mut out, p, ir, &q.document, &q.transcript);
                }
                if let Some(body) = raised {
                    render_occasion_human(&mut out, p, ir, &head, body);
                }
            }
            body @ StepBody::Occasion { .. } => render_occasion_human(&mut out, p, ir, &head, body),
        }
        for note in &s.notes {
            out.push_str(&format!("  note: {note}\n"));
        }
        for v in &s.exclusive {
            out.push_str(&format!("  ✗ exclusive: {v}\n"));
        }
        for q in &s.quests {
            render_records(&mut out, p, ir, &q.document, &q.transcript);
        }
    }
    for (n, label) in &play.skipped {
        let label = label
            .as_ref()
            .map(|l| format!(" ({l})"))
            .unwrap_or_default();
        out.push_str(&format!(
            "── step {n}{label} · skipped (the playthrough ended) {RULE}\n"
        ));
    }
    match &play.outcome {
        Ok(reason) => out.push_str(&format!("── end: {reason} {RULE}\n")),
        Err(h) => out.push_str(&format!("── halted: {} {RULE}\n", h.message())),
    }
    out
}

/// An occasion step's body (or the occasion an `advance:` raised) as human
/// lines under `head`: the header, the quest advances a `judge: before`
/// raise made, every candidate with its verdict, what was presented, then
/// each presentation's transcript.
fn render_occasion_human(out: &mut String, p: &ExecProject, ir: bool, head: &str, body: &StepBody) {
    let StepBody::Occasion {
        occasion,
        target,
        select,
        pick,
        candidates,
        winner,
        decided,
        presented,
        judged,
    } = body
    else {
        return;
    };
    let mut header = format!("{head} · {occasion}");
    if let Some(t) = target {
        header.push_str(&format!(" → {t}"));
    }
    match select {
        OccasionSelect::All => header.push_str(&format!(
            " (select: all, pick: {})",
            match (pick, decided) {
                (Some(pk), _) => pick_label(pk),
                (None, true) => "none (nothing offered)",
                (None, false) => "?",
            }
        )),
        OccasionSelect::Sequence => header.push_str(" (select: sequence)"),
        OccasionSelect::First => {}
    }
    out.push_str(&format!("{header} {RULE}\n"));
    for q in judged {
        render_records(out, p, ir, &q.document, &q.transcript);
    }
    if candidates.is_empty() {
        out.push_str("  (no candidates)\n");
    }
    for c in candidates
        .iter()
        .filter(|c| matches!(c.verdict, Verdict::Eligible))
    {
        out.push_str(&render_candidate(c));
    }
    for c in candidates
        .iter()
        .filter(|c| !matches!(c.verdict, Verdict::Eligible))
    {
        out.push_str(&render_candidate(c));
    }
    let is_also = |id: &str| candidates.iter().any(|c| c.also && c.id == id);
    if *decided {
        match (select, winner, pick) {
            (OccasionSelect::Sequence, _, _) if !presented.is_empty() => {
                for pr in presented {
                    out.push_str(&format!("  → {}\n", pr.id));
                }
            }
            (_, Some(id), _) => out.push_str(&format!("  → {id}\n")),
            (_, None, Some(Pick::Pass)) => {
                out.push_str("  → (pick: none — the list closes; nothing presented)\n")
            }
            (_, None, _) if !presented.is_empty() => out.push_str("  → (no eligible main beat)\n"),
            (_, None, _) => out.push_str("  → (no eligible beat — the occasion passes)\n"),
        }
        if *select == OccasionSelect::First {
            for pr in presented.iter().filter(|pr| is_also(&pr.id)) {
                out.push_str(&format!("  + {} (also)\n", pr.id));
            }
        }
    }
    for pr in presented {
        render_records(out, p, ir, &pr.document, &pr.transcript);
    }
}

/// The play's presented content, one canonical `@speaker{delivery}: text`
/// line ([`lute_trace::exec::said_line`], the head the human transcript
/// prints) per content line that actually played, in order — what
/// `transcriptContains` / `transcriptLacks` match (dsl 0.24.0 T1-2, 0.27
/// T1-11). The canonical form `lute test` matches a scene test's walk
/// against too ([`lute_trace::TraceReport::said`]): no step headers,
/// candidates, `skip … — when: false` lines, staging, or notes.
fn said(p: &ExecProject, play: &Playthrough) -> String {
    let mut out = String::new();
    let mut push = |document: &str, records: &[Json]| {
        let cmds = DocCmds::new(p, document, false);
        for rec in records.iter().filter(|r| str_of(r, "kind") == "line") {
            out.push_str(&lute_trace::exec::said_line(
                rec,
                cmds.get(str_of(rec, "addr")),
            ));
            out.push('\n');
        }
    };
    for q in &play.start {
        push(&q.document, &q.transcript);
    }
    for s in &play.steps {
        for played in s.body.days_played() {
            match played {
                Played::Quest(q) => push(&q.document, &q.transcript),
                Played::Beat(pr) => push(&pr.document, &pr.transcript),
            }
        }
        for q in s.body.settled() {
            push(&q.document, &q.transcript);
        }
        if let Some(StepBody::Occasion { presented, .. }) = s.body.occasion() {
            for pr in presented {
                push(&pr.document, &pr.transcript);
            }
        }
        for q in &s.quests {
            push(&q.document, &q.transcript);
        }
    }
    out
}

fn state_delta(before: &BTreeMap<String, Value>, after: &BTreeMap<String, Value>) -> Json {
    let mut delta = serde_json::Map::new();
    for (k, v) in after {
        if before.get(k) != Some(v) {
            delta.insert(k.clone(), value_to_json(v));
        }
    }
    Json::Object(delta)
}

fn quests_json(quests: &[QuestAdvance]) -> Json {
    Json::Array(
        quests
            .iter()
            .map(|q| json!({ "document": q.document, "commands": q.transcript }))
            .collect(),
    )
}

/// dsl 0.24.0 T3-2: what last asserted each base fact of the playthrough —
/// ``entry `keeperLog2`, step 4``, `engine step 7`, the script's `facts:` —
/// for `--explain`'s asserted leaves. `initial` is the world's fact set
/// before the first step (seed facts are labelled by the caller).
fn fact_origins(play: &Playthrough, initial: &BTreeSet<Fact>) -> BTreeMap<Fact, String> {
    let mut out: BTreeMap<Fact, String> = initial
        .iter()
        .map(|f| (f.clone(), "the script's `facts:`".to_string()))
        .collect();
    let mut take = |records: &[Json], who: String| {
        for rec in records {
            if rec.get("kind").and_then(Json::as_str) == Some("assert") {
                if let Some(f) = rec
                    .get("fact")
                    .and_then(Json::as_str)
                    .and_then(parse_ground_fact)
                {
                    out.insert(f, who.clone());
                }
            }
        }
    };
    for q in &play.start {
        take(
            &q.transcript,
            format!("a quest handler in {}, before step 1", q.document),
        );
    }
    for s in &play.steps {
        let step = match s.iteration {
            Some((k, of)) => format!("step {} ({k}/{of})", s.n),
            None => format!("step {}", s.n),
        };
        match &s.body {
            StepBody::Engine { writes } => take(writes, format!("engine {step}")),
            StepBody::NewRun { writes, .. } => take(writes, format!("newRun {step}")),
            _ => {}
        }
        for played in s.body.days_played() {
            match played {
                Played::Quest(q) => take(
                    &q.transcript,
                    format!("a quest handler in {}, {step}", q.document),
                ),
                Played::Beat(p) => take(
                    &p.transcript,
                    format!("{} `{}`, {step}", kind_label(p.kind), p.id),
                ),
            }
        }
        for q in s.body.settled() {
            take(
                &q.transcript,
                format!("a quest handler in {}, {step}", q.document),
            );
        }
        if let Some(StepBody::Occasion { presented, .. }) = s.body.occasion() {
            for p in presented {
                take(
                    &p.transcript,
                    format!("{} `{}`, {step}", kind_label(p.kind), p.id),
                );
            }
        }
        for q in &s.quests {
            take(
                &q.transcript,
                format!("a quest handler in {}, {step}", q.document),
            );
        }
    }
    out
}

fn render_json(play: &Playthrough) -> Json {
    let steps: Vec<Json> = play
        .steps
        .iter()
        .map(|s| {
            let mut o = serde_json::Map::new();
            o.insert("step".into(), json!(s.n));
            if let Some(label) = &s.label {
                o.insert("label".into(), json!(label));
            }
            if let Some((k, of)) = s.iteration {
                o.insert("iteration".into(), json!(k));
                o.insert("repeat".into(), json!(of));
            }
            if !s.notes.is_empty() {
                o.insert("notes".into(), json!(s.notes));
            }
            if !s.exclusive.is_empty() {
                o.insert("exclusive".into(), json!(s.exclusive));
            }
            match &s.body {
                StepBody::NewRun {
                    writes,
                    reset_quests,
                    accepted,
                    prev_run,
                    unjudged,
                } => {
                    o.insert("newRun".into(), json!(true));
                    o.insert("seed".into(), json!(writes));
                    if !reset_quests.is_empty() {
                        let reset: serde_json::Map<String, Json> = reset_quests
                            .iter()
                            .map(|(id, was)| (id.clone(), json!(was)))
                            .collect();
                        o.insert("resetQuests".into(), Json::Object(reset));
                    }
                    if !accepted.is_empty() {
                        o.insert("accepted".into(), json!(accepted));
                    }
                    if !prev_run.is_empty() {
                        let prev: serde_json::Map<String, Json> = prev_run
                            .iter()
                            .map(|(path, v)| (path.clone(), value_to_json(v)))
                            .collect();
                        o.insert("prevRun".into(), Json::Object(prev));
                    }
                    if !unjudged.is_empty() {
                        o.insert("resetUnjudged".into(), json!(unjudged));
                    }
                }
                StepBody::Engine { writes } => {
                    o.insert("engine".into(), json!(writes));
                }
                StepBody::Event { event } => {
                    o.insert("event".into(), json!(event));
                }
                StepBody::End => {
                    o.insert("end".into(), json!(true));
                }
                // dsl 0.24.0 §1: the move and its settle under `advance`,
                // each midnight raise (`dayEnd` / `dayStart`) under
                // `advance.days` in occasion-step fields; the occasion it
                // raised where it stopped as an occasion step's own fields.
                StepBody::Advance {
                    by,
                    from,
                    to,
                    writes,
                    settled,
                    days,
                    raised,
                } => {
                    let days: Vec<Json> = days
                        .iter()
                        .map(|d| {
                            let mut m = serde_json::Map::new();
                            m.insert("at".into(), json!(d.at));
                            m.insert("writes".into(), json!(d.writes));
                            m.insert("settled".into(), quests_json(&d.settled));
                            render_occasion_json(&mut m, &d.occasion);
                            m.insert("quests".into(), quests_json(&d.quests));
                            Json::Object(m)
                        })
                        .collect();
                    let mut advance = json!({
                        "by": by,
                        "from": from,
                        "to": to,
                        "writes": writes,
                        "quests": quests_json(settled),
                    });
                    if !days.is_empty() {
                        advance["days"] = Json::Array(days);
                    }
                    o.insert("advance".into(), advance);
                    if let Some(body) = raised {
                        render_occasion_json(&mut o, body);
                    }
                }
                body @ StepBody::Occasion { .. } => render_occasion_json(&mut o, body),
            }
            o.insert("quests".into(), quests_json(&s.quests));
            Json::Object(o)
        })
        .collect();
    let mut root = serde_json::Map::new();
    match &play.outcome {
        Ok(reason) => {
            root.insert("exit".into(), json!("complete"));
            root.insert("endReason".into(), json!(reason));
        }
        Err(h) => {
            root.insert("exit".into(), json!(h.exit_label()));
            root.insert("error".into(), json!({ "message": h.message() }));
        }
    }
    root.insert(
        "start".into(),
        json!({ "quests": quests_json(&play.start) }),
    );
    if !play.skipped.is_empty() {
        root.insert(
            "skipped".into(),
            Json::Array(
                play.skipped
                    .iter()
                    .map(|(n, label)| match label {
                        Some(l) => json!({ "step": n, "label": l }),
                        None => json!({ "step": n }),
                    })
                    .collect(),
            ),
        );
    }
    root.insert("steps".into(), Json::Array(steps));
    Json::Object(root)
}

/// An occasion step's `--json` fields.
fn render_occasion_json(o: &mut serde_json::Map<String, Json>, body: &StepBody) {
    let StepBody::Occasion {
        occasion,
        target,
        select,
        pick,
        candidates,
        winner,
        decided: _,
        presented,
        judged,
    } = body
    else {
        return;
    };
    o.insert("occasion".into(), json!(occasion));
    if let Some(t) = target {
        o.insert("target".into(), json!(t));
    }
    o.insert("select".into(), json!(select.as_str()));
    if let Some(pk) = pick {
        o.insert("pick".into(), json!(pick_label(pk)));
    }
    let cands: Vec<Json> = candidates
        .iter()
        .map(|c| {
            let mut m = serde_json::Map::new();
            m.insert("id".into(), json!(c.id));
            m.insert("kind".into(), json!(kind_label(c.kind)));
            m.insert("document".into(), json!(c.document));
            m.insert("priority".into(), json!(c.priority));
            let (eligible, reason) = match &c.verdict {
                Verdict::Eligible => (json!(true), None),
                Verdict::Ineligible(r) => (json!(false), Some(r.clone())),
                Verdict::Unknown(d) => (Json::Null, Some(format!("when: unknown ({d})"))),
            };
            m.insert("eligible".into(), eligible);
            if c.read {
                m.insert("read".into(), json!(true));
            }
            if c.also {
                m.insert("also".into(), json!(true));
            }
            if let Some(r) = reason {
                m.insert("reason".into(), json!(r));
            }
            Json::Object(m)
        })
        .collect();
    // dsl 0.24.0 §2: a `judge: before` raise's quest advances, made before
    // the candidates were decided (the step's `quests` are the ones after).
    if !judged.is_empty() {
        o.insert("judgedBefore".into(), quests_json(judged));
    }
    o.insert("candidates".into(), Json::Array(cands));
    o.insert("winner".into(), json!(winner));
    // `presented` is the first presentation (the 0.22 shape); every later one
    // — a winner's `also` riders, the rest of a `select: sequence` — follows
    // in `then`, in order (dsl 0.23.0 §3).
    let pr_json = |pr: &Presented| {
        let mut m = json!({
            "id": pr.id,
            "kind": kind_label(pr.kind),
            "document": pr.document,
            "commands": pr.transcript,
            "stateDelta": state_delta(&pr.state_before, &pr.state_after),
        });
        if candidates.iter().any(|c| c.also && c.id == pr.id) {
            m["also"] = json!(true);
        }
        m
    };
    if let Some((first, rest)) = presented.split_first() {
        o.insert("presented".into(), pr_json(first));
        if !rest.is_empty() {
            o.insert(
                "then".into(),
                Json::Array(rest.iter().map(pr_json).collect()),
            );
        }
    }
}

// ===========================================================================
// CLI entry point.
// ===========================================================================

/// A script parsed, its project compiled, its steps planned and its save
/// seeded — everything before the walk.
struct Loaded {
    script: PlayScript,
    project: ExecProject,
    plan: Vec<Step>,
    world: World,
}

/// Load a play. `Err((code, message))`: exit 2 with the usage error, or the
/// compile gate's exit code (its diagnostics already printed, `message`
/// empty).
fn load(dir: &Path, script_path: &Path, no_derive: bool) -> Result<Loaded, (ExitCode, String)> {
    let script = load_script(script_path)?;
    let project = compile_play_project(dir)?;
    let (plan, world) = plan_script(&project, &script, script_path, no_derive)?;
    Ok(Loaded {
        script,
        project,
        plan,
        world,
    })
}

/// Read and parse a play script (exit 2 with the usage error).
fn load_script(script_path: &Path) -> Result<PlayScript, (ExitCode, String)> {
    let usage = |e: String| (ExitCode::from(2), e);
    let text = std::fs::read_to_string(script_path).map_err(|e| {
        usage(format!(
            "cannot read play script {}: {e}",
            script_path.display()
        ))
    })?;
    parse_script(&text, script_path).map_err(|e| {
        usage(format!(
            "invalid play script {}: {e}",
            script_path.display()
        ))
    })
}

/// Compile the project a play runs over ([`compile_project`]); `dir` must be
/// a directory (exit 2 with the usage error).
fn compile_play_project(dir: &Path) -> Result<ExecProject, (ExitCode, String)> {
    if !dir.is_dir() {
        return Err((
            ExitCode::from(2),
            format!("{} is not a project directory", dir.display()),
        ));
    }
    compile_project(dir).map_err(|code| (code, String::new()))
}

/// Plan `script`'s steps over `project` and seed its save (exit 2 with the
/// usage error, prefixed with the script path).
fn plan_script(
    project: &ExecProject,
    script: &PlayScript,
    script_path: &Path,
    no_derive: bool,
) -> Result<(Vec<Step>, World), (ExitCode, String)> {
    let at = |e: String| (ExitCode::from(2), format!("{}: {e}", script_path.display()));
    let plan = plan_steps(project, &script.steps).map_err(at)?;
    let mut world = seed_world(
        project,
        &WorldSeed {
            surfaces: &script.surfaces,
            save: &script.save,
            derive: script.derive,
        },
    )
    .map_err(at)?;
    if no_derive {
        world.derive = Some(false);
    }
    Ok((plan, world))
}

/// What a play's expectations are judged against (dsl 0.22.0 §4): one row
/// per executed step (with the world after it, when its `expect:` judges
/// it), and the world the play ended in.
fn play_outcome(p: &ExecProject, play: &Playthrough, said: String) -> PlayOutcome {
    let steps = play
        .steps
        .iter()
        .filter_map(|s| {
            let mut row = StepOutcome {
                index: s.n,
                label: s.label.clone(),
                world: s.world.clone(),
                ..StepOutcome::default()
            };
            if let Some(StepBody::Occasion {
                occasion,
                target,
                candidates,
                winner,
                presented,
                ..
            }) = s.body.occasion()
            {
                row.occasion = occasion.clone();
                row.target = target.clone();
                row.winner = winner.clone();
                row.offered = candidates
                    .iter()
                    .filter(|c| matches!(c.verdict, Verdict::Eligible))
                    .map(|c| c.id.clone())
                    .collect();
                row.presented = presented.iter().map(|pr| pr.id.clone()).collect();
                for pr in presented {
                    offered_options(p, &pr.document, &pr.transcript, &mut row.options);
                }
            }
            // Summer R2 / lighthouse N15: an `advance:` step's `presented`
            // spans every raise it made — each midnight's `dayEnd` /
            // `dayStart`, then the slot raise — in order; `winner`, `offered`
            // and `notOffered` stay the slot raise's (where the clock stops).
            if let StepBody::Advance { days, raised, .. } = &s.body {
                let beats = days
                    .iter()
                    .map(|d| &*d.occasion)
                    .chain(raised.as_deref())
                    .flat_map(|b| match b {
                        StepBody::Occasion { presented, .. } => presented.as_slice(),
                        _ => &[],
                    });
                row.presented = beats.map(|pr| pr.id.clone()).collect();
            }
            match &s.body {
                StepBody::Occasion { .. } => {}
                StepBody::Advance {
                    by, raised: None, ..
                } => row.occasion = format!("advance {by}"),
                StepBody::Advance { .. } => {}
                StepBody::NewRun { .. } => row.occasion = "newRun".to_string(),
                StepBody::Engine { .. } => row.occasion = "engine".to_string(),
                StepBody::Event { event } => row.occasion = format!("event {event}"),
                StepBody::End => return None,
            }
            for played in s.body.days_played() {
                let (document, transcript) = match played {
                    Played::Quest(q) => (&q.document, &q.transcript),
                    Played::Beat(pr) => (&pr.document, &pr.transcript),
                };
                offered_options(p, document, transcript, &mut row.options);
            }
            for q in s.body.settled().chain(&s.quests) {
                offered_options(p, &q.document, &q.transcript, &mut row.options);
            }
            Some(row)
        })
        .collect();
    PlayOutcome {
        steps,
        end: world_view(p, &play.world, true),
        said,
        exit: match &play.outcome {
            Ok(_) => "complete",
            Err(h) => h.exit_label(),
        },
        entry_aliases: p.entry_aliases.clone(),
    }
}

/// Every branch/hub presentation in `records` (one document's runner
/// transcript) -> the options it offered: the IR command's options minus
/// the ones spent (`once`) or whose guard decided false, unioned per id into
/// `into` — what a play step's `expect.options` judges (dsl 0.24.0, T3-10).
fn offered_options(
    p: &ExecProject,
    document: &str,
    records: &[Json],
    into: &mut BTreeMap<String, BTreeSet<String>>,
) {
    let cmds = DocCmds::new(p, document, false);
    for rec in records {
        let id = match str_of(rec, "kind") {
            "choice" => str_of(rec, "branch"),
            "hub" => str_of(rec, "hub"),
            _ => continue,
        };
        let listed = |key: &str, opt: &str| {
            rec.get(key)
                .and_then(Json::as_array)
                .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(opt)))
        };
        let offered = cmds
            .get(str_of(rec, "addr"))
            .and_then(|c| c.get("options"))
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
            .filter_map(|o| o.get("id").and_then(Json::as_str))
            .filter(|o| !listed("spent", o) && !listed("ineligible", o))
            .map(str::to_string);
        into.entry(id.to_string()).or_default().extend(offered);
    }
}

/// Judge the script's expectations; empty when it carries none.
fn judge(script: &PlayScript, outcome: &PlayOutcome) -> Vec<ExpectMiss> {
    if !script.has_expect() {
        return Vec::new();
    }
    crate::play_expect::check(outcome, &script.step_expects, script.expect.as_ref())
}

/// See [`crate::Command::Play`].
pub fn run_play(
    dir: &Path,
    script_path: &Path,
    json: bool,
    no_derive: bool,
    explain: &[String],
    ir: bool,
) -> ExitCode {
    let Loaded {
        script,
        project,
        plan,
        world,
    } = match load(dir, script_path, no_derive) {
        Ok(l) => l,
        Err((code, msg)) => {
            if !msg.is_empty() {
                eprintln!("lute play: {msg}");
            }
            return code;
        }
    };
    let initial_facts = if explain.is_empty() {
        BTreeSet::new()
    } else {
        world.facts.clone()
    };
    let play = execute(&script, &plan, Session::resume(&project, world));
    // Expectations judge the presented content (`said`); `--ir` changes
    // only what is printed.
    let explained = if explain.is_empty() {
        None
    } else {
        let w = &play.world;
        let kinds = lute_trace::datalog::closed_kinds(&project.kinds);
        let origins = fact_origins(&play, &initial_facts);
        match crate::explain::render(
            &project.rules,
            kinds,
            &project.seed_facts,
            &origins,
            &w.state,
            &w.facts,
            explain,
        ) {
            Ok(e) => Some(e),
            Err(e) => {
                eprintln!("lute play: --explain: {e}");
                return ExitCode::from(2);
            }
        }
    };
    let outcome = play_outcome(&project, &play, said(&project, &play));
    let misses = judge(&script, &outcome);
    let text = if json {
        let mut v = render_json(&play);
        if let Json::Object(root) = &mut v {
            if script.has_expect() {
                root.insert(
                    "expect".into(),
                    json!({ "misses": misses.iter().map(ExpectMiss::to_json).collect::<Vec<_>>() }),
                );
            }
            if let Some((_, j)) = &explained {
                root.insert("explain".into(), j.clone());
            }
        }
        format!("{}\n", serde_json::to_string_pretty(&v).unwrap_or_default())
    } else {
        let mut text = render_human(&project, &play, ir);
        if let Some((h, _)) = &explained {
            text.push_str(h);
            if !h.ends_with('\n') {
                text.push('\n');
            }
        }
        if script.has_expect() {
            if misses.is_empty() {
                text.push_str(&format!("── expect: every expectation held {RULE}\n"));
            } else {
                text.push_str(&format!("── expect: {} missed {RULE}\n", misses.len()));
                for m in &misses {
                    text.push_str(&format!("  ✗ {m}\n"));
                }
            }
        }
        text
    };
    if crate::write_stdout(&text).is_err() {
        return ExitCode::from(2);
    }
    // A missed expectation fails the play (exit 1) — unless the walk itself
    // already failed harder (an error, or a usage-level refusal).
    match (&play.outcome, misses.is_empty()) {
        (Err(h @ (PlayHalt::Error(_) | PlayHalt::Fatal(_))), _) => ExitCode::from(h.exit_code()),
        (_, false) => ExitCode::from(1),
        (Ok(_), true) => ExitCode::SUCCESS,
        (Err(h), true) => ExitCode::from(h.exit_code()),
    }
}

/// One `*.play.yaml` as `lute test` runs it (dsl 0.22.0 §4).
pub(crate) struct PlayTestRun {
    pub misses: Vec<ExpectMiss>,
    /// `complete | incomplete | error`.
    pub exit: &'static str,
    /// Project-relative (forward-slash) source paths of every document a
    /// presentation came from — `--coverage`'s numerator.
    pub presented_docs: BTreeSet<String>,
    /// Why the walk halted, when it did (empty when complete).
    pub notes: Vec<String>,
}

/// A project compiled once for every play of one `lute test` run (T2-1):
/// `Err` is why no play can run over it — the usage error, or that it does
/// not compile (the compile gate's diagnostics are already printed).
pub(crate) struct PlayProject(Result<ExecProject, String>);

impl PlayProject {
    /// Compile the project at `dir` for play ([`compile_project`]).
    pub(crate) fn compile(dir: &Path) -> Self {
        PlayProject(compile_play_project(dir).map_err(|(_, msg)| {
            if msg.is_empty() {
                "the project does not compile (diagnostics above)".to_string()
            } else {
                msg
            }
        }))
    }
}

/// Run `script` over the compiled `project` in process for `lute test`.
/// `Err` is a usage or compile failure.
pub(crate) fn run_play_for_test(
    project: &PlayProject,
    script: &Path,
    derive: bool,
) -> Result<PlayTestRun, String> {
    let script_path = script;
    let script = load_script(script_path).map_err(|(_, msg)| msg)?;
    let p = project.0.as_ref().map_err(String::clone)?;
    let (plan, world) = plan_script(p, &script, script_path, !derive).map_err(|(_, msg)| msg)?;
    let play = execute(&script, &plan, Session::resume(p, world));
    let outcome = play_outcome(&p, &play, said(&p, &play));
    let misses = judge(&script, &outcome);
    // A presented beat's document — an `occasion:` step's, or the occasion
    // an `advance:` step's clock raised — and a quest document whose
    // lifecycle transitioned or whose handlers played during the play.
    let presented_docs = play
        .steps
        .iter()
        .flat_map(|s| match s.body.occasion() {
            Some(StepBody::Occasion { presented, .. }) => {
                presented.iter().map(|pr| pr.document.clone()).collect()
            }
            _ => Vec::new(),
        })
        .chain(play.steps.iter().flat_map(|s| {
            s.body.days_played().into_iter().map(|played| match played {
                Played::Quest(q) => q.document.clone(),
                Played::Beat(pr) => pr.document.clone(),
            })
        }))
        .chain(
            play.start
                .iter()
                .chain(
                    play.steps
                        .iter()
                        .flat_map(|s| s.body.settled().chain(&s.quests)),
                )
                .map(|q| q.document.clone()),
        )
        .collect();
    Ok(PlayTestRun {
        misses,
        exit: outcome.exit,
        presented_docs,
        notes: play
            .outcome
            .as_ref()
            .err()
            .map(|h| h.message().to_string())
            .into_iter()
            .collect(),
    })
}

/// One presentation a play made, as the differential harness replays it
/// (`crate::differential`, docs/design/runtime-unification.md §4.3).
#[cfg(test)]
pub(crate) struct PlayedBeat {
    pub(crate) presented: Presented,
    /// The last presentation of a step that ended holding exclusive relations
    /// together (`StepRecord::exclusive`), which fails the play there.
    pub(crate) step_exclusive: bool,
    /// The play halted at this presentation's step and it is the last one:
    /// [`PlayHalt::exit_label`].
    pub(crate) halted: Option<&'static str>,
}

/// Run the play `script` over the project at `dir` in process and hand back
/// every presentation it made, in order (an `advance:`'s midnight raises
/// before its slot raise).
#[cfg(test)]
pub(crate) fn presentations_for_diff(dir: &Path, script: &Path) -> Result<Vec<PlayedBeat>, String> {
    let Loaded {
        script,
        project,
        plan,
        world,
    } = load(dir, script, false).map_err(|(_, msg)| msg)?;
    let play = execute(&script, &plan, Session::resume(&project, world));
    let halt = play.outcome.as_ref().err().map(PlayHalt::exit_label);
    let mut out: Vec<PlayedBeat> = Vec::new();
    let steps = play.steps.len();
    for (i, s) in play.steps.iter().enumerate() {
        let before = out.len();
        let days = s.body.days_played();
        let raised = match s.body.occasion() {
            Some(StepBody::Occasion { presented, .. }) => presented.iter().collect(),
            _ => Vec::new(),
        };
        let beats = days
            .into_iter()
            .filter_map(|p| match p {
                Played::Beat(pr) => Some(pr),
                Played::Quest(_) => None,
            })
            .chain(raised);
        for pr in beats {
            out.push(PlayedBeat {
                presented: pr.clone(),
                step_exclusive: false,
                halted: None,
            });
        }
        if out.len() > before {
            let last = out.last_mut().expect("pushed above");
            last.step_exclusive = !s.exclusive.is_empty();
            if i + 1 == steps {
                last.halted = halt;
            }
        }
    }
    Ok(out)
}
