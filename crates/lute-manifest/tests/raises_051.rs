//! 0.51.0 §4.1–§4.2 (#170): declared occasion raises — `effects.raises` on
//! directives and reward kinds, `raisedBy: declared` on occasions, their load
//! faults (`E-PLUGIN-PARSE`), the refusal on a `lower:` directive
//! (`E-PLUGIN-INVALID-DIRECTIVE`), the assembly faults (`E-PLUGIN-RAISES`)
//! and the `capabilitySnapshot` of a manifest without the new keys. Every
//! package is written to disk and goes through the real loader and
//! assembler.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use lute_manifest::assemble::{assemble_snapshot, AssembleError};
use lute_manifest::loader::{load_plugins_dir, LoadError};
use lute_manifest::resolve::ActivePlugin;
use lute_manifest::schema::{
    DirectiveEffects, OccasionDecl, OccasionTarget, RaiseDecl, RaiseTargetDecl, RaisedBy,
    RewardEffects, RewardKindDecl,
};
use lute_manifest::snapshot::CapabilitySnapshot;

/// One plugin package: its id, `depends:` ids and export files (`None` =
/// not exported).
#[derive(Default)]
struct Pkg<'a> {
    id: &'a str,
    depends: &'a [&'a str],
    directives: Option<&'a str>,
    occasions: Option<&'a str>,
    reward_kinds: Option<&'a str>,
}

fn write_pkg(root: &Path, pkg: &Pkg) {
    let dir = root.join(pkg.id);
    fs::create_dir_all(&dir).unwrap();
    let mut depends = String::from("[{ id: lute.core, range: \"^0.0.1\" }");
    for d in pkg.depends {
        depends.push_str(&format!(", {{ id: {d}, range: \"^0.1.0\" }}"));
    }
    depends.push(']');
    let mut exports = String::new();
    for (export, sub, text) in [
        ("directives", "directives", pkg.directives),
        ("occasions", "occasions", pkg.occasions),
        ("rewardKinds", "rewardkinds", pkg.reward_kinds),
    ] {
        let Some(text) = text else { continue };
        fs::create_dir_all(dir.join(sub)).unwrap();
        fs::write(dir.join(sub).join("a.yaml"), text).unwrap();
        exports.push_str(&format!("  {export}: {sub}/\n"));
    }
    fs::write(
        dir.join("plugin.yaml"),
        format!(
            "id: {}\nversion: 0.1.0\nkind: capability\ndepends: {depends}\nexports:\n{exports}",
            pkg.id
        ),
    )
    .unwrap();
}

/// Load `pkgs` from disk and assemble the ones in `active` (plus core).
fn assemble(
    pkgs: &[Pkg],
    active: &[&str],
) -> (CapabilitySnapshot, Vec<LoadError>, Vec<AssembleError>) {
    let scratch = lute_scratch::ScratchDir::new();
    let root = scratch.join("plugins");
    for pkg in pkgs {
        write_pkg(&root, pkg);
    }
    let (installed, load_errs) = load_plugins_dir(&root);
    let mut plugins = vec![ActivePlugin {
        id: "lute.core".into(),
        options: BTreeMap::new(),
    }];
    plugins.extend(active.iter().map(|id| ActivePlugin {
        id: (*id).to_string(),
        options: BTreeMap::new(),
    }));
    let (snap, errs) = assemble_snapshot(&plugins, &installed);
    (snap, load_errs, errs)
}

const OCCASIONS: &str = "occasions:\n  \
     cutscene: { target: { prefix: scene, entity: cutsceneId }, raisedBy: declared }\n  \
     finale: { target: { prefix: scene, entity: cutsceneId, members: [ending, epilogue] } }\n  \
     any: { target: true }\n  \
     idle: {}\n";

/// One `acme.stage` package with [`OCCASIONS`] and the given directives /
/// reward kinds, assembled alone.
fn stage(
    directives: Option<&str>,
    reward_kinds: Option<&str>,
) -> (CapabilitySnapshot, Vec<LoadError>, Vec<AssembleError>) {
    assemble(
        &[Pkg {
            id: "acme.stage",
            directives,
            occasions: Some(OCCASIONS),
            reward_kinds,
            ..Default::default()
        }],
        &["acme.stage"],
    )
}

/// `(code, message)` of each load error.
fn load_faults(errs: &[LoadError]) -> Vec<(&'static str, String)> {
    errs.iter().map(|e| (e.code(), e.to_string())).collect()
}

