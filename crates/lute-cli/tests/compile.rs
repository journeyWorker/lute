//! `lute compile` acceptance: exit codes 0/1/2, stdout artifact JSON, `-o`.

use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

#[test]
fn compile_bianca_exits_zero_with_artifact_json() {
    let out = Command::new(BIN)
        .args(["compile", "../../docs/examples/bianca-s01ep02.lute"])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["lute"], "0.0.1");
    assert_eq!(v["meta"]["episodeId"], "S01EP02");
    let commands = v["commands"].as_array().unwrap();
    assert!(!commands.is_empty());
    assert_eq!(commands[0]["addr"], "001-0100");
}

#[test]
fn compile_error_doc_exits_one_and_emits_no_artifact() {
    // date-minigame needs its project; core-only it checks with errors.
    let out = Command::new(BIN)
        .args(["compile", "../../docs/examples/date-minigame.lute"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(
        out.stdout.is_empty() || !out.stdout.starts_with(b"{"),
        "no artifact on stdout"
    );
}

#[test]
fn compile_missing_file_exits_two() {
    let out = Command::new(BIN)
        .args(["compile", "../../docs/examples/no-such-file.lute"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn compile_writes_out_file() {
    let tmp = std::env::temp_dir().join("lute-compile-cli-test.json");
    let _ = std::fs::remove_file(&tmp);
    let out = Command::new(BIN)
        .args(["compile", "../../docs/examples/bianca-s01ep02.lute", "-o"])
        .arg(&tmp)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert!(
        out.stdout.is_empty(),
        "artifact goes to the file, not stdout"
    );
    let s = std::fs::read_to_string(&tmp).unwrap();
    assert!(s.starts_with("{\n"));
    assert!(s.ends_with("\n"));
    let _ = std::fs::remove_file(&tmp);
}
