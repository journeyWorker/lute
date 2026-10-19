mod support;

use std::path::Path;

use support::{json, run_cli, TempProject};

fn assert_error(output: &std::process::Output, command: &str, code: &str) -> serde_json::Value {
    assert_eq!(output.status.code(), Some(2));
    assert!(
        !output.stderr.is_empty(),
        "human error must remain on stderr"
    );
    let value = json(output);
    assert_eq!(value["schemaVersion"], "0.43.0.error");
    assert_eq!(value["ok"], false);
    assert_eq!(value["command"], command);
    assert_eq!(value["error"]["code"], code);
    value
}

fn assert_path(value: &serde_json::Value, path: &Path) {
    assert_eq!(value["error"]["path"], path.display().to_string());
}

const PROJECT: &str = "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n";

#[test]
fn trace_io_error_has_path() {
    let path = "/definitely/missing/trace.lute";
    let output = run_cli(["trace", path, "--json"]);
    let value = assert_error(&output, "trace", "E-CLI-IO");
    assert_path(&value, Path::new(path));
    assert!(String::from_utf8_lossy(&output.stderr).contains(path));
}

#[test]
fn trace_input_error_has_mock_path() {
    let project = TempProject::new("envelope-043-trace-input");
    let file = project.write("scene.lute", "---\nkind: scene\n---\n");
    let mock = project.write("bad.yaml", "facts: [");
    let output = run_cli([
        "trace",
        file.to_str().unwrap(),
        "--mock",
        mock.to_str().unwrap(),
        "--json",
    ]);
    let value = assert_error(&output, "trace", "E-CLI-INPUT");
    assert_path(&value, &mock);
    assert!(String::from_utf8_lossy(&output.stderr).contains("mock"));
}

#[test]
fn trace_usage_error_has_argument() {
    let output = run_cli(["trace", "missing.lute", "--target", "hero", "--json"]);
    let value = assert_error(&output, "trace", "E-CLI-USAGE");
    assert_eq!(value["error"]["argument"], "--target");
    assert!(String::from_utf8_lossy(&output.stderr).contains("--target"));
}

#[test]
fn run_io_error_has_artifact_path() {
    let path = "/definitely/missing/execution.json";
    let output = run_cli(["run", path, "--json"]);
    let value = assert_error(&output, "run", "E-CLI-IO");
    assert_path(&value, Path::new(path));
    assert!(String::from_utf8_lossy(&output.stderr).contains(path));
}

#[test]
fn run_input_error_has_artifact_path() {
    let project = TempProject::new("envelope-043-run-input");
    let artifact = project.write("bad.json", "{");
    let output = run_cli(["run", artifact.to_str().unwrap(), "--json"]);
    let value = assert_error(&output, "run", "E-CLI-INPUT");
    assert_path(&value, &artifact);
}

#[test]
fn run_usage_error_has_argument() {
    let output = run_cli([
        "run",
        "artifact.json",
        "--entry",
        "one",
        "--beat",
        "two",
        "--json",
    ]);
    let value = assert_error(&output, "run", "E-CLI-USAGE");
    // clap reports the first of the two conflicting options.
    assert_eq!(value["error"]["argument"], "--entry");
    assert!(String::from_utf8_lossy(&output.stderr).contains("cannot be used"));
}

#[test]
fn play_io_error_has_script_path() {
    let project = TempProject::new("envelope-043-play-io");
    project.write("lute.project.yaml", PROJECT);
    let script = project.path().join("missing.play.yaml");
    let output = run_cli([
        "play",
        "--project",
        project.path().to_str().unwrap(),
        "--script",
        script.to_str().unwrap(),
        "--json",
    ]);
    let value = assert_error(&output, "play", "E-CLI-IO");
    assert_path(&value, &script);
}

#[test]
fn play_input_error_has_script_path() {
    let project = TempProject::new("envelope-043-play-input");
    project.write("lute.project.yaml", PROJECT);
    let script = project.write("bad.play.yaml", "steps: [");
    let output = run_cli([
        "play",
        "--project",
        project.path().to_str().unwrap(),
        "--script",
        script.to_str().unwrap(),
        "--json",
    ]);
    let value = assert_error(&output, "play", "E-CLI-INPUT");
    assert_path(&value, &script);
}

#[test]
fn play_usage_error_has_argument() {
    let project = TempProject::new("envelope-043-play-usage");
    project.write("lute.project.yaml", PROJECT);
    let script = project.write("play.yaml", "steps: []\n");
    let output = run_cli([
        "play",
        "--project",
        project.path().to_str().unwrap(),
        "--script",
        script.to_str().unwrap(),
        "--where",
        "true",
        "--json",
    ]);
    let value = assert_error(&output, "play", "E-CLI-USAGE");
    assert_eq!(value["error"]["argument"], "--where");
}

#[test]
fn calendar_io_error_has_directory_path() {
    let path = "/definitely/missing/calendar-project";
    let output = run_cli(["calendar", "--project", path, "--json"]);
    let value = assert_error(&output, "calendar", "E-CLI-IO");
    assert_path(&value, Path::new(path));
}

#[test]
fn calendar_input_error_has_script_path() {
    let project = TempProject::new("envelope-043-calendar-input");
    project.write("lute.project.yaml", PROJECT);
    let script = project.write("bad.play.yaml", "steps: [");
    let output = run_cli([
        "calendar",
        "--project",
        project.path().to_str().unwrap(),
        "--script",
        script.to_str().unwrap(),
        "--json",
    ]);
    let value = assert_error(&output, "calendar", "E-CLI-INPUT");
    assert_path(&value, &script);
}

#[test]
fn calendar_usage_error_has_argument() {
    let project = TempProject::new("envelope-043-calendar-usage");
    project.write("lute.project.yaml", PROJECT);
    let output = run_cli([
        "calendar",
        "--project",
        project.path().to_str().unwrap(),
        "--where",
        "(",
        "--json",
    ]);
    let value = assert_error(&output, "calendar", "E-CLI-USAGE");
    assert_eq!(value["error"]["argument"], "--where");
}

#[test]
fn test_io_error_has_directory_path() {
    let path = "/definitely/missing/test-project";
    let output = run_cli(["test", "--project", path, "--json"]);
    let value = assert_error(&output, "test", "E-CLI-IO");
    assert_path(&value, Path::new(path));
}

#[test]
fn test_input_error_has_test_path() {
    let project = TempProject::new("envelope-043-test-input");
    let test = project.write("broken.test.yaml", "entries: [");
    let output = run_cli([
        "test",
        "--project",
        project.path().to_str().unwrap(),
        "--json",
    ]);
    let value = assert_error(&output, "test", "E-CLI-INPUT");
    assert_path(&value, &test);
}

#[test]
fn test_usage_error_has_argument() {
    let output = run_cli([
        "test",
        ".",
        "--project",
        "one",
        "--project",
        "two",
        "--json",
    ]);
    let value = assert_error(&output, "test", "E-CLI-USAGE");
    assert_eq!(value["error"]["argument"], "--project");
}