/// `(code, message)` of each assembly error.
fn assembly_faults(errs: &[AssembleError]) -> Vec<(&'static str, String)> {
    errs.iter().map(|e| (e.code(), e.to_string())).collect()
}

fn play_scene(effects: &str) -> String {
    format!(
        "directives:\n  - name: playScene\n    attrs:\n      - {{ name: scene, required: true, \
         type: {{ entity: cutsceneId }} }}\n      - {{ name: label, type: string }}\n    \
         effects: {effects}\n"
    )
}

#[test]
fn the_spec_example_loads_and_assembles() {
    let (snap, load, errs) = stage(
        Some(&play_scene(
            "{ raises: [{ occasion: cutscene, target: \"@scene\" }] }",
        )),
        Some(
            "rewardKinds:\n  CUTSCENE: { target: { entity: cutsceneId, required: true }, \
             effects: { raises: [{ occasion: cutscene, target: \"@target\" }] } }\n",
        ),
    );
    assert!(load.is_empty(), "{load:?}");
    assert!(errs.is_empty(), "{errs:?}");
    let raise = RaiseDecl {
        occasion: "cutscene".into(),
        target: Some(RaiseTargetDecl::Attr("scene".into())),
    };
    let effects = snap.directives["playScene"].effects.as_ref().unwrap();
    assert_eq!(effects.raises, vec![raise]);
    assert!(!effects.is_empty());
    assert!(!effects.writes_state(), "a raise writes no state");
    let kind = snap.reward_kinds["CUTSCENE"].effects.as_ref().unwrap();
    assert_eq!(kind.raises.len(), 1);
    assert!(kind.reads_target(), "a `@target` raise reads the target");
    assert_eq!(snap.occasions["cutscene"].raised_by, RaisedBy::Declared);
    assert_eq!(snap.occasions["idle"].raised_by, RaisedBy::Any);
    // A literal target and an untargeted raise land too.
    let (_, load, errs) = stage(
        Some(&play_scene(
            "{ raises: [{ occasion: cutscene, target: scene.chapter2 }, { occasion: idle }, \
             { occasion: finale, target: scene.ending }, { occasion: any, target: \"@label\" }] }",
        )),
        None,
    );
    assert!(load.is_empty(), "{load:?}");
    assert!(errs.is_empty(), "{errs:?}");
}

#[test]
fn a_reward_kind_may_declare_raises_alone_and_an_empty_block_names_raises() {
    let (snap, load, errs) = stage(
        None,
        Some("rewardKinds:\n  SHOW: { effects: { raises: [{ occasion: idle }] } }\n"),
    );
    assert!(load.is_empty() && errs.is_empty(), "{load:?} {errs:?}");
    assert_eq!(
        snap.reward_kinds["SHOW"]
            .effects
            .as_ref()
            .unwrap()
            .raises
            .len(),
        1
    );
    let (_, load, _) = stage(None, Some("rewardKinds:\n  SHOW: { effects: {} }\n"));
    let got = load_faults(&load);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0].0, "E-PLUGIN-PARSE");
    assert!(
        got[0]
            .1
            .contains("list at least one of `asserts:`, `retracts:`, `accepts:` or `raises:`"),
        "{got:?}"
    );
}

/// Every §4.2 load fault is `E-PLUGIN-PARSE`, with the shape a raise takes.
#[test]
fn malformed_raises_are_load_faults() {
    for (effects, needle) in [
        ("{ raises: [] }", "effects.raises is empty"),
        (
            "{ raises: [cutscene] }",
            "entry `cutscene` is not a mapping",
        ),
        (
            "{ raises: [{ target: \"@scene\" }] }",
            "entry names no `occasion:`",
        ),
        (
            "{ raises: [{ occasion: cutscene, taget: \"@scene\" }] }",
            "unknown key `taget` (did you mean `target`?)",
        ),
        (
            "{ raises: [{ occasion: cutscene, target: \"@\" }] }",
            "target `@` is neither `@<attr>` nor a dotted target",
        ),
        (
            "{ raises: [{ occasion: cutscene, target: \"scene..x\" }] }",
            "target `scene..x` is neither",
        ),
        (
            "{ raises: [{ occasion: cutscene, target: 3 }] }",
            "`target: 3` is not a string",
        ),
    ] {
        let (_, load, _) = stage(Some(&play_scene(effects)), None);
        let got = load_faults(&load);
        assert_eq!(got.len(), 1, "{effects}: {got:?}");
        assert_eq!(got[0].0, "E-PLUGIN-PARSE", "{effects}: {got:?}");
        assert!(got[0].1.contains(needle), "{effects}: {got:?}");
        assert!(
            got[0].1.contains("a raise is `{ occasion: <occasion>"),
            "{got:?}"
        );
    }
}

