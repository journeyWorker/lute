//! dsl 0.27.0 §4 (T2-5): a finite clock — `clock: { last: … }` / `days: N`
//! — through the built `lute` binary, over a copy of hollow-ward's one-night
//! clock (`h23` … `h05`, `raise: { slot: hourStrikes, dayEnd: dawn }`).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-finite-{tag}-{}-{n}", std::process::id()));
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

const LAST: &str = "  last: { day: 1, slot: h05 }\n";

const STRIKE: &str = "---\nkind: scene\nid: ward.strike\nuses: ../world.schema.yaml\non: hourStrikes\n\
                      once: false\n---\n\n## Strike\n\n@narrator: The clock strikes at {{run.hour}}.\n";
const DAWN: &str = "---\nkind: scene\nid: ward.dawn\nuses: ../world.schema.yaml\non: dawn\n\
                    once: false\n---\n\n## Dawn\n\n@narrator: Dawn breaks on night {{run.night}}.\n";

/// hollow-ward's clock, with `bound` (`last:` / `days:` lines, or `""` for
/// an unbounded clock), plus the strike and dawn scenes and `docs`.
fn ward(tag: &str, bound: &str, docs: &[(&str, &str)]) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    );
    write(
        &dir,
        "world.schema.yaml",
        &format!(
            "state:\n  run.night: {{ type: number, default: 1, owner: engine }}\n  \
             run.hour: {{ type: {{ enum: [h23, h00, h01, h02, h03, h04, h05] }}, default: h23, owner: engine }}\n\
             clock:\n  day: run.night\n  slot: run.hour\n  slots: [h23, h00, h01, h02, h03, h04, h05]\n  \
             raise: {{ slot: hourStrikes, dayEnd: dawn }}\n{bound}"
        ),
    );
    write(&dir, "scenes/strike.lute", STRIKE);
    write(&dir, "scenes/dawn.lute", DAWN);
    for (rel, body) in docs {
        write(&dir, rel, body);
    }
    dir
}

fn play(dir: &Path, script: &str, json: bool) -> Output {
    write(dir, "s.play.yaml", script);
    let mut cmd = Command::new(BIN);
    cmd.args(["play", &dir.display().to_string(), "--script"])
        .arg(dir.join("s.play.yaml"));
    if json {
        cmd.arg("--json");
    }
    cmd.output().unwrap()
}

fn check_project(dir: &Path) -> Output {
    Command::new(BIN)
        .arg("check-project")
        .arg(dir)
        .output()
        .unwrap()
}

/// Advancing from the last position (h05) raises `dawn` once and ends the
/// clock — no `night 2, h23`, no second `hourStrikes`; the next `advance:`
/// is `E-CLOCK-END` (exit 1). `days: 1` is the same clock.
#[test]
fn advancing_past_the_last_position_raises_the_last_day_end_once_and_ends_the_clock() {
    for bound in [LAST, "  days: 1\n"] {
        let dir = ward("end", bound, &[]);
        let out = play(
            &dir,
            "steps:\n  - advance: 6\n  - advance: slot\n  - advance: slot\n",
            false,
        );
        let t = text(&out);
        assert_eq!(out.status.code(), Some(1), "{t}");
        // Step 1 lands exactly on the last position: an ordinary advance.
        assert!(
            t.contains("step 1 · advance 6: day 1 h23 → day 1 h05 ──"),
            "{t}"
        );
        assert!(t.contains("The clock strikes at h05."), "{t}");
        // Step 2 leaves it: dawn, once, and the clock ends.
        assert!(
            t.contains(
                "step 2 · advance slot: day 1 h05 → day 1 h05 · the clock ends (its last position)"
            ),
            "{t}"
        );
        assert_eq!(t.matches("Dawn breaks on night 1.").count(), 1, "{t}");
        assert!(
            !t.contains("night 2") && !t.contains("run.night = 2"),
            "{t}"
        );
        assert_eq!(t.matches("The clock strikes").count(), 1, "{t}");
        assert!(
            t.contains(
                "step 3: `advance:` past the clock's last position (day 1 h05) — the clock ended; \
                 a `newRun` starts it over (E-CLOCK-END)"
            ),
            "{t}"
        );
    }
}

