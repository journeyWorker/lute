//! dsl 0.24.0 §2 quest structure through `lute play` (and `check-project`):
//! `activate="accept"` children, `complete="any"` with `superseded`, the
//! reserved `failedBy` / `objectives.<o>.failed` reads, `<on event target>`,
//! occasion `judge: before`, and `::accept{… at="nextRun"}` surviving the
//! `newRun` reset.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value as Json;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-quest024-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, rel: &str, text: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, text).unwrap();
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

const QUESTS: &str = r#"---
kind: quest
uses: ../world.schema.yaml
title: Road
---

<quest id="road" title="Road" start="true" complete="any">
  <objective id="words" title="Words" quest="parley"/>
  <objective id="silver" title="Silver" quest="toll"/>
</quest>

<quest id="parley" title="Parley" activate="accept">
  <objective id="terms" title="Terms" on="talk" target="npc.maud" done="run.talked"/>
</quest>

<quest id="toll" title="Toll" activate="accept">
  <objective id="pay" title="Pay" done="run.paid"/>
  <on event="questFailed">
    @narrator: Toll moot ({{quest.toll.failedBy}}).
  </on>
</quest>

<quest id="errand" title="Errand" start="true">
  <objective id="letter" title="Letter" done="run.paid" by="run.day >= 3"/>
</quest>

<quest id="gossip" title="Gossip" start="true">
  <objective id="hush" title="Hush" done="run.stormed"/>
  <on event="talk" target="npc.maud">
    @narrator: Maud handler.
  </on>
  <on event="talk">
    @narrator: Any handler.
  </on>
</quest>

<quest id="cross" title="Cross" start="true">
  <objective id="over" title="Over" on="chapterEnd" done="run.paid"/>
  <on event="questComplete">
    @narrator: Cross handler.
  </on>
</quest>

<quest id="dusk" title="Dusk" start="true">
  <objective id="rest" title="Rest" on="nightfall" done="run.paid"/>
</quest>

<quest id="ford" title="Ford" start="true" complete="any">
  <objective id="swim" title="Swim" done="run.stormed" by="run.day >= 3"/>
  <objective id="boat" title="Boat" quest="ferry"/>
</quest>

<quest id="ferry" title="Ferry" activate="accept" fail="run.day >= 4">
  <objective id="row" title="Row" done="run.stormed"/>
</quest>

<quest id="bounty" title="Bounty" tier="run" fail="run.day >= 9">
  <objective id="hunt" title="Hunt" done="run.stormed"/>
</quest>
"#;

/// A plugin declaring `hubVisit`, `board`, `talk` (targets `npc.<person>`,
/// also a world event), `chapterEnd` (`judge: before`) and `nightfall`
/// (the default `judge: after`), plus the quests above and one scene per
/// occasion.
fn project(tag: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: g\nprofiles:\n  g:\n    plugins: { g.occ: true }\n",
    );
    write(
        &dir,
        "plugins/g.occ/plugin.yaml",
        "id: g.occ\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports:\n  occasions: occasions/\n  events: events/\n",
    );
    write(
        &dir,
        "plugins/g.occ/occasions/o.yaml",
        "occasions:\n  hubVisit: {}\n  board: {}\n  talk: { target: { prefix: npc, entity: person } }\n  \
         chapterEnd: { judge: before }\n  nightfall: {}\n",
    );
    write(
        &dir,
        "plugins/g.occ/events/e.yaml",
        "events:\n  - { name: talk }\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.day: { type: number, default: 1 }\n  \
         run.talked: { type: bool, default: false }\n  \
         run.paid: { type: bool, default: false }\n  \
         run.stormed: { type: bool, default: false }\n\
         entities:\n  person: { members: [maud, oskar] }\n",
    );
    write(&dir, "quests/road.lute", QUESTS);
    let scene = |rel: &str, id: &str, fm: &str, body: &str| {
        write(
            &dir,
            rel,
            &format!("---\nkind: scene\nid: {id}\nuses: ../world.schema.yaml\n{fm}---\n\n## {id}\n\n{body}"),
        );
    };
    scene(
        "scenes/offer.lute",
        "hub.offer",
        "on: hubVisit\nonce: false\n",
        "@maud: Two ways across.\n\n<branch id=\"offer\">\n\
         <choice id=\"words\" label=\"Talk\">\n@maud: Talk, then.\n::accept{quest=\"parley\"}\n</choice>\n\
         <choice id=\"silver\" label=\"Pay\">\n@maud: Pay, then.\n::accept{quest=\"toll\"}\n</choice>\n\
         <choice id=\"boat\" label=\"Ferry\">\n@maud: Row, then.\n::accept{quest=\"ferry\"}\n</choice>\n\
         </branch>\n",
    );
    scene(
        "scenes/board.lute",
        "hub.board",
        "on: board\nonce: false\n",
        "@maud: A bounty for the next run.\n::accept{quest=\"bounty\" at=\"nextRun\"}\n",
    );
    scene(
        "scenes/over.lute",
        "end.over",
        "on: chapterEnd\nwhen: \"quest.cross.state == 'complete'\"\n",
        "@narrator: Crossed.\n",
    );
    scene(
        "scenes/rest.lute",
        "night.rest",
        "on: nightfall\nwhen: \"quest.dusk.state == 'complete'\"\n",
        "@narrator: Rested.\n",
    );
    scene(
        "scenes/late.lute",
        "night.late",
        "on: nightfall\npriority: 5\n\
         when: \"quest.errand.failedBy == 'by' && quest.errand.objectives.letter.failed\"\n",
        "@narrator: The letter is lost.\n",
    );
    dir
}

