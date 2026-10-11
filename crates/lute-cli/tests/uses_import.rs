//! End-to-end CLI test for scene `uses:` schema imports (dsl §9.2): spawn the
//! built `lute` binary against a temp-dir schema + scene and assert that the
//! imported `run.*` path resolves (exit 0) and that a missing import surfaces
//! `E-USES-NOT-FOUND` (exit 1). Mirrors the harness in `cli.rs`.

use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

const SCHEMA: &str = "---\nstate:\n  run.choseHelp: { type: bool, default: false }\n---\n";
// Scene reads the imported run path; `run.choseHelp` is defaulted so it's clean.
// Arm bodies sit on their OWN lines: an inline `<when …>@x: a` silently DROPPED
// that content line and is `E-TAG-INLINE-BODY` (dsl §2.3).
const SCENE: &str =
    "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nuses: schema.lute\n---\n\
## Shot 1.\n<match subject=\"run.choseHelp\">\n<when test=\"$ == true\">\n@x: a\n</when>\n\
<otherwise>\n@x: b\n</otherwise>\n</match>\n";

#[test]
fn cli_resolves_uses_import_exits_zero() {
    let dir = lute_scratch::ScratchDir::named("uses-ok");
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
    let dir = lute_scratch::ScratchDir::named("uses-missing");
    let scene = dir.join("scene.lute");
    std::fs::write(
        &scene,
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nuses: nope.lute\n---\n## Shot 1.\n@x: hi\n",
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
