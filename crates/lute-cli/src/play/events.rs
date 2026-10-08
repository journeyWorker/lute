//! `lute play --events` (spec 0.38.0 §10.2): a play script driven through
//! the public runtime's `begin` / `step`.
//!
//! The script is parsed and planned once, by the same code as `lute play`.
//! Its steps become runtime inputs; its awaits are answered with the
//! decisions and bridge answers `lute play`'s driver took for that script
//! (the `ScriptedChoices` cursor rules, `include:` scopes and per-tag bridge
//! queues apply in one place: [`PlayDriverState`]).

use std::collections::BTreeMap;
use std::path::Path;

use lute_runtime::runtime::{
    AdvanceBy, Await, ClockPosition, Input, Output, PickInput, Rejected, SaveInput, Seed, State,
    StateWrite, WritesInput,
};
use lute_runtime::session::{
    render_fact, value_to_json, ExecProject, Session, World, Write, Writes,
};
use lute_runtime::{BridgeAnswer, Runtime};
use serde_json::Value;

use super::driver::PlayDriverState;
use super::plan::Action;
use super::script::PlayScript;

/// One answer `lute play`'s driver gave while it played the script, in the
/// order it gave them.
#[derive(Clone, Debug)]
pub enum ScriptAnswer {
    /// A branch or hub decision: menu `menu`, option `option`.
    Choice { menu: String, option: String },
    /// A plugin call's bridge answer for `tag`.
    Bridge { tag: String, fields: BridgeAnswer },
}

/// What `lute play`'s report path produced for the script: its machine
/// records, in order, and the final world.
pub struct Reference {
    pub records: Vec<(String, Value)>,
    pub world: World,
}

/// A play script ready for the public runtime.
pub struct ScriptRun {
    pub runtime: Runtime,
    pub seed: Seed,
    /// One input per planned step repetition, up to an `end: true` step.
    pub inputs: Vec<Input>,
    /// The answers `lute play` gave, for the runtime's awaits.
    pub answers: Vec<ScriptAnswer>,
    pub reference: Reference,
}

/// Compile the project at `dir`, parse and plan the play script at
/// `script_path` exactly as `lute play` does, and play it once through
/// `lute play`'s driver to learn its answers. `Err` is `lute play`'s usage
/// or compile message.
pub fn script_run(dir: &Path, script_path: &Path, no_derive: bool) -> Result<ScriptRun, String> {
    let script = super::load_script(script_path).map_err(|(_, message)| message)?;
    let (project, needles) = super::project::compile_play_project(
        &lute_model::ModelMemo::default(),
        dir,
        super::project::PLAY,
        &crate::EngineMatrix::reference(),
    )
    .map_err(|(_, message)| {
        if message.is_empty() {
            format!("{} does not compile", dir.display())
        } else {
            message
        }
    })?;
    let (plan, world) =
        super::plan::plan_script(&project, &needles, &script, script_path, no_derive)
            .map_err(|(_, message)| message)?;
    let mut driver: PlayDriverState = super::driver_for(&project, &script, None);
    let played = super::run::execute(
        &script,
        &plan,
        Session::resume(&project, world, &mut driver),
    );
    let mut inputs = Vec::new();
    for step in &plan {
        let Some(input) = step_input(&step.action, &project) else {
            break;
        };
        inputs.extend(std::iter::repeat_n(input, step.repeat));
    }
    let seed = seed(&script, no_derive);
    Ok(ScriptRun {
        runtime: Runtime::from_project(project),
        seed,
        inputs,
        answers: driver.answers,
        reference: Reference {
            records: driver.records,
            world: played.world,
        },
    })
}

/// How [`drive_events`] stopped.
pub enum DriveEnd {
    /// Every input ran.
    Done,
    /// The runtime rejected `input`; the state is the one before it.
    Rejected { input: Input, rejected: Rejected },
    /// The runtime awaits an answer `lute play` never gave — the script
    /// has no decision or bridge answer for it.
    Unanswered(String),
    /// The runtime awaits something other than what `lute play` answered
    /// next: the two paths disagree, a runtime defect.
    Diverged(String),
}

/// The state and last output a driven script left, and why it stopped.
pub struct Driven {
    pub state: State,
    pub output: Output,
    pub end: DriveEnd,
}

