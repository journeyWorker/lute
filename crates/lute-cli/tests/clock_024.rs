//! dsl 0.24.0 §1 (the declared clock) and §2.1 (`by=` at every settle,
//! `until=` at the raise), through the built `lute` binary over a small
//! temp project whose schema declares the clock.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-clock-{tag}-{}-{n}", std::process::id()));
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

const CLOCK: &str =
    "clock:\n  day: run.day\n  slot: run.slot\n  slots: [morning, afternoon, night]\n  \
                     week: { length: 7, first: 0, labels: [Mon, Tue, Wed, Thu, Fri, Sat, Sun] }\n";

/// A project: `world.schema.yaml` with `state` and a clock, plus every
/// `(path, text)` document.
fn project(tag: &str, state_owner: &str, docs: &[(&str, &str)]) -> PathBuf {
    project_with(tag, state_owner, CLOCK, docs)
}

/// [`project`] with the schema's `clock:` text (`""` for none).
fn project_with(tag: &str, state_owner: &str, clock: &str, docs: &[(&str, &str)]) -> PathBuf {
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
            "state:\n  run.day: {{ type: number, default: 1{state_owner} }}\n  \
             run.slot: {{ type: {{ enum: [morning, afternoon, night] }}, default: morning{state_owner} }}\n\
             {clock}"
        ),
    );
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

const SCENE: &str =
    "---\nkind: scene\nid: hall.morning\nuses: ../world.schema.yaml\non: visit\nonce: day\n\
                     when: 'clock.index >= 0'\n---\n\n## Hall\n\n\
                     @narrator: Today is {{clock.weekdayLabel}}.\n";

#[test]
fn a_declared_clock_gives_readable_clock_paths_and_once_day() {
    let dir = project("ok", ", owner: engine", &[("scenes/hall.lute", SCENE)]);
    let out = check_project(&dir);
    assert!(out.status.success(), "{}", text(&out));
    // `once: day` spends the scene until the day changes (a second visit the
    // same day finds it spent); `clock.weekdayLabel` renders the live day.
    let out = play(
        &dir,
        "steps:\n  - occasion: visit\n  - occasion: visit\n  - engine: { state: { run.day: 2 } }\n  \
         - occasion: visit\n",
    );
    let t = text(&out);
    assert!(t.contains("Today is Mon."), "{t}");
    assert!(t.contains("once: day — already presented today"), "{t}");
    assert!(t.contains("Today is Tue."), "{t}");
    assert_eq!(t.matches("Today is").count(), 2, "{t}");
}

#[test]
fn clock_paths_must_be_engine_owned() {
    let dir = project("owner", "", &[("scenes/hall.lute", SCENE)]);
    let t = text(&check_project(&dir));
    assert!(t.contains("E-CLOCK-DECL"), "{t}");
    assert!(t.contains("must be declared `owner: engine`"), "{t}");
}

#[test]
fn once_day_needs_a_clock() {
    let dir = temp_dir("noclock");
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    );
    write(
        &dir,
        "scenes/hall.lute",
        "---\nkind: scene\nid: hall.morning\non: visit\nonce: day\n---\n\n## Hall\n\n@narrator: Hi.\n",
    );
    let t = text(&check_project(&dir));
    assert!(
        t.contains("E-BEAT-ATTR") && t.contains("declares no `clock:`"),
        "{t}"
    );
}

/// dsl 0.25.0 §9 (SU N8): a state entry swallowed by a malformed `clock:`
/// leaves the `<match>` over it undeclared — the import error is the report,
/// not a follow-on `E-NONEXHAUSTIVE` at the match; with the import intact the
/// undeclared subject is its own `E-UNDECLARED`, again without one.
#[test]
fn an_undeclared_match_subject_is_no_nonexhaustive_match() {
    let scene = "---\nkind: scene\nid: a\nuses: ../world.schema.yaml\n---\n\n## A\n\n\
                 <match on=\"run.route\">\n<when is=\"none\">\n@narrator: none\n</when>\n\
                 <when is=\"sol\">\n@narrator: sol\n</when>\n</match>\n";
    let swallowed =
        format!("{CLOCK}  run.route: {{ type: {{ enum: [none, sol] }}, default: none }}\n");
    let dir = project_with(
        "cascade",
        ", owner: engine",
        &swallowed,
        &[("scenes/a.lute", scene)],
    );
    let t = text(&check_project(&dir));
    assert!(
        t.contains("E-USES-PARSE") && t.contains("unknown field `run.route`"),
        "{t}"
    );
    assert!(!t.contains("E-NONEXHAUSTIVE"), "{t}");
    let dir = project_with(
        "undeclared",
        ", owner: engine",
        CLOCK,
        &[("scenes/a.lute", scene)],
    );
    let t = text(&check_project(&dir));
    assert!(t.contains("E-UNDECLARED") && t.contains("run.route"), "{t}");
    assert!(!t.contains("E-NONEXHAUSTIVE"), "{t}");
}

