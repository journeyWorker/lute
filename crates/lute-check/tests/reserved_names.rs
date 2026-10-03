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
        "state:\n  run.tips: { type: int, default: 0 }\ndefs:\n  run: \"run.tips > 4\"\n\
         entities:\n  item: { members: [lamp, clock] }\n",
        "",
    );
    // The example is written for the refused name, not a fixed one.
    assert!(
        ms.iter().any(
            |m| m.contains("`run` cannot name a def because it is a state root")
                && m.contains("`@run` reads as a bare `run`")
                && m.contains("e.g. `isRun`")
                && !m.contains("clock")
        ),
        "{ms:#?}"
    );
    assert!(
        ms.iter().any(
            |m| m.contains("`clock` cannot name a member of entity kind `item`")
                && m.contains("a bare `clock` starts a state path")
        ),
        "{ms:#?}"
    );
    // One "so" per sentence: the clause and the refusal do not chain.
    assert!(
        ms.iter().all(|m| m.matches(", so ").count() <= 1),
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
        "`unset` cannot name a member of `run.lock`'s enum because it is the no-value word",
        "`1` cannot name a member of enum `landing` because it is a number",
        "it is a CEL literal: a condition and `is=\"true\"` read `true` as the value",
        "`_` cannot name a member of entity kind `crew` because it is the wildcard",
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
            .any(|m| m.contains("`completed` cannot name a relation")
                && m.contains("`after=\"completed(…)\"` reads the quest call")),
        "{ms:#?}"
    );
}

#[test]
fn a_season_or_quest_refusal_names_its_own_example() {
    let ms = reserved(
        "seasons:\n  week: { live: \"run.day > 0\" }\n  clock: { live: \"run.day > 0\" }\n",
        "",
    );
    assert!(
        ms.iter().any(|m| m.contains("`week` cannot name a season")
            && m.contains("`once=\"season:week\"` would sit beside `once=\"week\"`")),
        "{ms:#?}"
    );
    assert!(
        ms.iter().any(|m| m.contains("`clock` cannot name a season")
            && m.contains("`season.clock.…`")
            && !m.contains("season:run")),
        "{ms:#?}"
    );
}
