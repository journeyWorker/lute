//! `lute check-project <dir>` acceptance for NESTED subprojects (0.3.0
//! triage fix): each walked file's project root is resolved as its OWN
//! nearest-ancestor directory containing a `lute.project.yaml` (bounded
//! below by the walk root), not pooled against the single walk root for
//! every file. Project-wide `<quest id>` uniqueness (dsl 0.2.0 §6.3) is
//! scoped PER RESOLVED PROJECT ROOT to match: two different subprojects each
//! declaring the same quest id is NOT a collision; the same id declared
//! twice within ONE resolved root still is.

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

/// A fresh unique temp dir (matches `check_project.rs`'s own helper — each
/// integration test binary is compiled separately, so this is intentionally
/// duplicated rather than shared).
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

/// A minimal valid core-only `lute.project.yaml` (mirrors
/// `docs/examples/lute.project.yaml`): a single `core` profile activating no
/// plugins, so any `.lute` file resolved against this root stays core-only.
fn core_only_project_yaml() -> String {
    "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n".to_string()
}

/// A self-contained, otherwise-CLEAN `kind: quest` doc declaring exactly one
/// quest id (its own state decl + a `done` slot that reads it, so no other
/// diagnostic — E-UNDECLARED/E-MAYBE-UNSET/etc — fires). Mirrors
/// `check_project.rs`'s own helper.
fn clean_quest_doc(quest_id: &str, state_path: &str) -> String {
    format!(
        "---\nkind: quest\nstate:\n  {state_path}: {{ type: bool, default: false }}\n---\n\
         <quest id=\"{quest_id}\">\n<objective id=\"o\" done=\"{state_path}\"/>\n</quest>\n"
    )
}

// --- Test A: same quest id in two DIFFERENT subproject roots is not a dup --

#[test]
fn check_project_same_quest_id_in_sibling_subprojects_is_not_a_collision() {
    // Nested layout:
    //   tmp/lute.project.yaml        (core-only walk-root project)
    //   tmp/subA/lute.project.yaml   (core-only subproject)
    //   tmp/subA/quest.lute          <quest id="dup">
    //   tmp/subB/lute.project.yaml   (core-only subproject)
    //   tmp/subB/quest.lute          <quest id="dup">
    //
    // `subA/quest.lute`'s and `subB/quest.lute`'s nearest ancestor with a
    // `lute.project.yaml` is their OWN subdirectory, not `tmp` -- so they
    // resolve against DIFFERENT project roots and the same id declared in
    // each is not a project-wide collision.
    let dir = temp_dir("sibling-subprojects-same-id");
    write(&dir, "lute.project.yaml", &core_only_project_yaml());
    write(&dir, "subA/lute.project.yaml", &core_only_project_yaml());
    write(&dir, "subA/quest.lute", &clean_quest_doc("dup", "run.a"));
    write(&dir, "subB/lute.project.yaml", &core_only_project_yaml());
    write(&dir, "subB/quest.lute", &clean_quest_doc("dup", "run.b"));

    let out = run(&["check-project", dir.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "same quest id in two DIFFERENT subproject roots must not collide: {stdout}"
    );
    assert!(
        !stdout.contains("E-QUEST-ID-DUP"),
        "no cross-subproject dup expected: {stdout}"
    );

    let out_json = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out_json.stdout).unwrap();
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(v["files"].as_array().unwrap().len(), 2, "{v}");
    assert!(
        v["project_diagnostics"].as_array().unwrap().is_empty(),
        "{v}"
    );
}

// --- Test B: same quest id TWICE within ONE resolved project root still --
// --- collides (regression guard against over-scoping the grouping) -------

#[test]
fn check_project_same_quest_id_twice_within_one_project_root_still_collides() {
    // Nested layout:
    //   tmp2/lute.project.yaml      (core-only walk-root project)
    //   tmp2/subC/lute.project.yaml (core-only subproject)
    //   tmp2/subC/a.lute            <quest id="dup">
    //   tmp2/subC/b.lute            <quest id="dup">
    //
    // Both `a.lute` and `b.lute` resolve to the SAME nearest-ancestor
    // project root (`subC`), so the shared id must still be flagged.
    let dir = temp_dir("one-subproject-same-id-twice");
    write(&dir, "lute.project.yaml", &core_only_project_yaml());
    write(&dir, "subC/lute.project.yaml", &core_only_project_yaml());
    write(&dir, "subC/a.lute", &clean_quest_doc("dup", "run.a"));
    write(&dir, "subC/b.lute", &clean_quest_doc("dup", "run.b"));

    let out = run(&["check-project", dir.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(1),
        "same quest id twice within ONE resolved project root must still collide: {stdout}"
    );
    assert!(stdout.contains("E-QUEST-ID-DUP"), "{stdout}");
    assert!(stdout.contains("a.lute"), "must name file a: {stdout}");
    assert!(stdout.contains("b.lute"), "must name file b: {stdout}");

    let out_json = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out_json.stdout).unwrap();
    assert_eq!(v["ok"], false, "{v}");
    let project_diags = v["project_diagnostics"].as_array().unwrap();
    assert_eq!(project_diags.len(), 1, "{v}");
    assert_eq!(project_diags[0]["code"], "E-QUEST-ID-DUP");
}
