//! dsl 0.41.0 §5.3 / §5.5: `awaitBridge` carries the call's `attrs`
//! (`occasion.target` bound, declared defaults filled, `@ref`s folded) and its
//! typed `results`; a `bridgeResult` is validated against them and may carry
//! `writes`, applied at the call after its declared effects — visible to the
//! lines after it, replayed on resume, serialized in a suspended state's
//! snapshot — and refused when unresolved or aimed at the clock. Every
//! `plugin` record carries `attrs`.

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

/// `lute compile --all` of the fixture into `scratch`: the bundle directory.
fn compiled(scratch: &str) -> (lute_scratch::ScratchDir, PathBuf) {
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
    (scratch, out)
}

fn read(path: &Path) -> Json {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// The compiled fixture, read back as a host reads it.
fn runtime(scratch: &str) -> Runtime {
    let (_scratch, out) = compiled(scratch);
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
            "attrs": {"foe": "kato", "mode": "ambush", "resultKey": "fight", "rounds": 3},
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
        // Past ±2^53 — the bound of every host-passed `int` — even when it
        // is a JSON integer: refused here, never a halt at the call.
        json!({"outcome": "win", "hpFloor": 9_223_372_036_854_775_808_u64}),
        json!({"outcome": "win", "hpFloor": 9_007_199_254_740_993_i64}),
        json!({"outcome": "win", "hpFloor": -9_007_199_254_740_993_i64}),
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
        // An `int` path takes an integer within ±2^53, as `value` or `add`.
        json!({"state": [{"path": "run.score", "value": 9_007_199_254_740_993_i64}]}),
        json!({"state": [{"path": "run.score", "value": "9007199254740993"}]}),
        json!({"state": [{"path": "run.score", "add": 1.5}]}),
        json!({"state": [{"path": "run.score", "add": 1e300}]}),
    ] {
        let r = refused(
            &runtime,
            &state,
            json!({"type": "bridgeResult", "request": request,
                   "fields": {"outcome": "win", "hpFloor": 3}, "writes": writes}),
        );
        assert_eq!(r.code, "E-RUNTIME-INPUT", "{writes}: {}", r.message);
    }
    // `hostWrite` resolves its writes by the same rule.
    let (start, _) = runtime.begin(Seed::default()).unwrap();
    for state in [
        json!([{"path": "run.score", "value": 9_007_199_254_740_993_i64}]),
        json!([{"path": "run.score", "add": 1e300}]),
        json!([{"path": "run.score", "add": -0.5}]),
    ] {
        let r = refused(
            &runtime,
            &start,
            json!({"type": "hostWrite", "writes": {"state": state}}),
        );
        assert_eq!(r.code, "E-RUNTIME-INPUT", "{state}: {}", r.message);
        assert!(r.message.contains("within ±2^53"), "{state}: {}", r.message);
    }
    // The bound itself is admitted.
    step(
        &runtime,
        &start,
        json!({"type": "hostWrite", "writes": {"state": [
            {"path": "run.score", "value": 9_007_199_254_740_992_i64},
            {"path": "run.score", "add": -9_007_199_254_740_992_i64},
        ]}}),
    );
}

/// A non-blocking plugin call records its `attrs` too: the omitted `wrapped`
/// and `weight` at their declared defaults, `count=@pair` as the int its def
/// folds to.
#[test]
fn every_plugin_record_carries_attrs() {
    let runtime = runtime("bridge041-attrs");
    let (start, _) = runtime.begin(Seed::default()).unwrap();
    let (_, output) = step(&runtime, &start, json!({"type": "raiseOccasion", "occasion": "give"}));
    let plugin = records(&output).into_iter().find(|r| r["kind"] == "plugin").unwrap();
    assert_eq!(plugin["tag"], "handOver");
    assert_eq!(
        plugin["attrs"],
        json!({"item": "rope", "to": "mira", "count": 2, "wrapped": false, "weight": 1.0})
    );
}

