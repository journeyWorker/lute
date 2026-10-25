//! 0.50.0 §5.1–§5.4 (#169): a reward kind's `effects:` — the facts a grant
//! asserts and retracts and the quests it accepts — read by whole-project
//! `check` and `lute scenario` as if each effect were the corresponding
//! `::assert` / `::retract` / `::accept` written first in the reward's grant
//! point.
//!
//! On 0.49.1 the `effects:` key of a reward kind is `E-PLUGIN-KEY` and the
//! plugin fails to load (`E-PLUGIN-MISSING-ACTIVE`), so every test below
//! fails there: each asserts the project's exact diagnostics.

use std::path::Path;
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

const PROJECT: &str = "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    \
                       plugins:\n      acme.stage: true\ndefaults:\n  contentLang: en\n  uses: \
                       [schema/world.schema.yaml]\n";

const PLUGIN: &str = "id: acme.stage\nversion: 0.1.0\nkind: capability\ndepends:\n  - { id: \
                      lute.core, range: \"^0.0.1\" }\nexports:\n  rewardKinds: rewardkinds/\n";

const KINDS: &str = concat!(
    "rewardKinds:\n",
    "  BADGE: { target: { entity: person, required: true }, effects: { asserts: [\"met(@target)\"] } }\n",
    "  REVOKE: { target: { entity: person, required: true }, effects: { retracts: [\"met(@target)\"] } }\n",
    "  SWAP: { effects: { retracts: [\"met(mira)\"], asserts: [\"met(mira)\"] } }\n",
    "  OFFER_QUEST: { target: { required: true }, effects: { accepts: [\"@target\"] } }\n",
    "  OFFER_SECOND: { effects: { accepts: [second] } }\n",
    "  TAG: { effects: { asserts: [\"seen(@target)\"] } }\n",
    "  COINS: {}\n",
);

const SCHEMA: &str = "entities:\n  person: { members: [mira, theo] }\nrelations:\n  met: { \
                      args: [person], tier: user }\nstate:\n  run.flag: { type: bool, default: \
                      false }\n  run.other: { type: bool, default: false }\ncast:\n  mira: { \
                      name: Mira }\n";

fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, text).unwrap();
}

/// A project with the reward kinds above and `quests` as its one quest
/// document (`quests/q.lute`), plus `extra` files.
fn project(tag: &str, quests: &str, extra: &[(&str, &str)]) -> lute_scratch::ScratchDir {
    let dir = lute_scratch::ScratchDir::named(tag);
    write(&dir, "lute.project.yaml", PROJECT);
    write(&dir, "plugins/acme.stage/plugin.yaml", PLUGIN);
    write(&dir, "plugins/acme.stage/rewardkinds/r.yaml", KINDS);
    write(&dir, "schema/world.schema.yaml", SCHEMA);
    write(
        &dir,
        "quests/q.lute",
        &format!("---\nkind: quest\nid: acme.quests\n---\n\n{quests}"),
    );
    for (rel, text) in extra {
        write(&dir, rel, text);
    }
    dir
}