const QUEST_BY: &str = "---\nkind: quest\nuses: ../world.schema.yaml\n---\n\n\
                        <quest id=\"fest\" title=\"Festival\" start=\"true\">\n\
                        <objective id=\"go\" on=\"visit\" done=\"true\" by=\"run.day >= 3\"/>\n\
                        </quest>\n";

const QUEST_UNTIL: &str = "---\nkind: quest\nuses: ../world.schema.yaml\n---\n\n\
                           <quest id=\"fest\" title=\"Festival\" start=\"true\">\n\
                           <objective id=\"go\" on=\"visit\" done=\"run.day >= 9\" until=\"run.day >= 3\"/>\n\
                           </quest>\n";

/// 0.23.1 judged an `on=` objective's `by` only when its occasion was raised,
/// so a player who never went there escaped the deadline. 0.24.0 §2.1: a
/// deadline is a moment — the settle after the clock passes it fails it.
#[test]
fn by_fails_an_on_objective_without_its_occasion() {
    let dir = project("by", ", owner: engine", &[("quests/fest.lute", QUEST_BY)]);
    let out = play(
        &dir,
        "steps:\n  - engine: { state: { run.day: 3 } }\nexpect:\n  quests: { fest: failed }\n",
    );
    assert!(out.status.success(), "{}", text(&out));
}

/// `until=` keeps the 0.23.1 place-bound rule: judged only at the raise.
#[test]
fn until_is_judged_only_at_the_raise() {
    let dir = project(
        "until",
        ", owner: engine",
        &[("quests/fest.lute", QUEST_UNTIL)],
    );
    let out = play(
        &dir,
        "steps:\n  - engine: { state: { run.day: 3 } }\n    expect: { quests: { fest: active } }\n  \
         - occasion: visit\nexpect:\n  quests: { fest: failed }\n",
    );
    assert!(out.status.success(), "{}", text(&out));
    assert!(
        text(&out).contains("  fest.go failed (until)\n  quest fest -> failed (until)\n"),
        "{}",
        text(&out)
    );
    // Without `on=` there is no place to judge it.
    let bad = QUEST_UNTIL.replace(" on=\"visit\"", "");
    let dir = project("until-on", ", owner: engine", &[("quests/fest.lute", &bad)]);
    let t = text(&check_project(&dir));
    assert!(t.contains("`until` requires `on`"), "{t}");
}

const RAISING_CLOCK: &str = "clock:\n  day: run.day\n  slot: run.slot\n  slots: [morning, afternoon, night]\n  \
                             raise: slotStart\n  \
                             week: { length: 7, first: 0, labels: [Mon, Tue, Wed, Thu, Fri, Sat, Sun] }\n";

const SLOT_SCENE: &str =
    "---\nkind: scene\nid: day.slot\nuses: ../world.schema.yaml\non: slotStart\nonce: slot\n\
                          when: 'run.slot != \"night\"'\n---\n\n## Slot\n\n\
                          @narrator: It is {{run.slot}} on {{clock.weekdayLabel}}.\n";

fn clock_play(tag: &str, clock: &str, script: &str) -> Output {
    let dir = project_with(
        tag,
        ", owner: engine",
        clock,
        &[
            ("scenes/slot.lute", SLOT_SCENE),
            ("quests/fest.lute", QUEST_BY),
        ],
    );
    play(&dir, script)
}

