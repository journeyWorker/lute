//! `lute compile`/`lute trace` project-aware connectivity gate (connectivity
//! design spec §5): WITH `--project <dir>` both commands gate on the target
//! document's RECONCILED `check-project` verdict — an envelope-Guaranteed
//! `run.*`/`user.*` read no longer blocks with a standalone `E-MAYBE-UNSET`,
//! and a read no route guarantees blocks with `E-STATE-MAYBE-UNAVAILABLE`.
//! WITHOUT `--project`, the standalone single-file `check` gate is unchanged.
//! Single-root: `--project <dir>` = `<dir>` for BOTH capability and
//! connectivity, no nested search; an out-of-tree target errors explicitly.
//!
//! Fixture helpers mirror `check_project.rs` (each integration test binary is
//! compiled separately, so the small helpers are intentionally duplicated).

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

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

/// A scene doc declaring `after: visited(<after_key>)` that reads `run.z`
/// (declared, no default) via a plain `::set` RHS — the entry-dependent read
/// shape every gate test needs (mirrors `check_project.rs`).
fn scene_reading_run_z(character: &str, after_expr: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: {character}\nseason: 1\nepisode: 1\n{after_expr}\
         state:\n  run.z: {{ type: number }}\n  run.out: {{ type: number }}\n---\n\
         ## Shot 1.\n::set{{run.out = run.z}}\n"
    )
}

/// A scene that UNCONDITIONALLY sets `run.z` — the Guaranteed predecessor.
fn scene_setting_run_z(character: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: {character}\nseason: 1\nepisode: 1\n\
         state:\n  run.z: {{ type: number }}\n---\n## Shot 1.\n::set{{run.z = 1}}\n"
    )
}

/// A scene that NEVER sets `run.z` — the never-Possible predecessor.
fn scene_silent(character: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: {character}\nseason: 1\nepisode: 1\n---\n\
         ## Shot 1.\n@narrator: hi\n"
    )
}

// --- (a) compile: an envelope-Guaranteed read compiles ONLY under --project -

