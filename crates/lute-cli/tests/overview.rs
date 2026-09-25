//! dsl 0.23.0 §1 / §11 author overviews, through the built `lute` binary:
//! `lute calendar` (play's eligibility over a grid of state values),
//! `lute beats` (the selection ladder with `check-project`'s verdicts),
//! `lute scenario knowledge` (fact-guarded conditions traced to producers),
//! the scenario graph's note on references it cannot draw, and the play
//! transcript's `read` mark on an already-read entry beat.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value as Json;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-overview-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(dir: &Path, rel: &str, text: &str) -> PathBuf {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, text).unwrap();
    p
}

fn lute(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn text(o: &Output) -> String {
    format!(
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

fn scene(id: &str, keys: &str, body: &str) -> String {
    format!("---\nkind: scene\nid: {id}\n{keys}---\n\n## {id}\n\n{body}\n")
}

/// A small day-clock town: two slots, two places, a routine and an event on
/// `dayStart`, fact-gated place beats, a `select: all` notice board.
fn town(tag: &str) -> PathBuf {
    let d = temp_dir(tag);
    write(
        &d,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: town\nprofiles:\n  town:\n    plugins: { town.clock: true }\n\
         defaults:\n  uses: [world.schema.yaml]\n",
    );
    write(
        &d,
        "plugins/town.clock/plugin.yaml",
        "id: town.clock\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\n\
         exports:\n  occasions: occasions/\n",
    );
    write(
        &d,
        "plugins/town.clock/occasions/clock.yaml",
        "occasions:\n  dayStart: {}\n  placeVisit: { target: true }\n  board: { select: all, target: true }\n",
    );
    write(
        &d,
        "world.schema.yaml",
        "state:\n  run.day: { type: number, default: 1 }\n  run.slot: { type: { enum: [morning, night] }, default: morning }\n\
         entities:\n  person: { members: [ada, bo] }\n  place: { members: [inn, dock] }\n\
         relations:\n  present: { args: [person, place], derive: true }\n  met: { args: [person] }\n\
         \x20 rumor: { args: [person] }\n  trusted: { args: [person], derive: true }\n\
         rules:\n  - \"present(ada, inn) :- cel(\\\"run.slot == 'night'\\\")\"\n\
         \x20 - \"present(bo, dock) :- cel(\\\"run.slot == 'morning' && run.day != 2\\\")\"\n\
         \x20 - \"trusted(P) :- met(P), not rumor(P)\"\n",
    );
    let beats = [
        ("dawn", "town.dawn", "title: Dawn\non: dayStart\nonce: false\nwhen: \"run.slot == 'morning'\"\n"),
        ("day2", "town.day2", "on: dayStart\nwhen: 'run.day == 2'\npriority: 10\n"),
        ("memo", "town.memo", "on: dayStart\nonce: false\nwhen: 'run.day >= 1'\n"),
        ("idle", "town.idle", "on: dayStart\nonce: false\npriority: -5\n"),
        ("never", "town.never", "on: dayStart\nonce: false\npriority: -9\n"),
        (
            "inn-again",
            "inn.again",
            "on: placeVisit\ntarget: place.inn\nafter: 'visited(\"inn.ada\")'\nwhen: 'holds(trusted(ada))'\npriority: 30\n",
        ),
    ];
    for (file, id, keys) in beats {
        write(&d, &format!("scenes/{file}.lute"), &scene(id, keys, "@narrator: A moment passes."));
    }
    write(
        &d,
        "scenes/inn-ada.lute",
        &scene(
            "inn.ada",
            "on: placeVisit\ntarget: place.inn\nwhen: 'holds(present(ada, inn))'\npriority: 20\n",
            "@ada: Evening.\n::assert{met(ada)}",
        ),
    );
    write(
        &d,
        "lore/barks.lute",
        "---\nkind: lore\nid: town.barks\n---\n\n\
         <entry id=\"innShut\" on=\"placeVisit\" target=\"place.inn\" when=\"!holds(present(ada, inn))\">\n  @narrator: The inn is quiet.\n</entry>\n\n\
         <entry id=\"dockBo\" on=\"placeVisit\" target=\"place.dock\" when=\"holds(present(bo, dock))\">\n  @bo: Morning.\n</entry>\n\n\
         <entry id=\"noteA\" on=\"board\" target=\"place.inn\" priority=\"5\" title=\"Notice A\" when=\"run.day == 1\">\n  @narrator: A.\n</entry>\n\n\
         <entry id=\"noteB\" on=\"board\" target=\"place.inn\" title=\"Notice B\">\n  @narrator: B.\n</entry>\n",
    );
    d
}

fn calendar_json(dir: &Path, extra: &[&str]) -> Json {
    let d = dir.to_str().unwrap();
    let mut args = vec![
        "calendar", d, "--axis", "run.day=1..2", "--axis", "run.slot=morning,night", "--json",
    ];
    args.extend_from_slice(extra);
    let out = lute(&args);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    serde_json::from_slice(&out.stdout).expect("--json emits one object")
}

/// The result of `occasion[@target]` at cell `n` (0-based, odometer order).
fn result<'a>(v: &'a Json, n: usize, occasion: &str, target: Option<&str>) -> &'a Json {
    v["cells"][n]["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["occasion"] == occasion && r["target"].as_str() == target)
        .unwrap_or_else(|| panic!("cell {n} has no {occasion}@{target:?}: {}", v["cells"][n]))
}

fn ids(v: &Json) -> Vec<&str> {
    v.as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect()
}

#[test]
fn calendar_evaluates_every_cell_with_winner_shadowed_and_holes() {
    let dir = town("cal");
    let v = calendar_json(&dir, &[]);
    assert_eq!(v["cells"].as_array().unwrap().len(), 4);
    assert_eq!(v["cells"][1]["at"], serde_json::json!({ "run.day": 1, "run.slot": "night" }));

    // Day 1 morning: the routine wins; the tie partner and the always-on
    // fallbacks are eligible but shadowed.
    let day_start = result(&v, 0, "dayStart", None);
    assert_eq!(day_start["winner"], "town.dawn");
    assert_eq!(ids(&day_start["shadowed"]), ["town.memo", "town.idle", "town.never"]);
    assert_eq!(result(&v, 0, "placeVisit", Some("place.dock"))["winner"], "dockBo");
    assert_eq!(result(&v, 0, "placeVisit", Some("place.inn"))["winner"], "innShut");
    // `select: all` offers every eligible beat and has no winner.
    let board = result(&v, 0, "board", Some("place.inn"));
    assert_eq!(ids(&board["presented"]), ["noteA", "noteB"]);
    assert!(board["winner"].is_null());

    // Night: the derived `present(ada, inn)` flips the inn; the dock is a hole.
    assert_eq!(result(&v, 1, "dayStart", None)["winner"], "town.memo");
    assert_eq!(result(&v, 1, "placeVisit", Some("place.inn"))["winner"], "inn.ada");
    let dock = result(&v, 1, "placeVisit", Some("place.dock"));
    assert!(dock["winner"].is_null() && ids(&dock["presented"]).is_empty(), "{dock}");
    // Day 2: the event outranks the routine; the board drops notice A.
    assert_eq!(result(&v, 2, "dayStart", None)["winner"], "town.day2");
    assert_eq!(ids(&result(&v, 3, "board", Some("place.inn"))["presented"]), ["noteB"]);

    // `after:` over an empty visited set: never eligible, and why.
    let never = v["neverEligible"].as_array().unwrap();
    assert_eq!(never.len(), 1, "{never:?}");
    assert_eq!(never[0]["id"], "inn.again");
    assert_eq!(never[0]["reasons"][0], "after: prerequisite not satisfied");
}

#[test]
fn calendar_starts_every_cell_from_the_script_save() {
    let dir = town("cal-save");
    let save = write(&dir, "save.play.yaml", "visited: [inn.ada]\nfacts: [\"met(ada)\"]\n");
    let v = calendar_json(&dir, &["--script", save.to_str().unwrap(), "--occasion", "placeVisit"]);
    let inn = result(&v, 1, "placeVisit", Some("place.inn"));
    assert_eq!(inn["winner"], "inn.again");
    assert_eq!(ids(&inn["shadowed"]), ["inn.ada"]);
    assert!(v["neverEligible"].as_array().unwrap().is_empty(), "{}", v["neverEligible"]);
    // Only the listed occasion is evaluated.
    assert!(v["columns"].as_array().unwrap().iter().all(|c| c["occasion"] == "placeVisit"));
}

#[test]
fn calendar_text_csv_and_usage_errors() {
    let dir = town("cal-text");
    let d = dir.to_str().unwrap();
    let out = lute(&["calendar", d, "--axis", "run.day=1..2", "--axis", "run.slot=morning,night"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("town.dawn +3"), "{s}");
    assert!(s.contains("run.day=1 run.slot=morning  dayStart: town.dawn over town.memo, town.idle, town.never"), "{s}");
    assert!(s.contains("never eligible in any cell: 1\n  inn.again [scene, scenes/inn-again.lute] placeVisit@place.inn"), "{s}");

    let csv = lute(&["calendar", d, "--axis", "run.slot=night", "--occasion", "placeVisit", "--csv"]);
    assert_eq!(csv.status.code(), Some(0), "{}", text(&csv));
    let rows = String::from_utf8_lossy(&csv.stdout).to_string();
    assert!(rows.starts_with("run.slot,occasion,target,select,winner,presented,shadowed,unknown,notes\n"), "{rows}");
    assert!(rows.contains("night,placeVisit,place.dock,first,,,,,\n"), "{rows}");
    assert!(rows.contains("night,placeVisit,place.inn,first,inn.ada,inn.ada,,,\n"), "{rows}");

    for (args, why) in [
        (vec!["--axis", "run.dy=1..2"], "undeclared path"),
        (vec!["--axis", "run.slot=noon"], "value outside the enum"),
        (vec!["--axis", "run.day=3..1"], "empty range"),
        (vec!["--axis", "run.day=1", "--occasion", "nope"], "unknown occasion"),
        (vec!["--axis", "run.day=1", "--occasion", "dayStart", "--target", "place.inn"], "target no listed occasion takes"),
    ] {
        let mut full = vec!["calendar", d];
        full.extend(args);
        let out = lute(&full);
        assert_eq!(out.status.code(), Some(2), "{why}: {}", text(&out));
    }
}

#[test]
fn calendar_replays_the_route_and_applies_quest_fact_and_where_axes() {
    let dir = town("cal-replay");
    // A domain-targeted occasion only the inn answers, and a quest-gated beat.
    write(
        &dir,
        "plugins/town.clock/occasions/clock.yaml",
        "occasions:\n  dayStart: {}\n  placeVisit: { target: true }\n  board: { select: all, target: true }\n  \
         look: { target: { prefix: place, entity: place } }\n",
    );
    write(&dir, "scenes/look.lute", &scene("inn.look", "on: look\ntarget: place.inn\nonce: false\n", "@narrator: Lamps."));
    write(
        &dir,
        "scenes/lost.lute",
        &scene("town.lost", "on: dayStart\nonce: false\npriority: 50\nwhen: \"quest.errand.state == 'failed'\"\n", "@narrator: Too late."),
    );
    write(
        &dir,
        "quests/errand.lute",
        "---\nkind: quest\ntitle: Errand\n---\n\n<quest id=\"errand\" title=\"Errand\">\n\
         <objective id=\"go\" title=\"Go\" done=\"run.day > 5\"/>\n</quest>\n",
    );
    let d = dir.to_str().unwrap();
    // The route meets ada at the inn, then comes back.
    let route = write(
        &dir,
        "route.play.yaml",
        "state:\n  run.slot: night\nsteps:\n  - occasion: placeVisit\n    target: place.inn\n  \
         - label: back at the inn\n    occasion: placeVisit\n    target: place.inn\n",
    );
    let r = route.to_str().unwrap();
    let run = |extra: &[&str]| {
        let mut args = vec!["calendar", d, "--json"];
        args.extend_from_slice(extra);
        let out = lute(&args);
        assert_eq!(out.status.code(), Some(0), "{}", text(&out));
        serde_json::from_slice::<Json>(&out.stdout).unwrap()
    };
    // Replayed up to the labelled step: ada was met and `inn.ada` visited.
    let base = ["--script", r, "--until", "back at the inn", "--occasion", "placeVisit", "--target", "place.inn"];
    let mut args = base.to_vec();
    args.extend(["--axis", "holds(rumor(ada))=false,true"]);
    let v = run(&args);
    assert!(v["from"].as_str().unwrap().contains("step 1 replayed"), "{}", v["from"]);
    assert_eq!(result(&v, 0, "placeVisit", Some("place.inn"))["winner"], "inn.again");
    assert_ne!(result(&v, 1, "placeVisit", Some("place.inn"))["winner"], "inn.again", "a rumor defeats the trust");

    // A quest-state axis seeds the status the lifecycle keeps.
    let v = run(&["--axis", "quest.errand.state=unset,failed", "--occasion", "dayStart"]);
    assert_ne!(result(&v, 0, "dayStart", None)["winner"], "town.lost");
    assert_eq!(result(&v, 1, "dayStart", None)["winner"], "town.lost");

    // `--where` drops cells; a domain-targeted occasion gets only the
    // targets its beats name.
    let v = run(&["--axis", "run.slot=morning,night", "--where", "run.slot == 'night'", "--occasion", "look"]);
    assert_eq!(v["pruned"], 1);
    assert_eq!(v["cells"].as_array().unwrap().len(), 1);
    let targets: Vec<&str> = v["columns"].as_array().unwrap().iter().map(|c| c["target"].as_str().unwrap()).collect();
    assert_eq!(targets, ["place.inn"]);

    for (args, why) in [
        (vec!["--axis", "quest.nope.state=failed"], "unknown quest"),
        (vec!["--axis", "quest.errand.activatedAt=1"], "quest bookkeeping"),
        (vec!["--axis", "holds(trusted(ada))=true"], "derived fact"),
        (vec!["--axis", "holds(met(ada))=maybe"], "not a bool"),
        (vec!["--script", r, "--until", "nowhere"], "unknown step"),
        (vec!["--axis", "run.day=1", "--where", "run.nope == 1"], "undecidable where"),
    ] {
        let mut full = vec!["calendar", d];
        full.extend(args);
        let out = lute(&full);
        assert_eq!(out.status.code(), Some(2), "{why}: {}", text(&out));
    }
}

#[test]
fn beats_ladder_lists_selection_order_and_check_verdicts() {
    let dir = town("beats");
    // An unreachable beat is an error `check-project` reports — and the
    // ladder still shows it (the project need not check clean).
    write(
        &dir,
        "scenes/bad.lute",
        &scene("town.bad", "on: dayStart\nwhen: 'false'\npriority: 1\n", "@narrator: Never."),
    );
    let d = dir.to_str().unwrap();
    let out = lute(&["beats", d, "--occasion", "dayStart", "--json"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let v: Json = serde_json::from_slice(&out.stdout).unwrap();
    let ladders = v["roots"][0]["ladders"].as_array().unwrap();
    assert_eq!(ladders.len(), 1, "{ladders:?}");
    let rows = ladders[0]["beats"].as_array().unwrap();
    let order: Vec<&str> = rows.iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert_eq!(order, ["town.day2", "town.bad", "town.dawn", "town.memo", "town.idle", "town.never"]);
    let codes = |id: &str| -> Vec<String> {
        rows.iter()
            .find(|r| r["id"] == id)
            .unwrap()["verdicts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|d| d["code"].as_str().unwrap().to_string())
            .collect()
    };
    assert_eq!(codes("town.bad"), ["E-BEAT-UNREACHABLE"]);
    assert_eq!(codes("town.memo"), ["W-BEAT-PRIORITY-TIE"]);
    assert_eq!(codes("town.never"), ["W-BEAT-SHADOWED"]);
    assert!(codes("town.dawn").is_empty() && codes("town.idle").is_empty());
    let dawn = &rows[2];
    assert_eq!(dawn["when"], "run.slot == 'morning'");
    assert_eq!(dawn["title"], "Dawn");
    assert_eq!(dawn["once"], "none");

    // One target's ladder: untargeted beats would ride along; `after:` shows.
    let out = lute(&["beats", d, "--target", "place.inn"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("placeVisit @ place.inn — select: first"), "{s}");
    assert!(!s.contains("dayStart"), "a --target ladder view drops untargeted occasions: {s}");
    let again = s.lines().find(|l| l.contains("inn.again")).unwrap();
    assert!(again.contains("visited(\"inn.ada\")") && again.contains("holds(trusted(ada))"), "{again}");
    let memo = String::from_utf8_lossy(&lute(&["beats", d]).stdout).to_string();
    assert!(memo.lines().any(|l| l.contains("town.memo") && l.contains("tied")), "{memo}");

    assert_eq!(lute(&["beats", d, "--occasion", "nope"]).status.code(), Some(2));
}

#[test]
fn knowledge_traces_a_fact_guard_through_rules_to_producers() {
    let dir = town("knowledge");
    let d = dir.to_str().unwrap();
    let out = lute(&["scenario", d, "knowledge", "--for", "inn.again"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("scene `inn.again` (scenes/inn-again.lute)"), "{s}");
    assert!(s.contains("trusted(ada) — derived by 1 rule"), "{s}");
    assert!(s.contains("rule: trusted(P) :- met(P), not rumor(P)"), "{s}");
    assert!(s.contains("met(ada) — asserted by scene `inn.ada` (scenes/inn-ada.lute)"), "{s}");
    assert!(s.contains("not rumor(ada) — NO PRODUCER"), "{s}");
    assert!(!s.contains("defeated"), "nothing produces rumor(ada) yet: {s}");
    assert!(!s.contains("innShut"), "--for selects one node: {s}");

    // A ground query follows only the rules that can conclude it.
    let all = String::from_utf8_lossy(&lute(&["scenario", d, "knowledge"]).stdout).to_string();
    let inn_shut = all.split("entry `innShut`").nth(1).unwrap().split("\n\n").next().unwrap();
    assert!(inn_shut.contains("present(ada, inn) — derived by 1 rule"), "{inn_shut}");
    assert!(!inn_shut.contains("present(bo, dock) :-"), "{inn_shut}");

    let json = lute(&["scenario", d, "--format", "json", "knowledge"]);
    assert_eq!(json.status.code(), Some(0), "{}", text(&json));
    let v: Json = serde_json::from_slice(&json.stdout).unwrap();
    let rel = &v["roots"][0]["relations"];
    assert_eq!(rel["met"]["assertedBy"][0], "scene `inn.ada` (scenes/inn-ada.lute)");
    assert_eq!(rel["rumor"]["assertedBy"].as_array().unwrap().len(), 0);
    assert_eq!(rel["trusted"]["rules"][0]["premises"][1]["negated"], true);

    let miss = lute(&["scenario", d, "knowledge", "--for", "inn.agian"]);
    assert_eq!(miss.status.code(), Some(2));
    assert!(text(&miss).contains("did you mean `inn.again`?"), "{}", text(&miss));

    // lamplight N10: a clue that defeats a negated premise is named, with
    // its producer; a bundle beat is labelled by its canonical id.
    write(
        &dir,
        "lore/gossip.lute",
        "---\nkind: lore\nid: town.gossip\n---\n\n\
         <beat id=\"whisper\" on=\"board\" target=\"place.dock\">\n  @bo: About Ada…\n  ::assert{rumor(ada)}\n</beat>\n",
    );
    let out = lute(&["scenario", d, "knowledge", "--for", "inn.again"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains("not rumor(ada) — asserted by beat `town.gossip.whisper` (lore/gossip.lute)"),
        "{s}"
    );
    assert!(
        s.contains("can be defeated by rumor(ada) [beat `town.gossip.whisper` (lore/gossip.lute)]"),
        "{s}"
    );
}

#[test]
fn scenario_graph_notes_references_it_cannot_draw() {
    let dir = town("graph-note");
    write(
        &dir,
        "quests/errand.lute",
        "---\nkind: quest\nid: town.quests\n---\n\n<quest id=\"errand\" title=\"Errand\" start=\"visited('inn.ada')\">\n  \
         <objective id=\"go\" title=\"Go\" done=\"run.day == 2\"/>\n</quest>\n",
    );
    write(
        &dir,
        "scenes/thanks.lute",
        &scene("town.thanks", "on: dayStart\nafter: 'completed(\"errand\")'\npriority: 3\n", "@ada: Thanks."),
    );
    let out = lute(&["scenario", dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("note: 2 `visited()`/`completed()`/`active()` reference(s) not drawn"), "{s}");
    assert!(s.contains("scene(town.thanks) -> completed(\"errand\") — quest(errand) declares no `after`"), "{s}");
    assert!(s.contains("quest(errand) reads visited('inn.ada')"), "{s}");
}

#[test]
fn play_marks_an_already_read_entry_candidate() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/play-hub");
    let dir = temp_dir("read");
    let script = write(
        &dir,
        "s.play.yaml",
        "steps:\n  - occasion: inbox\n    pick: megNote\n  - occasion: inbox\n    pick: none\n",
    );
    let (f, s) = (fixture.to_str().unwrap(), script.to_str().unwrap());
    let out = lute(&["play", f, "--script", s, "--json"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let v: Json = serde_json::from_slice(&out.stdout).unwrap();
    let meg = |step: usize| {
        v["steps"][step]["candidates"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == "megNote")
            .unwrap()
            .clone()
    };
    assert!(meg(0).get("read").is_none(), "unread before the pick: {}", meg(0));
    assert_eq!(meg(1)["read"], true);
    let human = lute(&["play", f, "--script", s]);
    assert!(
        String::from_utf8_lossy(&human.stdout).contains("megNote [entry, priority 5, read]"),
        "{}",
        text(&human)
    );
}
