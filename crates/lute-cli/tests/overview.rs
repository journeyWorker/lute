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

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

#[test]
fn calendar_visited_axis_gates_after_and_bad_axes_name_the_kinds() {
    let dir = town("cal-visited");
    let d = dir.to_str().unwrap();
    // `inn.again` needs `after: visited("inn.ada")` and a trusted ada.
    let save = write(&dir, "met.play.yaml", "facts: [\"met(ada)\"]\n");
    let args = [
        "--script", save.to_str().unwrap(), "--occasion", "placeVisit", "--target", "place.inn",
        "--axis", "visited('inn.ada')=true,false",
    ];
    let mut full = vec!["calendar", d, "--json"];
    full.extend(args);
    let out = lute(&full);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let v: Json = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["cells"][0]["at"]["visited('inn.ada')"], true);
    assert_eq!(result(&v, 0, "placeVisit", Some("place.inn"))["winner"], "inn.again");
    assert_ne!(result(&v, 1, "placeVisit", Some("place.inn"))["winner"], "inn.again");

    let err = |args: &[&str]| {
        let mut full = vec!["calendar", d];
        full.extend_from_slice(args);
        let out = lute(&full);
        assert_eq!(out.status.code(), Some(2), "{args:?}: {}", text(&out));
        stderr(&out)
    };
    let typo = err(&["--axis", "visited('inn.adda')=true"]);
    assert!(typo.contains("did you mean `inn.ada`?"), "{typo}");
    let maybe = err(&["--axis", "visited(\"inn.ada\")=maybe"]);
    assert!(maybe.contains("takes `true` (visited) and `false` (absent), not `maybe`"), "{maybe}");

    // An axis of no supported kind lists the kinds.
    let kinds = "an axis is one of: a declared state path (`run.day=1..7`), `quest.<id>.state=<status>,…`, \
                 `quest.<id>.objectives.<oid>.done=true,false`, `holds(<fact>)=true,false`, \
                 `visited('<scene or bundle-beat id>')=true,false`, \
                 `clock[=<d1>..<d2>]` (every slot of those days, in order)";
    let call = err(&["--axis", "completed('errand')=true"]);
    assert!(call.contains("`completed('errand')` is no axis the calendar can apply; ") && call.contains(kinds), "{call}");
    let path = err(&["--axis", "run.dy=1..2"]);
    assert!(path.contains("`run.dy` is not a declared state path in this project — did you mean `run.day`?"), "{path}");
    assert!(path.contains(kinds), "{path}");

    // A derived atom is named once.
    let derived = err(&["--axis", "holds(trusted(ada))=true"]);
    assert!(
        derived.contains("`--axis holds(trusted(ada))`: `trusted(ada)` is derived by rules and cannot be asserted"),
        "{derived}"
    );
}

