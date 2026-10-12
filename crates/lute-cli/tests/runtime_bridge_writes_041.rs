//! dsl 0.41.0 §5.3 / §5.5: `awaitBridge` carries the call's `attrs`
//! (`occasion.target` bound) and its typed `results`; a `bridgeResult` is
//! validated against them and may carry `writes`, applied at the call after
//! its declared effects — visible to the lines after it, replayed on resume,
//! serialized in a suspended state's snapshot — and refused when unresolved
//! or aimed at the clock. Every `plugin` record carries `attrs`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use lute_runtime::index::Bundle;
use lute_runtime::runtime::{Await, Event, Input, Output, Rejected, Seed, Snapshot, State};
use lute_runtime::Runtime;
use serde_json::{json, Value as Json};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/runtime-bridge-041")
}

/// `lute compile --all` of the fixture, read back as a host reads it.
fn runtime(scratch: &str) -> Runtime {
    let scratch = lute_scratch::ScratchDir::named(scratch);
    let out = scratch.join("bundle");
    let status = Command::new(BIN)
        .args(["compile", "--all"])
        .arg(fixture())
        .arg("-o")
        .arg(&out)
        .output()
        .unwrap();
    assert!(status.status.success(), "{}", String::from_utf8_lossy(&status.stderr));
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
        .step(state.clone(), serde_json::from_value::<Input>(value).unwrap())
        .unwrap_or_else(|(_, r)| panic!("{} {}", r.code, r.message))
}

fn refused(runtime: &Runtime, state: &State, value: Json) -> Rejected {
    let before = serde_json::to_string(&runtime.snapshot(state)).unwrap();
    let (after, rejected) = runtime
        .step(state.clone(), serde_json::from_value::<Input>(value).unwrap())
        .unwrap_err();
    assert_eq!(serde_json::to_string(&runtime.snapshot(&after)).unwrap(), before);
    rejected
}

fn records(output: &Output) -> Vec<Json> {
    output
        .events
        .iter()
        .filter_map(|event| match event {
            Event::Record { record, .. } => Some(record.clone()),
            _ => None,
        })
        .collect()
}

fn lines(output: &Output) -> Vec<String> {
    records(output)
        .iter()
        .filter(|r| r["kind"] == "line")
        .map(|r| r["text"].as_str().unwrap().to_string())
        .collect()
}

/// The fight beat raised for `foe.kato`, suspended at `::battle`.
fn at_the_battle(runtime: &Runtime) -> (State, u64) {
    let (start, _) = runtime.begin(Seed::default()).unwrap();
    let (state, output) = step(
        runtime,
        &start,
        json!({"type": "raiseOccasion", "occasion": "duel", "target": "foe.kato"}),
    );
    let Await::Bridge { request, .. } = output.await_ else {
        panic!("expected awaitBridge, got {:?}", output.await_);
    };
    (state, request)
}

#[test]
fn await_bridge_carries_attrs_and_typed_results() {
    let runtime = runtime("bridge041-await");
    let (state, _) = at_the_battle(&runtime);
    let await_ = serde_json::to_value(state.current_await()).unwrap();
    assert_eq!(
        await_,
        json!({
            "type": "awaitBridge", "request": 1, "tag": "battle", "document": "fight.lute",
            "position": "001-0200",
            "attrs": {"foe": "kato", "mode": "ambush", "resultKey": "fight"},
            "results": {"outcome": {"enum": ["win", "lose"]}, "hpFloor": "int"}
        })
    );
}

#[test]
fn answers_are_validated_against_the_result_types() {
    let runtime = runtime("bridge041-shape");
    let (state, request) = at_the_battle(&runtime);
    for fields in [
        json!({"outcome": "draw", "hpFloor": 3}),
        json!({"outcome": "win", "hpFloor": 3.5}),
        json!({"outcome": "win", "hpFloor": "3"}),
        json!({"outcome": "win"}),
    ] {
        let r = refused(
            &runtime,
            &state,
            json!({"type": "bridgeResult", "request": request, "fields": fields}),
        );
        assert_eq!(r.code, "E-RUNTIME-BRIDGE-SHAPE", "{fields}: {}", r.message);
    }
}

