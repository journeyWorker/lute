//! dsl 0.27.0 §5 (T2-6, T2-7, T3-25): `once: week`, `spentBy`, quest
//! `rearm`, and seasons — checked and played through the built `lute`
//! binary over small temp projects.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-cadence-{tag}-{}-{n}", std::process::id()));
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

/// A project whose `world.schema.yaml` holds `schema` (state, clock,
/// seasons), plus every `(path, text)` document.
fn project(tag: &str, schema: &str, docs: &[(&str, &str)]) -> PathBuf {
    let dir = temp_dir(tag);
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    );
    write(&dir, "world.schema.yaml", schema);
    for (rel, body) in docs {
        write(&dir, rel, body);
    }
    dir
}

fn check_project(dir: &Path) -> Output {
    Command::new(BIN)
        .arg("check-project")
        .arg(dir)
        .output()
        .unwrap()
}

fn play(dir: &Path, script: &str) -> Output {
    write(dir, "s.play.yaml", script);
    Command::new(BIN)
        .args(["play", &dir.display().to_string(), "--script"])
        .arg(dir.join("s.play.yaml"))
        .output()
        .unwrap()
}

const DAY_CLOCK: &str = "state:\n  run.day: { type: number, default: 1, owner: engine }\n  \
     run.solved: { type: bool, default: false, owner: engine }\n\
     clock:\n  day: run.day\n";

const WEEK_CLOCK: &str =
    "  week: { length: 7, first: 0, labels: [Mon, Tue, Wed, Thu, Fri, Sat, Sun] }\n";

fn scene(id: &str, keys: &str, line: &str) -> String {
    format!(
        "---\nkind: scene\nid: {id}\nuses: ../world.schema.yaml\non: visit\n{keys}---\n\n## S\n\n\
         @narrator: {line}\n"
    )
}

