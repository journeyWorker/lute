//! dsl 0.41.0 §5.2: a `raiseOccasion` the engine would not make — its
//! `raisedWhen` is false, or the game is over and the occasion is not
//! `outsideRun` — is the rejection `E-OCCASION-GATE` with the state unchanged
//! (the input's own writes included); an undecided gate still halts
//! `incomplete`; the `gate` query answers the same rule beforehand; and
//! `lute play --events` prints the rejection line while `lute play` keeps
//! its refusal text.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use lute_runtime::index::Bundle;
use lute_runtime::runtime::{Await, Gate, Input, Rejected, Seed, State};
use lute_runtime::Runtime;
use serde_json::{json, Value as Json};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime-gate-041")
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

fn input(value: Json) -> Input {
    serde_json::from_value(value).unwrap()
}

fn snapshot(runtime: &Runtime, state: &State) -> String {
    serde_json::to_string(&runtime.snapshot(state)).unwrap()
}

/// Step `value` expecting a rejection; the returned state must be the one
/// stepped, byte for byte.
fn rejected(runtime: &Runtime, state: &State, value: Json) -> Rejected {
    let before = snapshot(runtime, state);
    let (after, rejected) = runtime.step(state.clone(), input(value)).unwrap_err();
    assert_eq!(
        snapshot(runtime, &after),
        before,
        "the rejection changed the state"
    );
    rejected
}

fn accepted(
    runtime: &Runtime,
    state: &State,
    value: Json,
) -> (State, lute_runtime::runtime::Output) {
    runtime
        .step(state.clone(), input(value))
        .unwrap_or_else(|(_, r)| panic!("{} {}", r.code, r.message))
}

fn gate(runtime: &Runtime, state: &State, occasion: &str, target: Option<&str>) -> Gate {
    runtime
        .gate(state, occasion, target)
        .unwrap_or_else(|r| panic!("{} {}", r.code, r.message))
}

const OPEN: Gate = Gate {
    open: true,
    unresolved: Vec::new(),
};
const CLOSED: Gate = Gate {
    open: false,
    unresolved: Vec::new(),
};

#[test]
fn a_raise_whose_gate_is_false_is_rejected_with_the_state_unchanged() {
    let runtime = runtime("gate041-false");
    let (start, _) = runtime.begin(Seed::default()).unwrap();
    let r = rejected(
        &runtime,
        &start,
        json!({"type": "raiseOccasion", "occasion": "enter", "target": "room.office"}),
    );
    assert_eq!(r.code, "E-OCCASION-GATE");
    assert_eq!(
        r.message,
        "step 0: E-OCCASION-GATE: the engine raises `enter` for `room.office` only when \
         `holds('canEnter', [occasion.target])` (its `raisedWhen`), which is false here since \
         `canEnter(office)` does not hold — make it hold first (an `engine:` write, an earlier \
         step), or drop the step"
    );

    // The input's own writes land first and the gate is judged after them;
    // when it stays closed, they are rolled back with the rest.
    let r = rejected(
        &runtime,
        &start,
        json!({"type": "raiseOccasion", "occasion": "enter", "target": "room.office",
               "writes": {"state": [{"path": "run.mark", "value": 7}]}}),
    );
    assert_eq!(r.code, "E-OCCASION-GATE", "{}", r.message);

    // Writes that open it let the raise through.
    let (_, output) = accepted(
        &runtime,
        &start,
        json!({"type": "raiseOccasion", "occasion": "enter", "target": "room.office",
               "writes": {"facts": ["canEnter(office)"]}}),
    );
    assert!(matches!(output.await_, Await::Idle), "{:?}", output.await_);
    assert!(
        serde_json::to_string(&output.events)
            .unwrap()
            .contains("The office door gives."),
        "{:?}",
        output.events
    );
}

#[test]
fn a_raise_after_the_game_is_over_is_rejected_unless_outside_run() {
    let runtime = runtime("gate041-terminal");
    let (start, _) = runtime.begin(Seed::default()).unwrap();
    let (over, output) = accepted(
        &runtime,
        &start,
        json!({"type": "raiseOccasion", "occasion": "verdict"}),
    );
    assert!(
        matches!(output.await_, Await::Ended { .. }),
        "{:?}",
        output.await_
    );

    let r = rejected(
        &runtime,
        &over,
        json!({"type": "raiseOccasion", "occasion": "gallery"}),
    );
    assert_eq!(r.code, "E-OCCASION-GATE");
    assert!(
        r.message
            .contains("the game is over — `terminal: run.over` holds"),
        "{}",
        r.message
    );

    let (_, output) = accepted(
        &runtime,
        &over,
        json!({"type": "raiseOccasion", "occasion": "title"}),
    );
    assert!(
        serde_json::to_string(&output.events)
            .unwrap()
            .contains("The title screen."),
        "{:?}",
        output.events
    );

    // `gate` at `ended`: the `outsideRun` occasion is open, every other closed.
    assert_eq!(gate(&runtime, &over, "title", None), OPEN);
    assert_eq!(gate(&runtime, &over, "gallery", None), CLOSED);
    assert_eq!(gate(&runtime, &over, "enter", Some("room.lobby")), CLOSED);
}