#[test]
fn a_directive_raise_reads_a_declared_attribute() {
    let (_, load, _) = stage(
        Some(&play_scene(
            "{ raises: [{ occasion: cutscene, target: \"@scen\" }] }",
        )),
        None,
    );
    let got = load_faults(&load);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0].0, "E-PLUGIN-PARSE");
    assert!(
        got[0].1.contains(
            "directive `::playScene` effects.raises: `@scen` names no attribute of the directive \
             (declared: scene, label); did you mean `@scene`?"
        ),
        "{got:?}"
    );
    // At the raise's line.
    assert!(
        matches!(
            load[0],
            LoadError::Parse {
                at: Some((6, _)),
                ..
            }
        ),
        "{load:?}"
    );
}

#[test]
fn a_reward_kind_raise_reads_only_an_allowed_target() {
    let (_, load, _) = stage(
        None,
        Some(
            "rewardKinds:\n  SHOW: { effects: { raises: [{ occasion: cutscene, target: \"@scene\" }] } }\n",
        ),
    );
    let got = load_faults(&load);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0].0, "E-PLUGIN-PARSE");
    assert!(
        got[0]
            .1
            .contains("reward kind `SHOW` raises `cutscene` for `@scene`, but the kind declares no such attribute (declared: target)"),
        "{got:?}"
    );
    // 0.50's forbidden-target rule covers a raise.
    let (_, load, _) = stage(
        None,
        Some(
            "rewardKinds:\n  SHOW: { target: { forbidden: true }, effects: { raises: [{ occasion: \
             cutscene, target: \"@target\" }] } }\n",
        ),
    );
    let got = load_faults(&load);
    assert_eq!(got.len(), 1, "{got:?}");
    assert_eq!(got[0].0, "E-PLUGIN-PARSE");
    assert!(
        got[0].1.contains(
            "reward kind `SHOW` raises `cutscene` for `@target`, but the kind declares `target: { \
             forbidden: true }`"
        ),
        "{got:?}"
    );
}

#[test]
fn raised_by_takes_one_value() {
    for raised_by in ["host", "any", "Declared"] {
        let occasions = format!("occasions:\n  cutscene: {{ raisedBy: {raised_by} }}\n");
        let (_, load, _) = assemble(
            &[Pkg {
                id: "acme.stage",
                occasions: Some(&occasions),
                ..Default::default()
            }],
            &["acme.stage"],
        );
        let got = load_faults(&load);
        assert_eq!(got.len(), 1, "{raised_by}: {got:?}");
        assert_eq!(got[0].0, "E-PLUGIN-PARSE", "{got:?}");
        assert!(
            got[0].1.contains(&format!(
                "`occasions.cutscene.raisedBy` is `{raised_by}`, which is not one of declared"
            )),
            "{got:?}"
        );
    }
}

/// Review051b F4: a `lower:` directive has no carrier for raises.
#[test]
fn raises_on_a_lowered_directive_are_refused_and_dropped() {
    let directives =
        "directives:\n  - name: playVideo\n    attrs:\n      - { name: src, required: \
         true, type: string }\n    effects: { raises: [{ occasion: idle }] }\n    lower: { record: \
         video, fields: { assetId: { fromAttr: src } } }\n  - name: hop\n    effects: { raises: [{ \
         occasion: idle }] }\n    lower: { kind: builtin, name: jump }\n";
    let (snap, load, errs) = stage(Some(directives), None);
    assert!(load.is_empty(), "{load:?}");
    let got = assembly_faults(&errs);
    assert_eq!(got.len(), 2, "{got:?}");
    assert!(
        got.iter().all(|(c, _)| *c == "E-PLUGIN-INVALID-DIRECTIVE"),
        "{got:?}"
    );
    assert!(
        got.iter().any(|(_, m)| m
            == "directive `::playVideo` lowers to the `video` record, which carries no effects \
                (only a `plugin` record does), so its `effects.raises` could never reach the host \
                — drop `lower:` so the call stays a `plugin` record, or raise the occasion from a \
                passthrough directive"),
        "{got:?}"
    );
    assert!(
        got.iter()
            .any(|(_, m)| m.starts_with("directive `::hop` lowers through the builtin `jump` hook")),
        "{got:?}"
    );
    for tag in ["playVideo", "hop"] {
        let effects = snap.directives[tag].effects.as_ref().unwrap();
        assert!(effects.raises.is_empty(), "{tag}: the raise never lands");
    }
}