#[test]
fn once_week_respawns_when_the_week_turns() {
    let dir = project(
        "week",
        &format!("{DAY_CLOCK}{WEEK_CLOCK}"),
        &[(
            "scenes/reset.lute",
            &scene("tower.reset", "once: week\n", "The tower floors reset."),
        )],
    );
    let out = check_project(&dir);
    assert!(out.status.success(), "{}", text(&out));
    let out = play(
        &dir,
        "steps:\n  - occasion: visit\n  - advance: day\n  - occasion: visit\n  \
         - advance: { to: { weekday: 0 } }\n  - occasion: visit\n",
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(
        t.contains("once: week — already presented this week"),
        "{t}"
    );
    assert_eq!(t.matches("The tower floors reset.").count(), 2, "{t}");
}

#[test]
fn once_week_needs_a_clock_week() {
    let dir = project(
        "week-no-week",
        DAY_CLOCK,
        &[(
            "scenes/reset.lute",
            &scene("tower.reset", "once: week\n", "Reset."),
        )],
    );
    let t = text(&check_project(&dir));
    assert!(
        t.contains("E-BEAT-ATTR") && t.contains("declares no `week:`"),
        "{t}"
    );
}

#[test]
fn spent_by_keeps_a_beat_until_its_condition_holds() {
    let dir = project(
        "spent-by",
        DAY_CLOCK,
        &[(
            "scenes/valves.lute",
            &scene(
                "ward.valves",
                "spentBy: \"run.solved\"\n",
                "The valves hiss.",
            ),
        )],
    );
    let out = check_project(&dir);
    assert!(out.status.success(), "{}", text(&out));
    let out = play(
        &dir,
        "steps:\n  - occasion: visit\n  - occasion: visit\n  \
         - engine: { state: { run.solved: true } }\n  - occasion: visit\n",
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert_eq!(t.matches("The valves hiss.").count(), 2, "{t}");
    assert!(t.contains("spentBy: `run.solved` holds"), "{t}");
    // `spentBy` replaces `once`.
    let dir = project(
        "spent-by-once",
        DAY_CLOCK,
        &[(
            "scenes/valves.lute",
            &scene("ward.valves", "once: user\nspentBy: \"run.solved\"\n", "x"),
        )],
    );
    let t = text(&check_project(&dir));
    assert!(
        t.contains("E-BEAT-ATTR") && t.contains("`spentBy:` replaces `once:`"),
        "{t}"
    );
}

const SEASON_SCHEMA: &str = "state:\n  run.day: { type: number, default: 1, owner: engine }\n  \
     season.harvest.tokens: { type: number, default: 0 }\n\
     clock:\n  day: run.day\n\
     defs:\n  harvestLive: \"(run.day >= 3 && run.day <= 4) || (run.day >= 8 && run.day <= 9)\"\n\
     seasons:\n  harvest: { live: \"@harvestLive\" }\n";

const QUESTS: &str = "---\nkind: quest\nid: events\nuses: ../world.schema.yaml\n---\n\n\
     <quest id=\"missions\" title=\"Missions\" tier=\"season:harvest\" start=\"@harvestLive\">\n  \
     <objective id=\"tokens\" title=\"Tokens\" done=\"season.harvest.tokens >= 2\"/>\n</quest>\n\n\
     <quest id=\"festival\" title=\"Festival\" start=\"@harvestLive\" rearm=\"@harvestLive\">\n  \
     <objective id=\"one\" title=\"One\" done=\"season.harvest.tokens >= 1\"/>\n</quest>\n";

#[test]
fn a_season_reopening_resets_its_tier_and_a_rearm_takes_the_quest_again() {
    let dir = project("season", SEASON_SCHEMA, &[("quests/events.lute", QUESTS)]);
    let out = check_project(&dir);
    assert!(out.status.success(), "{}", text(&out));
    let out = play(
        &dir,
        "steps:\n  - advance: 2\n    expect: { quests: { missions: active, festival: active } }\n  \
         - engine: { state: { season.harvest.tokens: 2 } }\n    \
         expect: { quests: { missions: complete, festival: complete } }\n  \
         - advance: 2\n  - advance: 3\n    \
         expect:\n      quests: { missions: active, festival: active }\n      \
         state: { season.harvest.tokens: 0, prev.season.harvest.tokens: 2 }\n",
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("season harvest opens"), "{t}");
    assert!(t.contains("season harvest closes"), "{t}");
    assert!(
        t.contains("quest missions -> unset (season:harvest opened; was complete)"),
        "{t}"
    );
    assert!(
        t.contains("quest festival -> unset (rearmed; was complete)"),
        "{t}"
    );
}

#[test]
fn one_long_advance_settles_every_day_it_crosses() {
    // Day 3 opens the window; the next advance crosses day 5 (closes), day
    // 8 (opens again: both quests reset and start afresh) and lands on day
    // 10 (closed) — all inside one step.
    let dir = project(
        "season-long",
        SEASON_SCHEMA,
        &[("quests/events.lute", QUESTS)],
    );
    let out = check_project(&dir);
    assert!(out.status.success(), "{}", text(&out));
    let out = play(
        &dir,
        "steps:\n  - advance: 2\n  \
         - engine: { state: { season.harvest.tokens: 2 } }\n    \
         expect: { quests: { missions: complete, festival: complete } }\n  \
         - advance: 7\n    \
         expect:\n      quests: { missions: active, festival: active }\n      \
         state: { run.day: 10, season.harvest.tokens: 0, prev.season.harvest.tokens: 2 }\n",
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    // The first window has no last one to mirror.
    let (first, last) = t.split_at(t.find("advance 7").expect("the third step's header"));
    assert!(
        first.contains("season harvest opens — season.harvest.* reset to defaults\n"),
        "{t}"
    );
    assert_eq!(last.matches("season harvest closes").count(), 2, "{t}");
    assert_eq!(last.matches("season harvest opens").count(), 1, "{t}");
    assert!(
        last.contains("last window: prev.season.harvest.tokens = 2"),
        "{t}"
    );
    // Each crossed position that moved a quest prints its clock move first.
    let at = |s: &str| last.find(s).unwrap_or_else(|| panic!("`{s}` in\n{t}"));
    assert!(at("set run.day = 5") < at("season harvest closes"), "{t}");
    assert!(at("set run.day = 8") < at("season harvest opens"), "{t}");
    assert!(
        at("season harvest opens")
            < at("quest missions -> unset (season:harvest opened; was complete)"),
        "{t}"
    );
    assert!(
        at("quest festival -> unset (rearmed; was complete)") < at("quest festival -> active"),
        "{t}"
    );
    assert!(
        at("quest missions -> active") < at("set run.day = 10"),
        "{t}"
    );
}

/// G-3: on a clock that raises no `dayStart` / `dayEnd`, a season window
/// that opens and closes inside one `advance:` starts its season-tier quest
/// where it opens and fails its deadline where it closes.
#[test]
fn one_long_advance_starts_and_fails_a_season_quest_inside_the_window() {
    let schema = "state:\n  run.day: { type: number, default: 1, owner: engine }\n  \
                  run.slot: { type: { enum: [morning, night] }, default: morning, owner: engine }\n  \
                  season.fair.stalls: { type: number, default: 0 }\n\
                  clock:\n  day: run.day\n  slot: run.slot\n  slots: [morning, night]\n\
                  defs:\n  fairLive: \"run.day == 3\"\n\
                  seasons:\n  fair: { live: \"@fairLive\" }\n";
    let quests = "---\nkind: quest\nid: fair\nuses: ../world.schema.yaml\n---\n\n\
                  <quest id=\"stalls\" title=\"Stalls\" tier=\"season:fair\" start=\"@fairLive\">\n  \
                  <objective id=\"one\" title=\"One stall\" done=\"season.fair.stalls >= 1\" \
                  by=\"!@fairLive\"/>\n</quest>\n";
    let dir = project("season-inside", schema, &[("quests/fair.lute", quests)]);
    let out = check_project(&dir);
    assert!(out.status.success(), "{}", text(&out));
    // Day 1 morning -> day 4 morning: the fair opens on day 3 and closes
    // before the clock stops.
    let out = play(
        &dir,
        "steps:\n  - advance: 6\n    \
         expect: { clock: { day: 4 }, quests: { stalls: failed } }\n",
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    let at = |s: &str| t.find(s).unwrap_or_else(|| panic!("`{s}` in\n{t}"));
    assert!(at("set run.day = 3") < at("season fair opens"), "{t}");
    assert!(
        at("season fair opens") < at("quest stalls -> active"),
        "{t}"
    );
    assert!(at("quest stalls -> active") < at("set run.day = 4"), "{t}");
    assert!(
        at("set run.day = 4") < at("quest stalls -> failed (by)"),
        "{t}"
    );
}

/// G-3: a `rearm` fires at the crossed position where its condition turns
/// true, and the quest's `start` is judged there too — not at an arrival
/// where both are false.
#[test]
fn one_long_advance_rearms_at_the_crossed_position() {
    let schema = format!(
        "state:\n  run.day: {{ type: number, default: 1, owner: engine }}\n  \
         run.floors: {{ type: number, default: 0, owner: engine }}\n\
         clock:\n  day: run.day\n{WEEK_CLOCK}\
         defs:\n  monday: \"clock.weekday == 0\"\n"
    );
    let quests = "---\nkind: quest\nid: tower\nuses: ../world.schema.yaml\n---\n\n\
                  <quest id=\"weekly\" title=\"Weekly\" start=\"@monday\" rearm=\"@monday\">\n  \
                  <objective id=\"climb\" title=\"Climb\" done=\"run.floors >= 10\"/>\n</quest>\n";
    let dir = project("rearm-cross", &schema, &[("quests/tower.lute", quests)]);
    let out = check_project(&dir);
    assert!(out.status.success(), "{}", text(&out));
    // Monday day 1: taken and done; the floors reset; Friday -> Tuesday
    // crosses Monday day 8.
    let out = play(
        &dir,
        "steps:\n  - engine: { state: { run.floors: 10 } }\n    \
         expect: { quests: { weekly: complete } }\n  \
         - engine: { state: { run.floors: 0 } }\n  \
         - advance: 4\n    expect: { quests: { weekly: complete } }\n  \
         - advance: { to: { weekday: Tue } }\n    \
         expect: { clock: { day: 9 }, quests: { weekly: active } }\n",
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    let last = &t[t.find("advance to Tue").expect("the last step's header")..];
    let at = |s: &str| last.find(s).unwrap_or_else(|| panic!("`{s}` in\n{t}"));
    assert!(
        at("set run.day = 8") < at("quest weekly -> unset (rearmed; was complete)"),
        "{t}"
    );
    assert!(
        at("quest weekly -> unset (rearmed; was complete)") < at("quest weekly -> active"),
        "{t}"
    );
    assert!(at("quest weekly -> active") < at("set run.day = 9"), "{t}");
    assert_eq!(last.matches("rearmed").count(), 1, "{t}");
}

#[test]
fn one_long_advance_observes_every_slot_it_crosses() {
    let schema = "state:\n  run.day: { type: number, default: 1, owner: engine }\n  \
                  run.slot: { type: { enum: [morning, afternoon, night] }, default: morning, owner: engine }\n  \
                  season.fair.stalls: { type: number, default: 0 }\n\
                  clock:\n  day: run.day\n  slot: run.slot\n  slots: [morning, afternoon, night]\n\
                  seasons:\n  fair: { live: \"run.slot == 'afternoon'\" }\n";
    let dir = project(
        "season-slots",
        schema,
        &[("scenes/a.lute", &scene("a.b", "", "Hi."))],
    );
    let out = check_project(&dir);
    assert!(out.status.success(), "{}", text(&out));
    // Afternoon of day 1 (the fair opens) -> night of day 2: the advance
    // crosses that night (closes), day 2's afternoon (opens, mirroring the
    // first window) and lands on its night (closes).
    let out = play(
        &dir,
        "steps:\n  - advance: 1\n  - engine: { state: { season.fair.stalls: 4 } }\n  \
         - advance: 4\n    \
         expect: { state: { season.fair.stalls: 0, prev.season.fair.stalls: 4 } }\n",
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    let last = &t[t.find("advance 4").expect("the third step's header")..];
    assert_eq!(last.matches("season fair closes").count(), 2, "{t}");
    assert!(last.contains("season fair opens — season.fair.* reset to defaults; last window: prev.season.fair.stalls = 4"), "{t}");
}

#[test]
fn an_undeclared_season_is_a_season_decl_error() {
    let quests = QUESTS.replace("season:harvest", "season:harvset");
    let dir = project(
        "season-undeclared",
        SEASON_SCHEMA,
        &[("quests/events.lute", &quests)],
    );
    let t = text(&check_project(&dir));
    assert!(
        t.contains("E-SEASON-DECL") && t.contains("did you mean `harvest`"),
        "{t}"
    );
    let schema = SEASON_SCHEMA.replace("season.harvest.tokens", "season.winter.tokens");
    let dir = project(
        "season-path",
        &schema,
        &[("scenes/a.lute", &scene("a.b", "", "Hi."))],
    );
    let t = text(&check_project(&dir));
    assert!(
        t.contains("E-SEASON-DECL") && t.contains("season.winter.tokens"),
        "{t}"
    );
}

/// G-4: a season fault in a schema is one error at the schema's own line,
/// however many documents import it — never a copy at `1:1` of each
/// document. G-5: a season `live:` that is a bare non-bool path is the same
/// `E-REF-TYPE` a number-typed `@def` gets there.
#[test]
fn a_season_fault_in_a_schema_is_reported_once_at_the_schema() {
    let docs = [
        ("scenes/a.lute", scene("a.b", "", "Hi.")),
        ("scenes/c.lute", scene("c.d", "", "Ho.")),
        ("scenes/e.lute", scene("e.f", "", "Ha.")),
    ];
    let docs: Vec<(&str, &str)> = docs.iter().map(|(p, t)| (*p, t.as_str())).collect();
    let cases = [
        (
            "typo",
            SEASON_SCHEMA.replace("{ live:", "{ lvie:"),
            "[E-SEASON-DECL]",
        ),
        (
            "undeclared",
            SEASON_SCHEMA.replace("season.harvest.tokens", "season.winter.tokens"),
            "[E-SEASON-DECL] state path `season.winter.tokens`",
        ),
        (
            "live-number",
            SEASON_SCHEMA.replace("\"@harvestLive\" }", "\"run.day\" }"),
            "[E-REF-TYPE] season `harvest` `live: run.day`: `run.day` is a number but this \
             position expects a bool — compare it (for example `run.day > 0`)",
        ),
    ];
    for (tag, schema, want) in cases {
        let dir = project(&format!("season-once-{tag}"), &schema, &docs);
        let out = check_project(&dir);
        let t = text(&out);
        assert!(!out.status.success(), "{tag}: {t}");
        let errors: Vec<&str> = t.lines().filter(|l| l.contains(": error [")).collect();
        assert_eq!(errors.len(), 1, "{tag}: {t}");
        assert!(
            errors[0].contains("world.schema.yaml:")
                && errors[0].contains(want)
                && errors[0].ends_with("(imported by 3 documents)"),
            "{tag}: {t}"
        );
        // G-17: `lute play` and `lute test` fold it the same way — one
        // error at the schema line, then refuse (exit 1).
        write(&dir, "tests/a.test.yaml", "file: ../scenes/a.lute\n");
        let tested = Command::new(BIN)
            .args(["test", &dir.display().to_string(), "--project"])
            .arg(&dir)
            .output()
            .unwrap();
        for (what, out) in [
            ("play", play(&dir, "steps:\n  - occasion: talk\n")),
            ("test", tested),
        ] {
            let t = text(&out);
            assert_eq!(out.status.code(), Some(1), "{tag} {what}: {t}");
            let errors: Vec<&str> = t.lines().filter(|l| l.contains(": error [")).collect();
            assert_eq!(errors.len(), 1, "{tag} {what}: {t}");
            assert!(
                errors[0].contains("world.schema.yaml:")
                    && errors[0].contains(want)
                    && errors[0].ends_with("(imported by 3 documents)"),
                "{tag} {what}: {t}"
            );
            assert!(t.contains("refusing to"), "{tag} {what}: {t}");
        }
    }
}

/// `lute beats` shows each cadence — `spentBy: <cond>` in text and a
/// `spentBy` field in JSON, `once` `week` / `season:<name>` — and a
/// `spentBy` beat shadows nothing: it drops out once its condition holds.
#[test]
fn beats_lists_spent_by_week_and_season_cadences() {
    let schema =
        format!("{DAY_CLOCK}{WEEK_CLOCK}seasons:\n  harvest: {{ live: \"run.day >= 3\" }}\n");
    let dir = project(
        "beats",
        &schema,
        &[
            (
                "scenes/valves.lute",
                &scene("valves", "spentBy: \"run.solved\"\npriority: 3\n", "V."),
            ),
            (
                "scenes/reset.lute",
                &scene("reset", "once: week\npriority: 2\n", "R."),
            ),
            (
                "scenes/fair.lute",
                &scene("fair", "once: \"season:harvest\"\npriority: 1\n", "F."),
            ),
        ],
    );
    let beats = |json: bool| {
        let mut cmd = Command::new(BIN);
        cmd.arg("beats").arg(&dir);
        if json {
            cmd.arg("--json");
        }
        cmd.output().unwrap()
    };
    let t = text(&beats(false));
    assert!(t.contains("valves  scene  spentBy: run.solved  -"), "{t}");
    assert!(t.contains("reset   scene  week                 -"), "{t}");
    assert!(t.contains("fair    scene  season:harvest       -"), "{t}");
    assert!(!t.contains("shadowed"), "{t}");
    let out = beats(true);
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let rows = v["roots"][0]["ladders"][0]["beats"].as_array().unwrap();
    let row = |id: &str| rows.iter().find(|r| r["id"] == id).unwrap();
    assert_eq!(row("valves")["spentBy"], "run.solved", "{v}");
    assert_eq!(row("valves")["once"], "none", "{v}");
    assert_eq!(row("reset")["once"], "week", "{v}");
    assert_eq!(row("fair")["once"], "season:harvest", "{v}");
    assert!(rows.iter().all(|r| r.get("shadowedBy").is_none()), "{v}");
}

/// `lute test`'s `eligible:` is the session's rule, `spentBy` included: the
/// beat is eligible until its condition holds under the mocks, and a miss
/// names the holding `spentBy` as the false premise.
#[test]
fn test_eligible_honours_spent_by() {
    let schema = format!(
        "{DAY_CLOCK}entities:\n  puzzle: {{ members: [valves] }}\n\
         relations:\n  solved: {{ args: [puzzle] }}\n"
    );
    let dir = project(
        "spent-by-test",
        &schema,
        &[
            (
                "scenes/valves.lute",
                &scene("ward.valves", "spentBy: \"holds(solved(valves))\"\n", "The valves hiss."),
            ),
            (
                "tests/open.test.yaml",
                "file: ../scenes/valves.lute\nexpect:\n  eligible: true\n  \
                 transcriptContains: [\"The valves hiss.\"]\n",
            ),
            (
                "tests/solved.test.yaml",
                "file: ../scenes/valves.lute\nfacts: [solved(valves)]\nexpect:\n  eligible: false\n  \
                 transcriptLacks: [\"The valves hiss.\"]\n",
            ),
        ],
    );
    let out = check_project(&dir);
    assert!(out.status.success(), "{}", text(&out));
    let lute_test = |dir: &Path| {
        Command::new(BIN)
            .args(["test", "tests", "--project", "."])
            .current_dir(dir)
            .output()
            .unwrap()
    };
    let out = lute_test(&dir);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(0), "{t}");
    assert!(t.contains("2 passed, 0 failed"), "{t}");
    // Expecting it eligible once the fact holds is a miss naming `spentBy`.
    write(
        &dir,
        "tests/solved.test.yaml",
        "file: ../scenes/valves.lute\nfacts: [solved(valves)]\nexpect:\n  eligible: true\n",
    );
    let out = lute_test(&dir);
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains("its spentBy: `holds(solved(valves))` holds"),
        "{t}"
    );
}
