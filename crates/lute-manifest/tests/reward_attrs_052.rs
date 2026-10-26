//! 0.52.0 §7.1, §7.3 (#201): a reward kind's `attrs:` follow the directive
//! attribute declaration rules at load, its `effects` read `@<attr>` in
//! facts and raise targets, and assembly types a raise's `@<attr>` as a
//! directive's (`E-PLUGIN-RAISES`).
//!
//! On v0.51.0 every faulty declaration below loads silently (the kind's
//! `attrs:` were parsed and never judged), and an effect reading a declared
//! attribute is refused (`reads only @target`), so each test fails there.

use std::collections::BTreeMap;
use std::fs;

use lute_manifest::assemble::assemble_snapshot;
use lute_manifest::loader::{load_plugin_dir, load_plugins_dir, LoadError};
use lute_manifest::resolve::ActivePlugin;

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

/// `got` holds exactly one `E-PLUGIN-PARSE` at `line` whose message ends
/// with `tail`.
fn assert_one(got: &[(&'static str, Option<usize>, String)], line: usize, tail: &str) {
    assert_eq!(got.len(), 1, "{got:?}");
    let (code, at, message) = &got[0];
    assert_eq!((*code, *at), ("E-PLUGIN-PARSE", Some(line)), "{got:?}");
    assert!(message.ends_with(tail), "{message}");
}

#[test]
fn well_formed_attrs_and_effects_reading_them_load() {
    let loaded = load_plugin_dir(&pkg(
        "rattrs-ok",
        "rewardKinds:\n  \
         BADGE:\n    \
         attrs:\n      \
         - { name: rarity, type: string, default: common }\n      \
         - { name: tier, type: int }\n      \
         - { name: shiny, type: bool }\n    \
         effects: { asserts: [\"badge(@rarity)\"], retracts: [\"badge(@target)\"] }\n",
    ))
    .expect("declared attributes and effects reading them load");
    let badge = &loaded.reward_kinds[0];
    assert_eq!(badge.attrs.len(), 3);
    let effects = badge.effects.as_ref().unwrap();
    assert!(effects.reads_attr("rarity"));
    assert!(!effects.reads_attr("tier"));
    assert!(effects.reads_target());
}

#[test]
fn an_attribute_named_like_a_reward_key_is_refused() {
    let got = faults(
        "rattrs-reserved",
        "rewardKinds:\n  BADGE:\n    attrs:\n      - { name: amount, type: int }\n",
    );
    assert_one(
        &got,
        4,
        "reward kind `BADGE` declares attribute `amount`, which `<reward>` already has (`id`, \
         `kind`, `target`, `amount`, `when`, `outcome`) — give it another name",
    );
}

#[test]
fn a_repeated_attribute_is_refused_at_the_repeat() {
    let got = faults(
        "rattrs-dup",
        "rewardKinds:\n  BADGE:\n    attrs:\n      - { name: rarity, type: string }\n      \
         - { name: rarity, type: string }\n",
    );
    assert_one(
        &got,
        5,
        "reward kind `BADGE` declares attribute `rarity` more than once — keep one declaration",
    );
}

#[test]
fn a_default_of_another_type_is_refused() {
    let got = faults(
        "rattrs-default",
        "rewardKinds:\n  BADGE:\n    attrs:\n      - { name: rarity, type: string, default: 3 }\n",
    );
    assert_one(
        &got,
        4,
        "reward kind `BADGE` declares attribute `rarity` as `string` with `default: 3`, which is \
         not of type `string` — give a default of the declared type",
    );
}

#[test]
fn a_structured_attribute_is_refused() {
    let got = faults(
        "rattrs-list",
        "rewardKinds:\n  BADGE:\n    attrs:\n      - { name: tags, type: { list: string } }\n",
    );
    assert_one(
        &got,
        4,
        "reward kind `BADGE` declares attribute `tags` as `list<string>`; an attribute value is \
         written as one scalar (string, number, bool, an enum or entity member, a provider or \
         asset id), so no `.lute` value can satisfy a `list`, `map` or `record` (structured \
         attribute values are not supported yet): give each field its own attribute",
    );
}

#[test]
fn a_text_attribute_stays_refused_when_the_file_parses() {
    let got = faults(
        "rattrs-text",
        "rewardKinds:\n  BADGE:\n    attrs:\n      - { name: blurb, type: text }\n",
    );
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0].0, "E-PLUGIN-PARSE", "{got:?}");
    assert!(got[0].2.contains("text"), "{got:?}");
}

