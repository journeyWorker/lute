//! dsl 0.50.0 §6 (#166b): `lute.project.yaml` `diagnostics:` — a project's
//! level for a warning code, everywhere or under `paths` globs, applied where
//! diagnostics are reported (the CLI's severity policy).
//!
//! On 0.49.1 `diagnostics:` is `E-MANIFEST-KEY` and the project fails, so the
//! level tests below fail there. The `compile --deny` tests fail there too:
//! 0.49.1 compiled past a `--deny`-promoted check warning (#204).

use std::path::Path;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

const MANIFEST: &str = "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n";

/// A scene whose `@vesna` line follows her declared exit:
/// `W-STAGE-ABSENT` at line 17.
fn stage_absent_scene(id: &str) -> String {
    format!(
        "---\nkind: scene\nid: {id}\nenums:\n  action:\n    members: [show, hide]\n    \
         exits: [hide]\n  anchor:\n    members: [left, center, right]\n    default: center\n\
         ---\n\n## Shot 1.\n\n::actor{{character=\"vesna\" action=\"show\"}}\n\
         ::actor{{character=\"vesna\" action=\"hide\"}}\n@vesna: Gone.\n"
    )
}

/// A project of `scenes/a.lute` and `scenes/imported/b.lute`, each with one
/// `W-STAGE-ABSENT`, under a manifest ending in `diagnostics` (empty: none).
fn project(tag: &str, diagnostics: &str) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    let manifest = if diagnostics.is_empty() {
        MANIFEST.to_string()
    } else {
        format!("{MANIFEST}diagnostics:\n{diagnostics}")
    };
    std::fs::write(dir.join("lute.project.yaml"), manifest).unwrap();
    std::fs::create_dir_all(dir.join("scenes/imported")).unwrap();
    std::fs::write(dir.join("scenes/a.lute"), stage_absent_scene("a")).unwrap();
    std::fs::write(dir.join("scenes/imported/b.lute"), stage_absent_scene("b")).unwrap();
    dir
}

