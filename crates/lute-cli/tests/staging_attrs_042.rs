//! 0.42.0 §4 (#78): every staging record the runtime emits — `bg`, `music`,
//! `sfx`, `vfx`, `actor`, `camera`, `cg`, `video`, `sequence` — carries the
//! command's resolved fields as `attrs`: declared defaults filled in
//! (`cg`/`video` `display`, an actor's default `anchor`), absent optionals
//! omitted, and never the envelope, `timing` or provenance. Other records
//! keep their shape.

use std::path::Path;
use std::process::Command;

use serde_json::{json, Value};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

/// `lute run --json` over the conformance staging fixture: its records.
fn records() -> Vec<Value> {
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/staging-sequence-camera");
    let dir = lute_scratch::ScratchDir::named("staging-attrs-042");
    let artifact = dir.join("a.json");
    let compiled = Command::new(BIN)
        .arg("compile")
        .arg(fixture.join("source.lute"))
        .arg("-o")
        .arg(&artifact)
        .output()
        .unwrap();
    assert!(
        compiled.status.success(),
        "{}",
        String::from_utf8_lossy(&compiled.stderr)
    );
    let run = Command::new(BIN)
        .arg("run")
        .arg(&artifact)
        .arg("--mock")
        .arg(fixture.join("mock.yaml"))
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let out: Value = serde_json::from_slice(&run.stdout).unwrap();
    out["commands"].as_array().unwrap().clone()
}

#[test]
fn every_staging_record_carries_its_resolved_attrs() {
    let staging = [
        "bg", "music", "sfx", "vfx", "actor", "camera", "cg", "video", "sequence",
    ];
    let got: Vec<Value> = records()
        .into_iter()
        .filter(|r| staging.contains(&r["kind"].as_str().unwrap()))
        .collect();
    let expected = [
        json!({"kind": "bg", "position": "001-0100", "attrs": {"location": "harbor", "time": "dusk", "assetId": "BG.harbor"}}),
        json!({"kind": "music", "position": "001-0200", "attrs": {"playback": "start", "mood": "calm", "volume": "normal", "assetId": "BGM.harbor"}}),
        json!({"kind": "sfx", "position": "001-0300", "attrs": {"sound": "gull", "assetId": "SFX.gull"}}),
        json!({"kind": "vfx", "position": "001-0400", "attrs": {"type": "whiteOut", "transition": "flash"}}),
        json!({"kind": "sequence", "position": "001-0500", "attrs": {"name": "harborArrival"}}),
        json!({"kind": "actor", "position": "001-0600", "attrs": {"character": "hero", "action": "fadeIn", "emotion": "calm", "costume": "coat"}}),
        json!({"kind": "actor", "position": "001-0700", "attrs": {"character": "hero", "anchor": "center"}}),
        json!({"kind": "actor", "position": "001-0800", "attrs": {"character": "hero", "emotion": "shy"}}),
        json!({"kind": "camera", "position": "001-0900", "attrs": {"focus": "hero", "framing": "close", "move": "pan", "transition": "fade"}}),
        json!({"kind": "cg", "position": "001-1000", "attrs": {"assetId": "CG.harbor", "display": "show"}}),
        json!({"kind": "cg", "position": "001-1100", "attrs": {"assetId": "CG.harbor.inset", "display": "hide", "layout": "inset"}}),
        json!({"kind": "video", "position": "001-1200", "attrs": {"assetId": "VID.harbor", "display": "show"}}),
        json!({"kind": "actor", "position": "001-1300", "attrs": {"character": "hero", "posReset": true}}),
        json!({"kind": "camera", "position": "001-1500", "attrs": {"framing": "wide", "move": "shake"}}),
        json!({"kind": "sfx", "position": "001-1600", "attrs": {"sound": "horn"}}),
        json!({"kind": "sequence", "position": "001-1800", "attrs": {"name": "ferryLeaves"}}),
        json!({"kind": "actor", "position": "001-1900", "attrs": {"character": "hero", "action": "fadeOut", "exit": true}}),
        json!({"kind": "music", "position": "001-2000", "attrs": {"playback": "stop"}}),
    ];
    assert_eq!(got, expected);
}

#[test]
fn other_records_keep_their_shape() {
    for record in records() {
        let kind = record["kind"].as_str().unwrap();
        if matches!(kind, "line" | "barrier") {
            assert!(record.get("attrs").is_none(), "{record}");
        }
    }
}
