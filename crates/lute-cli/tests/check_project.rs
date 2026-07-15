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

// --- F1 (0.2.1 review): import-graph dup reaching OUTSIDE the walked dir ---

#[test]
fn check_project_flags_import_graph_dup_reaching_outside_walked_dir() {
    // `scene.lute` (inside the walked dir) `uses:` TWO docs OUTSIDE the
    // walked dir, both declaring `<quest id="q">`. Neither import target is
    // ever seen by `check_project_quest_ids` (it only walks `dir`), so the
    // ONLY surface that can catch this collision at all is `check()`'s own
    // import-graph resolver (`resolve_imports`) running on `scene.lute`
    // itself. RED before the fix: `run_check_project` blanket-stripped every
    // per-file `E-QUEST-ID-DUP` and trusted the project-wide pass as sole
    // authority, so this real collision was silently swallowed -> exit 0.
    let root = temp_dir("f1-out-of-dir-dup");
    let dir = root.join("proj");
    write(
        &root,
        "outside/doc1.lute",
        "---\nstate:\n  run.o1: { type: bool, default: false }\n---\n\
         <quest id=\"q\">\n<objective id=\"o1\" done=\"run.o1\"/>\n</quest>\n",
    );
    write(
        &root,
        "outside/doc2.lute",
        "---\nstate:\n  run.o2: { type: bool, default: false }\n---\n\
         <quest id=\"q\">\n<objective id=\"o2\" done=\"run.o2\"/>\n</quest>\n",
    );
    write(
        &dir,
        "scene.lute",
        "---\nkind: quest\nuses:\n  - ../outside/doc1.lute\n  - ../outside/doc2.lute\n\
         state:\n  run.scene: { type: bool, default: false }\n---\n\
         <quest id=\"scene_q\">\n<objective id=\"oscene\" done=\"run.scene\"/>\n</quest>\n",
    );

    let out = run(&["check-project", dir.to_str().unwrap()]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "an import-graph collision reaching outside the walked dir must still fail the run: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("E-QUEST-ID-DUP"), "{stdout}");

    // Structurally: the project-wide pass has NOTHING to say here (neither
    // import target is in the walked set); the dup must come from
    // scene.lute's own per-file result instead.
    let out_json = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out_json.stdout).unwrap();
    assert_eq!(v["ok"], false, "{v}");
    assert!(
        v["project_diagnostics"].as_array().unwrap().is_empty(),
        "the project-wide pass cannot see either out-of-dir doc: {v}"
    );
    let files = v["files"].as_array().unwrap();
    assert_eq!(files.len(), 1, "{v}");
    assert!(
        files[0]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "E-QUEST-ID-DUP"),
        "scene.lute's own per-file result must carry the collision: {v}"
    );
}

// --- F2 (0.2.1 review): a symlinked alias must not double-count a doc ------

#[cfg(unix)]
#[test]
fn check_project_symlink_alias_does_not_fabricate_a_cross_file_dup() {
    // `alias.lute` is a symlink to `a.lute` -- the SAME physical document
    // reachable under two path strings. RED before the fix: `find_lute_files`
    // pushed both path strings verbatim, so `check_project_quest_ids` saw the
    // SAME `<quest id="q">` "twice" (once per path) and reported a false
    // cross-file `E-QUEST-ID-DUP`.
    let dir = temp_dir("f2-symlink-alias");
    let real = write(&dir, "a.lute", &clean_quest_doc("q", "run.a"));
    let alias = dir.join("alias.lute");
    std::os::unix::fs::symlink(&real, &alias).unwrap();

    let out = run(&["check-project", dir.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "a symlink alias to an already-walked doc must not fabricate a cross-file dup: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("E-QUEST-ID-DUP"),
        "false dup from one physical doc counted twice: {stdout}"
    );

    let out_json = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out_json.stdout).unwrap();
    assert_eq!(v["ok"], true, "{v}");
    assert_eq!(
        v["files"].as_array().unwrap().len(),
        1,
        "the alias must be deduped to ONE physical doc, not checked twice: {v}"
    );
}

#[cfg(unix)]
#[test]
fn check_project_broken_symlink_exits_two_not_panic() {
    // A dangling symlink can't be canonicalized -- must surface as the SAME
    // io-error convention as every other walk failure ("never silently
    // under-report"), never panic.
    let dir = temp_dir("f2-broken-symlink");
    let missing = dir.join("missing.lute");
    let broken = dir.join("broken.lute");
    std::os::unix::fs::symlink(&missing, &broken).unwrap();

    let out = run(&["check-project", dir.to_str().unwrap()]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "an unresolvable symlink must be an io error, not a panic or a silent skip: {}",
        String::from_utf8_lossy(&out.stdout)
    );
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

// --- dsl 0.5.1 §1.4: `W-QUEST-REF-UNKNOWN` -----------------------------------

/// A self-contained `kind: quest` doc declaring `<quest id quest_id>` with
/// exactly one `<objective id objective_id>` (its own state decl + a `done`
/// slot that reads it, so no other diagnostic fires).
fn quest_doc_with(quest_id: &str, objective_id: &str, state_path: &str) -> String {
    format!(
        "---\nkind: quest\nstate:\n  {state_path}: {{ type: bool, default: false }}\n---\n\
         <quest id=\"{quest_id}\">\n<objective id=\"{objective_id}\" done=\"{state_path}\"/>\n</quest>\n"
    )
}

/// A `kind: scene` doc exhaustively matching the reserved
/// `quest.<quest_id>.state` path (dsl 0.2.0 §5.2 domain) -- check-clean on
/// its own regardless of whether `quest_id` names a real project quest
/// (single-file `check` never validates cross-document quest existence,
/// dsl 0.5.1 §1.4).
fn scene_matching_quest_state(quest_id: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
         <match on=\"quest.{quest_id}.state\">\n\
         <when is=\"active\">\n@x: is-active\n</when>\n\
         <when is=\"complete\">\n@x: is-complete\n</when>\n\
         <when is=\"failed\">\n@x: is-failed\n</when>\n\
         <when is=\"unset\">\n@x: is-unset\n</when>\n\
         </match>\n"
    )
}

