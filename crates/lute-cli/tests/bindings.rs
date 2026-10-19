use std::path::Path;
use std::process::Command;

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_lute")
}
fn fixture(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

#[test]
fn clean_fixture_has_no_binding_diagnostics() {
    let out = Command::new(bin())
        .args(["check", "--project", &fixture("bindings-clean"), "--json"])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        value["project_diagnostics"].as_array().unwrap().len(),
        0,
        "{value}"
    );
}

#[test]
fn problem_fixture_json_covers_key_file_missing_and_unused() {
    let out = Command::new(bin())
        .args([
            "check",
            "--project",
            &fixture("bindings-problems"),
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let diagnostics = value["project_diagnostics"].as_array().unwrap();
    for code in [
        "E-BINDING-KEY",
        "E-BINDING-FILE",
        "W-BINDING-MISSING",
        "W-BINDING-UNUSED",
    ] {
        assert!(
            diagnostics.iter().any(|d| d["code"] == code),
            "missing {code}: {value}"
        );
    }
    let key = diagnostics
        .iter()
        .find(|d| d["code"] == "E-BINDING-KEY")
        .unwrap();
    assert!(key["path"]
        .as_str()
        .unwrap()
        .ends_with("crates/lute-cli/tests/fixtures/bindings-problems/player.bindings.yaml"));
    assert!(key["span"]["line"].as_u64().unwrap() > 0);
}

#[test]
fn custom_vfx_bindings_are_clean() {
    let out = Command::new(bin())
        .args([
            "check",
            "--project",
            &fixture("bindings-custom-vfx"),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        value["project_diagnostics"].as_array().unwrap().len(),
        0,
        "{value}"
    );
}

#[test]
fn custom_effect_outside_vfx_is_a_binding_effect_error() {
    let out = Command::new(bin())
        .args([
            "check",
            "--project",
            &fixture("bindings-custom-misplaced"),
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let diagnostics = value["project_diagnostics"].as_array().unwrap();
    let effect = diagnostics
        .iter()
        .find(|d| d["code"] == "E-BINDING-EFFECT")
        .unwrap_or_else(|| panic!("{value}"));
    assert!(
        effect["message"]
            .as_str()
            .unwrap()
            .contains("only under `vfx`"),
        "{value}"
    );
}

fn write_temp(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

fn check_ui_bindings(tag: &str, bindings: &str) -> serde_json::Value {
    let dir = lute_scratch::ScratchDir::named(tag);
    write_temp(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\ndefaults:\n  uses: [world.schema.yaml]\n",
    );
    write_temp(
        &dir,
        "world.schema.yaml",
        "enums:\n  ending: [restore, retire]\nstate:\n  run.ending: {type: {domain: ending}, default: restore}\n",
    );
    write_temp(
        &dir,
        "scenes/start.lute",
        "---\nkind: scene\nid: start\non: start\n---\n## Start\n@narrator: Hello.\n",
    );
    write_temp(&dir, "assets/title.png", "x");
    write_temp(&dir, "assets/dawn.png", "x");
    write_temp(&dir, "player.bindings.yaml", bindings);
    let out = Command::new(bin())
        .args(["check", "--project", &dir.to_string_lossy(), "--json"])
        .output()
        .unwrap();
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn ui_asset_keys_are_used_and_direct_music_paths_are_checked() {
    let value = check_ui_bindings(
        "bindings-ui-assets",
        "bindingsVersion: '0.48'\ncanvas: {width: 1, height: 1}\nassets:\n  title: title.png\nui:\n  title: {background: title, music: missing.ogg}\n",
    );
    let diagnostics = value["project_diagnostics"].as_array().unwrap();
    assert!(diagnostics.iter().any(|d| d["code"] == "E-BINDING-FILE"));
    assert!(!diagnostics.iter().any(|d| {
        d["code"] == "W-BINDING-UNUSED" && d["message"].as_str().unwrap().contains("title")
    }));
}

#[test]
fn ended_screens_validate_closed_state_domains_and_assets() {
    let value = check_ui_bindings(
        "bindings-ended",
        "bindingsVersion: '0.48'\ncanvas: {width: 1, height: 1}\nassets:\n  dawn: dawn.png\nui:\n  ended:\n    by: run.ending\n    screens:\n      restore: {title: Again, text: Welcome, background: dawn}\n      retire: {title: Quiet, text: Goodbye}\n",
    );
    assert_eq!(
        value["project_diagnostics"].as_array().unwrap().len(),
        0,
        "{value}"
    );
}

#[test]
fn list_entity_kind_shape_suggests_members_mapping_with_authored_members() {
    let dir = lute_scratch::ScratchDir::named("entity-kind-list-shape-046");
    write_temp(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\ndefaults:\n  uses: [world.schema.yaml]\n",
    );
    write_temp(
        &dir,
        "world.schema.yaml",
        "entities:\n  spot: [bellRope, bellFooting]\n",
    );
    write_temp(
        &dir,
        "scenes/start.lute",
        "---\nkind: scene\nid: start\non: start\n---\n## Start\n@narrator: Hello.\n",
    );
    let out = Command::new(bin())
        .args(["check", "--project", &dir.to_string_lossy(), "--json"])
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let diagnostic = value["project_diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "E-ENTITY-KIND-SHAPE")
        .unwrap_or_else(|| panic!("{value}"));
    assert!(
        diagnostic["message"]
            .as_str()
            .unwrap()
            .contains("spot: { members: [bellRope, bellFooting] }"),
        "{value}"
    );
}
