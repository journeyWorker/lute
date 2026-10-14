use std::path::Path;
use std::process::Command;

fn scene(text: &str) -> String {
    format!(
        "---\nkind: scene\nid: hall\ncharacter: hero\nseason: 1\nepisode: 1\n---\n\n## Opening\n\n@hero: {text}\n"
    )
}

fn run(before: &Path, after: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lute"))
        .args([
            "diff",
            before.to_str().unwrap(),
            after.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap()
}

#[test]
fn diff_json_prints_revisions_and_semantic_changes() {
    let before = lute_scratch::ScratchDir::named("json-before");
    let after = lute_scratch::ScratchDir::named("json-after");
    std::fs::write(before.join("scene.lute"), scene("Hello")).unwrap();
    std::fs::write(after.join("scene.lute"), scene("Goodbye")).unwrap();
    let output = run(&before, &after);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schemaVersion"], "0.36.0.diff");
    assert!(report["before"]["sha256"].as_str().is_some());
    assert!(report["after"]["sha256"].as_str().is_some());
    assert_eq!(report["changes"].as_array().unwrap().len(), 1);
    assert_eq!(report["changes"][0]["kind"], "lineText");
}

#[test]
fn diff_json_is_empty_for_formatter_trivia() {
    let before = lute_scratch::ScratchDir::named("trivia-before");
    let after = lute_scratch::ScratchDir::named("trivia-after");
    std::fs::write(before.join("scene.lute"), scene("Hello")).unwrap();
    std::fs::write(after.join("scene.lute"), scene("Hello  \n")).unwrap();
    let output = run(&before, &after);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report["changes"].as_array().unwrap().is_empty());
}

#[test]
fn git_head_vs_working_copy_smoke() {
    let repo = lute_scratch::ScratchDir::named("git-head");
    std::fs::write(repo.join("scene.lute"), scene("Hello")).unwrap();
    let init = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert!(init.status.success());
    assert!(Command::new("git")
        .args(["add", "."])
        .current_dir(&repo)
        .output()
        .unwrap()
        .status
        .success());
    assert!(Command::new("git")
        .args([
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "user.name=test",
            "commit",
            "-qm",
            "base"
        ])
        .current_dir(&repo)
        .output()
        .unwrap()
        .status
        .success());
    let working = lute_scratch::ScratchDir::named("git-working");
    std::fs::write(working.join("scene.lute"), scene("Goodbye")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lute"))
        .args(["diff", "git:HEAD", working.to_str().unwrap(), "--json"])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["changes"].as_array().unwrap().len(), 1);
    assert_eq!(report["changes"][0]["kind"], "lineText");
}
