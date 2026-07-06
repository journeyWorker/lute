//! Transitional D6 gate: a `<hub>` document is rejected with `E-HUB-UNSUPPORTED`
//! until Plan B implements real hub semantics. This keeps the D6 clean-check gate
//! sound — a hub document can never pass check (and thus never compiles). Plan B
//! deletes both `E_HUB_UNSUPPORTED` and this test.
use lute_check::{check, CheckInput, Mode, SchemaImports, E_HUB_UNSUPPORTED};
use lute_manifest::provider::ProviderSet;

fn codes(text: &str) -> Vec<String> {
    let input = CheckInput {
        text: text.to_string(),
        uri: "hub_gate".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
    };
    check(&input).diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn hub_is_rejected_until_plan_b() {
    let out = codes(
        "---\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
         <hub id=\"h\">\n<choice id=\"a\" label=\"A\" exit>\n:narrator: hi.\n</choice>\n</hub>\n",
    );
    assert!(
        out.contains(&E_HUB_UNSUPPORTED.to_string()),
        "a <hub> document must yield {E_HUB_UNSUPPORTED} (transitional D6 gate); got {out:?}",
    );
}
