//! E2E artifact goldens (§8): real fixtures -> full-artifact snapshots +
//! structural invariants + byte determinism.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use lute_check::{CheckInput, Mode};
use lute_compile::compile;

/// Assemble the CheckInput exactly as `lute compile` (Task 13) does.
fn input_for(path: &str, project_dir: Option<&str>) -> CheckInput {
    let file = Path::new(path);
    let text = std::fs::read_to_string(file).unwrap();
    let project = project_dir
        .and_then(|d| lute_manifest::project::load_project(Path::new(d)).expect("project loads"));
    let providers = lute_manifest::project::project_providers(project.as_ref());
    let (doc, _) = lute_syntax::parse(&text);
    let (meta0, _) = lute_check::parse_meta(
        &doc.meta,
        &lute_manifest::snapshot::CapabilitySnapshot::default(),
    );
    let (snapshot, _) = lute_manifest::project::resolve_document_snapshot(
        project.as_ref(),
        meta0.profile.as_deref(),
        &meta0.plugins,
    );
    let base = file.parent().unwrap_or_else(|| Path::new("."));
    let imports = lute_check::resolve_imports(base, &meta0.uses, &meta0.extends, doc.meta.span);
    let components = lute_check::resolve_components(base, &meta0.components, doc.meta.span);
    CheckInput {
        text,
        uri: path.to_string(),
        snapshot,
        providers,
        mode: Mode::Ci,
        imports,
        components,
        defaults: Default::default(),
    }
}