#[test]
fn an_undecided_gate_still_halts_incomplete() {
    let runtime = runtime("gate041-undecided");
    let (start, _) = runtime.begin(Seed::default()).unwrap();
    let (_, output) = accepted(
        &runtime,
        &start,
        json!({"type": "raiseOccasion", "occasion": "listen"}),
    );
    match output.await_ {
        Await::Halted { kind, message, .. } => {
            assert_eq!(kind, "incomplete");
            assert!(message.contains("is undecided"), "{message}");
        }
        other => panic!("expected an incomplete halt, got {other:?}"),
    }
}

#[test]
fn the_gate_query_answers_the_raise_rule_and_changes_nothing() {
    let runtime = runtime("gate041-query");
    let (start, _) = runtime.begin(Seed::default()).unwrap();
    let before = snapshot(&runtime, &start);

    // The target maps through the occasion's target domain, as a raise does.
    assert_eq!(gate(&runtime, &start, "enter", Some("room.lobby")), OPEN);
    assert_eq!(gate(&runtime, &start, "enter", Some("lobby")), OPEN);
    assert_eq!(gate(&runtime, &start, "enter", Some("room.office")), CLOSED);
    // No gate, no terminal: open.
    assert_eq!(gate(&runtime, &start, "verdict", None), OPEN);
    // Undecided: one reason per unresolved read.
    assert_eq!(
        gate(&runtime, &start, "listen", None),
        Gate {
            open: false,
            unresolved: vec!["state path `run.noise` has no value".into()],
        }
    );
    let r = runtime.gate(&start, "missing", None).unwrap_err();
    assert_eq!(r.code, "E-RUNTIME-INPUT", "{}", r.message);
    assert_eq!(snapshot(&runtime, &start), before);

    // A decided-closed gate serializes without `unresolved`.
    assert_eq!(
        serde_json::to_value(CLOSED).unwrap(),
        json!({"open": false})
    );
}

fn play(script: &str, events: bool) -> std::process::Output {
    let scratch = lute_scratch::ScratchDir::named("gate041-play");
    let path = scratch.join("s.play.yaml");
    std::fs::write(&path, script).unwrap();
    let mut cmd = Command::new(BIN);
    cmd.arg("play").arg(fixture()).arg("--script").arg(&path);
    if events {
        cmd.arg("--events");
    }
    cmd.output().unwrap()
}

/// `lute play` keeps its refusal text and exit 1; `--events` prints the
/// rejection line instead of a halted output, exit 1.
#[test]
fn play_keeps_its_refusal_and_events_print_the_rejection_line() {
    let script = "steps:\n  - occasion: enter\n    target: room.lobby\n  \
                  - occasion: enter\n    target: room.office\n";
    let human = play(script, false);
    let text = String::from_utf8_lossy(&human.stdout).to_string()
        + &String::from_utf8_lossy(&human.stderr);
    assert_eq!(human.status.code(), Some(1), "{text}");
    assert!(
        text.contains(
            "step 2: E-OCCASION-GATE: the engine raises `enter` for `room.office` only when \
             `holds('canEnter', [occasion.target])` (its `raisedWhen`), which is false here"
        ),
        "{text}"
    );

    let events = play(script, true);
    assert_eq!(events.status.code(), Some(1));
    let stdout = String::from_utf8(events.stdout).unwrap();
    let last: Json = serde_json::from_str(stdout.lines().last().unwrap()).unwrap();
    assert_eq!(
        last["input"],
        json!({"type": "raiseOccasion", "occasion": "enter", "target": "room.office"})
    );
    assert_eq!(last["rejected"]["code"], "E-OCCASION-GATE", "{stdout}");
    assert!(
        last["rejected"]["message"]
            .as_str()
            .unwrap()
            .starts_with("step 0: E-OCCASION-GATE: the engine raises `enter` for `room.office`"),
        "{stdout}"
    );
    assert!(last.get("output").is_none(), "{stdout}");
    assert_eq!(stdout.lines().count(), 3, "{stdout}");
}