/// The artifact ships the declared defaults of omitted attrs in `fields`,
/// typed as an authored value (`double` `default: 1` reads `1.0`), and a
/// folded `@ref` as its typed constant — no CEL text, so a host without
/// directive declarations sees the call whole.
#[test]
fn artifacts_carry_declared_defaults_and_folded_refs() {
    let (_scratch, out) = compiled("bridge041-artifact");
    let fields = |artifact: &str, tag: &str| -> Json {
        read(&out.join(artifact))["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["kind"] == "plugin" && c["tag"] == tag)
            .unwrap()["fields"]
            .clone()
    };
    assert_eq!(
        fields("fight.lute.json", "battle"),
        json!({"foe": "occasion.target", "mode": "ambush", "resultKey": "fight", "rounds": 3})
    );
    let gift = fields("gift.lute.json", "handOver");
    assert_eq!(
        gift,
        json!({"item": "rope", "to": "mira", "count": 2, "wrapped": false, "weight": 1.0})
    );
    assert!(gift["weight"].is_f64(), "{gift}");
}

/// `lute check-project` of the fixture with each `(file, from, to)` edit
/// applied: whether it passed, and everything it printed.
fn check_edited(scratch: &str, edits: &[(&str, &str, &str)]) -> (bool, String) {
    let project = lute_scratch::ScratchDir::named(scratch);
    copy_tree(&fixture(), &project);
    for (file, from, to) in edits {
        let path = project.join(file);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains(from), "{file} has no `{from}`");
        std::fs::write(&path, text.replace(from, to)).unwrap();
    }
    let out = Command::new(BIN).arg("check-project").arg(&*project).output().unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

/// A declared default that is not a value of its attr's type never reaches
/// an artifact: the plugin is refused when the project loads.
#[test]
fn an_ill_typed_attr_default_is_refused_at_load() {
    let (ok, text) = check_edited(
        "bridge041-bad-default",
        &[(
            "plugins/arena/directives/d.yaml",
            "{ name: rounds, type: int, default: 3 }",
            "{ name: rounds, type: int, default: 1.5 }",
        )],
    );
    assert!(!ok, "{text}");
    assert!(
        text.contains(
            "E-PLUGIN-INVALID-DIRECTIVE: directive `::battle` declares attribute `rounds` as \
             `int` with `default: 1.5`, which is not of type `int`"
        ),
        "{text}"
    );
}

/// A default whose members the project resolves (`enumFromOption`) is judged
/// at each call that omits it, like the authored value it stands in for.
#[test]
fn a_project_resolved_default_is_judged_where_a_call_omits_it() {
    let check = |scratch: &str, default: &str| {
        let weight = "      - { name: weight, type: double, default: 1 }\n";
        let level = format!(
            "{weight}      - {{ name: level, type: {{ enumFromOption: levels }}, default: {default} }}\n"
        );
        check_edited(
            scratch,
            &[
                (
                    "plugins/arena/plugin.yaml",
                    "exports:",
                    "options:\n  - { name: levels, type: { list: string }, default: [easy, hard] }\nexports:",
                ),
                ("plugins/arena/directives/d.yaml", weight, &level),
                // Activation resolves the option the attr's members come from.
                ("lute.project.yaml", "arena: true", "arena: { levels: [easy, hard] }"),
            ],
        )
    };
    let (ok, text) = check("bridge041-option-default-ok", "hard");
    assert!(ok, "{text}");
    let (ok, text) = check("bridge041-option-default-bad", "bogus");
    assert!(!ok, "{text}");
    assert!(
        text.contains(
            "gift.lute:13:1: error [E-BAD-ENUM] `bogus` is not a valid value for `level` of \
             `::handOver` (expected one of: easy, hard) — this is the declared `default:` of \
             `level`, used because the call omits it"
        ),
        "{text}"
    );
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let (source, target) = (entry.path(), to.join(entry.file_name()));
        if source.is_dir() {
            copy_tree(&source, &target);
        } else {
            std::fs::copy(&source, &target).unwrap();
        }
    }
}
