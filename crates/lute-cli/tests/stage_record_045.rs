//! Artifact-level staging dispatch tests for plugin lowerings in 0.45.

use serde_json::Value;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");
const SCENE: &str = "../../crates/lute-cli/tests/fixtures/stage-record-045/scenes/stage.lute";
const PROJECT: &str = "../../crates/lute-cli/tests/fixtures/stage-record-045";

fn run(args: &[&str]) -> std::process::Output {
    Command::new(BIN).args(args).output().expect("run lute")
}

#[test]
fn plugin_bg_and_actor_follow_lowered_records_in_artifact() {
    let output = run(&["compile", SCENE, "--project", PROJECT, "--json"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let artifact: Value = serde_json::from_slice(&output.stdout).expect("artifact JSON");
    let commands = artifact["commands"].as_array().expect("commands");

    let authored: Vec<&Value> = commands
        .iter()
        .filter(|command| command.get("provenance").is_none())
        .collect();
    let authored_kinds: Vec<&str> = authored
        .iter()
        .map(|command| command["kind"].as_str().unwrap_or(""))
        .collect();
    assert_eq!(
        authored_kinds,
        ["actor", "actor", "line", "bg", "line", "actor", "line", "line", "actor", "line"]
    );
    let bg = commands
        .iter()
        .position(|command| command["kind"] == "bg")
        .unwrap();
    let after_bg_line = commands
        .iter()
        .enumerate()
        .skip(bg + 1)
        .find(|(_, command)| command["kind"] == "line")
        .map(|(index, _)| index)
        .unwrap();
    assert_eq!(commands[bg - 1]["provenance"]["by"], "stage-bookkeeping");
    assert_eq!(commands[bg - 1]["kind"], "actor");
    assert_eq!(commands[bg - 1]["exit"], true);
    assert!(!commands[bg + 1..=after_bg_line]
        .iter()
        .any(|command| command["provenance"]["by"] == "auto-pose-reset"));
    assert!(commands
        .iter()
        .any(|command| command["provenance"]["by"] == "auto-anchor-on-show"));
    assert!(commands
        .iter()
        .any(|command| command["provenance"]["by"] == "entry-emotion-lookahead"));
    assert!(commands
        .iter()
        .any(|command| command["provenance"]["by"] == "auto-pose-reset"));
    assert!(commands.iter().any(|command| {
        command["kind"] == "actor" && command["exit"] == true && command.get("provenance").is_none()
    }));
}

#[test]
fn plugin_stage_warnings_and_trace_exit_are_record_kind_driven() {
    let checked = run(&["check", SCENE, "--project", PROJECT]);
    let check_text = String::from_utf8_lossy(&checked.stdout);
    assert!(check_text.contains("W-STAGE-ABSENT"), "{check_text}");
    assert!(
        check_text.contains("auto-hidden by an earlier `::backdrop`"),
        "{check_text}"
    );
    assert!(check_text.contains("earlier declared exit"), "{check_text}");
    let traced = run(&["trace", SCENE, "--project", PROJECT, "--json"]);
    assert!(
        traced.status.success(),
        "{}",
        String::from_utf8_lossy(&traced.stderr)
    );
    let report: Value = serde_json::from_slice(&traced.stdout).expect("trace JSON");
    assert!(report["steps"].as_array().unwrap().iter().any(|step| {
        step["kind"] == "directive" && step["tag"] == "portrait" && step["exit"] == true
    }));
}
