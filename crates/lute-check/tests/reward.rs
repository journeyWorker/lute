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
            ..Default::default()
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

/// dsl 0.26.0 §2.5 (T1-8): a snapshot whose `ITEM` kind carries `target`.
fn snap_with_target(target: lute_manifest::schema::RewardTarget) -> CapabilitySnapshot {
    let mut snap = lute_manifest::core::load_core_snapshot();
    snap.reward_kinds.insert(
        "ITEM".into(),
        RewardKindDecl {
            name: "ITEM".into(),
            target: Some(target),
            ..Default::default()
        },
    );
    snap.version = lute_manifest::snapshot::capability_version(&snap);
    snap
}

fn reward_target_diags(
    rewards: &str,
    target: lute_manifest::schema::RewardTarget,
    providers: ProviderSet,
) -> Vec<(String, String)> {
    let text = format!(
        "---\nkind: quest\nentities:\n  bagItem: {{ members: [goodRod, potion] }}\n  \
         loot: {{ open: engine }}\n---\n<quest id=\"q\">\n{rewards}</quest>\n"
    );
    let input = CheckInput {
        text,
        uri: "reward".into(),
        snapshot: snap_with_target(target),
        providers,
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    };
    check(&input)
        .diagnostics
        .into_iter()
        .filter(|d| d.code == "E-REWARD-TARGET" || d.code == "W-CATALOG-STALE")
        .map(|d| (d.code, d.message))
        .collect()
}

fn entity_contract(kind: &str, required: bool) -> lute_manifest::schema::RewardTarget {
    lute_manifest::schema::RewardTarget {
        entity: Some(kind.into()),
        required,
        ..Default::default()
    }
}

#[test]
fn entity_target_contract_checks_membership_with_a_did_you_mean() {
    let ok = reward_target_diags(
        "<reward kind=\"ITEM\" target=\"goodRod\"/>\n",
        entity_contract("bagItem", false),
        ProviderSet::default(),
    );
    assert!(ok.is_empty(), "a member target is clean: {ok:?}");

    let bad = reward_target_diags(
        "<reward kind=\"ITEM\" target=\"godRod\"/>\n",
        entity_contract("bagItem", false),
        ProviderSet::default(),
    );
    assert_eq!(bad.len(), 1, "{bad:?}");
    assert_eq!(bad[0].0, "E-REWARD-TARGET");
    assert!(bad[0].1.contains("did you mean `goodRod`"), "{bad:?}");
}

#[test]
fn an_open_entity_kind_accepts_any_target() {
    let d = reward_target_diags(
        "<reward kind=\"ITEM\" target=\"anythingAtAll\"/>\n",
        entity_contract("loot", false),
        ProviderSet::default(),
    );
    assert!(d.is_empty(), "{d:?}");
}

#[test]
fn a_contract_naming_an_undeclared_kind_is_e_reward_target() {
    let d = reward_target_diags(
        "<reward kind=\"ITEM\" target=\"goodRod\"/>\n",
        entity_contract("bagItems", false),
        ProviderSet::default(),
    );
    assert_eq!(d.len(), 1, "{d:?}");
    assert!(d[0].1.contains("entity kind `bagItems`"), "{d:?}");
}

#[test]
fn required_rejects_a_missing_target_and_only_then() {
    let optional = reward_target_diags(
        "<reward kind=\"ITEM\"/>\n",
        entity_contract("bagItem", false),
        ProviderSet::default(),
    );
    assert!(
        optional.is_empty(),
        "no `required:`, no target is fine: {optional:?}"
    );

    let required = reward_target_diags(
        "<reward kind=\"ITEM\"/>\n<objective id=\"o\" title=\"O\" done=\"true\">\n\
         <reward kind=\"ITEM\"/>\n</objective>\n",
        entity_contract("bagItem", true),
        ProviderSet::default(),
    );
    assert_eq!(
        required
            .iter()
            .filter(|(c, _)| c == "E-REWARD-TARGET")
            .count(),
        2,
        "quest- and objective-level rewards both need a target: {required:?}"
    );
}

#[test]
fn provider_target_contract_resolves_against_the_pinned_catalog() {
    let providers = |stale: bool| {
        ProviderSet::from_one(lute_manifest::provider::ProviderSnapshot {
            manifest_version: "v".into(),
            provider_version: "1".into(),
            entries: [("items".to_string(), vec!["goodRod".to_string()])].into(),
            stale,
        })
    };
    let contract = || lute_manifest::schema::RewardTarget {
        provider: Some("items".into()),
        ..Default::default()
    };
    let ok = reward_target_diags(
        "<reward kind=\"ITEM\" target=\"goodRod\"/>\n",
        contract(),
        providers(false),
    );
    assert!(ok.is_empty(), "{ok:?}");
    let absent = reward_target_diags(
        "<reward kind=\"ITEM\" target=\"superRod\"/>\n",
        contract(),
        providers(false),
    );
    assert_eq!(absent.len(), 1, "{absent:?}");
    assert_eq!(absent[0].0, "E-REWARD-TARGET");
    let stale = reward_target_diags(
        "<reward kind=\"ITEM\" target=\"superRod\"/>\n",
        contract(),
        providers(true),
    );
    assert_eq!(stale.len(), 1, "{stale:?}");
    assert_eq!(stale[0].0, "W-CATALOG-STALE", "a stale catalog only warns");
}
