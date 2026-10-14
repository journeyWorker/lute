//! The public runtime (spec 0.38.0 §§3, 5–8, §12.2): every play script
//! driven through `begin` / `step` against `lute play`'s report path, and
//! snapshot / restore at its input points.

use std::fs;
use std::path::{Path, PathBuf};

use lute_cli::events::{self, DriveEnd, ScriptRun};
use lute_runtime::runtime::{Await, Event, Input, Output, Seed, State};
use lute_runtime::Runtime;
use serde_json::Value;
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn session_script(case: &str) -> (PathBuf, PathBuf) {
    let project = root()
        .join("conformance/session")
        .join(case)
        .join("project");
    let script = project.join("script.play.yaml");
    (project, script)
}

/// The project a play script plays: the nearest directory above it with a
/// `lute.project.yaml`.
fn project_of(script: &Path) -> PathBuf {
    script
        .ancestors()
        .skip(1)
        .find(|dir| dir.join("lute.project.yaml").is_file())
        .expect("a play script lives in a project")
        .to_path_buf()
}

fn session_run(case: &str) -> ScriptRun {
    let (project, script) = session_script(case);
    events::script_run(&project, &script, false).unwrap()
}

/// Every play script of the session conformance cases and the examples.
fn all_scripts() -> Vec<PathBuf> {
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.to_string_lossy().ends_with(".play.yaml") {
                out.push(path);
            }
        }
    }
    let mut out = Vec::new();
    walk(&root().join("conformance/session"), &mut out);
    walk(&root().join("docs/examples"), &mut out);
    out.sort();
    out
}

fn records(output: &Output) -> impl Iterator<Item = (String, Value)> + '_ {
    output.events.iter().filter_map(|event| match event {
        Event::Record { document, record } => Some((document.clone(), record.clone())),
        _ => None,
    })
}

/// One driven run: every state after an output, the input that led to it
/// (`None` for `begin`) and the output, plus why the drive stopped.
struct Points {
    points: Vec<(Option<Input>, State, Output)>,
    end: DriveEnd,
}

fn drive(run: &ScriptRun, script: &Path) -> Points {
    let mut points = Vec::new();
    let driven = events::drive_events(run, |input, state, output| {
        points.push((input.cloned(), state.clone(), output.clone()));
    })
    .unwrap_or_else(|r| {
        panic!(
            "{}: begin rejected: {} {}",
            script.display(),
            r.code,
            r.message
        )
    });
    Points {
        points,
        end: driven.end,
    }
}

fn json<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value).unwrap().to_string()
}

/// §12.2 (b): the public step path presents the same records and leaves the
/// same world as `lute play`'s report path.
#[test]
fn play_scripts_match_resumable_runtime() {
    let (mut exercised, mut skipped) = (0, Vec::new());
    for script in all_scripts() {
        let run = match events::script_run(&project_of(&script), &script, false) {
            Ok(run) => run,
            Err(why) => {
                skipped.push(format!("{} ({why})", script.display()));
                continue;
            }
        };
        let driven = drive(&run, &script);
        match &driven.end {
            DriveEnd::Done | DriveEnd::Unanswered(_) => {}
            DriveEnd::Rejected { input, rejected } => {
                panic!(
                    "{}: {input:?} rejected: {} {}",
                    script.display(),
                    rejected.code,
                    rejected.message
                )
            }
            DriveEnd::Diverged(what) => panic!("{}: {what}", script.display()),
        }
        let actual: Vec<(String, Value)> = driven
            .points
            .iter()
            .flat_map(|(_, _, output)| records(output))
            .collect();
        let expected = &run.reference.records;
        if let Some(i) =
            (0..actual.len().max(expected.len())).find(|&i| actual.get(i) != expected.get(i))
        {
            panic!(
                "{}: record {i} differs\n runtime: {:?}\n    play: {:?}",
                script.display(),
                actual.get(i),
                expected.get(i)
            );
        }
        let world = &driven.points.last().unwrap().1.world;
        assert_eq!(
            json(world.as_ref()),
            json(&run.reference.world),
            "world of {}",
            script.display()
        );
        exercised += 1;
    }
    println!(
        "runtime_step: exercised={exercised}, skipped={}",
        skipped.len()
    );
    for why in &skipped {
        println!("SKIP {why}");
    }
    assert!(
        exercised >= 80,
        "only {exercised} scripts exercised; skipped: {skipped:#?}"
    );
}