/// A `kind: scene` doc exhaustively matching the reserved
/// `quest.<quest_id>.objectives.<objective_id>.done` path.
fn scene_matching_quest_objective_done(quest_id: &str, objective_id: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 2\n---\n## Shot 1.\n\
         <match on=\"quest.{quest_id}.objectives.{objective_id}.done\">\n\
         <when is=\"true\">\n@x: is-true\n</when>\n\
         <when is=\"false\">\n@x: is-false\n</when>\n\
         </match>\n"
    )
}

#[test]
fn check_project_quest_ref_to_a_defined_quest_state_emits_no_warning() {
    let dir = temp_dir("quest-ref-known-state");
    write(&dir, "heist.lute", &quest_doc_with("heist", "steal", "run.steal"));
    write(&dir, "scene.lute", &scene_matching_quest_state("heist"));

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true, "{v}");
    let project_diags = v["project_diagnostics"].as_array().expect("project_diagnostics array");
    assert!(
        !project_diags.iter().any(|d| d["code"] == "W-QUEST-REF-UNKNOWN"),
        "a reference to a quest the project actually defines must not warn: {v}"
    );
}

#[test]
fn check_project_flags_mistyped_quest_id_reference() {
    // Project defines `heist`; the scene reads `quest.heits.state` (typo).
    let dir = temp_dir("quest-ref-typo");
    write(&dir, "heist.lute", &quest_doc_with("heist", "steal", "run.steal"));
    write(&dir, "scene.lute", &scene_matching_quest_state("heits"));

    let out = run(&["check-project", dir.to_str().unwrap()]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "a W-QUEST-REF-UNKNOWN warning must never flip the exit verdict to error: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("W-QUEST-REF-UNKNOWN"), "{stdout}");
    assert!(stdout.contains("scene.lute"), "must name the referencing doc: {stdout}");
    assert!(stdout.contains("quest.heits.state"), "must name the path: {stdout}");

    let out_json = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out_json.stdout).unwrap();
    assert_eq!(v["ok"], true, "{v}");
    let project_diags = v["project_diagnostics"].as_array().expect("project_diagnostics array");
    assert_eq!(project_diags.len(), 1, "{v}");
    assert_eq!(project_diags[0]["code"], "W-QUEST-REF-UNKNOWN");
    assert_eq!(project_diags[0]["severity"], "warning");
    assert!(
        project_diags[0]["path"].as_str().is_some_and(|p| p.ends_with("scene.lute")),
        "anchored in the referencing scene: {v}"
    );
}

#[test]
fn check_project_flags_reference_to_an_undefined_objective_under_a_defined_quest() {
    // Project defines `heist` with objective `steal`; the scene reads
    // `quest.heist.objectives.bogus.done` -- the quest exists, but that
    // objective does not.
    let dir = temp_dir("quest-ref-bad-objective");
    write(&dir, "heist.lute", &quest_doc_with("heist", "steal", "run.steal"));
    write(&dir, "scene.lute", &scene_matching_quest_objective_done("heist", "bogus"));

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stdout));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let project_diags = v["project_diagnostics"].as_array().expect("project_diagnostics array");
    assert_eq!(project_diags.len(), 1, "{v}");
    assert_eq!(project_diags[0]["code"], "W-QUEST-REF-UNKNOWN");
    assert!(
        project_diags[0]["message"]
            .as_str()
            .is_some_and(|m| m.contains("bogus") && m.contains("heist")),
        "{v}"
    );
}

#[test]
fn single_file_check_never_emits_quest_ref_unknown() {
    // Standalone `lute check` has no cross-document quest graph (dsl 0.5.1
    // §1.4: "Single-file `lute check` ... does not and cannot emit it").
    let dir = temp_dir("quest-ref-single-file");
    let scene = write(&dir, "scene.lute", &scene_matching_quest_state("heits"));

    let out = run(&["check", scene.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        !stdout.contains("W-QUEST-REF-UNKNOWN"),
        "single-file check must never emit the project-only warning: {stdout}"
    );

    let out_json = run(&["check", scene.to_str().unwrap(), "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out_json.stdout).unwrap();
    let diags = v["diagnostics"].as_array().expect("diagnostics array");
    assert!(
        !diags.iter().any(|d| d["code"] == "W-QUEST-REF-UNKNOWN"),
        "{v}"
    );
}

#[test]
fn check_project_clean_project_still_exits_zero_with_quest_refs_present() {
    // Preserve existing behavior: a project with a valid quest and a scene
    // that legitimately reads BOTH reserved shapes on it stays exit 0 with
    // an empty project_diagnostics list.
    let dir = temp_dir("quest-ref-clean");
    write(&dir, "heist.lute", &quest_doc_with("heist", "steal", "run.steal"));
    write(&dir, "state-scene.lute", &scene_matching_quest_state("heist"));
    write(
        &dir,
        "objective-scene.lute",
        &scene_matching_quest_objective_done("heist", "steal"),
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stdout));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true, "{v}");
    assert!(v["project_diagnostics"].as_array().unwrap().is_empty(), "{v}");
}

// ---------------------------------------------------------------------
// Connectivity T11: `E-STATE-MAYBE-UNAVAILABLE` envelope diagnostic MUST
// RECONCILE with the per-file `E-MAYBE-UNSET` `check()` already emits for
// the SAME entry-dependent read -- never coexist alongside it (mirrors the
// `E-QUEST-ID-DUP` retain-pass precedent). A read that falls back to entry
// state earns `E-MAYBE-UNSET` from every per-file `check()` call; at
// project scope that diagnostic must be REPLACED by the envelope's own
// verdict: dropped silently when `Guaranteed`, dropped-and-suppressed when
// `Possible\Guaranteed` (warning grade, never surfaced by default), or
// dropped-and-replaced by `E-STATE-MAYBE-UNAVAILABLE` when `∉ Possible`.
// ---------------------------------------------------------------------

/// A scene doc declaring `after: visited(<after_key>)` (or no `after` at
/// all when `after_key` is empty) that reads `run.z` (declared, no
/// default) via a plain `::set` RHS -- the entry-dependent read shape
/// every reconciliation test below needs.
fn scene_reading_run_z(character: &str, after_expr: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: {character}\nseason: 1\nepisode: 1\n{after_expr}\
         state:\n  run.z: {{ type: number }}\n  run.out: {{ type: number }}\n---\n\
         ## Shot 1.\n::set{{run.out = run.z}}\n"
    )
}