/// Structural invariants every artifact must satisfy (§4, §7): unique ordered
/// addrs; every control-flow target resolves to a real record OR a shot's
/// defined one-past-end converge slot; +100 gapping; id discipline; and fully
/// expanded, `@`/`$`-free CEL in every CEL-bearing field.
fn assert_artifact_invariants(json: &serde_json::Value) {
    let commands = json["commands"].as_array().expect("commands array");
    // 0.3.0 T14: every emitted assert/retract delta's relation must resolve
    // in the artifact's OWN emitted relational schema (§4/§5) — a dangling
    // relation name would mean the checker's write-policy gate (fact_write.rs)
    // and the compiler's schema emission (T13) disagree.
    let relation_names: BTreeSet<&str> = json["relations"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|r| r["name"].as_str())
        .collect();
    let mut addrs: Vec<&str> = Vec::new();
    for c in commands {
        addrs.push(c["addr"].as_str().expect("every record has addr"));
    }
    let unique: BTreeSet<&str> = addrs.iter().copied().collect();
    assert_eq!(unique.len(), addrs.len(), "addrs unique");
    let mut sorted = addrs.clone();
    sorted.sort();
    assert_eq!(sorted, addrs, "addrs strictly ascending");
    // 0.8.0 (§4.2, adoption G2): ONE addr width per ARTIFACT. This is what
    // makes the ascending assertion above a real ordering guarantee instead
    // of an accident of every shot holding <100 records — a 5-digit index
    // beside 4-digit siblings sorts `001-10000` before `001-1100`.
    let widths: BTreeSet<usize> = addrs.iter().map(|a| a.len()).collect();
    assert!(
        widths.len() <= 1,
        "every addr in one artifact shares a width, got {widths:?}"
    );

    // Valid control-flow destinations = every real record addr PLUS each shot's
    // one-past-end converge slot. Addressing (§5.6) assigns record `i` the
    // addr `{shot}-{(i+1)*100}` at the artifact's uniform width, and the
    // end-of-shot converge one slot past the last record, i.e.
    // `max idx in shot + 100`. A dangling addr resolves to neither and must
    // fail.
    let mut valid: BTreeSet<String> = addrs.iter().map(|a| a.to_string()).collect();
    // Width is read off the addrs rather than hardcoded, so the converge slot
    // is reconstructed correctly for a wide artifact too.
    let mut shot_max: BTreeMap<&str, (u32, usize)> = BTreeMap::new();
    for a in &addrs {
        let (shot, idx) = a.rsplit_once('-').expect("addr shaped `shot-idx`");
        let width = idx.len();
        let idx: u32 = idx.parse().expect("addr idx is numeric");
        let slot = shot_max.entry(shot).or_insert((0, width));
        slot.0 = slot.0.max(idx);
    }
    for (shot, (max, width)) in &shot_max {
        valid.insert(format!("{shot}-{:0width$}", max + 100, width = width));
    }

    for c in commands {
        for key in ["target", "converge", "otherwise"] {
            if let Some(t) = c[key].as_str() {
                assert_target(t, &valid);
            }
        }
        for arm in c["arms"].as_array().into_iter().flatten() {
            assert_target(arm["target"].as_str().unwrap(), &valid);
            assert_cel_clean("match.arm.test", arm["test"].as_str().unwrap());
        }
        for opt in c["options"].as_array().into_iter().flatten() {
            assert_target(opt["target"].as_str().unwrap(), &valid);
            assert!(opt["lineId"].as_str().is_some_and(|s| !s.is_empty()));
            if let Some(when) = opt["when"].as_str() {
                assert_cel_clean("choice.option.when", when);
            }
        }
        match c["kind"].as_str() {
            Some("line") => {
                assert!(c["lineId"].as_str().is_some_and(|s| !s.is_empty()));
                let voiced = matches!(
                    c["role"].as_str(),
                    Some("dialogue" | "voiceover" | "offscreen")
                );
                assert_eq!(
                    c["voiceKey"].is_string(),
                    voiced,
                    "voiceKey iff voiced: {c}"
                );
                assert!(c.get("code").is_none(), "no standalone code field (§4.2)");
            }
            Some("match") => assert_cel_clean("match.subject", c["subject"].as_str().unwrap()),
            Some("set") => assert_cel_clean("set.value", c["value"].as_str().unwrap()),
            // dsl 0.2.0 quest/on records (IR addendum §3): `start`/`fail`/
            // `done`/`when` are `{raw, expr}` CEL pairs — clean; each
            // objective/on `body` target resolves like any other
            // control-flow target (same `valid` set, quest-indexed units).
            Some("quest") => {
                if let Some(start) = c["start"]["raw"].as_str() {
                    assert_cel_clean("quest.start", start);
                }
                if let Some(fail) = c["fail"]["raw"].as_str() {
                    assert_cel_clean("quest.fail", fail);
                }
                for obj in c["objectives"].as_array().into_iter().flatten() {
                    assert_cel_clean("objective.done", obj["done"]["raw"].as_str().unwrap());
                    if let Some(when) = obj["when"]["raw"].as_str() {
                        assert_cel_clean("objective.when", when);
                    }
                    if let Some(body) = obj["body"].as_str() {
                        assert_target(body, &valid);
                    }
                }
            }
            Some("on") => {
                if let Some(when) = c["when"]["raw"].as_str() {
                    assert_cel_clean("on.when", when);
                }
                assert_target(c["body"].as_str().expect("on.body"), &valid);
            }
            Some("assert") | Some("retract") => {
                let rel = c["relation"].as_str().expect("assert/retract has relation");
                assert!(
                    relation_names.contains(rel),
                    "assert/retract relation {rel:?} must resolve in the emitted relations schema"
                );
            }
            _ => {}
        }
    }
    // Retired identifier must not exist anywhere (§4.2).
    assert!(!json.to_string().contains("textUnitId"));
}

/// A control-flow target must be a concrete addr that resolves to a real record
/// or a shot's one-past-end converge — never an un-lowered symbolic label.
fn assert_target(t: &str, valid: &BTreeSet<String>) {
    assert!(!t.starts_with('@'), "unresolved symbolic target {t}");
    assert!(
        valid.contains(t),
        "dangling control-flow target {t}: resolves to no record or one-past-end converge"
    );
}

/// CEL fields must be fully expanded (D4): no `@def`/`@fn` refs and no `$`
/// subject sigils survive into the artifact.
fn assert_cel_clean(field: &str, cel: &str) {
    assert!(
        !cel.contains('@') && !cel.contains('$'),
        "unexpanded DSL token in {field}: {cel:?}"
    );
}

fn golden(name: &str, path: &str, project: Option<&str>) {
    let input = input_for(path, project);
    let artifact = compile(&input).unwrap_or_else(|e| panic!("{path} compiles: {e:#?}"));
    let mut json = serde_json::to_string_pretty(&artifact).unwrap();
    json.push('\n');
    assert_artifact_invariants(&serde_json::from_str(&json).unwrap());
    // Determinism (§8): same input => byte-identical artifact.
    let again = compile(&input).expect("recompiles");
    let mut json2 = serde_json::to_string_pretty(&again).unwrap();
    json2.push('\n');
    assert_eq!(json, json2, "byte-stable across compiles");
    insta::assert_snapshot!(name, json);
}

