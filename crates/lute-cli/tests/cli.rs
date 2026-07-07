//! End-to-end CLI tests: spawn the built `lute` binary and assert exit codes +
//! output. These pin the Task 5.1 acceptance contract — `check` exit `0`/`1`
//! from `CheckResult::ok`, `--json` serializes the result, and `catalog refresh`
//! → `check --providers` round-trips the on-disk provider snapshot format.

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

/// A fresh unique temp dir (no `tempfile` dev-dep needed for these small tests).
fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-cli-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn check_clean_file_exits_zero_json() {
    let out = Command::new(BIN)
        .args(["check", "../../docs/examples/bianca-s01ep02.lute", "--json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
}

#[test]
fn check_json_has_resolved_view_and_diagnostics_array() {
    let out = Command::new(BIN)
        .args(["check", "../../docs/examples/bianca-s01ep02.lute", "--json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        v["diagnostics"].is_array(),
        "diagnostics must serialize as an array"
    );
    // A clean document carries a resolved view (Some-vs-None policy).
    assert!(
        v["resolved"].is_object(),
        "clean doc → resolved is Some: {v}"
    );
    assert!(v["resolved"]["commands_preview"].is_array());
    assert!(v["resolved"]["timeline_tables"].is_array());
    assert!(v["resolved"]["injections"].is_array());
}

#[test]
fn check_file_with_errors_exits_one() {
    let out = Command::new(BIN)
        .args(["check", "../../docs/examples/date-minigame.lute", "--json"])
        .output()
        .unwrap();
    assert!(
        !out.status.success(),
        "a file with error diagnostics must exit non-zero"
    );
    assert_eq!(out.status.code(), Some(1));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
}

#[test]
fn check_human_output_lists_diagnostics() {
    let out = Command::new(BIN)
        .args(["check", "../../docs/examples/date-minigame.lute"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("E-UNKNOWN-DIRECTIVE"),
        "human output names codes: {stdout}"
    );
    assert!(
        stdout.contains("failed:"),
        "human summary reports failure: {stdout}"
    );
}

#[test]
fn check_missing_file_exits_two() {
    let out = Command::new(BIN)
        .args(["check", "/no/such/file.lute"])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(2),
        "an I/O failure exits 2, distinct from a check failure"
    );
}

#[test]
fn check_with_empty_providers_dir_is_permissive() {
    // `--providers` on an empty dir yields an empty set → no provider-id errors;
    // the example uses no `providerRef` attrs, so it stays clean either way.
    let dir = temp_dir("empty-providers");
    let out = Command::new(BIN)
        .args([
            "check",
            "../../docs/examples/bianca-s01ep02.lute",
            "--providers",
        ])
        .arg(&dir)
        .arg("--json")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn catalog_refresh_then_load_round_trips() {
    let dir = temp_dir("refresh");
    // A stale snapshot with an old manifest stamp.
    std::fs::write(
        dir.join("core.yaml"),
        "manifestVersion: old-stamp\nproviderVersion: \"3\"\nstale: true\nentries:\n  character: [bianca]\n",
    )
    .unwrap();

    let out = Command::new(BIN)
        .arg("catalog")
        .arg("refresh")
        .arg(&dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    // The rewritten file must still parse as a snapshot, with stale cleared and
    // the manifest re-stamped to the current capabilityVersion.
    let refreshed = std::fs::read_to_string(dir.join("core.yaml")).unwrap();
    let snap: serde_yaml::Value = serde_yaml::from_str(&refreshed).unwrap();
    assert_eq!(snap["stale"], serde_yaml::Value::Bool(false));
    assert_ne!(
        snap["manifestVersion"],
        serde_yaml::Value::String("old-stamp".into())
    );

    // And `ProviderSet::load` reads the refreshed dir back (the load consumer).
    let set = lute_manifest::provider::ProviderSet::load(&dir);
    assert_eq!(set.snapshots().len(), 1);
    use lute_manifest::provider::IdStatus;
    assert_eq!(set.contains("character", "bianca"), IdStatus::Fresh);
    assert_eq!(set.contains("character", "ghost"), IdStatus::Absent);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn catalog_refresh_missing_dir_is_created() {
    let base = temp_dir("refresh-missing");
    let target = base.join("brand/new");
    let out = Command::new(BIN)
        .arg("catalog")
        .arg("refresh")
        .arg(&target)
        .output()
        .unwrap();
    assert!(out.status.success(), "refresh creates a missing dir");
    assert!(target.is_dir(), "the target dir now exists");
    std::fs::remove_dir_all(&base).unwrap();
}

// --- 0.1.0 golden coverage: the showcase `hub-demo.lute` exercises a `<hub>`,
// `<when is="…">` literal arms, and `{{…}}` interpolation (dsl §7.3.2, §7.3.1,
// §7.6). A `<hub>` PASSES `lute check` but its CFG lowering lands in a later
// cutover (Plan C), so this example is exercised by the CHECK path ONLY. These
// two tests pin both halves of that contract: clean check, and the explicit
// E-HUB-LOWERING-UNSUPPORTED on compile (which is why it is never added to a
// compile/e2e golden).

#[test]
fn hub_demo_example_checks_clean() {
    let out = Command::new(BIN)
        .args([
            "check",
            "../../docs/examples/showcase/hub-demo.lute",
            "--project",
            "../../docs/examples/showcase",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "hub-demo must check clean; stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true, "hub-demo → ok:true; got {v}");
    assert_eq!(
        v["diagnostics"].as_array().map(Vec::len),
        Some(0),
        "hub-demo must be diagnostic-free (0 errors, 0 warnings); got {v}"
    );
    // Prove the 0.1.0 features are actually present in the resolved view — the
    // `<hub>` and both `<when is>`-bearing matches — not a trivially clean doc.
    let preview = v["resolved"]["commands_preview"].to_string();
    assert!(preview.contains("<hub>"), "resolved view must contain the hub; got {preview}");
    assert!(
        preview.contains("scene.choices.chatWithBianca"),
        "resolved view must contain the `<when is>` match over the hub's recorded choices; got {preview}"
    );
}

#[test]
fn hub_demo_example_compile_is_unsupported() {
    // The reason hub-demo is check-only: hub CFG lowering is Plan C. Compiling it
    // MUST fail with E-HUB-LOWERING-UNSUPPORTED (exit 1) — pinned here so no one
    // accidentally wires the example into a compile golden expecting success.
    let out = Command::new(BIN)
        .args([
            "compile",
            "../../docs/examples/showcase/hub-demo.lute",
            "--project",
            "../../docs/examples/showcase",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "hub compile is not yet supported → exit 1");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        combined.contains("E-HUB-LOWERING-UNSUPPORTED"),
        "compile must report E-HUB-LOWERING-UNSUPPORTED; got {combined}"
    );
}
