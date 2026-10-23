//! 0.49.0 §3.1 (#155, #156): plugin stamps travel nested under `stamps` —
//! in the compiled record, on every runtime `plugin` record (normal and
//! halted), and on the bridge await with the snapshot that carries it —
//! beside and never merged into `attrs`. A stamp named like the record's own
//! key (`tag` here) no longer repeats that key. Transcripts render a line's
//! stamps as authored, so a needle naming one still matches.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Output as ProcessOutput};

use lute_runtime::index::Bundle;
use lute_runtime::runtime::{Await, Event, Input, Output, Seed, Snapshot, State};
use lute_runtime::Runtime;
use serde_json::{json, Value as Json};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugin-stamps-049")
}

fn lute(args: &[&str]) -> ProcessOutput {
    let project = fixture();
    let mut cmd = Command::new(BIN);
    for a in args {
        cmd.arg(a.replace("$P", project.to_str().unwrap()));
    }
    cmd.output().unwrap()
}

fn read(path: &Path) -> Json {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

/// Whole-project `lute compile` of the fixture, loaded as a host loads it.
fn runtime(scratch: &str) -> (Runtime, Json) {
    let scratch = lute_scratch::ScratchDir::named(scratch);
    let out = scratch.join("bundle");
    let compiled = Command::new(BIN)
        .args(["compile", "--project"])
        .arg(fixture())
        .arg("-o")
        .arg(&out)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
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
    let fight = artifacts["fight.lute.json"].clone();
    (Runtime::load(Bundle::new(artifacts, index)).unwrap(), fight)
}

fn step(runtime: &Runtime, state: &State, value: Json) -> (State, Output) {
    runtime
        .step(
            state.clone(),
            serde_json::from_value::<Input>(value).unwrap(),
        )
        .unwrap_or_else(|(_, r)| panic!("{} {}", r.code, r.message))
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

fn command<'a>(art: &'a Json, kind: &str, tag: &str) -> &'a Json {
    fn walk<'a>(v: &'a Json, kind: &str, tag: &str) -> Option<&'a Json> {
        match v {
            Json::Object(m)
                if m.get("kind") == Some(&json!(kind)) && m.get("tag") == Some(&json!(tag)) =>
            {
                Some(v)
            }
            Json::Object(m) => m.values().find_map(|c| walk(c, kind, tag)),
            Json::Array(a) => a.iter().find_map(|c| walk(c, kind, tag)),
            _ => None,
        }
    }
    walk(&art["commands"], kind, tag).unwrap_or_else(|| panic!("no {kind} `{tag}` in {art}"))
}

#[test]
fn compiled_plugin_records_keep_their_tag_and_nest_the_stamps() {
    let (_, fight) = runtime("stamps049-ir");
    let battle = command(&fight, "plugin", "battle");
    assert_eq!(battle["tag"], json!("battle"));
    assert_eq!(
        battle["fields"],
        json!({"foe": "occasion.target", "resultKey": "fight"})
    );
    assert_eq!(
        battle["stamps"],
        json!({"cue": "drums", "tag": "opening", "tempo": 3})
    );
    let wave = command(&fight, "plugin", "wave");
    assert_eq!(wave["stamps"], json!({"cue": "bell"}));
}

