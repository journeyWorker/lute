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
    let cs = codes("---\ncomponent: greet\nparams:\n  who: string\n---\n## Scene 1.\n@x: hi\n");
    assert!(!cs.iter().any(|c| c == "E-KIND-MISSING"), "{cs:?}");
}

#[test]
fn scene_missing_kind_still_errors() {
    // A doc with body nodes but no kind: is a real mistake -> still E-KIND-MISSING.
    let cs = codes("---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@x: hi\n");
    assert!(cs.contains(&"E-KIND-MISSING".to_string()), "{cs:?}");
}

#[test]
fn unrecognized_kind_with_schema_shape_still_errors() {
    // `kind:` PRESENT but unrecognized + schema-shaped + no body must NOT be
    // swallowed by shape inference — E-UNKNOWN-KIND must survive.
    let cs = codes("---\nkind: reward\nstate:\n  run.blessed: { type: bool, default: false }\n---\n");
    assert!(cs.contains(&"E-UNKNOWN-KIND".to_string()), "{cs:?}");
}

#[test]
fn standalone_component_param_ref_resolves_no_type_error() {
    // dsl §13/§8.1: a component's declared `params:` ARE the `@param` ref
    // namespace for its OWN presentational body — already true when the
    // component is expanded transitively via `::use`; this must ALSO hold
    // for a STANDALONE `lute check` of the component file itself. `@who` is
    // legal here because `::auto`'s `character` attr is `string`-typed (dsl
    // Appendix A), matching `who: string`.
    let cs = codes(
        "---\ncomponent: greet\nparams:\n  who: string\n---\n## Scene 1.\n\
         ::auto{character=@who action=\"fade-in-up\"}\n@narrator: hi\n",
    );
    assert!(!cs.iter().any(|c| c == "E-UNDECLARED-REF"), "{cs:?}");
    assert!(!cs.iter().any(|c| c == "E-REF-TYPE"), "{cs:?}");
    // A bare `@who` (0 args) matches its 0-arity `def_params` entry — no arity
    // error, the parity counterpart to the call-form regression below.
    assert!(!cs.iter().any(|c| c == "E-REF-ARITY"), "{cs:?}");
}

#[test]
fn standalone_component_param_type_mismatch_flags_ref_type() {
    // A `number`-typed param used where `::auto`'s `character` attr expects a
    // `string` is a produced-type mismatch (dsl §8) — proves `def_types`
    // (not merely `defs`) is seeded from `params:` for the standalone walk.
    let cs = codes(
        "---\ncomponent: greet\nparams:\n  n: number\n---\n## Scene 1.\n\
         ::auto{character=@n action=\"fade-in-up\"}\n@narrator: hi\n",
    );
    assert!(cs.contains(&"E-REF-TYPE".to_string()), "{cs:?}");
}

#[test]
fn scene_doc_not_polluted_by_component_param_seeding() {
    // A normal SCENE doc must NOT gain a `@param` namespace from this fix —
    // an undeclared `@ghost` ref still flags E-UNDECLARED-REF exactly as
    // before (the seeding is guarded to `MetaKind::Component` only).
    let cs = codes(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
         @narrator: {{@ghost}}\n",
    );
    assert!(cs.contains(&"E-UNDECLARED-REF".to_string()), "{cs:?}");
}

#[test]
fn standalone_component_param_call_form_flags_ref_arity() {
    // A component param is a 0-ARITY value ref: the call form `@who("x")` must
    // flag E-REF-ARITY in a STANDALONE check exactly as it does transitively
    // via `::use` (`component_env` seeds an empty `def_params` entry). Locks
    // standalone/transitive arity parity (post-review fix).
    let cs = codes(
        "---\ncomponent: greet\nparams:\n  who: string\n---\n## Scene 1.\n\
         ::auto{character=@who(\"x\") action=\"fade-in-up\"}\n@narrator: hi\n",
    );
    assert!(cs.contains(&"E-REF-ARITY".to_string()), "{cs:?}");
}