/// An advance whose destination lies well past the end walks to the last
/// position and ends there; `--json` marks it `ended`.
#[test]
fn a_long_advance_stops_at_the_last_position() {
    let dir = ward("long", LAST, &[]);
    let out = play(&dir, "steps:\n  - advance: 20\n", true);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let adv = &v["steps"][0]["advance"];
    assert_eq!(adv["to"], "day 1 h05", "{t}");
    assert_eq!(adv["ended"], true, "{t}");
    assert_eq!(adv["days"].as_array().map(Vec::len), Some(1), "{t}");
    assert_eq!(adv["days"][0]["winner"], "ward.dawn", "{t}");
    assert!(v["steps"][0].get("winner").is_none(), "no slot raise: {t}");

    // A clock that never ends keeps rolling into the next night.
    let dir = ward("unbounded", "", &[]);
    let out = play(&dir, "steps:\n  - advance: 20\n", true);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        v["steps"][0]["advance"]["to"],
        "day 3 h05",
        "{}",
        text(&out)
    );
    assert!(v["steps"][0]["advance"].get("ended").is_none());
}

/// A `newRun` starts the clock over; an `engine:` write that moves the day
/// past the end makes the next advance `E-CLOCK-END` too.
#[test]
fn new_run_restarts_the_clock_and_a_start_past_the_end_is_refused() {
    let dir = ward("newrun", LAST, &[]);
    let out = play(
        &dir,
        "steps:\n  - advance: 20\n  - newRun: true\n  - advance: slot\n",
        false,
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(
        t.contains("step 3 · advance slot: day 1 h23 → day 1 h00 ──"),
        "{t}"
    );
    assert!(t.contains("The clock strikes at h00."), "{t}");

    let out = play(
        &dir,
        "steps:\n  - engine: { state: { run.night: 2 } }\n  - advance: slot\n",
        false,
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains("step 2: `advance:` past the clock's last position (day 1 h05) — the clock stands at day 2 h23"),
        "{t}"
    );
}

/// `lute test` runs a play file through the same session: the E-CLOCK-END
/// advance fails the play.
#[test]
fn lute_test_fails_a_play_advancing_an_ended_clock() {
    let dir = ward("test", LAST, &[]);
    write(
        &dir,
        "plays/night.play.yaml",
        "steps:\n  - advance: 20\n    expect: { presented: [ward.dawn] }\n  - advance: slot\n",
    );
    let out = Command::new(BIN)
        .arg("test")
        .arg(dir.join("plays"))
        .output()
        .unwrap();
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("E-CLOCK-END"), "{t}");

    write(
        &dir,
        "plays/night.play.yaml",
        "steps:\n  - advance: 20\n    expect: { presented: [ward.dawn] }\n",
    );
    let out = Command::new(BIN)
        .arg("test")
        .arg(dir.join("plays"))
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", text(&out));
}

/// The checker ranges the clock: a `when` needing a position past the end
/// is unreachable, naming the clock's end; one inside it stays clean.
#[test]
fn a_when_past_the_last_position_is_unreachable() {
    let scene = |id: &str, when: &str| {
        format!(
            "---\nkind: scene\nid: {id}\nuses: ../world.schema.yaml\non: hourStrikes\n\
             when: \"{when}\"\n---\n\n## S\n\n@narrator: {id}.\n"
        )
    };
    let lore = "---\nkind: lore\nid: notes\nuses: ../world.schema.yaml\n---\n\n\
                <entry id=\"late\" on=\"hourStrikes\" when=\"clock.index >= 7\">\n  @narrator: Too late.\n</entry>\n";
    let docs = [
        ("scenes/second.lute", scene("ward.second", "run.night == 2")),
        ("scenes/after.lute", scene("ward.after", "clock.index >= 9")),
        // Priority 1: at h05 it and `ward.strike` (no `when`) are both
        // eligible — a genuine `W-BEAT-PRIORITY-TIE` otherwise.
        (
            "scenes/four.lute",
            scene("ward.four", "clock.index >= 6 && run.night == 1")
                .replace("on: hourStrikes\n", "on: hourStrikes\npriority: 1\n"),
        ),
        ("lore/notes.lute", lore.to_string()),
    ];
    let docs: Vec<(&str, &str)> = docs.iter().map(|(p, s)| (*p, s.as_str())).collect();
    let dir = ward("unreachable", LAST, &docs);
    let out = check_project(&dir);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains("[E-BEAT-UNREACHABLE] beat `ward.second` is never eligible: its `when` `run.night == 2` is provably false — the clock ends at its last position, so `run.night` only ranges over 1..1"),
        "{t}"
    );
    assert!(t.contains("beat `ward.after` is never eligible"), "{t}");
    assert!(
        t.contains("[E-ENTRY-UNREACHABLE] entry `late` is never eligible"),
        "{t}"
    );
    assert!(
        !t.contains("four.lute:"),
        "a reachable `when` stays clean: {t}"
    );

    // The same guards over a clock that never ends are clean.
    let dir = ward("reachable", "", &docs);
    let out = check_project(&dir);
    assert!(out.status.success(), "{}", text(&out));
}

