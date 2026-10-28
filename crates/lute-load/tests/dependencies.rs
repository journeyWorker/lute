//! The dependency record of a tracked [`InputCache`] (spec 0.53.0 §3.5): one
//! document of a generated project, and every lookup, listing and file its
//! assembly observed.

use std::path::Path;

use lute_check::Mode;
use lute_load::cache::Dependency;
use lute_load::{assemble_input_with_mode, InputCache};
use lute_manifest::deps::{Content, ListFilter, Outcome};

fn write(path: &Path, text: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, text).unwrap();
}

#[test]
fn a_tracked_cache_records_every_lookup_listing_and_file() {
    let dir = lute_scratch::ScratchDir::named("deps-record");
    let root = std::fs::canonicalize(&*dir).unwrap();
    write(
        &root.join("lute.project.yaml"),
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: { t.plug: {} }\n\
         defaults:\n  uses: \"schema/*.schema.yaml\"\n",
    );
    write(
        &root.join("schema/world.schema.yaml"),
        "state:\n  user.n: { type: int, default: 0 }\n",
    );
    write(
        &root.join("catalog/cast.yaml"),
        "manifestVersion: test\nproviderVersion: \"1\"\nstale: false\nentries:\n  speakerId: [hero]\n",
    );
    write(
        &root.join("plugins/t.plug/plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  directives: directives/\n",
    );
    write(
        &root.join("plugins/t.plug/directives/a.yaml"),
        "directives:\n  - { name: foo, attrs: [] }\n",
    );
    // `a` takes the manifest's `defaults: uses:`; `b` names a missing import.
    let a = root.join("scenes/a.lute");
    let a_text = "---\nkind: scene\nid: scene.a\n---\n\n## A\n\n@narrator: Hi.\n";
    write(&a, a_text);
    let b = root.join("scenes/b.lute");
    let b_text = "---\nkind: scene\nid: scene.b\nuses: [../schema/missing.yaml]\n---\n\n## B\n\n@narrator: Hi.\n";
    write(&b, b_text);

    let cache = InputCache::tracked();
    let assemble = |file: &Path, text: &str| {
        assemble_input_with_mode(
            &cache,
            file,
            text.to_string(),
            None,
            Some(&root),
            None,
            Mode::Author,
        )
        .0
    };
    assemble(&a, a_text);
    let built = assemble(&b, b_text);
    assert!(
        built
            .input
            .import_diags
            .iter()
            .any(|d| d.code == "E-USES-NOT-FOUND"),
        "the missing import is reported"
    );
    let deps = cache.dependencies();
    let lookup = |path: &Path| {
        deps.iter().find_map(|d| match d {
            Dependency::Lookup { path: p, outcome } if p == path => Some(outcome.clone()),
            _ => None,
        })
    };
    let listing = |dir: &Path, filter: &ListFilter| {
        deps.iter().find_map(|d| match d {
            Dependency::Listing {
                dir: d,
                filter: f,
                entries,
            } if d == dir && f == filter => Some(entries.clone()),
            _ => None,
        })
    };
    let file = |path: &Path| {
        deps.iter().any(|d| {
            matches!(d, Dependency::File { path: p, content: Content::Read { .. } } if p == path)
        })
    };

    // The missing `uses:` entry: a lookup whose outcome is `Missing`.
    assert_eq!(
        lookup(&root.join("scenes/../schema/missing.yaml")),
        Some(Outcome::Missing)
    );
    // The manifest: its lookup and its bytes.
    assert!(matches!(
        lookup(&root.join("lute.project.yaml")),
        Some(Outcome::Resolved { .. })
    ));
    assert!(file(&root.join("lute.project.yaml")));
    // The `defaults: uses:` glob: the listing of `schema/` and the schema.
    assert_eq!(
        listing(
            &root.join("schema"),
            &ListFilter::GlobSegment("*.schema.yaml".into())
        ),
        Some(Some(vec!["world.schema.yaml".to_string()]))
    );
    assert!(file(&root.join("schema/world.schema.yaml")));
    // The catalog listing and its file.
    assert_eq!(
        listing(&root.join("catalog/"), &ListFilter::Yaml),
        Some(Some(vec!["cast.yaml".to_string()]))
    );
    assert!(file(&root.join("catalog/cast.yaml")));
    // The plugins listing, the package manifest, its export listing and file.
    assert_eq!(
        listing(&root.join("plugins/"), &ListFilter::Dirs),
        Some(Some(vec!["t.plug/".to_string()]))
    );
    assert!(file(&root.join("plugins/t.plug/plugin.yaml")));
    assert_eq!(
        listing(&root.join("plugins/t.plug/directives/"), &ListFilter::Yaml),
        Some(Some(vec!["a.yaml".to_string()]))
    );
    assert!(file(&root.join("plugins/t.plug/directives/a.yaml")));

    // Every record holds while nothing changes.
    assert!(cache.revalidate(&|_| false));
    // Creating the missing import fails the record.
    write(&root.join("schema/missing.yaml"), "state: {}\n");
    assert!(!cache.revalidate(&|_| false));
}

#[test]
fn an_untracked_cache_records_nothing() {
    let dir = lute_scratch::ScratchDir::named("deps-untracked");
    let root = std::fs::canonicalize(&*dir).unwrap();
    write(
        &root.join("lute.project.yaml"),
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    );
    let scene = root.join("a.lute");
    let text = "---\nkind: scene\nid: scene.a\n---\n\n## A\n\n@narrator: Hi.\n";
    write(&scene, text);
    let cache = InputCache::default();
    assemble_input_with_mode(
        &cache,
        &scene,
        text.to_string(),
        None,
        Some(&root),
        None,
        Mode::Author,
    );
    assert!(cache.dependencies().is_empty());
}
