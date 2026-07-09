//! TRANSITIONAL regression (Plan A review finding): a top-level `<quest>`
//! (parsed into `doc.quests`, not `doc.shots`) must still hit the D6 clean-check
//! deny gate so a quest document can never pass check / compile until Plan C
//! lands real quest semantics. Plan C DELETES this file (it removes the
//! `E-QUEST-UNSUPPORTED` gate entirely; see `tests/quest.rs::no_more_quest_unsupported`).
use lute_check::{check, CheckInput, CheckResult, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;

fn codes(text: &str) -> Vec<String> {
    let input = CheckInput {
        text: text.to_string(),
        uri: "quest".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
    };
    let out: CheckResult = check(&input);
    out.diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn top_level_quest_is_rejected_transitionally() {
    // A doc whose ONLY content is a top-level <quest> with an <objective>/<on>
    // body: check() must NOT return clean — the quest span and the nested nodes
    // both reach the transitional gate.
    let cs = codes(
        "<quest id=\"q\">\n\
         <objective id=\"o\" done=\"run.d\"/>\n\
         <on event=\"questComplete\">\n:x: hi\n</on>\n\
         </quest>\n",
    );
    assert!(
        cs.contains(&"E-QUEST-UNSUPPORTED".to_string()),
        "top-level <quest> must be rejected until Plan C: {cs:?}",
    );
}

#[test]
fn empty_body_quest_is_still_rejected() {
    // Even a quest with no <on>/<objective> nodes must be rejected (the gate
    // fires at the <quest> span itself, not only on nested constructs).
    let cs = codes("<quest id=\"q\">\n</quest>\n");
    assert!(
        cs.contains(&"E-QUEST-UNSUPPORTED".to_string()),
        "empty <quest> must still be rejected: {cs:?}",
    );
}