/// Drive `run` through its runtime: `begin` with the seed, then each input,
/// answering every `awaitChoice` / `awaitBridge` with the next of the
/// script's answers. `observe` sees the first output with no input, then
/// every accepted input with the state and output it produced. `Err` when
/// `begin` rejects the seed.
pub fn drive_events(
    run: &ScriptRun,
    mut observe: impl FnMut(Option<&Input>, &State, &Output),
) -> Result<Driven, Rejected> {
    let runtime = &run.runtime;
    let (mut state, mut output) = runtime.begin(run.seed.clone())?;
    observe(None, &state, &output);
    let mut answers = run.answers.iter();
    for input in &run.inputs {
        let mut input = input.clone();
        loop {
            (state, output) = match runtime.step(state, input.clone()) {
                Ok(next) => next,
                Err((state, rejected)) => {
                    let end = DriveEnd::Rejected { input, rejected };
                    return Ok(Driven { state, output, end });
                }
            };
            observe(Some(&input), &state, &output);
            let answer = match &output.await_ {
                Await::Idle | Await::Ended { .. } | Await::Halted { .. } => break,
                Await::Choice { request, menu } => match answers.next() {
                    Some(ScriptAnswer::Choice { menu: id, option }) if *id == menu.id => {
                        Ok(Input::Choose {
                            request: *request,
                            option: option.clone(),
                        })
                    }
                    None => Err(DriveEnd::Unanswered(format!(
                        "{} `{}` has no scripted `choose:` decision",
                        menu.construct, menu.id
                    ))),
                    Some(other) => Err(DriveEnd::Diverged(format!(
                        "the runtime awaits a choice at {} `{}`; `lute play` answered {other:?}",
                        menu.construct, menu.id
                    ))),
                },
                Await::Bridge {
                    request,
                    tag,
                    fields: shape,
                    ..
                } => match answers.next() {
                    Some(ScriptAnswer::Bridge {
                        tag: answered,
                        fields,
                    }) if answered == tag => Ok(Input::BridgeResult {
                        request: *request,
                        fields: bridge_fields(fields, shape),
                    }),
                    None => Err(DriveEnd::Unanswered(format!(
                        "plugin call `{tag}` has no scripted `bridges:` answer"
                    ))),
                    Some(other) => Err(DriveEnd::Diverged(format!(
                        "the runtime awaits a bridge answer for `{tag}`; `lute play` answered \
                         {other:?}"
                    ))),
                },
            };
            match answer {
                Ok(answer) => input = answer,
                Err(end) => return Ok(Driven { state, output, end }),
            }
        }
    }
    Ok(Driven {
        state,
        output,
        end: DriveEnd::Done,
    })
}

/// `lute play --events`: the JSON Lines stream of a driven script, its exit
/// code — 0 when it completes, 3 when the runtime halts incomplete or awaits
/// an answer the script lacks, 1 on a rejected input, an error halt or a
/// divergence — and a message for stderr.
pub fn run_events(run: &ScriptRun) -> (String, u8, Option<String>) {
    let line =
        |value: Value| serde_json::to_string(&value).expect("runtime JSON serializes") + "\n";
    let mut out = String::new();
    let seed = &run.seed;
    let driven = drive_events(run, |input, _, output| {
        out.push_str(&line(match input {
            Some(input) => serde_json::json!({ "input": input, "output": output }),
            None => serde_json::json!({ "seed": seed, "output": output }),
        }));
    });
    let (code, message) = match driven {
        Err(rejected) => {
            out.push_str(&line(
                serde_json::json!({ "seed": seed, "rejected": rejected }),
            ));
            (1, None)
        }
        Ok(Driven { end, output, .. }) => match end {
            DriveEnd::Rejected { input, rejected } => {
                out.push_str(&line(
                    serde_json::json!({ "input": input, "rejected": rejected }),
                ));
                (1, None)
            }
            DriveEnd::Unanswered(what) => (3, Some(what)),
            DriveEnd::Diverged(what) => (1, Some(what)),
            DriveEnd::Done => match output.await_ {
                Await::Halted { kind, .. } if kind == "incomplete" => (3, None),
                Await::Halted { .. } => (1, None),
                _ => (0, None),
            },
        },
    };
    (out, code, message)
}

/// A planned step as a runtime input; `None` for `end: true`.
fn step_input(action: &Action, project: &ExecProject) -> Option<Input> {
    Some(match action {
        Action::Occasion {
            occasion,
            target,
            pick,
            payload,
            writes,
            ..
        } => Input::RaiseOccasion {
            occasion: occasion.clone(),
            target: target.clone(),
            payload: payload
                .iter()
                .map(|(path, value)| {
                    let field = path.strip_prefix("occasion.payload.").unwrap_or(path);
                    (field.to_string(), value_to_json(value))
                })
                .collect(),
            pick: pick.as_ref().map(pick_input),
            writes: writes.as_ref().map(writes_input).unwrap_or_default(),
        },
        Action::Advance {
            by, writes, pick, ..
        } => Input::AdvanceClock {
            by: advance_by(by, project),
            writes: writes_input(writes),
            pick: pick.as_ref().map(pick_input),
        },
        Action::Engine(writes) => Input::HostWrite {
            writes: writes_input(writes),
        },
        Action::Event(name) => Input::WorldEvent { name: name.clone() },
        Action::NewRun(writes) => Input::NewRun {
            writes: writes_input(writes),
        },
        Action::End => return None,
    })
}