/// §7.2 / §12.2 (c): a snapshot taken at an input point — suspended
/// mid-input included — round-trips through JSON text, and the run that
/// continues from the restored state produces the same outputs.
#[test]
fn snapshot_and_restore_at_input_points() {
    let (mut scripts, mut points, mut suspended) = (0, 0, 0);
    for script in all_scripts() {
        let Ok(run) = events::script_run(&project_of(&script), &script, false) else {
            continue;
        };
        let runtime = &run.runtime;
        let driven = drive(&run, &script);
        let example = !script.starts_with(root().join("conformance"));
        for (k, (_, state, _)) in driven.points.iter().enumerate() {
            let waiting = state.continuation.is_some();
            // Every point of the session cases; every third point and every
            // suspended one of the examples (debug-build time).
            if example && !waiting && k % 3 != 0 {
                continue;
            }
            let text = serde_json::to_string(&runtime.snapshot(state)).unwrap();
            let restored = runtime
                .restore(lute_runtime::runtime::Snapshot::from_json(text.as_str()).unwrap())
                .unwrap_or_else(|r| panic!("{} @{k}: {} {}", script.display(), r.code, r.message));
            assert_eq!(
                serde_json::to_string(&runtime.snapshot(&restored)).unwrap(),
                text,
                "{} @{k}: re-snapshot",
                script.display()
            );
            let mut state = restored;
            for (input, _, expected) in &driven.points[k + 1..] {
                let input = input.clone().expect("only begin has no input");
                let (next, output) = runtime.step(state, input).unwrap_or_else(|(_, r)| {
                    panic!("{} @{k}: {} {}", script.display(), r.code, r.message)
                });
                assert_eq!(json(&output), json(expected), "{} @{k}", script.display());
                state = next;
            }
            points += 1;
            suspended += usize::from(waiting);
        }
        scripts += 1;
    }
    println!("runtime_snapshot: scripts={scripts}, points={points}, suspended={suspended}");
    assert!(suspended > 0, "no suspended state was snapshotted");
}

/// `snapshot-hub`'s first raise, suspended at its first choice: the state,
/// the request and the menu's options with their verdicts.
fn suspended_hub(run: &ScriptRun) -> (State, u64, Vec<(String, String)>) {
    let (state, _) = run.runtime.begin(run.seed.clone()).unwrap();
    let (state, output) = run
        .runtime
        .step(state, run.inputs[0].clone())
        .unwrap_or_else(|(_, r)| panic!("{} {}", r.code, r.message));
    let Await::Choice { request, menu } = output.await_ else {
        panic!("snapshot-hub's first raise does not await a choice: {output:?}");
    };
    let options = menu
        .options
        .into_iter()
        .map(|o| (o.id, o.verdict))
        .collect();
    (state, request, options)
}

/// §5.3 / §12.2 (d): a rejected input changes nothing.
#[test]
fn rejected_inputs_leave_the_state_unchanged() {
    let run = session_run("snapshot-hub");
    let runtime: &Runtime = &run.runtime;
    let (idle, _) = runtime.begin(Seed::default()).unwrap();
    let (waiting, request, options) = suspended_hub(&run);
    let open = options
        .iter()
        .find(|(_, v)| v == "open")
        .map(|(id, _)| id.clone())
        .unwrap();
    let cases = [
        (
            &idle,
            Input::Choose {
                request: 1,
                option: open.clone(),
            },
            "E-RUNTIME-BUSY",
        ),
        (
            &waiting,
            Input::Choose {
                request: request + 1,
                option: open.clone(),
            },
            "E-RUNTIME-REQUEST",
        ),
        (
            &waiting,
            Input::Choose {
                request,
                option: "noSuchOption".into(),
            },
            "E-RUNTIME-OPTION",
        ),
        (&waiting, run.inputs[0].clone(), "E-RUNTIME-BUSY"),
        (
            &waiting,
            Input::WorldEvent {
                name: "anything".into(),
            },
            "E-RUNTIME-BUSY",
        ),
    ];
    for (state, input, code) in cases {
        let before = serde_json::to_string(&runtime.snapshot(state)).unwrap();
        let Err((returned, rejected)) = runtime.step(state.clone(), input.clone()) else {
            panic!("{input:?} was accepted");
        };
        assert_eq!(rejected.code, code, "{input:?}: {}", rejected.message);
        assert_eq!(
            serde_json::to_string(&runtime.snapshot(&returned)).unwrap(),
            before,
            "{input:?} changed the state"
        );
    }
}

/// §7.2: a snapshot is restored only by the bundle and contract minor that
/// took it.
#[test]
fn a_snapshot_from_another_project_or_minor_is_refused() {
    let hub = session_run("snapshot-hub");
    let other = session_run("occasion-first");
    let (state, _, _) = suspended_hub(&hub);
    let snapshot = hub.runtime.snapshot(&state);
    let r = other
        .runtime
        .restore(snapshot.clone())
        .expect_err("another project restored it");
    assert_eq!(r.code, "E-RUNTIME-SNAPSHOT-PROJECT");
    let mut older = snapshot;
    older.snapshot_version = "0.37.0".into();
    let r = hub
        .runtime
        .restore(older)
        .expect_err("another minor restored it");
    assert_eq!(r.code, "E-RUNTIME-SNAPSHOT-VERSION");
}

/// A case's first raise, suspended at its first choice.
fn first_choice(run: &ScriptRun) -> (State, u64, lute_runtime::runtime::MenuAwait) {
    let (state, _) = run.runtime.begin(run.seed.clone()).unwrap();
    let (state, output) = run
        .runtime
        .step(state, run.inputs[0].clone())
        .unwrap_or_else(|(_, r)| panic!("{} {}", r.code, r.message));
    let Await::Choice { request, menu } = output.await_ else {
        panic!("the first raise does not await a choice: {output:?}");
    };
    (state, request, menu)
}

