use std::path::{Path, PathBuf};
use std::process::Command;

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("lute-cli-diff-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn scene(text: &str) -> String {
    format!(
        "---\nkind: scene\nid: hall\ncharacter: hero\nseason: 1\nepisode: 1\n---\n\n## Opening\n\n@hero: {text}\n"
    )
}

fn run(before: &Path, after: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_lute"))
        .args(["diff", before.to_str().unwrap(), after.to_str().unwrap(), "--json"])
        .output()
        .unwrap()
}

#[test]
fn diff_json_prints_revisions_and_semantic_changes() {
    let before = temp_dir("json-before");
    let after = temp_dir("json-after");
    std::fs::write(before.join("scene.lute"), scene("Hello")).unwrap();
    std::fs::write(after.join("scene.lute"), scene("Goodbye")).unwrap();
    let output = run(&before, &after);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["schemaVersion"], "0.36.0.diff");
    assert!(report["before"]["sha256"].as_str().is_some());
    assert!(report["after"]["sha256"].as_str().is_some());
    assert_eq!(report["changes"].as_array().unwrap().len(), 1);
    assert_eq!(report["changes"][0]["kind"], "lineText");
    let _ = std::fs::remove_dir_all(before);
    let _ = std::fs::remove_dir_all(after);
}

#[test]
fn diff_json_is_empty_for_formatter_trivia() {
    let before = temp_dir("trivia-before");
    let after = temp_dir("trivia-after");
    std::fs::write(before.join("scene.lute"), scene("Hello")).unwrap();
    std::fs::write(after.join("scene.lute"), scene("Hello  \n")).unwrap();
    let output = run(&before, &after);
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report["changes"].as_array().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(before);
    let _ = std::fs::remove_dir_all(after);
}

#[test]
fn git_head_vs_working_copy_smoke() {
    let repo = temp_dir("git-head");
    std::fs::write(repo.join("scene.lute"), scene("Hello")).unwrap();
    let init = Command::new("git").args(["init", "-q"]).current_dir(&repo).output().unwrap();
    assert!(init.status.success());
    assert!(Command::new("git").args(["add", "."]).current_dir(&repo).output().unwrap().status.success());
    assert!(Command::new("git").args(["-c", "user.email=test@example.invalid", "-c", "user.name=test", "commit", "-qm", "base"]).current_dir(&repo).output().unwrap().status.success());
    let working = temp_dir("git-working");
    std::fs::write(working.join("scene.lute"), scene("Goodbye")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lute"))
        .args(["diff", "git:HEAD", working.to_str().unwrap(), "--json"])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["changes"].as_array().unwrap().len(), 1);
    assert_eq!(report["changes"][0]["kind"], "lineText");
    let _ = std::fs::remove_dir_all(repo);
    let _ = std::fs::remove_dir_all(working);
}