#[test]
fn envelope_guaranteed_read_drops_the_reconciled_maybe_unset_and_exits_zero() {
    // `y` is the ONLY predecessor route and unconditionally sets `run.z` --
    // `run.z ∈ Guaranteed(x)`. Per-file `check()` on `x` alone flags
    // `E-MAYBE-UNSET` (it can't see the project); at project scope that
    // diagnostic MUST be reconciled away with no replacement.
    let dir = temp_dir("envelope-guaranteed");
    let y = "---\nkind: scene\ncharacter: y\nseason: 1\nepisode: 1\nstate:\n  run.z: { type: number }\n---\n## Shot 1.\n::set{run.z = 1}\n";
    write(&dir, "y.lute", y);
    write(&dir, "x.lute", &scene_reading_run_z("x", "after: 'visited(\"y.s01ep01\")'\n"));

    let out_x = run(&["check", dir.join("x.lute").to_str().unwrap()]);
    assert!(
        !out_x.status.success(),
        "x.lute alone must flag E-MAYBE-UNSET standalone (can't see the project): {}",
        String::from_utf8_lossy(&out_x.stdout)
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stdout));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true, "{v}");
    assert!(v["project_diagnostics"].as_array().unwrap().is_empty(), "{v}");
    for f in v["files"].as_array().unwrap() {
        let diags = f["diagnostics"].as_array().unwrap();
        assert!(
            !diags.iter().any(|d| d["code"] == "E-MAYBE-UNSET"),
            "the reconciled read must not keep its per-file E-MAYBE-UNSET: {v}"
        );
        assert!(diags.iter().all(|d| d["code"] != "E-STATE-MAYBE-UNAVAILABLE"));
    }
}

#[test]
fn envelope_possible_not_guaranteed_read_is_fully_suppressed_by_default() {
    // `after: visited(a) || visited(b)`; `a` unconditionally sets `run.z`,
    // `b` never does -- `run.z ∈ Possible(x) \ Guaranteed(x)`, warning
    // grade, default-suppressed. Project scope MUST exit 0 with NEITHER
    // the per-file E-MAYBE-UNSET NOR any E-STATE-MAYBE-UNAVAILABLE
    // (error or otherwise) anywhere in the default (human or --json)
    // output -- and no `envelope_warnings` key at all (T14 territory).
    let dir = temp_dir("envelope-possible-not-guaranteed");
    let a = "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\nstate:\n  run.z: { type: number }\n---\n## Shot 1.\n::set{run.z = 1}\n";
    let b = "---\nkind: scene\ncharacter: b\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@narrator: hi\n";
    write(&dir, "a.lute", a);
    write(&dir, "b.lute", b);
    write(
        &dir,
        "x.lute",
        &scene_reading_run_z("x", "after: 'visited(\"a.s01ep01\") || visited(\"b.s01ep01\")'\n"),
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stdout));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true, "{v}");
    assert!(v["project_diagnostics"].as_array().unwrap().is_empty(), "{v}");
    assert!(
        v.get("envelope_warnings").is_none(),
        "the warning grade must not be surfaced anywhere in default check-project output: {v}"
    );
    for f in v["files"].as_array().unwrap() {
        let diags = f["diagnostics"].as_array().unwrap();
        assert!(
            diags.iter().all(|d| d["code"] != "E-MAYBE-UNSET" && d["code"] != "E-STATE-MAYBE-UNAVAILABLE"),
            "no diagnostic at all for a Possible-but-not-Guaranteed read by default: {v}"
        );
    }

    let out_human = run(&["check-project", dir.to_str().unwrap()]);
    assert_eq!(out_human.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out_human.stdout);
    assert!(!stdout.contains("E-MAYBE-UNSET"), "{stdout}");
    assert!(!stdout.contains("E-STATE-MAYBE-UNAVAILABLE"), "{stdout}");
}

#[test]
fn envelope_never_possible_read_replaces_maybe_unset_with_state_unavailable_error() {
    // `y` is the ONLY predecessor route and NEVER sets `run.z` -- `run.z ∉
    // Possible(x)`. Project scope MUST replace the per-file `E-MAYBE-UNSET`
    // with a project-wide error-grade `E-STATE-MAYBE-UNAVAILABLE` (never
    // both at once), and the wording must carry the declared-routes
    // qualifier verbatim.
    let dir = temp_dir("envelope-never-possible");
    let y = "---\nkind: scene\ncharacter: y\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@narrator: hi\n";
    write(&dir, "y.lute", y);
    write(&dir, "x.lute", &scene_reading_run_z("x", "after: 'visited(\"y.s01ep01\")'\n"));

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(1), "{}", String::from_utf8_lossy(&out.stdout));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false, "{v}");
    let project_diags = v["project_diagnostics"].as_array().unwrap();
    assert_eq!(project_diags.len(), 1, "{v}");
    assert_eq!(project_diags[0]["code"], "E-STATE-MAYBE-UNAVAILABLE");
    assert_eq!(project_diags[0]["severity"], "error");
    assert!(
        project_diags[0]["message"]
            .as_str()
            .is_some_and(|m| m.contains("under your declared routes")),
        "{v}"
    );
    for f in v["files"].as_array().unwrap() {
        let diags = f["diagnostics"].as_array().unwrap();
        assert!(
            !diags.iter().any(|d| d["code"] == "E-MAYBE-UNSET"),
            "the reconciled read must not keep its per-file E-MAYBE-UNSET: {v}"
        );
    }
}