/// Every §4.2 assembly fault: `E-PLUGIN-RAISES`, the entry dropped and its
/// siblings kept.
#[test]
fn assembly_judges_each_raise_against_the_snapshot() {
    for (raise, message) in [
        (
            "{ occasion: cutscne, target: \"@scene\" }",
            "directive `::playScene` (`acme.stage`) raises occasion `cutscne`, which no active \
             plugin declares (a plugin raising another plugin's occasion lists it in `depends:`) \
             — did you mean `cutscene`?",
        ),
        (
            "{ occasion: cutscene }",
            "directive `::playScene` (`acme.stage`) raises `cutscene` without a target, but \
             `cutscene` is raised for a target (`scene.<cutsceneId>`) — add `target: \"@<attr>\"`",
        ),
        (
            "{ occasion: idle, target: \"@scene\" }",
            "directive `::playScene` (`acme.stage`) raises `idle` for `@scene`, but `idle` is \
             raised for no target — drop the raise's `target:`",
        ),
        (
            "{ occasion: cutscene, target: \"@label\" }",
            "directive `::playScene` (`acme.stage`) raises `cutscene` for `@label`, which is \
             typed `string`; the target of `cutscene` is a `cutsceneId` member, so type the \
             attribute `{ entity: cutsceneId }`",
        ),
        (
            "{ occasion: cutscene, target: room.office }",
            "directive `::playScene` (`acme.stage`) raises `cutscene` for `room.office`, but its \
             targets are `scene.<cutsceneId>`",
        ),
        (
            "{ occasion: finale, target: scene.chapter2 }",
            "directive `::playScene` (`acme.stage`) raises `finale` for `scene.chapter2`, but its \
             targets are `scene.ending`, `scene.epilogue`",
        ),
    ] {
        let effects = format!("{{ raises: [{raise}, {{ occasion: idle }}] }}");
        let (snap, load, errs) = stage(Some(&play_scene(&effects)), None);
        assert!(load.is_empty(), "{raise}: {load:?}");
        let got = assembly_faults(&errs);
        assert_eq!(
            got,
            vec![("E-PLUGIN-RAISES", message.to_string())],
            "{raise}"
        );
        let kept = &snap.directives["playScene"]
            .effects
            .as_ref()
            .unwrap()
            .raises;
        assert_eq!(
            kept,
            &vec![RaiseDecl {
                occasion: "idle".into(),
                target: None
            }],
            "{raise}: the refused entry is dropped, its sibling lands"
        );
    }
}

#[test]
fn a_reward_kind_raise_is_judged_by_its_target_contract() {
    for (kind, message) in [
        (
            "{ target: { provider: scenes }, effects: { raises: [{ occasion: cutscene, target: \
             \"@target\" }] } }",
            "reward kind `SHOW` (`acme.stage`) raises `cutscene` for `@target`, whose kind's \
             `target:` contract is `{ provider: scenes }`; the target of `cutscene` is a \
             `cutsceneId` member, so declare the kind's `target: { entity: cutsceneId }`",
        ),
        (
            "{ effects: { raises: [{ occasion: cutscene, target: \"@target\" }] } }",
            "reward kind `SHOW` (`acme.stage`) raises `cutscene` for `@target`, but the kind \
             declares no `target:` contract; the target of `cutscene` is a `cutsceneId` member, \
             so declare the kind's `target: { entity: cutsceneId }`",
        ),
        (
            "{ effects: { raises: [{ occasion: cutscene }] } }",
            "reward kind `SHOW` (`acme.stage`) raises `cutscene` without a target, but \
             `cutscene` is raised for a target (`scene.<cutsceneId>`) — add `target: \"@target\"`",
        ),
    ] {
        let kinds = format!(
            "rewardKinds:\n  SHOW: {kind}\n  KEEP: {{ effects: {{ raises: [{{ occasion: idle }}] }} }}\n"
        );
        let providers = "providers:\n  - { name: scenes, snapshot: scenes.json }\n";
        let scratch = lute_scratch::ScratchDir::new();
        let root = scratch.join("plugins");
        write_pkg(
            &root,
            &Pkg {
                id: "acme.stage",
                occasions: Some(OCCASIONS),
                reward_kinds: Some(&kinds),
                ..Default::default()
            },
        );
        let dir = root.join("acme.stage");
        fs::create_dir_all(dir.join("providers")).unwrap();
        fs::write(dir.join("providers/a.yaml"), providers).unwrap();
        let manifest = fs::read_to_string(dir.join("plugin.yaml")).unwrap();
        fs::write(
            dir.join("plugin.yaml"),
            format!("{manifest}  providers: providers/\n"),
        )
        .unwrap();
        let (installed, load) = load_plugins_dir(&root);
        assert!(load.is_empty(), "{kind}: {load:?}");
        let active = [
            ActivePlugin {
                id: "lute.core".into(),
                options: BTreeMap::new(),
            },
            ActivePlugin {
                id: "acme.stage".into(),
                options: BTreeMap::new(),
            },
        ];
        let (snap, errs) = assemble_snapshot(&active, &installed);
        assert_eq!(
            assembly_faults(&errs),
            vec![("E-PLUGIN-RAISES", message.to_string())],
            "{kind}"
        );
        assert!(snap.reward_kinds["SHOW"]
            .effects
            .as_ref()
            .unwrap()
            .raises
            .is_empty());
        assert_eq!(
            snap.reward_kinds["KEEP"]
                .effects
                .as_ref()
                .unwrap()
                .raises
                .len(),
            1
        );
    }
}

