//! End-to-end `lute lint` subcommand (spec §2 / §3).
//!
//! Spawns the built `lute` binary against temp projects and asserts the
//! surface contract: default runs fire built-in rules, `lute.lint.yaml`
//! overrides them, `--deny` promotes non-errors, `--json` emits the
//! `{ok, diagnostics, configDiagnostics}` shape, malformed YAML exits 2,
//! and a plugin's `lints/` export fires when its plugin is active in the
//! project's default profile.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-cli-lint-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn run(args: &[&str]) -> std::process::Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn write(p: &Path, body: &str) {
    if let Some(parent) = p.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(p, body).unwrap();
}

/// A scene whose `@bob` line runs past the 40-word default cap and whose
/// first shot opens without a `::bg` (so both `L-DIALOGUE-LENGTH` and
/// `L-SHOT-STARTS-WITH-BACKGROUND` fire by default).
const LONG_LINE: &str = "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty twenty-one twenty-two twenty-three twenty-four twenty-five twenty-six twenty-seven twenty-eight twenty-nine thirty thirty-one thirty-two thirty-three thirty-four thirty-five thirty-six thirty-seven thirty-eight thirty-nine forty forty-one";

fn scene_with_long_line() -> String {
    format!("---\nkind: scene\n---\n## Shot 1.\n@alice: hi\n@bob: {LONG_LINE}\n")
}

/// A control scene: opens with a `::bg` and has no over-long line, so no
/// default rule fires.
const CLEAN_SCENE: &str = "---\nkind: scene\n---\n## Shot 1.\n::bg{location=\"a\"}\n@alice: hi\n";

