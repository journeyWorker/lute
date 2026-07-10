//! `lute check-project <dir>` acceptance (0.2.1 §6.3 gap #3): project-wide
//! `<quest id>` uniqueness for quest docs NOT connected by an import edge —
//! `lute check`'s own `E-QUEST-ID-DUP` (0.2.0 F4) only sees a collision within
//! one document or across ITS OWN `uses:`/`extends:` import graph.

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

/// A fresh unique temp dir (matches `cli.rs`'s own helper — each integration
/// test binary is compiled separately, so this is intentionally duplicated
/// rather than shared).
fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-cli-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &std::path::Path, rel: &str, text: &str) -> PathBuf {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, text).unwrap();
    path
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(BIN).args(args).output().unwrap()
}

/// A self-contained, otherwise-CLEAN `kind: quest` doc declaring exactly one
/// quest id (its own state decl + a `done` slot that reads it, so no other
/// diagnostic — E-UNDECLARED/E-MAYBE-UNSET/etc — fires).
fn clean_quest_doc(quest_id: &str, state_path: &str) -> String {
    format!(
        "---\nkind: quest\nstate:\n  {state_path}: {{ type: bool, default: false }}\n---\n\
         <quest id=\"{quest_id}\">\n<objective id=\"o\" done=\"{state_path}\"/>\n</quest>\n"
    )
}

// --- gap #3: two quest docs, same id, NO import edge -----------------------

#[test]
fn standalone_check_on_either_unlinked_file_stays_clean_red_proof() {
    // RED proof: BEFORE `check-project` existed, nothing caught this — `lute
    // check` on either file alone is clean, because neither imports the
    // other and `check()`'s own E-QUEST-ID-DUP is scoped to one document (or
    // its own import graph).
    let dir = temp_dir("red-proof");
    let a = write(&dir, "a.lute", &clean_quest_doc("shared", "run.a"));
    let b = write(&dir, "b.lute", &clean_quest_doc("shared", "run.b"));

    let out_a = run(&["check", a.to_str().unwrap()]);
    assert!(
        out_a.status.success(),
        "a.lute alone must stay clean (the gap): {}",
        String::from_utf8_lossy(&out_a.stdout)
    );
    let out_b = run(&["check", b.to_str().unwrap()]);
    assert!(
        out_b.status.success(),
        "b.lute alone must stay clean (the gap): {}",
        String::from_utf8_lossy(&out_b.stdout)
    );
}

#[test]
fn check_project_flags_unlinked_cross_file_quest_id_dup() {
    let dir = temp_dir("cross-file-dup");
    write(&dir, "a.lute", &clean_quest_doc("shared", "run.a"));
    write(&dir, "b.lute", &clean_quest_doc("shared", "run.b"));

    let out = run(&["check-project", dir.to_str().unwrap()]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "a project-wide dup must exit non-zero: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("E-QUEST-ID-DUP"), "{stdout}");
    assert!(stdout.contains("a.lute"), "must name file a: {stdout}");
    assert!(stdout.contains("b.lute"), "must name file b: {stdout}");
}

#[test]
fn check_project_json_reports_ok_false_and_project_diagnostic_for_cross_file_dup() {
    let dir = temp_dir("cross-file-dup-json");
    write(&dir, "a.lute", &clean_quest_doc("shared", "run.a"));
    write(&dir, "b.lute", &clean_quest_doc("shared", "run.b"));

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(1));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false, "{v}");
    let files = v["files"].as_array().expect("files array");
    assert_eq!(files.len(), 2, "{v}");
    // Neither per-file result carries its own E-QUEST-ID-DUP -- the
    // project-wide pass is the sole authority (never a per-file copy AND a
    // project-wide copy of the same collision).
    for f in files {
        let diags = f["diagnostics"].as_array().expect("diagnostics array");
        assert!(
            !diags.iter().any(|d| d["code"] == "E-QUEST-ID-DUP"),
            "per-file result must not carry E-QUEST-ID-DUP: {v}"
        );
    }
    let project_diags = v["project_diagnostics"].as_array().expect("project_diagnostics array");
    assert_eq!(project_diags.len(), 1, "{v}");
    assert_eq!(project_diags[0]["code"], "E-QUEST-ID-DUP");
    assert!(
        project_diags[0]["path"]
            .as_str()
            .is_some_and(|p| p.ends_with("b.lute")),
        "anchored in the SECOND file: {v}"
    );
}

