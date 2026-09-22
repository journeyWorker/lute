//! End-to-end plugin projection: typed presentation directives lower to core
//! command records, while host-owned directives remain typed plugin records.
//! This is the contract a game plugin uses to transform authored content into
//! its existing engine data shape.

use serde_json::Value;
use std::process::Command;

fn lute_bin() -> &'static str {
    env!("CARGO_BIN_EXE_lute")
}

#[test]
fn typed_projection_preserves_core_records_and_host_fields() {
    let output = Command::new(lute_bin())
        .args([
            "compile",
            "../../docs/examples/typed-projection/first-scene.lute",
            "--project",
            "../../docs/examples/typed-projection",
            "--json",
        ])
        .output()
        .expect("run lute compile");

    assert_eq!(
        output.status.code(),
        Some(0),
        "compile failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let artifact: Value = serde_json::from_slice(&output.stdout).expect("artifact JSON");
    let commands = artifact["commands"].as_array().expect("commands array");

    assert_eq!(commands[0]["kind"], "background");
    assert_eq!(commands[0]["assetId"], "BG.location_alpha.night");
    assert_eq!(commands[1]["kind"], "sprite");
    assert_eq!(commands[1]["character"], "actor_a");
    assert_eq!(commands[1]["costume"], "ACTOR_DEFAULT");

    let host_panel = commands
        .iter()
        .find(|command| command["kind"] == "plugin")
        .expect("host panel plugin command");
    assert_eq!(host_panel["tag"], "host-panel");
    assert_eq!(
        host_panel["fields"]["assetId"],
        "CG.chapter_one.first_choice"
    );
    assert_eq!(host_panel["fields"]["resultKey"], "first-choice");
    assert_eq!(host_panel["plugin"], "game.presentation");
}

#[test]
fn pinned_provider_rejects_unknown_asset_ids() {
    let output = Command::new(lute_bin())
        .args([
            "check",
            "../../docs/examples/typed-projection/missing-asset.fixture",
            "--project",
            "../../docs/examples/typed-projection",
            "--json",
        ])
        .output()
        .expect("run lute check");

    assert_eq!(output.status.code(), Some(1));
    let result: Value = serde_json::from_slice(&output.stdout).expect("check JSON");
    assert_eq!(result["ok"], false);
    assert!(
        output
            .stdout
            .windows(b"not a known".len())
            .any(|window| window == b"not a known"),
        "expected an unknown provider diagnostic: {}",
        String::from_utf8_lossy(&output.stdout)
    );
}
