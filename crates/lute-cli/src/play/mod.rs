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
//! the [`PlayOutcome`](crate::play_expect::PlayOutcome) the walk fills; a miss exits 1.
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

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::io::Write;
use std::path::Path;
use std::process::ExitCode;
use std::rc::Rc;

use lute_trace::exec::session::{
    domain_members, entry_flag, is_candidate, json_to_value, kind_label, resolve_fact,
    resolve_state, value_to_json, ExecProject, PlayHalt, Played, Presented, Session,
    SessionEvalObserver, StepBody, Verdict, World,
};
use serde_json::{json, Value as Json};

use crate::play_expect::ExpectMiss;

pub(crate) mod calendar;
mod human;
mod json;
mod outcome;
mod plan;
mod project;
mod provenance;
mod run;
mod script;

use human::{render_human, View, RULE};
use json::render_json;
use outcome::{judge, play_outcome, played_choices, PlayedChoice};
use plan::{locate_step_error, plan_script, plan_steps, Step};
use project::{compile_play_project, compile_project};
use provenance::fact_origins;
use run::execute;
use script::{parse_script, parse_script_with, PlayScript, ScriptStep};
pub(crate) use script::{SCRIPT_KEYS, STEP_KEYS};

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
    let project = compile_play_project(dir, project::PLAY)?;
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
        let e = lute_manifest::io_reason(&e);
        usage(format!(
            "cannot read play script {}: {e}",
            script_path.display()
        ))
    })?;
    parse_script(&text, script_path).map_err(usage)
}

fn install_condition_dump(
    project: &ExecProject,
    world: &mut World,
    path: &Path,
) -> Result<(), String> {
    let file = std::fs::File::create(path)
        .map_err(|e| format!("cannot create condition dump {}: {e}", path.display()))?;
    let file = Rc::new(RefCell::new(file));
    let mut env = project
        .eval_json
        .get("celEnv")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let roots: BTreeSet<String> = project
        .state_table
        .keys()
        .filter_map(|path| path.split('.').next())
        .map(str::to_string)
        .chain(["prev", "occasion", "clock", "entry"].into_iter().map(str::to_string))
        .collect();
    if let Json::Object(map) = &mut env {
        let variables = map.entry("variables").or_insert_with(|| json!([]));
        if let Json::Array(variables) = variables {
            for root in roots {
                if !variables.iter().any(|v| v.get("name").and_then(Json::as_str) == Some(&root)) {
                    variables.push(json!({"name": root, "type": "map(string, dyn)"}));
                }
            }
        }
    }
    if let Ok(mut f) = file.try_borrow_mut() {
        serde_json::to_writer(&mut *f, &json!({"kind": "env", "env": env}))
            .map_err(|e| format!("cannot write condition dump header: {e}"))?;
        use std::io::Write;
        writeln!(f).map_err(|e| format!("cannot write condition dump header: {e}"))?;
    }
    let defaults: std::collections::BTreeMap<String, lute_trace::Value> = project
        .state_table
        .iter()
        .filter_map(|(path, entry)| {
            entry.get("default").and_then(json_to_value).map(|v| (path.clone(), v))
        })
        .collect();
    let artifacts: Vec<Json> = project.artifacts.values().cloned().collect();
    let observer: SessionEvalObserver = Rc::new(move |raw, value, _atoms, snapshot| {
        let expr = artifacts
            .iter()
            .find_map(|artifact| crate::runner::find_expr(artifact, raw))
            .unwrap_or(Json::Null);
        let (_roots, relations, needs_visited) = crate::runner::condition_scope(&expr);
        let paths: BTreeSet<String> = snapshot.reads.iter().map(|(p, _)| p.clone()).collect();
        let mut dump_state = snapshot.state.clone();
        for (path, read) in snapshot.reads {
            if let lute_trace::Read::Value(value) = read {
                dump_state.insert(path.clone(), value.clone());
            }
        }
        for (id, status) in snapshot.quest_status {
            dump_state.insert(
                format!("quest.{id}.state"),
                lute_trace::Value::Str(status.clone()),
            );
        }
        if let Some(target) = snapshot.occasion_target {
            dump_state.insert(
                "occasion.target".to_string(),
                lute_trace::Value::Str(target.to_string()),
            );
        }
        for (path, default) in &defaults {
            dump_state.entry(path.clone()).or_insert_with(|| default.clone());
        }
        let mut line = json!({
            "cel": raw,
            "expr": expr,
            "activation": crate::runner::activation_json_paths(&dump_state, snapshot.state_types, &paths),
            "facts": crate::runner::condition_facts(&snapshot.facts, &relations),
            "result": crate::runner::typed_value(value),
        });
        if needs_visited {
            line["visited"] = snapshot.visited.iter().cloned().collect::<Vec<_>>().into();
        }
        if let Ok(mut f) = file.try_borrow_mut() {
            let _ = serde_json::to_writer(&mut *f, &line);
            let _ = writeln!(f);
        }
    });
    world.eval_observer = Some(observer);
    Ok(())
}

