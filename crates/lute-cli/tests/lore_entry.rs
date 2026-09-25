//! dsl 0.19.0 §8: `lute trace <doc> --entry <id>` and `lute run <artifact>
//! --entry <id>` present ONE lore entry; a lore document/artifact without
//! `--entry` is a usage error (exit 2) — there is no sequence to play.
//! Fixture: the `conformance/lore-entry` source and its frozen artifact.

use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");
const SOURCE: &str = "../../conformance/lore-entry/source.lute";
const ARTIFACT: &str = "../../conformance/lore-entry/artifact.json";
const SCENE_ARTIFACT: &str = "../../conformance/end-reason/artifact.json";

fn lute(args: &[&str]) -> (Option<i32>, String, String) {
    let out = Command::new(BIN).args(args).output().unwrap();
    (
        out.status.code(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn trace_entry_first_read_applies_effects_and_re_read_skips_them() {
    let (code, stdout, stderr) = lute(&["trace", SOURCE, "--entry", "scientistLog1"]);
    assert_eq!(code, Some(0), "stderr: {stderr}\nstdout: {stdout}");
    assert!(
        stdout.contains("<entry scientistLog1>   (first read)"),
        "{stdout}"
    );
    assert!(
        stdout.contains("Subject E does not respond to light."),
        "{stdout}"
    );
    assert!(
        stdout.contains("::assert  knows(vesna, project_lumen)\n"),
        "{stdout}"
    );
    assert!(stdout.contains("::set  run.logsRead = 1\n"), "{stdout}");

    let (code, stdout, stderr) = lute(&[
        "trace",
        SOURCE,
        "--entry",
        "scientistLog1",
        "--state",
        "entry.scientistLog1.read=true",
        "--state",
        "run.labBurned=true",
    ]);
    assert_eq!(code, Some(0), "stderr: {stderr}\nstdout: {stdout}");
    assert!(stdout.contains("(re-read: effects skipped)"), "{stdout}");
    assert!(
        stdout.contains("The page is scorched at the edges."),
        "{stdout}"
    );
    assert!(
        stdout.contains("::assert  knows(vesna, project_lumen)  (skipped: re-read)"),
        "{stdout}"
    );
    assert!(
        stdout.contains("::set  run.logsRead += 1  (skipped: re-read)"),
        "{stdout}"
    );
}

#[test]
fn trace_lore_without_entry_is_a_usage_error() {
    let (code, stdout, stderr) = lute(&["trace", SOURCE]);
    assert_eq!(code, Some(2), "stdout: {stdout}");
    assert!(stderr.contains("pass `--entry <id>`"), "{stderr}");
    assert!(stderr.contains("scientistLog1, scientistLog2"), "{stderr}");
}

#[test]
fn trace_entry_on_an_unknown_id_or_a_scene_is_refused() {
    let (code, stdout, _) = lute(&["trace", SOURCE, "--entry", "nope"]);
    assert_eq!(code, Some(1), "{stdout}");
    assert!(stdout.contains("E-TRACE-ENTRY"), "{stdout}");
    assert!(stdout.contains("invalid `--entry`"), "{stdout}");

    let (code, stdout, _) = lute(&[
        "trace",
        "../../conformance/end-reason/source.lute",
        "--entry",
        "scientistLog1",
    ]);
    assert_eq!(code, Some(1), "{stdout}");
    assert!(stdout.contains("E-TRACE-ENTRY"), "{stdout}");
}

#[test]
fn run_entry_presents_one_entry_and_the_engine_marks_it_read() {
    let (code, stdout, stderr) = lute(&["run", ARTIFACT, "--entry", "scientistLog1", "--json"]);
    assert_eq!(code, Some(0), "stderr: {stderr}");
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["kind"], "lore");
    assert_eq!(v["commands"][0]["kind"], "entry");
    assert_eq!(v["commands"][0]["firstRead"], true);
    let kinds: Vec<&str> = v["commands"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["kind"].as_str().unwrap())
        .collect();
    // Body segment only: runs to the next `entry` record, never into it.
    assert_eq!(kinds, ["entry", "match", "line", "assert", "set"]);
    assert_eq!(
        v["facts"],
        serde_json::json!(["knows(vesna, project_lumen)"])
    );
    assert_eq!(v["state"]["entry.scientistLog1.read"], true);
    assert_eq!(v["state"]["run.logsRead"], 1);

    // The second log gates on the first: presented, but `eligible: false`.
    let (code, stdout, _) = lute(&["run", ARTIFACT, "--entry", "scientistLog2", "--json"]);
    assert_eq!(code, Some(0));
    let v: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(v["commands"][0]["eligible"], false);
}

#[test]
fn run_lore_without_entry_or_entry_on_a_scene_is_a_usage_error() {
    let (code, stdout, stderr) = lute(&["run", ARTIFACT, "--json"]);
    assert_eq!(code, Some(2), "stdout: {stdout}");
    assert!(stderr.contains("is a lore artifact"), "{stderr}");
    assert!(stdout.is_empty(), "{stdout}");

    let (code, _, stderr) = lute(&["run", SCENE_ARTIFACT, "--entry", "scientistLog1"]);
    assert_eq!(code, Some(2));
    assert!(stderr.contains("needs a lore artifact"), "{stderr}");

    let (code, _, stderr) = lute(&["run", ARTIFACT, "--entry", "nope"]);
    assert_eq!(code, Some(2));
    assert!(stderr.contains("names no entry"), "{stderr}");
}
