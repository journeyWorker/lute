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

fn git(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args([
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "user.name=test",
        ])
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A repository whose Lute project lives in `story/`, beside a symlink
/// outside it (the Brinewake layout of #126), committed once.
fn story_repo(name: &str) -> lute_scratch::ScratchDir {
    let repo = lute_scratch::ScratchDir::named(name);
    std::fs::create_dir_all(repo.join("story")).unwrap();
    std::fs::create_dir_all(repo.join(".cursor")).unwrap();
    std::fs::write(repo.join("README.md"), "# game\n").unwrap();
    std::fs::write(repo.join("story/scene.lute"), scene("Hello")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink("../README.md", repo.join(".cursor/link")).unwrap();
    git(&repo, &["init", "-q"]);
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "base"]);
    repo
}

/// dsl 0.47.0 §5.3 (#126): from the repository root, `git:HEAD story`
/// archives only `story/` and roots the git side there, so a symlink outside
/// it does not matter and both sides compare the same project root.
#[cfg(unix)]
#[test]
fn git_head_vs_a_subproject_compares_the_subtree() {
    let repo = story_repo("git-subtree");
    std::fs::write(repo.join("story/scene.lute"), scene("Goodbye")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lute"))
        .args(["diff", "git:HEAD", "story", "--json"])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let before: Vec<&String> = report["before"]["files"]
        .as_object()
        .unwrap()
        .keys()
        .collect();
    assert_eq!(before, ["scene.lute"], "{report:#}");
    assert_eq!(report["changes"].as_array().unwrap().len(), 1, "{report:#}");
    assert_eq!(report["changes"][0]["kind"], "lineText");
}

/// A symlink inside the compared subtree is still refused.
#[cfg(unix)]
#[test]
fn a_symlink_inside_the_compared_subtree_is_still_refused() {
    let repo = story_repo("git-subtree-link");
    std::os::unix::fs::symlink("scene.lute", repo.join("story/alias.lute")).unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-qm", "link"]);
    let output = Command::new(env!("CARGO_BIN_EXE_lute"))
        .args(["diff", "git:HEAD", "story", "--json"])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(text.contains("symlink"), "{text}");
}

/// The compared directory's name is a literal path, not pathspec magic or a
/// glob: `:odd` and `[x]` subprojects compare like any other.
#[cfg(unix)]
#[test]
fn a_subproject_name_with_pathspec_characters_is_literal() {
    for name in [":odd", "[x]"] {
        let repo = lute_scratch::ScratchDir::named("git-subtree-literal");
        std::fs::create_dir_all(repo.join(name)).unwrap();
        std::fs::create_dir_all(repo.join("x")).unwrap();
        std::fs::write(repo.join(name).join("scene.lute"), scene("Hello")).unwrap();
        std::fs::write(repo.join("x/other.lute"), scene("Elsewhere")).unwrap();
        git(&repo, &["init", "-q"]);
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-qm", "base"]);
        std::fs::write(repo.join(name).join("scene.lute"), scene("Goodbye")).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_lute"))
            .args(["diff", "git:HEAD", name, "--json"])
            .current_dir(&repo)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let before: Vec<&String> = report["before"]["files"]
            .as_object()
            .unwrap()
            .keys()
            .collect();
        assert_eq!(before, ["scene.lute"], "{name}: {report:#}");
        assert_eq!(report["changes"].as_array().unwrap().len(), 1, "{name}");
    }
}

/// A subproject that does not exist at the revision is an `E-CLI-IO` that
/// names the directory and the revision, not git's raw pathspec error.
#[cfg(unix)]
#[test]
fn a_subproject_missing_at_the_revision_is_named() {
    let repo = story_repo("git-subtree-missing");
    std::fs::create_dir_all(repo.join("chapter2")).unwrap();
    std::fs::write(repo.join("chapter2/scene.lute"), scene("New")).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_lute"))
        .args(["diff", "git:HEAD", "chapter2", "--json"])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let report: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["error"]["code"], "E-CLI-IO");
    let message = report["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("`chapter2`") && message.contains("`HEAD`"),
        "{message}"
    );
    assert!(!message.contains("pathspec"), "{message}");
}