#[test]
fn calendar_per_occasion_axes_evaluate_an_occasion_once_per_value() {
    let dir = town("cal-per-occasion");
    let v = calendar_json(&dir, &["--occasion", "dayStart@run.day", "--occasion", "placeVisit"]);
    let has = |n: usize, occ: &str| {
        v["cells"][n]["results"].as_array().unwrap().iter().any(|r| r["occasion"] == occ)
    };
    // Held at the first slot: once per day, in the morning cells only.
    assert_eq!((0..4).map(|n| has(n, "dayStart")).collect::<Vec<_>>(), [true, false, true, false]);
    assert!((0..4).all(|n| has(n, "placeVisit")), "an unrestricted occasion varies over every axis");
    let col = v["columns"].as_array().unwrap().iter().find(|c| c["occasion"] == "dayStart").unwrap();
    assert_eq!(col["varies"], serde_json::json!(["run.day"]));
    assert_eq!(col["heldAt"], serde_json::json!({ "run.slot": "morning" }));
    assert_eq!(result(&v, 2, "dayStart", None)["winner"], "town.day2");

    // `=value` holds the axis at another value.
    let v = calendar_json(&dir, &["--occasion", "dayStart@run.day,run.slot=night"]);
    assert_eq!(v["cells"][0]["results"].as_array().unwrap().len(), 0);
    assert_eq!(result(&v, 1, "dayStart", None)["winner"], "town.memo");

    let d = dir.to_str().unwrap();
    let base = ["calendar", d, "--axis", "run.day=1..2", "--axis", "run.slot=morning,night"];
    let mut args = base.to_vec();
    args.extend(["--occasion", "dayStart@run.day", "--occasion", "placeVisit"]);
    let out = lute(&args);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(s.contains("  dayStart: varies over run.day only, at run.slot=morning; blank elsewhere\n"), "{s}");
    let night = s.lines().find(|l| l.starts_with("1        night")).unwrap_or_else(|| panic!("{s}"));
    assert!(!night.contains("town."), "the night row leaves dayStart blank: {night}");
    args.push("--csv");
    let csv = String::from_utf8_lossy(&lute(&args).stdout).to_string();
    assert_eq!(csv.lines().filter(|l| l.contains(",dayStart,,first,")).count(), 2, "{csv}");

    for (extra, want) in [
        (["--occasion", "dayStart@run.dy"], "did you mean `run.day`?"),
        (["--occasion", "dayStart@run.slot=noon"], "`noon` is not a value of `--axis run.slot` (morning, night)"),
    ] {
        let mut full = base.to_vec();
        full.extend(extra);
        let out = lute(&full);
        assert_eq!(out.status.code(), Some(2), "{}", text(&out));
        assert!(stderr(&out).contains(want), "{}", text(&out));
    }
}