#[test]
fn answer_writes_land_at_the_call_before_the_lines_after_it() {
    let runtime = runtime("bridge041-writes");
    let (state, request) = at_the_battle(&runtime);
    let (state, output) = step(
        &runtime,
        &state,
        json!({"type": "bridgeResult", "request": request,
               "fields": {"outcome": "win", "hpFloor": 3},
               "writes": {"state": [{"path": "run.score", "value": 5}], "facts": ["pulled(rope)"]}}),
    );
    let recs = records(&output);
    let plugin = recs.iter().position(|r| r["kind"] == "plugin").unwrap();
    let host: Vec<&Json> = recs.iter().filter(|r| r["by"] == "host").collect();
    assert_eq!(
        host,
        [
            &json!({"position": "001-0200", "kind": "set", "path": "run.score", "value": 5,
                    "effectOf": "battle", "by": "host"}),
            &json!({"position": "001-0200", "kind": "assert", "fact": "pulled(rope)",
                    "effectOf": "battle", "by": "host"}),
        ]
    );
    // After the call's own effects, before anything the walk does next.
    let first_host = recs.iter().position(|r| r["by"] == "host").unwrap();
    let between: Vec<&Json> = recs[plugin + 1..first_host].iter().map(|r| &r["bridgeResult"]).collect();
    assert_eq!(between, [&json!("outcome"), &json!("hpFloor")]);
    assert_eq!(lines(&output), ["Won.", "Floor 3, score 5."]);

    // The walk goes on to the branch: the suspended state's answers carry
    // the writes, its snapshot serializes them, and a resume replays them.
    let Await::Choice { request, .. } = output.await_ else {
        panic!("expected awaitChoice, got {:?}", output.await_);
    };
    let snap = serde_json::to_value(runtime.snapshot(&state)).unwrap();
    assert_eq!(
        snap["continuation"]["answers"][0]["bridge"]["writes"],
        json!({"state": [{"path": "run.score", "value": 5}], "facts": ["pulled(rope)"]}),
        "{snap}"
    );
    let restored = runtime
        .restore(Snapshot::from_json(snap.to_string()).unwrap())
        .unwrap();
    let (done, _) = step(
        &runtime,
        &restored,
        json!({"type": "choose", "request": request, "option": "rest"}),
    );
    let view = serde_json::to_value(runtime.view(&done, true)).unwrap();
    assert_eq!(view["state"]["run.score"]["value"], json!(5), "{view}");
    assert_eq!(view["facts"], json!(["pulled(rope)"]), "{view}");
}

#[test]
fn unresolved_or_clock_answer_writes_are_refused() {
    let runtime = runtime("bridge041-refused");
    let (state, request) = at_the_battle(&runtime);
    for writes in [
        json!({"state": [{"path": "run.nope", "value": 1}]}),
        json!({"facts": ["unknownRel(x)"]}),
        json!({"state": [{"path": "run.day", "value": 2}]}),
        json!({"state": [{"path": "run.slot", "value": "dusk"}]}),
    ] {
        let r = refused(
            &runtime,
            &state,
            json!({"type": "bridgeResult", "request": request,
                   "fields": {"outcome": "win", "hpFloor": 3}, "writes": writes}),
        );
        assert_eq!(r.code, "E-RUNTIME-INPUT", "{writes}: {}", r.message);
    }
}

/// A non-blocking plugin call records its `attrs` too.
#[test]
fn every_plugin_record_carries_attrs() {
    let runtime = runtime("bridge041-attrs");
    let (start, _) = runtime.begin(Seed::default()).unwrap();
    let (_, output) = step(&runtime, &start, json!({"type": "raiseOccasion", "occasion": "give"}));
    let plugin = records(&output).into_iter().find(|r| r["kind"] == "plugin").unwrap();
    assert_eq!(plugin["tag"], "handOver");
    assert_eq!(plugin["attrs"], json!({"item": "rope", "to": "mira"}));
}
