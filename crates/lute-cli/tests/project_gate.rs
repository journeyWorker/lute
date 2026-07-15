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

/// A scene declaring ONLY `after: visited(<after_key>)` with no `run.*` read —
/// a cycle member with NO own read fault, so ONLY cycle membership can block
/// it (the non-anchor-cycle-member regression fixture).
fn scene_after_only(character: &str, after_key: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: {character}\nseason: 1\nepisode: 1\n\
         after: 'visited(\"{after_key}\")'\n---\n## Shot 1.\n@narrator: hi\n"
    )
}

/// A pass-through scene: declares `after` and `run.z` in schema but neither
/// reads nor writes it — carries an upstream-Guaranteed `run.z` through
/// untouched (the MIDDLE hop of the multi-hop guaranteed-chain test).
fn scene_passthrough_after(character: &str, after_key: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: {character}\nseason: 1\nepisode: 1\n\
         after: 'visited(\"{after_key}\")'\nstate:\n  run.z: {{ type: number }}\n---\n\
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
    assert!(
        String::from_utf8_lossy(&standalone.stdout).contains("E-MAYBE-UNSET"),
        "the standalone refusal must be the entry-dependent read fault (E-MAYBE-UNSET), \
         not an unrelated error: {}",
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
    assert!(
        String::from_utf8_lossy(&cp.stdout).contains("E-CONN-UNKNOWN-NODE"),
        "the sibling's project-only fault must be E-CONN-UNKNOWN-NODE: {}",
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
    // Pin that NONE of the sibling's fault codes leaked onto the target's own
    // gated verdict (spec §5: only the target's OWN diagnostics gate).
    let gated_out = String::from_utf8_lossy(&gated.stdout);
    assert!(
        !gated_out.contains("E-CONN-UNKNOWN-NODE")
            && !gated_out.contains("E-STATE-MAYBE-UNAVAILABLE")
            && !gated_out.contains("E-MAYBE-UNSET"),
        "no sibling/project fault code may appear on the target's gated output: {gated_out}"
    );
    let v: serde_json::Value = serde_json::from_slice(&gated.stdout).unwrap();
    assert_eq!(v["kind"], "scene", "{v}");
}

// --- (f) a NON-ANCHOR cycle MEMBER blocks (spec §5 "cycle membership") -------

/// A 2-node `after` cycle `p <-> q`: `assemble_graph`'s DFS visits `p` first
/// (lexically), so the SINGLE emitted `E-CONN-CYCLE` anchors to `p.lute`. The
/// target `q` is the OTHER member — a genuine cycle member whose file carries
/// NO anchored diagnostic and NO own read fault. Before the gate fix it
/// compiled/traced clean (the target-anchored merge saw nothing on `q.lute`);
/// the fix synthesizes a TARGET-anchored `E-CONN-CYCLE` from the graph's
/// `cycle_members` side channel. (Verified RED pre-fix: with the
/// `project_gate_result` cycle-member block removed, `q` compiled at exit 0.)
#[test]
fn compile_gate_non_anchor_cycle_member_blocks() {
    let dir = temp_dir("gate-cycle-nonanchor-compile");
    write(&dir, "p.lute", &scene_after_only("p", "q.s01ep01"));
    let q = write(&dir, "q.lute", &scene_after_only("q", "p.s01ep01"));

    // Sanity: the ONE cycle diagnostic anchors to p.lute, NOT q.lute — so q is
    // provably the non-anchored member.
    let cp = run(&["check-project", dir.to_str().unwrap()]);
    let cp_out = String::from_utf8_lossy(&cp.stdout);
    let cycle_line = cp_out.lines().find(|l| l.contains("E-CONN-CYCLE")).unwrap_or("");
    assert!(cycle_line.contains("p.lute"), "cycle diag must anchor to p.lute: {cp_out}");
    assert!(
        !cycle_line.contains("q.lute"),
        "cycle diag must NOT anchor to q.lute (q is the non-anchored member): {cp_out}"
    );

    let gated = run(&["compile", q.to_str().unwrap(), "--project", dir.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&gated.stdout);
    assert_eq!(
        gated.status.code(),
        Some(1),
        "a non-anchored cycle member must block compilation: {stdout}"
    );
    assert!(stdout.contains("E-CONN-CYCLE"), "the block must cite the cycle: {stdout}");
    assert!(!stdout.starts_with('{'), "no artifact on a blocked gate: {stdout}");
}

#[test]
fn trace_gate_non_anchor_cycle_member_blocks() {
    let dir = temp_dir("gate-cycle-nonanchor-trace");
    write(&dir, "p.lute", &scene_after_only("p", "q.s01ep01"));
    let q = write(&dir, "q.lute", &scene_after_only("q", "p.s01ep01"));

    let gated = run(&["trace", q.to_str().unwrap(), "--project", dir.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&gated.stdout);
    assert_eq!(
        gated.status.code(),
        Some(1),
        "a non-anchored cycle member must refuse the trace: {stdout}"
    );
    assert!(stdout.contains("E-CONN-CYCLE"), "the refusal must cite the cycle: {stdout}");
}

// --- (g) a Guaranteed write reconciles ACROSS multiple `after` hops ----------

/// `up` (entry) ALWAYS sets `run.z`; `mid` (`after: up`) passes it through
/// untouched; `term` (`after: mid`) reads `run.z` — Guaranteed TWO hops
/// upstream. Pins that the §4.3 envelope propagates transitively, not just one
/// hop: `term` blocks standalone (E-MAYBE-UNSET) but compiles under --project.
#[test]
fn compile_gate_multi_hop_guaranteed_read_compiles_under_project() {
    let dir = temp_dir("gate-multi-hop");
    write(&dir, "up.lute", &scene_setting_run_z("up"));
    write(&dir, "mid.lute", &scene_passthrough_after("mid", "up.s01ep01"));
    let term = write(
        &dir,
        "term.lute",
        &scene_reading_run_z("term", "after: 'visited(\"mid.s01ep01\")'\n"),
    );

    // Standalone: the terminal read blocks (no project view) — E-MAYBE-UNSET.
    let standalone = run(&["compile", term.to_str().unwrap()]);
    assert_eq!(
        standalone.status.code(),
        Some(1),
        "standalone compile must block: {}",
        String::from_utf8_lossy(&standalone.stdout)
    );
    assert!(
        String::from_utf8_lossy(&standalone.stdout).contains("E-MAYBE-UNSET"),
        "standalone diagnostics: {}",
        String::from_utf8_lossy(&standalone.stdout)
    );

    // Project-aware: `run.z ∈ Guaranteed(term)` via up -> mid -> term (2 hops).
    let gated = run(&["compile", term.to_str().unwrap(), "--project", dir.to_str().unwrap()]);
    assert_eq!(
        gated.status.code(),
        Some(0),
        "a guaranteed write must reconcile transitively across hops: stdout={} stderr={}",
        String::from_utf8_lossy(&gated.stdout),
        String::from_utf8_lossy(&gated.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&gated.stdout).unwrap();
    assert_eq!(v["kind"], "scene", "an artifact must be emitted: {v}");
}

// --- (h) topological-order exclusion: overlapping-cycle + downstream ---------

/// A scene declaring `after: visited(k1) && visited(k2)` and reading no state
/// — a two-prerequisite cycle node whose only possible block is its graph
/// position (used to wire the overlapping-cycle counterexample).
fn scene_after_two(character: &str, k1: &str, k2: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: {character}\nseason: 1\nepisode: 1\n\
         after: 'visited(\"{k1}\") && visited(\"{k2}\")'\n---\n## Shot 1.\n@narrator: hi\n"
    )
}

/// The cross-model re-review counterexample. Edges (prereq -> dependent):
/// `p->q, p->r, q->p, r->q` — encoded as `p after q`, `q after p && r`,
/// `r after p`. This has TWO overlapping directed cycles: the 2-cycle `p<->q`
/// and the 3-cycle `p->r->q->p`. `assemble_graph`'s DFS starts at `p`, finds
/// the back edge `q->p` (stack slice `[p, q]`), then reaches `r` only AFTER
/// `q` is already finished, so its `r->q` edge is NOT a back edge and `r` is
/// NEVER added to the old DFS `cycle_members` set — yet `r` genuinely sits on
/// the 3-cycle. Kahn's topological sort frees NONE of `p`/`q`/`r` (every
/// in-degree stays >= 1), so `r` is absent from `topo_order`; the fix blocks
/// on that absence (`node_cycle_degraded`), which is COMPLETE. (Verified RED:
/// against the pre-fix `cycle_members` gate, `lute compile r --project` exits
/// 0 because `r` is missing from `cycle_members`.)
#[test]
fn compile_gate_overlapping_cycle_member_blocks() {
    let dir = temp_dir("gate-overlap-cycle-compile");
    write(&dir, "p.lute", &scene_after_only("p", "q.s01ep01"));
    write(&dir, "q.lute", &scene_after_two("q", "p.s01ep01", "r.s01ep01"));
    let r = write(&dir, "r.lute", &scene_after_only("r", "p.s01ep01"));

    let gated = run(&["compile", r.to_str().unwrap(), "--project", dir.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&gated.stdout);
    assert_eq!(
        gated.status.code(),
        Some(1),
        "r is on the overlapping 3-cycle -> must block compilation: {stdout}"
    );
    assert!(stdout.contains("E-CONN-CYCLE"), "the block must cite the cycle: {stdout}");
    assert!(!stdout.starts_with('{'), "no artifact on a blocked gate: {stdout}");
}

#[test]
fn trace_gate_overlapping_cycle_member_blocks() {
    let dir = temp_dir("gate-overlap-cycle-trace");
    write(&dir, "p.lute", &scene_after_only("p", "q.s01ep01"));
    write(&dir, "q.lute", &scene_after_two("q", "p.s01ep01", "r.s01ep01"));
    let r = write(&dir, "r.lute", &scene_after_only("r", "p.s01ep01"));

    let gated = run(&["trace", r.to_str().unwrap(), "--project", dir.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&gated.stdout);
    assert_eq!(
        gated.status.code(),
        Some(1),
        "r is on the overlapping 3-cycle -> must refuse the trace: {stdout}"
    );
    assert!(stdout.contains("E-CONN-CYCLE"), "the refusal must cite the cycle: {stdout}");
}

/// A node DOWNSTREAM of a cycle (spec §5): `a<->b` is a 2-node `after` cycle;
/// `d` (`after: visited(a)`) reads NO state of its own and is on no cycle
/// itself — but Kahn's sort can never free `a`, so `d` never reaches in-degree
/// 0 and is absent from `topo_order`. Per spec §5 the gate refuses any target
/// absent from the sound topological order, so `d` must block even though it
/// is not a cycle MEMBER (the old membership-only gate let it through).
#[test]
fn compile_gate_downstream_of_cycle_blocks() {
    let dir = temp_dir("gate-downstream-cycle");
    write(&dir, "a.lute", &scene_after_only("a", "b.s01ep01"));
    write(&dir, "b.lute", &scene_after_only("b", "a.s01ep01"));
    let d = write(&dir, "d.lute", &scene_after_only("d", "a.s01ep01"));

    let gated = run(&["compile", d.to_str().unwrap(), "--project", dir.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&gated.stdout);
    assert_eq!(
        gated.status.code(),
        Some(1),
        "a node downstream of a cycle is absent from topo_order -> must block: {stdout}"
    );
    assert!(stdout.contains("E-CONN-CYCLE"), "the block must cite the cycle: {stdout}");
    assert!(!stdout.starts_with('{'), "no artifact on a blocked gate: {stdout}");
}
