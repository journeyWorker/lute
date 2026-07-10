// crates/lute-cli/tests/examples_check.rs
// Mirrors the harness in crates/lute-cli/tests/cli.rs (assert_cmd style).
use std::process::Command;

fn check(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lute"))
        .arg("check")
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap()
}

#[test]
fn extends_demo_scene_checks_clean_under_project() {
    // renamed extends-scene.lute -> extends-demo.lute, uses child.schema.lute
    let out = check(&[
        "../../docs/examples/extends-demo.lute",
        "--project",
        "../../docs/examples",
    ]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
}

#[test]
fn standalone_schema_fragment_has_no_kind_missing() {
    let out = check(&["../../docs/examples/base.schema.lute"]);
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(!s.contains("E-KIND-MISSING") && !s.contains("E-META-MISSING"), "{s}");
}
