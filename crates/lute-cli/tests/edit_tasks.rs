use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};

use lute_model::{apply_patch_to, project_revision, PatchRefusal, PatchRequest, ProjectModel};

fn workspace() -> PathBuf { PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..") }
fn root() -> PathBuf { workspace().join("conformance/edit-tasks") }
fn readj(path: &Path) -> Value { serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap() }

fn build_base(base: &str) -> ProjectModel {
    let path = workspace().join(base);
    ProjectModel::build_single_root(&path, &Default::default())
        .unwrap_or_else(|error| panic!("cannot build {base}: {error}"))
}

static SUMMER: LazyLock<ProjectModel> = LazyLock::new(|| build_base("docs/examples/games/summer-station"));
static ASHEN: LazyLock<ProjectModel> = LazyLock::new(|| build_base("docs/examples/games/ashen-stair"));
static MONSTER: LazyLock<ProjectModel> = LazyLock::new(|| build_base("docs/examples/games/monster-league"));
static HOLLOW: LazyLock<ProjectModel> = LazyLock::new(|| build_base("docs/examples/games/hollow-ward"));
static JOB_RESTRICTION: LazyLock<ProjectModel> = LazyLock::new(|| build_base("conformance/edit-tasks/_games/job-restriction"));

fn model(base: &str) -> &'static ProjectModel {
    match base {
        "docs/examples/games/summer-station" => &SUMMER,
        "docs/examples/games/ashen-stair" => &ASHEN,
        "docs/examples/games/monster-league" => &MONSTER,
        "docs/examples/games/hollow-ward" => &HOLLOW,
        "conformance/edit-tasks/_games/job-restriction" => &JOB_RESTRICTION,
        _ => panic!("unknown task base {base}"),
    }
}


fn base_path(task: &Path) -> String {
    readj(&task.join("task.json"))["base"].as_str().unwrap().to_owned()
}