#[test]
fn bianca_s01ep02() {
    golden(
        "bianca_s01ep02",
        "../../docs/examples/bianca-s01ep02.lute",
        None,
    );
}

#[test]
fn showcase_episode01() {
    golden(
        "showcase_episode01",
        "../../docs/examples/showcase/episode01.lute",
        Some("../../docs/examples/showcase"),
    );
}

#[test]
fn quest_grove() {
    golden("quest_grove", "../../docs/examples/quest-grove.lute", None);
}

#[test]
fn quest_rescue_halsin() {
    golden(
        "quest_rescue_halsin",
        "../../docs/examples/quest-rescue-halsin.lute",
        None,
    );
}

/// 2026-08-31 subquest design end-to-end: `<objective quest=child>` in a
/// parent with an AUTHORED `fail` exercises every branch of the synthesis
/// pass in one artifact — the snapshot is the load-bearing wire contract
/// `lute-trace` and any engine consumer reads. Verifies:
///
///  - §2.1 `done` synthesis into three empty subquest slots (single-quoted
///    `quest.<child>.state == 'complete'`, exact bytes);
///  - §2.2 `fail` extension: authored `run.npc.halsin.dead` wrapped in
///    parens, then required children (`findHalsin`, `stopRitual`) in
///    document order, `||`-joined;
///  - the OPTIONAL subquest child (`scoutPerimeter`) contributes nothing
///    to `fail` yet still serializes its `quest` field on the objective;
///  - a plain sibling objective (`talkRath`) with an authored `done` mixes
///    freely (design §1) and its `ObjectiveEntry.quest` stays omitted
///    (`skip_serializing_if`, IR §3);
///  - the referenced child quests declared in the SAME file receive no
///    synthesis of their own (their own objectives are plain `done=`).
#[test]
fn quest_subquest() {
    golden(
        "quest_subquest",
        "../../docs/examples/quest-subquest.lute",
        None,
    );
}

/// dsl 0.16.0 §2/§3 (Task 3): a reward-carrying quest lowered end-to-end.
/// The load-bearing snapshot pins the exact wire — declaration-order
/// `rewards[]` on both `QuestCmd` and `ObjectiveEntry`, `amountMin`/
/// `amountMax` for a range, `amount` default `1` for an unauthored scalar,
/// `on="failed"` only on the quest entry, `when.raw` verbatim. Task 4/5
/// (vocabulary + runtime grant events) read this exact shape.
#[test]
fn quest_rewards() {
    golden("quest_rewards", "tests/fixtures/quest_rewards.lute", None);
}

/// IR A12: the `::serve` plugin record carries resolved effect bindings. The
/// `fromAttr` template (`resultKey="debut"`) is substituted into each path at
/// compile time; `from` is the bridge-result key or the `op`/`by` increment,
/// with `by` collapsed to the JSON integer `1` (not `1.0`). This proves end-to-
/// end resolution + attr substitution + integral collapse independent of the
/// full golden.
#[test]
fn plugin_record_carries_resolved_effects() {
    let input = input_for(
        "../../docs/examples/showcase/episode01.lute",
        Some("../../docs/examples/showcase"),
    );
    let artifact = compile(&input).expect("compiles");
    let json: serde_json::Value =
        serde_json::from_str(&serde_json::to_string(&artifact).unwrap()).unwrap();
    let serve = json["commands"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["kind"] == "plugin" && c["tag"] == "serve")
        .expect("serve plugin record present");
    let effects = serve["effects"]
        .as_array()
        .expect("serve record carries an effects array");
    assert_eq!(effects.len(), 4, "serve declares 4 writes");

    // `fromAttr` (resultKey="debut") substituted + increment with integral `by`.
    let attempts = effects
        .iter()
        .find(|e| e["path"] == "scene.serve.debut.attempts")
        .expect("attempts write resolved with debut key");
    assert_eq!(
        attempts["from"],
        serde_json::json!({ "op": "increment", "by": 1 }),
        "increment `by` serializes as the JSON integer 1"
    );
    assert!(
        attempts["from"]["by"].is_i64(),
        "`by` is an integer, not 1.0"
    );

    // A bridge-result write, path likewise resolved through `debut`.
    let rank = effects
        .iter()
        .find(|e| e["path"] == "scene.serve.debut.rank")
        .expect("rank write resolved with debut key");
    assert_eq!(rank["from"], serde_json::json!({ "bridgeResult": "rank" }));
}

