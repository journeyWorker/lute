use std::process::Command;

use lute_model::{ModelOptions, ProjectModel};
use lute_semantic::NodeKind;

/// A fresh project per test. Tests run in parallel, so the directory name
/// carries the process id and a per-process counter, not only a timestamp.
fn project() -> lute_scratch::ScratchDir {
    let root = lute_scratch::ScratchDir::new();
    std::fs::write(root.join("scene.lute"), "---\nkind: scene\nid: hall\ncharacter: hero\nseason: 1\nepisode: 1\n---\n\n## Opening\n\n@hero: Hello\n").unwrap();
    root
}

#[test]
fn dry_run_json_reports_diff_and_preserves_source() {
    let root = project();
    let model = ProjectModel::build_single_root(&root, &ModelOptions::default()).unwrap();
    let line = model
        .graph()
        .nodes
        .keys()
        .find(|key| key.kind == NodeKind::Line)
        .unwrap()
        .canonical();
    let request = serde_json::json!({
        "base": {"project": format!("sha256:{}", model.revisions().sha256)},
        "targets": [line.clone()],
        "edits": [{"op":"replaceNode", "node":line, "text":"@hero: Goodbye"}],
        "preserve": []
    });
    let patch = root.join("patch.json");
    std::fs::write(&patch, serde_json::to_vec(&request).unwrap()).unwrap();
    let original = std::fs::read(root.join("scene.lute")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lute"))
        .args([
            "patch",
            root.to_str().unwrap(),
            patch.to_str().unwrap(),
            "--dry-run",
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schemaVersion"], "0.43.0.patch");
    assert_eq!(report["writes"][0], "scene.lute");
    assert_eq!(std::fs::read(root.join("scene.lute")).unwrap(), original);
}

#[test]
fn unknown_json_field_is_refused() {
    let root = project();
    let patch = root.join("patch.json");
    std::fs::write(
        &patch,
        br#"{"base":{"project":"sha256:x"},"edits":[],"unknown":true}"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lute"))
        .args([
            "patch",
            root.to_str().unwrap(),
            patch.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["code"], "E-PATCH-EDIT");
}

#[test]
fn model_io_failure_is_untyped_json_error() {
    let root_scratch = lute_scratch::ScratchDir::new();
    let root = root_scratch.join("lute-cli-patch-missing");
    let patch_scratch = lute_scratch::ScratchDir::new();
    let patch = patch_scratch.join("lute-cli-patch-request.json");
    std::fs::write(&patch, br#"{"base":{"project":"sha256:x"},"edits":[]}"#).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lute"))
        .args([
            "patch",
            root.to_str().unwrap(),
            patch.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
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
    let line = model
        .graph()
        .nodes
        .keys()
        .find(|key| key.kind == NodeKind::Line)
        .unwrap()
        .canonical();
    let patch = root.join("patch.json");
    std::fs::write(
        &patch,
        serde_json::to_vec(&serde_json::json!({
            "base": {"project": format!("sha256:{}", model.revisions().sha256)},
            "targets": [line.clone()],
            "edits": [{"op":"replaceNode", "node":line, "text":"@hero: Applied"}],
            "preserve": []
        }))
        .unwrap(),
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lute"))
        .args([
            "patch",
            root.to_str().unwrap(),
            patch.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(
        output.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schemaVersion"], "0.43.0.patch");
    assert_eq!(report["command"], "patch");
    // Accepted with semantic changes: exit 1, so the envelope's `ok` is false
    // while `accepted` says the patch applied (dsl 0.43.0 §3.3).
    assert_eq!(report["ok"], false);
    assert_eq!(report["accepted"], true);
    assert!(report["before"]["sha256"].is_string());
    assert!(report["after"]["sha256"].is_string());
    assert!(report["diff"]["changes"].is_array());
    assert!(std::fs::read_to_string(root.join("scene.lute"))
        .unwrap()
        .contains("Applied"));
}

#[test]
fn stale_json_refusal_has_code_and_exit_two() {
    let root = project();
    let patch = root.join("patch.json");
    std::fs::write(
        &patch,
        br#"{"base":{"project":"sha256:stale"},"edits":[],"preserve":[]}"#,
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lute"))
        .args([
            "patch",
            root.to_str().unwrap(),
            patch.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schemaVersion"], "0.43.0.patch");
    assert_eq!(report["ok"], false);
    assert_eq!(report["accepted"], false);
    assert_eq!(report["code"], "E-PATCH-STALE");
    assert!(report["message"].is_string());
}

/// #123 through the binary: two `replaceNode` edits in one file, the second
/// below indented lines the first edit's growth shifts. `accepted`, the file
/// passes `lute fmt --check`, and only the two nodes' lines changed.
#[test]
fn two_edits_in_one_file_leave_it_fmt_check_clean() {
    let root = lute_scratch::ScratchDir::new();
    let original = "---\nkind: scene\nid: fair\n---\n\n## Opening\n\n@narrator{code=\"0010\"}: First line of the fair.\n@narrator{code=\"0020\"}: Someone runs past.\n\n<branch id=\"pick\">\n  <choice id=\"go\" text=\"Go\">\n    ::bg{location=\"square\"}\n    @pell{code=\"0010\"}: Paper and a frame.\n  </choice>\n</branch>\n";
    std::fs::write(root.join("scene.lute"), original).unwrap();
    let lute = |args: &[&str]| {
        Command::new(env!("CARGO_BIN_EXE_lute"))
            .args(args)
            .output()
            .unwrap()
    };
    let scene = root.join("scene.lute");
    assert_eq!(
        lute(&["fmt", "--check", scene.to_str().unwrap()])
            .status
            .code(),
        Some(0)
    );
    let model = ProjectModel::build_single_root(&root, &ModelOptions::default()).unwrap();
    let request = serde_json::json!({
        "base": {"project": format!("sha256:{}", model.revisions().sha256)},
        "targets": ["line:fair.narrator_0020", "line:fair.pell_0010"],
        "edits": [
            {"op": "replaceNode", "node": "line:fair.narrator_0020",
             "text": "@narrator{code=\"0020\"}: Someone runs past, shouting loudly."},
            {"op": "replaceNode", "node": "line:fair.pell_0010",
             "text": "@pell{code=\"0010\"}: Paper, a frame, and a pencil."}
        ],
        "preserve": [{"lineIds": []}, {"hostContracts": true}]
    });
    let patch = root.join("patch.json");
    std::fs::write(&patch, serde_json::to_vec(&request).unwrap()).unwrap();
    let output = lute(&[
        "patch",
        root.to_str().unwrap(),
        patch.to_str().unwrap(),
        "--json",
    ]);
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["accepted"], true, "{report:#}");
    let fmt = lute(&["fmt", "--check", scene.to_str().unwrap()]);
    assert_eq!(
        fmt.status.code(),
        Some(0),
        "{}",
        std::fs::read_to_string(&scene).unwrap()
    );
    assert_eq!(
        std::fs::read_to_string(&scene).unwrap(),
        original
            .replace("Someone runs past.", "Someone runs past, shouting loudly.")
            .replace("Paper and a frame.", "Paper, a frame, and a pencil.")
    );
}