#[test]
fn calendar_facts_grid_and_never_presented_list() {
    let dir = town("cal-facts");
    let d = dir.to_str().unwrap();
    let base = ["calendar", d, "--axis", "run.day=1..2", "--axis", "run.slot=morning,night", "--occasion", "dayStart"];
    let mut args = base.to_vec();
    args.extend(["--facts", "present"]);
    let out = lute(&args);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let s = String::from_utf8_lossy(&out.stdout);
    assert!(
        s.contains(
            "facts present(person, place):\n\
             person  1/morning  1/night  2/morning  2/night\n\
             ada     -          inn      -          inn\n\
             bo      dock       -        -          -\n"
        ),
        "{s}"
    );
    // Always outranked on `select: first`: eligible everywhere, presented
    // nowhere, and what beat it.
    assert!(
        s.contains("  town.never [scene, scenes/never.lute] dayStart — lost to town.dawn; town.day2; town.memo\n"),
        "{s}"
    );
    assert!(s.contains("eligible but never presented in any cell: 2\n"), "{s}");

    let v = calendar_json(&dir, &["--occasion", "dayStart", "--facts", "present"]);
    assert_eq!(v["cells"][1]["facts"]["present"], serde_json::json!(["present(ada, inn)"]));
    assert_eq!(v["cells"][2]["facts"]["present"], serde_json::json!([]));
    let never = v["neverPresented"].as_array().unwrap();
    let ids: Vec<&str> = never.iter().map(|b| b["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["town.idle", "town.never"], "{never:?}");
    assert_eq!(never[1]["beatenBy"], serde_json::json!(["town.dawn", "town.day2", "town.memo"]));

    args.push("--csv");
    let csv = String::from_utf8_lossy(&lute(&args).stdout).to_string();
    assert!(csv.starts_with("run.day,run.slot,occasion,target,select,winner,presented,shadowed,unknown,notes,facts:present\n"), "{csv}");
    assert!(csv.contains("1,night,dayStart,,first,town.memo,town.memo,town.idle;town.never,,,\"present(ada, inn)\"\n"), "{csv}");
    assert!(
        csv.contains("\nneverPresented,kind,document,occasion,target,beatenBy\n\
                      town.idle,scene,scenes/idle.lute,dayStart,,town.dawn;town.day2;town.memo\n"),
        "{csv}"
    );

    let mut bad = base.to_vec();
    bad.extend(["--facts", "presnt"]);
    let out = lute(&bad);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(stderr(&out).contains("`--facts` names `presnt`, which is no relation in this project — did you mean `present`?"), "{}", text(&out));
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
    // dsl 0.24.0 T3-1: grouped by document; a negated premise nothing
    // produces is the good case and says so (was `NO PRODUCER`).
    assert!(s.contains("  scenes/inn-again.lute\n    scene `inn.again`\n"), "{s}");
    assert!(s.contains("trusted(ada) — derived by 1 rule"), "{s}");
    assert!(s.contains("rule: trusted(P) :- met(P), not rumor(P)"), "{s}");
    assert!(s.contains("met(ada) — asserted by scene `inn.ada` (scenes/inn-ada.lute)"), "{s}");
    assert!(s.contains("not rumor(ada) — always holds (nothing produces rumor(ada)"), "{s}");
    assert!(s.contains("— cannot be defeated"), "{s}");
    assert!(!s.contains("defeated when"), "nothing produces rumor(ada) yet: {s}");
    assert!(!s.contains("innShut"), "--for selects one node: {s}");

    // A ground query follows only the rules that can conclude it.
    let all = String::from_utf8_lossy(&lute(&["scenario", d, "knowledge"]).stdout).to_string();
    let inn_shut = all.split("entry `innShut`").nth(1).unwrap().split("\n\n").next().unwrap();
    assert!(inn_shut.contains("not present(ada, inn) — holds unless defeated"), "{inn_shut}");
    assert!(inn_shut.contains("rule: present(ada, inn) :-"), "{inn_shut}");
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
    assert!(s.contains("not rumor(ada) — holds unless defeated\n"), "{s}");
    assert!(
        s.contains("defeated when rumor(ada) is asserted by beat `town.gossip.whisper` (lore/gossip.lute)"),
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

/// dsl 0.24.0 T3-12: `lute beats` shows a beat's `when` as the author wrote
/// it; `--expand` shows the expansion (no parentheses around an atomic
/// argument); `--json` carries both.
#[test]
fn beats_show_the_authored_when_and_expand_on_request() {
    let dir = town("beats-authored");
    let world = std::fs::read_to_string(dir.join("world.schema.yaml")).unwrap();
    write(
        &dir,
        "world.schema.yaml",
        &format!("{world}defs:\n  daysAtLeast: {{ type: bool, cel: \"run.day >= n\", params: {{ n: number }} }}\n"),
    );
    write(
        &dir,
        "scenes/late.lute",
        &scene("town.late", "on: dayStart\nonce: false\nwhen: '@daysAtLeast(2)'\npriority: 3\n", "@narrator: Late."),
    );
    let d = dir.to_str().unwrap();
    let row = |args: &[&str]| {
        let out = lute(args);
        assert_eq!(out.status.code(), Some(0), "{}", text(&out));
        let s = String::from_utf8_lossy(&out.stdout).to_string();
        s.lines().find(|l| l.contains("town.late")).map(str::to_string).unwrap_or_else(|| panic!("{s}"))
    };
    let authored = row(&["beats", d, "--occasion", "dayStart"]);
    assert!(authored.trim_end().ends_with("@daysAtLeast(2)"), "{authored}");
    let expanded = row(&["beats", d, "--occasion", "dayStart", "--expand"]);
    assert!(expanded.trim_end().ends_with("(run.day >= 2)"), "{expanded}");

    let out = lute(&["beats", d, "--occasion", "dayStart", "--json"]);
    let v: Json = serde_json::from_slice(&out.stdout).unwrap();
    let late = v["roots"][0]["ladders"][0]["beats"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == "town.late")
        .unwrap()
        .clone();
    assert_eq!(late["when"], "(run.day >= 2)");
    assert_eq!(late["whenAuthored"], "@daysAtLeast(2)");
}