fn pick_input(pick: &lute_runtime::session::Pick) -> PickInput {
    match pick {
        lute_runtime::session::Pick::Pass => PickInput::Pass("pass".into()),
        lute_runtime::session::Pick::Beat(beat) => PickInput::Beat { beat: beat.clone() },
    }
}

fn writes_input(writes: &Writes) -> WritesInput {
    WritesInput {
        state: writes
            .state
            .iter()
            .map(|(path, write)| match write {
                Write::Set(value) => StateWrite {
                    path: path.clone(),
                    value: Some(value_to_json(value)),
                    add: None,
                },
                Write::Add(add) => StateWrite {
                    path: path.clone(),
                    value: None,
                    add: Some(*add),
                },
            })
            .collect(),
        facts: writes.facts.iter().map(render_fact).collect(),
        retract: writes.retract.iter().map(render_fact).collect(),
        accept: writes.accept.clone(),
    }
}

/// A resolved `advance:` in the wire form (spec 0.38.0 §5.4): slots and
/// weekdays by name where the clock names them.
fn advance_by(by: &lute_manifest::clock::Advance, project: &ExecProject) -> AdvanceBy {
    use lute_manifest::clock::Advance;
    let clock = project.index.clock.as_ref();
    match *by {
        Advance::Slots(1) => AdvanceBy::Named("slot".into()),
        Advance::Slots(n) => AdvanceBy::Slots(n),
        Advance::Day => AdvanceBy::Named("day".into()),
        Advance::To { weekday, slot } => {
            let slot = slot.map(|i| {
                clock
                    .and_then(|c| c.slots.get(i))
                    .cloned()
                    .expect("the planner resolved the slot against this clock")
            });
            let weekday = weekday.map(|i| {
                clock
                    .and_then(|c| c.week.as_ref())
                    .and_then(|w| usize::try_from(i).ok().and_then(|i| w.labels.get(i)))
                    .cloned()
                    .unwrap_or_else(|| i.to_string())
            });
            match (weekday, slot) {
                (None, Some(to)) => AdvanceBy::ToSlot { to },
                (weekday, slot) => AdvanceBy::To {
                    to: ClockPosition { weekday, slot },
                },
            }
        }
    }
}

/// The script's seed: its `state:` / `facts:` surfaces and save, as written.
fn seed(script: &PlayScript, no_derive: bool) -> Seed {
    let save = &script.save;
    let scoped = |run: &[String], user: &[String]| {
        [("run", run), ("user", user)]
            .into_iter()
            .filter(|(_, ids)| !ids.is_empty())
            .map(|(scope, ids)| (scope.to_string(), ids.to_vec()))
            .collect()
    };
    Seed {
        state: script
            .surfaces
            .state
            .iter()
            .map(|(path, text, _)| StateWrite {
                path: path.clone(),
                value: Some(scalar(text)),
                add: None,
            })
            .collect(),
        facts: script.surfaces.facts.clone(),
        derive: !no_derive && script.derive != Some(false),
        save: SaveInput {
            visited: save.visited.clone(),
            presented: scoped(&save.presented_run, &save.presented_user),
            quests: save.quests.iter().cloned().collect(),
            quest_instances: save.quest_instances.iter().cloned().collect(),
            entries_read: scoped(&save.entries_run, &save.entries_user),
        },
    }
}

/// A literal as written, as a JSON scalar: a number or boolean when its
/// JSON form reads back as the same text, else a string.
fn scalar(text: &str) -> Value {
    match serde_json::from_str::<Value>(text) {
        Ok(v @ (Value::Bool(_) | Value::Number(_))) if v.to_string() == text => v,
        _ => Value::String(text.to_string()),
    }
}

/// A bridge answer as `bridgeResult` fields, each typed by the await's
/// field shape (`bool`, `number`, else string).
fn bridge_fields(
    answer: &BridgeAnswer,
    shape: &BTreeMap<String, String>,
) -> BTreeMap<String, Value> {
    answer
        .iter()
        .map(|(field, text)| {
            let value = match shape.get(field).map(String::as_str) {
                Some("bool" | "number") => match scalar(text) {
                    Value::String(_) => Value::String(text.clone()),
                    v => v,
                },
                _ => Value::String(text.clone()),
            };
            (field.clone(), value)
        })
        .collect()
}