#[test]
fn envelope_tainted_node_leaves_maybe_unset_untouched() {
    // `after` references an UNRESOLVABLE `visited()` target -- the node is
    // tainted (`propagate`'s own unreliable D/D placeholder). The
    // reconciliation pass must NOT touch this node's reads at all: the
    // per-file `E-MAYBE-UNSET` stays exactly as `check()` reported it, and
    // no `E-STATE-MAYBE-UNAVAILABLE` is ever added for it.
    let dir = temp_dir("envelope-tainted");
    write(
        &dir,
        "x.lute",
        &scene_reading_run_z("x", "after: 'visited(\"ghost.s01ep01\")'\n"),
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(1), "{}", String::from_utf8_lossy(&out.stdout));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        !v["project_diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "E-STATE-MAYBE-UNAVAILABLE"),
        "a tainted node's Env is untrustworthy -- must never seed E-STATE-MAYBE-UNAVAILABLE: {v}"
    );
    let files = v["files"].as_array().unwrap();
    let x = files.iter().find(|f| f["path"].as_str().unwrap().ends_with("x.lute")).unwrap();
    assert!(
        x["diagnostics"].as_array().unwrap().iter().any(|d| d["code"] == "E-MAYBE-UNSET"),
        "a tainted node's per-file E-MAYBE-UNSET must be left untouched: {v}"
    );
}

#[test]
fn envelope_reconciliation_is_per_node_across_a_cycle() {
    // Per-node cycle recovery (spec §4.1): a p <-> q prerequisite cycle
    // excludes ONLY its own members (and anything downstream) from
    // `topo_order`, so `propagate` still builds a REAL `envs` entry for every
    // cycle-INDEPENDENT node. Two contrasting non-cyclic scenes both read
    // `run.z` (declared, never set on any route -> E-MAYBE-UNSET standalone):
    //   * `x` is a cycle-INDEPENDENT entry -> it HAS a real `envs` entry, so
    //     reconciliation runs: its per-file E-MAYBE-UNSET is REPLACED by the
    //     project-wide error-grade E-STATE-MAYBE-UNAVAILABLE (never both).
    //   * `d` is DOWNSTREAM of the cycle (`after: visited(p)`), so it is
    //     excluded from `topo_order`/`envs` -> `check_envelope` skips it and
    //     reconciliation leaves its genuine E-MAYBE-UNSET untouched (nothing
    //     trustworthy to reclassify against). This keeps coverage of the
    //     still-excluded/untrusted path the old whole-root wipeout exercised.
    // E-CONN-CYCLE still fires. Before per-node recovery the WHOLE root's
    // envs was emptied, so BOTH x and d kept their raw E-MAYBE-UNSET and x
    // never earned its project-wide envelope verdict -- that was the bug this
    // test used to encode.
    let dir = temp_dir("envelope-cycle-per-node");
    let p = "---\nkind: scene\ncharacter: p\nseason: 1\nepisode: 1\nafter: 'visited(\"q.s01ep01\")'\n---\n## Shot 1.\n@narrator: hi\n";
    let q = "---\nkind: scene\ncharacter: q\nseason: 1\nepisode: 1\nafter: 'visited(\"p.s01ep01\")'\n---\n## Shot 1.\n@narrator: hi\n";
    write(&dir, "p.lute", p);
    write(&dir, "q.lute", q);
    write(&dir, "x.lute", &scene_reading_run_z("x", ""));
    write(&dir, "d.lute", &scene_reading_run_z("d", "after: 'visited(\"p.s01ep01\")'\n"));

    // Red proof: both cycle-independent `x` and downstream `d` flag
    // E-MAYBE-UNSET standalone (neither can see the project).
    for f in ["x.lute", "d.lute"] {
        let out_f = run(&["check", dir.join(f).to_str().unwrap()]);
        assert!(
            !out_f.status.success(),
            "{f} alone must flag E-MAYBE-UNSET standalone: {}",
            String::from_utf8_lossy(&out_f.stdout)
        );
    }

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(1), "{}", String::from_utf8_lossy(&out.stdout));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let project_diags = v["project_diagnostics"].as_array().unwrap();
    assert!(
        project_diags.iter().any(|d| d["code"] == "E-CONN-CYCLE"),
        "the p<->q cycle must still be reported: {v}"
    );

    // Cycle-INDEPENDENT `x`: reconciled -> per-file E-MAYBE-UNSET GONE, and an
    // error-grade E-STATE-MAYBE-UNAVAILABLE now stands in for it project-wide.
    assert!(
        project_diags.iter().any(|d| d["code"] == "E-STATE-MAYBE-UNAVAILABLE"
            && d["severity"] == "error"
            && d["path"].as_str().is_some_and(|p| p.ends_with("x.lute"))),
        "cycle-independent x must earn its project-wide E-STATE-MAYBE-UNAVAILABLE error: {v}"
    );
    let files = v["files"].as_array().unwrap();
    let x = files.iter().find(|f| f["path"].as_str().unwrap().ends_with("x.lute")).unwrap();
    assert!(
        !x["diagnostics"].as_array().unwrap().iter().any(|d| d["code"] == "E-MAYBE-UNSET"),
        "cycle-independent x's per-file E-MAYBE-UNSET must be reconciled away, not kept alongside \
         its project envelope verdict: {v}"
    );

    // DOWNSTREAM-of-cycle `d`: genuinely absent from envs -> its E-MAYBE-UNSET
    // survives untouched, and it earns NO E-STATE-MAYBE-UNAVAILABLE.
    let d = files.iter().find(|f| f["path"].as_str().unwrap().ends_with("d.lute")).unwrap();
    assert!(
        d["diagnostics"].as_array().unwrap().iter().any(|dg| dg["code"] == "E-MAYBE-UNSET"),
        "downstream-of-cycle d's genuine E-MAYBE-UNSET must survive (no trustworthy envelope to \
         reclassify against): {v}"
    );
    assert!(
        !project_diags.iter().any(|dg| dg["code"] == "E-STATE-MAYBE-UNAVAILABLE"
            && dg["path"].as_str().is_some_and(|p| p.ends_with("d.lute"))),
        "downstream-of-cycle d must NOT earn a project envelope verdict: {v}"
    );
}

