use lute_manifest::project::{load_project, resolve_document_snapshot};
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
