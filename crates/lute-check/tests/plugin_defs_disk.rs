//! Disk-path coverage: a plugin `defs` export flows load_plugins_dir ->
//! assemble_snapshot -> snapshot.defs -> check(), so a plugin-exported `@ref`
//! is a declared def AND type-checks (dsl §8). Guards the end-to-end path that
//! ref_type.rs only exercises via a synthetic in-memory snapshot.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_manifest::project::{load_project, project_providers, resolve_document_snapshot};

static N: AtomicU32 = AtomicU32::new(0);

fn unique_dir() -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "lute-plugindefs-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn write(path: &Path, body: &str) {
    if let Some(p) = path.parent() {
        std::fs::create_dir_all(p).unwrap();
    }
    std::fs::write(path, body).unwrap();
}

/// Build a temp project whose one plugin exports a bool def `warm` and a number
/// def `tally`, then resolve+check `scene` (its text) sitting in the project root.
fn codes_for_scene(scene: &str) -> Vec<String> {
    let root = unique_dir();
    write(
        &root.join("lute.project.yaml"),
        "pluginsDir: plugins/\ndefaultProfile: demo\nprofiles:\n  demo:\n    plugins: { demo.defs: true }\n",
    );
    write(
        &root.join("plugins/demo.defs/plugin.yaml"),
        "id: demo.defs\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\nexports:\n  defs: defs/\n",
    );
    write(
        &root.join("plugins/demo.defs/defs/defs.yaml"),
        "defs:\n  - { name: warm, type: bool, cel: \"true\" }\n  - { name: tally, type: number, cel: \"1\" }\n",
    );
    let scene_path = root.join("scene.lute");
    write(&scene_path, scene);

    // Mirror crates/lute-cli/src/main.rs:116-160. `load_project` yields an
    // `Option<ProjectConfig>`; both resolve helpers take `Option<&ProjectConfig>`.
    let project = load_project(&root).unwrap();
    // The scene declares no `profile:`/`plugins:` inline -> default profile.
    let (snapshot, _rdiags) = resolve_document_snapshot(project.as_ref(), None, &BTreeMap::new());
    let providers = project_providers(project.as_ref());
    let input = CheckInput {
        text: scene.to_string(),
        uri: scene_path.display().to_string(),
        snapshot,
        providers,
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
    };
    check(&input)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

const HDR: &str = "---\nkind: scene\ncharacter: demo\nseason: 1\nepisode: 1\nstate:\n  scene.flag: { type: bool, default: false }\n---\n## Shot 1.\n";

#[test]
fn plugin_bool_def_from_disk_is_declared_and_clean() {
    let scene = format!(
        "{HDR}<match on=\"scene.flag\">\n<when test=\"@warm\">:narrator: a\n</when>\n<otherwise>:narrator: b\n</otherwise>\n</match>\n"
    );
    let codes = codes_for_scene(&scene);
    assert!(
        !codes.contains(&"E-UNDECLARED-REF".to_string()),
        "plugin def must be declared from disk; got {codes:?}"
    );
    assert!(
        !codes.contains(&"E-REF-TYPE".to_string()),
        "bool def in bool guard is compatible; got {codes:?}"
    );
}

#[test]
fn plugin_number_def_from_disk_flags_ref_type() {
    let scene = format!(
        "{HDR}<match on=\"scene.flag\">\n<when test=\"@tally\">:narrator: a\n</when>\n<otherwise>:narrator: b\n</otherwise>\n</match>\n"
    );
    let codes = codes_for_scene(&scene);
    assert!(
        !codes.contains(&"E-UNDECLARED-REF".to_string()),
        "got {codes:?}"
    );
    assert!(
        codes.contains(&"E-REF-TYPE".to_string()),
        "number def in bool guard must flag E-REF-TYPE from disk; got {codes:?}"
    );
}