#[test]
fn envelope_out_of_scope_scene_maybe_unset_survives_check_project() {
    // `scene.local` is entry-dependent (declared, no default, never
    // locally proven) but SCENE-tier -- out of the envelope's `run.*`/
    // `user.*` scope (dsl §4.3 §386-393). Reconciliation must NEVER touch
    // it: the per-file `E-MAYBE-UNSET` survives check-project untouched,
    // no `E-STATE-MAYBE-UNAVAILABLE` is ever produced for it.
    let dir = temp_dir("envelope-out-of-scope-scene");
    write(
        &dir,
        "x.lute",
        "---\nkind: scene\ncharacter: x6\nseason: 1\nepisode: 1\nstate:\n  scene.local: { type: number }\n---\n## Shot 1.\n@narrator: value {{scene.local}}\n",
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(1), "{}", String::from_utf8_lossy(&out.stdout));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false, "{v}");
    assert!(
        !v["project_diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "E-STATE-MAYBE-UNAVAILABLE"),
        "scene.* is never envelope-classified: {v}"
    );
    let files = v["files"].as_array().unwrap();
    let x = files.iter().find(|f| f["path"].as_str().unwrap().ends_with("x.lute")).unwrap();
    assert!(
        x["diagnostics"].as_array().unwrap().iter().any(|d| d["code"] == "E-MAYBE-UNSET"),
        "an out-of-scope scene.* read's E-MAYBE-UNSET must survive reconciliation: {v}"
    );
}

#[test]
fn envelope_out_of_scope_quest_maybe_unset_survives_check_project() {
    // `quest.foo.state` is a reserved, always-declared, never-defaulted
    // read (dsl 0.2.0 §5.2) -- entry-dependent by defassign's rules, but
    // QUEST-tier -- out of the envelope's `run.*`/`user.*` scope entirely
    // (dsl §4.3 §386-393: quest lifecycle is read via `completed()`, never
    // this lattice). Its per-file `E-MAYBE-UNSET` must survive untouched.
    let dir = temp_dir("envelope-out-of-scope-quest");
    write(
        &dir,
        "x.lute",
        "---\nkind: scene\ncharacter: x7\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@narrator: value {{quest.foo.state}}\n",
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(1), "{}", String::from_utf8_lossy(&out.stdout));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        !v["project_diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "E-STATE-MAYBE-UNAVAILABLE"),
        "quest.* is never envelope-classified: {v}"
    );
    let files = v["files"].as_array().unwrap();
    let x = files.iter().find(|f| f["path"].as_str().unwrap().ends_with("x.lute")).unwrap();
    assert!(
        x["diagnostics"].as_array().unwrap().iter().any(|d| d["code"] == "E-MAYBE-UNSET"),
        "an out-of-scope quest.* read's E-MAYBE-UNSET must survive reconciliation: {v}"
    );
}

