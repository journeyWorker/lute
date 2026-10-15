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
    // The plugin re-show after the scene change is an entrance: its anchor
    // and first-emotion preload follow it directly.
    let reshow = commands
        .iter()
        .enumerate()
        .skip(bg + 1)
        .find(|(_, command)| command["kind"] == "actor" && command.get("provenance").is_none())
        .map(|(index, _)| index)
        .unwrap();
    assert_eq!(
        commands[reshow + 1]["provenance"]["by"],
        "auto-anchor-on-show"
    );
    assert_eq!(commands[reshow + 1]["character"], "marina");
    assert_eq!(
        commands[reshow + 2]["provenance"]["by"],
        "entry-emotion-lookahead"
    );
    assert_eq!(commands[reshow + 2]["emotion"], "calm");
    // Its stateful line dirties the pose, so the plain line after it gets a
    // reset placed directly before it.
    let plain = commands
        .iter()
        .position(|command| command["text"] == "Plain after stateful.")
        .unwrap();
    assert_eq!(commands[plain - 1]["provenance"]["by"], "auto-pose-reset");
    assert_eq!(commands[plain - 1]["character"], "marina");
    // The authored exit is the plugin record with `exit: true`.
    let exit = commands
        .iter()
        .rposition(|command| command["kind"] == "actor" && command.get("provenance").is_none())
        .unwrap();
    assert_eq!(commands[exit]["exit"], true);
    assert_eq!(commands[exit + 1]["text"], "Absent after exit.");
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

#[test]
fn plugin_actor_in_quest_gets_entry_emotion_preload() {
    let output = run(&[
        "compile",
        "../../crates/lute-cli/tests/fixtures/stage-record-045/quests/stage.lute",
        "--project",
        PROJECT,
        "--json",
    ]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let artifact: Value = serde_json::from_slice(&output.stdout).expect("quest artifact JSON");
    let commands = artifact["commands"].as_array().expect("quest commands");
    let actor = commands
        .iter()
        .position(|command| command["kind"] == "actor" && command.get("provenance").is_none())
        .unwrap();
    assert_eq!(
        commands[actor + 1]["provenance"]["by"],
        "entry-emotion-lookahead"
    );
    assert_eq!(commands[actor + 1]["emotion"], "happy");
}
