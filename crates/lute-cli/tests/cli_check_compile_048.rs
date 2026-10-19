//! 0.48.0 §2.1, §3.1, §3.2, §5: project discovery, `check` (documents and
//! whole project, one `0.48.0.check` report), `compile` (document, whole
//! project, `--stream`) and the removed spellings that answer with their
//! replacement. Every test fails on 0.47.2 (`check-project`, `compile --all`,
//! `compile-stream`, no discovery gate).

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_lute");
const PROJECT_YAML: &str = "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n";
const SCENE: &str = "---\nkind: scene\nid: a\n---\n## A\n@narrator: hi\n";
const BAD_SCENE: &str = "---\nkind: scene\nid: bad\n---\n## Bad\n@narrator: {{run.missing}}\n";

fn write(dir: &Path, rel: &str, text: &str) -> PathBuf {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
    path
}

fn lute_in(cwd: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap()
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn json(out: &Output) -> Value {
    serde_json::from_slice(&out.stdout)
        .unwrap_or_else(|e| panic!("{e}: {}{}", stdout(out), stderr(out)))
}

/// A project of two clean scenes and a nested subproject.
fn project(tag: &str) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(&dir, "lute.project.yaml", PROJECT_YAML);
    write(&dir, "scenes/a.lute", SCENE);
    write(
        &dir,
        "scenes/b.lute",
        "---\nkind: scene\nid: b\n---\n## B\n@narrator: two\n",
    );
    dir
}

// --- discovery (§2.1) -------------------------------------------------------

#[test]
fn discovery_starts_at_each_file_operands_directory_and_notes_once_per_project() {
    let one = project("disc-one");
    let two = project("disc-two");
    let other = lute_scratch::ScratchDir::named("disc-cwd");
    let (a, b) = (one.join("scenes/a.lute"), one.join("scenes/b.lute"));
    let c = two.join("scenes/a.lute");
    let out = lute_in(
        &other,
        &[
            "check",
            a.to_str().unwrap(),
            b.to_str().unwrap(),
            c.to_str().unwrap(),
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}{}",
        stdout(&out),
        stderr(&out)
    );
    let notes = discovery_notes(&out);
    assert_eq!(notes.len(), 2, "one note per distinct project: {notes:?}");
    assert!(notes
        .iter()
        .all(|n| n.contains("nearest lute.project.yaml")));
    // Three per-document verdicts, in operand order.
    let text = stdout(&out);
    assert_eq!(text.matches("ok:").count(), 3, "{text}");
}

fn discovery_notes(out: &Output) -> Vec<String> {
    stderr(out)
        .lines()
        .filter(|l| l.starts_with("lute: note: using project"))
        .map(str::to_string)
        .collect()
}

#[test]
fn whole_project_discovers_from_the_current_directory_and_its_ancestors() {
    let dir = project("disc-cwd");
    let at_root = lute_in(&dir, &["check"]);
    let from_sub = lute_in(&dir.join("scenes"), &["check"]);
    let explicit = lute_in(&dir, &["check", "--project", "."]);
    assert_eq!(at_root.status.code(), Some(0), "{}", stderr(&at_root));
    assert_eq!(stdout(&at_root), stdout(&explicit));
    assert_eq!(from_sub.status.code(), Some(0), "{}", stderr(&from_sub));
    assert!(
        stdout(&from_sub).contains("2 file(s)"),
        "the whole project, not the subfolder: {}",
        stdout(&from_sub)
    );
    // Directory commands print no discovery note (it is the file commands').
    assert!(!stderr(&at_root).contains("note:"), "{}", stderr(&at_root));
}