#[test]
fn envelope_mixed_slot_span_collision_only_reconciles_the_in_scope_path() {
    // `run.out = run.upstream && scene.local` -- BOTH reads sit in the
    // SAME CEL slot, so `check_read` fires each with the IDENTICAL `Span`
    // (defassign.rs has no per-path span within one slot). `run.upstream`
    // is Guaranteed at `x` (its only predecessor route, `y8`, sets it
    // unconditionally) -- reconciled away. `scene.local` is scene-tier,
    // out of scope, and genuinely never set -- its E-MAYBE-UNSET at that
    // SAME span must survive. A span-only match would wrongly drop BOTH.
    let dir = temp_dir("envelope-mixed-slot-collision");
    let y = "---\nkind: scene\ncharacter: y8\nseason: 1\nepisode: 1\nstate:\n  run.upstream: { type: bool }\n---\n## Shot 1.\n::set{run.upstream = true}\n";
    write(&dir, "y.lute", y);
    write(
        &dir,
        "x.lute",
        "---\nkind: scene\ncharacter: x8\nseason: 1\nepisode: 1\nafter: 'visited(\"y8.s01ep01\")'\nstate:\n  run.upstream: { type: bool }\n  scene.local: { type: bool }\n  run.out: { type: bool }\n---\n## Shot 1.\n::set{run.out = run.upstream && scene.local}\n",
    );

    // Standalone red proof: BOTH reads flag E-MAYBE-UNSET at the same span.
    let out_x = run(&["check", dir.join("x.lute").to_str().unwrap(), "--json"]);
    let vx: serde_json::Value = serde_json::from_slice(&out_x.stdout).unwrap();
    let unset: Vec<&serde_json::Value> = vx["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["code"] == "E-MAYBE-UNSET")
        .collect();
    assert_eq!(unset.len(), 2, "expected both reads to flag standalone: {vx}");
    assert_eq!(unset[0]["span"], unset[1]["span"], "both reads must share the same slot span: {vx}");

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(1), "{}", String::from_utf8_lossy(&out.stdout));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false, "{v}");
    assert!(
        !v["project_diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "E-STATE-MAYBE-UNAVAILABLE"),
        "run.upstream is Guaranteed -> no envelope diagnostic at all: {v}"
    );
    let files = v["files"].as_array().unwrap();
    let x = files.iter().find(|f| f["path"].as_str().unwrap().ends_with("x.lute")).unwrap();
    let remaining: Vec<&serde_json::Value> = x["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["code"] == "E-MAYBE-UNSET")
        .collect();
    assert_eq!(
        remaining.len(),
        1,
        "exactly the scene.local site must survive reconciliation, run.upstream's must not: {v}"
    );
    assert!(
        remaining[0]["message"].as_str().unwrap().contains("scene.local"),
        "the surviving E-MAYBE-UNSET must be scene.local's, not run.upstream's: {v}"
    );
}

// --- Defect B (persona review): `completed(Q)` unreachable via a dead
// REQUIRED objective ---------------------------------------------------
//
// dsl 0.4.0 §8.2 rule C4 deliberately suppresses a standalone
// `E-QUEST-UNREACHABLE` for a quest dead via a required objective (that
// cause rides as a note on `E-OBJECTIVE-UNSATISFIABLE` instead), so
// `completed(Q)` reachability must consult the underlying objective-
// liveness signal directly, never the (C4-suppressed) diagnostic code.

/// Scalar flavor: a REQUIRED objective whose `done` predicate decides
/// false directly (`decide_slot`) must mark its quest unreachable-to-
/// complete, so a scene gated on `completed(Q)` earns `E-CONN-UNREACHABLE`
/// -- even though NO standalone `E-QUEST-UNREACHABLE` is ever emitted for
/// this cause (C4).
#[test]
fn dead_required_objective_scalar_marks_completed_gate_unreachable() {
    let dir = temp_dir("dead-required-objective-scalar");
    write(
        &dir,
        "deadquest.lute",
        "---\nkind: quest\nstate:\n  run.rank: { type: { enum: [novice, veteran, hero] }, \
         default: novice }\n---\n<quest id=\"deadQuest\" start=\"true\">\n\
         <objective id=\"becomeLegend\" done=\"run.rank == 'legendary'\"/>\n</quest>\n",
    );
    write(
        &dir,
        "gated.lute",
        "---\nkind: scene\ncharacter: repro\nseason: 1\nepisode: 1\n\
         after: 'completed(\"deadQuest\")'\n---\n## Shot 1.\n@narrator: hi\n",
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(1), "{}", String::from_utf8_lossy(&out.stdout));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let project_diags = v["project_diagnostics"].as_array().unwrap();
    assert!(
        project_diags.iter().any(|d| d["code"] == "E-CONN-UNREACHABLE"),
        "a scene gated on completed(Q) where Q has a dead REQUIRED (scalar) objective must be \
         E-CONN-UNREACHABLE: {v}"
    );
    // C4 unbroken: no duplicate standalone E-QUEST-UNREACHABLE anywhere.
    let files = v["files"].as_array().unwrap();
    assert!(
        !files
            .iter()
            .flat_map(|f| f["diagnostics"].as_array().unwrap())
            .any(|d| d["code"] == "E-QUEST-UNREACHABLE"),
        "C4 regression: must never emit a duplicate standalone E-QUEST-UNREACHABLE for the \
         required-objective cause: {v}"
    );
}

/// Relational flavor: a REQUIRED objective gated on a never-producible
/// relation (§4.2 second-order consequence) must ALSO mark its quest
/// unreachable-to-complete.
#[test]
fn dead_required_objective_relational_marks_completed_gate_unreachable() {
    let dir = temp_dir("dead-required-objective-relational");
    write(
        &dir,
        "deadrelquest.lute",
        "---\nkind: quest\nentities:\n  loc: { members: [a] }\nrelations:\n  \
         neverProduced: { args: [loc] }\n---\n<quest id=\"deadRelQuest\" start=\"true\">\n\
         <objective id=\"o\" done=\"holds(neverProduced(a))\"/>\n</quest>\n",
    );
    write(
        &dir,
        "gated.lute",
        "---\nkind: scene\ncharacter: repro2\nseason: 1\nepisode: 1\n\
         after: 'completed(\"deadRelQuest\")'\n---\n## Shot 1.\n@narrator: hi\n",
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(1), "{}", String::from_utf8_lossy(&out.stdout));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let project_diags = v["project_diagnostics"].as_array().unwrap();
    assert!(
        project_diags.iter().any(|d| d["code"] == "E-CONN-UNREACHABLE"),
        "a scene gated on completed(Q) where Q has a dead REQUIRED (never-producible relation) \
         objective must be E-CONN-UNREACHABLE: {v}"
    );
    assert!(
        project_diags.iter().any(|d| d["code"] == "E-OBJECTIVE-UNSATISFIABLE"),
        "the underlying relational-objective-liveness diagnostic must still fire too: {v}"
    );
}

/// An OPTIONAL dead objective must NEVER make its quest unreachable-to-
/// complete (C4's quest-level consequence is REQUIRED-only) -- the quest
/// can still complete via its other (live, required) objective, so a
/// scene gated on `completed(Q)` must stay clean.
#[test]
fn optional_dead_objective_does_not_make_quest_unreachable() {
    let dir = temp_dir("optional-dead-objective-unreachable");
    write(
        &dir,
        "mixedquest.lute",
        "---\nkind: quest\nstate:\n  run.live: { type: bool, default: true }\n---\n\
         <quest id=\"mixedQuest\" start=\"true\">\n\
         <objective id=\"deadOpt\" done=\"false\" optional/>\n\
         <objective id=\"liveReq\" done=\"run.live\"/>\n</quest>\n",
    );
    write(
        &dir,
        "gated.lute",
        "---\nkind: scene\ncharacter: repro3\nseason: 1\nepisode: 1\n\
         after: 'completed(\"mixedQuest\")'\n---\n## Shot 1.\n@narrator: hi\n",
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let project_diags = v["project_diagnostics"].as_array().unwrap();
    assert!(
        !project_diags.iter().any(|d| d["code"] == "E-CONN-UNREACHABLE"),
        "an OPTIONAL dead objective must never make the quest unreachable-to-complete: {v}"
    );
    let files = v["files"].as_array().unwrap();
    let gated = files.iter().find(|f| f["path"].as_str().unwrap().ends_with("gated.lute")).unwrap();
    assert_eq!(
        gated["diagnostics"].as_array().unwrap().len(),
        0,
        "the gated scene itself must stay clean -- its route is still live: {v}"
    );
}

/// Control (unbroken by the fix): `start="false"` still marks its quest
/// unreachable-to-complete via the REAL `E-QUEST-UNREACHABLE` lifecycle
/// signal, propagating to `E-CONN-UNREACHABLE` exactly as before.
#[test]
fn start_false_quest_still_marks_completed_gate_unreachable() {
    let dir = temp_dir("start-false-completed-gate-unreachable");
    write(
        &dir,
        "questa.lute",
        "---\nkind: quest\n---\n<quest id=\"questA\" start=\"false\">\n\
         <objective id=\"o\" done=\"true\"/>\n</quest>\n",
    );
    write(
        &dir,
        "gated.lute",
        "---\nkind: scene\ncharacter: repro\nseason: 1\nepisode: 1\n\
         after: 'completed(\"questA\")'\n---\n## Shot 1.\n@narrator: hi\n",
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(1), "{}", String::from_utf8_lossy(&out.stdout));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let project_diags = v["project_diagnostics"].as_array().unwrap();
    assert!(
        project_diags.iter().any(|d| d["code"] == "E-CONN-UNREACHABLE"),
        "the start=false control must still propagate to E-CONN-UNREACHABLE: {v}"
    );
    let files = v["files"].as_array().unwrap();
    assert!(
        files
            .iter()
            .flat_map(|f| f["diagnostics"].as_array().unwrap())
            .any(|d| d["code"] == "E-QUEST-UNREACHABLE"),
        "the REAL E-QUEST-UNREACHABLE lifecycle signal must still fire for this cause: {v}"
    );
}

/// A live quest (no dead objective, no dead lifecycle guard) stays
/// Reachable -- a scene gated on `completed(Q)` stays clean.
#[test]
fn live_quest_completed_gate_stays_reachable() {
    let dir = temp_dir("live-quest-completed-gate-reachable");
    write(
        &dir,
        "livequest.lute",
        "---\nkind: quest\nstate:\n  run.live: { type: bool, default: true }\n---\n\
         <quest id=\"liveQuest\" start=\"true\">\n\
         <objective id=\"o\" done=\"run.live\"/>\n</quest>\n",
    );
    write(
        &dir,
        "gated.lute",
        "---\nkind: scene\ncharacter: repro4\nseason: 1\nepisode: 1\n\
         after: 'completed(\"liveQuest\")'\n---\n## Shot 1.\n@narrator: hi\n",
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stdout));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true, "{v}");
    assert!(v["project_diagnostics"].as_array().unwrap().is_empty(), "{v}");
}