fn lute(dir: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .env("LUTE_TELEMETRY", "0")
        .output()
        .unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn json(o: &Output) -> serde_json::Value {
    serde_json::from_slice(&o.stdout).unwrap_or_else(|e| panic!("{e}: {}", stdout(o)))
}

const ABSENT_LINE: &str = "warning [W-STAGE-ABSENT]";

/// The #166 repro: `W-STAGE-ABSENT` off under `scenes/imported/**` is
/// suppressed there, reported elsewhere, and the summary counts it.
#[test]
fn a_paths_scoped_off_suppresses_only_under_its_globs() {
    let dir = project(
        "dl-repro",
        "  W-STAGE-ABSENT: { level: off, paths: [\"scenes/imported/**\"] }\n",
    );
    let out = lute(&dir, &["check"]);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(
        text.contains(&format!("./scenes/a.lute:17:1: {ABSENT_LINE}")),
        "{text}"
    );
    assert!(!text.contains("./scenes/imported/b.lute:17"), "{text}");
    assert!(
        text.contains("ok: ./scenes/a.lute (1 warning(s))\n"),
        "{text}"
    );
    assert!(
        text.contains(
            "ok: ./scenes/imported/b.lute (0 warning(s), 1 suppressed by lute.project.yaml)\n"
        ),
        "{text}"
    );
    // A single-file check of the suppressed document applies its project too.
    let one = lute(&dir, &["check", "scenes/imported/b.lute"]);
    assert_eq!(
        stdout(&one),
        "ok: scenes/imported/b.lute (0 warning(s), 1 suppressed by lute.project.yaml)\n"
    );
}

#[test]
fn json_reports_suppressed_counts_and_configured_markers() {
    let dir = project(
        "dl-json",
        "  W-STAGE-ABSENT: { level: off, paths: [\"scenes/imported/**\"] }\n",
    );
    let report = json(&lute(&dir, &["check", "--json"]));
    let files = report["files"].as_array().unwrap();
    let b = files
        .iter()
        .find(|f| f["path"].as_str().unwrap().ends_with("imported/b.lute"))
        .unwrap();
    assert_eq!(b["suppressed"], serde_json::json!({ "W-STAGE-ABSENT": 1 }));
    assert_eq!(b["diagnostics"], serde_json::json!([]));
    let a = files
        .iter()
        .find(|f| f["path"].as_str().unwrap().ends_with("scenes/a.lute"))
        .unwrap();
    assert!(a.get("suppressed").is_none(), "{a}");

    let dir = project("dl-json-info", "  W-STAGE-ABSENT: info\n");
    let report = json(&lute(&dir, &["check", "--json"]));
    let diag = &report["files"][0]["diagnostics"][0];
    assert_eq!(diag["code"], "W-STAGE-ABSENT");
    assert_eq!(diag["severity"], "info");
    assert_eq!(diag["configured"], "lute.project.yaml");
    assert!(diag.get("denied").is_none(), "{diag}");
}

/// No config: every output is byte-identical to a manifest without the key.
#[test]
fn an_empty_block_changes_nothing() {
    let plain = project("dl-plain", "");
    let empty = project("dl-empty", "  {}\n");
    for args in [
        &["check"][..],
        &["check", "--json"],
        &["check", "scenes/a.lute"],
    ] {
        let a = lute(&plain, args);
        let b = lute(&empty, args);
        assert_eq!(stdout(&a), stdout(&b), "{args:?}");
        assert_eq!(a.status.code(), b.status.code(), "{args:?}");
    }
}

#[test]
fn an_error_level_fails_check_and_compile() {
    let dir = project("dl-error", "  W-STAGE-ABSENT: error\n");
    let out = lute(&dir, &["check", "scenes/a.lute"]);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(
        text.contains("scenes/a.lute:17:1: error [W-STAGE-ABSENT] [project] "),
        "{text}"
    );
    assert!(
        text.contains("failed: scenes/a.lute (1 error(s), 0 warning(s))"),
        "{text}"
    );
    let report = json(&lute(&dir, &["check", "scenes/a.lute", "--json"]));
    let diag = &report["files"][0]["diagnostics"][0];
    assert_eq!(diag["severity"], "error");
    assert_eq!(diag["configured"], "lute.project.yaml");
    assert_eq!(report["ok"], false);

    let out = lute(&dir, &["compile", "scenes/a.lute"]);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(
        text.contains("error [W-STAGE-ABSENT] [project] ")
            && text.ends_with("1 error(s); no execution IR emitted\n"),
        "{text}"
    );
    let out = lute(&dir, &["compile", "-o", "out"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(!dir.join("out/project.index.json").exists());
}

/// `--deny W-X` beats the config, even an `off`; `--deny-warnings`
/// neither resurrects `off` nor promotes `info`.
#[test]
fn deny_and_deny_warnings_against_the_config() {
    let off = project("dl-deny-off", "  W-STAGE-ABSENT: off\n");
    let out = lute(
        &off,
        &["check", "scenes/a.lute", "--deny", "W-STAGE-ABSENT"],
    );
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(text.contains("error [W-STAGE-ABSENT] [denied] "), "{text}");
    let out = lute(&off, &["check", "scenes/a.lute", "--deny-warnings"]);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    assert_eq!(
        stdout(&out),
        "ok: scenes/a.lute (0 warning(s), 1 suppressed by lute.project.yaml)\n"
    );

    let info = project("dl-deny-info", "  W-STAGE-ABSENT: info\n");
    let out = lute(&info, &["check", "scenes/a.lute", "--deny-warnings"]);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(text.contains("info [W-STAGE-ABSENT] [project] "), "{text}");
    assert!(
        text.contains("ok: scenes/a.lute (0 warning(s), 1 info)"),
        "{text}"
    );
}

/// §6.2: levels never change what a `.test.yaml` expectation observes — a
/// project `error` level does not refuse the test's trace.
#[test]
fn an_error_level_does_not_refuse_a_test_trace() {
    let dir = project("dl-test", "  W-STAGE-ABSENT: error\n");
    std::fs::create_dir_all(dir.join("tests")).unwrap();
    std::fs::write(
        dir.join("tests/a.test.yaml"),
        "file: ../scenes/a.lute\nexpect:\n  transcriptContains: [\"@vesna: Gone.\"]\n",
    )
    .unwrap();
    let out = lute(&dir, &["test"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}{}",
        stdout(&out),
        String::from_utf8_lossy(&out.stderr)
    );
}

/// #204: `compile` gates on a `--deny`/`--deny-warnings`-promoted check
/// warning, as its help says. 0.49.1 emitted the IR with exit 0.
#[test]
fn compile_gates_on_a_denied_check_warning() {
    let dir = project("dl-compile-deny", "");
    for flag in [&["--deny", "W-STAGE-ABSENT"][..], &["--deny-warnings"]] {
        let mut args = vec!["compile", "scenes/a.lute"];
        args.extend_from_slice(flag);
        let out = lute(&dir, &args);
        let text = stdout(&out);
        assert_eq!(out.status.code(), Some(1), "{flag:?}: {text}");
        assert!(
            text.contains("error [W-STAGE-ABSENT] [denied] "),
            "{flag:?}: {text}"
        );
        let mut args = vec!["compile", "-o", "out"];
        args.extend_from_slice(flag);
        let out = lute(&dir, &args);
        assert_eq!(out.status.code(), Some(1), "{flag:?}: {}", stdout(&out));
    }
    // Without a gate option it compiles.
    let out = lute(&dir, &["compile", "scenes/a.lute"]);
    assert_eq!(out.status.code(), Some(0));
}

/// Each `E-DIAGNOSTICS-CONFIG` fault, reported once at the manifest; the
/// manifest is invalid, and every bad entry is dropped.
#[test]
fn each_config_fault_is_reported_at_the_manifest() {
    let dir = project(
        "dl-faults",
        "  E-UNKNOWN-ATTR: off\n  L-LINE-LENGTH: off\n  W-STAGE-ABSNT: off\n  \
         W-TEXT-GLUE: silent\n  W-WIP: { level: off, paths: [] }\n  \
         W-SEASON-UNGATED: { level: off, paths: [\"/abs/**\"] }\n  \
         W-STAGE-ABSENT: { level: off, paths: [\"../x/**\"] }\n",
    );
    let out = lute(&dir, &["check"]);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{text}");
    let faults: Vec<&str> = text
        .lines()
        .filter(|l| l.contains("[E-DIAGNOSTICS-CONFIG]"))
        .collect();
    assert_eq!(
        faults,
        [
            "./lute.project.yaml:6:3: error [E-DIAGNOSTICS-CONFIG] `E-UNKNOWN-ATTR` is an error: an error is a contract violation and cannot be re-levelled; fix the reported source instead",
            "./lute.project.yaml:7:3: error [E-DIAGNOSTICS-CONFIG] `L-LINE-LENGTH` is a lint rule; set its level in `lute.lint.yaml` `rules:`",
            "./lute.project.yaml:8:3: error [E-DIAGNOSTICS-CONFIG] `W-STAGE-ABSNT` is not a diagnostic code — did you mean `W-STAGE-ABSENT`?",
            "./lute.project.yaml:9:3: error [E-DIAGNOSTICS-CONFIG] `W-TEXT-GLUE`: level `silent` is not one of `off`, `info`, `warn`, `error`",
            "./lute.project.yaml:10:3: error [E-DIAGNOSTICS-CONFIG] `W-WIP`: `paths` takes project-relative globs, e.g. `scenes/imported/**`",
            "./lute.project.yaml:11:3: error [E-DIAGNOSTICS-CONFIG] `W-SEASON-UNGATED`: `paths` takes project-relative globs, e.g. `scenes/imported/**`",
            "./lute.project.yaml:12:3: error [E-DIAGNOSTICS-CONFIG] `W-STAGE-ABSENT`: `paths` takes project-relative globs, e.g. `scenes/imported/**`",
        ],
        "{text}"
    );
    // The dropped `W-STAGE-ABSENT` entry does not apply: a single-file
    // check still reports the warning.
    let one = stdout(&lute(&dir, &["check", "scenes/a.lute"]));
    assert!(one.contains(ABSENT_LINE), "{one}");
}

/// `E-LINT-CONFIG` names a checker code in `lute.lint.yaml` `rules:`.
#[test]
fn a_checker_code_in_lint_rules_points_at_the_project_manifest() {
    let dir = project("dl-lint", "");
    std::fs::write(
        dir.join("lute.lint.yaml"),
        "rules:\n  W-STAGE-ABSENT: off\n  E-UNKNOWN-ATTR: off\n",
    )
    .unwrap();
    let out = lute(&dir, &["lint"]);
    let text = format!("{}{}", stdout(&out), String::from_utf8_lossy(&out.stderr));
    assert!(
        text.contains(
            "`W-STAGE-ABSENT` is a checker code, not a lint rule — set its level in \
             `lute.project.yaml` `diagnostics:`"
        ),
        "{text}"
    );
    assert!(
        text.contains(
            "`E-UNKNOWN-ATTR` is a checker code, not a lint rule — an error cannot be re-levelled"
        ),
        "{text}"
    );
}

/// A plugin project (occasion `go`, two cast members both named `Old Ned`)
/// whose scene `a` answers `go` with a `W-STAGE-ABSENT` and, with `b`, raises
/// the project-wide `W-DISPLAY-NAME-DUP`; `plays/p.play.yaml` plays `go`.
fn plugin_project(tag: &str, diagnostics: &str) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write_plugin_project(&dir, diagnostics);
    dir
}

/// [`plugin_project`]'s files, written under `dir`.
fn write_plugin_project(dir: &Path, diagnostics: &str) {
    let write = |rel: &str, text: &str| {
        let path = dir.join(rel);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    };
    write(
        "plugins/p/plugin.yaml",
        "id: p\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } \
         ]\nexports:\n  occasions: occasions/\n  cast: cast/\n",
    );
    write(
        "plugins/p/occasions/o.yaml",
        "occasions:\n  go: { select: first }\n",
    );
    write(
        "plugins/p/cast/c.yaml",
        "cast:\n  oldNed: { name: Old Ned }\n  fisherNed: { name: Old Ned }\n  vesna: { name: \
         Vesna }\n",
    );
    write(
        "lute.project.yaml",
        &format!(
            "pluginsDir: plugins/\ndefaultProfile: base\nprofiles:\n  base: {{ plugins: {{ p: \
             true }} }}\ndiagnostics:\n{diagnostics}"
        ),
    );
    write(
        "scenes/a.lute",
        "---\nkind: scene\nid: a\non: go\nenums:\n  action:\n    members: [show, hide]\n    \
         exits: [hide]\n  anchor:\n    members: [left, center, right]\n    default: center\n\
         ---\n\n## Shot 1.\n\n@oldNed: Hi.\n::actor{character=\"vesna\" action=\"show\"}\n\
         ::actor{character=\"vesna\" action=\"hide\"}\n@vesna: Gone.\n",
    );
    write(
        "scenes/b.lute",
        "---\nkind: scene\nid: b\n---\n\n## Shot 1.\n\n@fisherNed: Hello.\n",
    );
    write(
        "plays/p.play.yaml",
        "steps:\n  - occasion: go\n    expect: { winner: a }\n",
    );
}

/// §6.2: a simulation runs on native results — a project `error` level
/// fails neither `lute play` nor a `.play.yaml` under `lute test`.
#[test]
fn an_error_level_does_not_refuse_a_play() {
    let dir = plugin_project(
        "dl-play",
        "  W-STAGE-ABSENT: error\n  W-DISPLAY-NAME-DUP: error\n",
    );
    let out = lute(&dir, &["play", "--script", "plays/p.play.yaml"]);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(0), "{text}");
    assert!(text.contains("@vesna: Gone."), "{text}");
    // `lute test` (the `.play.yaml` plus a trace of the promoted document):
    // results and exit code identical to the same project without levels —
    // project levels gate `check`/`compile`; tests are not a second gate.
    std::fs::create_dir_all(dir.join("tests")).unwrap();
    let trace = "file: ../scenes/a.lute\nexpect:\n  transcriptContains: [\"@vesna: Gone.\"]\n";
    std::fs::write(dir.join("tests/a.test.yaml"), trace).unwrap();
    let plain = plugin_project("dl-play-plain", "");
    std::fs::create_dir_all(plain.join("tests")).unwrap();
    std::fs::write(plain.join("tests/a.test.yaml"), trace).unwrap();
    let out = lute(&dir, &["test"]);
    let baseline = lute(&plain, &["test"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}{}",
        stdout(&out),
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.status.code(), baseline.status.code());
    assert_eq!(stdout(&out), stdout(&baseline));
    // `--deny` still refuses before the simulation, as in 0.49.1.
    let out = lute(
        &dir,
        &[
            "play",
            "--script",
            "plays/p.play.yaml",
            "--deny",
            "W-STAGE-ABSENT",
        ],
    );
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
}

/// §6.3: a project-wide diagnostic a project level promotes fails
/// `compile -o` and is reported as the policy shows it: `[project]` in the
/// human line, the COMPILE envelope with `configured` under `--json`.
#[test]
fn a_promoted_project_wide_diagnostic_fails_compile_with_its_report() {
    let dir = plugin_project("dl-project-wide", "  W-DISPLAY-NAME-DUP: error\n");
    let out = lute(&dir, &["compile", "-o", "out"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert_eq!(out.status.code(), Some(1), "{err}");
    assert!(
        err.contains("error [W-DISPLAY-NAME-DUP] [project] display name `Old Ned`"),
        "{err}"
    );
    let out = lute(&dir, &["compile", "-o", "out", "--json"]);
    assert_eq!(out.status.code(), Some(1));
    let report = json(&out);
    assert_eq!(report["command"], "compile");
    let diag = &report["diagnostics"][0];
    assert_eq!(diag["code"], "W-DISPLAY-NAME-DUP");
    assert_eq!(diag["severity"], "error");
    assert_eq!(diag["configured"], "lute.project.yaml");
}

/// `lute lint` reports the checker's `W-DISPLAY-NAME-DUP`; the project's
/// level applies there too, and the summary counts what it suppressed.
#[test]
fn lint_applies_the_project_level_to_the_checker_warning() {
    let dir = plugin_project("dl-lint-level", "  W-DISPLAY-NAME-DUP: off\n");
    let out = lute(&dir, &["lint", "."]);
    let text = stdout(&out);
    assert!(!text.contains("[W-DISPLAY-NAME-DUP]"), "{text}");
    assert!(
        text.contains(", 1 suppressed by lute.project.yaml)"),
        "{text}"
    );
    let report = json(&lute(&dir, &["lint", ".", "--json"]));
    assert_eq!(
        report["suppressed"],
        serde_json::json!({ "W-DISPLAY-NAME-DUP": 1 })
    );
}

/// The promoted `W-DISPLAY-NAME-DUP` of [`plugin_project`]: `lute lint`
/// prints it at `scenes/b.lute` (relative to its own project root) as a
/// configured error, and fails.
fn assert_lint_promotes_the_dup(dir: &Path, target: &str) {
    let out = lute(dir, &["lint", target]);
    let text = stdout(&out);
    assert_eq!(out.status.code(), Some(1), "{text}");
    assert!(
        text.contains("scenes/b.lute:8:1: error [W-DISPLAY-NAME-DUP] [project] display name"),
        "{text}"
    );
    let report = json(&lute(dir, &["lint", target, "--json"]));
    let diag = &report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "W-DISPLAY-NAME-DUP")
        .unwrap_or_else(|| panic!("{report:#}"))
        .clone();
    assert_eq!(diag["path"], "scenes/b.lute", "{report:#}");
    assert_eq!(diag["severity"], "error", "{report:#}");
    assert_eq!(diag["configured"], "lute.project.yaml", "{report:#}");
}

/// A nested project's diagnostic is matched against the invoked project's
/// `paths` by its real file (`nested/scenes/b.lute`), although lint prints
/// it relative to the nested root.
#[test]
fn lint_levels_match_a_nested_project_file_by_its_real_path() {
    let outer = lute_scratch::ScratchDir::named("dl-lint-nested");
    std::fs::write(
        outer.join("lute.project.yaml"),
        format!(
            "{MANIFEST}diagnostics:\n  W-DISPLAY-NAME-DUP: {{ level: error, paths: \
             [\"nested/scenes/**\"] }}\n"
        ),
    )
    .unwrap();
    write_plugin_project(&outer.join("nested"), "");
    assert_lint_promotes_the_dup(&outer, ".");
}

/// A same-named file in the working directory does not stand in for the
/// project's: `lute lint project` matches `project/scenes/b.lute`.
#[test]
fn lint_levels_ignore_a_same_named_file_in_the_working_directory() {
    let cwd = lute_scratch::ScratchDir::named("dl-lint-cwd");
    write_plugin_project(
        &cwd.join("project"),
        "  W-DISPLAY-NAME-DUP: { level: error, paths: [\"scenes/**\"] }\n",
    );
    std::fs::create_dir_all(cwd.join("scenes")).unwrap();
    for name in ["a", "b"] {
        std::fs::write(cwd.join(format!("scenes/{name}.lute")), "unrelated\n").unwrap();
    }
    assert_lint_promotes_the_dup(&cwd, "project");
}