/// The blocking call suspends with `stamps` beside `attrs`; the suspended
/// state's snapshot carries them and restores; the `plugin` records the walk
/// emits — the non-blocking `::wave` before the suspension, the answered
/// `::battle` after it — carry them too.
#[test]
fn await_bridge_and_runtime_plugin_records_carry_stamps() {
    let (runtime, _) = runtime("stamps049-runtime");
    let (start, _) = runtime.begin(Seed::default()).unwrap();
    let (state, output) = step(
        &runtime,
        &start,
        json!({"type": "raiseOccasion", "occasion": "duel", "target": "foe.kato"}),
    );
    let plugins = |output: &Output| -> Vec<Json> {
        records(output)
            .into_iter()
            .filter(|r| r["kind"] == "plugin")
            .map(|r| json!({"tag": r["tag"], "attrs": r["attrs"], "stamps": r["stamps"]}))
            .collect()
    };
    assert_eq!(
        plugins(&output),
        [json!({"tag": "wave", "attrs": {"to": "kato"}, "stamps": {"cue": "bell"}})]
    );
    let Await::Bridge { request, .. } = output.await_ else {
        panic!("expected awaitBridge, got {:?}", output.await_);
    };
    assert_eq!(
        serde_json::to_value(state.current_await()).unwrap(),
        json!({
            "type": "awaitBridge", "request": request, "tag": "battle", "document": "fight.lute",
            "position": "001-0300",
            "attrs": {"foe": "kato", "resultKey": "fight"},
            "stamps": {"cue": "drums", "tag": "opening", "tempo": 3},
            "results": {"outcome": {"enum": ["win", "lose"]}}
        })
    );

    let snap = serde_json::to_value(runtime.snapshot(&state)).unwrap();
    assert_eq!(
        snap["await"]["stamps"],
        json!({"cue": "drums", "tag": "opening", "tempo": 3}),
        "{snap}"
    );
    let restored = runtime
        .restore(Snapshot::from_json(snap.to_string()).unwrap())
        .unwrap();
    assert_eq!(
        serde_json::to_value(restored.current_await()).unwrap(),
        serde_json::to_value(state.current_await()).unwrap()
    );

    let (_, output) = step(
        &runtime,
        &restored,
        json!({"type": "bridgeResult", "request": request, "fields": {"outcome": "win"}}),
    );
    assert_eq!(
        plugins(&output),
        [
            json!({"tag": "battle", "attrs": {"foe": "kato", "resultKey": "fight"},
                "stamps": {"cue": "drums", "tag": "opening", "tempo": 3}})
        ]
    );
}

/// The `lute play` walk: a halted call's record carries `stamps` too.
#[test]
fn a_halted_plugin_record_carries_stamps() {
    let out = lute(&[
        "play",
        "--project",
        "$P",
        "--script",
        "$P/halt.play.yaml",
        "--json",
    ]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let report: Json = serde_json::from_slice(&out.stdout).unwrap();
    let commands = report["steps"][0]["presented"]["commands"]
        .as_array()
        .unwrap();
    let halted = commands
        .iter()
        .find(|c| c["kind"] == "plugin" && c["tag"] == "battle")
        .unwrap_or_else(|| panic!("{report}"));
    assert_eq!(halted["unanswered"], json!(["outcome"]), "{halted}");
    assert_eq!(
        halted["attrs"],
        json!({"foe": "kato", "resultKey": "fight"})
    );
    assert_eq!(
        halted["stamps"],
        json!({"cue": "drums", "tag": "opening", "tempo": 3})
    );
}

/// The human `play` transcript renders a line's stamp as authored, and a
/// `lute test` needle naming it matches (`tests/hush.test.yaml`); one naming
/// another value does not.
#[test]
fn transcripts_render_line_stamps_and_needles_match_them() {
    let out = lute(&["play", "--project", "$P", "--script", "$P/win.play.yaml"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}");
    assert!(
        stdout.contains("\n@narrator{cue=\"hush\"}: Won.\n"),
        "{stdout}"
    );

    let out = lute(&["test", "--project", "$P", "$P/tests/hush.test.yaml"]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{stdout}");

    let scratch = lute_scratch::ScratchDir::named("stamps049-needle");
    let wrong = scratch.join("loud.test.yaml");
    std::fs::write(
        &wrong,
        std::fs::read_to_string(fixture().join("tests/hush.test.yaml"))
            .unwrap()
            .replace(
                "../fight.lute",
                fixture().join("fight.lute").to_str().unwrap(),
            )
            .replace("cue=\"hush\"", "cue=\"loud\""),
    )
    .unwrap();
    let out = lute(&["test", "--project", "$P", wrong.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{stdout}");
    assert!(
        stdout.contains("nearest line: \"@narrator{cue=\\\"hush\\\"}: Won.\""),
        "{stdout}"
    );
}
