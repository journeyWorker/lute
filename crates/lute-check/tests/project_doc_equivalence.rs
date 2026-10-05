//! Project passes read a document's frontmatter through [`ProjectDoc`]'s
//! typed metadata. Identity reads must still see what the AUTHOR wrote
//! (`TypedMeta::yaml`), never the defaults-overlaid typed fields — the
//! readings these tests pin are the ones the pre-`ProjectDoc` raw parse gave.

use std::path::{Path, PathBuf};

use lute_check::connectivity::{quest_id_set, resolve_nodes, scene_key, scene_key_set};
use lute_check::meta::{parse_meta_kind_with_defaults, MetaKind, TypedMeta};
use lute_check::{ProjectDoc, ProjectDocs};
use lute_manifest::project::MetaDefaults;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_syntax::ast::Document;

fn parse(text: &str) -> Document {
    lute_syntax::parse(text).0
}

/// The typed metadata a project with manifest `defaults:` gives `doc`.
fn typed_with_defaults(doc: &Document, defaults: &MetaDefaults) -> TypedMeta {
    parse_meta_kind_with_defaults(&doc.meta, &CapabilitySnapshot::default(), MetaKind::Scene, defaults).0
}

#[test]
fn manifest_defaults_never_mint_a_scene_key() {
    // The scene authors no `id:` and no episode triad; the manifest default
    // supplies `character`/`season`/`episode`. The canonical key is derived
    // from authored keys only, so the scene has none.
    let defaults: MetaDefaults = [
        ("character".to_string(), serde_yaml::Value::from("mira")),
        ("season".to_string(), serde_yaml::Value::from(1)),
        ("episode".to_string(), serde_yaml::Value::from(2)),
    ]
    .into_iter()
    .collect();
    let doc = parse("---\nkind: scene\n---\n## Shot 1.\n@narrator: hi\n");
    let typed = typed_with_defaults(&doc, &defaults);
    assert_eq!(scene_key(&typed), None);
    let views = [ProjectDoc::new(Path::new("a.lute"), &doc, &typed)];
    assert!(scene_key_set(&views).is_empty(), "{:?}", scene_key_set(&views));
}

#[test]
fn a_malformed_authored_id_gives_no_scene_key() {
    let doc = parse("---\nkind: scene\nid: \"not a name!\"\n---\n## Shot 1.\n@narrator: hi\n");
    let typed = typed_with_defaults(&doc, &MetaDefaults::default());
    assert_eq!(scene_key(&typed), None);
    let views = [ProjectDoc::new(Path::new("a.lute"), &doc, &typed)];
    assert!(scene_key_set(&views).is_empty());
}

#[test]
fn a_quest_document_id_is_not_a_scene_key() {
    let doc = parse("---\nkind: quest\nid: errands\n---\n<quest id=\"q\">\n<objective id=\"o\" done=\"true\"/>\n</quest>\n");
    let docs = ProjectDocs::parse(vec![(PathBuf::from("q.lute"), doc)], &CapabilitySnapshot::default());
    let views = docs.views();
    assert_eq!(scene_key(views[0].meta), None);
    assert!(scene_key_set(&views).is_empty());
}

#[test]
fn a_duplicate_block_frontmatter_keeps_reference_checks_on() {
    // A same-block duplicate `relations:` child (its own `E-RELATION-DUP`)
    // parses on the sanitized retry, so the project's key set stays complete
    // and an unknown `visited(...)` in another document is still reported.
    let dup = "---\nkind: scene\ncharacter: b\nseason: 1\nepisode: 1\nrelations:\n  knows: {args: [a]}\n  knows: {args: [a]}\n---\n## Shot 1.\n@b: hi\n";
    let scene = "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\nafter: 'visited(\"nope.s99ep99\")'\n---\n## Shot 1.\n@a: hi\n";
    let docs = ProjectDocs::parse(
        vec![(PathBuf::from("b.lute"), parse(dup)), (PathBuf::from("a.lute"), parse(scene))],
        &CapabilitySnapshot::default(),
    );
    let views = docs.views();
    let res = resolve_nodes(&views, &scene_key_set(&views), &quest_id_set(&views));
    assert!(res.iter().any(|(_, d)| d.code == "E-CONN-UNKNOWN-NODE"), "{res:?}");
}