// --- Defect A (persona review): real spans on project-level connectivity
// diagnostics, instead of `0:0` ------------------------------------------

#[test]
fn episode_id_dup_span_points_at_character_line() {
    let dir = temp_dir("span-episode-id-dup");
    let scene = "---\nkind: scene\ncharacter: dupchar\nseason: 1\nepisode: 1\n---\n\
                 ## Shot 1.\n@narrator: hi\n";
    write(&dir, "a.lute", scene);
    write(&dir, "b.lute", scene);

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let d = v["project_diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "E-CONN-EPISODE-ID-DUP")
        .unwrap_or_else(|| panic!("E-CONN-EPISODE-ID-DUP present: {v}"));
    assert_eq!(d["span"]["line"], 3, "must point at `character:` (line 3), not 0:0: {v}");
    assert_eq!(d["span"]["column"], 1, "{v}");
}

#[test]
fn unknown_node_span_points_at_after_line() {
    let dir = temp_dir("span-unknown-node");
    write(
        &dir,
        "y.lute",
        "---\nkind: scene\ncharacter: y\nseason: 1\nepisode: 1\n\
         after: 'visited(\"nonexistent.s01ep01\")'\n---\n## Shot 1.\n@narrator: hi\n",
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let d = v["project_diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "E-CONN-UNKNOWN-NODE")
        .unwrap_or_else(|| panic!("E-CONN-UNKNOWN-NODE present: {v}"));
    assert_eq!(d["span"]["line"], 6, "must point at `after:` (line 6), not 0:0: {v}");
    assert_eq!(d["span"]["column"], 1, "{v}");
}

#[test]
fn cycle_span_points_at_a_cyclic_scenes_own_frontmatter() {
    let dir = temp_dir("span-cycle");
    write(
        &dir,
        "p.lute",
        "---\nkind: scene\ncharacter: p\nseason: 1\nepisode: 1\n\
         after: 'visited(\"q.s01ep01\")'\n---\n## Shot 1.\n@narrator: hi\n",
    );
    write(
        &dir,
        "q.lute",
        "---\nkind: scene\ncharacter: q\nseason: 1\nepisode: 1\n\
         after: 'visited(\"p.s01ep01\")'\n---\n## Shot 1.\n@narrator: hi\n",
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let d = v["project_diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "E-CONN-CYCLE")
        .unwrap_or_else(|| panic!("E-CONN-CYCLE present: {v}"));
    assert_eq!(
        d["span"]["line"], 3,
        "must point at the cyclic scene's own `character:` line (3), not 0:0: {v}"
    );
    assert_eq!(d["span"]["column"], 1, "{v}");
}

#[test]
fn unreachable_span_points_at_gated_scenes_own_frontmatter() {
    let dir = temp_dir("span-unreachable");
    write(
        &dir,
        "questa.lute",
        "---\nkind: quest\n---\n<quest id=\"questA\" start=\"false\">\n\
         <objective id=\"o\" done=\"true\"/>\n</quest>\n",
    );
    write(
        &dir,
        "gated.lute",
        "---\nkind: scene\ncharacter: repro\nseason: 1\nepisode: 1\n\
         after: 'completed(\"questA\")'\n---\n## Shot 1.\n@narrator: hi\n",
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let d = v["project_diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "E-CONN-UNREACHABLE")
        .unwrap_or_else(|| panic!("E-CONN-UNREACHABLE present: {v}"));
    assert_eq!(
        d["span"]["line"], 3,
        "must point at the gated scene's own `character:` line (3), not 0:0: {v}"
    );
    assert_eq!(d["span"]["column"], 1, "{v}");
}

// --- Defect B, round 2 (cross-model review): the fixpoint closure MUST
// iterate past round 1, and MUST NOT false-positive a still-live producer
// -----------------------------------------------------------------------

/// A 3+ node feedback chain (dsl 0.4.0 §4.2's relational-objective-
/// liveness CLOSURE): `chainQ1` has a dead REQUIRED objective (round 1) ->
/// scene `chainS` (`after: completed("chainQ1")`) becomes unreachable,
/// dropping its `::assert{chainRel(a)}` -- the SOLE producer of
/// `chainRel` (round 2) -> `chainQ2`'s REQUIRED objective
/// `done="holds(chainRel(a))"` is now dead too (round 2) -> scene `chainT`
/// (`after: completed("chainQ2")`) becomes unreachable (round 3). A
/// single-round (or two-pass) computation only catches `chainS`/`chainQ1`
/// and misses `chainQ2`/`chainT` entirely -- the fixpoint must catch all
/// four.
#[test]
fn fixpoint_closure_propagates_through_a_multi_hop_chain() {
    let dir = temp_dir("fixpoint-chain");
    write(
        &dir,
        "q1.lute",
        "---\nkind: quest\n---\n<quest id=\"chainQ1\" start=\"true\">\n\
         <objective id=\"deadReq\" done=\"false\"/>\n</quest>\n",
    );
    write(
        &dir,
        "s.lute",
        "---\nkind: scene\ncharacter: chainS\nseason: 1\nepisode: 1\n\
         after: 'completed(\"chainQ1\")'\nentities:\n  loc: { members: [a] }\n\
         relations:\n  chainRel: { args: [loc] }\n---\n## Shot 1.\n\
         ::assert{ chainRel(a) }\n",
    );
    write(
        &dir,
        "q2.lute",
        "---\nkind: quest\nentities:\n  loc: { members: [a] }\nrelations:\n  \
         chainRel: { args: [loc] }\n---\n<quest id=\"chainQ2\" start=\"true\">\n\
         <objective id=\"checkChain\" done=\"holds(chainRel(a))\"/>\n</quest>\n",
    );
    write(
        &dir,
        "t.lute",
        "---\nkind: scene\ncharacter: chainT\nseason: 1\nepisode: 1\n\
         after: 'completed(\"chainQ2\")'\n---\n## Shot 1.\n@narrator: hi\n",
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(1), "{}", String::from_utf8_lossy(&out.stdout));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let project_diags = v["project_diagnostics"].as_array().unwrap();

    let unreachable_paths: Vec<&str> = project_diags
        .iter()
        .filter(|d| d["code"] == "E-CONN-UNREACHABLE")
        .filter_map(|d| d["path"].as_str())
        .collect();
    assert!(
        unreachable_paths.iter().any(|p| p.ends_with("s.lute")),
        "round 1: chainS must be E-CONN-UNREACHABLE (gated on the dead chainQ1): {v}"
    );
    assert!(
        unreachable_paths.iter().any(|p| p.ends_with("t.lute")),
        "round 3: chainT must ALSO be E-CONN-UNREACHABLE -- a single-round computation would \
         miss this, since chainQ2's own deadness is only provable once chainS's assert site \
         drops out in round 2: {v}"
    );
    assert!(
        project_diags.iter().any(|d| {
            d["code"] == "E-OBJECTIVE-UNSATISFIABLE"
                && d["path"].as_str().unwrap().ends_with("q2.lute")
        }),
        "round 2: chainQ2's holds(chainRel(a)) must be flagged dead once chainRel's sole \
         producer (chainS) drops out: {v}"
    );
}

/// False-positive guard (Fix 1): a quest with `start="true"`, a dead
/// REQUIRED objective, AND a separate OPTIONAL objective whose body
/// asserts `liveRel(a)` -- `liveRel` must stay producible (the quest
/// itself still ACTIVATES and runs its other objectives; "can never
/// COMPLETE" is not "never activates"), so a THIRD, unrelated required
/// objective gated on `holds(liveRel(a))` must NOT be flagged
/// `E-OBJECTIVE-UNSATISFIABLE`.
#[test]
fn dead_required_objective_never_drops_a_sibling_optional_objectives_live_assert() {
    let dir = temp_dir("dead-required-objective-false-positive-guard");
    write(
        &dir,
        "mixedquest2.lute",
        "---\nkind: quest\nentities:\n  loc: { members: [a] }\nrelations:\n  \
         liveRel: { args: [loc] }\nstate:\n  run.opt: { type: bool, default: true }\n\
         ---\n<quest id=\"mixedQuest2\" start=\"true\">\n\
         <objective id=\"deadReq\" done=\"false\"/>\n\
         <objective id=\"liveOpt\" done=\"run.opt\" optional>\n\
         ::assert{ liveRel(a) }\n</objective>\n\
         <objective id=\"checkLive\" done=\"holds(liveRel(a))\"/>\n</quest>\n",
    );

    let out = run(&["check-project", dir.to_str().unwrap(), "--json"]);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let project_diags = v["project_diagnostics"].as_array().unwrap();
    assert!(
        !project_diags.iter().any(|d| d["code"] == "E-CONN-UNREACHABLE"),
        "{v}"
    );
    // Load-bearing: `checkLive`'s `holds(liveRel(a))` cause is a RELATIONAL
    // one, which `scan_objective_liveness` emits into `project_diags`
    // (never `files[].diagnostics` -- a per-file `check()` alone can't
    // decide it, R5-Undecided). If Fix 1 regressed (host-liveness gated on
    // the COMBINED unreachable set instead of lifecycle-only), `liveRel`
    // would wrongly go non-producible and THIS is where the false
    // `E-OBJECTIVE-UNSATISFIABLE` would land -- the per-file assertion
    // below alone can never observe it.
    assert!(
        !project_diags.iter().any(|d| d["code"] == "E-OBJECTIVE-UNSATISFIABLE"),
        "no project-wide (relational) E-OBJECTIVE-UNSATISFIABLE may fire -- `checkLive` must \
         stay live: {v}"
    );
    let files = v["files"].as_array().unwrap();
    let f = files
        .iter()
        .find(|f| f["path"].as_str().unwrap().ends_with("mixedquest2.lute"))
        .unwrap();
    let objective_diags: Vec<&str> = f["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|d| d["code"] == "E-OBJECTIVE-UNSATISFIABLE")
        .filter_map(|d| d["message"].as_str())
        .collect();
    assert_eq!(
        objective_diags.len(),
        1,
        "exactly `deadReq` must be flagged dead -- `checkLive` (holds(liveRel(a))) must stay \
         live, since `liveOpt`'s `::assert{{liveRel(a)}}` is still a REAL producer (the quest \
         still activates and runs it): {v}"
    );
    assert!(
        objective_diags[0].contains("false"),
        "the ONE flagged objective must be `deadReq` (done=\"false\"), never `checkLive`: {v}"
    );
}
