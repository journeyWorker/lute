use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;

fn codes(text: &str) -> Vec<String> {
    let input = CheckInput {
        text: text.to_string(),
        uri: "t".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
    };
    check(&input).diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn standalone_schema_fragment_has_no_kind_or_meta_error() {
    // A schema doc (state/defs only, no kind:) checked standalone must NOT
    // trip E-KIND-MISSING or E-META-MISSING (dsl 0.2.0 §3.1 / §6.1).
    let cs = codes("---\nstate:\n  run.blessed: { type: bool, default: false }\n---\n");
    assert!(!cs.iter().any(|c| c == "E-KIND-MISSING" || c == "E-META-MISSING"), "{cs:?}");
}

#[test]
fn standalone_component_fragment_has_no_kind_error() {
    let cs = codes("---\ncomponent: greet\nparams:\n  who: string\n---\n## Scene 1.\n:x: hi\n");
    assert!(!cs.iter().any(|c| c == "E-KIND-MISSING"), "{cs:?}");
}

#[test]
fn scene_missing_kind_still_errors() {
    // A doc with body nodes but no kind: is a real mistake -> still E-KIND-MISSING.
    let cs = codes("---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n:x: hi\n");
    assert!(cs.contains(&"E-KIND-MISSING".to_string()), "{cs:?}");
}

#[test]
fn unrecognized_kind_with_schema_shape_still_errors() {
    // `kind:` PRESENT but unrecognized + schema-shaped + no body must NOT be
    // swallowed by shape inference — E-UNKNOWN-KIND must survive.
    let cs = codes("---\nkind: reward\nstate:\n  run.blessed: { type: bool, default: false }\n---\n");
    assert!(cs.contains(&"E-UNKNOWN-KIND".to_string()), "{cs:?}");
}