fn play(dir: &Path, tag: &str, script: &str, json: bool) -> Output {
    let path = dir.join(format!("{tag}.play.yaml"));
    std::fs::write(&path, script).unwrap();
    let mut cmd = Command::new(BIN);
    cmd.args(["play", &dir.display().to_string(), "--script"])
        .arg(&path);
    if json {
        cmd.arg("--json");
    }
    cmd.output().unwrap()
}

fn play_json(dir: &Path, tag: &str, script: &str) -> Json {
    let out = play(dir, tag, script, true);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    serde_json::from_slice(&out.stdout).unwrap()
}

/// Every quest transition of step `n` (`0` = the start settle), as
/// `"<quest> -> <state>"` or `"<quest> -> failed (<reason>)"`.
fn transitions(v: &Json, n: usize) -> Vec<String> {
    let records = if n == 0 {
        &v["start"]["quests"]
    } else {
        &v["steps"][n - 1]["quests"]
    };
    quest_records(records)
}

/// The quest records of `records` (a `quests` / `judgedBefore` array).
fn quest_records(records: &Json) -> Vec<String> {
    records
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|doc| doc["commands"].as_array().unwrap())
        .filter(|r| r["kind"] == "quest")
        .map(|r| match r["failedBy"].as_str() {
            Some(by) => format!(
                "{} -> {} ({by})",
                r["quest"].as_str().unwrap(),
                r["state"].as_str().unwrap()
            ),
            None => format!(
                "{} -> {}",
                r["quest"].as_str().unwrap(),
                r["state"].as_str().unwrap()
            ),
        })
        .collect()
}

#[test]
fn the_project_checks_clean() {
    let dir = project("check");
    let out = Command::new(BIN)
        .arg("check-project")
        .arg(&dir)
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out));
    let t = text(&out);
    assert!(!t.contains("W-QUEST-NEVER-ACCEPTED"), "{t}");
}