/// dsl 0.24.0 §1: `advance:` writes the clock's paths (wrapping slots into
/// the next day), settles the quests — a `by` the new time passes fails
/// there, before anything is presented — then raises the clock's `raise`.
#[test]
fn advance_moves_the_clock_settles_the_quests_then_raises_its_occasion() {
    let out = clock_play(
        "advance",
        RAISING_CLOCK,
        "steps:\n  \
         - advance: slot\n    \
           expect: { presented: [day.slot], state: { run.day: 1, run.slot: afternoon, clock.index: 1 } }\n  \
         - advance: 2\n    \
           expect: { state: { run.day: 2, run.slot: morning, clock.index: 3 }, quests: { fest: active } }\n  \
         - advance: slot\n  \
         - advance: slot\n    \
           expect: { presented: [], winner: none }\n  \
         - advance: day\n    \
           expect: { presented: [day.slot], state: { run.day: 3, run.slot: morning }, quests: { fest: failed } }\n",
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(
        t.contains("── step 1 · advance slot: day 1 (Mon) morning → day 1 (Mon) afternoon"),
        "{t}"
    );
    assert!(t.contains("  set run.slot = \"afternoon\""), "{t}");
    assert!(t.contains("── step 1 · slotStart"), "{t}");
    assert!(t.contains("It is afternoon on Mon."), "{t}");
    assert!(
        t.contains("── step 2 · advance 2: day 1 (Mon) afternoon → day 2 (Tue) morning"),
        "{t}"
    );
    assert!(
        t.contains("── step 5 · advance day: day 2 (Tue) night → day 3 (Wed) morning"),
        "{t}"
    );
    // The deadline fails at the settle, before the raised occasion presents.
    let failed = t
        .find("fest.go failed (by)")
        .unwrap_or_else(|| panic!("{t}"));
    let wed = t
        .find("It is morning on Wed.")
        .unwrap_or_else(|| panic!("{t}"));
    assert!(failed < wed, "{t}");
}

#[test]
fn advance_needs_a_clock_and_moves_only_forward() {
    // A clock without `raise:` moves and settles, and presents nothing — so
    // a selection expectation or a `pick` on it is refused.
    let out = clock_play(
        "advance-noraise",
        CLOCK,
        "steps:\n  - advance: day\n    expect: { state: { run.day: 2, run.slot: morning } }\n",
    );
    assert!(out.status.success(), "{}", text(&out));
    let out = clock_play(
        "advance-noraise-expect",
        CLOCK,
        "steps:\n  - advance: slot\n    expect: { presented: [day.slot] }\n",
    );
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(
        text(&out).contains("the clock declares no `raise:` slot occasion"),
        "{}",
        text(&out)
    );
    let dir = project_with(
        "advance-noclock",
        ", owner: engine",
        "",
        &[("quests/fest.lute", QUEST_BY)],
    );
    let out = play(&dir, "steps:\n  - advance: slot\n");
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(
        text(&out).contains("no schema of this project declares a `clock:`"),
        "{}",
        text(&out)
    );
    let out = clock_play("advance-zero", RAISING_CLOCK, "steps:\n  - advance: 0\n");
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(
        text(&out).contains("a slot count is a whole number ≥ 1"),
        "{}",
        text(&out)
    );
}

/// Ember N16 / R3: an `advance:` step carries the `engine:` writes of the
/// same moment — applied where the clock arrives, one settle for both — but
/// not a write to the clock's own paths.
#[test]
fn an_advance_step_carries_engine_writes() {
    let leg = format!("  run.leg: {{ type: number, default: 1, owner: engine }}\n{RAISING_CLOCK}");
    let out = clock_play(
        "advance-engine",
        &leg,
        "steps:\n  - advance: day\n    engine: { state: { run.leg: 3 } }\n    \
         expect: { presented: [day.slot], state: { run.leg: 3, run.day: 2, run.slot: morning } }\n",
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    let leg = t.find("  set run.leg = 3").unwrap_or_else(|| panic!("{t}"));
    let day = t.find("  set run.day = 2").unwrap_or_else(|| panic!("{t}"));
    assert!(
        day < leg,
        "the engine writes apply where the clock arrives: {t}"
    );
    let out = clock_play(
        "advance-engine-clock",
        RAISING_CLOCK,
        "steps:\n  - advance: slot\n    engine: { state: { run.day: 4 } }\n",
    );
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(
        text(&out).contains("`engine:` writes `run.day`, which the `advance:` beside it moves"),
        "{}",
        text(&out)
    );
}

const MAP_CLOCK: &str = "  run.leg: { type: number, default: 1, owner: engine }\n\
                         clock:\n  day: run.day\n  slot: run.slot\n  slots: [morning, afternoon, night]\n  \
                         week: { length: 7, first: 0, labels: [Mon, Tue, Wed, Thu, Fri, Sat, Sun] }\n  \
                         raise: { slot: slotStart, dayEnd: dayEnd }\n";

const CLOSE_SCENE: &str =
    "---\nkind: scene\nid: day.close\nuses: ../world.schema.yaml\non: dayEnd\nonce: false\n---\n\n\
                           ## Close\n\n@narrator: Leg {{run.leg}} ends.\n";

fn map_play(tag: &str, script: &str) -> Output {
    let dir = project_with(
        tag,
        ", owner: engine",
        MAP_CLOCK,
        &[
            ("scenes/slot.lute", SLOT_SCENE),
            ("scenes/close.lute", CLOSE_SCENE),
        ],
    );
    play(&dir, script)
}

/// Ember R3: the `engine:` writes of an `advance:` land where the clock
/// arrives — after the `dayEnd` it raises on the way, which still reads the
/// day it closes. Summer R2 / lighthouse N15: the step's `presented` spans
/// every raise in it; `winner` stays the slot raise's.
#[test]
fn an_advance_judges_every_raise_and_writes_on_arrival() {
    let out = map_play(
        "map-advance",
        "steps:\n  - advance: day\n    engine: { state: { run.leg: 3 } }\n    \
         expect: { presented: [day.close, day.slot], winner: day.slot, state: { run.leg: 3 } }\n",
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    let closed = t.find("Leg 1 ends.").unwrap_or_else(|| panic!("{t}"));
    let leg = t.find("  set run.leg = 3").unwrap_or_else(|| panic!("{t}"));
    assert!(closed < leg, "dayEnd reads the day it closes: {t}");
    let out = map_play(
        "map-advance-miss",
        "steps:\n  - advance: day\n    expect: { presented: [day.slot] }\n",
    );
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(
        text(&out).contains("expected [day.slot], actual [day.close, day.slot]"),
        "{}",
        text(&out)
    );
}

/// Summer R1: raising the clock's `dayEnd` by hand as well closes the day
/// twice — the step says so.
#[test]
fn a_manual_raise_of_the_clocks_day_end_is_noted() {
    let out = map_play(
        "map-manual",
        "steps:\n  - occasion: dayEnd\n  - advance: day\nexpect: { transcriptContains: [\"Leg 1 ends.\"] }\n",
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert_eq!(t.matches("Leg 1 ends.").count(), 2, "{t}");
    let (manual, _) = t.split_once("── step 2").unwrap_or_else(|| panic!("{t}"));
    assert!(
        manual.contains(
            "note: `dayEnd` is the clock's `raise: { dayEnd: dayEnd }` — an `advance:` raises it"
        ),
        "{t}"
    );
    assert_eq!(t.matches("note: `dayEnd`").count(), 1, "{t}");
}

/// dsl 0.24.0 §1: an `engine:` step moving `clock.index` backward is a
/// usage error; forward is legal, and a `newRun` starts the clock over.
#[test]
fn an_engine_step_moving_the_clock_backward_is_a_usage_error() {
    let out = clock_play(
        "backward",
        CLOCK,
        "state: { run.day: 2, run.slot: afternoon }\nsteps:\n  - engine: { state: { run.slot: night } }\n  \
         - newRun: true\n    expect: { state: { run.day: 1, clock.index: 0 } }\n  \
         - engine: { state: { run.day: 3 } }\n  - engine: { state: { run.slot: afternoon, run.day: 2 } }\n",
    );
    let t = text(&out);
    assert_eq!(out.status.code(), Some(2), "{t}");
    assert!(
        t.contains("step 4: `engine:` moves the clock backward, from day 3 (Wed) morning to day 2 (Tue) afternoon (clock.index 6 → 4)"),
        "{t}"
    );
}

/// dsl 0.24.0 §1: `- include: <file>` splices that file's steps in place,
/// resolved against the including file; steps are numbered after the splice.
#[test]
fn include_splices_steps_and_refuses_a_cycle() {
    let dir = project_with(
        "include",
        ", owner: engine",
        RAISING_CLOCK,
        &[
            ("scenes/slot.lute", SLOT_SCENE),
            ("quests/fest.lute", QUEST_BY),
        ],
    );
    write(
        &dir,
        "parts/morning.yaml",
        "- advance: slot\n- include: evening.yaml\n",
    );
    write(
        &dir,
        "parts/evening.yaml",
        "steps:\n  - advance: slot\n    label: evening\n",
    );
    let out = play(
        &dir,
        "steps:\n  - include: parts/morning.yaml\n  - advance: slot\n    \
         expect: { state: { run.day: 2, run.slot: morning } }\n",
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("── step 2 (evening) · advance slot"), "{t}");
    assert!(
        t.contains("── step 3 · advance slot: day 1 (Mon) night → day 2 (Tue) morning"),
        "{t}"
    );

    write(
        &dir,
        "parts/loop.yaml",
        "- advance: slot\n- include: ../parts/loop.yaml\n",
    );
    let out = play(&dir, "steps:\n  - include: parts/loop.yaml\n");
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(
        text(&out).contains("`include: ../parts/loop.yaml` is a cycle"),
        "{}",
        text(&out)
    );
    let out = play(&dir, "steps:\n  - include: parts/missing.yaml\n");
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(
        text(&out).contains("cannot read `include: parts/missing.yaml`"),
        "{}",
        text(&out)
    );
}

/// dsl 0.24.0 §1: `--axis clock=d1..d2` is day × slot in clock order; bare
/// `clock` is one week.
#[test]
fn calendar_clock_axis_expands_day_by_slot_in_order() {
    let dir = project_with(
        "calendar",
        ", owner: engine",
        RAISING_CLOCK,
        &[("scenes/slot.lute", SLOT_SCENE)],
    );
    let calendar = |args: &[&str]| {
        Command::new(BIN)
            .arg("calendar")
            .arg(&dir)
            .args(args)
            .output()
            .unwrap()
    };
    let out = calendar(&["--axis", "clock=1..2", "--occasion", "slotStart", "--csv"]);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    let order = [
        "1 Mon morning",
        "1 Mon afternoon",
        "1 Mon night",
        "2 Tue morning",
        "2 Tue afternoon",
        "2 Tue night",
    ];
    let mut last = 0;
    for cell in order {
        let at = t[last..]
            .find(cell)
            .unwrap_or_else(|| panic!("{cell} after byte {last}: {t}"))
            + last;
        last = at + cell.len();
    }
    // The night cells present nothing; the others the slot scene.
    assert_eq!(
        t.lines().filter(|l| l.contains("day.slot")).count(),
        4,
        "{t}"
    );

    let out = calendar(&["--axis", "clock", "--occasion", "slotStart", "--csv"]);
    assert!(text(&out).contains("7 Sun night"), "{}", text(&out));
    let out = calendar(&["--axis", "clock", "--axis", "run.day=1..2"]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(text(&out).contains("both set the clock"), "{}", text(&out));
}

/// Round-3 SU N1: `clock.weekday` is the whole numbers `0..length-1` and
/// `clock.weekdayLabel` the enum of the week's labels, so a weekday
/// `<match>` is exhaustive without `<otherwise>` and a bad arm is an error.
#[test]
fn weekday_matches_are_exhaustive_and_checked() {
    let scene = |body: &str| {
        format!(
            "---\nkind: scene\nid: hall.morning\nuses: ../world.schema.yaml\non: visit\n---\n\n\
             ## Hall\n\n{body}"
        )
    };
    let clean = scene(
        "<match on=\"clock.weekday\">\n<when is=\"0\">\n@narrator: Monday.\n</when>\n\
         <when is=\"1..5\">\n@narrator: Midweek.\n</when>\n<when is=\"6\">\n@narrator: Sunday.\n</when>\n</match>\n\
         <match on=\"clock.weekdayLabel\">\n<when is=\"Sat|Sun\">\n@narrator: Weekend.\n</when>\n\
         <when is=\"Mon|Tue|Wed|Thu|Fri\">\n@narrator: Workday.\n</when>\n</match>\n",
    );
    let dir = project(
        "weekday-ok",
        ", owner: engine",
        &[("scenes/hall.lute", &clean)],
    );
    let out = check_project(&dir);
    assert!(out.status.success(), "{}", text(&out));

    let bad = scene(
        "<match on=\"clock.weekday\">\n<when is=\"0..5\">\n@narrator: a.\n</when>\n\
         <when is=\"7\">\n@narrator: b.\n</when>\n</match>\n\
         <match on=\"clock.weekdayLabel\">\n<when is=\"Sundy\">\n@narrator: c.\n</when>\n\
         <otherwise>\n@narrator: d.\n</otherwise>\n</match>\n\
         @narrator{when=\"clock.weekdayLabel == 'Sundy'\"}: e.\n",
    );
    let dir = project(
        "weekday-bad",
        ", owner: engine",
        &[("scenes/hall.lute", &bad)],
    );
    let t = text(&check_project(&dir));
    assert!(
        t.contains("E-NONEXHAUSTIVE") && t.contains("`6` is not covered"),
        "{t}"
    );
    assert!(
        t.contains("`7` matches none of the subject's values, the whole numbers 0..6"),
        "{t}"
    );
    assert!(
        t.contains("`Sundy` is not a member of the subject's domain [Mon, Tue"),
        "{t}"
    );
    assert!(
        t.contains("E-ARM-DEAD"),
        "the `== 'Sundy'` guard can never hold: {t}"
    );
}

/// Round-3 SU N7: `raise: { slot, dayStart, dayEnd }` — every midnight an
/// advance crosses raises `dayEnd` at the day's last slot (the day not yet
/// advanced; `advance: <n>` walks there, never skipping it) and `dayStart`
/// at the next day's first slot, then `slot` where the clock stops.
#[test]
fn advance_raises_day_end_and_day_start_at_every_midnight_it_crosses() {
    let scene = |id: &str, on: &str, say: &str| {
        format!(
            "---\nkind: scene\nid: {id}\nuses: ../world.schema.yaml\non: {on}\nonce: false\n---\n\n## S\n\n\
             @narrator: {say} day {{{{run.day}}}} {{{{run.slot}}}}.\n"
        )
    };
    let (end, start, slot) = (
        scene("c.end", "dayEnd", "End"),
        scene("c.start", "dayStart", "Start"),
        scene("c.slot", "slotStart", "Slot"),
    );
    let dir = project_with(
        "day-raises",
        ", owner: engine",
        "clock: { day: run.day, slot: run.slot, slots: [morning, afternoon, night], \
         raise: { slot: slotStart, dayStart: dayStart, dayEnd: dayEnd } }\n",
        &[
            ("scenes/end.lute", &end),
            ("scenes/start.lute", &start),
            ("scenes/slot.lute", &slot),
        ],
    );
    let out = play(
        &dir,
        "steps:\n  - advance: slot\n  - advance: 3\n    expect: { presented: [c.end, c.start, c.slot], state: { run.day: 2, run.slot: afternoon } }\n  \
         - advance: day\n",
    );
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    let at = |s: &str| t.find(s).unwrap_or_else(|| panic!("no `{s}`:\n{t}"));
    // Step 1 crosses no midnight.
    assert!(at("Slot day 1 afternoon.") < at("── step 2"), "{t}");
    assert!(!t[..at("── step 2")].contains("End day"), "{t}");
    // Step 2: walks to Monday night, closes it, opens Tuesday, stops at afternoon.
    let (e, s, sl) = (
        at("End day 1 night."),
        at("Start day 2 morning."),
        at("Slot day 2 afternoon."),
    );
    assert!(at("── step 2") < e && e < s && s < sl, "{t}");
    assert!(t.contains("── step 2 · day 1 night · dayEnd"), "{t}");
    // Step 3: `advance: day` closes the day where the clock stands.
    assert!(t.contains("End day 2 afternoon."), "{t}");
    assert!(
        at("Start day 3 morning.") < at("Slot day 3 morning."),
        "{t}"
    );
}

/// Round-3 LH N7: a clock without `slot:` counts whole days; a scalar
/// `raise:` fires once per advance, `advance: 2` moves two days.
#[test]
fn a_day_granular_clock_needs_no_slot() {
    let scene = "---\nkind: scene\nid: d.arrive\nuses: ../world.schema.yaml\non: arrive\nonce: day\n---\n\n## S\n\n\
                 @narrator: Day {{run.day}}, index {{clock.index}}.\n";
    let dir = project_with(
        "day-granular",
        ", owner: engine",
        "clock: { day: run.day, raise: arrive }\n",
        &[("scenes/arrive.lute", scene)],
    );
    let check = check_project(&dir);
    assert!(check.status.success(), "{}", text(&check));
    let out = play(&dir, "steps:\n  - advance: day\n  - advance: 2\n    expect: { state: { run.day: 4, clock.index: 3 } }\n");
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("── step 1 · advance day: day 1 → day 2"), "{t}");
    assert!(
        t.contains("Day 2, index 1.") && t.contains("Day 4, index 3."),
        "{t}"
    );
    let out = Command::new(BIN)
        .arg("calendar")
        .arg(&dir)
        .args(["--axis", "clock=1..2"])
        .output()
        .unwrap();
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(
        t.contains("\n1  ") && t.contains("\n2  "),
        "a position is its day alone: {t}"
    );
}

/// Round-3 SU N3: with `--axis clock`, `--occasion O@clock.day` (or the
/// clock's own day path) evaluates `O` once per day, at a held slot.
#[test]
fn calendar_occasion_varies_over_one_part_of_the_clock_axis() {
    let dir = project_with(
        "calendar-part",
        ", owner: engine",
        RAISING_CLOCK,
        &[("scenes/slot.lute", SLOT_SCENE)],
    );
    let run = |occasion: &str| {
        let out = Command::new(BIN)
            .arg("calendar")
            .arg(&dir)
            .args(["--axis", "clock=1..2", "--occasion", occasion, "--json"])
            .output()
            .unwrap();
        assert!(out.status.success(), "{}", text(&out));
        let j: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        j["columns"][0].clone()
    };
    let col = run("slotStart@run.day,run.slot=night");
    assert_eq!(col["varies"], serde_json::json!(["clock.day"]), "{col}");
    assert_eq!(
        col["heldAt"],
        serde_json::json!({ "clock.slot": "night" }),
        "{col}"
    );
    let col = run("slotStart@clock.day");
    assert_eq!(
        col["heldAt"],
        serde_json::json!({ "clock.slot": "morning" }),
        "{col}"
    );
}

/// Round-3 SU N2: `lute beats` lists a bundle beat with `once="day"`.
#[test]
fn beats_lists_a_once_day_bundle_beat() {
    let lore = "---\nkind: lore\nid: b\nuses: ../world.schema.yaml\n---\n\n\
                <beat id=\"daily\" on=\"slotStart\" once=\"day\">\n  @narrator: Once a day.\n</beat>\n";
    let dir = project_with(
        "beats-once-day",
        ", owner: engine",
        RAISING_CLOCK,
        &[("lore/b.lute", lore)],
    );
    let out = Command::new(BIN).arg("beats").arg(&dir).output().unwrap();
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    let row = t
        .lines()
        .find(|l| l.contains("b.daily"))
        .unwrap_or_else(|| panic!("no b.daily row:\n{t}"));
    assert!(row.contains(" day "), "{row}");
}

/// Round-3 (cheatsheet p2): a clock over a path that is not `owner: engine`,
/// or naming an undeclared `raise` occasion, is reported by `lute check` on
/// the schema itself (it said `ok`), and by `check-project` once — folded
/// across importers — attributed to the schema's `clock:` line.
#[test]
fn a_bad_clock_is_reported_on_the_schema_and_once_per_project() {
    let scene = |id: &str| {
        format!("---\nkind: scene\nid: {id}\nuses: ../world.schema.yaml\non: slotStart\n---\n\n## S\n\n@narrator: Hi.\n")
    };
    let (a, b) = (scene("a"), scene("b"));
    let dir = project_with(
        "bad-clock",
        "",
        "clock: { day: run.day, slot: run.slot, slots: [morning, afternoon, night] }\n",
        &[("scenes/a.lute", &a), ("scenes/b.lute", &b)],
    );
    let schema = dir.join("world.schema.yaml");
    let out = Command::new(BIN)
        .arg("check")
        .arg(&schema)
        .output()
        .unwrap();
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(t.contains("world.schema.yaml:4:1: error [E-CLOCK-DECL] `clock:` `day: run.day` must be declared `owner: engine`"), "{t}");

    let t = text(&check_project(&dir));
    assert_eq!(
        t.matches("must be declared `owner: engine`").count(),
        4,
        "day and slot, each once plus its schema line: {t}"
    );
    assert!(t.contains("(+1 more caller)"), "{t}");
    assert!(
        t.contains("world.schema.yaml:4:1: error [E-CLOCK-DECL] `clock:` `slot: run.slot`"),
        "{t}"
    );

    // An undeclared `raise` occasion, with the project's occasions known.
    write(&dir, "world.schema.yaml", &std::fs::read_to_string(&schema).unwrap().replace(
        "run.day: { type: number, default: 1 }",
        "run.day: { type: number, default: 1, owner: engine }",
    ).replace("run.slot: { type: { enum: [morning, afternoon, night] }, default: morning }",
        "run.slot: { type: { enum: [morning, afternoon, night] }, default: morning, owner: engine }")
     .replace("slots: [morning, afternoon, night] }", "slots: [morning, afternoon, night], raise: nope }"));
    write(
        &dir,
        "lute.project.yaml",
        "pluginsDir: plugins/\ndefaultProfile: m\nprofiles:\n  m:\n    plugins: { m.occ: true }\n",
    );
    write(&dir, "plugins/m.occ/plugin.yaml", "id: m.occ\nversion: 0.1.0\nkind: capability\ndepends: [ { id: lute.core, range: \"^0.0.1\" } ]\nexports:\n  occasions: occasions/\n");
    write(
        &dir,
        "plugins/m.occ/occasions/occ.yaml",
        "occasions:\n  slotStart: {}\n",
    );
    let out = Command::new(BIN)
        .arg("check")
        .arg(&schema)
        .output()
        .unwrap();
    let t = text(&out);
    assert!(
        t.contains("[E-CLOCK-DECL] `clock:` `raise: nope` is not a declared occasion"),
        "{t}"
    );
    let t = text(&check_project(&dir));
    assert_eq!(
        t.matches("`raise: nope` is not a declared occasion")
            .count(),
        2,
        "{t}"
    );
}

/// Round-3 docs pass (a): `lute check` on a schema alone runs what an
/// importer's check would report about it — enum labels, entity kinds,
/// seed facts — at the schema's own lines; `check-project` folds the
/// importers' copies into one attributed to the schema.
#[test]
fn a_schema_checked_alone_reports_what_its_importers_would() {
    let dir = temp_dir("schema-alone");
    write(
        &dir,
        "lute.project.yaml",
        "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n",
    );
    write(
        &dir,
        "w.schema.yaml",
        "enums:\n  weekday:\n    members: [mon, tue]\n    labels: { mon: Monday, sunday: Sunday }\n\
         state:\n  run.weekday: { type: { domain: weekday }, default: mon }\n\
         entities:\n  person: { members: [ada, bo] }\n  pet: { members: [bo] }\n\
         relations:\n  likes: { args: [person] }\nfacts:\n  - \"likes(zed)\"\n",
    );
    let scene = |id: &str| {
        format!("---\nkind: scene\nid: {id}\nuses: ../w.schema.yaml\n---\n\n## S\n\n@narrator: {{{{run.weekday}}}}.\n")
    };
    write(&dir, "scenes/a.lute", &scene("a"));
    write(&dir, "scenes/b.lute", &scene("b"));
    let out = Command::new(BIN)
        .arg("check")
        .arg(dir.join("w.schema.yaml"))
        .output()
        .unwrap();
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    for want in [
        "w.schema.yaml:2:3: error [E-ENUM-LABEL-NOT-MEMBER]",
        "w.schema.yaml:9:3: error [E-ENTITY-KIND-CLASH]",
        "w.schema.yaml:13:6: error [E-FACT-DOMAIN]",
    ] {
        assert!(t.contains(want), "missing `{want}`:\n{t}");
    }
    let t = text(&check_project(&dir));
    assert_eq!(
        t.matches("[E-ENUM-LABEL-NOT-MEMBER]").count(),
        2,
        "once, plus its schema line: {t}"
    );
    assert!(
        t.contains("w.schema.yaml:2:3: error [E-ENUM-LABEL-NOT-MEMBER]"),
        "{t}"
    );
    assert!(t.contains("which is not one of its members (dsl 0.24.0 §1) (declared in schema import `w.schema.yaml`) (+1 more caller)"), "{t}");
}

/// Round-3 docs pass (b): a frontmatter rule whose `cel("…")` guard names
/// no def is reported at the rule's line, not at 1:1.
#[test]
fn a_rule_guard_def_error_lands_on_the_rule() {
    let dir = temp_dir("rule-guard-def");
    let f = dir.join("s.lute");
    write(
        &dir,
        "s.lute",
        "---\nkind: scene\nid: s\nentities:\n  item: { members: [lamp] }\n\
         relations:\n  lit: { args: [item], derive: true }\nrules:\n  - \"lit(lamp) :- cel(\\\"@firstDy\\\")\"\n---\n\n\
         ## S\n\n@narrator{when=\"holds(lit(lamp))\"}: Hi.\n",
    );
    let t = text(&Command::new(BIN).arg("check").arg(&f).output().unwrap());
    assert!(t.contains("s.lute:9:6: error [E-RULE-GUARD-DEF]"), "{t}");
}

/// Round-3 docs pass (d): an entry's `once="slot"` without a clock is not
/// told to use `false`, which an entry refuses.
#[test]
fn once_slot_on_an_entry_without_a_clock_suggests_what_an_entry_accepts() {
    let lore = "---\nkind: lore\nid: l\nuses: ../world.schema.yaml\n---\n\n\
                <entry id=\"e\" on=\"visit\" once=\"slot\">\n  @narrator: Hi.\n</entry>\n";
    let dir = project_with(
        "entry-once-slot",
        ", owner: engine",
        "",
        &[("lore/l.lute", lore)],
    );
    let t = text(&check_project(&dir));
    assert!(
        t.contains("`once=\"slot\"` spends a beat once per clock slot"),
        "{t}"
    );
    assert!(
        t.contains("or omit `once`") && !t.contains("`user` / `false`"),
        "{t}"
    );
}

/// Round-3 docs pass (e): a mock may not seed a derived `clock.*` path —
/// it names the day / slot paths to seed instead.
#[test]
fn a_mock_seeding_a_clock_path_is_refused() {
    let dir = project_with(
        "mock-clock",
        ", owner: engine",
        CLOCK,
        &[("scenes/hall.lute", SCENE)],
    );
    write(
        &dir,
        "m.yaml",
        "file: scenes/hall.lute\nstate:\n  clock.index: 5\n",
    );
    let out = Command::new(BIN)
        .arg("trace")
        .arg(dir.join("scenes/hall.lute"))
        .arg("--mock")
        .arg(dir.join("m.yaml"))
        .output()
        .unwrap();
    let t = text(&out);
    assert_eq!(out.status.code(), Some(1), "{t}");
    assert!(
        t.contains(
            "[E-TRACE-MOCK-UNDECLARED] `--state clock.index=…` seeds a path the clock derives"
        ),
        "{t}"
    );
    assert!(t.contains("seed `run.day` / `run.slot` instead"), "{t}");
}

/// Round-3 docs pass (f): a cell an unknown `when` leaves undecided reads
/// as such — not `? over quiet` / `lost to ?`.
#[test]
fn calendar_names_the_unknown_when_of_an_undecided_cell() {
    let odd = "---\nkind: scene\nid: odd\nuses: ../world.schema.yaml\non: visit\npriority: 5\n\
               when: \"validAt(met(ada), quest.f.activatedAt)\"\n---\n\n## S\n\n@narrator: Odd.\n";
    let quiet = "---\nkind: scene\nid: quiet\nuses: ../world.schema.yaml\non: visit\n---\n\n## S\n\n@narrator: Quiet.\n";
    let quest = "---\nkind: quest\nid: q\nuses: ../world.schema.yaml\n---\n\n\
                 <quest id=\"f\" start=\"true\">\n  <objective id=\"o\" done=\"run.day > 2\"/>\n</quest>\n";
    let dir = project_with(
        "calendar-undecided",
        "",
        "entities:\n  person: { members: [ada] }\nrelations:\n  met: { args: [person] }\n",
        &[
            ("scenes/odd.lute", odd),
            ("scenes/quiet.lute", quiet),
            ("quests/f.lute", quest),
        ],
    );
    let out = Command::new(BIN)
        .arg("calendar")
        .arg(&dir)
        .output()
        .unwrap();
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(
        t.contains("visit: undecided (odd's `when` is unknown) over quiet"),
        "{t}"
    );
    assert!(t.contains("quiet [scene, scenes/quiet.lute] visit — lost to an undecided cell (odd's `when` is unknown)"), "{t}");
    assert!(!t.contains("lost to ?") && !t.contains(": ? over"), "{t}");
}
