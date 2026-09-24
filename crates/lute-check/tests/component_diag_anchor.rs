//! 0.21.1 T3-7 — component diagnostics a writer can act on.
//!
//! * lamplight F4: a component-body diagnostic reached through a `::use` was
//!   anchored at the importing scene's 1:1 (the frontmatter), with the
//!   component's ABSOLUTE path in the message. It now lands on the `::use`
//!   that brings the body in, and names the component project-relative.
//! * seven F9: `{{memory}}` for a declared param `memory` said "reads ambient
//!   state … bind it through a param" — advice already followed. It now says
//!   to write `{{@memory}}`.
//! * lamplight F3: a line whose whole text is `@memory` ships the literal
//!   string "@memory". `W-TEXT-LOOKS-LIKE-REF` flags it when `memory` is a
//!   declared param (component) or def (scene).
//!
//! Harness: temp-dir component files resolved through `resolve_components`,
//! the resolver the CLI/LSP call (as `tests/component_line_code.rs`).
use lute_check::{
    check, parse_meta, resolve_components, CheckInput, ComponentSet, Mode, SchemaImports,
};
use lute_core_span::Diagnostic;
use lute_manifest::provider::ProviderSet;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_test_vocab::vocab_snapshot;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static UNIQ: AtomicU64 = AtomicU64::new(0);

fn unique_dir() -> PathBuf {
    let n = UNIQ.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("lute_cda_{}_{}_{}", std::process::id(), n, nanos));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn check_text(text: &str, components: ComponentSet) -> Vec<Diagnostic> {
    let input = CheckInput {
        text: text.to_string(),
        uri: "scene".into(),
        snapshot: vocab_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components,
        defaults: Default::default(),
    };
    check(&input).diagnostics
}

/// Check `scene` (a scene whose frontmatter imports components relative to
/// `scene_dir`) with its components resolved from disk.
fn check_scene(scene_dir: &Path, scene: &str) -> Vec<Diagnostic> {
    let (doc, _) = lute_syntax::parse(scene);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let components = resolve_components(scene_dir, &meta0.components, doc.meta.span);
    check_text(scene, components)
}

/// A project: `lute.project.yaml` at the root, the component under
/// `components/`, the scene under `scenes/` importing it by relative path.
/// The `::use` sits on scene line 10.
fn project_with_component(component_body: &str, params: &str) -> (PathBuf, String) {
    let root = unique_dir();
    std::fs::create_dir_all(root.join("components")).unwrap();
    std::fs::create_dir_all(root.join("scenes")).unwrap();
    std::fs::write(
        root.join("lute.project.yaml"),
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("components/c.component.lute"),
        format!("---\ncomponent: c\n{params}---\n## Scene 1.\n{component_body}"),
    )
    .unwrap();
    let scene = format!(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
         components: [../components/c.component.lute]\n---\n## Shot 1.\n\
         @narrator: before.\n::use{{component=\"c\"{}}}\n",
        if params.is_empty() { "" } else { " memory=\"hi\"" }
    );
    (root, scene)
}

#[test]
fn body_diagnostic_lands_on_the_use_with_a_project_relative_path() {
    let (root, scene) = project_with_component("@narrator: {{run.gold}}\n", "");
    let diags = check_scene(&root.join("scenes"), &scene);
    let d = diags
        .iter()
        .find(|d| d.code == "E-COMPONENT-STATE")
        .unwrap_or_else(|| panic!("the ambient read must report: {diags:#?}"));
    assert_eq!(
        d.span.line, 10,
        "anchored at the `::use` (line 10), not the frontmatter: {d:#?}"
    );
    assert!(
        d.message.contains("(components/c.component.lute)"),
        "names the component relative to the project root: {}",
        d.message
    );
    let canonical_root = std::fs::canonicalize(&root).unwrap();
    assert!(
        !d.message.contains(&*canonical_root.to_string_lossy()),
        "no absolute path in the message: {}",
        d.message
    );
    // The secondary location keeps the canonical identity other passes key on
    // (the CLI's standalone caller intersection matches on it).
    assert_eq!(
        Path::new(&d.related[0].file),
        std::fs::canonicalize(root.join("components/c.component.lute")).unwrap()
    );
}

#[test]
fn bare_param_interpolation_says_write_the_ref() {
    let (root, scene) =
        project_with_component("@narrator: {{memory}}\n", "params:\n  memory: string\n");
    let diags = check_scene(&root.join("scenes"), &scene);
    let d = diags
        .iter()
        .find(|d| d.code == "E-COMPONENT-STATE")
        .unwrap_or_else(|| panic!("`{{{{memory}}}}` is still not a param ref: {diags:#?}"));
    assert!(
        d.message.contains("write `{{@memory}}`"),
        "a declared param names the fix: {}",
        d.message
    );
    assert!(
        !d.message.contains("bind it through a param"),
        "not the ambient-state advice the author already followed: {}",
        d.message
    );
}

#[test]
fn line_text_exactly_a_declared_param_warns() {
    let (root, scene) =
        project_with_component("@narrator: @memory\n", "params:\n  memory: string\n");
    let diags = check_scene(&root.join("scenes"), &scene);
    assert!(
        diags.iter().any(|d| d.code == "W-TEXT-LOOKS-LIKE-REF"
            && d.severity == lute_core_span::Severity::Warning
            && d.message.contains("{{@memory}}")),
        "`@narrator: @memory` ships the literal \"@memory\": {diags:#?}"
    );
}

#[test]
fn line_text_exactly_a_declared_def_warns_and_an_unknown_name_does_not() {
    let scene = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
                 defs:\n  twice: { type: number, body: \"2 * 3\" }\n---\n## Shot 1.\n\
                 @narrator: @twice\n@narrator: @nobody\n@narrator: say @twice now\n";
    let diags = check_text(scene, Default::default());
    let hits: Vec<_> = diags
        .iter()
        .filter(|d| d.code == "W-TEXT-LOOKS-LIKE-REF")
        .collect();
    assert_eq!(
        hits.len(),
        1,
        "only the line that is exactly a declared def warns: {diags:#?}"
    );
    assert_eq!(hits[0].span.line, 10, "{:#?}", hits[0]);
}