/// See [`crate::Command::Play`].
pub fn run_play(
    dir: &Path,
    script_path: &Path,
    json: bool,
    no_derive: bool,
    explain: &[String],
    ir: bool,
    quiet: bool,
    dump_conditions: Option<&Path>,
) -> ExitCode {
    let Loaded {
        script,
        project,
        plan,
        mut world,
    } = match load(dir, script_path, no_derive) {
        Ok(l) => l,
        Err((code, msg)) => {
            // Every usage error of the script, one per line.
            for line in msg.lines() {
                eprintln!("lute play: {}", lute_core_span::plain_message(line));
            }
            return code;
        }
    };
    if let Some(path) = dump_conditions {
        if let Err(msg) = install_condition_dump(&project, &mut world, path) {
            eprintln!("lute play: {msg}");
            return ExitCode::from(2);
        }
    }
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
                eprintln!(
                    "lute play: --explain: {}",
                    lute_core_span::plain_message(&e)
                );
                return ExitCode::from(2);
            }
        }
    };
    let outcome = play_outcome(&project, &play);
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
        let mut text = render_human(
            &project,
            &play,
            View {
                ir,
                quiet,
                ended_game: false,
            },
        );
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
    /// How the play ended: `complete | terminal | incomplete | error`.
    pub end: &'static str,
    /// Project-relative (forward-slash) source paths of every document a
    /// presentation came from — `--coverage`'s numerator.
    pub presented_docs: BTreeSet<String>,
    /// T3-20: every beat the play presented, as `(document, id)` — the id
    /// the project index gives it (a scene's key, an entry's id, a bundle
    /// beat's `<document id>.<beat id>`): `--coverage`'s per-beat units.
    pub presented: BTreeSet<(String, String)>,
    /// T3-20: every branch/hub the play answered — `--coverage`'s choice
    /// rows, fed the same table trace's paths feed.
    pub choices: Vec<PlayedChoice>,
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
        PlayProject(
            compile_play_project(dir, project::TEST).map_err(|(_, msg)| {
                if msg.is_empty() {
                    "the project does not compile (diagnostics above)".to_string()
                } else {
                    msg
                }
            }),
        )
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
    let outcome = play_outcome(&p, &play);
    let misses = judge(&script, &outcome);
    // A presented beat — an `occasion:` step's, or one the occasion an
    // `advance:` step's clock raised — and a quest document whose lifecycle
    // transitioned or whose handlers played during the play.
    let mut beats: Vec<&Presented> = Vec::new();
    for s in &play.steps {
        if let Some(StepBody::Occasion { presented, .. }) = s.body.occasion() {
            for pr in presented {
                collect_presented(pr, &mut beats);
            }
        }
        for played in s.body.days_played() {
            if let Played::Beat(pr) = played {
                collect_presented(pr, &mut beats);
            }
        }
    }
    let quests: Vec<(&str, &[Json])> = play
        .start
        .iter()
        .chain(
            play.steps
                .iter()
                .flat_map(|s| s.body.settled().chain(&s.quests)),
        )
        .map(|q| (q.document.as_str(), q.transcript.as_slice()))
        .chain(play.steps.iter().flat_map(|s| {
            s.body
                .days_played()
                .into_iter()
                .filter_map(|played| match played {
                    Played::Quest(q) => Some((q.document.as_str(), q.transcript.as_slice())),
                    Played::Beat(_) => None,
                })
        }))
        .collect();
    let presented_docs = beats
        .iter()
        .map(|pr| pr.document.clone())
        .chain(quests.iter().map(|(doc, _)| doc.to_string()))
        .collect();
    let presented = beats
        .iter()
        .map(|pr| (pr.document.clone(), pr.id.clone()))
        .collect();
    let mut choices = Vec::new();
    for (document, transcript) in beats
        .iter()
        .map(|pr| (pr.document.as_str(), pr.transcript.as_slice()))
        .chain(quests.iter().copied())
    {
        played_choices(p, document, transcript, &mut choices);
    }
    Ok(PlayTestRun {
        misses,
        // The exit class of how the play ended: a terminal ending completed.
        exit: match outcome.ended {
            "terminal" => "complete",
            ended => ended,
        },
        end: outcome.ended,
        presented_docs,
        presented,
        choices,
        notes: play
            .outcome
            .as_ref()
            .err()
            .map(|h| h.message().to_string())
            .into_iter()
            .collect(),
    })
}

fn collect_presented<'a>(beat: &'a Presented, out: &mut Vec<&'a Presented>) {
    out.push(beat);
    for raised in &beat.raised {
        collect_presented(raised, out);
    }
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

/// Run the play at `script_path` over the project at `dir` in process and
/// hand back every presentation it made, in order (an `advance:`'s midnight
/// raises before its slot raise). A script without `steps:` is a save — what
/// `lute calendar --script` starts every cell from, read the calendar's way —
/// and presents nothing.
#[cfg(test)]
pub(crate) fn presentations_for_diff(
    dir: &Path,
    script_path: &Path,
) -> Result<Vec<PlayedBeat>, String> {
    let text = std::fs::read_to_string(script_path)
        .map_err(|e| format!("cannot read play script: {}", lute_manifest::io_reason(&e)))?;
    let script = parse_script_with(&text, script_path, false)?;
    if script.steps.is_empty() {
        return Ok(Vec::new());
    }
    let project = compile_play_project(dir, project::PLAY).map_err(|(_, msg)| msg)?;
    let (plan, world) =
        plan_script(&project, &script, script_path, false).map_err(|(_, msg)| msg)?;
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
        let mut presentations = Vec::new();
        for pr in beats {
            collect_presented(pr, &mut presentations);
        }
        for pr in presentations {
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
