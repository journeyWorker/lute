//! Plugin-loaded acceptance (plugin §4/§11): `check --project <dir>` loads the
//! project's installed plugins and resolves the scene's activated snapshot, so a
//! document that is `E-UNKNOWN-DIRECTIVE` under core-only checks clean once its
//! plugins are active. The regression guard pins the untouched core-only path:
//! WITHOUT `--project`, `date-minigame.lute` still exits `1`.

use std::process::Command;

fn lute_bin() -> &'static str {
    env!("CARGO_BIN_EXE_lute")
}

#[test]
fn date_minigame_is_clean_with_plugin_project() {
    let out = Command::new(lute_bin())
        .args([
            "check",
            "../../docs/examples/date-minigame.lute",
            "--project",
            "../../docs/examples/idola-project",
            "--providers",
            "../../docs/examples/idola-project/catalog",
            "--json",
        ])
        .output()
        .expect("run lute");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\"ok\": true"),
        "expected ok:true, got: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0), "exit 0 on clean");
}

#[test]
fn date_minigame_core_only_still_errors() {
    // REGRESSION GUARD: without --project, the existing core-only contract holds.
    let out = Command::new(lute_bin())
        .args(["check", "../../docs/examples/date-minigame.lute", "--json"])
        .output()
        .expect("run lute");
    assert_eq!(out.status.code(), Some(1), "core-only still exits 1");
}

#[test]
fn refresh_stamps_resolved_version_under_project() {
    // Copy the fixture catalog to a temp dir, refresh with --project, and assert
    // the stamped manifestVersion equals the RESOLVED multi-plugin
    // capabilityVersion (and thus differs from core-only) — proving the
    // --project path actually resolves the plugin, not just replaces "pending".
    let proj = lute_manifest::project::load_project(std::path::Path::new(
        "../../docs/examples/idola-project",
    ))
    .unwrap()
    .unwrap();
    let resolved_version = lute_manifest::project::resolve_document_snapshot(
        Some(&proj),
        None,
        &std::collections::BTreeMap::new(),
    )
    .0
    .version;
    let core_version = lute_manifest::core::load_core_snapshot().version;
    assert_ne!(
        resolved_version, core_version,
        "fixture must make the resolved version differ from core-only, else the test can't distinguish"
    );

    let tmp = std::env::temp_dir().join(format!("lute_cat_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::copy(
        "../../docs/examples/idola-project/catalog/minigame.yaml",
        tmp.join("minigame.yaml"),
    )
    .unwrap();
    let out = std::process::Command::new(lute_bin())
        .args([
            "catalog",
            "refresh",
            tmp.to_str().unwrap(),
            "--project",
            "../../docs/examples/idola-project",
        ])
        .output()
        .expect("refresh");
    assert_eq!(out.status.code(), Some(0));
    let after = std::fs::read_to_string(tmp.join("minigame.yaml")).unwrap();
    let snap: serde_yaml::Value = serde_yaml::from_str(&after).unwrap();
    let stamped = snap
        .get("manifestVersion")
        .and_then(|v| v.as_str())
        .expect("manifestVersion present");
    assert_eq!(
        stamped, resolved_version,
        "refresh --project must stamp the RESOLVED capabilityVersion, not core-only"
    );
    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn date_minigame_clean_with_project_catalog_autodiscovered() {
    let out = std::process::Command::new(lute_bin())
        .args([
            "check",
            "../../docs/examples/date-minigame.lute",
            "--project",
            "../../docs/examples/idola-project",
            "--json",
        ])
        .output()
        .expect("run lute");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\"ok\": true"),
        "catalog auto-discovered from project -> ok:true; got {stdout}\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), Some(0));
}