/// A tree with a single .lute doc + NO `lute.lint.yaml` runs with built-in
/// defaults and reports the expected `L-DIALOGUE-LENGTH` finding at exit 0
/// (it is warn-level).
#[test]
fn default_run_reports_dialogue_length() {
    let dir = temp_dir("default");
    write(&dir.join("scene.lute"), &scene_with_long_line());

    let out = run(&["lint", dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("L-DIALOGUE-LENGTH"), "{stdout}");
}

/// `rules: { dialogue-length: off }` in `lute.lint.yaml` silences the
/// finding.
#[test]
fn level_off_silences_rule() {
    let dir = temp_dir("off");
    write(&dir.join("scene.lute"), &scene_with_long_line());
    write(&dir.join("lute.lint.yaml"), "rules:\n  dialogue-length: off\n");

    let out = run(&["lint", dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(!stdout.contains("L-DIALOGUE-LENGTH"), "unexpected: {stdout}");
}

/// A rule promoted to `error` in `lute.lint.yaml` (native, no `--deny`)
/// flips the exit code to 1.
#[test]
fn level_error_flips_exit_code() {
    let dir = temp_dir("error-level");
    write(&dir.join("scene.lute"), &scene_with_long_line());
    write(
        &dir.join("lute.lint.yaml"),
        "rules:\n  dialogue-length: error\n",
    );

    let out = run(&["lint", dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("error [L-DIALOGUE-LENGTH]"), "{stdout}");
}

/// A `custom:` rule declared in `lute.lint.yaml` participates end-to-end:
/// the id becomes `L-<UPPER-KEBAB>` and the CEL predicate resolves.
#[test]
fn custom_rule_fires_from_config() {
    let dir = temp_dir("custom");
    write(&dir.join("scene.lute"), CLEAN_SCENE);
    write(
        &dir.join("lute.lint.yaml"),
        "custom:\n  \
         - id: too-few-shots\n    \
           target: scene\n    \
           when: \"scene.shots < options.min\"\n    \
           level: warn\n    \
           message: \"only {scene.shots} shots (need {options.min})\"\n    \
           options: { min: 3 }\n",
    );

    let out = run(&["lint", dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("L-TOO-FEW-SHOTS"), "{stdout}");
    assert!(stdout.contains("only 1 shots"), "{stdout}");
}

/// `ignore: [drafts/**]` excludes matching documents (root-relative
/// globs). The excluded file's finding must NOT appear; a sibling
/// non-excluded file still fires.
#[test]
fn ignore_glob_excludes_matching_docs() {
    let dir = temp_dir("ignore");
    write(&dir.join("drafts/skip.lute"), &scene_with_long_line());
    write(&dir.join("keep.lute"), &scene_with_long_line());
    write(&dir.join("lute.lint.yaml"), "ignore: [\"drafts/**\"]\n");

    let out = run(&["lint", dir.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let diags = v["diagnostics"].as_array().unwrap();
    let paths: Vec<&str> = diags.iter().map(|d| d["path"].as_str().unwrap()).collect();
    assert!(paths.contains(&"keep.lute"), "{paths:?}");
    assert!(!paths.iter().any(|p| p.starts_with("drafts/")), "{paths:?}");
}

/// `--json` emits the documented shape: top-level `ok`, `diagnostics`
/// (with `path`), and `configDiagnostics` — even when empty.
#[test]
fn json_shape_is_stable() {
    let dir = temp_dir("json");
    write(&dir.join("scene.lute"), &scene_with_long_line());

    let out = run(&["lint", dir.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(0));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], true);
    assert!(v["diagnostics"].is_array(), "{v}");
    assert!(v["configDiagnostics"].is_array(), "{v}");
    let d = v["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "L-DIALOGUE-LENGTH")
        .unwrap();
    assert_eq!(d["severity"], "warning");
    assert_eq!(d["path"], "scene.lute");
    assert!(d.get("denied").is_none());
}

/// `--deny L-DIALOGUE-LENGTH` promotes the warning to an error: exit 1,
/// `severity: error`, `denied: true`.
#[test]
fn deny_promotes_lint_code() {
    let dir = temp_dir("deny");
    write(&dir.join("scene.lute"), &scene_with_long_line());

    let out = run(&["lint", dir.to_str().unwrap(), "--json", "--deny", "L-DIALOGUE-LENGTH"]);
    assert_eq!(out.status.code(), Some(1), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
    let d = v["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "L-DIALOGUE-LENGTH")
        .unwrap();
    assert_eq!(d["severity"], "error");
    assert_eq!(d["denied"], true);
}

/// A typo'd `--deny` code is a clap usage error (exit 2) — parity with
/// `check`'s registry, over the `L-*` / `E-LINT-*` shape.
#[test]
fn unknown_deny_code_is_usage_error() {
    let dir = temp_dir("bad-deny");
    write(&dir.join("scene.lute"), CLEAN_SCENE);

    let out = run(&["lint", dir.to_str().unwrap(), "--deny", "L-lowercase"]);
    assert_eq!(out.status.code(), Some(2));
}

/// A malformed `lute.lint.yaml` is a hard exit-2 failure with a stderr
/// message (spec §3: "malformed YAML ⇒ exit 2"). Semantic errors go
/// through `E-LINT-CONFIG` instead — those still exit 1 with a
/// diagnostic.
#[test]
fn malformed_yaml_is_exit_2() {
    let dir = temp_dir("malformed");
    write(&dir.join("scene.lute"), CLEAN_SCENE);
    write(&dir.join("lute.lint.yaml"), "rules: [\n  not: a: mapping\n");

    let out = run(&["lint", dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2), "stderr: {}", String::from_utf8_lossy(&out.stderr));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("malformed"), "{stderr}");
}

/// A plugin's `lints/*.yaml` export gets namespaced (`<plugin-id>/<id>`)
/// and fires when the plugin is active in the default profile. Sets up
/// `lute.project.yaml` + `plugins/<id>/plugin.yaml` + `plugins/<id>/lints/*.yaml`
/// and asserts the resulting `L-<PLUGIN>-…` diagnostic.
#[test]
fn plugin_lint_export_fires_end_to_end() {
    let dir = temp_dir("plugin");
    write(&dir.join("scene.lute"), CLEAN_SCENE);
    write(
        &dir.join("lute.project.yaml"),
        "defaultProfile: core\n\
         profiles:\n  \
           core:\n    \
             plugins:\n      \
               my-plugin: true\n",
    );
    write(
        &dir.join("plugins/my-plugin/plugin.yaml"),
        "id: my-plugin\n\
         version: 0.1.0\n\
         kind: capability\n\
         exports:\n  \
           lints: lints/\n",
    );
    // Fires when the scene has fewer than 3 shots — the fixture has 1.
    write(
        &dir.join("plugins/my-plugin/lints/shots.yaml"),
        "lints:\n  \
         - id: too-few-shots\n    \
           target: scene\n    \
           when: \"scene.shots < 3\"\n    \
           level: warn\n    \
           message: \"only {scene.shots} shots\"\n",
    );

    let out = run(&["lint", dir.to_str().unwrap(), "--json"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "stderr: {}\nstdout: {}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout),
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let diags = v["diagnostics"].as_array().unwrap();
    let hit = diags
        .iter()
        .find(|d| d["code"] == "L-MY-PLUGIN-TOO-FEW-SHOTS");
    assert!(hit.is_some(), "expected namespaced plugin rule; got {diags:?}");
    assert!(hit.unwrap()["message"].as_str().unwrap().contains("only 1 shots"));
}
