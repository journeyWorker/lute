//! 0.52.0 §7 (#201): a reward kind's `attrs:` are authored on `<reward>`,
//! typed like directive attributes, read by the kind's `effects` as
//! `@<attr>`, compiled into `RewardEntry.attrs` and copied onto the `grant`
//! record.
//!
//! On v0.51.0 every kind attribute on a `<reward>` is `E-UNKNOWN-ATTR`, a
//! kind effect reading `@<attr>` fails the plugin's load, and no entry or
//! grant carries `attrs`, so each test below fails there.

use std::path::Path;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

const PROJECT: &str = "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    \
                       plugins:\n      acme.stage: true\ndefaults:\n  contentLang: en\n  uses: \
                       [schema/world.schema.yaml]\n";

const PLUGIN: &str = "id: acme.stage\nversion: 0.1.0\nkind: capability\ndepends:\n  - { id: \
                      lute.core, range: \"^0.0.1\" }\nexports:\n  occasions: occasions/\n  \
                      rewardKinds: rewardkinds/\n";

const OCCASIONS: &str = "occasions:\n  \
     cutscene: { target: { prefix: scene, entity: cutsceneId } }\n  \
     begin: {}\n";

/// `BADGE` is the #201 repro kind; the others type, require, default and
/// read their attributes.
const KINDS: &str = concat!(
    "rewardKinds:\n",
    "  BADGE: { attrs: [ { name: rarity, type: string } ] }\n",
    "  TYPED:\n",
    "    attrs:\n",
    "      - { name: tier, type: int }\n",
    "      - { name: grade, type: { enum: [bronze, silver, gold] } }\n",
    "      - { name: who, type: { entity: person } }\n",
    "      - { name: shiny, type: bool }\n",
    "  SEALED: { attrs: [ { name: seal, required: true, type: string } ] }\n",
    "  MEDAL:\n",
    "    attrs: [ { name: grade, type: { entity: grade }, default: bronze } ]\n",
    "    effects: { asserts: [\"medal(@grade)\"] }\n",
    "  SHOW:\n",
    "    attrs: [ { name: scene, type: { entity: cutsceneId } } ]\n",
    "    effects: { asserts: [\"seen(@scene)\"], raises: [{ occasion: cutscene, target: \"@scene\" }] }\n",
);

const SCHEMA: &str = "entities:\n  person: { members: [mira, theo] }\n  grade: { members: \
                      [bronze, silver, gold] }\n  cutsceneId: { members: [intro, chapter2] }\n\
                      relations:\n  medal: { args: [grade], tier: run }\n  seen: { args: \
                      [cutsceneId], tier: run }\nstate:\n  run.flag: { type: bool, default: \
                      false }\ndefs:\n  rewarded: { type: bool, cel: \"holds('seen', \
                      ['chapter2']) || holds('medal', ['gold'])\", host: true }\n";

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
}

/// A project with the kinds above (`kinds` replaces them when given), a
/// `chapter2` scene answering `cutscene`, an `intro` scene run on `begin`,
/// and `quests` as its quest document (`quests/q.lute`).
fn project(tag: &str, kinds: Option<&str>, quests: &str) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(&dir, "lute.project.yaml", PROJECT);
    write(&dir, "plugins/acme.stage/plugin.yaml", PLUGIN);
    write(&dir, "plugins/acme.stage/occasions/o.yaml", OCCASIONS);
    write(
        &dir,
        "plugins/acme.stage/rewardkinds/r.yaml",
        kinds.unwrap_or(KINDS),
    );
    write(&dir, "schema/world.schema.yaml", SCHEMA);
    write(
        &dir,
        "scenes/intro.lute",
        "---\nkind: scene\nid: intro\non: begin\n---\n\n## Shot 1.\n\n@narrator: Intro.\n",
    );
    write(
        &dir,
        "scenes/chapter2.lute",
        "---\nkind: scene\nid: chapter2\non: cutscene\ntarget: scene.chapter2\n---\n\n\
         ## Shot 1.\n\n@narrator: Chapter two.\n",
    );
    write(
        &dir,
        "quests/q.lute",
        &format!("---\nkind: quest\nid: acme.quests\n---\n\n{quests}"),
    );
    dir
}

