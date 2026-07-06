//! End-to-end CLI test for scene `uses:` schema imports (dsl §9.2): spawn the
//! built `lute` binary against a temp-dir schema + scene and assert that the
//! imported `run.*` path resolves (exit 0) and that a missing import surfaces
//! `E-USES-NOT-FOUND` (exit 1). Mirrors the harness in `cli.rs`.

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

const SCHEMA: &str = "---\nstate:\n  run.choseHelp: { type: bool, default: false }\n---\n";
// Scene reads the imported run path; `run.choseHelp` is defaulted so it's clean.
const SCENE: &str = "---\ncharacter: x\nseason: 1\nepisode: 1\nuses: schema.lute\n---\n\
## Shot 1.\n<match on=\"run.choseHelp\">\n<when test=\"$ == true\">:x: a\n</when>\n\
<otherwise>:x: b\n</otherwise>\n</match>\n";

#[test]
fn cli_resolves_uses_import_exits_zero() {
    let dir = temp_dir("uses-ok");
    std::fs::write(dir.join("schema.lute"), SCHEMA).unwrap();
    let scene = dir.join("scene.lute");
    std::fs::write(&scene, SCENE).unwrap();
    let out = Command::new(BIN)
        .args(["check", scene.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "expected exit 0 (imported run path resolves); stderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn cli_missing_import_flags_not_found() {
    let dir = temp_dir("uses-missing");
    let scene = dir.join("scene.lute");
    std::fs::write(
        &scene,
        "---\ncharacter: x\nseason: 1\nepisode: 1\nuses: nope.lute\n---\n## Shot 1.\n:x: hi\n",
    )
    .unwrap();
    let out = Command::new(BIN)
        .args(["check", scene.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!out.status.success(), "missing import must fail");
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(combined.contains("E-USES-NOT-FOUND"), "got: {combined}");
}

#[test]
fn carry_ep_fixture_resolves_via_uses_import() {
    let out = Command::new(BIN)
        .args(["check", "../../docs/examples/carry-ep.lute"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "carry-ep.lute must be clean via its uses: import; stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
}
