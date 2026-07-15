//! End-to-end `--deny <CODE>` / `--deny-warnings` promotion (spec §5): spawn the
//! built `lute` binary and assert exit codes + promotion markers. A clean
//! document that emits only a WARNING passes `check` (exit 0) unless a matching
//! promotion is requested, which turns it into an error (exit 1) with a
//! `denied` marker; a typo'd `--deny` code is a clap usage error (exit 2).
//!
//! The reliable warning-only fixture is a STALE `luteVersion:` stamp
//! (`W-LUTE-VERSION-STALE`, spec §3): a warning-grade freshness signal that
//! never flips a clean verdict on its own.

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

/// A minimal scene, check-clean except for a stale `luteVersion` warning.
const STALE_SCENE: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
    luteVersion: \"0.5.0\"\n---\n## Shot 1.\n@narrator: hi\n";

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-cli-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write_stale(tag: &str) -> (PathBuf, PathBuf) {
    let dir = temp_dir(tag);
    let file = dir.join("stale.lute");
    std::fs::write(&file, STALE_SCENE).unwrap();
    (dir, file)
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(BIN).args(args).output().unwrap()
}

/// Sanity: the fixture emits `W-LUTE-VERSION-STALE` as a WARNING and, with no
/// promotion flags, `check` is clean (exit 0). This is the baseline every
/// promotion test flips.
#[test]
fn warning_doc_clean_without_flags() {
    let (_dir, file) = write_stale("clean");
    let out = run(&["check", file.to_str().unwrap(), "--json"]);
    assert!(out.status.success(), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
    let diags = v["diagnostics"].as_array().unwrap();
    let stale = diags
        .iter()
        .find(|d| d["code"] == "W-LUTE-VERSION-STALE")
        .expect("fixture must emit W-LUTE-VERSION-STALE");
    assert_eq!(stale["severity"], "warning");
    assert!(stale.get("denied").is_none(), "unpromoted warning has no denied marker");
}

/// `--deny <thatcode>` promotes exactly that code: exit 1, `ok: false`, the
/// diagnostic reports `severity: error` + `denied: true` (spec §5).
#[test]
fn deny_specific_code_promotes_to_error() {
    let (_dir, file) = write_stale("deny-code");
    let out = run(&["check", file.to_str().unwrap(), "--json", "--deny", "W-LUTE-VERSION-STALE"]);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
    let stale = v["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "W-LUTE-VERSION-STALE")
        .unwrap();
    assert_eq!(stale["severity"], "error", "promoted severity is error");
    assert_eq!(stale["denied"], true, "promoted diagnostic carries denied: true");
}

/// `--deny-warnings` promotes every warning the same way: exit 1 + denied.
#[test]
fn deny_warnings_promotes_to_error() {
    let (_dir, file) = write_stale("deny-warnings");
    let out = run(&["check", file.to_str().unwrap(), "--json", "--deny-warnings"]);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
    let stale = v["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "W-LUTE-VERSION-STALE")
        .unwrap();
    assert_eq!(stale["severity"], "error");
    assert_eq!(stale["denied"], true);
}

/// The human line prints `error` with a `[denied]` marker for a promoted
/// diagnostic (spec §5).
#[test]
fn deny_human_line_marks_denied() {
    let (_dir, file) = write_stale("deny-human");
    let out = run(&["check", file.to_str().unwrap(), "--deny", "W-LUTE-VERSION-STALE"]);
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout
        .lines()
        .find(|l| l.contains("W-LUTE-VERSION-STALE"))
        .expect("a diagnostic line naming the code");
    assert!(line.contains("[denied]"), "promoted line must carry the [denied] marker: {line}");
    assert!(line.contains(" error "), "promoted line reports error severity: {line}");
}

/// An unknown `--deny` code is a clap usage error (exit 2) — "a typo'd
/// promotion MUST NOT silently protect nothing" (spec §5).
#[test]
fn unknown_deny_code_is_usage_error() {
    let (_dir, file) = write_stale("deny-unknown");
    let out = run(&["check", file.to_str().unwrap(), "--json", "--deny", "W-NOPE-NOT-REAL"]);
    assert_eq!(out.status.code(), Some(2), "unknown --deny code must be a usage error");
    // clap usage errors go to stderr, not the JSON stdout surface.
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("W-NOPE-NOT-REAL"),
        "stderr names the offending code: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A `--deny` naming a real code that is NOT present in the document protects
/// nothing but is not an error: the clean verdict stands (exit 0).
#[test]
fn deny_nonmatching_code_leaves_verdict_unchanged() {
    let (_dir, file) = write_stale("deny-nonmatch");
    let out = run(&["check", file.to_str().unwrap(), "--json", "--deny", "W-OVERLAP-ARMS"]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
}

/// `check-project` honors `--deny-warnings` over per-file diagnostics: a
/// project whose only defect is a warning fails under promotion (exit 1).
#[test]
fn check_project_deny_warnings_promotes() {
    let (dir, _file) = write_stale("proj-deny");
    let clean = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(clean.status.code(), Some(0), "baseline project is clean");

    let out = run(&["check-project", dir.to_str().unwrap(), "--json", "--deny-warnings"]);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
    let file0 = &v["files"].as_array().unwrap()[0];
    assert_eq!(file0["ok"], false, "the promoted file's own ok flips too");
    let stale = file0["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "W-LUTE-VERSION-STALE")
        .unwrap();
    assert_eq!(stale["denied"], true);
}

/// `check-project` also accepts `--deny <CODE>` and rejects an unknown one at
/// the clap layer (exit 2), mirroring `check`.
#[test]
fn check_project_unknown_deny_code_is_usage_error() {
    let (dir, _file) = write_stale("proj-unknown");
    let out = run(&["check-project", dir.to_str().unwrap(), "--deny", "NOT-A-CODE"]);
    assert_eq!(out.status.code(), Some(2));
}
