// crates/lute-cli/tests/examples_check.rs
// Mirrors the harness in crates/lute-cli/tests/cli.rs (assert_cmd style).
use std::path::{Path, PathBuf};
use std::process::Command;

fn check(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lute"))
        .arg("check")
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap()
}

fn examples_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples")
}

#[test]
fn extends_demo_scene_checks_clean_under_project() {
    // renamed extends-scene.lute -> extends-demo.lute, uses child.schema.yaml
    // (0.3.0 B4: declaration files migrated .schema.lute -> .schema.yaml, the
    // Lute `---` envelope stripped, per foundation B2/B4).
    let out = check(&[
        "../../docs/examples/extends-demo.lute",
        "--project",
        "../../docs/examples",
    ]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
}

#[test]
fn extends_demo_chain_uses_declaration_yaml_not_schema_lute() {
    // B4: `child.schema.yaml`/`base.schema.yaml` are the declaration files
    // under docs/examples/ now — the old `.schema.lute` envelope form is gone.
    assert!(
        examples_dir().join("child.schema.yaml").exists(),
        "child.schema.yaml must exist after the B4 .schema.lute -> .schema.yaml migration"
    );
    assert!(
        examples_dir().join("base.schema.yaml").exists(),
        "base.schema.yaml must exist after the B4 .schema.lute -> .schema.yaml migration"
    );
    assert!(
        !examples_dir().join("child.schema.lute").exists()
            && !examples_dir().join("base.schema.lute").exists()
            && !examples_dir().join("state.schema.lute").exists(),
        "the old .schema.lute envelope files must be gone after migration"
    );
}

#[test]
fn showcase_episode_checks_clean_with_yaml_schema_chain() {
    // B4: showcase/schema/{base,game}.schema.lute -> .schema.yaml; episode01's
    // `uses:` (and game's `extends:`) target the new names.
    let showcase = examples_dir().join("showcase");
    assert!(
        showcase.join("schema/game.schema.yaml").exists()
            && showcase.join("schema/base.schema.yaml").exists(),
        "showcase/schema/{{base,game}}.schema.yaml must exist after migration"
    );
    let out = check(&[
        "../../docs/examples/showcase/episode01.lute",
        "--project",
        "../../docs/examples/showcase",
    ]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
}

#[test]
fn rescue_halsin_quest_checks_clean_under_project() {
    // 0.3.0 T15: spec Appendix B worked example (act1.schema.yaml +
    // quest-rescue-halsin.lute) — derived recursion (canReach), epistemic
    // derivation (believesLocation), seeds, key-relations, quest gating on
    // `holds`, exercised end-to-end under the core-only project.
    let out = check(&[
        "../../docs/examples/quest-rescue-halsin.lute",
        "--project",
        "../../docs/examples",
    ]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
}