fn fill(value: &mut Value, project: &str, files: &BTreeMap<String, String>) {
    match value {
        Value::String(text) => {
            if text == "@BASE@" {
                *text = project.to_owned();
            } else if let Some(key) = text.strip_prefix("@BASE_FILE:").and_then(|x| x.strip_suffix('@')) {
                *text = files.get(key).unwrap_or_else(|| panic!("missing base file {key}")).clone();
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|item| fill(item, project, files)),
        Value::Object(items) => items.values_mut().for_each(|item| fill(item, project, files)),
        _ => {}
    }
}

fn refusal_json(refusal: &PatchRefusal) -> Value {
    if let PatchRefusal::Io { path, message } = refusal {
        return json!({
            "schemaVersion": "0.36.0.patch",
            "ok": false,
            "error": {"kind": "io", "path": path, "message": message},
        });
    }
    let mut value = json!({
        "schemaVersion": "0.36.0.patch",
        "ok": false,
        "code": refusal.code(),
        "message": refusal.message(),
    });
    match refusal {
        PatchRefusal::Stale { expected, actual } => {
            value["expected"] = Value::String(expected.clone());
            value["actual"] = Value::String(actual.clone());
        }
        PatchRefusal::Target { node, reason } => {
            value["target"] = Value::String(node.canonical());
            value["reason"] = Value::String(reason.clone());
        }
        PatchRefusal::Edit { index, reason } => {
            value["index"] = json!(index);
            value["reason"] = Value::String(reason.clone());
        }
        PatchRefusal::Check(diagnostics) => {
            value["diagnostics"] = serde_json::to_value(diagnostics).unwrap_or_else(|_| Value::Array(Vec::new()));
        }
        PatchRefusal::Preserve(changes) => {
            value["changes"] = Value::Array(changes.iter().map(|(preserve, change)| {
                let mut item = serde_json::to_value(change).unwrap_or_else(|_| json!({}));
                item["preserve"] = Value::String(preserve.clone());
                item
            }).collect());
        }
        PatchRefusal::Io { .. } => unreachable!(),
    }
    value
}

fn apply(model: &ProjectModel, value: Value) -> (i32, Value) {
    let request: PatchRequest = serde_json::from_value(value).unwrap();
    match apply_patch_to(model, request, true) {
        Ok(report) => {
            let mut output = serde_json::to_value(report).unwrap();
            output["schemaVersion"] = Value::String("0.36.0.patch".into());
            output["ok"] = Value::Bool(true);
            let status = if output["diff"]["changes"].as_array().is_some_and(|changes| changes.is_empty()) { 0 } else { 1 };
            (status, output)
        }
        Err(refusal) => (2, refusal_json(&refusal)),
    }
}

fn changes(value: &Value) -> &[Value] { value["diff"]["changes"].as_array().map(Vec::as_slice).unwrap_or(&[]) }
fn item(changes: &[Value], expected: &Value) -> bool {
    let node = expected["node"].as_str().unwrap_or("");
    let kind = expected["kind"].as_str().unwrap_or("");
    changes.iter().any(|change| change["node"].as_str() == Some(node) && change["kind"].as_str() == Some(kind))
}
fn forbidden(changes: &[Value], expected: &Value) -> bool {
    match expected {
        Value::String(node) => changes.iter().any(|change| change["node"].as_str() == Some(node)),
        Value::Object(_) => item(changes, expected),
        _ => false,
    }
}
fn preserved(changes: &[Value], expected: &Value) -> Vec<String> {
    let ids = expected["ids"].as_array().cloned().unwrap_or_default();
    let mut output = Vec::new();
    for change in changes {
        let node = change["node"].as_str().unwrap_or("");
        let kind = change["kind"].as_str().unwrap_or("");
        if ids.iter().any(|id| id.as_str() == Some(node)) && (kind == "added" || kind == "removed" || kind.to_ascii_lowercase().contains("id")) {
            output.push(format!("{node}:{kind}"));
        }
        if expected["lineIds"] == true && kind.to_ascii_lowercase().contains("lineid") {
            output.push(format!("{node}:{kind}"));
        }
        if expected["voiceKeys"] == true && kind.to_ascii_lowercase().contains("voice") {
            output.push(format!("{node}:{kind}"));
        }
        if let Some(reachability) = expected["reachability"].as_array() {
            if reachability.iter().any(|value| value.as_str() == Some(node)) && kind == "reachability" {
                output.push(format!("{node}:{kind}"));
            }
        }
    }
    output.sort();
    output.dedup();
    output
}

fn revision(model: &ProjectModel) -> lute_model::ProjectRevision {
    let inputs = model.revisions().files.keys().map(|path| {
        (path.clone(), fs::read(model.root().join(path)).unwrap())
    }).collect::<Vec<_>>();
    project_revision(model.root(), &inputs).unwrap()
}

fn record(task: &str, result: Value) {
    static RESULTS: LazyLock<Mutex<BTreeMap<String, Value>>> = LazyLock::new(|| Mutex::new(BTreeMap::new()));
    let mut results = match RESULTS.lock() { Ok(guard) => guard, Err(poisoned) => poisoned.into_inner() };
    if std::env::var_os("LUTE_BLESS_EDIT_TASKS").is_some() {
        results.insert(task.to_owned(), result);
        if results.len() == 12 {
            fs::write(root().join("REPORT.json"), serde_json::to_vec_pretty(&*results).unwrap()).unwrap();
        }
    } else {
        let expected = readj(&root().join("REPORT.json"));
        assert_eq!(expected[task], result, "REPORT stale for {task}");
    }
}

fn run_task(task_name: &str) {
    let task = root().join(task_name);
    let base_name = base_path(&task);
    let model = model(&base_name);
    let before = model.revisions().clone();
    let project = before.sha256.clone();
    let files = before.files.iter().map(|(path, revision)| (path.to_string_lossy().into_owned(), revision.sha256.clone())).collect::<BTreeMap<_, _>>();

    let mut patch = readj(&task.join("patch.json"));
    fill(&mut patch, &project, &files);
    let (status, report) = apply(model, patch);
    let expected = readj(&task.join("expect.json"));
    let intended = expected["intended"].as_array().unwrap();
    let valid = status != 2 && report["ok"] == true;
    let intended_present = intended.iter().all(|change| item(changes(&report), change));
    let forbidden_changed = expected["forbidden"].as_array().is_some_and(|items| items.iter().any(|item| forbidden(changes(&report), item)));
    let moved = intended.iter().any(|change| change["kind"] == "moved");
    let unintended = if valid {
        changes(&report).iter().filter(|change| {
            change["kind"] != "vocabulary"
                && !(moved && change["kind"] == "moved")
                && !intended.iter().any(|expected| item(std::slice::from_ref(change), expected))
        }).cloned().collect::<Vec<_>>()
    } else {
        vec![json!({"error":"reference refused","report":report})]
    };
    let preserved_ids = if valid { preserved(changes(&report), &expected["preserve"]) } else { vec!["reference refused".into()] };

    let mut trap_verdicts = Vec::new();
    let mut traps = fs::read_dir(&task).unwrap().flatten().map(|entry| entry.path())
        .filter(|path| path.file_name().unwrap().to_string_lossy().starts_with("trap-"))
        .collect::<Vec<_>>();
    traps.sort();
    for trap in traps {
        let mut value = readj(&trap);
        let trap_expect = value.as_object_mut().unwrap().remove("trapExpect").unwrap();
        fill(&mut value, &project, &files);
        let (trap_status, trap_report) = apply(model, value);
        let code_ok = trap_expect.get("code").is_some_and(|code| trap_report["code"] == *code && trap_status == 2);
        let specific = if let Some(preserve) = trap_expect.get("preserve").and_then(Value::as_str) {
            let node = trap_expect.get("node").and_then(Value::as_str).unwrap_or("");
            trap_report["changes"].as_array().is_some_and(|items| items.iter().any(|change| change["node"] == node && change["preserve"] == preserve))
        } else if let Some(diagnostic) = trap_expect.get("diagnostic").and_then(Value::as_str) {
            trap_report["diagnostics"].as_array().is_some_and(|items| items.iter().any(|item| item["code"] == diagnostic))
        } else if let Some(node) = trap_expect.get("node").and_then(Value::as_str) {
            trap_report["changes"].as_array().is_some_and(|items| items.iter().any(|change| change["node"] == node))
        } else { true };
        let diff_ok = trap_expect.get("diff").is_some_and(|expected_diff| trap_status == 1 && item(changes(&trap_report), expected_diff));
        let caught = specific && ((code_ok && trap_expect.get("diff").is_none()) || (diff_ok && trap_expect.get("code").is_none()) || (code_ok && diff_ok));
        trap_verdicts.push(json!({
            "name": trap.file_name().unwrap().to_string_lossy(),
            "caught": caught,
            "status": trap_status,
            "actualCode": trap_report["code"],
            "expected": trap_expect,
        }));
    }
    let traps_ok = trap_verdicts.iter().all(|verdict| verdict["caught"] == true);
    let ok = valid && intended_present && !forbidden_changed && unintended.is_empty() && preserved_ids.is_empty() && traps_ok;
    assert!(revision(model) == before, "dry-run changed {}", model.root().display());
    record(task_name, json!({
        "validity": valid,
        "intendedPresent": intended_present,
        "unintendedDiff": unintended,
        "preservedIdChanges": preserved_ids,
        "trapVerdicts": trap_verdicts,
        "ok": ok,
    }));
    assert!(ok, "edit-task {task_name} failed");
}

macro_rules! edit_task_test {
    ($name:ident, $task:literal) => {
        #[test]
        fn $name() { run_task($task); }
    };
}

edit_task_test!(edit_task_01_new_choice, "01-new-choice");
edit_task_test!(edit_task_02_delay_clue, "02-delay-clue");
edit_task_test!(edit_task_03_optional_quest, "03-optional-quest");
edit_task_test!(edit_task_04_event_period, "04-event-period");
edit_task_test!(edit_task_05_component_insert, "05-component-insert");
edit_task_test!(edit_task_06_host_result, "06-host-result");
edit_task_test!(edit_task_07_item_description, "07-item-description");
edit_task_test!(edit_task_08_merge_area, "08-merge-area");
edit_task_test!(edit_task_09_npc_death, "09-npc-death");
edit_task_test!(edit_task_10_job_restriction, "10-job-restriction");
edit_task_test!(edit_task_11_move_scene, "11-move-scene");
edit_task_test!(edit_task_12_schedule_action, "12-schedule-action");

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap().flatten() {
        let (source, target) = (entry.path(), to.join(entry.file_name()));
        if source.is_dir() { copy_tree(&source, &target) } else { fs::copy(&source, &target).unwrap(); }
    }
}

