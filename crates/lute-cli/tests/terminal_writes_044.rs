//! dsl 0.44.0 §3 (#97): once the runtime answers `ended` it accepts only
//! `newRun` or an `outsideRun` raise, so a host write that belongs to the
//! ending rides as `writes` on the input that ends the run. A `hostWrite`
//! after the ending is refused `E-RUNTIME-BUSY` with the state unchanged and
//! a message that names the rule; other refused inputs keep their message.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use lute_runtime::index::Bundle;
use lute_runtime::runtime::{Await, Input, Seed, State};
use lute_runtime::Runtime;
use serde_json::{json, Value as Json};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

/// The 0.41 gate fixture: raising `verdict` sets `terminal: run.over`.
fn runtime() -> Runtime {
    let fixture: PathBuf =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime-gate-041");
    let scratch = lute_scratch::ScratchDir::named("terminal-writes-044");
    let out = scratch.join("bundle");
    let status = Command::new(BIN)
        .args(["compile", "--all"])
        .arg(&fixture)
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
            (artifact.clone(), read(&out.join(&artifact)))
        })
        .collect();
    Runtime::load(Bundle::new(artifacts, index)).unwrap()
}

fn step(runtime: &Runtime, state: &State, value: Json) -> Result<State, (State, String, String)> {
    let input: Input = serde_json::from_value(value).unwrap();
    match runtime.step(state.clone(), input) {
        Ok((state, output)) => {
            assert!(
                matches!(output.await_, Await::Ended { .. }),
                "{:?}",
                output.await_
            );
            Ok(state)
        }
        Err((state, rejected)) => Err((state, rejected.code, rejected.message)),
    }
}

fn snapshot(runtime: &Runtime, state: &State) -> Json {
    serde_json::to_value(runtime.snapshot(state)).unwrap()
}

#[test]
fn a_write_for_the_ending_rides_on_the_input_that_ends_the_run() {
    let runtime = runtime();
    let (start, _) = runtime.begin(Seed::default()).unwrap();
    let over = step(
        &runtime,
        &start,
        json!({"type": "raiseOccasion", "occasion": "verdict",
               "writes": {"state": [{"path": "run.mark", "value": 7}]}}),
    )
    .unwrap_or_else(|(_, code, message)| panic!("{code} {message}"));
    let mark = |state: &State| snapshot(&runtime, state)["world"]["state"]["run.mark"].to_string();
    assert!(
        mark(&over).contains('7'),
        "the ending's write is in the ended state: {}",
        mark(&over)
    );
    assert!(!mark(&start).contains('7'), "{}", mark(&start));

    let before = snapshot(&runtime, &over);
    let (after, code, message) = step(
        &runtime,
        &over,
        json!({"type": "hostWrite", "writes": {"state": [{"path": "run.mark", "value": 8}]}}),
    )
    .expect_err("a hostWrite after the ending was accepted");
    assert_eq!(code, "E-RUNTIME-BUSY");
    assert_eq!(
        snapshot(&runtime, &after),
        before,
        "the refusal changed the state"
    );
    assert_eq!(
        message,
        "the runtime has ended; only newRun or an outsideRun raise is accepted \u{2014} a write \
         that belongs to the ending rides as `writes` on the input that ends the run"
    );

    let (_, code, message) = step(
        &runtime,
        &over,
        json!({"type": "advanceClock", "by": "slot"}),
    )
    .expect_err("advanceClock after the ending was accepted");
    assert_eq!(code, "E-RUNTIME-BUSY");
    assert_eq!(
        message,
        "the runtime has ended; only newRun or an outsideRun raise is accepted"
    );
}