#[test]
fn compile_gate_guaranteed_read_fails_standalone_succeeds_under_project() {
    let dir = temp_dir("gate-compile-guaranteed");
    write(&dir, "y.lute", &scene_setting_run_z("y"));
    let x = write(&dir, "x.lute", &scene_reading_run_z("x", "after: 'visited(\"y.s01ep01\")'\n"));

    // Standalone: single-file check can't see the project -> E-MAYBE-UNSET
    // blocks compilation (exit 1, no artifact).
    let standalone = run(&["compile", x.to_str().unwrap()]);
    assert_eq!(
        standalone.status.code(),
        Some(1),
        "standalone compile must block on E-MAYBE-UNSET: {}",
        String::from_utf8_lossy(&standalone.stdout)
    );
    assert!(
        String::from_utf8_lossy(&standalone.stdout).contains("E-MAYBE-UNSET"),
        "standalone diagnostics: {}",
        String::from_utf8_lossy(&standalone.stdout)
    );

    // Project-aware: `run.z ∈ Guaranteed(x)` (y is the only route and always
    // sets it) -> reconciled away -> the artifact emits (exit 0).
    let gated = run(&["compile", x.to_str().unwrap(), "--project", dir.to_str().unwrap()]);
    assert_eq!(
        gated.status.code(),
        Some(0),
        "project-gated compile must succeed: stdout={} stderr={}",
        String::from_utf8_lossy(&gated.stdout),
        String::from_utf8_lossy(&gated.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&gated.stdout).unwrap();
    assert_eq!(v["kind"], "scene", "an artifact must be emitted: {v}");
}

// --- (b) trace: same reconciliation for the trace refusal gate --------------

#[test]
fn trace_gate_guaranteed_read_refused_standalone_runs_under_project() {
    let dir = temp_dir("gate-trace-guaranteed");
    write(&dir, "y.lute", &scene_setting_run_z("y"));
    let x = write(&dir, "x.lute", &scene_reading_run_z("x", "after: 'visited(\"y.s01ep01\")'\n"));

    // Standalone: refused (exit 1) — the check gate has E-MAYBE-UNSET.
    let standalone = run(&["trace", x.to_str().unwrap()]);
    assert_eq!(
        standalone.status.code(),
        Some(1),
        "standalone trace must refuse on the check error: {}",
        String::from_utf8_lossy(&standalone.stdout)
    );

    // Project-aware: reconciled clean -> trace walks and completes (exit 0).
    let gated = run(&["trace", x.to_str().unwrap(), "--project", dir.to_str().unwrap()]);
    assert_eq!(
        gated.status.code(),
        Some(0),
        "project-gated trace must complete: stdout={} stderr={}",
        String::from_utf8_lossy(&gated.stdout),
        String::from_utf8_lossy(&gated.stderr)
    );
}

// --- (c) a read no route guarantees BLOCKS under --project ------------------

#[test]
fn compile_gate_never_possible_read_blocks_under_project() {
    let dir = temp_dir("gate-compile-never");
    // `y` is the ONLY route and NEVER sets `run.z` -> `run.z ∉ Possible(x)`.
    write(&dir, "y.lute", &scene_silent("y"));
    let x = write(&dir, "x.lute", &scene_reading_run_z("x", "after: 'visited(\"y.s01ep01\")'\n"));

    let gated = run(&["compile", x.to_str().unwrap(), "--project", dir.to_str().unwrap()]);
    assert_eq!(
        gated.status.code(),
        Some(1),
        "an unavailable read must block the project-gated compile: {}",
        String::from_utf8_lossy(&gated.stdout)
    );
    let stdout = String::from_utf8_lossy(&gated.stdout);
    assert!(
        stdout.contains("E-STATE-MAYBE-UNAVAILABLE"),
        "the target's own envelope fault must be reported: {stdout}"
    );
    assert!(
        !stdout.starts_with('{'),
        "no artifact on a blocked gate: {stdout}"
    );
}

#[test]
fn trace_gate_never_possible_read_blocks_under_project() {
    let dir = temp_dir("gate-trace-never");
    write(&dir, "y.lute", &scene_silent("y"));
    let x = write(&dir, "x.lute", &scene_reading_run_z("x", "after: 'visited(\"y.s01ep01\")'\n"));

    let gated = run(&["trace", x.to_str().unwrap(), "--project", dir.to_str().unwrap()]);
    assert_eq!(
        gated.status.code(),
        Some(1),
        "an unavailable read must refuse the project-gated trace: {}",
        String::from_utf8_lossy(&gated.stdout)
    );
    assert!(
        String::from_utf8_lossy(&gated.stdout).contains("E-STATE-MAYBE-UNAVAILABLE"),
        "diagnostics: {}",
        String::from_utf8_lossy(&gated.stdout)
    );
}

// --- (d) out-of-tree target under --project -> explicit error ---------------

#[test]
fn compile_gate_out_of_tree_target_errors_explicitly() {
    let dir = temp_dir("gate-out-of-tree-proj");
    write(&dir, "y.lute", &scene_setting_run_z("y"));
    // The target lives OUTSIDE `dir` entirely.
    let other = temp_dir("gate-out-of-tree-target");
    let outside = write(&other, "outside.lute", &scene_silent("outside"));

    let out = run(&["compile", outside.to_str().unwrap(), "--project", dir.to_str().unwrap()]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "an out-of-tree target must error, never silently fall back: stdout={} stderr={}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("not within"),
        "stderr must explain the out-of-tree rejection: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Sanity: the SAME file compiles clean standalone (no --project) — proving
    // the exit 2 is the gate's out-of-tree rule, not a document fault.
    let standalone = run(&["compile", outside.to_str().unwrap()]);
    assert_eq!(standalone.status.code(), Some(0), "{}", String::from_utf8_lossy(&standalone.stdout));
}

#[test]
fn trace_gate_out_of_tree_target_errors_explicitly() {
    let dir = temp_dir("gate-trace-out-of-tree-proj");
    write(&dir, "y.lute", &scene_setting_run_z("y"));
    let other = temp_dir("gate-trace-out-of-tree-target");
    let outside = write(&other, "outside.lute", &scene_silent("outside"));

    let out = run(&["trace", outside.to_str().unwrap(), "--project", dir.to_str().unwrap()]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "an out-of-tree target must error the trace gate: stderr={}",
        String::from_utf8_lossy(&out.stderr)
    );
}

// --- (e) a SIBLING's project-only fault does NOT block the target -----------

#[test]
fn compile_gate_sibling_project_fault_does_not_block_target() {
    let dir = temp_dir("gate-sibling-fault");
    // A clean, standalone-compilable target with no `after`/reads of its own.
    let x = write(&dir, "x.lute", &scene_silent("x"));
    // A SIBLING whose `after` names a node that does not exist anywhere in the
    // project -> `E-CONN-UNKNOWN-NODE`, a project-only fault anchored on IT.
    write(
        &dir,
        "bad.lute",
        &scene_reading_run_z("bad", "after: 'visited(\"ghost.s99ep99\")'\n"),
    );

    // `check-project` on the whole dir FAILS (the sibling's fault).
    let cp = run(&["check-project", dir.to_str().unwrap()]);
    assert_eq!(
        cp.status.code(),
        Some(1),
        "the sibling fault must fail check-project: {}",
        String::from_utf8_lossy(&cp.stdout)
    );

    // …but compiling the TARGET under the same --project SUCCEEDS: the gate
    // blocks on the target's OWN reconciled diagnostics only (spec §5).
    let gated = run(&["compile", x.to_str().unwrap(), "--project", dir.to_str().unwrap()]);
    assert_eq!(
        gated.status.code(),
        Some(0),
        "a sibling's project-only fault must not block the target: stdout={} stderr={}",
        String::from_utf8_lossy(&gated.stdout),
        String::from_utf8_lossy(&gated.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&gated.stdout).unwrap();
    assert_eq!(v["kind"], "scene", "{v}");
}
