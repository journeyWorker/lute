use lute_manifest::project::{
    load_project, project_providers, resolve_document_snapshot, IdentityTemplates,
};
use std::collections::BTreeMap;
use std::fs;

fn write_project(root: &std::path::Path) {
    // plugin package
    let pdir = root.join("plugins/idola.minigame/directives");
    fs::create_dir_all(&pdir).unwrap();
    fs::write(
        root.join("plugins/idola.minigame/plugin.yaml"),
        "id: idola.minigame\nversion: 0.1.0\nkind: capability\nexports:\n  directives: directives/\n",
    )
    .unwrap();
    fs::write(
        pdir.join("d.yaml"),
        "directives:\n  - { name: minigame, attrs: [ { name: kind, type: string } ], lower: { kind: builtin, name: n } }\n",
    )
    .unwrap();
    // project config
    fs::write(
        root.join("lute.project.yaml"),
        "pluginsDir: plugins/\ndefaultProfile: date\nprofiles:\n  global:\n    plugins: { lute.core: true }\n  date:\n    plugins: { idola.minigame: true }\n",
    )
    .unwrap();
}

#[test]
fn resolves_project_snapshot_with_active_plugin() {
    let root = std::env::temp_dir().join(format!("lute_proj_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    write_project(&root);
    let proj = load_project(&root)
        .expect("valid project loads")
        .expect("present");
    let (snap, diags) = resolve_document_snapshot(Some(&proj), None, &BTreeMap::new());
    assert!(
        diags.is_empty(),
        "{:?}",
        diags.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
    assert!(
        snap.directive("minigame").is_some(),
        "active plugin directive present"
    );
    fs::remove_dir_all(&root).ok();
}

#[test]
fn no_project_is_core_only() {
    let (snap, diags) = resolve_document_snapshot(None, None, &BTreeMap::new());
    assert!(diags.is_empty());
    assert!(snap.directive("bg").is_some());
    assert!(snap.directive("minigame").is_none());
}

#[test]
fn missing_project_is_ok_none() {
    let dir = std::env::temp_dir().join(format!("lute_noproj_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    assert!(
        matches!(load_project(&dir), Ok(None)),
        "absent lute.project.yaml -> Ok(None)"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn malformed_project_is_err() {
    let dir = std::env::temp_dir().join(format!("lute_badproj_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("lute.project.yaml"),
        "profiles: [this is not a map\n  : : :",
    )
    .unwrap();
    assert!(
        load_project(&dir).is_err(),
        "malformed lute.project.yaml -> Err"
    );
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn resolve_diag_carries_stable_code() {
    // temp project with two mutually-depending plugins (a.x->a.dep->a.x DependsCycle)
    let root = std::env::temp_dir().join(format!("lute_rdcode_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (id, dep) in [("a.x", "a.dep"), ("a.dep", "a.x")] {
        let d = root.join(format!("plugins/{id}/directives"));
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(root.join(format!("plugins/{id}/plugin.yaml")),
            format!("id: {id}\nversion: 0.1.0\nkind: capability\ndepends: [ {{ id: {dep}, range: \"^0.1.0\" }} ]\nexports:\n  directives: directives/\n")).unwrap();
        std::fs::write(d.join("x.yaml"), "directives: []\n").unwrap();
    }
    std::fs::write(
        root.join("lute.project.yaml"),
        "defaultProfile: p\nprofiles:\n  p:\n    plugins: { a.x: true }\n",
    )
    .unwrap();
    let proj = lute_manifest::project::load_project(&root)
        .unwrap()
        .unwrap();
    let (_snap, diags) = lute_manifest::project::resolve_document_snapshot(
        Some(&proj),
        None,
        &std::collections::BTreeMap::new(),
    );
    assert!(
        diags.iter().any(|d| d.code == "E-DEPENDS-CYCLE"),
        "expected E-DEPENDS-CYCLE code, got {:?}",
        diags.iter().map(|d| &d.code).collect::<Vec<_>>()
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn project_providers_loads_catalog() {
    let proj = load_project(std::path::Path::new("../../docs/examples/idola-project"))
        .unwrap()
        .unwrap();
    let ps = project_providers(Some(&proj));
    assert_eq!(
        ps.contains("minigameId", "bianca_service_01"),
        lute_manifest::provider::IdStatus::Fresh,
        "project catalog resolves the minigame id"
    );
    assert_eq!(
        project_providers(None).contains("minigameId", "bianca_service_01"),
        lute_manifest::provider::IdStatus::Absent,
        "no project -> empty providers"
    );
}

/// Write a minimal (plugin-free) project whose `lute.project.yaml` carries the
/// given `identity:` YAML body, returning its root.
fn identity_project(tag: &str, identity_yaml: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("lute_ident_{}_{tag}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();
    fs::write(
        root.join("lute.project.yaml"),
        format!("defaultProfile: base\nprofiles:\n  base:\n    plugins: {{}}\n{identity_yaml}"),
    )
    .unwrap();
    root
}

/// A project with no `identity:` block resolves 0.7.0's hardcoded pair, so an
/// existing project compiles byte-identically (0.8.0 §9).
#[test]
fn absent_identity_block_defaults_to_070_templates() {
    let root = identity_project("absent", "");
    let proj = load_project(&root).unwrap().unwrap();
    assert_eq!(proj.identity, IdentityTemplates::default());
    assert_eq!(proj.identity.line_id, "{prefix}.{speaker}_{code}");
    assert_eq!(proj.identity.voice_key, "{speaker}-{code}");
    assert!(proj.identity_diags.is_empty());
    fs::remove_dir_all(&root).ok();
}

/// Each key is authored and resolved independently: retemplating `lineId`
/// alone leaves `voiceKey` at its default.
#[test]
fn authored_identity_templates_resolve_per_key() {
    let root = identity_project("authored", "identity:\n  lineId: \"{prefix}/{speaker}#{code}\"\n");
    let proj = load_project(&root).unwrap().unwrap();
    assert_eq!(proj.identity.line_id, "{prefix}/{speaker}#{code}");
    assert_eq!(proj.identity.voice_key, "{speaker}-{code}");
    assert!(proj.identity_diags.is_empty());
    assert_eq!(
        proj.identity.render_line_id("npc_koyuki_ep05", "koyuki", "0010"),
        "npc_koyuki_ep05/koyuki#0010"
    );
    fs::remove_dir_all(&root).ok();
}

/// An unknown `{token}` is `E-IDENTITY-TEMPLATE` at load. The project still
/// loads (its plugin graph is unaffected) with the offending key reset to its
/// default, and `resolve_document_snapshot` — the ONE resolver both the CLI
/// and the LSP call — replays the diagnostic, so neither surface can miss it.
#[test]
fn unknown_identity_token_is_reported_at_project_load() {
    let root = identity_project("unknown", "identity:\n  lineId: \"{prefix}.{foo}\"\n");
    let proj = load_project(&root).unwrap().unwrap();

    assert_eq!(proj.identity_diags.len(), 1, "{:?}", proj.identity_diags);
    assert_eq!(proj.identity_diags[0].code, "E-IDENTITY-TEMPLATE");
    assert_eq!(
        proj.identity_diags[0].message,
        "unknown token `{foo}` in identity template `lineId`; \
         valid tokens are {prefix}, {speaker}, {code}"
    );
    // Fail closed: the rejected key falls back to 0.7.0's shape.
    assert_eq!(proj.identity.line_id, "{prefix}.{speaker}_{code}");

    let (_snap, diags) = resolve_document_snapshot(Some(&proj), None, &BTreeMap::new());
    assert!(
        diags.iter().any(|d| d.code == "E-IDENTITY-TEMPLATE"),
        "the shared resolver replays it: {:?}",
        diags.iter().map(|d| &d.code).collect::<Vec<_>>()
    );
    fs::remove_dir_all(&root).ok();
}

/// A template that resolves to an empty string is rejected too — an empty
/// `lineId` would silently un-key every line.
#[test]
fn empty_identity_template_is_reported_at_project_load() {
    let root = identity_project("empty", "identity:\n  voiceKey: \"\"\n");
    let proj = load_project(&root).unwrap().unwrap();
    assert_eq!(proj.identity_diags.len(), 1, "{:?}", proj.identity_diags);
    assert_eq!(
        proj.identity_diags[0].message,
        "identity template `voiceKey` resolves to an empty string"
    );
    assert_eq!(proj.identity.voice_key, "{speaker}-{code}");
    fs::remove_dir_all(&root).ok();
}

/// The renderer is total: repeated tokens substitute every time, an
/// unterminated `{` is literal text, and multi-byte literals around a token
/// never split a char boundary. None of these panic.
#[test]
fn identity_renderer_is_total() {
    use lute_manifest::project::render_identity_template;

    assert_eq!(
        render_identity_template("{speaker}/{speaker}", "p", "koyuki", "0010"),
        "koyuki/koyuki"
    );
    assert_eq!(
        render_identity_template("日本{speaker}語{code}", "p", "koyuki", "0010"),
        "日本koyuki語0010"
    );
    // Unterminated `{` -> literal tail, nothing swallowed.
    assert_eq!(
        render_identity_template("{prefix}.{speaker", "ep05", "koyuki", "0010"),
        "ep05.{speaker"
    );
    // An empty substitution is still a total render (an empty `code` is a
    // back-fill failure the caller already handled by skipping the line).
    assert_eq!(render_identity_template("{speaker}-{code}", "p", "koyuki", ""), "koyuki-");
    assert_eq!(render_identity_template("", "p", "s", "c"), "");
    // No tokens at all -> the literal, unchanged.
    assert_eq!(render_identity_template("fixed", "p", "s", "c"), "fixed");
}
