use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use lute_model::{ModelOptions, ProjectModel};

/// A fresh project per test. Tests run in parallel, so the directory name
/// carries the process id and a per-process counter, not only a timestamp.
fn project() -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let serial = NEXT.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("lute-cli-patch-{}-{stamp}-{serial}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("scene.lute"), "---\nkind: scene\nid: hall\ncharacter: hero\nseason: 1\nepisode: 1\n---\n\n## Opening\n\n@hero: Hello\n").unwrap();
    root
}

#[test]
fn dry_run_json_reports_diff_and_preserves_source() {
    let root = project();
    let model = ProjectModel::build_single_root(&root, &ModelOptions::default()).unwrap();
    let line = model.graph().nodes.keys().find(|key| key.kind == lute_model::NodeKind::Line).unwrap().canonical();
    let request = serde_json::json!({
        "base": {"project": format!("sha256:{}", model.revisions().sha256)},
        "targets": [line.clone()],
        "edits": [{"op":"replaceNode", "node":line, "text":"@hero: Goodbye"}],
        "preserve": []
    });
    let patch = root.join("patch.json");
    std::fs::write(&patch, serde_json::to_vec(&request).unwrap()).unwrap();
    let original = std::fs::read(root.join("scene.lute")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lute")).args(["patch", root.to_str().unwrap(), patch.to_str().unwrap(), "--dry-run", "--json"]).output().unwrap();
    assert_eq!(output.status.code(), Some(1), "{}", String::from_utf8_lossy(&output.stderr));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schemaVersion"], "0.36.0.patch");
    assert_eq!(report["writes"][0], "scene.lute");
    assert_eq!(std::fs::read(root.join("scene.lute")).unwrap(), original);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn unknown_json_field_is_refused() {
    let root = project();
    let patch = root.join("patch.json");
    std::fs::write(&patch, br#"{"base":{"project":"sha256:x"},"edits":[],"unknown":true}"#).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lute")).args(["patch", root.to_str().unwrap(), patch.to_str().unwrap(), "--json"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["code"], "E-PATCH-EDIT");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn model_io_failure_is_untyped_json_error() {
    let root = std::env::temp_dir().join(format!("lute-cli-patch-missing-{}", std::process::id()));
    let patch = std::env::temp_dir().join(format!("lute-cli-patch-request-{}.json", std::process::id()));
    std::fs::write(&patch, br#"{"base":{"project":"sha256:x"},"edits":[]}"#).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lute")).args(["patch", root.to_str().unwrap(), patch.to_str().unwrap(), "--json"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["error"]["kind"], "io");
    assert!(report.get("code").is_none());
    let _ = std::fs::remove_file(patch);
}
#[test]
fn accepted_json_has_shape_and_exit_review() {
    let root = project();
    let model = ProjectModel::build_single_root(&root, &ModelOptions::default()).unwrap();
    let line = model.graph().nodes.keys().find(|key| key.kind == lute_model::NodeKind::Line).unwrap().canonical();
    let patch = root.join("patch.json");
    std::fs::write(&patch, serde_json::to_vec(&serde_json::json!({
        "base": {"project": format!("sha256:{}", model.revisions().sha256)},
        "targets": [line.clone()],
        "edits": [{"op":"replaceNode", "node":line, "text":"@hero: Applied"}],
        "preserve": []
    })).unwrap()).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lute")).args(["patch", root.to_str().unwrap(), patch.to_str().unwrap(), "--json"]).output().unwrap();
    assert_eq!(output.status.code(), Some(1), "{}", String::from_utf8_lossy(&output.stderr));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schemaVersion"], "0.36.0.patch");
    assert_eq!(report["ok"], true);
    assert!(report["before"]["sha256"].is_string());
    assert!(report["after"]["sha256"].is_string());
    assert!(report["diff"]["changes"].is_array());
    assert_eq!(std::fs::read_to_string(root.join("scene.lute")).unwrap().contains("Applied"), true);
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn stale_json_refusal_has_code_and_exit_two() {
    let root = project();
    let patch = root.join("patch.json");
    std::fs::write(&patch, br#"{"base":{"project":"sha256:stale"},"edits":[],"preserve":[]}"#).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lute")).args(["patch", root.to_str().unwrap(), patch.to_str().unwrap(), "--json"]).output().unwrap();
    assert_eq!(output.status.code(), Some(2));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schemaVersion"], "0.36.0.patch");
    assert_eq!(report["ok"], false);
    assert_eq!(report["code"], "E-PATCH-STALE");
    assert!(report["message"].is_string());
    let _ = std::fs::remove_dir_all(root);
}