// --- clean project (distinct ids) -------------------------------------------

#[test]
fn check_project_clean_project_with_distinct_quest_ids_exits_zero() {
    let dir = temp_dir("clean");
    write(&dir, "a.lute", &clean_quest_doc("questA", "run.a"));
    // Nested subdirectory -- the walk must be recursive.
    write(&dir, "sub/b.lute", &clean_quest_doc("questB", "run.b"));

    let out = run(&["check-project", dir.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "distinct quest ids across files must exit zero: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let out_json = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out_json.stdout).unwrap();
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["files"].as_array().unwrap().len(), 2, "{v}");
    assert!(v["project_diagnostics"].as_array().unwrap().is_empty(), "{v}");
}

// --- an unrelated per-file error still surfaces + fails the run ------------

#[test]
fn check_project_reports_unrelated_per_file_error_and_exits_nonzero() {
    let dir = temp_dir("unrelated-error");
    write(&dir, "ok.lute", &clean_quest_doc("questA", "run.a"));
    // `run.missing` is never declared in `state:` -> E-UNDECLARED, nothing to
    // do with quest-id uniqueness at all.
    write(
        &dir,
        "bad.lute",
        "---\nkind: quest\n---\n<quest id=\"questB\">\n\
         <objective id=\"o\" done=\"run.missing\"/>\n</quest>\n",
    );

    let out = run(&["check-project", dir.to_str().unwrap()]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "an unrelated per-file error must still fail the run: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("E-UNDECLARED"), "{stdout}");
    assert!(
        stdout.contains("ok: ") && stdout.contains("ok.lute"),
        "the clean file's per-file check must still pass: {stdout}"
    );
    assert!(
        !stdout.contains("E-QUEST-ID-DUP"),
        "distinct quest ids must never spuriously collide: {stdout}"
    );
}

// --- import-linked collision: never double-reported ------------------------

#[test]
fn check_project_import_linked_dup_is_not_double_reported() {
    // `a.lute` `uses:` `b.lute`; both declare `<quest id="q">`. Pre-0.2.1,
    // `lute check a.lute` ALONE already reports this (F4, seeded via
    // `imported_quest_ids`). `check-project`'s project-wide pass ALSO sees
    // the same two files declaring `q` -- the SAME real-world collision must
    // surface as exactly ONE E-QUEST-ID-DUP across the whole report, not a
    // per-file copy plus a project-wide copy.
    let dir = temp_dir("import-linked-dup");
    // The `uses:` TARGET is a schema-shaped import (no `kind:`) that still
    // happens to declare a `<quest>` -- `resolve_imports` reads `<quest>` ids
    // from any successfully-parsed import target (kind-agnostic).
    write(
        &dir,
        "b.lute",
        "---\nstate:\n  run.b: { type: bool, default: false }\n---\n\
         <quest id=\"q\">\n<objective id=\"ob\" done=\"run.b\"/>\n</quest>\n",
    );
    write(
        &dir,
        "a.lute",
        "---\nkind: quest\nuses: b.lute\nstate:\n  run.a: { type: bool, default: false }\n\
         ---\n<quest id=\"q\">\n<objective id=\"oa\" done=\"run.a\"/>\n</quest>\n",
    );

    let out = run(&["check-project", dir.to_str().unwrap()]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    let dup_count = stdout.matches("E-QUEST-ID-DUP").count();
    assert_eq!(
        dup_count, 1,
        "one real-world collision must be reported exactly once: {stdout}"
    );

    // Same assertion, structurally, via --json: sum diagnostics carrying
    // E-QUEST-ID-DUP across EVERY file's own result plus the project-wide
    // list.
    let out_json = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out_json.stdout).unwrap();
    let mut total = 0usize;
    for f in v["files"].as_array().unwrap() {
        total += f["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| d["code"] == "E-QUEST-ID-DUP")
            .count();
    }
    total += v["project_diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["code"] == "E-QUEST-ID-DUP")
        .count();
    assert_eq!(total, 1, "{v}");
}

// --- misc CLI behavior -------------------------------------------------------

#[test]
fn check_project_nonexistent_dir_exits_two() {
    let dir = temp_dir("missing");
    std::fs::remove_dir_all(&dir).ok();
    let out = run(&["check-project", dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn check_project_empty_dir_exits_zero() {
    let dir = temp_dir("empty");
    let out = run(&["check-project", dir.to_str().unwrap()]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
}