#[test]
fn components_scene() {
    golden(
        "components_scene",
        "../../docs/examples/components/scene.lute",
        None,
    );
}

/// dsl 0.4.0 §6.4/§6.5 (T8): the deduplicated affinity-reaction worked
/// example — three literal `::use{tier="…"}` sites (§6.4 case 1: fold to
/// the selected `@bianca` line, zero match records) plus one def-bound
/// `::use{tier=@currentTier}` site (§6.4 case 2: an ordinary residual
/// `MatchCmd` on the substituted subject). B2: the caller's OWN
/// `<match on="scene.affect.bianca">` is a scene-level match — untouched by
/// this fold either way.
#[test]
fn affinity_reaction() {
    golden(
        "affinity_reaction",
        "../../docs/examples/affinity-reaction.lute",
        None,
    );
}

/// dsl 0.4.0 §7.2/§7.4 (T11): the `when=` gated-line sugar worked example
/// — a sugared content line (shot 1) plus its hand-written explicit-match
/// twin (shot 2). Proves the desugar end-to-end through the SAME artifact-
/// invariants + determinism gate every other golden runs through (a
/// synthesized match record is real IR, not a special case).
#[test]
fn gated_line() {
    golden("gated_line", "../../docs/examples/gated-line.lute", None);
}

/// Connectivity T15 grounding: the FIRST corpus example declaring `after`
/// (dsl connectivity spec §2.2/§4.4/§7) -- `<quest after="visited('kestrel.s01ep01')">`
/// routes through connected-intro.lute's canonical key. Proves T13's
/// `prereqEdges` IR emission end-to-end against a REAL, otherwise-ordinary
/// worked example (not just the synthetic unit fixtures in
/// `lute-compile/src/lib.rs`'s own test module) -- the golden's
/// `prereqEdges` entry carries the raw `after` text verbatim.
#[test]
fn connected_quest() {
    golden(
        "connected_quest",
        "../../docs/examples/connected-quest.lute",
        None,
    );
}

/// The strengthened resolvability check must REJECT a dangling target — proving
/// it is a genuine graph proof, not an "any 8-char string" shape check.
#[test]
fn dangling_target_fails_the_checker() {
    let artifact = serde_json::json!({
        "commands": [
            {
                "kind": "line", "addr": "001-0100", "role": "dialogue",
                "speaker": "x", "text": "hi", "lineId": "x", "voiceKey": "v"
            },
            { "kind": "jump", "addr": "001-0200", "target": "999-9999" }
        ]
    });
    let caught = std::panic::catch_unwind(|| assert_artifact_invariants(&artifact));
    assert!(
        caught.is_err(),
        "dangling control-flow target must fail the checker"
    );
}

/// The strengthened CEL check must REJECT an unexpanded `$`/`@` DSL token in any
/// CEL-bearing field (here `set.value`).
#[test]
fn unexpanded_cel_token_fails_the_checker() {
    let artifact = serde_json::json!({
        "commands": [
            {
                "kind": "set", "addr": "001-0100",
                "path": "scene.x", "op": "=", "value": "$ + 1"
            }
        ]
    });
    let caught = std::panic::catch_unwind(|| assert_artifact_invariants(&artifact));
    assert!(
        caught.is_err(),
        "unexpanded `$` in set.value must fail the checker"
    );
}

/// 0.3.0 T14: the relation-resolves-in-schema check must REJECT an assert/
/// retract command whose `relation` is absent from the artifact's own
/// emitted `relations` schema — proving the check is a genuine schema
/// cross-reference, not a no-op.
#[test]
fn assert_relation_missing_from_schema_fails_the_checker() {
    let artifact = serde_json::json!({
        "commands": [
            { "kind": "assert", "addr": "001-0100", "relation": "ghost", "args": ["ana"] }
        ],
        "relations": [
            { "name": "inParty", "args": ["c"], "derive": false, "reserved": false }
        ]
    });
    let caught = std::panic::catch_unwind(|| assert_artifact_invariants(&artifact));
    assert!(
        caught.is_err(),
        "an assert relation absent from the emitted schema must fail the checker"
    );
}
