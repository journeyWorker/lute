//! 0.50.0 §5.1 (#169): a reward kind's `effects:` — the facts a grant
//! asserts and retracts and the quests it accepts — and its load faults.

use std::fs;

use lute_manifest::loader::{load_plugin_dir, LoadError};
use lute_manifest::schema::{AcceptEffect, RewardKindDecl, RewardTarget};

/// A plugin package exporting `rewardkinds/a.yaml` with `body`.
fn pkg(tag: &str, body: &str) -> lute_scratch::ScratchDir {
    let tmp = lute_scratch::ScratchDir::named(tag);
    fs::create_dir_all(tmp.join("rewardkinds")).unwrap();
    fs::write(
        tmp.join("plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  rewardKinds: rewardkinds/\n",
    )
    .unwrap();
    fs::write(tmp.join("rewardkinds/a.yaml"), body).unwrap();
    tmp
}

/// The load errors of `body`, as `(code, line, message)`.
fn faults(tag: &str, body: &str) -> Vec<(&'static str, Option<usize>, String)> {
    let errs = load_plugin_dir(&pkg(tag, body)).expect_err(body);
    errs.iter()
        .map(|e| {
            let line = match e {
                LoadError::Parse { at, .. } | LoadError::Key { at, .. } => at.map(|(l, _)| l),
                _ => None,
            };
            (e.code(), line, e.to_string())
        })
        .collect()
}

#[test]
fn effects_load_and_print_in_the_kind() {
    let dir = pkg(
        "effects-ok",
        "rewardKinds:\n  \
         BADGE: { target: { entity: person, required: true }, effects: { asserts: [\"met(@target)\"] } }\n  \
         REVOKE: { target: { entity: person, required: true }, effects: { retracts: [\"met(@target)\", \"met(_)\"] } }\n  \
         OFFER_QUEST: { target: { required: true }, effects: { accepts: [\"@target\", \"second\"] } }\n  \
         MOVE: { effects: { retracts: [\"at(_)\"], asserts: [\"at(lab)\"] } }\n",
    );
    let loaded = load_plugin_dir(&dir).expect("every effect shape loads");
    let kind = |n: &str| loaded.reward_kinds.iter().find(|k| k.name == n).unwrap();
    let badge = kind("BADGE").effects.as_ref().unwrap();
    assert_eq!(badge.asserts[0].to_string(), "met(@target)");
    assert!(badge.reads_target());
    let offer = kind("OFFER_QUEST").effects.as_ref().unwrap();
    assert_eq!(
        offer.accepts,
        vec![AcceptEffect::Target, AcceptEffect::Quest("second".into())]
    );
    let moved = kind("MOVE").effects.as_ref().unwrap();
    assert!(!moved.reads_target());
    // Retracts first, then asserts: the order a grant applies them.
    let order: Vec<(String, bool)> = moved
        .facts()
        .map(|(f, assert)| (f.to_string(), assert))
        .collect();
    assert_eq!(
        order,
        vec![("at(_)".to_string(), false), ("at(lab)".to_string(), true)]
    );
    assert_eq!(
        format!("{:?}", kind("BADGE")),
        "RewardKindDecl { name: \"BADGE\", target: Some(RewardTarget { entity: \"person\", \
         required: true }), attrs: [], effects: RewardEffects { asserts: [\"met(@target)\"] } }"
    );
}

/// `capabilitySnapshot` hashes `RewardKindDecl`'s `Debug`: a kind without
/// `effects` prints exactly as on 0.49.1, so its hash is unchanged.
#[test]
fn a_kind_without_effects_prints_as_in_0_49() {
    let kind = RewardKindDecl {
        name: "COINS".into(),
        target: Some(RewardTarget {
            entity: Some("person".into()),
            required: true,
            ..Default::default()
        }),
        attrs: vec![],
        credits: Some("run.coins".into()),
        effects: None,
    };
    assert_eq!(
        format!("{kind:?}"),
        "RewardKindDecl { name: \"COINS\", target: Some(RewardTarget { entity: \"person\", \
         required: true }), attrs: [], credits: \"run.coins\" }"
    );
}

#[test]
fn an_empty_effects_block_declares_no_effect() {
    let got = faults("effects-empty", "rewardKinds:\n  BADGE: { effects: {} }\n");
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0].0, "E-PLUGIN-PARSE");
    assert!(got[0].2.contains("declares no effect"), "{got:?}");
}

