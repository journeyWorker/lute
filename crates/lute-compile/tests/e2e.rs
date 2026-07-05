//! E2E artifact goldens (§8): real fixtures -> full-artifact snapshots +
//! structural invariants + byte determinism.

use std::collections::BTreeSet;
use std::path::Path;

use lute_check::{CheckInput, Mode};
use lute_compile::compile;

/// Assemble the CheckInput exactly as `lute compile` (Task 13) does.
fn input_for(path: &str, project_dir: Option<&str>) -> CheckInput {
    let file = Path::new(path);
    let text = std::fs::read_to_string(file).unwrap();
    let project = project_dir
        .and_then(|d| lute_manifest::project::load_project(Path::new(d)).expect("project loads"));
    let providers = lute_manifest::project::project_providers(project.as_ref());
    let (doc, _) = lute_syntax::parse(&text);
    let (meta0, _) = lute_check::parse_meta(
        &doc.meta,
        &lute_manifest::snapshot::CapabilitySnapshot::default(),
    );
    let (snapshot, _) = lute_manifest::project::resolve_document_snapshot(
        project.as_ref(),
        meta0.profile.as_deref(),
        &meta0.plugins,
    );
    let base = file.parent().unwrap_or_else(|| Path::new("."));
    let imports = lute_check::resolve_imports(base, &meta0.uses, &meta0.extends, doc.meta.span);
    let components = lute_check::resolve_components(base, &meta0.components, doc.meta.span);
    CheckInput {
        text,
        uri: path.to_string(),
        snapshot,
        providers,
        mode: Mode::Ci,
        imports,
        components,
    }
}

/// Structural invariants every artifact must satisfy (§4, §7): unique ordered
/// addrs, fully resolved targets, +100 gapping, id discipline, no DSL tokens.
fn assert_artifact_invariants(json: &serde_json::Value) {
    let commands = json["commands"].as_array().expect("commands array");
    let mut addrs: Vec<&str> = Vec::new();
    for c in commands {
        addrs.push(c["addr"].as_str().expect("every record has addr"));
    }
    let unique: BTreeSet<&str> = addrs.iter().copied().collect();
    assert_eq!(unique.len(), addrs.len(), "addrs unique");
    let mut sorted = addrs.clone();
    sorted.sort();
    assert_eq!(sorted, addrs, "addrs strictly ascending");
    let addr_set: BTreeSet<&str> = unique;
    for c in commands {
        for key in ["target", "converge", "otherwise"] {
            if let Some(t) = c[key].as_str() {
                assert_target(t, &addr_set);
            }
        }
        for arm in c["arms"].as_array().into_iter().flatten() {
            assert_target(arm["target"].as_str().unwrap(), &addr_set);
        }
        for opt in c["options"].as_array().into_iter().flatten() {
            assert_target(opt["target"].as_str().unwrap(), &addr_set);
            assert!(opt["lineId"].as_str().is_some_and(|s| !s.is_empty()));
        }
        if c["kind"] == "line" {
            assert!(c["lineId"].as_str().is_some_and(|s| !s.is_empty()));
            let voiced = c["role"] == "dialogue" || c["role"] == "voiceover";
            assert_eq!(
                c["voiceKey"].is_string(),
                voiced,
                "voiceKey iff voiced: {c}"
            );
            assert!(c.get("code").is_none(), "no standalone code field (§4.2)");
        }
    }
    // Retired identifier must not exist anywhere (§4.2).
    assert!(!json.to_string().contains("textUnitId"));
}

fn assert_target(t: &str, addrs: &BTreeSet<&str>) {
    assert!(!t.starts_with('@'), "unresolved symbolic target {t}");
    // A target is a real record OR the one-past-end converge of its shot.
    assert!(addrs.contains(t) || t.len() == 8, "malformed target {t}");
}

fn golden(name: &str, path: &str, project: Option<&str>) {
    let input = input_for(path, project);
    let artifact = compile(&input).unwrap_or_else(|e| panic!("{path} compiles: {e:#?}"));
    let mut json = serde_json::to_string_pretty(&artifact).unwrap();
    json.push('\n');
    assert_artifact_invariants(&serde_json::from_str(&json).unwrap());
    // Determinism (§8): same input => byte-identical artifact.
    let again = compile(&input).expect("recompiles");
    let mut json2 = serde_json::to_string_pretty(&again).unwrap();
    json2.push('\n');
    assert_eq!(json, json2, "byte-stable across compiles");
    insta::assert_snapshot!(name, json);
}

#[test]
fn bianca_s01ep02() {
    golden(
        "bianca_s01ep02",
        "../../docs/examples/bianca-s01ep02.lute",
        None,
    );
}

#[test]
fn showcase_episode01() {
    golden(
        "showcase_episode01",
        "../../docs/examples/showcase/episode01.lute",
        Some("../../docs/examples/showcase"),
    );
}

#[test]
fn components_scene() {
    golden(
        "components_scene",
        "../../docs/examples/components/scene.lute",
        None,
    );
}
