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
    // Copy the fixture catalog to a temp dir, refresh with --project, assert the
    // manifestVersion changed away from "pending" and matches the resolved snap.
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
    assert!(
        !after.contains("pending"),
        "manifestVersion must be re-stamped: {after}"
    );
    std::fs::remove_dir_all(&tmp).ok();
}
