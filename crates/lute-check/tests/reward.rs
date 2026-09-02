//! Reward-element checks (dsl 0.16.0 §2/§4/§6): the shape closure
//! (`E-REWARD-ATTR`), the D-J attribute closure (`E-UNKNOWN-ATTR`), the
//! vocabulary closure (`E-REWARD-KIND`), the Bool profile gate on
//! `reward.when`, and `E-MAYBE-UNSET` for a maybe-unset read reached
//! through a reward guard.

use lute_check::{check, CheckInput, CheckResult, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;
use lute_manifest::schema::RewardKindDecl;
use lute_manifest::snapshot::CapabilitySnapshot;

fn run_with(text: &str, snapshot: CapabilitySnapshot) -> CheckResult {
    let input = CheckInput {
        text: text.to_string(),
        uri: "reward".into(),
        snapshot,
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    };
    check(&input)
}

fn run(text: &str) -> CheckResult {
    run_with(text, lute_manifest::core::load_core_snapshot())
}

fn codes(text: &str) -> Vec<String> {
    run(text).diagnostics.into_iter().map(|d| d.code).collect()
}

fn codes_with(text: &str, snapshot: CapabilitySnapshot) -> Vec<String> {
    run_with(text, snapshot)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn quest_reward_missing_kind_is_e_reward_attr() {
    let cs = codes("---\nkind: quest\n---\n<quest id=\"q\">\n<reward amount=\"1\"/>\n</quest>\n");
    assert!(
        cs.iter().any(|c| c == "E-REWARD-ATTR"),
        "want E-REWARD-ATTR for a missing kind: {cs:?}"
    );
}

#[test]
fn quest_reward_malformed_amount_string_is_e_reward_attr() {
    let cs = codes(
        "---\nkind: quest\n---\n<quest id=\"q\">\n<reward kind=\"gold\" amount=\"x\"/>\n</quest>\n",
    );
    assert!(
        cs.iter().any(|c| c == "E-REWARD-ATTR"),
        "want E-REWARD-ATTR for a non-numeric amount: {cs:?}"
    );
}

#[test]
fn quest_reward_reversed_range_is_e_reward_attr() {
    let cs = codes(
        "---\nkind: quest\n---\n<quest id=\"q\">\n<reward kind=\"gold\" amount=\"5..2\"/>\n</quest>\n",
    );
    assert!(
        cs.iter().any(|c| c == "E-REWARD-ATTR"),
        "want E-REWARD-ATTR for a reversed range: {cs:?}"
    );
}

#[test]
fn objective_reward_with_on_is_e_reward_attr() {
    // dsl 0.16.0 §2 D-D: only a QUEST-level reward may carry `on="failed"`;
    // an objective grants at first `done` and never at fail.
    let cs = codes(
        "---\nkind: quest\n---\n<quest id=\"q\">\n\
         <objective id=\"o\" done=\"run.d\">\n\
         <reward kind=\"gold\" amount=\"1\" on=\"failed\"/>\n\
         </objective>\n</quest>\n",
    );
    assert!(
        cs.iter().any(|c| c == "E-REWARD-ATTR"),
        "want E-REWARD-ATTR for an objective-level `on=`: {cs:?}"
    );
}

#[test]
fn quest_reward_bad_on_enum_value_is_e_reward_attr() {
    let cs = codes(
        "---\nkind: quest\n---\n<quest id=\"q\">\n\
         <reward kind=\"gold\" amount=\"1\" on=\"banana\"/>\n</quest>\n",
    );
    assert!(
        cs.iter().any(|c| c == "E-REWARD-ATTR"),
        "want E-REWARD-ATTR for on=\"banana\": {cs:?}"
    );
}

#[test]
fn quest_reward_unknown_attr_is_e_unknown_attr() {
    let cs = codes(
        "---\nkind: quest\n---\n<quest id=\"q\">\n\
         <reward kind=\"gold\" amount=\"1\" foo=\"bar\"/>\n</quest>\n",
    );
    assert!(
        cs.iter().any(|c| c == "E-UNKNOWN-ATTR"),
        "want E-UNKNOWN-ATTR for `foo=`: {cs:?}"
    );
}

#[test]
fn reward_when_that_fails_to_parse_hits_e_cel_parse() {
    let cs = codes(
        "---\nkind: quest\nstate:\n  run.x: { type: number, default: 0 }\n---\n\
         <quest id=\"q\">\n<reward kind=\"gold\" amount=\"1\" when=\"run.x >\"/>\n</quest>\n",
    );
    assert!(
        cs.iter().any(|c| c == "E-CEL-PARSE"),
        "want E-CEL-PARSE for a truncated when guard: {cs:?}"
    );
}

#[test]
fn reward_when_reading_maybe_unset_path_is_e_maybe_unset() {
    // `run.x` is declared but has no default → a bare read in the guard
    // is `E-MAYBE-UNSET` at the guard's slot span.
    let cs = codes(
        "---\nkind: quest\nstate:\n  run.x: { type: bool }\n---\n\
         <quest id=\"q\">\n<reward kind=\"gold\" amount=\"1\" when=\"run.x\"/>\n</quest>\n",
    );
    assert!(
        cs.iter().any(|c| c == "E-MAYBE-UNSET"),
        "want E-MAYBE-UNSET for a bare maybe-unset read in reward.when: {cs:?}"
    );
}

#[test]
fn vocabulary_gate_is_silent_when_no_reward_kinds_declared() {
    // dsl 0.16.0 §4: shape-only mode — a fresh scenario compiles with any
    // kind name because no plugin has published a vocabulary yet.
    let cs = codes(
        "---\nkind: quest\n---\n<quest id=\"q\">\n<reward kind=\"GOLD\" amount=\"1\"/>\n</quest>\n",
    );
    assert!(
        !cs.iter().any(|c| c == "E-REWARD-KIND"),
        "shape-only mode must NOT gate on the vocabulary: {cs:?}"
    );
}

#[test]
fn foreign_kind_is_e_reward_kind_when_vocabulary_declared() {
    // With a `rewardKinds: {SHARD}` snapshot: `kind="GOLD"` is foreign;
    // `kind="SHARD"` is clean.
    let mut snap = lute_manifest::core::load_core_snapshot();
    snap.reward_kinds.insert(
        "SHARD".into(),
        RewardKindDecl {
            name: "SHARD".into(),
        },
    );
    snap.version = lute_manifest::snapshot::capability_version(&snap);

    let cs_bad = codes_with(
        "---\nkind: quest\n---\n<quest id=\"q\">\n<reward kind=\"GOLD\" amount=\"1\"/>\n</quest>\n",
        snap.clone(),
    );
    assert!(
        cs_bad.iter().any(|c| c == "E-REWARD-KIND"),
        "want E-REWARD-KIND for a foreign kind: {cs_bad:?}"
    );

    let cs_ok = codes_with(
        "---\nkind: quest\n---\n<quest id=\"q\">\n<reward kind=\"SHARD\" amount=\"1\"/>\n</quest>\n",
        snap,
    );
    assert!(
        !cs_ok.iter().any(|c| c == "E-REWARD-KIND"),
        "a declared kind must not flag E-REWARD-KIND: {cs_ok:?}"
    );
}