/// HW27-05: a `by=` deadline the finite clock can never reach never fails
/// its objective — `W-DEADLINE-NEVER` at the `by`, naming the clock's end.
/// A deadline inside the clock, and the same deadlines on a clock that
/// never ends, stay clean.
#[test]
fn a_deadline_past_the_last_position_never_fails() {
    let quest = "---\nkind: quest\nid: q\nuses: ../world.schema.yaml\n---\n\n\
                 <quest id=\"night\" title=\"Night\" start=\"true\">\n  \
                 <objective id=\"leave\" title=\"Leave\" done=\"clock.index >= 6\" by=\"run.night >= 2\"/>\n  \
                 <objective id=\"hide\" title=\"Hide\" done=\"clock.index >= 6\" by=\"clock.index >= 9\"/>\n  \
                 <objective id=\"wait\" title=\"Wait\" done=\"clock.index >= 6\" by=\"clock.index >= 6\"/>\n\
                 </quest>\n";
    let docs = [("quests/q.lute", quest)];
    let t = text(&check_project(&ward("deadline", LAST, &docs)));
    assert!(
        t.contains("q.lute:8:") && t.contains(
            "[W-DEADLINE-NEVER] objective `leave` never fails: its deadline `by: run.night >= 2` \
             can never hold (the clock ends at its last position, so `run.night` only ranges over 1..1)"
        ),
        "{t}"
    );
    assert!(
        t.contains("objective `hide` never fails: its deadline `by: clock.index >= 9`"),
        "{t}"
    );
    assert_eq!(t.matches("[W-DEADLINE-NEVER]").count(), 2, "{t}");
    let t = text(&check_project(&ward("deadline-open", "", &docs)));
    assert!(!t.contains("W-DEADLINE-NEVER"), "{t}");
}

/// A scene on `hourStrikes` behind `when`, at its own priority (so it never
/// ties `ward.strike` or another).
fn hour_scene(id: &str, when: &str) -> String {
    let priority = 1 + id.bytes().map(u32::from).sum::<u32>() % 97;
    format!(
        "---\nkind: scene\nid: {id}\nuses: ../world.schema.yaml\non: hourStrikes\n\
         priority: {priority}\nwhen: \"{when}\"\n---\n\n## S\n\n@narrator: {id}.\n"
    )
}

/// On a clock of several nights, the last night only reaches the last slot:
/// a slot after it on that night is unreachable, naming what the slot
/// holds there. The same slot on an earlier night, or an earlier slot on
/// the last night, stays clean.
#[test]
fn a_slot_past_the_last_slot_on_the_last_day_is_unreachable() {
    let docs = [
        (
            "scenes/late.lute",
            hour_scene("ward.late", "run.night == 2 && run.hour == 'h04'"),
        ),
        (
            "scenes/first.lute",
            hour_scene("ward.first", "run.night == 1 && run.hour == 'h04'"),
        ),
        (
            "scenes/early.lute",
            hour_scene("ward.early", "run.night == 2 && run.hour == 'h01'"),
        ),
    ];
    let docs: Vec<(&str, &str)> = docs.iter().map(|(p, s)| (*p, s.as_str())).collect();
    let t = text(&check_project(&ward(
        "last-day",
        "  last: { day: 2, slot: h02 }\n",
        &docs,
    )));
    assert!(
        t.contains(
            "[E-BEAT-UNREACHABLE] beat `ward.late` is never eligible: its `when` \
             `run.night == 2 && run.hour == 'h04'` is provably false — the clock ends at its last \
             position, so on day 2 `run.hour` only holds h23, h00, h01, h02"
        ),
        "{t}"
    );
    assert_eq!(t.matches("[E-BEAT-UNREACHABLE]").count(), 1, "{t}");
}

/// A conjunction of clock reads no position satisfies is provably false —
/// on a clock that never ends too. One a position satisfies stays clean.
#[test]
fn clock_reads_no_position_satisfies_are_false() {
    let docs = [
        (
            "scenes/over.lute",
            hour_scene(
                "ward.over",
                "run.night == 1 && run.hour == 'h01' && clock.index > 2",
            ),
        ),
        (
            "scenes/later.lute",
            hour_scene(
                "ward.later",
                "run.night == 2 && run.hour == 'h01' && clock.index > 2",
            ),
        ),
    ];
    let docs: Vec<(&str, &str)> = docs.iter().map(|(p, s)| (*p, s.as_str())).collect();
    let t = text(&check_project(&ward("positions", "", &docs)));
    assert!(
        t.contains(
            "[E-BEAT-UNREACHABLE] beat `ward.over` is never eligible: its `when` \
             `run.night == 1 && run.hour == 'h01' && clock.index > 2` is provably false — no clock \
             position has `run.night == 1`, `run.hour == 'h01'` and `clock.index > 2`"
        ),
        "{t}"
    );
    assert!(
        !t.contains("later.lute:"),
        "a satisfiable conjunction stays clean: {t}"
    );
}

