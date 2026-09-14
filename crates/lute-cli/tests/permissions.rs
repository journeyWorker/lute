use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};

const BIN: &str = env!("CARGO_BIN_EXE_lute");
static NEXT: AtomicU32 = AtomicU32::new(0);

fn temp_project(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "lute-permissions-{tag}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("lute.project.yaml"),
        "defaultProfile: authored\nprofiles:\n  authored:\n    plugins: {}\n  restricted:\n    plugins: {}\n    permissions:\n      directives: [camera]\n      bridges: []\n      rewards: false\n      quests: false\n",
    )
    .unwrap();
    dir
}

fn scene(profile: &str, episode: u32, body: &str) -> String {
    format!(
        "---\nkind: scene\nprofile: {profile}\ncharacter: hero\nseason: 1\nepisode: {episode}\n---\n\n## Opening\n\n{body}"
    )
}

fn write_scene(dir: &Path, name: &str, episode: u32, body: &str) -> PathBuf {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, scene("authored", episode, body)).unwrap();
    path
}

fn output(args: &[&str]) -> std::process::Output {
    Command::new(BIN).args(args).output().unwrap()
}

#[test]
fn trusted_profile_ceiling_defeats_source_profile_for_check_and_compile() {
    let dir = temp_project("host-ceiling");
    let file = write_scene(&dir, "scene.lute", 1, "::end\n");
    for command in ["check", "compile"] {
        let result = output(&[
            command,
            file.to_str().unwrap(),
            "--project",
            dir.to_str().unwrap(),
            "--permission-profile",
            "restricted",
        ]);
        assert_eq!(
            result.status.code(),
            Some(1),
            "{command} stderr: {} stdout: {}",
            String::from_utf8_lossy(&result.stderr),
            String::from_utf8_lossy(&result.stdout)
        );
        assert!(
            String::from_utf8_lossy(&result.stdout).contains("E-PERMISSION-DIRECTIVE"),
            "{command} must reject source-authored `profile: authored`: {}",
            String::from_utf8_lossy(&result.stdout)
        );
    }
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn compile_all_applies_ceiling_to_every_document_and_writes_nothing() {
    let dir = temp_project("all");
    write_scene(&dir, "a.lute", 1, "::end\n");
    write_scene(&dir, "nested/b.lute", 2, "::end\n");
    let out_dir = dir.join("out");
    let result = output(&[
        "compile",
        "--all",
        "--project",
        dir.to_str().unwrap(),
        "--permission-profile",
        "restricted",
        "-o",
        out_dir.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&result.stdout).contains("E-PERMISSION-DIRECTIVE"));
    assert!(
        !out_dir.exists() || std::fs::read_dir(&out_dir).unwrap().next().is_none(),
        "the all-or-nothing gate must not emit any denied artifact"
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn context_reports_the_same_effective_ceiling_and_filters_authoring_surface() {
    let dir = temp_project("context");
    let file = write_scene(&dir, "scene.lute", 1, "::end\n");
    let result = output(&[
        "context",
        file.to_str().unwrap(),
        "--json",
        "--project",
        dir.to_str().unwrap(),
        "--permission-profile",
        "restricted",
    ]);
    assert_eq!(
        result.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let context: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(context["permissions"]["layers"][0]["directives"][0], "camera");
    assert_eq!(context["questsAllowed"], false);
    assert!(context["bridges"].as_array().unwrap().is_empty());
    assert!(context["rewardKinds"].as_object().unwrap().is_empty());
    let directives: Vec<&str> = context["directives"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|value| value["name"].as_str())
        .collect();
    assert!(directives.contains(&"camera"));
    assert!(!directives.contains(&"end"));

    let checked = output(&[
        "check",
        file.to_str().unwrap(),
        "--project",
        dir.to_str().unwrap(),
        "--permission-profile",
        "restricted",
    ]);
    assert!(String::from_utf8_lossy(&checked.stdout).contains("E-PERMISSION-DIRECTIVE"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn invalid_or_projectless_permission_profile_is_an_explicit_error() {
    let dir = temp_project("invalid");
    let file = write_scene(&dir, "scene.lute", 1, "::end\n");
    let unknown = output(&[
        "check",
        file.to_str().unwrap(),
        "--project",
        dir.to_str().unwrap(),
        "--permission-profile",
        "missing",
    ]);
    assert_eq!(unknown.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("E-PROFILE-UNKNOWN"));

    let projectless = output(&[
        "check",
        file.to_str().unwrap(),
        "--permission-profile",
        "restricted",
    ]);
    assert_eq!(projectless.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&projectless.stderr).contains("E-PERMISSION-PROFILE"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn omitting_permission_profile_preserves_existing_behavior_and_surface() {
    let dir = temp_project("backward");
    let file = write_scene(&dir, "scene.lute", 1, "::end\n");
    let checked = output(&[
        "check",
        file.to_str().unwrap(),
        "--project",
        dir.to_str().unwrap(),
    ]);
    assert_eq!(
        checked.status.code(),
        Some(0),
        "stderr: {} stdout: {}",
        String::from_utf8_lossy(&checked.stderr),
        String::from_utf8_lossy(&checked.stdout)
    );
    let context = output(&[
        "context",
        file.to_str().unwrap(),
        "--json",
        "--project",
        dir.to_str().unwrap(),
    ]);
    assert_eq!(context.status.code(), Some(0));
    let context: serde_json::Value = serde_json::from_slice(&context.stdout).unwrap();
    assert_eq!(context["permissions"]["layers"], serde_json::json!([]));
    assert!(context["directives"]
        .as_array()
        .unwrap()
        .iter()
        .any(|value| value["name"] == "end"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn compile_stream_freezes_ceiling_for_appended_body() {
    let dir = temp_project("stream");
    let prefix = write_scene(&dir, "prefix.lute", 1, "");
    let mut child = Command::new(BIN)
        .args([
            "compile-stream",
            prefix.to_str().unwrap(),
            "--project",
            dir.to_str().unwrap(),
            "--permission-profile",
            "restricted",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child.stdin.take().unwrap().write_all(b"::end\n").unwrap();
    let result = child.wait_with_output().unwrap();
    assert_eq!(result.status.code(), Some(1));
    let events: Vec<serde_json::Value> = String::from_utf8(result.stdout)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(events.first().unwrap()["kind"], "start");
    assert_eq!(events.last().unwrap()["kind"], "error");
    assert!(events.last().unwrap()["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|diagnostic| diagnostic["code"] == "E-PERMISSION-DIRECTIVE"));
    assert!(!events.iter().any(|event| event["kind"] == "finish"));
    let _ = std::fs::remove_dir_all(dir);
}
