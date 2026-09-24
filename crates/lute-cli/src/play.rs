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
//! - Execution: [`crate::runner::Runner`] — `lute run`'s evaluator — runs
//!   every scene beat, every entry beat (its `--entry` path: first-read
//!   effects, `entry.<id>.read`), every quest-lifecycle advance
//!   ([`Runner::advance_quests`]) and every `when` ([`Runner::eval_guard`]).
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

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_check::PrereqFormula;
use lute_compile::index::{build_index, BeatKind, IndexBeat, IndexInput, ProjectIndex};
use lute_compile::{Artifact, BeatOnce};
use lute_manifest::relations::{EntityKindDecl, KindShape};
use lute_manifest::schema::{OccasionDecl, OccasionSelect};
use lute_trace::{MockSet, UnresolvedAtom, Value};
use serde_json::{json, Value as Json};

use crate::play_expect::{ExpectMiss, PlayOutcome, StepOutcome};
use crate::runner::{Fact, Runner, RunnerOutcome};

pub(crate) mod calendar;

// ===========================================================================
// Play script (`*.play.yaml`, dsl 0.21.0 §6, 0.22.0 §1–§4, §9, §10, §13).
// ===========================================================================

/// The complete legal top-level key set of a play script.
const SCRIPT_KEYS: &[&str] = &[
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
const MOCK_SURFACES: &[&str] = &["choose", "facts", "state"];
/// The complete legal key set of one `steps:` entry.
const STEP_KEYS: &[&str] = &[
    "choose", "engine", "event", "expect", "label", "newRun", "occasion", "pick", "repeat",
    "target",
];
/// The keys of a step that DO something — exactly one per step.
const STEP_ACTIONS: &[&str] = &["occasion", "newRun", "engine", "event"];

/// A `pick:` on a `select: all` occasion.
#[derive(Clone, PartialEq)]
enum Pick {
    /// Present this beat.
    Beat(String),
    /// `pick: none` (dsl 0.22.0 §10): close the list — nothing presented or
    /// spent; `on=` objectives are still judged.
    Pass,
}

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
}

