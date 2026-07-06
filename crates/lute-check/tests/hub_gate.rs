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

/// Even though hubs are gated, defassign still walks each choice's `when` guard
/// (regression for the Task-8 review fix): a maybe-unset DECLARED read inside a
/// hub choice guard must be flagged `E-MAYBE-UNSET`, exactly as `walk_branch`
/// does — it must not escape definite-assignment.
#[test]
fn hub_choice_when_guard_is_checked_by_defassign() {
    let out = codes(
        "---\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  scene.n: { type: number }\n---\n## Shot 1.\n\
         <hub id=\"h\">\n<choice id=\"a\" label=\"A\" when=\"scene.n > 0\" exit>\n:narrator: hi.\n</choice>\n</hub>\n",
    );
    assert!(
        out.contains(&"E-MAYBE-UNSET".to_string()),
        "a hub choice `when` reading a declared no-default path must be flagged E-MAYBE-UNSET \
         (guard must not escape defassign); got {out:?}",
    );
}
