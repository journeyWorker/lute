mod support;

use support::{json, run_cli, TempProject};

const SCENE: &str = "---\nkind: scene\ncharacter: narrator\nseason: 1\nepisode: 1\n---\n\n## Start\n\n@narrator: hello\n";

#[test]
fn check_json_report_has_common_envelope_and_exit_agreement() {
    let project = TempProject::new("envelope-043-check");
    let file = project.write("scene.lute", SCENE);
    let output = run_cli(["check", file.to_str().unwrap(), "--json"]);
    assert_eq!(output.status.code(), Some(0));
    let value = json(&output);
    assert_eq!(value["command"], "check");
    assert_eq!(value["schemaVersion"], "0.48.0.check");
    assert_eq!(value["ok"], true);
}

#[test]
fn check_json_error_has_path_and_exit_two() {
    let output = run_cli(["check", "/definitely/missing/scene.lute", "--json"]);
    assert_eq!(output.status.code(), Some(2));
    let value = json(&output);
    assert_eq!(value["command"], "check");
    assert_eq!(value["schemaVersion"], "0.43.0.error");
    assert_eq!(value["ok"], false);
    assert_eq!(value["error"]["code"], "E-CLI-IO");
    assert_eq!(value["error"]["path"], "/definitely/missing/scene.lute");
}

#[test]
fn check_json_diagnostic_report_sets_ok_false_for_exit_one() {
    let project = TempProject::new("envelope-043-check-failure");
    let file = project.write(
        "broken.lute",
        "---\nkind: scene\ncharacter: narrator\nseason: 1\nepisode: 1\n---\n\n## Start\n\n@narrator: {{run.missing}}\n",
    );
    let output = run_cli(["check", file.to_str().unwrap(), "--json"]);
    assert_eq!(output.status.code(), Some(1));
    let value = json(&output);
    assert_eq!(value["command"], "check");
    assert_eq!(value["schemaVersion"], "0.48.0.check");
    assert_eq!(value["ok"], false);
    assert!(value["files"][0]["diagnostics"].is_array());
}

#[test]
fn constraints_json_uses_named_field() {
    let project = TempProject::new("envelope-043-constraints");
    let output = run_cli(["constraints", project.path().to_str().unwrap(), "--json"]);
    assert_eq!(output.status.code(), Some(0));
    let value = json(&output);
    assert_eq!(value["command"], "constraints");
    assert_eq!(value["schemaVersion"], "0.43.0.constraints");
    assert_eq!(value["ok"], true);
    assert!(value["constraints"].is_array());
}

#[test]
fn patch_malformed_json_keeps_patch_refusal_document() {
    let project = TempProject::new("envelope-043-patch");
    let patch = project.write("bad.json", "{");
    let output = run_cli([
        "patch",
        project.path().to_str().unwrap(),
        patch.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(output.status.code(), Some(2));
    let value = json(&output);
    assert_eq!(value["command"], "patch");
    assert_eq!(value["schemaVersion"], "0.43.0.patch");
    assert_eq!(value["ok"], false);
    assert_eq!(value["code"], "E-PATCH-EDIT");
}