#[test]
fn accept_children_wait_and_complete_any_supersedes_the_rest() {
    let dir = project("any");
    let v = play_json(
        &dir,
        "any",
        "steps:\n  - occasion: hubVisit\n    choose: { offer: silver }\n  \
         - occasion: hubVisit\n    choose: { offer: words }\n  \
         - engine: { state: { run.talked: true } }\n  \
         - occasion: talk\n    target: npc.maud\n\
         expect:\n  quests: { road: complete, parley: complete, toll: failed }\n",
    );
    // The children do not activate with their parent.
    let start = transitions(&v, 0);
    assert!(start.contains(&"road -> active".to_string()), "{start:?}");
    assert!(
        !start
            .iter()
            .any(|t| t.starts_with("parley") || t.starts_with("toll")),
        "{start:?}"
    );
    // Each activates once accepted, the parent being active.
    assert_eq!(transitions(&v, 1), ["toll -> active"]);
    assert_eq!(transitions(&v, 2), ["parley -> active"]);
    // `talk` for Maud judges `parley.terms`: the road completes, `toll`
    // (still active) is superseded.
    assert_eq!(
        transitions(&v, 4),
        [
            "parley -> complete",
            "road -> complete",
            "toll -> failed (superseded)"
        ]
    );

    let out = play(
        &dir,
        "any-human",
        "steps:\n  - occasion: hubVisit\n    choose: { offer: silver }\n  \
         - engine: { state: { run.talked: true, run.paid: false } }\n  \
         - occasion: hubVisit\n    choose: { offer: words }\n  \
         - occasion: talk\n    target: npc.maud\n",
        false,
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    assert!(t.contains("  quest toll -> failed (superseded)\n"), "{t}");
    assert!(t.contains("Toll moot (superseded)."), "{t}");
}

#[test]
fn a_never_accepted_alternative_stays_unset_when_the_parent_completes() {
    let dir = project("unset");
    let v = play_json(
        &dir,
        "unset",
        "steps:\n  - occasion: hubVisit\n    choose: { offer: silver }\n  \
         - engine: { state: { run.paid: true } }\n\
         expect:\n  quests: { road: complete, toll: complete, parley: unset }\n",
    );
    let settled = transitions(&v, 2);
    assert!(
        settled.contains(&"road -> complete".to_string()),
        "{settled:?}"
    );
    assert!(
        !settled.iter().any(|t| t.starts_with("parley")),
        "{settled:?}"
    );
}

#[test]
fn failed_by_and_objective_failed_are_readable_by_beats() {
    let dir = project("failed-by");
    let script = "steps:\n  - occasion: nightfall\n    \
                  expect: { state: { quest.errand.failedBy: unset, quest.errand.objectives.letter.failed: false } }\n  \
                  - engine: { state: { run.day: 3 } }\n    \
                  expect: { state: { quest.errand.failedBy: by, quest.errand.objectives.letter.failed: true } }\n  \
                  - occasion: nightfall\n";
    let v = play_json(&dir, "failed-by", script);
    assert!(v["steps"][0]["winner"].is_null(), "{}", v["steps"][0]);
    assert_eq!(transitions(&v, 2), ["errand -> failed (by)"]);
    assert_eq!(v["steps"][2]["winner"], "night.late");

    let out = play(&dir, "failed-by-human", script, false);
    let t = text(&out);
    assert!(t.contains("  quest errand -> failed (by)\n"), "{t}");
    assert!(t.contains("The letter is lost."), "{t}");
}

/// `ford` is `complete="any"` over a plain objective (`swim`, missed on day
/// 3) and an accept child (`ferry`, failing on day 4): its synthesized fail
/// waits for both.
#[test]
fn an_any_parent_fails_only_once_every_alternative_failed() {
    let dir = project("any-fail");
    let v = play_json(
        &dir,
        "any-fail",
        "steps:\n  - occasion: hubVisit\n    choose: { offer: boat }\n  \
         - engine: { state: { run.day: 3 } }\n    expect: { quests: { ford: active, ferry: active } }\n  \
         - engine: { state: { run.day: 4 } }\n\
         expect:\n  quests: { ford: failed, ferry: failed }\n",
    );
    assert_eq!(transitions(&v, 1), ["ferry -> active"]);
    assert!(
        !transitions(&v, 2).iter().any(|t| t.starts_with("ford")),
        "{:?}",
        transitions(&v, 2)
    );
    assert_eq!(
        transitions(&v, 3),
        ["ferry -> failed (fail)", "ford -> failed (fail)"]
    );
}

#[test]
fn a_targeted_on_answers_only_a_raise_for_its_target() {
    let dir = project("on-target");
    let out = play(
        &dir,
        "on-target",
        "steps:\n  - occasion: talk\n    target: npc.oskar\n    label: oskar\n  \
         - occasion: talk\n    target: npc.maud\n    label: maud\n",
        false,
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    let (oskar, maud) = t.split_once("── step 2").unwrap_or_else(|| panic!("{t}"));
    assert!(
        oskar.contains("Any handler.") && !oskar.contains("Maud handler."),
        "{t}"
    );
    assert!(
        maud.contains("Maud handler.") && maud.contains("Any handler."),
        "{t}"
    );
}

#[test]
fn judge_before_judges_the_occasion_before_its_beats_are_presented() {
    let dir = project("judge");
    let v = play_json(
        &dir,
        "judge",
        "steps:\n  - engine: { state: { run.paid: true } }\n  - occasion: chapterEnd\n  \
         - occasion: nightfall\n",
    );
    // `chapterEnd` is `judge: before`: `cross` completes first, so its
    // epilogue sees it.
    assert_eq!(
        quest_records(&v["steps"][1]["judgedBefore"]),
        ["cross -> complete"]
    );
    assert!(transitions(&v, 2).is_empty(), "{:?}", transitions(&v, 2));
    assert_eq!(v["steps"][1]["winner"], "end.over");
    // `nightfall` judges after: its beat still reads `dusk` active.
    assert!(v["steps"][2]["winner"].is_null(), "{}", v["steps"][2]);
    assert!(
        v["steps"][2].get("judgedBefore").is_none(),
        "{}",
        v["steps"][2]
    );
    assert_eq!(transitions(&v, 3), ["dusk -> complete"]);

    // The transcript shows the judging where it happened: before the beat;
    // the lifecycle handler it fired runs after the beat.
    let out = play(
        &dir,
        "judge-human",
        "steps:\n  - engine: { state: { run.paid: true } }\n  - occasion: chapterEnd\n",
        false,
    );
    let t = text(&out);
    let judged = t
        .find("  quest cross -> complete\n")
        .unwrap_or_else(|| panic!("{t}"));
    let chosen = t.find("  → end.over\n").unwrap_or_else(|| panic!("{t}"));
    assert!(
        t.find("· chapterEnd").unwrap() < judged && judged < chosen,
        "{t}"
    );
    let scene = t.find("Crossed.").unwrap_or_else(|| panic!("{t}"));
    let handler = t.find("Cross handler.").unwrap_or_else(|| panic!("{t}"));
    assert!(
        scene < handler,
        "the questComplete handler runs after the beat: {t}"
    );
}

/// dsl 0.24.0 §2.1 (LH N2): an `on=` objective whose `by` holds wherever its
/// `done` does. `inquest` presents the hearing, which files the verdict.
fn deadline_project(tag: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: g\nprofiles:\n  g:\n    plugins: { g.occ: true }\n",
    );
    write(
        &dir,
        "plugins/g.occ/plugin.yaml",
        "id: g.occ\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports:\n  occasions: occasions/\n",
    );
    write(
        &dir,
        "plugins/g.occ/occasions/o.yaml",
        "occasions:\n  file: {}\n  inquest: {}\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        "state:\n  run.v: { type: { enum: [undecided, fell, drowned] }, default: undecided }\n",
    );
    write(
        &dir,
        "quests/verdict.lute",
        "---\nkind: quest\nuses: ../world.schema.yaml\ntitle: Verdict\n---\n\n\
         <quest id=\"verdict\" title=\"Verdict\" start=\"true\">\n  \
         <objective id=\"fate\" title=\"Fate\" on=\"inquest\" done=\"run.v == 'fell'\" by=\"run.v != 'undecided'\"/>\n\
         </quest>\n",
    );
    for (rel, id, on) in [
        ("scenes/file.lute", "hub.file", "file"),
        ("scenes/hearing.lute", "end.hearing", "inquest"),
    ] {
        write(
            &dir,
            rel,
            &format!(
                "---\nkind: scene\nid: {id}\nuses: ../world.schema.yaml\non: {on}\nonce: false\n---\n\n\
                 ## {id}\n\n@narrator: Filed.\n::set{{run.v = \"fell\"}}\n"
            ),
        );
    }
    dir
}

#[test]
fn a_deadline_that_comes_true_in_the_raising_step_loses_to_done() {
    let dir = deadline_project("deadline-same-step");
    // The hearing files the verdict, then `inquest` judges `done` first.
    let v = play_json(
        &dir,
        "same-step",
        "steps:\n  - occasion: inquest\nexpect:\n  quests: { verdict: complete }\n",
    );
    assert_eq!(transitions(&v, 1), ["verdict -> complete"]);
    // Filed in an earlier step, the deadline is a moment: it fails there.
    let v = play_json(
        &dir,
        "earlier-step",
        "steps:\n  - occasion: file\n  - occasion: inquest\nexpect:\n  quests: { verdict: failed }\n",
    );
    assert_eq!(transitions(&v, 1), ["verdict -> failed (by)"]);
    assert!(transitions(&v, 2).is_empty(), "{:?}", transitions(&v, 2));
}

#[test]
fn an_accept_at_next_run_survives_the_new_run_reset() {
    let dir = project("next-run");
    let script = "steps:\n  - occasion: board\n    expect: { quests: { bounty: unset } }\n  \
                  - newRun: true\nexpect:\n  quests: { bounty: active }\n";
    let v = play_json(&dir, "next-run", script);
    assert!(transitions(&v, 1).is_empty(), "{:?}", transitions(&v, 1));
    assert_eq!(v["steps"][1]["accepted"], serde_json::json!(["bounty"]));
    assert_eq!(transitions(&v, 2), ["bounty -> active"]);

    let out = play(&dir, "next-run-human", script, false);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    assert!(
        t.contains("  quest bounty accepted (queued: applies after the next newRun)\n"),
        "{t}"
    );
    assert!(
        t.contains("  quest bounty accepted (queued at=\"nextRun\")\n"),
        "{t}"
    );
    // A run-tier quest's reset clears why it failed.
    let v = play_json(
        &dir,
        "next-run-reset",
        "steps:\n  - occasion: board\n  - newRun: true\n  - engine: { state: { run.day: 9 } }\n    \
         expect: { quests: { bounty: failed }, state: { quest.bounty.failedBy: fail } }\n  - newRun: true\n\
         expect:\n  quests: { bounty: unset }\n  state: { quest.bounty.failedBy: unset }\n",
    );
    assert!(
        transitions(&v, 3).contains(&"bounty -> failed (fail)".to_string()),
        "{:?}",
        transitions(&v, 3)
    );
    // Once applied, the queue is empty: a second run start accepts nothing.
    let v = play_json(
        &dir,
        "next-run-twice",
        "steps:\n  - occasion: board\n  - newRun: true\n  - newRun: true\n",
    );
    assert!(v["steps"][2].get("accepted").is_none(), "{}", v["steps"][2]);
}