#[test]
fn an_effect_reading_an_undeclared_attribute_names_the_declared_ones() {
    let got = faults(
        "rattrs-effect",
        "rewardKinds:\n  BADGE:\n    attrs:\n      - { name: rarity, type: string }\n    \
         effects: { asserts: [\"badge(@rarty)\"] }\n",
    );
    assert_one(
        &got,
        5,
        "reward kind `BADGE` effect `badge(@rarty)` reads `@rarty`, but the kind declares no \
         such attribute (declared: target, rarity); did you mean `@rarity`?",
    );
}

#[test]
fn a_raise_reading_an_undeclared_attribute_reads_as_the_fact_fault() {
    let got = faults(
        "rattrs-raise",
        "rewardKinds:\n  BADGE:\n    attrs:\n      - { name: scene, type: string }\n    \
         effects: { raises: [{ occasion: cutscene, target: \"@scen\" }] }\n",
    );
    assert_one(
        &got,
        5,
        "reward kind `BADGE` raises `cutscene` for `@scen`, but the kind declares no such \
         attribute (declared: target, scene); did you mean `@scene`?",
    );
}

/// Assembly types a reward kind's raise `@<attr>` by the attribute's
/// declared type, as a directive's (`E-PLUGIN-RAISES`); `@target` keeps
/// the kind's `target:` contract.
#[test]
fn a_raise_attribute_is_typed_like_a_directive_attribute() {
    let assembled = |ty: &str| {
        let scratch = lute_scratch::ScratchDir::named("rattrs-assemble");
        let dir = scratch.join("plugins/acme.stage");
        fs::create_dir_all(dir.join("occasions")).unwrap();
        fs::create_dir_all(dir.join("rewardkinds")).unwrap();
        fs::write(
            dir.join("plugin.yaml"),
            "id: acme.stage\nversion: 0.1.0\nkind: capability\ndepends: [{ id: lute.core, \
             range: \"^0.0.1\" }]\nexports:\n  occasions: occasions/\n  rewardKinds: \
             rewardkinds/\n",
        )
        .unwrap();
        fs::write(
            dir.join("occasions/a.yaml"),
            "occasions:\n  cutscene: { target: { prefix: scene, entity: cutsceneId }, \
             raisedBy: declared }\n",
        )
        .unwrap();
        fs::write(
            dir.join("rewardkinds/a.yaml"),
            format!(
                "rewardKinds:\n  SHOW: {{ attrs: [{{ name: scene, type: {ty} }}], effects: {{ \
                 raises: [{{ occasion: cutscene, target: \"@scene\" }}] }} }}\n"
            ),
        )
        .unwrap();
        let (installed, load) = load_plugins_dir(&scratch.join("plugins"));
        assert!(load.is_empty(), "{load:?}");
        let active = ["lute.core", "acme.stage"].map(|id| ActivePlugin {
            id: id.into(),
            options: BTreeMap::new(),
        });
        let (snap, errs) = assemble_snapshot(&active, &installed);
        let raises = snap.reward_kinds["SHOW"]
            .effects
            .as_ref()
            .map_or(0, |e| e.raises.len());
        (
            errs.iter()
                .map(|e| (e.code(), e.to_string()))
                .collect::<Vec<_>>(),
            raises,
        )
    };
    assert_eq!(assembled("{ entity: cutsceneId }"), (Vec::new(), 1));
    assert_eq!(
        assembled("string"),
        (
            vec![(
                "E-PLUGIN-RAISES",
                "reward kind `SHOW` (`acme.stage`) raises `cutscene` for `@scene`, which is \
                 typed `string`; the target of `cutscene` is a `cutsceneId` member, so type the \
                 attribute `{ entity: cutsceneId }`"
                    .to_string()
            )],
            0
        )
    );
}
