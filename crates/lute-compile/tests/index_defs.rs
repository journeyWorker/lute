//! dsl 0.41.0 §3.2 / §4.3: the project index `defs` one document
//! contributes ([`lute_compile::index::document_defs`]) never carry an
//! invalid imported def — one typed `number` keeps its name for resolution
//! but is not a condition a host may evaluate — nor a def that references
//! one.

use lute_check::{parse_meta, resolve_components, resolve_imports, CheckInput, Mode};
use lute_manifest::core::load_core_snapshot;
use lute_manifest::provider::ProviderSet;
use lute_manifest::snapshot::CapabilitySnapshot;

#[test]
fn invalid_schema_defs_and_their_dependants_are_not_exported() {
    let dir = lute_scratch::ScratchDir::named("index-defs-invalid");
    std::fs::write(
        dir.join("world.schema.yaml"),
        "state:\n  run.gold: { type: int, default: 0 }\n\
         defs:\n  \
           bad: { type: number, cel: \"true\" }\n  \
           leans: \"@bad && run.gold > 0\"\n  \
           good: \"run.gold > 1\"\n",
    )
    .unwrap();
    let text = "---\nkind: scene\nid: a\nuses: [world.schema.yaml]\n---\n## A\n@narrator: Hi.\n";
    let (doc, _) = lute_syntax::parse(text);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let input = CheckInput::new(
        text.to_string(),
        "a.lute".into(),
        Mode::Ci,
        lute_check::InputSources::from_parts(
            load_core_snapshot(),
            ProviderSet::default(),
            resolve_imports(&dir, &meta0.uses, &meta0.extends, doc.meta.span),
            resolve_components(&dir, &meta0.components, doc.meta.span),
            Default::default(),
        ),
    );
    assert!(
        lute_check::def_decl::is_invalid(&input.imports().defs["bad"]),
        "the checker marks `bad` invalid: {:?}",
        input.imports().defs["bad"]
    );

    let defs = lute_compile::index::document_defs(&input);
    let names: Vec<&str> = defs.keys().map(String::as_str).collect();
    assert_eq!(names, ["good"], "{defs:#?}");
}