#[test]
fn writes_is_no_key_and_names_credits() {
    let got = faults(
        "effects-writes",
        "rewardKinds:\n  BADGE: { effects: { writes: [] } }\n",
    );
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0].0, "E-PLUGIN-KEY");
    assert!(
        got[0]
            .2
            .contains("has no key `writes` (its keys: asserts, retracts, accepts, raises) — a reward's state write is `credits:`"),
        "{got:?}"
    );
}

/// 0.52.0 §7.3: an effect reads `@target` or a declared attribute; an
/// undeclared one names what the kind declares.
#[test]
fn an_effect_reads_target_or_a_declared_attribute() {
    let got = faults(
        "effects-rarity",
        "rewardKinds:\n  COINS: {}\n  BADGE:\n    target: { entity: person }\n    effects:\n      asserts: [\"met(@rarity)\"]\n",
    );
    assert_eq!(got.len(), 1, "{got:?}");
    let (code, line, message) = &got[0];
    assert_eq!((*code, *line), ("E-PLUGIN-PARSE", Some(6)), "{got:?}");
    assert!(
        message.ends_with(
            "reward kind `BADGE` effect `met(@rarity)` reads `@rarity`, but the kind declares \
             no such attribute (declared: target)"
        ),
        "{message}"
    );
}

#[test]
fn an_assert_is_ground() {
    let got = faults(
        "effects-wild",
        "rewardKinds:\n  BADGE: { effects: { asserts: [\"met(_)\"] } }\n",
    );
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0].0, "E-PLUGIN-PARSE");
    assert!(
        got[0]
            .2
            .contains("reward kind `BADGE` effect `met(_)` asserts `_`"),
        "{got:?}"
    );
}

#[test]
fn accepts_take_target_or_a_quest_id() {
    for bad in ["\"@rarity\"", "\"two words\"", "\"q(x)\""] {
        let got = faults(
            "effects-accepts",
            &format!("rewardKinds:\n  OFFER: {{ effects: {{ accepts: [{bad}] }} }}\n"),
        );
        assert_eq!(got.len(), 1, "{bad}: {got:?}");
        assert_eq!(got[0].0, "E-PLUGIN-PARSE", "{bad}");
        assert!(
            got[0].2.contains("is neither `@target` nor a quest id"),
            "{bad}: {got:?}"
        );
    }
}

#[test]
fn a_forbidden_target_reads_no_target() {
    for effect in [
        "asserts: [\"met(@target)\"]",
        "retracts: [\"met(@target)\"]",
        "accepts: [\"@target\"]",
    ] {
        let got = faults(
            "effects-forbidden",
            &format!("rewardKinds:\n  BADGE: {{ target: {{ forbidden: true }}, effects: {{ {effect} }} }}\n"),
        );
        assert_eq!(got.len(), 1, "{effect}: {got:?}");
        assert_eq!(got[0].0, "E-PLUGIN-PARSE", "{effect}");
        assert!(
            got[0].2.contains(
                "reads `@target`, but the kind declares `target: { forbidden: true }`, so no \
                 `<reward>` of it has a target to bind"
            ),
            "{effect}: {got:?}"
        );
    }
    // A literal accept binds nothing.
    let dir = pkg(
        "effects-forbidden-literal",
        "rewardKinds:\n  OFFER: { target: { forbidden: true }, effects: { accepts: [second] } }\n",
    );
    load_plugin_dir(&dir).expect("a literal quest id needs no target");
}