#[test]
fn none_found_is_a_usage_error_naming_project() {
    let empty = lute_scratch::ScratchDir::named("disc-none");
    for args in [
        &["check", "--json"][..],
        &["compile", "-o", "out", "--json"][..],
    ] {
        let out = lute_in(&empty, args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        let v = json(&out);
        assert_eq!(v["error"]["code"], "E-CLI-USAGE");
        assert_eq!(v["error"]["argument"], "--project");
        let message = v["error"]["message"].as_str().unwrap();
        assert!(
            message.starts_with(&format!(
                "lute {}: no lute.project.yaml at or above ",
                args[0]
            )) && message.ends_with("pass the project directory with --project <DIR>"),
            "{message}"
        );
    }
}

#[test]
fn an_explicit_directory_without_a_manifest_is_a_workspace_and_nested_projects_keep_their_own() {
    let ws = lute_scratch::ScratchDir::named("disc-workspace");
    write(&ws, "inner/lute.project.yaml", PROJECT_YAML);
    write(&ws, "inner/a.lute", SCENE);
    write(&ws, "loose/b.lute", SCENE);
    let out = lute_in(&ws, &["check", "--project", ".", "--json"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}{}",
        stdout(&out),
        stderr(&out)
    );
    let paths: Vec<String> = json(&out)["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(paths.len(), 2, "{paths:?}");
}

#[test]
fn compile_and_trace_of_a_document_gate_on_the_discovered_project_verdict() {
    // 0.47.2 refused these with the single-file E-MAYBE-UNSET; the project
    // envelope guarantees the read, and a discovered project means what
    // `--project` means.
    let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples");
    let file = examples.join("connected-outro.lute");
    let discovered = lute_in(&examples, &["compile", file.to_str().unwrap()]);
    let explicit = lute_in(
        &examples,
        &[
            "compile",
            file.to_str().unwrap(),
            "--project",
            examples.to_str().unwrap(),
        ],
    );
    assert_eq!(discovered.status.code(), Some(0), "{}", stderr(&discovered));
    assert_eq!(explicit.status.code(), Some(0), "{}", stderr(&explicit));
    assert_eq!(stdout(&discovered), stdout(&explicit));
}

// --- check, documents (§3.1) --------------------------------------------------

#[test]
fn document_mode_reports_one_entry_per_document_in_operand_order() {
    let dir = project("doc-two");
    let bad = write(&dir, "scenes/bad.lute", BAD_SCENE);
    let a = dir.join("scenes/a.lute");
    let out = lute_in(
        &dir,
        &[
            "check",
            bad.to_str().unwrap(),
            a.to_str().unwrap(),
            "--json",
        ],
    );
    assert_eq!(out.status.code(), Some(1));
    let v = json(&out);
    assert_eq!(v["command"], "check");
    assert_eq!(v["schemaVersion"], "0.48.0.check");
    assert_eq!(v["ok"], false);
    let files = v["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(files[0]["path"], bad.to_str().unwrap());
    assert_eq!(files[0]["ok"], false);
    assert_eq!(files[1]["ok"], true);
    assert!(files[1]["resolved"].is_object());
    assert!(v.get("project_diagnostics").is_none());
    // Text: each document as a lone `check` printed it, in operand order.
    let text = stdout(&lute_in(
        &dir,
        &["check", bad.to_str().unwrap(), a.to_str().unwrap()],
    ));
    let lone_bad = stdout(&lute_in(&dir, &["check", bad.to_str().unwrap()]));
    let lone_a = stdout(&lute_in(&dir, &["check", a.to_str().unwrap()]));
    assert_eq!(text, format!("{lone_bad}{lone_a}"));
}

#[test]
fn wip_is_whole_project_only_and_a_directory_operand_names_project() {
    let dir = project("doc-refusals");
    let a = dir.join("scenes/a.lute");
    let wip = lute_in(&dir, &["check", a.to_str().unwrap(), "--wip", "--json"]);
    assert_eq!(wip.status.code(), Some(2));
    let v = json(&wip);
    assert_eq!(v["error"]["code"], "E-CLI-USAGE");
    assert_eq!(v["error"]["argument"], "--wip");
    assert_eq!(lute_in(&dir, &["check", "--wip"]).status.code(), Some(0));

    let operand = dir.join("scenes");
    let by_dir = lute_in(&dir, &["check", operand.to_str().unwrap(), "--json"]);
    assert_eq!(by_dir.status.code(), Some(2));
    let v = json(&by_dir);
    assert_eq!(v["error"]["code"], "E-CLI-USAGE");
    let message = v["error"]["message"].as_str().unwrap();
    assert!(
        message.contains("is a directory — check the whole project with lute check --project"),
        "{message}"
    );
    assert_eq!(
        v["error"]["hint"],
        format!("lute check --project {}", operand.display())
    );
}

#[test]
fn an_unreadable_operand_ends_the_run_with_the_io_envelope_and_no_report() {
    let dir = project("doc-io");
    let a = dir.join("scenes/a.lute");
    let out = lute_in(
        &dir,
        &["check", a.to_str().unwrap(), "no/such.lute", "--json"],
    );
    assert_eq!(out.status.code(), Some(2));
    let v = json(&out);
    assert_eq!(v["schemaVersion"], "0.43.0.error");
    assert_eq!(v["error"]["code"], "E-CLI-IO");
    assert!(v.get("files").is_none());
}

#[test]
fn a_document_whose_project_does_not_resolve_has_an_ok_false_entry() {
    let dir = project("doc-resolve");
    let a = dir.join("scenes/a.lute");
    let out = lute_in(
        &dir,
        &[
            "check",
            a.to_str().unwrap(),
            "--permission-profile",
            "nope",
            "--json",
        ],
    );
    assert_eq!(
        out.status.code(),
        Some(1),
        "{}{}",
        stdout(&out),
        stderr(&out)
    );
    let v = json(&out);
    assert_eq!(v["ok"], false);
    let entry = &v["files"][0];
    assert_eq!(entry["ok"], false);
    assert!(entry["resolved"].is_null());
    let diagnostics = entry["diagnostics"].as_array().unwrap();
    assert!(
        diagnostics
            .iter()
            .any(|d| d["code"].as_str().unwrap().starts_with("E-")),
        "{entry}"
    );
}

// --- check, whole project (§3.1) ----------------------------------------------

#[test]
fn whole_project_reports_files_and_project_diagnostics() {
    let dir = project("proj-report");
    write(&dir, "scenes/bad.lute", BAD_SCENE);
    let out = lute_in(&dir, &["check", "--project", ".", "--json"]);
    assert_eq!(out.status.code(), Some(1));
    let v = json(&out);
    assert_eq!(v["command"], "check");
    assert_eq!(v["schemaVersion"], "0.48.0.check");
    assert_eq!(v["files"].as_array().unwrap().len(), 3);
    assert!(v["project_diagnostics"].is_array());
}

#[test]
fn an_invalid_manifest_prints_the_report_with_the_manifest_diagnostics() {
    let dir = lute_scratch::ScratchDir::named("proj-invalid");
    write(&dir, "lute.project.yaml", "profiles: [this is: not valid\n");
    write(&dir, "a.lute", SCENE);
    let out = lute_in(&dir, &["check", "--project", ".", "--json"]);
    assert_eq!(out.status.code(), Some(1), "{}", stderr(&out));
    let v = json(&out);
    assert_eq!(v["ok"], false);
    assert_eq!(v["files"], serde_json::json!([]));
    let rows = v["project_diagnostics"].as_array().unwrap();
    assert!(!rows.is_empty());
    assert!(rows[0]["path"]
        .as_str()
        .unwrap()
        .ends_with("lute.project.yaml"));
    // Text mode is unchanged: the manifest line on stdout, exit 1.
    let text = lute_in(&dir, &["check", "--project", "."]);
    assert_eq!(text.status.code(), Some(1));
    assert!(
        stdout(&text).contains("lute.project.yaml"),
        "{}",
        stdout(&text)
    );
}

#[test]
fn the_permission_profile_reaches_the_whole_project_check() {
    let dir = lute_scratch::ScratchDir::named("proj-profile");
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: authored\nprofiles:\n  authored:\n    plugins: {}\n  restricted:\n    plugins: {}\n    permissions:\n      directives: [camera]\n      bridges: []\n      rewards: false\n      quests: false\n",
    );
    write(
        &dir,
        "a.lute",
        "---\nkind: scene\nprofile: authored\ncharacter: hero\nseason: 1\nepisode: 1\n---\n\n## Opening\n\n::end\n",
    );
    let open = lute_in(&dir, &["check", "--project", "."]);
    assert_eq!(
        open.status.code(),
        Some(0),
        "{}{}",
        stdout(&open),
        stderr(&open)
    );
    let ceiling = lute_in(
        &dir,
        &[
            "check",
            "--project",
            ".",
            "--permission-profile",
            "restricted",
        ],
    );
    assert_eq!(
        ceiling.status.code(),
        Some(1),
        "{}{}",
        stdout(&ceiling),
        stderr(&ceiling)
    );
    assert!(
        stdout(&ceiling).contains("E-PERMISSION-DIRECTIVE"),
        "{}",
        stdout(&ceiling)
    );
}

// --- check --json schema ------------------------------------------------------

#[test]
fn check_outputs_name_the_new_report_version() {
    let out = lute_in(Path::new("."), &["schema", "check"]);
    let v = json(&out);
    assert_eq!(
        v["outputs"],
        serde_json::json!([{"flag": "--json", "kind": "report", "schemaVersion": "0.48.0.check"}])
    );
    let compile = json(&lute_in(Path::new("."), &["schema", "compile"]));
    let outputs: Vec<(Value, Value)> = compile["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| (o["kind"].clone(), o["flag"].clone()))
        .collect();
    assert_eq!(
        outputs,
        vec![
            ("artifact".into(), Value::Null),
            ("report".into(), "--json".into()),
            ("stream".into(), "--stream".into()),
        ]
    );
    for removed in ["check-project", "compile-stream"] {
        assert_eq!(
            lute_in(Path::new("."), &["schema", removed]).status.code(),
            Some(2)
        );
    }
}

// --- compile (§3.2) -------------------------------------------------------------

#[test]
fn compile_without_a_document_writes_the_project_and_its_index() {
    let dir = project("compile-all");
    let out = lute_in(&dir, &["compile", "-o", "out"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    assert!(dir.join("out/project.index.json").is_file());
    assert!(dir.join("out/scenes/a.lute.json").is_file());
    let missing = lute_in(&dir, &["compile", "--json"]);
    assert_eq!(missing.status.code(), Some(2));
    assert_eq!(json(&missing)["error"]["argument"], "--output");
}

#[test]
fn output_and_locale_bundle_are_the_new_spellings() {
    let dir = project("compile-output");
    let file = dir.join("scenes/a.lute");
    let out = lute_in(&dir, &["compile", file.to_str().unwrap(), "-o", "a.json"]);
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let long = lute_in(
        &dir,
        &["compile", file.to_str().unwrap(), "--output", "b.json"],
    );
    assert_eq!(long.status.code(), Some(0));
    assert_eq!(
        std::fs::read(dir.join("a.json")).unwrap(),
        std::fs::read(dir.join("b.json")).unwrap()
    );
    let missing = lute_in(
        &dir,
        &[
            "compile",
            file.to_str().unwrap(),
            "--locale-bundle",
            "none.json",
        ],
    );
    assert_eq!(missing.status.code(), Some(2));
    assert!(
        stderr(&missing).contains("cannot read none.json"),
        "{}",
        stderr(&missing)
    );
}

fn stream(args: &[&str], cwd: &Path, body: &str) -> Output {
    let mut child = Command::new(BIN)
        .args(args)
        .current_dir(cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(body.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn stream_is_the_old_compile_stream_and_its_conflicts_name_the_option() {
    let dir = project("compile-stream");
    let prefix = write(&dir, "stream.lute", "---\nkind: scene\nid: s\n---\n## S\n");
    let out = stream(
        &["compile", "--stream", prefix.to_str().unwrap()],
        &dir,
        "@narrator: hello\n",
    );
    assert_eq!(out.status.code(), Some(0), "{}", stderr(&out));
    let lines: Vec<Value> = stdout(&out)
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(!lines.is_empty());
    assert!(lines.iter().all(|l| l.get("kind").is_some()), "{lines:?}");

    let p = prefix.to_str().unwrap();
    for (args, argument) in [
        (
            vec!["compile", "--stream", p, "-o", "x.json", "--json"],
            "--output",
        ),
        (
            vec![
                "compile",
                "--stream",
                p,
                "--locale-bundle",
                "x.json",
                "--json",
            ],
            "--locale-bundle",
        ),
        (vec!["compile", "--stream", p, "--json"], "--json"),
        (vec!["compile", "--stream", "--json"], "--stream"),
    ] {
        let out = lute_in(&dir, &args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        let v = json(&out);
        assert_eq!(v["error"]["code"], "E-CLI-USAGE", "{args:?}");
        assert_eq!(v["error"]["argument"], argument, "{args:?}");
    }
}

#[test]
fn stream_refuses_the_gate_options() {
    let dir = project("compile-stream-gate");
    let prefix = write(&dir, "stream.lute", "---\nkind: scene\nid: s\n---\n## S\n");
    let p = prefix.to_str().unwrap();
    for (flag, extra) in [("--deny", Some("W-WIP")), ("--deny-warnings", None)] {
        let mut args = vec!["compile", "--stream", p, flag];
        args.extend(extra);
        let out = lute_in(&dir, &args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        assert!(
            stderr(&out).contains(&format!("--stream does not take {flag}")),
            "{}",
            stderr(&out)
        );
    }
}

// --- removed spellings (§5) -----------------------------------------------------

#[test]
fn removed_commands_answer_with_the_rewritten_command_and_no_command_id() {
    let dir = project("removed-commands");
    let state = lute_scratch::ScratchDir::named("removed-telemetry");
    for (args, argument, rewritten) in [
        (
            vec!["check-project", "docs/x", "--json"],
            "check-project",
            "lute check --project docs/x --json",
        ),
        (
            vec!["check-project", "--wip", "d", "--deny", "W-WIP", "--json"],
            "check-project",
            "lute check --project d --wip --deny W-WIP --json",
        ),
        (
            vec!["compile-stream", "s.lute", "--providers", "p", "--json"],
            "compile-stream",
            "lute compile --stream s.lute --providers p --json",
        ),
    ] {
        let out = Command::new(BIN)
            .args(&args)
            .current_dir(&dir)
            .env("LUTE_TELEMETRY", "1")
            .env_remove("DO_NOT_TRACK")
            .env("XDG_STATE_HOME", &*state)
            .output()
            .unwrap();
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        let v = json(&out);
        assert_eq!(v["command"], Value::Null, "{args:?}");
        assert_eq!(v["error"]["code"], "E-CLI-USAGE");
        assert_eq!(v["error"]["argument"], argument);
        assert_eq!(v["error"]["hint"], rewritten);
        assert_eq!(
            v["error"]["message"],
            format!("lute {argument} is now `{rewritten}`")
        );
        assert!(stderr(&out).contains(rewritten), "{}", stderr(&out));
    }
    let telemetry = std::fs::read_to_string(state.join("lute/telemetry.jsonl")).unwrap();
    for line in telemetry.lines() {
        let record: Value = serde_json::from_str(line).unwrap();
        assert_eq!(record["command"], Value::Null, "{record}");
    }
    assert_eq!(telemetry.lines().count(), 3);
}

#[test]
fn removed_compile_options_answer_with_the_replacement() {
    let dir = project("removed-compile");
    for (args, argument, rewritten) in [
        (
            vec!["compile", "--all", "proj", "-o", "out", "--json"],
            "--all",
            "lute compile --project proj -o out --json",
        ),
        (
            vec![
                "compile",
                "--all",
                "--project",
                "proj",
                "-o",
                "out",
                "--json",
            ],
            "--all",
            "lute compile --project proj -o out --json",
        ),
        (
            vec!["compile", "scene.lute", "--out", "x.json", "--json"],
            "--out",
            "lute compile scene.lute --output x.json --json",
        ),
        (
            vec!["compile", "scene.lute", "--locales=b.json", "--json"],
            "--locales",
            "lute compile scene.lute --locale-bundle=b.json --json",
        ),
    ] {
        let out = lute_in(&dir, &args);
        assert_eq!(out.status.code(), Some(2), "{args:?}");
        let v = json(&out);
        assert_eq!(v["command"], "compile", "{args:?}");
        assert_eq!(v["error"]["code"], "E-CLI-USAGE");
        assert_eq!(v["error"]["argument"], argument, "{args:?}");
        assert_eq!(v["error"]["hint"], rewritten, "{args:?}");
        assert!(
            v["error"]["message"].as_str().unwrap().contains(rewritten),
            "{v}"
        );
    }
    assert!(!dir.join("out").exists());
}
