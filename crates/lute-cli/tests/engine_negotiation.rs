use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU32, Ordering};

use serde_json::Value as Json;

const BIN: &str = env!("CARGO_BIN_EXE_lute");
const ALL_IDS: &[&str] = &[
    "lute.core/1",
    "lute.staging/1",
    "lute.timeline/1",
    "lute.quest.lifecycle/1",
    "lute.quest.rewards/1",
    "lute.time.clock/1",
    "lute.time.cadence/1",
    "lute.time.seasons/1",
    "lute.occasions.selection/1",
    "lute.occasions.gates/1",
    "lute.knowledge.facts/1",
    "lute.knowledge.rules/1",
    "lute.knowledge.temporal/1",
    "lute.lore/1",
];

fn temp_dir(tag: &str) -> PathBuf {
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-engine-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, rel: &str, text: &str) -> PathBuf {
    let path = dir.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&path, text).unwrap();
    path
}

fn run(args: &[String]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn run_str(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn scene_source() -> &'static str {
    "---\nkind: scene\nluteVersion: \"0.8.0\"\ncharacter: hero\nseason: 1\nepisode: 1\ntitle: T\n---\n\n## Shot 1.\n\n::bg{location=\"room\" time=\"day\"}\n@narrator{code=\"0010\"}: Presented.\n"
}

fn compile_scene(tag: &str) -> (PathBuf, Json) {
    let dir = temp_dir(tag);
    let source = write(&dir, "scene.lute", scene_source());
    let artifact = dir.join("scene.json");
    let out = run(&[
        "compile".into(),
        source.display().to_string(),
        "-o".into(),
        artifact.display().to_string(),
    ]);
    assert!(out.status.success(), "compile failed: {}", String::from_utf8_lossy(&out.stderr));
    let json: Json = serde_json::from_str(&std::fs::read_to_string(&artifact).unwrap()).unwrap();
    (artifact, json)
}

fn ir_version(artifact: &Json) -> &str {
    artifact["irVersion"].as_str().expect("compiled artifact irVersion")
}

fn matrix_yaml(engine: &str, ir: &str, ids: &[&str], extra: &str) -> String {
    let mut out = format!("engine: {engine}\nirVersion: \"{ir}\"\nsupportedIds:\n");
    for id in ids {
        out.push_str(&format!("  - {id}\n"));
    }
    out.push_str(extra);
    out
}

fn matrix_file(dir: &Path, name: &str, engine: &str, ir: &str, ids: &[&str], extra: &str) -> PathBuf {
    write(dir, name, &matrix_yaml(engine, ir, ids, extra))
}

fn required_ids(artifact: &Json) -> Vec<&str> {
    artifact["requiredSemantics"]
        .as_array()
        .expect("requiredSemantics array")
        .iter()
        .map(|id| id.as_str().unwrap())
        .collect()
}

#[test]
fn run_engine_refuses_before_playback_and_accepts_all_required_ids() {
    let (artifact, json) = compile_scene("run-semantics");
    let dir = artifact.parent().unwrap();
    let ir = ir_version(&json);
    let missing = matrix_file(dir, "missing.yaml", "text-engine", ir, &["lute.core/1"], "");
    let failed = run_str(&[
        "run",
        artifact.to_str().unwrap(),
        "--engine",
        missing.to_str().unwrap(),
    ]);
    assert_eq!(failed.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&failed.stderr);
    assert!(stderr.contains("E-ENGINE-SEMANTICS"), "{stderr}");
    assert!(stderr.contains("lute.staging/1"), "{stderr}");
    assert!(stderr.contains("text-engine"), "{stderr}");
    assert!(failed.stdout.is_empty(), "refused playback must not emit commands");

    let supported = required_ids(&json);
    let accepted = matrix_file(dir, "all.yaml", "text-engine", ir, &supported, "");
    let played = run(&[
        "run".into(),
        artifact.display().to_string(),
        "--engine".into(),
        accepted.display().to_string(),
        "--json".into(),
    ]);
    assert!(played.status.success(), "{}", String::from_utf8_lossy(&played.stderr));
    let transcript: Json = serde_json::from_slice(&played.stdout).unwrap();
    assert!(transcript["commands"].as_array().unwrap().iter().any(|c| c["kind"] == "line"));
}

#[test]
fn engine_ir_version_accepts_patch_but_rejects_different_minor() {
    let (artifact, json) = compile_scene("ir-version");
    let dir = artifact.parent().unwrap();
    let ir = ir_version(&json);
    let parts: Vec<&str> = ir.split('.').collect();
    let major: u64 = parts[0].parse().unwrap();
    let minor: u64 = parts[1].parse().unwrap();
    let patch_ir = format!("{major}.{minor}.99");
    let different_minor = format!("{major}.{}.0", minor + 1);
    let ids = required_ids(&json);
    let patch = matrix_file(dir, "patch.yaml", "patch-engine", &patch_ir, &ids, "");
    let accepted = run_str(&["run", artifact.to_str().unwrap(), "--engine", patch.to_str().unwrap()]);
    assert!(accepted.status.success(), "{}", String::from_utf8_lossy(&accepted.stderr));

    let minor_file = matrix_file(dir, "minor.yaml", "minor-engine", &different_minor, &ids, "");
    let rejected = run_str(&["run", artifact.to_str().unwrap(), "--engine", minor_file.to_str().unwrap()]);
    assert_eq!(rejected.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&rejected.stderr);
    assert!(stderr.contains("E-ENGINE-IR-VERSION"), "{stderr}");
    assert!(stderr.contains("minor-engine"), "{stderr}");
    for bad_artifact_version in ["0.33", "0.33.x", "0.33.0.1"] {
        let bad_artifact = dir.join(format!("artifact-{bad_artifact_version}.json"));
        let mut tampered = json.clone();
        tampered["irVersion"] = bad_artifact_version.into();
        std::fs::write(&bad_artifact, serde_json::to_vec(&tampered).unwrap()).unwrap();
        let out = run_str(&["run", bad_artifact.to_str().unwrap(), "--engine", patch.to_str().unwrap()]);
        assert_eq!(out.status.code(), Some(2), "{bad_artifact_version}: {}", String::from_utf8_lossy(&out.stderr));
        assert!(String::from_utf8_lossy(&out.stderr).contains("E-ENGINE-IR-VERSION"));
    }
}


#[test]
fn malformed_engine_matrices_exit_two() {
    let (artifact, json) = compile_scene("matrix-errors");
    let dir = artifact.parent().unwrap();
    let ir = ir_version(&json);
    let cases = [
        ("missing-engine.yaml", format!("irVersion: \"{ir}\"\nsupportedIds: [lute.core/1]\n"), "E-ENGINE-MATRIX"),
        ("duplicate.yaml", matrix_yaml("dup", ir, &["lute.core/1", "lute.core/1"], ""), "E-ENGINE-MATRIX"),
        ("malformed.yaml", matrix_yaml("bad-id", ir, &["not-an-id"], ""), "E-ENGINE-MATRIX"),
        ("bad-version.yaml", matrix_yaml("bad-version", "0.33", &["lute.core/1"], ""), "E-ENGINE-MATRIX"),
        ("unknown.yaml", matrix_yaml("unknown-id", ir, &["lute.nope.feature/1"], ""), "E-ENGINE-MATRIX"),
    ];
    for (name, yaml, code) in cases {
        let file = write(dir, name, &yaml);
        let out = run_str(&["run", artifact.to_str().unwrap(), "--engine", file.to_str().unwrap()]);
        assert_eq!(out.status.code(), Some(2), "{name}: {}", String::from_utf8_lossy(&out.stderr));
        assert!(String::from_utf8_lossy(&out.stderr).contains(code), "{name}: {}", String::from_utf8_lossy(&out.stderr));
    }

    let extra = matrix_file(dir, "extra.yaml", "extra-fields", ir, &["lute.core/1", "lute.staging/1"], "ignoredField: true\n");
    let out = run_str(&["run", artifact.to_str().unwrap(), "--engine", extra.to_str().unwrap()]);
    assert!(out.status.success(), "unknown YAML fields are ignored: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn check_engine_reports_staging_at_the_directive_line() {
    let (artifact, json) = compile_scene("check-span");
    let source = artifact.parent().unwrap().join("scene.lute");
    let engine = matrix_file(artifact.parent().unwrap(), "core.yaml", "checker", ir_version(&json), &["lute.core/1"], "");
    let out = run(&[
        "check".into(),
        source.display().to_string(),
        "--engine".into(),
        engine.display().to_string(),
        "--json".into(),
    ]);
    assert_eq!(out.status.code(), Some(1), "{}", String::from_utf8_lossy(&out.stderr));
    let report: Json = serde_json::from_slice(&out.stdout).unwrap();
    let diag = report["diagnostics"].as_array().unwrap().iter().find(|d| d["code"] == "E-CHECK-ENGINE-SEMANTICS").expect("engine semantic diagnostic");
    assert_eq!(diag["severity"], "error");
    assert_eq!(diag["span"]["line"], 12, "::bg is source line 12: {report}");
    assert!(diag["message"].as_str().unwrap().contains("lute.staging/1"));
}

#[test]
fn check_project_reports_each_contributing_document_and_clean_supports_all() {
    let dir = temp_dir("check-project");
    let a = write(&dir, "a.lute", scene_source());
    let b = write(&dir, "nested/b.lute", &scene_source().replace("character: hero", "character: villain"));
    let (_, artifact) = compile_scene("check-project-ir");
    let ir = ir_version(&artifact);
    let core = matrix_file(&dir, "core.yaml", "project-checker", ir, &["lute.core/1"], "");
    let out = run_str(&["check-project", dir.to_str().unwrap(), "--engine", core.to_str().unwrap(), "--json"]);
    assert_eq!(out.status.code(), Some(1), "{}", String::from_utf8_lossy(&out.stderr));
    let report: Json = serde_json::from_slice(&out.stdout).unwrap();
    let findings: Vec<&Json> = report["files"].as_array().unwrap().iter().flat_map(|f| f["diagnostics"].as_array().unwrap()).filter(|d| d["code"] == "E-CHECK-ENGINE-SEMANTICS").collect();
    assert_eq!(findings.len(), 2, "{report}");
    assert!(findings.iter().all(|d| d["span"]["line"] == 12));
    assert!(report["files"].as_array().unwrap().iter().any(|f| f["path"].as_str().unwrap().ends_with("a.lute")));
    assert!(report["files"].as_array().unwrap().iter().any(|f| f["path"].as_str().unwrap().ends_with("b.lute")));

    let all = matrix_file(&dir, "all.yaml", "project-reference", ir, ALL_IDS, "");
    let clean = run_str(&["check-project", dir.to_str().unwrap(), "--engine", all.to_str().unwrap(), "--json"]);
    assert_eq!(clean.status.code(), Some(0), "{}", String::from_utf8_lossy(&clean.stderr));
    let clean_report: Json = serde_json::from_slice(&clean.stdout).unwrap();
    assert_eq!(clean_report["ok"], true, "{clean_report}");
    assert!(!clean_report["files"].as_array().unwrap().iter().flat_map(|f| f["diagnostics"].as_array().unwrap()).any(|d| d["code"] == "E-CHECK-ENGINE-SEMANTICS"));
    let _ = (a, b);
}


#[test]
fn check_engine_anchors_frontmatter_semantics_at_the_declaring_key() {
    let dir = temp_dir("check-frontmatter-span");
    let source_text = scene_source().replace(
        "title: T\n",
        "title: T\nentities:\n  person: { members: [hero] }\nrelations:\n  knows: { args: [person], tier: run }\n",
    );
    let source = write(&dir, "facts.lute", &source_text);
    let artifact = dir.join("facts.json");
    let compiled = run(&[
        "compile".into(),
        source.display().to_string(),
        "-o".into(),
        artifact.display().to_string(),
    ]);
    assert!(compiled.status.success(), "{}", String::from_utf8_lossy(&compiled.stderr));
    let json: Json = serde_json::from_str(&std::fs::read_to_string(&artifact).unwrap()).unwrap();
    let engine = matrix_file(&dir, "core.yaml", "checker", ir_version(&json), &["lute.core/1"], "");
    let out = run(&[
        "check".into(),
        source.display().to_string(),
        "--engine".into(),
        engine.display().to_string(),
        "--json".into(),
    ]);
    assert_eq!(out.status.code(), Some(1), "{}", String::from_utf8_lossy(&out.stderr));
    let report: Json = serde_json::from_slice(&out.stdout).unwrap();
    let diag = report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "E-CHECK-ENGINE-SEMANTICS" && d["message"].as_str().unwrap().contains("lute.knowledge.facts/1"))
        .expect("facts engine diagnostic");
    assert_eq!(diag["span"]["line"], 10, "relations: is source line 10: {report}");
}
#[test]
fn built_in_reference_plays_a_staging_corpus_game() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/command-staging/source.lute");
    let dir = temp_dir("reference-corpus");
    let artifact = dir.join("artifact.json");
    let out = run(&["compile".into(), source.display().to_string(), "-o".into(), artifact.display().to_string()]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let played = run(&["run".into(), artifact.display().to_string(), "--json".into()]);
    assert!(played.status.success(), "{}", String::from_utf8_lossy(&played.stderr));
    let transcript: Json = serde_json::from_slice(&played.stdout).unwrap();
    assert!(transcript["commands"].as_array().unwrap().iter().any(|c| c["kind"] == "line"));
}

#[test]
fn play_engine_refuses_staging_before_presenting_and_full_matrix_plays() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/play-hub");
    let dir = temp_dir("play-project");
    copy_tree(&fixture, &dir);
    let scene = dir.join("scenes/hub/staging.lute");
    write(&dir, "scenes/hub/staging.lute", "---\nkind: scene\nid: hub.staging\nuses: ../../world.schema.yaml\non: hubVisit\npriority: 30\n---\n\n## Staged\n\n::bg{location=\"room\" time=\"day\"}\n@narrator: Staged line.\n");
    let script = write(&dir, "staging.play.yaml", "steps:\n  - occasion: hubVisit\n");
    let ir = compile_scene("play-ir").1["irVersion"].as_str().unwrap().to_string();
    let matrix = matrix_file(&dir, "core.yaml", "play-checker", &ir, &["lute.core/1"], "");
    let failed = run_str(&["play", dir.to_str().unwrap(), "--script", script.to_str().unwrap(), "--engine", matrix.to_str().unwrap()]);
    assert_eq!(failed.status.code(), Some(2), "{}", String::from_utf8_lossy(&failed.stderr));
    assert!(String::from_utf8_lossy(&failed.stderr).contains("E-ENGINE-SEMANTICS"));
    assert!(!String::from_utf8_lossy(&failed.stdout).contains("Staged line"), "refusal must happen before presentation");

    let all = matrix_file(&dir, "all.yaml", "play-reference", &ir, ALL_IDS, "");
    let accepted = run_str(&["play", dir.to_str().unwrap(), "--script", script.to_str().unwrap(), "--engine", all.to_str().unwrap()]);
    assert!(accepted.status.success(), "{}", String::from_utf8_lossy(&accepted.stderr));
    assert!(String::from_utf8_lossy(&accepted.stdout).contains("Staged line"));
    let _ = scene;
}

fn copy_tree(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_tree(&from, &to);
        } else {
            std::fs::copy(from, to).unwrap();
        }
    }
}