/// An objective whose `done` can only hold once its deadline already does
/// fails before it can be done: `W-DEADLINE-BEFORE-WINDOW` at the `by`. A
/// `visited` beat whose `when` is a derived schedule opens the window when
/// its rule's guard holds; the deadline is judged on arrival, before the
/// beat is presented. A deadline after the window, and a `done` over the
/// clock that comes true with the deadline (`done` wins the tie), are clean.
#[test]
fn a_deadline_before_the_only_window_fails_the_objective() {
    let bound = "  last: { day: 2, slot: h05 }\nentities:\n  room: { members: [hall, cell] }\n\
                 relations:\n  lit: { args: [room], derive: true }\n\
                 rules:\n  - \"lit(hall) :- cel(\\\"run.night == 2 && run.hour == 'h03'\\\")\"\n";
    let lamp = hour_scene("ward.lamp", "holds(lit(hall))");
    let quest = "---\nkind: quest\nid: q\nuses: ../world.schema.yaml\n---\n\n\
                 <quest id=\"night\" title=\"Night\" start=\"true\">\n  \
                 <objective id=\"lamp\" title=\"Lamp\" done=\"visited('ward.lamp')\" by=\"clock.index > 8\"/>\n  \
                 <objective id=\"late\" title=\"Late\" done=\"visited('ward.lamp')\" by=\"clock.index > 11\"/>\n  \
                 <objective id=\"hour\" title=\"Hour\" done=\"clock.index >= 11\" by=\"clock.index > 9\"/>\n  \
                 <objective id=\"tie\" title=\"Tie\" done=\"clock.index >= 11\" by=\"clock.index > 10\"/>\n\
                 </quest>\n";
    let docs = [
        ("scenes/lamp.lute", lamp.as_str()),
        ("quests/q.lute", quest),
    ];
    let t = text(&check_project(&ward("window", bound, &docs)));
    assert!(
        t.contains(
            "[W-DEADLINE-BEFORE-WINDOW] objective `lamp` fails before it can be done: its `done` \
             `visited('ward.lamp')` can first hold at day 2 h03, but its deadline \
             `by: clock.index > 8` already holds at day 2 h01 — move the deadline after that window"
        ),
        "{t}"
    );
    assert!(
        t.contains("objective `hour` fails before it can be done"),
        "{t}"
    );
    assert_eq!(t.matches("[W-DEADLINE-BEFORE-WINDOW]").count(), 2, "{t}");
}

#[test]
fn a_last_position_the_clock_does_not_have_is_a_clock_decl_error() {
    for (bound, needle) in [
        (
            "  last: { day: 1, slot: h06 }\n",
            "`last.slot: h06` is not one of `slots:`",
        ),
        ("  days: 0\n", "`days:` must be at least 1"),
        (
            "  days: 1\n  last: { day: 1 }\n",
            "both `last:` and `days:`",
        ),
        ("  last: { day: 0 }\n", "`last.day` is 0"),
        // HW27-13: an unknown key gets a did-you-mean, a non-number days
        // no serde text.
        (
            "  lats: { day: 1 }\n",
            "has no key `lats` — did you mean `last`?",
        ),
        (
            "  days: one\n",
            "`days: \"one\"` must be a whole number, e.g. `days: 1`",
        ),
    ] {
        let dir = ward("decl", bound, &[]);
        let t = text(&check_project(&dir));
        assert!(
            t.contains("[E-CLOCK-DECL]") && t.contains(needle),
            "{bound}: {t}"
        );
    }
}

/// `lute calendar --axis clock` stops at the last position: bare `clock`
/// is the one night, and a day past the end is a usage error.
#[test]
fn the_calendar_clock_axis_ends_at_the_last_position() {
    let dir = ward("calendar", "  last: { day: 1, slot: h03 }\n", &[]);
    let calendar = |args: &[&str]| {
        Command::new(BIN)
            .arg("calendar")
            .arg(&dir)
            .args(args)
            .output()
            .unwrap()
    };
    let out = calendar(&["--axis", "clock", "--occasion", "hourStrikes", "--csv"]);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(
        t.contains("1 h03") && !t.contains("1 h04") && !t.contains("2 h23"),
        "{t}"
    );
    let out = calendar(&["--axis", "clock=1..2", "--occasion", "hourStrikes"]);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(2), "{t}");
    assert!(
        t.contains("past the clock's last position (day 1 h03)"),
        "{t}"
    );
}