/// The suite above checks every task as a dry-run against one shared model.
/// This runs the reference patch for real through `lute patch` on a copy of
/// the base — staging, atomic commit, the post-write rebuild and the command's
/// exit code — and requires the written tree to be exactly what the dry-run
/// promised. Two bases keep it cheap: one dogfood game, one standalone root.
fn write_matches_dry_run(task_name: &str) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let task = root().join(task_name);
    let base_name = base_path(&task);
    let model = model(&base_name);
    let before = model.revisions();
    let files = before.files.iter().map(|(path, revision)| (path.to_string_lossy().into_owned(), revision.sha256.clone())).collect();
    let mut patch = readj(&task.join("patch.json"));
    fill(&mut patch, &before.sha256, &files);
    let (dry_status, dry) = apply(model, patch.clone());

    let serial = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let scratch = std::env::temp_dir().join(format!("lute-edit-write-{}-{serial}", std::process::id()));
    let copy = scratch.join("base");
    copy_tree(&workspace().join(&base_name), &copy);
    let patch_file = scratch.join("patch.json");
    fs::write(&patch_file, serde_json::to_vec(&patch).unwrap()).unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_lute"))
        .arg("patch").arg(&copy).arg(&patch_file).arg("--json")
        .output().unwrap();
    let written: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|e| panic!("{task_name}: {e}: {}", String::from_utf8_lossy(&output.stderr)));
    assert_eq!(output.status.code(), Some(dry_status), "{task_name}: {written}");
    assert_eq!(written["after"]["sha256"], dry["after"]["sha256"], "{task_name}: written tree differs from the dry-run");
    let kinds = |report: &Value| changes(report).iter().map(|c| (c["node"].clone(), c["kind"].clone())).collect::<Vec<_>>();
    assert_eq!(kinds(&written), kinds(&dry), "{task_name}");
    let rebuilt = ProjectModel::build_single_root(&copy, &Default::default()).unwrap();
    assert_eq!(Value::String(rebuilt.revisions().sha256.clone()), written["after"]["sha256"], "{task_name}: report does not describe the tree on disk");
    let _ = fs::remove_dir_all(scratch);
}

#[test]
fn edit_task_write_path_matches_dry_run_on_a_game() { write_matches_dry_run("01-new-choice"); }

#[test]
fn edit_task_write_path_matches_dry_run_standalone() { write_matches_dry_run("10-job-restriction"); }