/// dsl 0.40.0 §4.2: a timed menu carries its timeout and default, and the
/// `timeout` input is the choice of the default.
#[test]
fn a_timeout_chooses_the_default() {
    let run = session_run("timed-choice");
    let (state, request, menu) = first_choice(&run);
    assert_eq!(menu.timeout, Some(5));
    assert_eq!(menu.default.as_deref(), Some("wait"));
    let (timed_out, by_timeout) = run
        .runtime
        .step(state.clone(), Input::Timeout { request })
        .unwrap_or_else(|(_, r)| panic!("{} {}", r.code, r.message));
    let (chosen, by_choice) = run
        .runtime
        .step(
            state,
            Input::Choose {
                request,
                option: "wait".into(),
            },
        )
        .unwrap_or_else(|(_, r)| panic!("{} {}", r.code, r.message));
    assert_eq!(json(&by_timeout), json(&by_choice));
    let world =
        |state: &State| serde_json::to_value(run.runtime.snapshot(state)).unwrap()["world"].clone();
    assert_eq!(world(&timed_out), world(&chosen));
}

/// dsl 0.40.0 §4.2: `timeout` is refused on an untimed menu and on a stale
/// request, and a refusal changes nothing.
#[test]
fn a_timeout_is_refused_off_a_timed_choice() {
    let hub = session_run("snapshot-hub");
    let (untimed, request, _) = suspended_hub(&hub);
    let timed = session_run("timed-choice");
    let (waiting, timed_request, _) = first_choice(&timed);
    let cases = [
        (
            &hub,
            &untimed,
            Input::Timeout { request },
            "E-RUNTIME-TIMEOUT",
        ),
        (
            &timed,
            &waiting,
            Input::Timeout {
                request: timed_request + 1,
            },
            "E-RUNTIME-REQUEST",
        ),
    ];
    for (run, state, input, code) in cases {
        let before = serde_json::to_string(&run.runtime.snapshot(state)).unwrap();
        let Err((returned, rejected)) = run.runtime.step(state.clone(), input.clone()) else {
            panic!("{input:?} was accepted");
        };
        assert_eq!(rejected.code, code, "{input:?}: {}", rejected.message);
        assert_eq!(
            serde_json::to_string(&run.runtime.snapshot(&returned)).unwrap(),
            before,
            "{input:?} changed the state"
        );
    }
}

/// dsl 0.40.0 §4.1: the runtime stage reduces the staging records, survives
/// a snapshot, and drops what `stops`, exits and `display="hide"` end.
#[test]
fn the_stage_follows_the_staging_records() {
    let run = session_run("stage-snapshot");
    let runtime = &run.runtime;
    let (state, request, _) = first_choice(&run);
    let stage = runtime.stage(&state);
    let bg = stage.bg.as_ref().expect("a background");
    assert_eq!(
        (
            bg.location.as_deref(),
            bg.time.as_deref(),
            bg.asset_id.as_deref()
        ),
        (Some("parlor"), Some("evening"), Some("BG.parlor.evening"))
    );
    let music = stage.music.as_ref().expect("music");
    assert_eq!(
        (
            music.playback.as_deref(),
            music.mood.as_deref(),
            music.asset_id.as_deref()
        ),
        (Some("start"), Some("calm"), Some("BGM.parlor"))
    );
    let actors: Vec<_> = stage
        .actors
        .iter()
        .map(|a| {
            (
                a.character.as_str(),
                a.anchor.as_deref(),
                a.emotion.as_deref(),
                a.costume.as_deref(),
            )
        })
        .collect();
    assert_eq!(
        actors,
        [("mira", Some("left"), Some("smile"), Some("casual"))]
    );
    let cg = stage.cg.as_ref().expect("a CG");
    assert_eq!(
        (cg.asset_id.as_str(), cg.layout.as_deref()),
        ("CG.parlor", Some("full"))
    );
    assert!(stage.video.is_none());

    let text = serde_json::to_string(&runtime.snapshot(&state)).unwrap();
    let restored = runtime
        .restore(lute_runtime::runtime::Snapshot::from_json(text.as_str()).unwrap())
        .unwrap_or_else(|r| panic!("{} {}", r.code, r.message));
    assert_eq!(runtime.stage(&restored), stage);

    let (after, _) = runtime
        .step(
            restored,
            Input::Choose {
                request,
                option: "yes".into(),
            },
        )
        .unwrap_or_else(|(_, r)| panic!("{} {}", r.code, r.message));
    let end = runtime.stage(&after);
    assert_eq!(end.bg, stage.bg, "the background stays");
    assert!(end.music.is_none(), "a `stops` member ends the music");
    assert!(
        end.actors.is_empty(),
        "an `exits` action takes the actor off"
    );
    assert!(end.cg.is_none(), "`display=\"hide\"` clears the CG");
}
