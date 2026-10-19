//! dsl 0.41.0 §5.4: `advanceClock { writes }` applies its writes (and
//! settles) before the clock moves, so the raises the move makes read them;
//! if the game is over then, the clock stays and the input ends; the
//! `clock` event follows every event of the move. A beat's `advances:`
//! moves the clock after its `presentationEnd`, then emits `clock`. `lute
//! play`'s `advance:` + `engine:` step follows the same order.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use lute_runtime::index::Bundle;
use lute_runtime::runtime::{Await, Event, Input, Output, Seed, State};
use lute_runtime::Runtime;
use serde_json::{json, Value as Json};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime-clock-041")
}

/// `lute compile --all` of the fixture, read back as a host reads it.
fn runtime(scratch: &str) -> Runtime {
    let scratch = lute_scratch::ScratchDir::named(scratch);
    let out = scratch.join("bundle");
    let status = Command::new(BIN)
        .args(["compile", "--project"])
        .arg(fixture())
        .arg("-o")
        .arg(&out)
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "{}",
        String::from_utf8_lossy(&status.stderr)
    );
    let read = |path: &Path| -> Json {
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    };
    let index = read(&out.join("project.index.json"));
    let artifacts: BTreeMap<String, Json> = index["documents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|doc| {
            let artifact = doc["artifact"].as_str().unwrap().to_string();
            let json = read(&out.join(&artifact));
            (artifact, json)
        })
        .collect();
    Runtime::load(Bundle::new(artifacts, index)).unwrap()
}

fn step(runtime: &Runtime, state: &State, value: Json) -> (State, Output) {
    runtime
        .step(
            state.clone(),
            serde_json::from_value::<Input>(value).unwrap(),
        )
        .unwrap_or_else(|(_, r)| panic!("{} {}", r.code, r.message))
}

/// The output's events in brief: `presentation <beat>`, `end <beat>`,
/// `clock d.s->d.s`, and each line's text.
fn brief(output: &Output) -> Vec<String> {
    output
        .events
        .iter()
        .filter_map(|event| match event {
            Event::Presentation { beat, .. } => Some(format!("presentation {beat}")),
            Event::PresentationEnd { beat, .. } => Some(format!("end {beat}")),
            Event::Clock { from, to, .. } => Some(format!(
                "clock {}.{}->{}.{}",
                from["day"], from["slot"], to["day"], to["slot"]
            )),
            Event::Record { record, .. } if record["kind"] == "line" => {
                Some(record["text"].as_str().unwrap().to_string())
            }
            _ => None,
        })
        .collect()
}

fn state_of(runtime: &Runtime, state: &State, path: &str) -> Json {
    serde_json::to_value(runtime.view(state, false)).unwrap()["state"][path]["value"].clone()
}

#[test]
fn advance_clock_writes_land_before_the_move_and_its_raises_read_them() {
    let runtime = runtime("clock041-writes");
    let (start, _) = runtime.begin(Seed::default()).unwrap();
    let (state, output) = step(
        &runtime,
        &start,
        json!({"type": "advanceClock", "by": "day",
               "writes": {"state": [{"path": "run.ready", "value": true}]}}),
    );
    assert_eq!(
        brief(&output),
        [
            "presentation night",
            "Night falls.",
            "end night",
            "presentation morning",
            "Morning reads the write.",
            "end morning",
            "presentation tick",
            "The slot is raised.",
            "end tick",
            "clock 1.0->2.0",
        ]
    );
    assert_eq!(state_of(&runtime, &state, "run.greeted"), json!(1));
}

#[test]
fn writes_that_end_the_game_stop_the_clock() {
    let runtime = runtime("clock041-terminal");
    let (start, _) = runtime.begin(Seed::default()).unwrap();
    let (state, output) = step(
        &runtime,
        &start,
        json!({"type": "advanceClock", "by": "day",
               "writes": {"state": [{"path": "run.over", "value": true}]}}),
    );
    assert!(
        matches!(output.await_, Await::Ended { .. }),
        "{:?}",
        output.await_
    );
    assert!(brief(&output).is_empty(), "{:?}", brief(&output));
    assert_eq!(state_of(&runtime, &state, "run.day"), json!(1));
    assert_eq!(state_of(&runtime, &state, "run.over"), json!(true));
}

#[test]
fn a_beat_advance_moves_the_clock_after_its_presentation_ends() {
    let runtime = runtime("clock041-advances");
    let (start, _) = runtime.begin(Seed::default()).unwrap();
    let (_, output) = step(
        &runtime,
        &start,
        json!({"type": "raiseOccasion", "occasion": "work"}),
    );
    assert_eq!(
        brief(&output),
        [
            "presentation work",
            "Work takes the morning.",
            "end work",
            "presentation tick",
            "The slot is raised.",
            "end tick",
            "clock 1.0->1.1",
        ]
    );
}

/// `lute play`: an `advance:` step's `engine:` writes are printed before
/// the move, and the new day's raises read them.
#[test]
fn play_prints_advance_writes_before_the_move() {
    let scratch = lute_scratch::ScratchDir::named("clock041-play");
    let script = scratch.join("s.play.yaml");
    std::fs::write(
        &script,
        "steps:\n  - advance: day\n    engine: { state: { run.ready: true } }\n    \
         expect: { state: { run.greeted: 1 } }\n",
    )
    .unwrap();
    let out = Command::new(BIN)
        .arg("play")
        .arg("--project")
        .arg(fixture())
        .arg("--script")
        .arg(&script)
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{text}{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let write = text
        .find("set run.ready = true")
        .expect("the write is printed");
    let moved = text.find("set run.day = 2").expect("the move is printed");
    assert!(write < moved, "{text}");
    assert!(text.contains("Morning reads the write."), "{text}");
}