/// The save a script starts from (dsl 0.22.0 §3), as written.
#[derive(Default)]
struct SaveSeed {
    visited: Vec<String>,
    presented_user: Vec<String>,
    presented_run: Vec<String>,
    quests: Vec<(String, String)>,
    entries_run: Vec<String>,
    entries_user: Vec<String>,
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
fn tiered_lists(
    v: &serde_yaml::Value,
    key: &str,
) -> Result<(Vec<String>, Vec<String>), String> {
    let serde_yaml::Value::Mapping(m) = v else {
        return Err(format!("`{key}:` must be a mapping `{{ run: [...], user: [...] }}`"));
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

/// Parse a play script. Total: never panics; `Err` names what is wrong.
fn parse_script(text: &str) -> Result<PlayScript, String> {
    parse_script_with(text, true)
}

/// [`parse_script`]; `steps_required: false` also admits a script that is
/// only a save (seeds and no `steps:`) — what `lute calendar --script`
/// starts every cell from (dsl 0.23.0 §1).
fn parse_script_with(text: &str, steps_required: bool) -> Result<PlayScript, String> {
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

/// `engine:` / long-form `newRun:` writes. `retract` says whether
/// `retract:` is legal (a new run's seed only adds).
fn parse_writes(n: usize, key: &str, v: &serde_yaml::Value, retract: bool) -> Result<RawWrites, String> {
    let legal = if retract { "state, facts, retract" } else { "state, facts" };
    let serde_yaml::Value::Mapping(m) = v else {
        return Err(format!("step {n}: `{key}:` must be a mapping ({legal})"));
    };
    let mut w = RawWrites::default();
    for (k, v) in m {
        match k.as_str() {
            Some("state") => {
                let serde_yaml::Value::Mapping(paths) = v else {
                    return Err(format!("step {n}: `{key}.state` must be a mapping of path -> value"));
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
            other => {
                return Err(format!(
                    "step {n}: unknown `{key}:` key `{}` (legal: {legal})",
                    other.unwrap_or("?")
                ))
            }
        }
    }
    if w.state.is_empty() && w.facts.is_empty() && w.retract.is_empty() {
        return Err(format!("step {n}: `{key}:` writes nothing ({legal})"));
    }
    Ok(w)
}

/// One `steps:` entry and its `expect:` (validated). Exactly one action key
/// ([`STEP_ACTIONS`]); `target`/`pick`/`choose`/`expect` only beside
/// `occasion`; `label`/`repeat` beside any.
fn parse_step(n: usize, item: &serde_yaml::Value) -> Result<(ScriptStep, Option<serde_yaml::Value>), String> {
    let shape = "one of `occasion` (with `target`, `pick`, `choose`, `expect`), `newRun`, \
                 `engine`, `event` — plus `label`/`repeat`";
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
                pick = Some(if s == "none" { Pick::Pass } else { Pick::Beat(s) });
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
            "expect" => {
                occasion_only.push(key);
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
            _ => unreachable!("every STEP_KEYS key is matched"),
        }
    }
    let action = match actions.as_slice() {
        [] => return Err(format!("step {n} names no action — {shape}")),
        [a, b, ..] => {
            return Err(format!(
                "step {n}: a step raises an `occasion`, starts a `newRun`, applies `engine` \
                 writes or fires an `event` — not both `{a}` and `{b}`"
            ))
        }
        [one] => *one,
    };
    if action != "occasion" {
        if let Some(key) = occasion_only.first() {
            return Err(format!(
                "step {n}: `{key}` applies only to an `occasion` step, not `{action}`"
            ));
        }
    }
    let action = match action {
        "occasion" => StepAction::Occasion {
            occasion: occasion.unwrap_or_default(),
            target,
            pick,
            choose,
        },
        "event" => StepAction::Event(event.unwrap_or_default()),
        "newRun" => StepAction::NewRun(writes.unwrap_or_default()),
        _ => StepAction::Engine(writes.unwrap_or_default()),
    };
    Ok((
        ScriptStep {
            n,
            label,
            repeat,
            action,
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

/// Everything the playthrough reads from the compiled project.
struct Project {
    /// project-relative path -> compiled artifact JSON.
    artifacts: BTreeMap<String, Json>,
    /// The `compile --all` project index: `beats` (selection tiebreak
    /// order), `relations` (tiers), `seedFacts`, `rules`.
    index: ProjectIndex,
    /// The occasions every resolved plugin declares, unioned across the
    /// project's documents (first declaration wins). Empty ⇒ shape-only.
    occasions: BTreeMap<String, OccasionDecl>,
    /// State-TABLE union (`build_index` does not cover it): path -> its
    /// `StateEntry` JSON, first declaration in path order wins.
    state_table: BTreeMap<String, Json>,
    /// `index.rules` as artifact JSON, handed to every runner.
    rules: Json,
    /// `index.seedFacts` as ground facts.
    seed_facts: BTreeSet<Fact>,
    /// Base relations of `tier: run` — the facts a `newRun` resets.
    run_relations: BTreeSet<String>,
    /// Quest documents, path order — advanced after every presentation.
    quest_docs: Vec<String>,
    /// Every quest id those documents declare — the ids a `state:` seed of
    /// `quest.<id>.state` may name.
    quest_ids: BTreeSet<String>,
    /// Every occasion some quest objective is judged at (`<objective
    /// on=…>`, dsl 0.21.0 §7a.2) — raising one advances those objectives
    /// even when no beat answers it.
    objective_occasions: BTreeSet<String>,
    /// A command-less artifact carrying the union rules + state table: the
    /// evaluator runner every `when` is decided by.
    eval_json: Json,
    /// Capability-declared world events, unioned across the documents — what
    /// an `event:` step may fire (dsl 0.22.0 §9).
    world_events: BTreeSet<String>,
    /// Every scene id (`meta.id`) — what `visited:` may name.
    scene_ids: BTreeSet<String>,
    /// Every `<entry>` id — what `entriesRead:` may name, and the
    /// `entry.<id>.everRead` paths the playthrough keeps (dsl 0.22.0 §7).
    entry_ids: BTreeSet<String>,
    /// `<quest tier="run">` quests (dsl 0.22.0 §7) -> their objective ids:
    /// reset to `unset` at every `newRun`.
    run_quests: BTreeMap<String, Vec<String>>,
    /// The union entity kinds, the domain an occasion `target: { prefix,
    /// entity }` names (dsl 0.22.0 §8).
    kinds: BTreeMap<String, EntityKindDecl>,
}

impl Project {
    fn select_of(&self, occasion: &str) -> OccasionSelect {
        self.occasions
            .get(occasion)
            .map(|d| d.select)
            .unwrap_or_default()
    }
}

/// Compile every non-component document under `project_dir` in memory with
/// the `compile --all` gate — refusing to play a project that does not
/// wholly compile — and build its index. `Err` carries the exit code after
/// the diagnostics are printed.
fn compile_project(project_dir: &Path) -> Result<Project, ExitCode> {
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
        let Some(built) = crate::build_input(file, None, Some(project_dir), None) else {
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

    let inputs: Vec<IndexInput> = compiled
        .iter()
        .map(|(rel, art)| IndexInput {
            path: rel.clone(),
            artifact_path: format!("{rel}.json"),
            artifact: art,
        })
        .collect();
    let index = match build_index(lute_compile::LUTE_IR_VERSION, &inputs) {
        Ok(index) => index,
        Err(errs) => {
            for e in &errs {
                eprintln!("lute play: {e}");
            }
            eprintln!(
                "lute play: {} vocabulary conflict(s); refusing to play",
                errs.len()
            );
            return Err(ExitCode::from(1));
        }
    };

    let mut artifacts: BTreeMap<String, Json> = BTreeMap::new();
    for (rel, art) in &compiled {
        match serde_json::to_value(art) {
            Ok(j) => {
                artifacts.insert(rel.clone(), j);
            }
            Err(e) => {
                eprintln!("lute play: cannot serialize the artifact of {rel}: {e}");
                return Err(ExitCode::from(2));
            }
        }
    }

    let mut state_table: BTreeMap<String, Json> = BTreeMap::new();
    for art in artifacts.values() {
        for e in art
            .get("state")
            .and_then(Json::as_array)
            .into_iter()
            .flatten()
        {
            if let Some(path) = e.get("path").and_then(Json::as_str) {
                state_table
                    .entry(path.to_string())
                    .or_insert_with(|| e.clone());
            }
        }
    }
    let rules = serde_json::to_value(&index.rules).unwrap_or_else(|_| json!([]));
    let seed_facts = index
        .seed_facts
        .iter()
        .map(|f| (f.relation.clone(), f.args.clone()))
        .collect();
    let run_relations = index
        .relations
        .iter()
        .filter(|r| r.tier.as_deref() == Some("run"))
        .map(|r| r.name.clone())
        .collect();
    let quest_docs: Vec<String> = artifacts
        .iter()
        .filter(|(_, a)| a.get("kind").and_then(Json::as_str) == Some("quest"))
        .map(|(rel, _)| rel.clone())
        .collect();
    let quest_ids = quest_docs
        .iter()
        .filter_map(|rel| artifacts.get(rel))
        .flat_map(|a| a.get("commands").and_then(Json::as_array).into_iter().flatten())
        .filter(|c| c.get("kind").and_then(Json::as_str) == Some("quest"))
        .filter_map(|c| c.get("id").and_then(Json::as_str).map(str::to_string))
        .collect();
    let quest_cmds = || {
        quest_docs
            .iter()
            .filter_map(|rel| artifacts.get(rel))
            .flat_map(|a| a.get("commands").and_then(Json::as_array).into_iter().flatten())
            .filter(|c| c.get("kind").and_then(Json::as_str) == Some("quest"))
    };
    let objective_occasions = quest_cmds()
        .flat_map(|c| c.get("objectives").and_then(Json::as_array).into_iter().flatten())
        .filter_map(|o| o.get("on").and_then(Json::as_str).map(str::to_string))
        .collect();
    let run_quests = quest_cmds()
        .filter(|c| c.get("tier").and_then(Json::as_str) == Some("run"))
        .filter_map(|c| {
            let id = c.get("id").and_then(Json::as_str)?.to_string();
            let objectives = c
                .get("objectives")
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
                .filter_map(|o| o.get("id").and_then(Json::as_str).map(str::to_string))
                .collect();
            Some((id, objectives))
        })
        .collect();
    // dsl 0.23.0 §4: a bundle beat is visited like a scene.
    let scene_ids = artifacts
        .values()
        .filter(|a| a.get("kind").and_then(Json::as_str) == Some("scene"))
        .filter_map(|a| a.get("meta")?.get("id")?.as_str().map(str::to_string))
        .chain(
            index
                .beats
                .iter()
                .filter(|b| b.kind == BeatKind::Bundle)
                .map(|b| b.id.clone()),
        )
        .collect();
    let entry_ids = index.entries.iter().map(|e| e.id.clone()).collect();
    let kinds = index
        .entities
        .iter()
        .map(|k| {
            let shape = match &k.members {
                Some(members) if !k.open => KindShape::Members(members.clone()),
                _ => KindShape::Open,
            };
            (k.name.clone(), EntityKindDecl { shape })
        })
        .collect();
    let eval_json = json!({
        "kind": "scene",
        "commands": [],
        "rules": rules.clone(),
        "state": state_table.values().cloned().collect::<Vec<_>>(),
    });

    Ok(Project {
        artifacts,
        index,
        occasions,
        state_table,
        rules,
        seed_facts,
        run_relations,
        quest_docs,
        quest_ids,
        objective_occasions,
        eval_json,
        world_events,
        scene_ids,
        entry_ids,
        run_quests,
        kinds,
    })
}

/// The artifact JSON handed to a presentation's [`Runner`]: this document's
/// OWN `commands`/`meta`/`kind`/`prereqEdges`, with `rules`/`state` REPLACED
/// by the project-wide union — a relation asserted in one document is
/// derived over in another, and a `run.*`/`user.*`/`quest.*` path declared
/// elsewhere still needs its declared type here.
fn play_artifact_json(doc_json: &Json, p: &Project) -> Json {
    let mut v = doc_json.clone();
    if let Json::Object(map) = &mut v {
        map.insert("rules".to_string(), p.rules.clone());
        map.insert(
            "state".to_string(),
            Json::Array(p.state_table.values().cloned().collect()),
        );
    }
    v
}

/// A `state:` write resolved against the declared type.
#[derive(Clone)]
enum Write {
    Set(Value),
    Add(f64),
}

/// An `engine:` step's (or a `newRun` seed's) writes, validated.
#[derive(Clone, Default)]
struct Writes {
    state: Vec<(String, Write)>,
    facts: Vec<Fact>,
    retract: Vec<Fact>,
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
}

/// One step, validated against the project.
struct Step {
    n: usize,
    label: Option<String>,
    repeat: usize,
    action: Action,
}

/// A literal against a state-table entry's declared type (the trace-mock
/// rule, `E-TRACE-MOCK-TYPE`): `bool` takes `true`/`false`, `number` a
/// number, `enum` a member of its domain; every other type the text.
fn typed_literal(entry: &Json, lit: &str) -> Result<Value, String> {
    match entry.get("type").and_then(Json::as_str) {
        Some("bool") => match lit {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err("a `bool` is `true` or `false`".to_string()),
        },
        Some("number") => lit
            .parse::<f64>()
            .map(Value::Num)
            .map_err(|_| "a `number` takes a number".to_string()),
        Some("enum") => {
            let domain: Vec<&str> = entry
                .get("domain")
                .and_then(Json::as_array)
                .into_iter()
                .flatten()
                .filter_map(Json::as_str)
                .collect();
            if domain.contains(&lit) {
                Ok(Value::Str(lit.to_string()))
            } else {
                Err(format!("the enum's members are {}", domain.join(", ")))
            }
        }
        _ => Ok(Value::Str(lit.to_string())),
    }
}

/// The entry id and flag of a reserved `entry.<id>.read` /
/// `entry.<id>.everRead` path (dsl 0.19.0 §5, 0.22.0 §7).
fn entry_flag(path: &str) -> Option<(&str, &str)> {
    let rest = path.strip_prefix("entry.")?;
    let (id, flag) = rest.rsplit_once('.')?;
    (matches!(flag, "read" | "everRead") && !id.is_empty() && !id.contains('.'))
        .then_some((id, flag))
}

/// The `entry.<id>.everRead` path (dsl 0.22.0 §7: user tier, set on first
/// read, never reset by `newRun`).
fn ever_read_path(id: &str) -> String {
    format!("entry.{id}.everRead")
}

/// Resolve one written state value — a `state:` seed, an `engine:` write,
/// a `newRun` seed — against the project: the path is declared (a reserved
/// entry flag names a declared entry), not `scene.*`, and the literal fits
/// the declared type. `Err` is the reason as a predicate of the path
/// (`… is not a declared state path`), for the caller to prefix.
fn resolve_state(p: &Project, path: &str, lit: &str) -> Result<Value, String> {
    if path.starts_with("scene.") {
        return Err(
            "is `scene.*`, which resets at every scene boundary and cannot be written".to_string(),
        );
    }
    if let Some((id, _)) = entry_flag(path) {
        if !p.entry_ids.contains(id) {
            return Err(format!("names entry `{id}`, which no document declares"));
        }
        return match lit {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            _ => Err(format!("is a bool: `{lit}` is not `true` or `false`")),
        };
    }
    // dsl 0.23.0 §6: a save made after a run ended carries `prev.run.*`,
    // typed by the `run.*` path it mirrors.
    let declared = match path.strip_prefix("prev.") {
        Some(run) if run.starts_with("run.") => run,
        _ => path,
    };
    let Some(entry) = p.state_table.get(declared) else {
        return Err("is not a declared state path in this project".to_string());
    };
    typed_literal(entry, lit).map_err(|why| format!("does not take `{lit}`: {why}"))
}

/// Resolve one ground atom a script asserts or retracts (`facts:`,
/// `engine.facts` / `engine.retract`, a `newRun` seed): a declared,
/// non-derived relation at its arity whose closed-domain args are members.
/// Reserved relations are allowed — the engine is exactly who asserts them
/// (dsl 0.22.0 §1.1). `Err` is the reason, unprefixed.
fn resolve_fact(p: &Project, f: &str) -> Result<Fact, String> {
    let fact = parse_ground_fact(f)
        .filter(|(_, args)| args.iter().all(|a| !a.is_empty() && a != "_"))
        .ok_or_else(|| "is not a ground fact `rel(arg, …)`".to_string())?;
    let Some(r) = p.index.relations.iter().find(|r| r.name == fact.0) else {
        return Err(format!("names an undeclared relation `{}`", fact.0));
    };
    if r.derive {
        return Err(format!("`{}` is derived by rules and cannot be asserted", fact.0));
    }
    if r.args.len() != fact.1.len() {
        return Err(format!("`{}` takes {} argument(s)", fact.0, r.args.len()));
    }
    for (arg, domain) in fact.1.iter().zip(&r.args) {
        let members: Option<&[String]> = match p.kinds.get(domain) {
            Some(EntityKindDecl {
                shape: KindShape::Members(ms),
            }) => Some(ms),
            Some(_) => None,
            None => p
                .index
                .enums
                .iter()
                .find(|e| &e.name == domain)
                .map(|e| e.members.as_slice()),
        };
        if let Some(ms) = members.filter(|ms| !ms.contains(arg)) {
            return Err(format!(
                "`{arg}` is not a member of `{domain}` ({})",
                ms.join(", ")
            ));
        }
    }
    Ok(fact)
}

/// Resolve an `engine:` step's / `newRun` seed's writes. A `quest.*` path is
/// the quest runner's (its lifecycle transitions fire handlers and grants),
/// never written directly — a save's quest status is top-level `quests:`.
fn resolve_writes(p: &Project, n: usize, key: &str, raw: &RawWrites) -> Result<Writes, String> {
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
                let ty = p.state_table.get(path).map(|e| e.get("type").and_then(Json::as_str));
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
    for (list, into, what) in [(&raw.facts, &mut out.facts, "facts"), (&raw.retract, &mut out.retract, "retract")] {
        for f in list {
            into.push(resolve_fact(p, f).map_err(|e| format!("step {n}: `{key}.{what}` entry `{f}` {e}"))?);
        }
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
fn plan_steps(p: &Project, steps: &[ScriptStep]) -> Result<Vec<Step>, String> {
    let mut answered: BTreeSet<&str> = p.index.beats.iter().map(|b| b.on.as_str()).collect();
    answered.extend(p.objective_occasions.iter().map(String::as_str));
    let mut plan = Vec::with_capacity(steps.len());
    for step in steps {
        let n = step.n;
        let action = match &step.action {
            StepAction::NewRun(raw) => Action::NewRun(resolve_writes(p, n, "newRun", raw)?),
            StepAction::Engine(raw) => Action::Engine(resolve_writes(p, n, "engine", raw)?),
            StepAction::Event(name) => {
                if lute_manifest::snapshot::BUILTIN_LIFECYCLE_EVENTS.contains(&name.as_str()) {
                    return Err(format!(
                        "step {n}: `{name}` is a quest lifecycle event — the quest runner fires it \
                         on a transition; it cannot be fired from a script"
                    ));
                }
                if !p.world_events.contains(name) {
                    let hint = if p.occasions.contains_key(name) || answered.contains(name.as_str()) {
                        format!(" — `{name}` is an occasion; raise it with `occasion: {name}`")
                    } else if p.world_events.is_empty() {
                        " (no plugin declares world events)".to_string()
                    } else {
                        let declared: Vec<&str> = p.world_events.iter().map(String::as_str).collect();
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
                plan_occasion(p, &answered, n, occasion, target.as_deref(), pick.as_ref())?;
                Action::Occasion {
                    occasion: occasion.clone(),
                    target: target.clone(),
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
        });
    }
    Ok(plan)
}

/// [`plan_steps`] for one occasion step.
fn plan_occasion(
    p: &Project,
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
        (OccasionSelect::All, None) => Err(format!(
            "step {n}: occasion `{occasion}` is `select: all` — name the beat the player \
             takes with `pick:` (or `pick: none` to close the list)"
        )),
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
            if p
                .index
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

/// dsl 0.21.0 §4: a candidate answers `occasion` and its `target` is absent
/// or equal to the raised one.
fn is_candidate(b: &IndexBeat, occasion: &str, target: Option<&str>) -> bool {
    b.on == occasion && b.target.as_deref().is_none_or(|t| Some(t) == target)
}

// ===========================================================================
// The live playthrough.
// ===========================================================================

/// Everything that carries from one step to the next.
struct World {
    /// Persistent-tier state (`run.*`/`user.*`/`app.*`/`quest.*`/`entry.*`);
    /// `scene.*` never lives here — it resets at every scene boundary.
    /// Carries `entry.<id>.everRead` for every entry (dsl 0.22.0 §7).
    state: BTreeMap<String, Value>,
    /// Base facts (the runner derives over them).
    facts: BTreeSet<Fact>,
    /// quest id -> `unset`/`active`/`complete`/`failed`.
    quests: BTreeMap<String, String>,
    /// Canonical ids of every presented scene — the `visited(…)` set both
    /// `after:` and CEL `visited('<id>')` read (dsl 0.21.0 §7a.1).
    visited: BTreeSet<String>,
    /// Scene beats presented since the last `newRun` (`once: run`).
    spent_run: BTreeSet<String>,
    /// Scene beats presented in this play (`once: user`).
    spent_user: BTreeSet<String>,
    /// Quest ids `accept` records named since the last quest advance (dsl
    /// 0.21.0 §7a.3) — the next advance activates those still `unset`.
    accepts: Vec<String>,
    /// Per `<branch>` id: the decisions of a multi-decision `choose:` list
    /// earlier presentations consumed ([`Runner::with_choice_cursor`]).
    choice_cursor: BTreeMap<String, usize>,
    /// `Some(false)` under `--no-derive` / `derive: false` (dsl 0.22.0 §6):
    /// handed to every runner's mock.
    derive: Option<bool>,
    /// dsl 0.23.0 §2: `<quest>.<objective>` ids a `by` deadline failed —
    /// carried to every quest advance so a failed objective stays failed.
    failed_objectives: BTreeSet<String>,
}

impl World {
    /// A mock carrying the playthrough's derive setting.
    fn mock(&self) -> MockSet {
        MockSet {
            derive: self.derive,
            ..MockSet::default()
        }
    }
}

fn json_to_value(j: &Json) -> Option<Value> {
    match j {
        Json::Bool(b) => Some(Value::Bool(*b)),
        Json::Number(n) => n.as_f64().map(Value::Num),
        Json::String(s) => Some(Value::Str(s.clone())),
        _ => None,
    }
}

fn value_to_json(v: &Value) -> Json {
    match v {
        Value::Bool(b) => Json::Bool(*b),
        Value::Num(n) if n.fract() == 0.0 && n.abs() < 1e15 => json!(*n as i64),
        Value::Num(n) => json!(n),
        Value::Str(s) => Json::String(s.clone()),
        Value::Unknown => Json::Null,
    }
}

/// Parse a ground `"rel(a, b)"` fact.
fn parse_ground_fact(s: &str) -> Option<Fact> {
    let s = s.trim();
    let open = s.find('(')?;
    if !s.ends_with(')') {
        return None;
    }
    let rel = s[..open].trim();
    if rel.is_empty() {
        return None;
    }
    let inner = &s[open + 1..s.len() - 1];
    let args = if inner.trim().is_empty() {
        Vec::new()
    } else {
        inner.split(',').map(|a| a.trim().to_string()).collect()
    };
    Some((rel.to_string(), args))
}

/// The lifecycle values `quest.<id>.state` takes (always assigned: a quest
/// nothing has activated yet is `unset`).
const QUEST_STATES: &[&str] = &["unset", "active", "complete", "failed"];

/// The quest id of a `quest.<id>.state` path.
fn quest_state_id(path: &str) -> Option<&str> {
    path.strip_prefix("quest.")?
        .strip_suffix(".state")
        .filter(|id| !id.is_empty() && !id.contains('.'))
}

/// Register a save's quest status (a `quests:` entry or a `quest.<id>.state`
/// seed) so the start settle resumes it instead of starting the quest over.
fn seed_quest(
    p: &Project,
    w: &mut World,
    at: &str,
    id: &str,
    status: &str,
) -> Result<(), String> {
    if !p.quest_ids.contains(id) {
        let declared: Vec<&str> = p.quest_ids.iter().map(String::as_str).collect();
        return Err(format!(
            "{at}: no quest `{id}` is declared in this project (quests: {})",
            if declared.is_empty() {
                "none".to_string()
            } else {
                declared.join(", ")
            }
        ));
    }
    if !QUEST_STATES.contains(&status) {
        return Err(format!(
            "{at}: `{status}` — a quest state is one of {}",
            QUEST_STATES.join(", ")
        ));
    }
    w.quests.insert(id.to_string(), status.to_string());
    w.state
        .insert(format!("quest.{id}.state"), Value::Str(status.to_string()));
    Ok(())
}

/// `Err` naming an id a save seed names that the project does not declare,
/// with a did-you-mean.
fn unknown_id<'a>(at: &str, id: &str, what: &str, known: impl Iterator<Item = &'a str>) -> String {
    let hint = lute_manifest::suggest::nearest(id, known, 2)
        .map(|k| format!(" — did you mean `{k}`?"))
        .unwrap_or_default();
    format!("{at} names `{id}`, which is no {what} in this project{hint}")
}

/// The playthrough's starting world: every declared default (scene tier
/// excluded; `entry.<id>.everRead` false for every entry), the script's
/// `state:` over it, the save seeds (dsl 0.22.0 §3: `visited:`,
/// `presented:`, `quests:`, `entriesRead:`), the project's seed facts plus
/// the script's `facts:`. A `quest.<id>.state` seed is a `quests:` entry.
/// A seed naming an undeclared path, id, quest or relation — or a value that
/// does not fit — is a usage error, never a silent no-op.
fn seed_world(p: &Project, script: &PlayScript) -> Result<World, String> {
    let mut w = World {
        state: BTreeMap::new(),
        facts: p.seed_facts.clone(),
        quests: BTreeMap::new(),
        visited: BTreeSet::new(),
        spent_run: BTreeSet::new(),
        spent_user: BTreeSet::new(),
        accepts: Vec::new(),
        choice_cursor: BTreeMap::new(),
        derive: None,
        failed_objectives: BTreeSet::new(),
    };
    for (path, e) in &p.state_table {
        if path.starts_with("scene.") {
            continue;
        }
        if let Some(v) = e.get("default").and_then(json_to_value) {
            w.state.insert(path.clone(), v);
        }
    }
    for id in &p.entry_ids {
        w.state.insert(ever_read_path(id), Value::Bool(false));
    }
    for (path, lit, _) in &script.surfaces.state {
        let at = format!("`state.{path}`");
        if let Some(id) = quest_state_id(path) {
            seed_quest(p, &mut w, &at, id, lit)?;
            continue;
        }
        let v = resolve_state(p, path, lit).map_err(|e| format!("{at} {e}"))?;
        w.state.insert(path.clone(), v);
    }
    let save = &script.save;
    for (id, status) in &save.quests {
        seed_quest(p, &mut w, &format!("`quests.{id}`"), id, status)?;
    }
    for id in &save.visited {
        if !p.scene_ids.contains(id) {
            return Err(unknown_id("`visited:`", id, "scene", p.scene_ids.iter().map(String::as_str)));
        }
        w.visited.insert(id.clone());
    }
    for (ids, tier) in [(&save.presented_run, "run"), (&save.presented_user, "user")] {
        let at = format!("`presented.{tier}`");
        for id in ids {
            match p.index.beats.iter().find(|b| &b.id == id).map(|b| b.kind) {
                Some(BeatKind::Scene | BeatKind::Bundle) => {}
                Some(BeatKind::Entry) => {
                    return Err(format!(
                        "{at} names entry `{id}` — an entry's read history is `entriesRead:`"
                    ))
                }
                None => {
                    return Err(unknown_id(
                        &at,
                        id,
                        "scene beat",
                        p.index
                            .beats
                            .iter()
                            .filter(|b| matches!(b.kind, BeatKind::Scene | BeatKind::Bundle))
                            .map(|b| b.id.as_str()),
                    ))
                }
            }
            // A beat presented this run was also presented ever, and a
            // presented scene is a visited one — as a live presentation
            // records it.
            if tier == "run" {
                w.spent_run.insert(id.clone());
            }
            w.spent_user.insert(id.clone());
            w.visited.insert(id.clone());
        }
    }
    for (ids, tier) in [(&save.entries_run, "run"), (&save.entries_user, "user")] {
        for id in ids {
            if !p.entry_ids.contains(id) {
                return Err(unknown_id(
                    &format!("`entriesRead.{tier}`"),
                    id,
                    "entry",
                    p.entry_ids.iter().map(String::as_str),
                ));
            }
            // Read this run ⇒ read ever.
            if tier == "run" {
                w.state.insert(format!("entry.{id}.read"), Value::Bool(true));
            }
            w.state.insert(ever_read_path(id), Value::Bool(true));
        }
    }
    for f in &script.surfaces.facts {
        let fact = resolve_fact(p, f).map_err(|e| format!("`facts:` entry `{f}` {e}"))?;
        w.facts.insert(fact);
    }
    w.derive = script.derive.filter(|d| !d);
    Ok(w)
}

/// `newRun`: `run.*` state back to its declared defaults, `entry.<id>.read`
/// flags too (run-tier, dsl 0.19.0 §5), run-tier facts back to the
/// project's seed facts, `<quest tier="run">` quests back to `unset` with
/// their objectives undone (dsl 0.22.0 §7), and `once: run` spending
/// cleared; then the long form's seed (§1.1). `user.*`/`app.*`, user-tier
/// quests, `entry.<id>.everRead`, other facts, `visited` and `once: user`
/// spending persist. Returns the seed's write records.
fn new_run(p: &Project, w: &mut World, seed: &Writes) -> Result<Vec<Json>, String> {
    let run_tier = |path: &str| {
        path.starts_with("run.") || entry_flag(path).is_some_and(|(_, flag)| flag == "read")
    };
    // dsl 0.23.0 §6: the ending run's `run.*` values become `prev.run.*`
    // (a path unset at run end stays unset in the mirror).
    let ended: Vec<(String, Value)> = w
        .state
        .iter()
        .filter_map(|(k, v)| lute_check::cel_paths::prev_run_path(k).map(|prev| (prev, v.clone())))
        .collect();
    w.state.retain(|k, _| !run_tier(k) && !lute_check::cel_paths::is_prev_path(k));
    w.state.extend(ended);
    for (path, e) in &p.state_table {
        if run_tier(path) {
            if let Some(v) = e.get("default").and_then(json_to_value) {
                w.state.insert(path.clone(), v);
            }
        }
    }
    w.facts.retain(|(rel, _)| !p.run_relations.contains(rel));
    for f in &p.seed_facts {
        if p.run_relations.contains(&f.0) {
            w.facts.insert(f.clone());
        }
    }
    for (id, objectives) in &p.run_quests {
        w.quests.insert(id.clone(), "unset".to_string());
        w.state
            .insert(format!("quest.{id}.state"), Value::Str("unset".to_string()));
        w.state.remove(&format!("quest.{id}.activatedAt"));
        for oid in objectives {
            w.state.insert(
                format!("quest.{id}.objectives.{oid}.done"),
                Value::Bool(false),
            );
        }
        // dsl 0.23.0 §2: a run-tier quest's missed deadlines reset with it.
        let prefix = format!("{id}.");
        w.failed_objectives.retain(|k| !k.starts_with(&prefix));
    }
    w.spent_run.clear();
    apply_writes(w, seed)
}

/// Apply an `engine:` step's (or a `newRun` seed's) writes — `state:`, then
/// `facts:`, then `retract:` — returning one transcript record per write in
/// the runner's own record shapes (`set` / `assert` / `retract`).
fn apply_writes(w: &mut World, writes: &Writes) -> Result<Vec<Json>, String> {
    let mut records = Vec::new();
    for (path, write) in &writes.state {
        let v = match write {
            Write::Set(v) => v.clone(),
            Write::Add(d) => match w.state.get(path) {
                Some(Value::Num(n)) => Value::Num(n + d),
                _ => return Err(format!("`{path}` has no number value to add {d} to")),
            },
        };
        records.push(json!({ "kind": "set", "path": path, "value": value_to_json(&v) }));
        w.state.insert(path.clone(), v);
    }
    for f in &writes.facts {
        records.push(json!({ "kind": "assert", "fact": render_fact(f) }));
        w.facts.insert(f.clone());
    }
    for f in &writes.retract {
        let held = w.facts.remove(f);
        records.push(json!({ "kind": "retract", "pattern": render_fact(f), "held": held }));
    }
    Ok(records)
}

/// `rel(a, b)` — the runner's rendering of a ground fact.
fn render_fact((rel, args): &Fact) -> String {
    format!("{rel}({})", args.join(", "))
}

/// Fold a finished runner back into the world: persistent tiers only
/// (`scene.*` never carries), facts, quest statuses, the accepts it made
/// (dsl 0.21.0 §7a.3) for the next quest advance, and how far it consumed
/// the script's multi-decision `choose:` lists.
fn absorb(w: &mut World, outcome: &RunnerOutcome) {
    for (k, v) in &outcome.state {
        if !k.starts_with("scene.") {
            w.state.insert(k.clone(), v.clone());
        }
    }
    w.facts = outcome.base_facts.clone();
    w.quests = outcome.quest_status.clone();
    for id in &outcome.accepted {
        if !w.accepts.contains(id) {
            w.accepts.push(id.clone());
        }
    }
    w.choice_cursor = outcome.choice_cursor.clone();
    w.failed_objectives
        .extend(outcome.failed_objectives.iter().cloned());
}

/// A scene's fresh starting state: its OWN `scene.*` defaults (never the
/// union's, which may carry another document's same-named scene path), with
/// the world's persistent tiers overlaid.
fn scene_initial_state(doc_json: &Json, live: &BTreeMap<String, Value>) -> BTreeMap<String, Value> {
    let mut state = BTreeMap::new();
    for e in doc_json
        .get("state")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
    {
        let Some(path) = e.get("path").and_then(Json::as_str) else {
            continue;
        };
        if path.starts_with("scene.") {
            if let Some(v) = e.get("default").and_then(json_to_value) {
                state.insert(path.to_string(), v);
            }
        }
    }
    for (k, v) in live {
        state.insert(k.clone(), v.clone());
    }
    state
}

/// Why the playthrough stopped short of its last step.
enum PlayHalt {
    /// exit 1 — a `pick` that is not eligible, or a scripted `choose:`
    /// decision the runner refused (`E-TRACE-CHOICE`: guard false, or a
    /// spent `once` option) at its presentation point.
    Error(String),
    /// exit 2 — the runner refused a malformed artifact / unknown command.
    Fatal(String),
    /// exit 3 — something the reference runner cannot decide.
    Incomplete(String),
}

impl PlayHalt {
    fn exit_code(&self) -> ExitCode {
        ExitCode::from(match self {
            PlayHalt::Error(_) => 1,
            PlayHalt::Fatal(_) => 2,
            PlayHalt::Incomplete(_) => 3,
        })
    }

    fn message(&self) -> &str {
        match self {
            PlayHalt::Error(m) | PlayHalt::Fatal(m) | PlayHalt::Incomplete(m) => m,
        }
    }

    fn exit_label(&self) -> &'static str {
        match self {
            PlayHalt::Incomplete(_) => "incomplete",
            PlayHalt::Error(_) | PlayHalt::Fatal(_) => "error",
        }
    }
}

/// What ends a playthrough before its last step.
enum Stop {
    /// A `::end` (dsl 0.8.0) — the whole playthrough is over, complete.
    End(String),
    Halt(PlayHalt),
}

fn describe_atoms(atoms: &[UnresolvedAtom]) -> String {
    let mut parts: Vec<String> = atoms
        .iter()
        .map(|a| match a {
            UnresolvedAtom::Path(p) => format!("state path `{p}` has no value"),
            UnresolvedAtom::Fact(f) | UnresolvedAtom::DerivedFact(f) => {
                format!("fact `{f}` is undetermined")
            }
            UnresolvedAtom::Time => {
                "now()/validAt(...) has no reference-runtime resolution".to_string()
            }
        })
        .collect();
    parts.dedup();
    if parts.is_empty() {
        "it does not evaluate to a bool".to_string()
    } else {
        parts.join("; ")
    }
}

/// The honesty gate every runner outcome passes (`what` names the
/// presentation or quest document): an unscripted decision, an undecidable
/// quest objective, `now()`/`validAt(...)`, an unresolved plugin effect.
fn outcome_halt(outcome: &RunnerOutcome, what: &str, doc_json: &Json) -> Option<PlayHalt> {
    if outcome.incomplete {
        if let Some(rec) = outcome.transcript.iter().rev().find(|c| {
            c.get("note").and_then(Json::as_str) == Some("no mock decision — incomplete")
        }) {
            let kind = rec.get("kind").and_then(Json::as_str).unwrap_or("choice");
            let id = rec
                .get("branch")
                .or_else(|| rec.get("hub"))
                .and_then(Json::as_str)
                .unwrap_or("?");
            let options = decision_options(doc_json, id);
            let used_up = rec
                .get("scripted")
                .and_then(Json::as_u64)
                .map(|n| format!(" — all {n} decisions of its `choose:` list were used by earlier presentations"))
                .unwrap_or_default();
            return Some(PlayHalt::Incomplete(format!(
                "{what} reached {kind} `{id}` with no scripted `choose:` decision{used_up} (options: {})",
                if options.is_empty() {
                    "none".to_string()
                } else {
                    options.join(", ")
                }
            )));
        }
        if let Some(rec) = outcome.transcript.iter().find(|c| {
            c.get("kind").and_then(Json::as_str) == Some("objective")
                && (c.get("done").is_some_and(Json::is_null)
                    || c.get("failed").is_some_and(Json::is_null))
        }) {
            let slot = if rec.get("failed").is_some_and(Json::is_null) {
                "`by` condition (dsl 0.23.0 §2)"
            } else {
                "`done` condition"
            };
            return Some(PlayHalt::Incomplete(format!(
                "{what}: required objective `{}.{}` has a {slot} that evaluates unknown",
                rec.get("quest").and_then(Json::as_str).unwrap_or("?"),
                rec.get("objective").and_then(Json::as_str).unwrap_or("?"),
            )));
        }
        return Some(PlayHalt::Incomplete(format!("{what} is incomplete")));
    }
    if outcome
        .unresolved
        .iter()
        .any(|a| matches!(a, UnresolvedAtom::Time))
    {
        return Some(PlayHalt::Incomplete(format!(
            "{what} depends on now()/validAt(...), which the reference runner cannot resolve"
        )));
    }
    if let Some(plugin) = outcome.transcript.iter().find(|c| {
        c.get("kind").and_then(Json::as_str) == Some("plugin")
            && c.get("unresolvedEffects")
                .and_then(Json::as_array)
                .is_some_and(|a| !a.is_empty())
    }) {
        return Some(PlayHalt::Incomplete(format!(
            "{what}: plugin `{}` left a `bridgeResult` effect unresolved (no bridge is invoked)",
            plugin.get("tag").and_then(Json::as_str).unwrap_or("?")
        )));
    }
    None
}

/// The option ids of the `choice`/`hub` `id` in an artifact, declared order.
fn decision_options(doc_json: &Json, id: &str) -> Vec<String> {
    doc_json
        .get("commands")
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
        .find(|c| {
            let key = match c.get("kind").and_then(Json::as_str) {
                Some("choice") => "branchId",
                Some("hub") => "id",
                _ => return false,
            };
            c.get(key).and_then(Json::as_str) == Some(id)
        })
        .and_then(|c| c.get("options"))
        .and_then(Json::as_array)
        .into_iter()
        .flatten()
        .filter_map(|o| o.get("id").and_then(Json::as_str).map(str::to_string))
        .collect()
}

/// `::end` reason text, from the `end` record a terminated walk carries.
fn end_reason(outcome: &RunnerOutcome, what: &str) -> String {
    let reason = outcome
        .transcript
        .iter()
        .rev()
        .find(|c| c.get("kind").and_then(Json::as_str) == Some("end"))
        .and_then(|c| c.get("reason").and_then(Json::as_str))
        .map(|r| format!(" (reason: {r})"))
        .unwrap_or_default();
    format!("::end in {what}{reason}")
}

/// One quest document's lifecycle transitions from one advance.
struct QuestAdvance {
    document: String,
    transcript: Vec<Json>,
}

/// Advance every quest lifecycle to a fixpoint (dsl 0.21.0 §6, D-H): each
/// quest document's [`Runner::advance_quests`], repeated in path order until
/// a whole pass transitions nothing — a quest in one document may gate on
/// another's state. The pending accepts (§7a.3) ride every pass and are
/// spent once the lifecycle settles.
fn advance_quests(p: &Project, w: &mut World) -> (Vec<QuestAdvance>, Option<Stop>) {
    let mut out = Vec::new();
    let passes = p.quest_docs.len() * 8 + 8;
    for _ in 0..passes {
        let (moved, stop) = advance_pass(p, w, None, &mut out);
        if stop.is_some() {
            return (out, stop);
        }
        if !moved {
            break;
        }
    }
    w.accepts.clear();
    (out, None)
}

/// A moment raised for the quest lifecycles.
#[derive(Clone, Copy)]
enum Raise<'a> {
    /// dsl 0.21.0 §7a.2: judges active quests' `on="<occasion>"` objectives;
    /// the target it was raised for, if any, judges only the objectives
    /// without a `target` or with that one (dsl 0.23.0 §2).
    Occasion(&'a str, Option<&'a str>),
    /// dsl 0.22.0 §9: a world event — active quests' `<on event>` handlers
    /// run, exactly as trace `events:` fires it.
    Event(&'a str),
}

/// Raise an occasion or a world event — ONE pass in which every quest
/// document answers it (the moment is never re-raised by the fixpoint),
/// then the ordinary settle to a fixpoint.
fn raise(p: &Project, w: &mut World, moment: Raise<'_>) -> (Vec<QuestAdvance>, Option<Stop>) {
    let mut out = Vec::new();
    let (_, stop) = advance_pass(p, w, Some(moment), &mut out);
    if stop.is_some() {
        return (out, stop);
    }
    let (more, stop) = advance_quests(p, w);
    out.extend(more);
    (out, stop)
}

/// One pass over every quest document, path order: its runner resumes the
/// carried lifecycle with the pending accepts and, when given, the moment
/// raised. `true` when any document transitioned.
fn advance_pass(
    p: &Project,
    w: &mut World,
    moment: Option<Raise<'_>>,
    out: &mut Vec<QuestAdvance>,
) -> (bool, Option<Stop>) {
    let mut moved = false;
    for doc in &p.quest_docs {
        let doc_json = &p.artifacts[doc];
        let mut mock = w.mock();
        mock.accepts = w.accepts.clone();
        match moment {
            // The runner reads a targeted raise as `name@target`.
            Some(Raise::Occasion(o, t)) => mock
                .occasions
                .push(t.map_or_else(|| o.to_string(), |t| format!("{o}@{t}"))),
            Some(Raise::Event(e)) => mock.events.push(e.to_string()),
            None => {}
        }
        let mut runner = Runner::with_carryover(
            &play_artifact_json(doc_json, p),
            mock,
            w.state.clone(),
            w.facts.clone(),
            w.quests.clone(),
        )
        .with_visited(&w.visited)
        .with_choice_cursor(&w.choice_cursor)
        .with_failed_objectives(&w.failed_objectives);
        let result = runner.advance_quests();
        let outcome = runner.into_outcome();
        absorb(w, &outcome);
        let what = format!("quest document `{doc}`");
        let stop = walk_stop(result, &outcome, &what, doc_json);
        let transcript: Vec<Json> = outcome
            .transcript
            .into_iter()
            .filter(|c| {
                !c.get("done").is_some_and(Json::is_null) && !c.get("failed").is_some_and(Json::is_null)
            })
            .collect();
        if !transcript.is_empty() {
            moved = true;
            out.push(QuestAdvance {
                document: doc.clone(),
                transcript,
            });
        }
        if stop.is_some() {
            return (moved, stop);
        }
    }
    (moved, None)
}

/// How a finished runner walk ends the playthrough, if it does: a refused
/// scripted decision (`E-TRACE-CHOICE`) is an error like an ineligible
/// `pick:` (exit 1); any other runner failure is fatal (exit 2); then the
/// honesty gate; then a `::end`.
fn walk_stop(
    result: Result<(), String>,
    outcome: &RunnerOutcome,
    what: &str,
    doc_json: &Json,
) -> Option<Stop> {
    match result {
        Err(msg) if outcome.refused => Some(Stop::Halt(PlayHalt::Error(format!("{what}: {msg}")))),
        Err(msg) => Some(Stop::Halt(PlayHalt::Fatal(format!("{what}: {msg}")))),
        Ok(()) => outcome_halt(outcome, what, doc_json)
            .map(Stop::Halt)
            .or_else(|| {
                outcome
                    .terminated
                    .then(|| Stop::End(end_reason(outcome, what)))
            }),
    }
}

/// A candidate's verdict (dsl 0.21.0 §4).
enum Verdict {
    Eligible,
    Ineligible(String),
    /// `when` evaluated unknown — the detail names why.
    Unknown(String),
}

struct Candidate {
    id: String,
    kind: BeatKind,
    document: String,
    priority: i64,
    verdict: Verdict,
    /// dsl 0.23.0 §11: an entry beat already read this run
    /// (`entry.<id>.read`) — an interview menu shows it as read.
    read: bool,
    /// dsl 0.23.0 §3: a scene beat's `also: true` — presented after the
    /// `select: first` winner, never the winner itself.
    also: bool,
}

fn kind_label(kind: BeatKind) -> &'static str {
    match kind {
        BeatKind::Scene => "scene",
        BeatKind::Entry => "entry",
        BeatKind::Bundle => "beat",
    }
}

/// A scene's declared `after:` (its `prereqEdges` row), parsed by the
/// checker's restricted profile parser the compile gate already proved it
/// well-formed under.
fn scene_prereq(doc_json: &Json) -> Option<PrereqFormula> {
    let raw = doc_json
        .get("prereqEdges")
        .and_then(Json::as_array)?
        .first()?
        .get("after")
        .and_then(Json::as_str)?;
    if raw.trim().is_empty() {
        return None;
    }
    let span = lute_core_span::Span {
        byte_start: 0,
        byte_end: 0,
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    };
    lute_check::parse_prereq(raw, span).0
}

fn eval_prereq(f: &PrereqFormula, w: &World) -> bool {
    match f {
        PrereqFormula::Visited(k) => w.visited.contains(k),
        PrereqFormula::Completed(q) => w.quests.get(q).map(String::as_str) == Some("complete"),
        PrereqFormula::Active(q) => w.quests.get(q).map(String::as_str) == Some("active"),
        PrereqFormula::And(a, b) => eval_prereq(a, w) && eval_prereq(b, w),
        PrereqFormula::Or(a, b) => eval_prereq(a, w) || eval_prereq(b, w),
    }
}

/// The beat's `when` raw CEL: a scene's `meta.beat.when`, an entry's own
/// `when` on its `entry` record, a bundle beat's on its `beat` record.
fn beat_when(p: &Project, beat: &IndexBeat) -> Option<String> {
    let doc = p.artifacts.get(&beat.document)?;
    let pair = match beat.kind {
        BeatKind::Scene => doc.get("meta")?.get("beat")?.get("when")?,
        BeatKind::Entry | BeatKind::Bundle => doc
            .get("commands")?
            .as_array()?
            .iter()
            .find(|c| {
                c.get("kind").and_then(Json::as_str) == Some(record_kind(beat.kind))
                    && c.get("id").and_then(Json::as_str) == Some(beat.id.as_str())
            })?
            .get("when")?,
    };
    pair.get("raw")
        .and_then(Json::as_str)
        .filter(|r| !r.trim().is_empty())
        .map(str::to_string)
}

/// dsl 0.23.0 §3: the beat's `also: true` — a scene's `meta.beat.also`, a
/// bundle beat's `also` on its `beat` record. An entry beat never rides
/// along.
fn beat_also(p: &Project, beat: &IndexBeat) -> bool {
    match beat.kind {
        BeatKind::Scene => p
            .artifacts
            .get(&beat.document)
            .and_then(|d| d.pointer("/meta/beat/also"))
            .and_then(Json::as_bool)
            .unwrap_or(false),
        BeatKind::Bundle => p
            .artifacts
            .get(&beat.document)
            .and_then(|d| d.get("commands")?.as_array())
            .and_then(|cs| {
                cs.iter().find(|c| {
                    c.get("kind").and_then(Json::as_str) == Some("beat")
                        && c.get("id").and_then(Json::as_str) == Some(beat.id.as_str())
                })
            })
            .and_then(|c| c.get("also"))
            .and_then(Json::as_bool)
            .unwrap_or(false),
        BeatKind::Entry => false,
    }
}

/// The artifact record kind that declares a lore beat: `entry` / `beat`.
fn record_kind(kind: BeatKind) -> &'static str {
    match kind {
        BeatKind::Bundle => "beat",
        BeatKind::Scene | BeatKind::Entry => "entry",
    }
}

/// Every candidate for `occasion`/`target` with its verdict, in selection
/// order: priority descending, then `ProjectIndex.beats` order. Pure over
/// the world — what a play step presents from and what `lute calendar`
/// evaluates at every cell (dsl 0.23.0 §1).
fn eligible_at(p: &Project, w: &World, occasion: &str, target: Option<&str>) -> Vec<Candidate> {
    let mut eval = Runner::with_carryover(
        &p.eval_json,
        w.mock(),
        w.state.clone(),
        w.facts.clone(),
        w.quests.clone(),
    )
    .with_visited(&w.visited);
    let flag = |path: String| w.state.get(&path) == Some(&Value::Bool(true));
    let mut out: Vec<(usize, Candidate)> = Vec::new();
    for (idx, beat) in p.index.beats.iter().enumerate() {
        if !is_candidate(beat, occasion, target) {
            continue;
        }
        // A scene's (or bundle beat's) `once` is spent by presenting it; an
        // entry's (dsl 0.22.0 §7) by its read flag — `entry.<id>.read` (run)
        // / `.everRead` (user).
        let spent = match (beat.kind, beat.once) {
            (BeatKind::Scene | BeatKind::Bundle, Some(BeatOnce::Run))
                if w.spent_run.contains(&beat.id) =>
            {
                Some("once: run — already presented this run")
            }
            (BeatKind::Scene | BeatKind::Bundle, Some(BeatOnce::User))
                if w.spent_user.contains(&beat.id) =>
            {
                Some("once: user — already presented")
            }
            (BeatKind::Entry, Some(BeatOnce::Run)) if flag(format!("entry.{}.read", beat.id)) => {
                Some("once: run — already read this run")
            }
            (BeatKind::Entry, Some(BeatOnce::User)) if flag(ever_read_path(&beat.id)) => {
                Some("once: user — already read")
            }
            _ => None,
        };
        let after_unmet = beat.kind == BeatKind::Scene
            && p.artifacts
                .get(&beat.document)
                .and_then(scene_prereq)
                .is_some_and(|f| !eval_prereq(&f, w));
        let verdict = if let Some(reason) = spent {
            Verdict::Ineligible(reason.to_string())
        } else if after_unmet {
            Verdict::Ineligible("after: prerequisite not satisfied".to_string())
        } else {
            match beat_when(p, beat) {
                None => Verdict::Eligible,
                Some(raw) => match eval.eval_guard(&raw) {
                    Ok(true) => Verdict::Eligible,
                    Ok(false) => Verdict::Ineligible("when: false".to_string()),
                    Err(atoms) => Verdict::Unknown(format!(
                        "`{raw}` evaluates unknown: {}",
                        describe_atoms(&atoms)
                    )),
                },
            }
        };
        out.push((
            idx,
            Candidate {
                id: beat.id.clone(),
                kind: beat.kind,
                document: beat.document.clone(),
                priority: beat.priority,
                verdict,
                read: beat.kind == BeatKind::Entry && flag(format!("entry.{}.read", beat.id)),
                also: beat_also(p, beat),
            },
        ));
    }
    out.sort_by_key(|(idx, c)| (std::cmp::Reverse(c.priority), *idx));
    out.into_iter().map(|(_, c)| c).collect()
}

/// The unknown `when` that could change this step's outcome, if any. On a
/// `select: first` occasion: a main beat ordered BEFORE the first
/// definitely-eligible main beat (a later one can never win), or any `also`
/// beat (each rides along on its own, dsl 0.23.0 §3). On `select: all` /
/// `sequence` any (the offered or presented list itself depends on it).
fn deciding_unknown(cands: &[Candidate], select: OccasionSelect) -> Option<&Candidate> {
    let unknown = |c: &&Candidate| matches!(c.verdict, Verdict::Unknown(_));
    if select != OccasionSelect::First {
        return cands.iter().find(unknown);
    }
    cands
        .iter()
        .filter(|c| !c.also)
        .take_while(|c| !matches!(c.verdict, Verdict::Eligible))
        .find(unknown)
        .or_else(|| cands.iter().filter(|c| c.also).find(unknown))
}

/// What an occasion presents without a `pick:` (dsl 0.21.0 §4, 0.23.0 §3),
/// as indices into `cands` (selection order), in presentation order:
/// `select: first` — the first eligible non-`also` beat (the winner), then
/// every eligible `also` beat; `select: sequence` — every eligible beat;
/// `select: all` — every eligible beat, which is the OFFERED list (the
/// player's `pick:` presents one of them). Eligibility is decided once, when
/// the occasion is raised. Pure: `lute play` presents from it and `lute
/// calendar` reports it.
fn presented(select: OccasionSelect, cands: &[Candidate]) -> Vec<usize> {
    let eligible = |c: &Candidate| matches!(c.verdict, Verdict::Eligible);
    match select {
        OccasionSelect::First => cands
            .iter()
            .position(|c| !c.also && eligible(c))
            .into_iter()
            .chain((0..cands.len()).filter(|&i| cands[i].also && eligible(&cands[i])))
            .collect(),
        OccasionSelect::All | OccasionSelect::Sequence => {
            (0..cands.len()).filter(|&i| eligible(&cands[i])).collect()
        }
    }
}

/// One presented beat.
struct Presented {
    id: String,
    kind: BeatKind,
    document: String,
    transcript: Vec<Json>,
    state_before: BTreeMap<String, Value>,
    state_after: BTreeMap<String, Value>,
}

/// Present `beat`: a scene through the runner (`scene.*` fresh), an entry
/// through the runner's entry path (first-read effects, `entry.<id>.read`),
/// a bundle beat through its `beat` record's body (dsl 0.23.0 §4).
fn present(p: &Project, w: &mut World, beat: &IndexBeat, mock: &MockSet) -> (Presented, Option<Stop>) {
    let doc_json = &p.artifacts[&beat.document];
    let state_before = w.state.clone();
    let runner = Runner::with_carryover(
        &play_artifact_json(doc_json, p),
        mock.clone(),
        scene_initial_state(doc_json, &w.state),
        w.facts.clone(),
        w.quests.clone(),
    )
    .with_visited(&w.visited)
    .with_choice_cursor(&w.choice_cursor);
    let mut runner = match beat.kind {
        BeatKind::Entry => runner.with_entry(&beat.id),
        BeatKind::Scene => runner,
        BeatKind::Bundle => runner.with_bundle_beat(&beat.id),
    };
    let result = runner.run();
    let outcome = runner.into_outcome();
    absorb(w, &outcome);
    match beat.kind {
        BeatKind::Scene | BeatKind::Bundle => {
            w.visited.insert(beat.id.clone());
            w.spent_run.insert(beat.id.clone());
            w.spent_user.insert(beat.id.clone());
        }
        // dsl 0.22.0 §7: a completed first read sets the user-tier
        // `everRead` beside the runner's run-tier `read`; never reset.
        BeatKind::Entry => {
            if w.state.get(&format!("entry.{}.read", beat.id)) == Some(&Value::Bool(true)) {
                w.state.insert(ever_read_path(&beat.id), Value::Bool(true));
            }
        }
    }
    let what = format!("{} `{}` ({})", kind_label(beat.kind), beat.id, beat.document);
    let stop = walk_stop(result, &outcome, &what, doc_json);
    let presented = Presented {
        id: beat.id.clone(),
        kind: beat.kind,
        document: beat.document.clone(),
        transcript: outcome.transcript,
        state_before,
        state_after: w.state.clone(),
    };
    (presented, stop)
}

/// One script step's record.
enum StepBody {
    Occasion {
        occasion: String,
        target: Option<String>,
        select: OccasionSelect,
        pick: Option<Pick>,
        candidates: Vec<Candidate>,
        /// The main beat — `None` with `decided: true` when the occasion
        /// passed with no main story (no eligible non-`also` beat, or
        /// `pick: none`).
        winner: Option<String>,
        decided: bool,
        /// Every presentation, in order (dsl 0.23.0 §3): the winner, then
        /// its `also` riders; or a `select: sequence`'s eligible beats.
        presented: Vec<Presented>,
    },
    /// `writes`: the long form's seed records.
    NewRun { writes: Vec<Json> },
    Engine { writes: Vec<Json> },
    Event { event: String },
}

struct StepRecord {
    n: usize,
    label: Option<String>,
    /// `(k, of)` for the k-th run of a `repeat: of` step (of > 1).
    iteration: Option<(usize, usize)>,
    body: StepBody,
    quests: Vec<QuestAdvance>,
}

/// The whole playthrough: the initial quest settle, then every step, and
/// the world it ended in.
struct Playthrough {
    start: Vec<QuestAdvance>,
    steps: Vec<StepRecord>,
    outcome: Result<String, PlayHalt>,
    world: World,
}

fn execute(p: &Project, script: &PlayScript, plan: &[Step], mut w: World) -> Playthrough {
    let (start, stop) = advance_quests(p, &mut w);
    let mut steps = Vec::new();
    let finish = |start, steps, stop: Stop, world| Playthrough {
        start,
        steps,
        outcome: match stop {
            Stop::End(reason) => Ok(reason),
            Stop::Halt(h) => Err(h),
        },
        world,
    };
    if let Some(stop) = stop {
        return finish(start, steps, stop, w);
    }
    for step in plan {
        for k in 1..=step.repeat {
            let (body, quests, stop) = run_step(p, script, &mut w, step);
            steps.push(StepRecord {
                n: step.n,
                label: step.label.clone(),
                iteration: (step.repeat > 1).then_some((k, step.repeat)),
                body,
                quests,
            });
            if let Some(stop) = stop {
                return finish(start, steps, stop, w);
            }
        }
    }
    let n = steps.len();
    Playthrough {
        start,
        steps,
        outcome: Ok(format!("complete ({n} step{})", if n == 1 { "" } else { "s" })),
        world: w,
    }
}

/// Run one step (one repetition): its body record, the quest advances it
/// made, and what ends the playthrough, if anything. Every step that
/// changes the world settles the quest lifecycle after it — a presentation,
/// an `engine:` write, a new run (dsl 0.22.0 §1.1) — and a raised occasion
/// or event is then answered by the quests.
fn run_step(
    p: &Project,
    script: &PlayScript,
    w: &mut World,
    step: &Step,
) -> (StepBody, Vec<QuestAdvance>, Option<Stop>) {
    let n = step.n;
    let (occasion, target, pick, choose) = match &step.action {
        Action::Occasion {
            occasion,
            target,
            pick,
            choose,
        } => (occasion, target, pick, choose),
        Action::NewRun(seed) => {
            return match new_run(p, w, seed) {
                Ok(writes) => {
                    let (quests, stop) = advance_quests(p, w);
                    (StepBody::NewRun { writes }, quests, stop)
                }
                Err(e) => (
                    StepBody::NewRun { writes: Vec::new() },
                    Vec::new(),
                    Some(Stop::Halt(PlayHalt::Error(format!("step {n}: {e}")))),
                ),
            };
        }
        Action::Engine(writes) => {
            return match apply_writes(w, writes) {
                Ok(writes) => {
                    let (quests, stop) = advance_quests(p, w);
                    (StepBody::Engine { writes }, quests, stop)
                }
                Err(e) => (
                    StepBody::Engine { writes: Vec::new() },
                    Vec::new(),
                    Some(Stop::Halt(PlayHalt::Error(format!("step {n}: {e}")))),
                ),
            };
        }
        Action::Event(event) => {
            let (quests, stop) = raise(p, w, Raise::Event(event));
            return (StepBody::Event { event: event.clone() }, quests, stop);
        }
    };
    let select = p.select_of(occasion);
    let cands = eligible_at(p, w, occasion, target.as_deref());
    let halt = if let Some(c) = deciding_unknown(&cands, select) {
        let Verdict::Unknown(detail) = &c.verdict else {
            unreachable!("deciding_unknown returns only unknown verdicts")
        };
        Some(PlayHalt::Incomplete(format!(
            "step {n}: the `when` of {} `{}` ({}) decides the {occasion} outcome but {detail}",
            kind_label(c.kind),
            c.id,
            c.document
        )))
    } else if let Some(Pick::Beat(pk)) = pick {
        match cands.iter().find(|c| &c.id == pk).map(|c| &c.verdict) {
            Some(Verdict::Eligible) => None,
            Some(Verdict::Ineligible(reason)) => Some(PlayHalt::Error(format!(
                "step {n}: `pick: {pk}` is not eligible — {reason}"
            ))),
            _ => Some(PlayHalt::Error(format!(
                "step {n}: `pick: {pk}` is not a candidate of {occasion}"
            ))),
        }
    } else {
        None
    };
    let decided = halt.is_none();
    // What the step presents, in order (dsl 0.23.0 §3): the `pick` on
    // `select: all`; otherwise [`presented`] — the `select: first` winner
    // then its eligible `also` beats, or every eligible beat of a
    // `select: sequence`. Eligibility was decided once, above.
    let order: Vec<&Candidate> = match (decided, pick) {
        (false, _) | (true, Some(Pick::Pass)) => Vec::new(),
        (true, Some(Pick::Beat(pk))) => cands.iter().filter(|c| &c.id == pk).take(1).collect(),
        (true, None) => presented(select, &cands).into_iter().map(|i| &cands[i]).collect(),
    };
    // The winner is the main beat: never an `also` rider.
    let winner = order.iter().find(|c| !c.also).map(|c| c.id.clone());
    let beats: Vec<&IndexBeat> = order
        .iter()
        .filter_map(|c| {
            p.index
                .beats
                .iter()
                .find(|b| b.id == c.id && b.document == c.document && b.kind == c.kind)
        })
        .collect();
    // Each presentation that plays through settles every quest before the
    // next one (so a `by` deadline is judged after each). One that halts
    // presents and advances nothing further; one that ends the playthrough
    // with `::end` presents nothing further, but its settle and the
    // occasion's judging still run — `::end` stops the walk only AFTER the
    // step's lifecycle is settled, so the quest progress the ending scene
    // made still lands.
    let mut presented_beats = Vec::new();
    let mut quests = Vec::new();
    let mut ended = None;
    let mut stop = halt.map(Stop::Halt);
    for b in beats {
        if stop.is_some() || ended.is_some() {
            break;
        }
        let (pr, s) = present_with_choose(p, script, w, b, choose);
        presented_beats.push(pr);
        match s {
            Some(Stop::End(reason)) => ended = Some(reason),
            s => stop = s,
        }
        if stop.is_none() {
            let (more, s) = advance_quests(p, w);
            quests.extend(more);
            stop = s;
        }
    }
    // dsl 0.21.0 §7a.2: after the presentations (or none — `pick: none`
    // included, dsl 0.22.0 §10), the occasion judges the `on=` objectives
    // of every active quest — for the step's target (dsl 0.23.0 §2).
    if stop.is_none() && p.objective_occasions.contains(occasion) {
        let (more, s) = raise(p, w, Raise::Occasion(occasion, target.as_deref()));
        quests.extend(more);
        stop = s;
    }
    // A settle that halts still halts; otherwise the presentation's own
    // `::end` is the reason the playthrough is over.
    let stop = match (stop, ended) {
        (Some(Stop::Halt(h)), _) => Some(Stop::Halt(h)),
        (_, Some(reason)) => Some(Stop::End(reason)),
        (stop, None) => stop,
    };
    let body = StepBody::Occasion {
        occasion: occasion.clone(),
        target: target.clone(),
        select,
        pick: pick.clone(),
        candidates: cands,
        winner,
        decided,
        presented: presented_beats,
    };
    (body, quests, stop)
}

/// [`present`] with the script's `choose:`, the step's own `choose:`
/// replacing it key by key (dsl 0.22.0 §2). A step-local decision list is
/// consumed from its start and leaves the script-wide list's consumption
/// where it was.
fn present_with_choose(
    p: &Project,
    script: &PlayScript,
    w: &mut World,
    beat: &IndexBeat,
    step_choose: &BTreeMap<String, Vec<String>>,
) -> (Presented, Option<Stop>) {
    let mut mock = w.mock();
    mock.choose = script.surfaces.choose.clone();
    mock.choose
        .extend(step_choose.iter().map(|(k, v)| (k.clone(), v.clone())));
    let saved: Vec<(String, Option<usize>)> = step_choose
        .keys()
        .map(|k| (k.clone(), w.choice_cursor.remove(k)))
        .collect();
    let out = present(p, w, beat, &mock);
    for (k, cursor) in saved {
        match cursor {
            Some(c) => w.choice_cursor.insert(k, c),
            None => w.choice_cursor.remove(&k),
        };
    }
    out
}

// ===========================================================================
// Rendering. Human by default, `--json` one object; both reuse the runner's
// own transcript records rather than re-deriving per-kind semantics.
// ===========================================================================

fn render_attrs(cmd: &Json, skip: &[&str]) -> String {
    let Json::Object(map) = cmd else {
        return String::new();
    };
    let mut parts = Vec::new();
    for (k, v) in map {
        if skip.contains(&k.as_str()) {
            continue;
        }
        match v {
            Json::String(s) => parts.push(format!("{k}=\"{s}\"")),
            Json::Bool(b) => parts.push(format!("{k}={b}")),
            Json::Number(n) => parts.push(format!("{k}={n}")),
            _ => {}
        }
    }
    parts.join(" ")
}

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

/// A line's source head: `@speaker` plus its authored delivery — the role
/// flag it was written with (`mono`/`vo`/`os`) and its attrs (`as=`,
/// `emotion=`, …) — never the compiler's identity fields.
fn line_head(speaker: &str, orig: Option<&Json>) -> String {
    let flag = match orig.map(|c| str_of(c, "role")) {
        Some("monologue") => Some("mono"),
        Some("voiceover") => Some("vo"),
        Some("offscreen") => Some("os"),
        _ => None,
    };
    let attrs = orig
        .map(|c| {
            render_attrs(
                c,
                &[
                    "addr",
                    "kind",
                    "text",
                    "speaker",
                    "lineId",
                    "voiceKey",
                    "role",
                    "placeholders",
                    "texts",
                ],
            )
        })
        .unwrap_or_default();
    let inner: Vec<&str> = flag
        .into_iter()
        .chain((!attrs.is_empty()).then_some(attrs.as_str()))
        .collect();
    if inner.is_empty() {
        format!("@{speaker}")
    } else {
        format!("@{speaker}{{{}}}", inner.join(" "))
    }
}

/// One document's commands, by address and in stream order.
struct DocCmds<'a> {
    list: &'a [Json],
    at: BTreeMap<&'a str, usize>,
}

impl<'a> DocCmds<'a> {
    fn new(doc_json: Option<&'a Json>) -> Self {
        let list = doc_json
            .and_then(|d| d.get("commands"))
            .and_then(Json::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();
        let at = list
            .iter()
            .enumerate()
            .filter_map(|(i, c)| c.get("addr").and_then(Json::as_str).map(|a| (a, i)))
            .collect();
        DocCmds { list, at }
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

/// The line a `when=` line guard wraps (dsl §7.2/§7.4 desugar: a one-arm
/// match whose `$` test is the guard itself, whose arm plays exactly that
/// line and whose `otherwise` is empty). The transcript shows such a match
/// as the line or its skip — the synthetic match is compiler plumbing.
fn guarded_line<'a>(m: &Json, cmds: &DocCmds<'a>) -> Option<&'a Json> {
    let [arm] = m.get("arms")?.as_array()?.as_slice() else {
        return None;
    };
    let subject = str_of(m, "subject").trim();
    let test = str_of(arm, "test").trim();
    if subject.is_empty() || (test != subject && test != format!("({subject})")) {
        return None;
    }
    let converge = str_of(m, "converge");
    let jumps_to_converge =
        |c: Option<&Json>| c.is_some_and(|c| str_of(c, "kind") == "jump" && str_of(c, "target") == converge);
    let mut body = cmds.authored_from(str_of(arm, "target"));
    let line = body.next().filter(|c| str_of(c, "kind") == "line")?;
    (jumps_to_converge(body.next())
        && jumps_to_converge(cmds.authored_from(str_of(m, "otherwise")).next()))
    .then_some(line)
}

/// One runner transcript record -> a human line at source level, enriched
/// with the original command's authored attrs (looked up by `addr`).
/// `None` for a record that is not authored source: a staging record the
/// compiler injected (`provenance.injected`), or the synthetic match of a
/// line guard whose line played. `--json` keeps every record verbatim.
fn render_record(rec: &Json, cmds: &DocCmds<'_>) -> Option<String> {
    let kind = str_of(rec, "kind");
    let orig = cmds.get(str_of(rec, "addr"));
    Some(match kind {
        "line" => format!(
            "{}: {}",
            line_head(str_of(rec, "speaker"), orig),
            str_of(rec, "text")
        ),
        "background" | "music" | "sfx" | "vfx" | "sprite" | "camera" | "cut" | "video" => {
            if orig.is_some_and(is_injected) {
                return None;
            }
            let attrs = orig
                .map(|c| render_attrs(c, &["addr", "kind"]))
                .unwrap_or_default();
            if attrs.is_empty() {
                format!("::{kind}")
            } else {
                format!("::{kind}{{{attrs}}}")
            }
        }
        "set" => format!(
            "  set {} = {}",
            str_of(rec, "path"),
            rec.get("value").map(Json::to_string).unwrap_or_default()
        ),
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
        "match" => match orig.and_then(|m| guarded_line(m, cmds)) {
            // The guard held: the line record that follows is the output.
            Some(_) if str_of(rec, "result") == "arm 1" => return None,
            Some(line) => format!(
                "  skip {} \"{}\" — when: false",
                line_head(str_of(line, "speaker"), Some(line)),
                str_of(line, "text")
            ),
            None => format!("  match -> {}", str_of(rec, "result")),
        },
        "barrier" => "  barrier (no real clock simulated)".to_string(),
        "end" => match rec.get("reason").and_then(Json::as_str) {
            Some(r) => format!("  ::end reason={r}"),
            None => "  ::end".to_string(),
        },
        "plugin" => format!(
            "  plugin {} (external call, not invoked)",
            str_of(rec, "tag")
        ),
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
        "accept" => {
            let ignored = rec
                .get("ignored")
                .and_then(Json::as_str)
                .map(|s| format!(" ({s} — ignored)"))
                .unwrap_or_default();
            format!("  quest {} accepted{ignored}", str_of(rec, "quest"))
        }
        "objective" => format!(
            "  {}.{} {}",
            str_of(rec, "quest"),
            str_of(rec, "objective"),
            // dsl 0.23.0 §2: the objective's `by` came true first.
            if rec.get("failed").and_then(Json::as_bool) == Some(true) {
                "failed (by)"
            } else {
                "done"
            }
        ),
        "quest" => format!(
            "  quest {} -> {}",
            str_of(rec, "quest"),
            str_of(rec, "state")
        ),
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

fn render_records(out: &mut String, p: &Project, document: &str, records: &[Json]) {
    let cmds = DocCmds::new(p.artifacts.get(document));
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
        _ if rec.get("held").and_then(Json::as_bool) == Some(false) => {
            format!("  retract {} (did not hold)", str_of(rec, "pattern"))
        }
        _ => format!("  retract {}", str_of(rec, "pattern")),
    }
}

fn render_human(p: &Project, play: &Playthrough) -> String {
    let mut out = String::new();
    if !play.start.is_empty() {
        out.push_str(&format!("── start {RULE}\n"));
        for q in &play.start {
            render_records(&mut out, p, &q.document, &q.transcript);
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
            StepBody::NewRun { writes } => {
                out.push_str(&format!("{head} · new run {RULE}\n"));
                out.push_str("  run.* state, run-tier facts and once: run reset\n");
                for id in p.run_quests.keys() {
                    out.push_str(&format!("  quest {id} -> unset (tier: run)\n"));
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
            StepBody::Occasion {
                occasion,
                target,
                select,
                pick,
                candidates,
                winner,
                decided,
                presented,
            } => {
                let mut header = format!("{head} · {occasion}");
                if let Some(t) = target {
                    header.push_str(&format!(" → {t}"));
                }
                match select {
                    OccasionSelect::All => header.push_str(&format!(
                        " (select: all, pick: {})",
                        pick.as_ref().map(pick_label).unwrap_or("?")
                    )),
                    OccasionSelect::Sequence => header.push_str(" (select: sequence)"),
                    OccasionSelect::First => {}
                }
                out.push_str(&format!("{header} {RULE}\n"));
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
                let is_also =
                    |id: &str| candidates.iter().any(|c| c.also && c.id == id);
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
                        (_, None, _) if !presented.is_empty() => {
                            out.push_str("  → (no eligible main beat)\n")
                        }
                        (_, None, _) => {
                            out.push_str("  → (no eligible beat — the occasion passes)\n")
                        }
                    }
                    if *select == OccasionSelect::First {
                        for pr in presented.iter().filter(|pr| is_also(&pr.id)) {
                            out.push_str(&format!("  + {} (also)\n", pr.id));
                        }
                    }
                }
                for pr in presented {
                    render_records(&mut out, p, &pr.document, &pr.transcript);
                }
            }
        }
        for q in &s.quests {
            render_records(&mut out, p, &q.document, &q.transcript);
        }
    }
    match &play.outcome {
        Ok(reason) => out.push_str(&format!("── end: {reason} {RULE}\n")),
        Err(h) => out.push_str(&format!("── halted: {} {RULE}\n", h.message())),
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
            match &s.body {
                StepBody::NewRun { writes } => {
                    o.insert("newRun".into(), json!(true));
                    o.insert("seed".into(), json!(writes));
                }
                StepBody::Engine { writes } => {
                    o.insert("engine".into(), json!(writes));
                }
                StepBody::Event { event } => {
                    o.insert("event".into(), json!(event));
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
    root.insert("start".into(), json!({ "quests": quests_json(&play.start) }));
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
            o.insert("then".into(), Json::Array(rest.iter().map(pr_json).collect()));
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
    project: Project,
    plan: Vec<Step>,
    world: World,
}

/// Load a play. `Err((code, message))`: exit 2 with the usage error, or the
/// compile gate's exit code (its diagnostics already printed, `message`
/// empty).
fn load(dir: &Path, script_path: &Path, no_derive: bool) -> Result<Loaded, (ExitCode, String)> {
    let usage = |e: String| (ExitCode::from(2), e);
    let text = std::fs::read_to_string(script_path).map_err(|e| {
        usage(format!(
            "cannot read play script {}: {e}",
            script_path.display()
        ))
    })?;
    let script = parse_script(&text)
        .map_err(|e| usage(format!("invalid play script {}: {e}", script_path.display())))?;
    if !dir.is_dir() {
        return Err(usage(format!("{} is not a project directory", dir.display())));
    }
    let project = compile_project(dir).map_err(|code| (code, String::new()))?;
    let at = |e: String| usage(format!("{}: {e}", script_path.display()));
    let plan = plan_steps(&project, &script.steps).map_err(at)?;
    let mut world = seed_world(&project, &script).map_err(at)?;
    if no_derive {
        world.derive = Some(false);
    }
    Ok(Loaded {
        script,
        project,
        plan,
        world,
    })
}

/// What a play's expectations are judged against (dsl 0.22.0 §4): one row
/// per executed occasion step, and the world it ended in — effective
/// state, every fact after derivation, every declared quest's status.
fn play_outcome(p: &Project, play: &Playthrough, transcript: String) -> PlayOutcome {
    let steps = play
        .steps
        .iter()
        .filter_map(|s| match &s.body {
            StepBody::Occasion {
                occasion,
                target,
                candidates,
                winner,
                presented,
                ..
            } => Some(StepOutcome {
                index: s.n,
                label: s.label.clone(),
                occasion: occasion.clone(),
                target: target.clone(),
                winner: winner.clone(),
                offered: candidates
                    .iter()
                    .filter(|c| matches!(c.verdict, Verdict::Eligible))
                    .map(|c| c.id.clone())
                    .collect(),
                presented: presented.iter().map(|pr| pr.id.clone()).collect(),
            }),
            _ => None,
        })
        .collect();
    let w = &play.world;
    let facts = Runner::with_carryover(
        &p.eval_json,
        w.mock(),
        w.state.clone(),
        w.facts.clone(),
        w.quests.clone(),
    )
    .all_facts()
    .iter()
    .map(render_fact)
    .collect();
    let quests = p
        .quest_ids
        .iter()
        .map(|id| {
            let status = w.quests.get(id).map_or("unset", String::as_str);
            (id.clone(), status.to_string())
        })
        .collect();
    PlayOutcome {
        steps,
        state: w.state.clone(),
        facts,
        quests,
        transcript,
        exit: match &play.outcome {
            Ok(_) => "complete",
            Err(h) => h.exit_label(),
        },
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
    let play = execute(&project, &script, &plan, world);
    let human = render_human(&project, &play);
    let explained = if explain.is_empty() {
        None
    } else {
        let w = &play.world;
        match crate::explain::render(&project.rules, &project.seed_facts, &w.state, &w.facts, explain) {
            Ok(e) => Some(e),
            Err(e) => {
                eprintln!("lute play: --explain: {e}");
                return ExitCode::from(2);
            }
        }
    };
    let outcome = play_outcome(&project, &play, human);
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
        let mut text = outcome.transcript.clone();
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
        (Err(h @ (PlayHalt::Error(_) | PlayHalt::Fatal(_))), _) => h.exit_code(),
        (_, false) => ExitCode::from(1),
        (Ok(_), true) => ExitCode::SUCCESS,
        (Err(h), true) => h.exit_code(),
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

/// Run `script` over `project` in process for `lute test`. `Err` is a usage
/// or compile failure (the compile gate's diagnostics are already printed).
pub(crate) fn run_play_for_test(
    project: &Path,
    script: &Path,
    derive: bool,
) -> Result<PlayTestRun, String> {
    let Loaded {
        script,
        project: p,
        plan,
        world,
    } = load(project, script, !derive).map_err(|(_, msg)| {
        if msg.is_empty() {
            "the project does not compile (diagnostics above)".to_string()
        } else {
            msg
        }
    })?;
    let play = execute(&p, &script, &plan, world);
    let outcome = play_outcome(&p, &play, render_human(&p, &play));
    let misses = judge(&script, &outcome);
    // A presented beat's document, and a quest document whose lifecycle
    // transitioned or whose handlers played during the play.
    let presented_docs = play
        .steps
        .iter()
        .flat_map(|s| match &s.body {
            StepBody::Occasion { presented, .. } => {
                presented.iter().map(|pr| pr.document.clone()).collect()
            }
            _ => Vec::new(),
        })
        .chain(
            play.start
                .iter()
                .chain(play.steps.iter().flat_map(|s| &s.quests))
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