fn lute(dir: &Path, args: &[&str]) -> Output {
    Command::new(BIN)
        .args(args)
        .current_dir(dir)
        .env("LUTE_TELEMETRY", "0")
        .output()
        .unwrap()
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

/// Every diagnostic line of `lute check --project .`, as `file:line:col
/// [CODE] message` (the severity dropped).
fn check(dir: &Path) -> Vec<String> {
    let out = lute(dir, &["check", "--project", "."]);
    text(&out)
        .lines()
        .filter_map(|l| {
            let l = l.strip_prefix("./")?;
            let (at, rest) = l.split_once(": ")?;
            let (_, d) = rest.split_once(' ')?;
            d.starts_with('[').then(|| format!("{at} {d}"))
        })
        .collect()
}

fn codes(diags: &[String]) -> Vec<&str> {
    diags
        .iter()
        .filter_map(|d| d.split_once('[')?.1.split_once(']').map(|(c, _)| c))
        .collect()
}

/// A `<match>` testing `holds('met', ['mira'])` — the guard each must-set
/// test reads (`W-FACT-GUARANTEED` on it when the fact is in Must there).
const GUARD: &str = "<match>\n      <when test=\"holds('met', ['mira'])\">\n        \
                     ::set{run.other=true}\n      </when>\n      <otherwise>\n        \
                     ::set{run.other=false}\n      </otherwise>\n    </match>";

/// `<on event="questComplete">` holding [`GUARD`].
fn on_complete() -> String {
    format!("<on event=\"questComplete\">\n    {GUARD}\n  </on>")
}

/// The #169 repro: a `BADGE` reward is the only producer of `met(mira)`.
#[test]
fn a_reward_assert_produces_the_fact_a_start_reads() {
    let dir = project(
        "re-badge",
        "<quest id=\"firstVisit\" tier=\"user\" title=\"First visit\" start=\"true\">\n  \
         <objective id=\"c1\" title=\"Do it\" done=\"run.flag\"/>\n  \
         <reward id=\"r1\" kind=\"BADGE\" amount=\"1\" target=\"mira\"/>\n</quest>\n\n\
         <quest id=\"secondVisit\" tier=\"user\" title=\"Second visit\" \
         start=\"holds('met', ['mira'])\">\n  <objective id=\"c1\" title=\"Do it\" \
         done=\"run.flag\"/>\n</quest>\n",
        &[],
    );
    assert_eq!(check(&dir), Vec::<String>::new());
    let knowledge = text(&lute(&dir, &["scenario", "knowledge", "--project", "."]));
    assert!(
        knowledge.contains("met(mira) — produced by reward BADGE of quest `firstVisit`"),
        "{knowledge}"
    );
}

/// The #169 second repro: an `OFFER_QUEST` reward accepts `second`.
#[test]
fn a_reward_accept_draws_the_accept_edge_and_accepts_the_quest() {
    let dir = project(
        "re-offer",
        "<quest id=\"first\" tier=\"run\" title=\"First\" start=\"true\">\n  \
         <objective id=\"o\" title=\"Do it\" done=\"run.flag\"/>\n  \
         <reward id=\"r1\" kind=\"OFFER_QUEST\" amount=\"1\" target=\"second\"/>\n</quest>\n\n\
         <quest id=\"second\" tier=\"run\" title=\"Second\" activate=\"accept\">\n  \
         <objective id=\"o\" title=\"Do it\" done=\"run.flag\"/>\n</quest>\n",
        &[],
    );
    assert_eq!(check(&dir), Vec::<String>::new());
    let graph = text(&lute(&dir, &["scenario", "--project", "."]));
    assert!(
        graph.contains("quest(first) -> quest(second) [accept]"),
        "{graph}"
    );
}

#[test]
fn a_quest_no_accept_names_still_warns_with_the_new_wording() {
    let dir = project(
        "re-never",
        "<quest id=\"second\" tier=\"run\" title=\"Second\" activate=\"accept\">\n  \
         <objective id=\"o\" title=\"Do it\" done=\"run.flag\"/>\n</quest>\n",
        &[],
    );
    let got = check(&dir);
    assert_eq!(codes(&got), ["W-QUEST-NEVER-ACCEPTED"], "{got:?}");
    assert!(
        got[0].contains("but no `::accept` or reward `accepts` in the project names it"),
        "{got:?}"
    );
}

/// The three `E-ACCEPT-TARGET` faults of a reward's `effects.accepts`.
#[test]
fn reward_accepts_are_judged_like_accept() {
    let dir = project(
        "re-accept-faults",
        "<quest id=\"first\" tier=\"run\" title=\"First\" start=\"true\">\n  \
         <objective id=\"o\" title=\"Do it\" done=\"run.flag\"/>\n  \
         <reward kind=\"OFFER_QUEST\" amount=\"1\" target=\"secnd\"/>\n  \
         <reward kind=\"OFFER_QUEST\" amount=\"1\" target=\"started\"/>\n  \
         <reward kind=\"OFFER_QUEST\" amount=\"1\" target=\"child\"/>\n  \
         <reward kind=\"OFFER_SECOND\" amount=\"1\"/>\n</quest>\n\n\
         <quest id=\"second\" tier=\"run\" title=\"Second\" activate=\"accept\">\n  \
         <objective id=\"o\" title=\"Do it\" done=\"run.flag\"/>\n</quest>\n\n\
         <quest id=\"started\" tier=\"run\" title=\"Started\" start=\"run.flag\">\n  \
         <objective id=\"o\" title=\"Do it\" done=\"run.flag\"/>\n</quest>\n\n\
         <quest id=\"parent\" tier=\"run\" title=\"Parent\" start=\"true\">\n  \
         <objective id=\"sub\" title=\"Child\" quest=\"child\"/>\n</quest>\n\n\
         <quest id=\"child\" tier=\"run\" title=\"Child\">\n  \
         <objective id=\"o\" title=\"Do it\" done=\"run.flag\"/>\n</quest>\n",
        &[],
    );
    assert_eq!(
        check(&dir),
        [
            // Anchored at each `target=` value; the literal accept of
            // `OFFER_SECOND` names a valid quest.
            "quests/q.lute:8:49 [E-ACCEPT-TARGET] `<reward kind=\"OFFER_QUEST\" \
             target=\"secnd\">` accepts quest `secnd` (its kind's `effects.accepts`), but no \
             quest in the project declares that id — did you mean `second`?",
            "quests/q.lute:9:49 [E-ACCEPT-TARGET] `<reward kind=\"OFFER_QUEST\" \
             target=\"started\">` accepts quest `started` (its kind's `effects.accepts`), which \
             has a `start` condition and activates on its own; only an accept-driven quest (no \
             `start`) can be accepted",
            "quests/q.lute:10:49 [E-ACCEPT-TARGET] `<reward kind=\"OFFER_QUEST\" \
             target=\"child\">` accepts quest `child` (its kind's `effects.accepts`), which \
             activates with its parent `parent`; declare `activate=\"accept\"` on it to accept \
             it from a scene",
        ]
    );
}

/// Each resolved fact is judged as an `::assert` of it, at the reward's
/// `kind=`, prefixed with the reward.
#[test]
fn reward_facts_are_judged_like_assert() {
    let dir = project(
        "re-fact-faults",
        "<quest id=\"first\" tier=\"run\" title=\"First\" start=\"true\">\n  \
         <objective id=\"o\" title=\"Do it\" done=\"run.flag\"/>\n  \
         <reward kind=\"TAG\" amount=\"1\" target=\"mira\"/>\n  \
         <reward kind=\"TAG\" amount=\"1\"/>\n</quest>\n",
        &[],
    );
    let got = check(&dir);
    // The second reward has no `target=`: its effect is not applied and
    // draws nothing.
    assert_eq!(codes(&got), ["E-RELATION-UNKNOWN"], "{got:?}");
    assert!(
        got[0].starts_with(
            "quests/q.lute:8:17 [E-RELATION-UNKNOWN] `<reward kind=\"TAG\">` asserts \
             `seen(mira)` (its kind's `effects`): "
        ),
        "{got:?}"
    );
}

/// Accept-driven quests that accept each other only through rewards draw
/// no start cycle: accept anchors do not gate a start (0.49 exclusion).
#[test]
fn rewards_accepting_each_other_draw_no_start_cycle() {
    let dir = project(
        "re-cycle",
        "<quest id=\"opener\" tier=\"run\" title=\"Opener\" start=\"true\">\n  \
         <objective id=\"o\" title=\"Do it\" done=\"run.flag\"/>\n  \
         <reward kind=\"OFFER_QUEST\" amount=\"1\" target=\"ping\"/>\n</quest>\n\n\
         <quest id=\"ping\" tier=\"run\" title=\"Ping\">\n  \
         <objective id=\"o\" title=\"Do it\" done=\"run.flag\"/>\n  \
         <reward kind=\"OFFER_QUEST\" amount=\"1\" target=\"pong\"/>\n</quest>\n\n\
         <quest id=\"pong\" tier=\"run\" title=\"Pong\">\n  \
         <objective id=\"o\" title=\"Do it\" done=\"run.flag\"/>\n  \
         <reward kind=\"OFFER_QUEST\" amount=\"1\" target=\"ping\"/>\n</quest>\n",
        &[],
    );
    assert_eq!(check(&dir), Vec::<String>::new());
}

/// `<quest id="firstVisit">` with `rewards` (quest-level rewards and its
/// `c1` objective's body, both XML), a `questComplete` handler reading the
/// guard, and a dependent quest and scene gated on its completion.
fn must_project(tag: &str, objective: &str, rewards: &str) -> lute_scratch::ScratchDir {
    project(
        tag,
        &format!(
            "<quest id=\"firstVisit\" tier=\"user\" title=\"First visit\" start=\"true\">\n  \
             <objective id=\"c1\" title=\"Do it\" done=\"run.flag\">\n    {objective}\n  \
             </objective>\n  {rewards}\n  {}\n</quest>\n\n\
             <quest id=\"later\" tier=\"user\" title=\"Later\" \
             start=\"quest.firstVisit.state == 'complete'\">\n  \
             <objective id=\"c1\" title=\"Do it\" done=\"run.flag\"/>\n  {}\n</quest>\n",
            on_complete(),
            on_complete()
        ),
        &[(
            "scenes/s.lute",
            "---\nkind: scene\nid: afterFirst\nafter: completed(\"firstVisit\")\n---\n\n## Shot \
             1.\n\n@mira{when=\"holds('met', ['mira'])\"}: We met.\n",
        )],
    )
}

/// The `W-FACT-GUARANTEED` guards of `diags`, by `file:line`.
fn guaranteed(diags: &[String]) -> Vec<&str> {
    diags
        .iter()
        .filter(|d| d.contains("[W-FACT-GUARANTEED]"))
        .map(|d| d.split_once(' ').map_or(d.as_str(), |(at, _)| at))
        .map(|at| at.rsplit_once(':').map_or(at, |(l, _)| l))
        .collect()
}

#[test]
fn an_unconditional_reward_assert_is_guaranteed_after_the_grant() {
    let dir = must_project(
        "re-must-plain",
        "",
        "<reward kind=\"BADGE\" amount=\"1\" target=\"mira\"/>",
    );
    let got = check(&dir);
    // The completion handler, the dependent quest's handler, and the scene
    // `after: completed("firstVisit")`.
    assert_eq!(
        guaranteed(&got),
        ["quests/q.lute:13", "quests/q.lute:27", "scenes/s.lute:9"],
        "{got:?}"
    );
    assert!(
        got[0].contains("is asserted on every route to here (./quests/q.lute:10)"),
        "{got:?}"
    );
    // Its negation is a dead arm.
    let dir = project(
        "re-must-dead",
        &format!(
            "<quest id=\"firstVisit\" tier=\"user\" title=\"First visit\" start=\"true\">\n  \
             <objective id=\"c1\" title=\"Do it\" done=\"run.flag\"/>\n  \
             <reward kind=\"BADGE\" amount=\"1\" target=\"mira\"/>\n  \
             <on event=\"questComplete\">\n    {}\n  </on>\n</quest>\n",
            GUARD.replace("holds('met'", "!holds('met'")
        ),
        &[],
    );
    let got = check(&dir);
    assert_eq!(codes(&got), ["E-ARM-DEAD"], "{got:?}");
}

#[test]
fn a_guarded_reward_assert_is_possible_only() {
    let dir = must_project(
        "re-must-guarded",
        "",
        "<reward kind=\"BADGE\" amount=\"1\" target=\"mira\" when=\"run.other\"/>",
    );
    assert_eq!(check(&dir), Vec::<String>::new());
}

#[test]
fn a_retract_after_an_earlier_assert_removes_the_fact() {
    // The objective's grant asserts; the quest's grant, later, retracts.
    let dir = must_project(
        "re-must-revoke",
        &format!("<reward kind=\"BADGE\" amount=\"1\" target=\"mira\"/>\n    {GUARD}"),
        "<reward kind=\"REVOKE\" amount=\"1\" target=\"mira\"/>",
    );
    let got = check(&dir);
    // Guaranteed in the objective's body, right after its grant, and no
    // longer at the quest's completion or after it.
    assert_eq!(guaranteed(&got), ["quests/q.lute:10"], "{got:?}");
}

#[test]
fn grants_apply_in_order_across_and_within_rewards() {
    // Assert, then retract: absent after the grant.
    let dir = must_project(
        "re-must-order",
        "",
        "<reward kind=\"BADGE\" amount=\"1\" target=\"mira\"/>\n  \
         <reward kind=\"REVOKE\" amount=\"1\" target=\"mira\"/>",
    );
    assert_eq!(check(&dir), Vec::<String>::new());
    // Retract, then assert: present after the grant. A fact some grant
    // retracts is not monotone, so it does not cross into the dependent
    // quest or the scene.
    let dir = must_project(
        "re-must-order-back",
        "",
        "<reward kind=\"REVOKE\" amount=\"1\" target=\"mira\"/>\n  \
         <reward kind=\"BADGE\" amount=\"1\" target=\"mira\"/>",
    );
    assert_eq!(guaranteed(&check(&dir)), ["quests/q.lute:14"]);
    // One kind retracting and asserting the same pattern: retracts first,
    // so present.
    let dir = must_project("re-must-swap", "", "<reward kind=\"SWAP\" amount=\"1\"/>");
    assert_eq!(guaranteed(&check(&dir)), ["quests/q.lute:13"]);
}

#[test]
fn an_objective_grant_holds_in_its_body_and_at_completion() {
    let dir = must_project(
        "re-must-objective",
        &format!("<reward kind=\"BADGE\" amount=\"1\" target=\"mira\"/>\n    {GUARD}"),
        "",
    );
    let got = check(&dir);
    assert_eq!(
        guaranteed(&got),
        [
            "quests/q.lute:10",
            "quests/q.lute:21",
            "quests/q.lute:35",
            "scenes/s.lute:9"
        ],
        "{got:?}"
    );
}

/// 0.50.0 §5.6: at the grant the engine applies the reward's effects, and
/// `lute play`, `lute run` and `lute trace` name the reward each effect
/// came from. On 0.49.1 the plugin fails to load, so nothing grants.
#[test]
fn granted_effects_apply_and_name_their_reward() {
    let dir = project(
        "re-play",
        "<quest id=\"first\" tier=\"run\" title=\"First\" start=\"true\">\n  \
         <objective id=\"c1\" title=\"Do it\" done=\"run.flag\"/>\n  \
         <reward kind=\"BADGE\" target=\"mira\"/>\n  \
         <reward kind=\"OFFER_QUEST\" target=\"second\"/>\n</quest>\n\n\
         <quest id=\"second\" tier=\"run\" title=\"Second\">\n  \
         <objective id=\"c1\" title=\"Do it\" done=\"holds('met', ['mira'])\"/>\n</quest>\n",
        &[(
            "script.play.yaml",
            "steps:\n  - engine: { state: { run.flag: true } }\n    expect: { quests: { first: \
             complete, second: complete } }\n",
        )],
    );
    let out = lute(
        &dir,
        &["play", "--project", ".", "--script", "script.play.yaml"],
    );
    let play = text(&out);
    assert!(out.status.success(), "{play}");
    assert!(
        play.contains("assert met(mira)  (effect of reward BADGE)"),
        "{play}"
    );
    assert!(
        play.contains("quest second accepted  (effect of reward OFFER_QUEST)"),
        "{play}"
    );

    let trace = text(&lute(
        &dir,
        &["trace", "quests/q.lute", "--state", "run.flag=true"],
    ));
    assert!(
        trace.contains("::assert  met(mira)  (effect of reward BADGE)"),
        "{trace}"
    );
    assert!(
        trace.contains("quest second accepted  (effect of reward OFFER_QUEST)"),
        "{trace}"
    );
}