/// A quest started by `intro` whose one objective is done at once, with
/// `rewards` (quest-level `<reward>` lines).
fn quest(rewards: &str) -> String {
    format!(
        "<quest id=\"first\" tier=\"run\" title=\"First\" start=\"visited('intro')\">\n  \
         <objective id=\"o\" title=\"Do it\" done=\"true\"/>\n  {rewards}\n</quest>\n"
    )
}

fn lute(dir: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .env("LUTE_TELEMETRY", "0")
        .output()
        .unwrap()
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn json(s: &str) -> serde_json::Value {
    serde_json::from_str(s).unwrap_or_else(|e| panic!("{e}: {s}"))
}

/// Every diagnostic line of `lute check --project .`, as `file:line:col
/// [CODE] message` (the severity dropped).
fn check(dir: &Path) -> Vec<String> {
    text(&lute(dir, &["check", "--project", "."]))
        .lines()
        .filter_map(|l| {
            let l = l.strip_prefix("./")?;
            let (at, rest) = l.split_once(": ")?;
            let (_, d) = rest.split_once(' ')?;
            d.starts_with('[').then(|| format!("{at} {d}"))
        })
        .collect()
}

/// The compiled quest-level rewards of `quests/q.lute`.
fn rewards(dir: &Path) -> serde_json::Value {
    let out = lute(dir, &["compile", "quests/q.lute"]);
    assert!(out.status.success(), "{}", text(&out));
    let ir = json(&String::from_utf8_lossy(&out.stdout));
    let quest = ir["commands"]
        .as_array()
        .into_iter()
        .flatten()
        .find(|c| c["kind"] == "quest")
        .cloned()
        .expect("a quest record");
    quest["rewards"].clone()
}

/// The `grant` records of `lute play --json` raising `begin` once.
fn grants(dir: &Path) -> Vec<serde_json::Value> {
    write(dir, "p.play.yaml", "steps:\n  - { occasion: begin }\n");
    let out = lute(
        dir,
        &[
            "play",
            "--project",
            ".",
            "--script",
            "p.play.yaml",
            "--json",
        ],
    );
    assert!(out.status.success(), "{}", text(&out));
    let v = json(&String::from_utf8_lossy(&out.stdout));
    let mut out = Vec::new();
    for step in v["steps"].as_array().into_iter().flatten() {
        for q in step["quests"].as_array().into_iter().flatten() {
            out.extend(
                q["commands"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter(|c| c["kind"] == "grant")
                    .cloned(),
            );
        }
    }
    out
}

/// The #201 repro: `rarity="gold"` checks clean, compiles into the entry's
/// `attrs` and rides on the grant record.
#[test]
fn the_issue_repro_checks_compiles_and_grants_its_attribute() {
    let dir = project(
        "rattrs-repro",
        None,
        &quest("<reward kind=\"BADGE\" amount=\"1\" rarity=\"gold\"/>"),
    );
    assert_eq!(check(&dir), Vec::<String>::new());
    assert_eq!(
        rewards(&dir),
        json(r#"[{"kind":"BADGE","amount":1,"attrs":{"rarity":"gold"}}]"#)
    );
    let grants = grants(&dir);
    assert_eq!(grants.len(), 1, "{grants:?}");
    assert_eq!(
        grants[0]["reward"],
        json(r#"{"kind":"BADGE","amount":1,"attrs":{"rarity":"gold"}}"#)
    );
}

/// A reward that writes no kind attribute compiles and grants as before:
/// no `attrs` key at all.
#[test]
fn a_reward_without_kind_attributes_carries_no_attrs() {
    let dir = project("rattrs-none", None, &quest("<reward kind=\"BADGE\"/>"));
    assert_eq!(check(&dir), Vec::<String>::new());
    assert_eq!(rewards(&dir), json(r#"[{"kind":"BADGE","amount":1}]"#));
    assert_eq!(
        grants(&dir)[0]["reward"],
        json(r#"{"kind":"BADGE","amount":1}"#)
    );
}

/// Typed like directive attributes: `int`, `enum`, entity and `bool`
/// values compile typed; wrong ones draw the directive codes, with the
/// owner `<reward kind="TYPED">`.
#[test]
fn kind_attributes_are_typed_like_directive_attributes() {
    let dir = project(
        "rattrs-typed-ok",
        None,
        &quest("<reward kind=\"TYPED\" tier=\"2\" grade=\"gold\" who=\"mira\" shiny/>"),
    );
    assert_eq!(check(&dir), Vec::<String>::new());
    assert_eq!(
        rewards(&dir)[0]["attrs"],
        json(r#"{"grade":"gold","shiny":true,"tier":2,"who":"mira"}"#)
    );
    let dir = project(
        "rattrs-typed-bad",
        None,
        &quest("<reward kind=\"TYPED\" tier=\"two\" grade=\"platinum\" who=\"zed\"/>"),
    );
    let diags = check(&dir);
    let codes: Vec<&str> = diags
        .iter()
        .filter_map(|d| d.split_once('[')?.1.split_once(']').map(|(c, _)| c))
        .collect();
    assert_eq!(
        codes,
        ["E-ATTR-TYPE", "E-BAD-ENUM", "E-BAD-ENUM"],
        "{diags:#?}"
    );
    assert!(
        diags[0].contains("attribute `tier` of `<reward kind=\"TYPED\">` expects"),
        "{diags:#?}"
    );
    assert!(diags[1].contains("`platinum`"), "{diags:#?}");
    assert!(
        diags[2].contains("`zed` is not a member of entity kind `person`"),
        "{diags:#?}"
    );
}

/// A `required: true` kind attribute must be written; a declared one
/// written as an `@ref` is `E-REWARD-ATTR` (a reward is static data).
#[test]
fn a_required_attribute_and_an_at_ref_value_are_refused() {
    let dir = project("rattrs-required", None, &quest("<reward kind=\"SEALED\"/>"));
    assert_eq!(
        check(&dir),
        [
            "quests/q.lute:8:3 [E-MISSING-ATTR] `<reward kind=\"SEALED\">` requires attribute \
             `seal`"
        ]
    );
    let dir = project(
        "rattrs-ref",
        None,
        &quest("<reward kind=\"BADGE\" rarity=@r/>"),
    );
    assert_eq!(
        check(&dir),
        [
            "quests/q.lute:8:31 [E-REWARD-ATTR] `<reward rarity=@r>`: a reward's attributes are \
             static data the engine grants against — write a quoted value (`rarity=\"…\"`)"
        ]
    );
}

/// `E-UNKNOWN-ATTR` on a `<reward>` of a kind with attributes names them,
/// with the did-you-mean over both lists; a kind without any keeps the
/// 0.51 message.
#[test]
fn an_unknown_attribute_lists_the_kind_attributes() {
    let dir = project(
        "rattrs-unknown",
        None,
        &quest("<reward kind=\"BADGE\" rarety=\"gold\"/>\n  <reward kind=\"SEALED\" seal=\"x\" bogus=\"1\"/>"),
    );
    assert_eq!(
        check(&dir),
        [
            "quests/q.lute:8:24 [E-UNKNOWN-ATTR] `<reward>` has no attribute `rarety` — did you \
             mean `rarity`?; its attributes are `id`, `kind`, `target`, `amount`, `when`, \
             `outcome`, and kind `BADGE` declares `rarity`",
            "quests/q.lute:9:34 [E-UNKNOWN-ATTR] `<reward>` has no attribute `bogus`; its \
             attributes are `id`, `kind`, `target`, `amount`, `when`, `outcome`, and kind \
             `SEALED` declares `seal`",
        ]
    );
    let plain = "rewardKinds:\n  COINS: {}\n";
    let dir = project(
        "rattrs-unknown-plain",
        Some(plain),
        &quest("<reward kind=\"COINS\" rarity=\"gold\"/>"),
    );
    // (Without `TYPED`, `person` is read by nothing: `W-DOMAIN-UNREAD`.)
    let quest_diags: Vec<String> = check(&dir)
        .into_iter()
        .filter(|d| d.starts_with("quests/"))
        .collect();
    assert_eq!(
        quest_diags,
        [
            "quests/q.lute:8:24 [E-UNKNOWN-ATTR] `<reward>` has no attribute `rarity`; its \
             attributes are `id`, `kind`, `target`, `amount`, `when`, `outcome`"
        ]
    );
}

/// A declared `default:` ships in `attrs` and binds the kind's effects; an
/// authored value wins.
#[test]
fn a_default_ships_in_attrs_and_binds_the_effects() {
    let dir = project(
        "rattrs-default",
        None,
        &quest("<reward kind=\"MEDAL\"/>\n  <reward kind=\"MEDAL\" grade=\"gold\"/>"),
    );
    assert_eq!(check(&dir), Vec::<String>::new());
    assert_eq!(
        rewards(&dir),
        json(
            r#"[{"kind":"MEDAL","amount":1,"asserts":[{"relation":"medal","args":["bronze"]}],"attrs":{"grade":"bronze"}},
                {"kind":"MEDAL","amount":1,"asserts":[{"relation":"medal","args":["gold"]}],"attrs":{"grade":"gold"}}]"#
        )
    );
}

/// Facts and a raise reading `@scene`: bound, both apply (the fact
/// produced, the raise on the grant record); unbound, neither does.
#[test]
fn facts_and_raises_read_a_kind_attribute_bound_and_unbound() {
    let dir = project(
        "rattrs-bound",
        None,
        &quest("<reward kind=\"SHOW\" scene=\"chapter2\"/>"),
    );
    assert_eq!(check(&dir), Vec::<String>::new());
    assert_eq!(
        rewards(&dir),
        json(
            r#"[{"kind":"SHOW","amount":1,"asserts":[{"relation":"seen","args":["chapter2"]}],
                 "raises":[{"occasion":"cutscene","target":"scene.chapter2"}],"attrs":{"scene":"chapter2"}}]"#
        )
    );
    let bound = grants(&dir);
    assert_eq!(
        bound[0]["raises"],
        json(r#"[{"occasion":"cutscene","target":"scene.chapter2"}]"#),
        "{bound:?}"
    );
    let dir = project("rattrs-unbound", None, &quest("<reward kind=\"SHOW\"/>"));
    assert_eq!(rewards(&dir), json(r#"[{"kind":"SHOW","amount":1}]"#));
    assert!(grants(&dir)[0].get("raises").is_none());
}

/// A bound raise target outside the occasion's answerers is judged where
/// the value is written (`E-RAISES-TARGET` at `scene="intro"`).
#[test]
fn a_bound_raise_target_is_judged_at_the_attribute() {
    let dir = project(
        "rattrs-raise-target",
        None,
        &quest("<reward kind=\"SHOW\" scene=\"intro\"/>"),
    );
    let diags = check(&dir);
    assert_eq!(diags.len(), 1, "{diags:#?}");
    assert!(
        diags[0].starts_with("quests/q.lute:8:30 [E-RAISES-TARGET]"),
        "{diags:#?}"
    );
}

/// A kind attribute declaration no `<reward>` could author fails the
/// plugin's load (`E-PLUGIN-PARSE` at the entry).
#[test]
fn a_faulty_declaration_fails_the_load() {
    let dir = project(
        "rattrs-load",
        Some("rewardKinds:\n  BADGE:\n    attrs:\n      - { name: amount, type: int }\n"),
        &quest("<reward kind=\"BADGE\"/>"),
    );
    let diags = check(&dir);
    assert!(
        diags.iter().any(|d| d.contains("[E-PLUGIN-PARSE]")
            && d.contains(
                "reward kind `BADGE` declares attribute `amount`, which `<reward>` already has"
            )),
        "{diags:#?}"
    );
}
