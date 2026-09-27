//! dsl 0.28.0 §1 (round-6 T1-10): a reserved name is refused where it is
//! declared (`E-RESERVED-NAME`), naming a replacement — not accepted there and
//! misread where it is used.
use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_core_span::Diagnostic;
use lute_manifest::provider::ProviderSet;

fn diags(text: &str) -> Vec<Diagnostic> {
    check(&CheckInput {
        text: text.to_string(),
        uri: "reserved_names".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    })
    .diagnostics
}

/// The `E-RESERVED-NAME` messages of a scene with frontmatter `extra` and
/// body `body`.
fn reserved(extra: &str, body: &str) -> Vec<String> {
    let text = format!("---\nkind: scene\nid: s\n{extra}---\n## A\n{body}\n@narrator: a\n");
    diags(&text)
        .into_iter()
        .filter(|d| d.code == "E-RESERVED-NAME")
        .map(|d| d.message)
        .collect()
}

#[test]
fn a_state_root_cannot_name_a_def_or_an_entity_member() {
    let ms = reserved(
        "state:\n  run.tips: { type: number, default: 0 }\ndefs:\n  run: \"run.tips > 4\"\n\
         entities:\n  item: { members: [lamp, clock] }\n",
        "",
    );
    assert!(
        ms.iter().any(|m| m.contains("`run` is a state root")
            && m.contains("a def")
            && m.contains("e.g. `isRun`")),
        "{ms:#?}"
    );
    assert!(
        ms.iter()
            .any(|m| m.contains("`clock`") && m.contains("entity kind `item`")),
        "{ms:#?}"
    );
}

#[test]
fn value_words_and_numbers_cannot_be_members() {
    let ms = reserved(
        "state:\n  run.lock: { type: { enum: [set, unset] }, default: set }\n\
         enums:\n  landing: [1, 2]\nentities:\n  crew: { members: [mira, true, _] }\n",
        "",
    );
    for needle in [
        "`unset` is the no-value word",
        "`1` is a number",
        "`true` is a CEL literal",
        "`_` is the wildcard",
    ] {
        assert!(ms.iter().any(|m| m.contains(needle)), "{needle}: {ms:#?}");
    }
    // `none` is a fine member: only an id a play can pick is refused.
    assert!(reserved("enums:\n  weapon: [none, sword]\n", "").is_empty());
}

#[test]
fn reserved_ids_are_refused_at_the_id() {
    let body = "<branch id=\"door\">\n  <choice id=\"true\" label=\"Yes\">\n    @narrator: y\n  </choice>\n  <choice id=\"none\" label=\"No\">\n    @narrator: n\n  </choice>\n</branch>";
    let ms = reserved("", body);
    assert!(
        ms.iter()
            .any(|m| m.contains("`true`") && m.contains("e.g. `yes`")),
        "{ms:#?}"
    );
    assert!(
        ms.iter()
            .any(|m| m.contains("`none`") && m.contains("pick: none")),
        "{ms:#?}"
    );
    let unset = reserved("", &body.replace("\"none\"", "\"unset\""));
    assert!(unset.iter().any(|m| m.contains("`unset`")), "{unset:#?}");
}

#[test]
fn relations_refuse_after_calls_and_suggest_a_name() {
    let ms = reserved(
        "entities:\n  place: { members: [dorm] }\nrelations:\n  visited: { args: [place], tier: run }\n  completed: { args: [place], tier: run }\n",
        "",
    );
    assert!(
        ms.iter()
            .any(|m| m.contains("`visited`") && m.contains("e.g. `wasAt`")),
        "{ms:#?}"
    );
    assert!(
        ms.iter()
            .any(|m| m.contains("`completed`") && m.contains("`after:` call")),
        "{ms:#?}"
    );
}