/// A plugin raising another plugin's occasion: found when that plugin is
/// active (in any merge order), named as installed-but-inactive when not.
#[test]
fn another_plugins_occasion_resolves_across_plugins() {
    let pkgs = [
        Pkg {
            id: "acme.stage",
            depends: &["acme.scenes"],
            directives: Some(
                "directives:\n  - name: cue\n    effects: { raises: [{ occasion: curtain }] }\n",
            ),
            ..Default::default()
        },
        Pkg {
            id: "acme.scenes",
            occasions: Some("occasions:\n  curtain: { raisedBy: declared }\n"),
            ..Default::default()
        },
    ];
    let (_, load, errs) = assemble(&pkgs, &["acme.stage", "acme.scenes"]);
    assert!(load.is_empty() && errs.is_empty(), "{load:?} {errs:?}");
    let (_, _, errs) = assemble(&pkgs, &["acme.stage"]);
    let got = assembly_faults(&errs);
    assert!(
        got.contains(&(
            "E-PLUGIN-RAISES",
            "directive `::cue` (`acme.stage`) raises occasion `curtain`, which no active plugin \
             declares: it is an occasion of `acme.scenes`, installed but not active in this \
             profile — declare `acme.scenes` in the `depends:` of `acme.stage`, so that the \
             occasion is in every profile that activates `acme.stage`"
                .to_string()
        )),
        "{got:?}"
    );
}

/// `capabilitySnapshot` hashes these `Debug`s: a manifest without the new
/// keys prints exactly as on 0.50.0, so its hash is unchanged.
#[test]
fn blocks_without_the_new_keys_print_as_in_0_50() {
    let directive: DirectiveEffects =
        yaml_serde::from_str("writes: []\nasserts: [\"met(a)\"]\n").unwrap();
    assert_eq!(
        format!("{directive:?}"),
        "DirectiveEffects { writes: [], asserts: [\"met(a)\"] }"
    );
    let reward: RewardEffects = yaml_serde::from_str("accepts: [\"@target\"]\n").unwrap();
    assert_eq!(
        format!("{reward:?}"),
        "RewardEffects { accepts: [\"@target\"] }"
    );
    let kind = RewardKindDecl {
        name: "OFFER".into(),
        effects: Some(reward),
        ..Default::default()
    };
    assert_eq!(
        format!("{kind:?}"),
        "RewardKindDecl { name: \"OFFER\", target: None, attrs: [], effects: RewardEffects { \
         accepts: [\"@target\"] } }"
    );
    let occasion = OccasionDecl {
        name: "cutscene".into(),
        target: OccasionTarget::Shape(true),
        ..Default::default()
    };
    assert_eq!(
        format!("{occasion:?}"),
        "OccasionDecl { name: \"cutscene\", select: First, target: true, description: None }"
    );
    // With the new keys, each prints them (so the hash moves).
    let directive: DirectiveEffects =
        yaml_serde::from_str("raises: [{ occasion: idle }]\n").unwrap();
    assert_eq!(
        format!("{directive:?}"),
        "DirectiveEffects { writes: [], raises: [RaiseDecl { occasion: \"idle\", target: None }] }"
    );
    let declared = OccasionDecl {
        raised_by: RaisedBy::Declared,
        ..occasion
    };
    assert!(format!("{declared:?}").ends_with(", raised_by: Declared }"));
}
