//! `lute play` acceptance (design spec
//! `docs/superpowers/specs/2026-08-14-lute-schedule-and-play-design.md` v2
//! §4): a mini fixture project (a cold-open/rewind placement, a two-variant
//! branching event, a causally-gated follow-up event, and one world-lane
//! event, `schedule.yaml`, and two route scripts) exercised end-to-end
//! through the built `lute` binary — route A/B transcript divergence, world
//! interleaving, rewind, `--steps`, the exit-code contract (0/1/3), and the
//! hard failure when a project has no `schedule.yaml` at all (design v2 §2:
//! there is no `after:`-graph fallback).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-play-{tag}-{}-{n}", std::process::id()));
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

fn run(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

const PROJECT_YAML: &str = "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\n";

const SCHEDULE_YAML: &str = "\
clock:
  buckets: [morning, afternoon, evening]
  ticksPerBucket: 10
  days: 2

lanes:
  user: { exclusive: true }
  world: { exclusive: false }

placements:
  - event: confinement
    lane: user
    at: d2.evening+0
    size: 5
    presentation: 0
    doc: scenes/confinement/main.lute
  - event: meet
    lane: user
    at: morning+0
    size: 5
    presentation: 100
    variants:
      - when: \"run.route == 'a'\"
        doc: scenes/meet/routeA.lute
      - when: \"run.route == 'b'\"
        doc: scenes/meet/routeB.lute
  - event: errand
    lane: user
    size: 5
    doc: scenes/errand/main.lute
  - event: nera
    lane: world
    at: afternoon+0
    size: 5
    doc: scenes/nera/main.lute
";

const CONFINEMENT: &str = "\
---
kind: scene
character: confinement
season: 1
episode: 1
---

## Shot 1.

::bg{location=\"cell\" time=\"night\"}
@hero: Cold open flashback.
";

const ROUTE_A: &str = "\
---
kind: scene
character: meet-a
season: 1
episode: 1
state:
  run.route: { type: { enum: [a, b] }, default: a }
---

## Shot 1.

::bg{location=\"cafe\" time=\"morning\"}
@hero: Route A meeting.

<branch id=\"pick\">
  <choice id=\"left\" label=\"Left\">
    @hero: Went left.
  </choice>
  <choice id=\"right\" label=\"Right\">
    @hero: Went right.
  </choice>
</branch>
";

const ROUTE_B: &str = "\
---
kind: scene
character: meet-b
season: 1
episode: 1
state:
  run.route: { type: { enum: [a, b] }, default: a }
---

## Shot 1.

::bg{location=\"cafe\" time=\"afternoon\"}
@hero: Route B meeting.
";
const ERRAND: &str = "\
---
kind: scene
character: errand
season: 1
episode: 1
after: 'visited(\"meet-a.s01ep01\") || visited(\"meet-b.s01ep01\")'
---

## Shot 1.

::bg{location=\"street\" time=\"morning\"}
@hero: Running an errand.
";

const NERA: &str = "\
---
kind: scene
character: nera
season: 1
episode: 1
---

## Shot 1.

::bg{location=\"woods\" time=\"afternoon\"}
@nera: World event fires.
";

/// The mini fixture project: a cold-open/rewind placement (`confinement`,
/// `presentation: 0`, story tick on day 2 but presented FIRST), a
/// two-variant branching event (`meet`, routes `a`/`b`), a follow-up event
/// causally gated on either branch having played (`errand`), and one
/// world-lane event (`nera`) interleaved between `errand` and the eventual
/// rewind back to day 1.
fn fixture(tag: &str) -> PathBuf {
    let dir = temp_dir(tag);
    write(&dir, "lute.project.yaml", PROJECT_YAML);
    write(&dir, "schedule.yaml", SCHEDULE_YAML);
    write(&dir, "scenes/confinement/main.lute", CONFINEMENT);
    write(&dir, "scenes/meet/routeA.lute", ROUTE_A);
    write(&dir, "scenes/meet/routeB.lute", ROUTE_B);
    write(&dir, "scenes/errand/main.lute", ERRAND);
    write(&dir, "scenes/nera/main.lute", NERA);
    write(
        &dir,
        "routes/a.play.yaml",
        "state:\n  run.route: a\nchoose:\n  meet/pick: left\n",
    );
    write(&dir, "routes/b.play.yaml", "state:\n  run.route: b\n");
    dir
}

#[test]
fn route_a_and_route_b_transcripts_diverge_on_the_branch_and_dialogue() {
    let dir = fixture("routes");
    let script_a = dir.join("routes/a.play.yaml");
    let script_b = dir.join("routes/b.play.yaml");

    let out_a = run(&[
        "play",
        dir.to_str().unwrap(),
        "--script",
        script_a.to_str().unwrap(),
    ]);
    assert!(out_a.status.success(), "route A: {}", stderr(&out_a));
    let text_a = stdout(&out_a);
    assert!(text_a.contains("Route A meeting."), "{text_a}");
    assert!(text_a.contains("Went left."), "{text_a}");
    assert!(text_a.contains("chosen: left"), "{text_a}");
    assert!(
        !text_a.contains("Route B meeting."),
        "route A transcript must not play route B's doc: {text_a}"
    );

    let out_b = run(&[
        "play",
        dir.to_str().unwrap(),
        "--script",
        script_b.to_str().unwrap(),
    ]);
    assert!(out_b.status.success(), "route B: {}", stderr(&out_b));
    let text_b = stdout(&out_b);
    assert!(text_b.contains("Route B meeting."), "{text_b}");
    assert!(
        !text_b.contains("Route A meeting."),
        "route B transcript must not play route A's doc: {text_b}"
    );

    // Both routes still reach the causally-gated `errand` scene.
    assert!(text_a.contains("Running an errand."), "{text_a}");
    assert!(text_b.contains("Running an errand."), "{text_b}");
}

#[test]
fn world_lane_interleaves_between_errand_and_the_rewind_and_is_hidden_by_default() {
    let dir = fixture("world");
    let script_a = dir.join("routes/a.play.yaml");

    // Default `--lanes user`: world scene EXECUTES (its dialogue never
    // appears) but is omitted from the transcript.
    let default_out = run(&[
        "play",
        dir.to_str().unwrap(),
        "--script",
        script_a.to_str().unwrap(),
    ]);
    assert!(default_out.status.success(), "{}", stderr(&default_out));
    let default_text = stdout(&default_out);
    assert!(
        !default_text.contains("nera"),
        "world scene must be hidden under default --lanes user: {default_text}"
    );

    let all_out = run(&[
        "play",
        dir.to_str().unwrap(),
        "--script",
        script_a.to_str().unwrap(),
        "--lanes",
        "all",
    ]);
    assert!(all_out.status.success(), "{}", stderr(&all_out));
    let all_text = stdout(&all_out);
    assert!(all_text.contains("· world · nera/main"), "{all_text}");
    assert!(all_text.contains("World event fires."), "{all_text}");
    // World placement fires strictly after `errand`'s dialogue in the
    // transcript (drain happens after the covering user placement).
    let errand_pos = all_text
        .find("Running an errand.")
        .expect("errand line present");
    let world_pos = all_text
        .find("World event fires.")
        .expect("world line present");
    assert!(
        errand_pos < world_pos,
        "world event must interleave AFTER errand: {all_text}"
    );
}

#[test]
fn cold_open_presentation_override_rewinds_and_flags_the_world_drain() {
    let dir = fixture("rewind");
    let script_a = dir.join("routes/a.play.yaml");
    let out = run(&[
        "play",
        dir.to_str().unwrap(),
        "--script",
        script_a.to_str().unwrap(),
        "--lanes",
        "all",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);

    // `confinement` (presentation: 0) plays FIRST despite its story tick
    // being on day 2, well after `meet`/`errand`'s day-1 ticks.
    let confinement_pos = text
        .find("Cold open flashback.")
        .expect("confinement present");
    let meet_pos = text.find("Route A meeting.").expect("meet present");
    assert!(
        confinement_pos < meet_pos,
        "confinement must present before meet: {text}"
    );

    // The presentation jump backward is marked as a rewind.
    assert!(
        text.contains('\u{23EA}'),
        "expected a rewind marker (⏪) in: {text}"
    );
    assert!(text.contains("(rewind"), "{text}");

    // The world event drains inside the rewound segment and is flagged.
    assert!(text.contains("W-SCHED-WORLD-IN-FLASHBACK"), "{text}");
}

#[test]
fn incomplete_unscripted_choice_exits_three_and_names_the_hub_and_options() {
    let dir = fixture("incomplete");
    let out = run(&["play", dir.to_str().unwrap(), "--state", "run.route=a"]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "stdout: {}\nstderr: {}",
        stdout(&out),
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(text.contains("INCOMPLETE"), "{text}");
    assert!(text.contains("halted"), "{text}");
    assert!(text.contains("pick"), "{text}");
    assert!(text.contains("left"), "{text}");
    assert!(text.contains("right"), "{text}");
}

#[test]
fn auto_first_resolves_the_unscripted_choice_and_completes() {
    let dir = fixture("auto-first");
    let out = run(&[
        "play",
        dir.to_str().unwrap(),
        "--state",
        "run.route=a",
        "--auto",
        "first",
    ]);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&out),
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(
        text.contains("chosen: left"),
        "`first` must pick the first declared option: {text}"
    );
}

#[test]
fn variant_gap_exits_one_and_names_the_event() {
    let dir = fixture("gap");
    // Neither `meet` variant's guard (`run.route == 'a'|'b'`) is satisfiable.
    let out = run(&["play", dir.to_str().unwrap(), "--state", "run.route=c"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "stdout: {}\nstderr: {}",
        stdout(&out),
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(text.contains("E-SCHED-VARIANT-GAP"), "{text}");
    assert!(text.contains("meet"), "{text}");
}

#[test]
fn steps_stops_after_n_presented_placements() {
    let dir = fixture("steps");
    let script_a = dir.join("routes/a.play.yaml");
    let out = run(&[
        "play",
        dir.to_str().unwrap(),
        "--script",
        script_a.to_str().unwrap(),
        "--steps",
        "2",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("stopped after 2 step"), "{text}");
    // Only the first two presented placements (confinement, meet) ran —
    // `errand` (the third) never appears.
    assert!(text.contains("Cold open flashback."), "{text}");
    assert!(text.contains("Route A meeting."), "{text}");
    assert!(
        !text.contains("Running an errand."),
        "--steps 2 must stop before errand: {text}"
    );
}

#[test]
fn json_output_is_valid_and_carries_scene_records() {
    let dir = fixture("json");
    let script_a = dir.join("routes/a.play.yaml");
    let out = run(&[
        "play",
        dir.to_str().unwrap(),
        "--script",
        script_a.to_str().unwrap(),
        "--json",
    ]);
    assert!(out.status.success(), "{}", stderr(&out));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    assert_eq!(v["exit"], "complete");
    let scenes = v["scenes"].as_array().expect("scenes array");
    assert!(scenes.len() >= 3, "{v}");
    assert_eq!(scenes[0]["event"], "confinement");
    let meet = scenes
        .iter()
        .find(|s| s["event"] == "meet")
        .expect("meet scene present");
    assert_eq!(meet["doc"], "scenes/meet/routeA.lute");
    assert_eq!(meet["lane"], "user");
}

#[test]
fn same_seeds_and_script_produce_byte_identical_output() {
    let dir = fixture("determinism");
    let script_a = dir.join("routes/a.play.yaml");
    let args = [
        "play",
        dir.to_str().unwrap(),
        "--script",
        script_a.to_str().unwrap(),
        "--lanes",
        "all",
    ];
    let out1 = run(&args);
    let out2 = run(&args);
    assert!(out1.status.success() && out2.status.success());
    assert_eq!(
        out1.stdout, out2.stdout,
        "same seeds + script must produce byte-identical output"
    );
}

#[test]
fn a_project_with_no_schedule_yaml_is_a_hard_error() {
    // Design v2 §2: there is no `after:`-graph fallback — a project without
    // a schedule cannot select a route, so `play` refuses outright.
    let dir = temp_dir("no-schedule");
    write(&dir, "lute.project.yaml", PROJECT_YAML);
    write(&dir, "scenes/only.lute", CONFINEMENT);
    let out = run(&["play", dir.to_str().unwrap()]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "stdout: {}\nstderr: {}",
        stdout(&out),
        stderr(&out)
    );
    assert!(
        stderr(&out).contains("no schedule.yaml"),
        "{}",
        stderr(&out)
    );
}

#[test]
fn a_project_that_fails_to_compile_refuses_to_play() {
    let dir = fixture("badcompile");
    // Corrupt `errand`'s content so the whole-project compile gate fails.
    write(&dir, "scenes/errand/main.lute", "---\nkind: scene\ncharacter: errand\nseason: 1\nepisode: 1\n---\n\n@undeclaredspeakerwithnotext\n");
    let out = run(&["play", dir.to_str().unwrap(), "--state", "run.route=a"]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "stdout: {}\nstderr: {}",
        stdout(&out),
        stderr(&out)
    );
}

#[test]
fn play_help_matches_the_existing_command_tone() {
    let out = run(&["play", "--help"]);
    assert!(out.status.success());
    let text = stdout(&out);
    assert!(text.contains("schedule.yaml"), "{text}");
    assert!(text.contains("--script"), "{text}");
    assert!(text.contains("--lanes"), "{text}");
    assert!(text.contains("--steps"), "{text}");
}

#[test]
fn coverage_reports_full_corpus_covered_and_exits_zero() {
    let dir = fixture("coverage-full");
    // `a.play.yaml` (chooses `left`), a third script forcing `right`, and
    // `b.play.yaml` together exercise every placement (confinement/meet/
    // errand/nera), both `meet` variants, and both `pick` hub options — the
    // review-gap detector should report clean.
    write(
        &dir,
        "routes/a-right.play.yaml",
        "state:\n  run.route: a\nchoose:\n  meet/pick: right\n",
    );
    let out = run(&[
        "play",
        dir.to_str().unwrap(),
        "--coverage",
        dir.join("routes/a.play.yaml").to_str().unwrap(),
        "--coverage",
        dir.join("routes/a-right.play.yaml").to_str().unwrap(),
        "--coverage",
        dir.join("routes/b.play.yaml").to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&out),
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(text.contains("3 script(s) replayed"), "{text}");
    assert!(text.contains("COVERED"), "{text}");
    assert!(!text.contains("never presented"), "{text}");
    assert!(!text.contains("never selected"), "{text}");
    assert!(!text.contains("never chosen"), "{text}");
}

#[test]
fn coverage_reports_an_unchosen_hub_option_and_exits_one() {
    let dir = fixture("coverage-gap");
    // Only `a.play.yaml` (chooses `left`) and `b.play.yaml` (route B's
    // `meet` variant carries no hub at all) are in the corpus — `right` is
    // never chosen anywhere, though every placement/variant still presents.
    let out = run(&[
        "play",
        dir.to_str().unwrap(),
        "--coverage",
        dir.join("routes/a.play.yaml").to_str().unwrap(),
        "--coverage",
        dir.join("routes/b.play.yaml").to_str().unwrap(),
    ]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "stdout: {}\nstderr: {}",
        stdout(&out),
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(text.contains("never chosen: `meet/pick` "), "{text}");
    assert!(text.contains("right"), "{text}");
    assert!(!text.contains("never presented"), "{text}");
    assert!(!text.contains("never selected"), "{text}");
    assert!(text.contains("UNCOVERED"), "{text}");
}

#[test]
fn coverage_json_output_reports_the_uncovered_option() {
    let dir = fixture("coverage-json");
    let out = run(&[
        "play",
        dir.to_str().unwrap(),
        "--coverage",
        dir.join("routes/a.play.yaml").to_str().unwrap(),
        "--coverage",
        dir.join("routes/b.play.yaml").to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(
        out.status.code(),
        Some(1),
        "stdout: {}\nstderr: {}",
        stdout(&out),
        stderr(&out)
    );
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    assert_eq!(v["exit"], "uncovered");
    assert_eq!(v["scripts"].as_array().expect("scripts array").len(), 2);
    let opts = v["options"]["uncovered"]
        .as_array()
        .expect("uncovered options array");
    assert!(
        opts.iter()
            .any(|o| o["event"] == "meet" && o["id"] == "pick" && o["missing"][0] == "right"),
        "{v}"
    );
}

#[test]
fn coverage_is_exclusive_with_script_choose_and_steps() {
    let dir = fixture("coverage-exclusive");
    let out = run(&[
        "play",
        dir.to_str().unwrap(),
        "--coverage",
        dir.join("routes/a.play.yaml").to_str().unwrap(),
        "--script",
        dir.join("routes/b.play.yaml").to_str().unwrap(),
    ]);
    assert_eq!(
        out.status.code(),
        Some(2),
        "stdout: {}\nstderr: {}",
        stdout(&out),
        stderr(&out)
    );
    assert!(stderr(&out).contains("--coverage"), "{}", stderr(&out));
}

// ===========================================================================
// Implementation review fixes (2026-08-14 design v2 review) — one fixture
// per defect, each isolated from the `fixture()` project above so a
// regression in one never masks another.
// ===========================================================================

// -- review fix #1: reset each scene from its own state defaults -----------

const RESET_SCHEDULE: &str = "\
clock:
  buckets: [morning, afternoon]
  ticksPerBucket: 10
  days: 1

lanes:
  user: { exclusive: true }
  world: { exclusive: false }

placements:
  - event: scene-a
    lane: user
    at: morning+0
    size: 3
    doc: scenes/a-scene.lute
  - event: scene-b
    lane: user
    size: 3
    doc: scenes/b-scene.lute
";

const RESET_SCENE_A: &str = "\
---
kind: scene
character: scene-a
season: 1
episode: 1
state:
  scene.mood: { type: { enum: [calm, tense] }, default: calm }
---

## Shot 1.

<match on=\"scene.mood\">
<when is=\"calm\">
@narrator: scene-a sees calm.
</when>
<when is=\"tense\">
@narrator: scene-a sees tense.
</when>
</match>
";

const RESET_SCENE_B: &str = "\
---
kind: scene
character: scene-b
season: 1
episode: 1
state:
  scene.mood: { type: { enum: [calm, tense] }, default: tense }
---

## Shot 1.

<match on=\"scene.mood\">
<when is=\"calm\">
@narrator: scene-b sees calm.
</when>
<when is=\"tense\">
@narrator: scene-b sees tense.
</when>
</match>
";

/// Two scenes reuse the SAME `scene.*` path with DIFFERENT declared
/// defaults. Before the fix, the project-wide union picked the path-
/// sorted-FIRST document's default (`a-scene.lute`'s `calm`) for every
/// scene that reuses the path — `scene-b` would incorrectly start `calm`
/// too, instead of resetting to its own `tense` default.
#[test]
fn each_scene_resets_scene_state_from_its_own_declared_default() {
    let dir = temp_dir("scene-reset");
    write(&dir, "lute.project.yaml", PROJECT_YAML);
    write(&dir, "schedule.yaml", RESET_SCHEDULE);
    write(&dir, "scenes/a-scene.lute", RESET_SCENE_A);
    write(&dir, "scenes/b-scene.lute", RESET_SCENE_B);
    let out = run(&["play", dir.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&out),
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(text.contains("scene-a sees calm."), "{text}");
    assert!(text.contains("scene-b sees tense."), "{text}");
    assert!(
        !text.contains("scene-b sees calm."),
        "scene-b must reset from ITS OWN default: {text}"
    );
}

// -- review fix #2: resolve presentation order from live boundary state ----

const REVISIT_SCHEDULE: &str = "\
clock:
  buckets: [morning, afternoon]
  ticksPerBucket: 10
  days: 1

lanes:
  user: { exclusive: true }
  world: { exclusive: false }

placements:
  - event: gate-setter
    lane: user
    at: morning+0
    size: 2
    doc: scenes/gate-setter.lute
  - event: hidden-optional
    lane: user
    optional: true
    at: morning+2
    size: 2
    variants:
      - when: \"run.flag == 'yes'\"
        doc: scenes/hidden-optional.lute
";

const GATE_SETTER: &str = "\
---
kind: scene
character: gate-setter
season: 1
episode: 1
state:
  run.flag: { type: string, default: \"no\" }
---

## Shot 1.

@narrator: setter runs.
::set{run.flag = \"yes\"}
";

const HIDDEN_OPTIONAL: &str = "\
---
kind: scene
character: hidden-optional
season: 1
episode: 1
---

## Shot 1.

@narrator: hidden optional fires.
";

/// `hidden-optional`'s single variant guard reads false against SEED state
/// (`run.flag` defaults `no`). Before the fix, `presentation_order` resolved
/// every placement once at seed time and permanently dropped an unsatisfied
/// `optional` placement — even though `gate-setter` (presented first) flips
/// `run.flag` to `yes` before `hidden-optional`'s own boundary is reached.
#[test]
fn optional_placement_unsatisfied_at_seed_is_reconsidered_after_an_earlier_scene_sets_state() {
    let dir = temp_dir("optional-reconsidered");
    write(&dir, "lute.project.yaml", PROJECT_YAML);
    write(&dir, "schedule.yaml", REVISIT_SCHEDULE);
    write(&dir, "scenes/gate-setter.lute", GATE_SETTER);
    write(&dir, "scenes/hidden-optional.lute", HIDDEN_OPTIONAL);
    let out = run(&["play", dir.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&out),
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(text.contains("setter runs."), "{text}");
    assert!(
        text.contains("hidden optional fires."),
        "hidden-optional must be reconsidered live: {text}"
    );
}

const REORDER_SCHEDULE: &str = "\
clock:
  buckets: [morning, afternoon, evening]
  ticksPerBucket: 10
  days: 1

lanes:
  user: { exclusive: true }
  world: { exclusive: false }

placements:
  - event: setter
    lane: user
    at: morning+0
    size: 2
    doc: scenes/setter.lute
  - event: movable
    lane: user
    at: morning+2
    size: 2
    variants:
      - when: \"run.flag == 'no'\"
        doc: scenes/movable-no.lute
      - when: \"run.flag == 'yes'\"
        doc: scenes/movable-yes.lute
        at: evening+0
  - event: checkpoint
    lane: user
    at: afternoon+5
    size: 2
    doc: scenes/checkpoint.lute
";

const REORDER_SETTER: &str = "\
---
kind: scene
character: setter
season: 1
episode: 1
state:
  run.flag: { type: { enum: [no, yes] }, default: no }
---

## Shot 1.

@narrator: setter runs.
::set{run.flag = \"yes\"}
";

const MOVABLE_NO: &str = "\
---
kind: scene
character: movable-no
season: 1
episode: 1
---

## Shot 1.

@narrator: movable sees no.
";

const MOVABLE_YES: &str = "\
---
kind: scene
character: movable-yes
season: 1
episode: 1
---

## Shot 1.

@narrator: movable sees yes.
";

const REORDER_CHECKPOINT: &str = "\
---
kind: scene
character: checkpoint
season: 1
episode: 1
---

## Shot 1.

@narrator: checkpoint fires.
";

/// `movable`'s `no` variant (seed-active) sits at `morning+2`; `setter`
/// (presented first) flips `run.flag` to `yes`, switching movable's LIVE
/// variant to `yes`, whose OWN declared position is `evening+0` — strictly
/// after `checkpoint` (`afternoon+5`). Before the fix, `movable` still
/// played in its stale seed-time `no`-variant slot (right after `setter`,
/// before `checkpoint`) even though it rendered the live `yes` doc.
#[test]
fn a_placement_that_switches_variant_presents_at_the_live_variants_own_position() {
    let dir = temp_dir("reorder");
    write(&dir, "lute.project.yaml", PROJECT_YAML);
    write(&dir, "schedule.yaml", REORDER_SCHEDULE);
    write(&dir, "scenes/setter.lute", REORDER_SETTER);
    write(&dir, "scenes/movable-no.lute", MOVABLE_NO);
    write(&dir, "scenes/movable-yes.lute", MOVABLE_YES);
    write(&dir, "scenes/checkpoint.lute", REORDER_CHECKPOINT);
    let out = run(&["play", dir.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&out),
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(text.contains("movable sees yes."), "{text}");
    assert!(
        !text.contains("movable sees no."),
        "the live-active variant must be `yes`, never `no`: {text}"
    );
    let checkpoint_pos = text.find("checkpoint fires.").expect("checkpoint present");
    let movable_pos = text.find("movable sees yes.").expect("movable present");
    assert!(
        checkpoint_pos < movable_pos,
        "movable must present at its LIVE variant's own (later) position, after checkpoint, not the stale seed-time slot: {text}"
    );
}

// -- review fix #3: propagate unresolved variant guards as incomplete ------

const UNRESOLVED_GUARD_SCHEDULE: &str = "\
clock:
  buckets: [morning]
  ticksPerBucket: 10
  days: 1

lanes:
  user: { exclusive: true }
  world: { exclusive: false }

placements:
  - event: futures
    lane: user
    at: morning+0
    size: 2
    variants:
      - when: \"now() > 0\"
        doc: scenes/futures.lute
";

const FUTURES_SCENE: &str = "\
---
kind: scene
character: futures
season: 1
episode: 1
---

## Shot 1.

@narrator: should never print.
";

/// `futures`' sole variant guards on `now()` — a reference-runtime surface
/// `lute play` cannot resolve (design spec §4.5). Before the fix, variant
/// selection folded the guard's `Unknown` result straight to `false`,
/// raising a mundane `E-SCHED-VARIANT-GAP` (exit 1) instead of honestly
/// halting incomplete (exit 3) naming the unresolved surface.
#[test]
fn unresolved_variant_guard_halts_incomplete_naming_the_surface() {
    let dir = temp_dir("unresolved-guard");
    write(&dir, "lute.project.yaml", PROJECT_YAML);
    write(&dir, "schedule.yaml", UNRESOLVED_GUARD_SCHEDULE);
    write(&dir, "scenes/futures.lute", FUTURES_SCENE);
    let out = run(&["play", dir.to_str().unwrap()]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "stdout: {}\nstderr: {}",
        stdout(&out),
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(text.contains("futures"), "{text}");
    assert!(text.contains("now()"), "{text}");
    assert!(
        !text.contains("should never print."),
        "the guard must never silently decide false: {text}"
    );
    assert!(
        !text.contains("E-SCHED-VARIANT-GAP"),
        "an unresolved guard is incomplete, never a mundane gap: {text}"
    );
}

// -- review fix #4: rescan world placements after each world scene ---------

const RESCAN_SCHEDULE: &str = "\
clock:
  buckets: [morning, afternoon]
  ticksPerBucket: 12
  days: 1

lanes:
  user: { exclusive: true }
  world: { exclusive: false }

placements:
  - event: anchor
    lane: user
    at: 0
    size: 20
    doc: scenes/anchor.lute
  - event: flipper
    lane: world
    at: 1
    size: 1
    doc: scenes/flipper.lute
  - event: mover
    lane: world
    size: 1
    variants:
      - when: \"run.state == 'A'\"
        at: 10
        doc: scenes/mover-a.lute
      - when: \"run.state == 'B'\"
        at: 3
        doc: scenes/mover-b.lute
";

const RESCAN_ANCHOR: &str = "\
---
kind: scene
character: anchor
season: 1
episode: 1
state:
  run.state: { type: { enum: [A, B] }, default: A }
---

## Shot 1.

@narrator: anchor runs.
";

const RESCAN_FLIPPER: &str = "\
---
kind: scene
character: flipper
season: 1
episode: 1
state:
  run.state: { type: { enum: [A, B] }, default: A }
---

## Shot 1.

@narrator: flipper fires.
::set{run.state = \"B\"}
";

const RESCAN_MOVER_A: &str = "\
---
kind: scene
character: mover-a
season: 1
episode: 1
---

## Shot 1.

@narrator: mover sees A.
";

const RESCAN_MOVER_B: &str = "\
---
kind: scene
character: mover-b
season: 1
episode: 1
---

## Shot 1.

@narrator: mover sees B.
";

/// `flipper` (tick 1) and `mover` (whose SEED-active variant `A` sits at
/// tick 10) both drain in `anchor`'s ONE `[0, 20)` window. `flipper` fires
/// first and flips `run.state` to `B`, switching `mover`'s LIVE variant to
/// `B` (tick 3) — still inside the same window. Before the fix, the drain
/// scanned candidates ONCE upfront: `mover` was captured as the `A`
/// candidate (tick 10), and by the time its turn came the re-resolved tick
/// (3) no longer matched the scanned one, so it was silently dropped for
/// the rest of the playthrough once `world_cursor` advanced past tick 3.
#[test]
fn world_placement_switching_variant_mid_drain_is_rescanned_and_fires() {
    let dir = temp_dir("world-rescan");
    write(&dir, "lute.project.yaml", PROJECT_YAML);
    write(&dir, "schedule.yaml", RESCAN_SCHEDULE);
    write(&dir, "scenes/anchor.lute", RESCAN_ANCHOR);
    write(&dir, "scenes/flipper.lute", RESCAN_FLIPPER);
    write(&dir, "scenes/mover-a.lute", RESCAN_MOVER_A);
    write(&dir, "scenes/mover-b.lute", RESCAN_MOVER_B);
    let out = run(&["play", dir.to_str().unwrap(), "--lanes", "all"]);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&out),
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(text.contains("flipper fires."), "{text}");
    assert!(
        text.contains("mover sees B."),
        "mover must be rescanned onto its live `B` variant: {text}"
    );
    assert!(
        !text.contains("mover sees A."),
        "mover's seed-time `A` variant must never fire once B is live: {text}"
    );
}

// -- review fix #5: defer world variant gaps until the placement is due ----

const DEFER_SCHEDULE: &str = "\
clock:
  buckets: [morning, afternoon]
  ticksPerBucket: 12
  days: 1

lanes:
  user: { exclusive: true }
  world: { exclusive: false }

placements:
  - event: scene-one
    lane: user
    at: 0
    size: 5
    doc: scenes/scene-one.lute
  - event: scene-two
    lane: user
    at: 5
    size: 5
    doc: scenes/scene-two.lute
  - event: late-event
    lane: world
    at: 8
    size: 1
    variants:
      - when: \"run.unlocked == 'yes'\"
        doc: scenes/late-event.lute
";

const DEFER_SCENE_ONE: &str = "\
---
kind: scene
character: scene-one
season: 1
episode: 1
state:
  run.unlocked: { type: string, default: \"no\" }
---

## Shot 1.

@narrator: scene one runs.
";

const DEFER_SCENE_TWO: &str = "\
---
kind: scene
character: scene-two
season: 1
episode: 1
state:
  run.unlocked: { type: string, default: \"no\" }
---

## Shot 1.

@narrator: scene two runs.
::set{run.unlocked = \"yes\"}
";

const DEFER_LATE_EVENT: &str = "\
---
kind: scene
character: late-event
season: 1
episode: 1
---

## Shot 1.

@narrator: late event fires.
";

/// `late-event` (non-optional, world lane, tick 8) is only satisfiable once
/// `scene-two` sets `run.unlocked = yes` — but tick 8 is not due until
/// `scene-two`'s OWN drain window `[5, 10)`. Before the fix, the drain scan
/// resolved EVERY unfired world placement's guard before checking whether
/// its tick was even in range, so `late-event`'s guard was evaluated (and
/// found unsatisfied) during `scene-one`'s EARLIER `[0, 5)` drain, raising a
/// premature `E-SCHED-VARIANT-GAP` and aborting the whole playthrough.
#[test]
fn world_variant_gap_is_deferred_until_the_placement_is_actually_due() {
    let dir = temp_dir("world-defer");
    write(&dir, "lute.project.yaml", PROJECT_YAML);
    write(&dir, "schedule.yaml", DEFER_SCHEDULE);
    write(&dir, "scenes/scene-one.lute", DEFER_SCENE_ONE);
    write(&dir, "scenes/scene-two.lute", DEFER_SCENE_TWO);
    write(&dir, "scenes/late-event.lute", DEFER_LATE_EVENT);
    let out = run(&["play", dir.to_str().unwrap(), "--lanes", "all"]);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&out),
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(text.contains("scene one runs."), "{text}");
    assert!(text.contains("scene two runs."), "{text}");
    assert!(
        text.contains("late event fires."),
        "late-event's gap check must defer until its own boundary: {text}"
    );
    let scene_two_pos = text.find("scene two runs.").expect("scene two present");
    let late_pos = text.find("late event fires.").expect("late event present");
    assert!(
        scene_two_pos < late_pos,
        "late-event must drain after the scene that unlocks it: {text}"
    );
}

// -- review fix #6: halt when a hub decision sequence is exhausted ---------

const HUB_SCHEDULE: &str = "\
clock:
  buckets: [morning]
  ticksPerBucket: 10
  days: 1

lanes:
  user: { exclusive: true }
  world: { exclusive: false }

placements:
  - event: camp-scene
    lane: user
    at: morning+0
    size: 4
    doc: scenes/camp-scene.lute
";

const HUB_SCENE: &str = "\
---
kind: scene
character: camp-scene
season: 1
episode: 1
---

## Shot 1.

<hub id=\"camp\">
<choice id=\"try\" label=\"Try\" once>
@narrator: tried once.
</choice>
<choice id=\"leave\" label=\"Leave\" exit>
@narrator: left camp.
</choice>
</hub>
@narrator: after hub.
";

/// A route script scripts only `[try]` — `try` is `once`, not `exit`, so the
/// hub is re-presented with `leave` still eligible and no more scripted
/// decisions. Before the fix, `Runner::do_hub` iterated the whole forced
/// vector and converged regardless, silently leaving the hub (exit 0)
/// instead of re-presenting it or halting incomplete.
#[test]
fn hub_sequence_exhausted_with_eligible_options_remaining_halts_incomplete() {
    let dir = temp_dir("hub-exhausted");
    write(&dir, "lute.project.yaml", PROJECT_YAML);
    write(&dir, "schedule.yaml", HUB_SCHEDULE);
    write(&dir, "scenes/camp-scene.lute", HUB_SCENE);
    let script = write(&dir, "routes/partial.play.yaml", "choose:\n  camp: [try]\n");
    let out = run(&[
        "play",
        dir.to_str().unwrap(),
        "--script",
        script.to_str().unwrap(),
    ]);
    assert_eq!(
        out.status.code(),
        Some(3),
        "stdout: {}\nstderr: {}",
        stdout(&out),
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(text.contains("incomplete"), "{text}");
    assert!(text.contains("camp"), "{text}");
    assert!(
        text.contains("leave"),
        "the still-eligible option must be named: {text}"
    );
    assert!(
        !text.contains("after hub."),
        "the walk must not fall through past an exhausted hub: {text}"
    );
}

/// The SAME hub with NO script at all, under `--auto first`: the policy must
/// keep applying at EVERY re-presentation (first `try`, once it is spent
/// `leave`), not just supply one decision for the whole hub.
#[test]
fn auto_first_applies_at_every_hub_re_presentation_until_exit() {
    let dir = temp_dir("hub-auto-first-multistep");
    write(&dir, "lute.project.yaml", PROJECT_YAML);
    write(&dir, "schedule.yaml", HUB_SCHEDULE);
    write(&dir, "scenes/camp-scene.lute", HUB_SCENE);
    let out = run(&["play", dir.to_str().unwrap(), "--auto", "first"]);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&out),
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(text.contains("tried once."), "{text}");
    assert!(text.contains("left camp."), "{text}");
    assert!(
        text.contains("after hub."),
        "the hub must fully converge, not stop after one re-presentation: {text}"
    );
}

// ===========================================================================
// dsl 0.12.0: forward jump (`::mark`/line `id=`/`::next`) — one placement
// whose branch arm rejoins a LATER shot via an unconditional `::next`, then
// a GUARDED `::next` picks between two independent `::end{reason}`s
// (multi-end combination). `lute play` and `lute run` share the SAME
// `Runner` (`runner.rs`), so a `lute play` smoke over ONE scene doc is a
// faithful end-to-end exercise of the whole forward-jump pipeline: check
// (labels resolve, no E-NEXT-*/E-MARK-DUP) -> compile (`jump`/`match`
// records, named-label addressing) -> runtime walk (the jump actually
// moves the PC, the guard actually forks).
// ===========================================================================

const NEXT_SCHEDULE: &str = "\
clock:
  buckets: [morning]
  ticksPerBucket: 10
  days: 1

lanes:
  user: { exclusive: true }
  world: { exclusive: false }

placements:
  - event: forward-jump
    lane: user
    at: morning+0
    size: 5
    doc: scenes/forward-jump.lute
";

const NEXT_SCENE: &str = "\
---
kind: scene
character: forward-jump
season: 1
episode: 1
state:
  run.blessed: { type: bool, default: false }
---

## Shot 1.

<branch id=\"pick\">
  <choice id=\"a\" label=\"A\">
    ::next{to=\"join\"}
  </choice>
  <choice id=\"b\" label=\"B\">
    @narrator: taking the b path
  </choice>
</branch>

## Shot 2.

::mark{id=\"join\"}
@narrator{id=\"afterJoin\"}: we joined here
::next{to=\"tail\" when=\"run.blessed\"}
@narrator: fallthrough content
::end{reason=\"completed\"}

## Shot 3.

::mark{id=\"tail\"}
@narrator: tail reached
::end{reason=\"tailed\"}
";

/// Choice `a`'s unconditional `::next{to=\"join\"}` skips straight to shot
/// 2's `::mark{id=\"join\"}` — never rendering \"taking the b path\" — then
/// the guarded `::next{to=\"tail\" when=\"run.blessed\"}` fires TRUE
/// (`run.blessed=true`), joining shot 3 and ending on `reason=tailed`
/// rather than shot 2's own `reason=completed`.
#[test]
fn branch_arm_next_joins_a_later_shot_and_guarded_next_reaches_the_far_end() {
    let dir = temp_dir("next-join-true");
    write(&dir, "lute.project.yaml", PROJECT_YAML);
    write(&dir, "schedule.yaml", NEXT_SCHEDULE);
    write(&dir, "scenes/forward-jump.lute", NEXT_SCENE);
    let script = write(
        &dir,
        "route.play.yaml",
        "state:\n  run.blessed: true\nchoose:\n  pick: a\n",
    );
    let out = run(&[
        "play",
        dir.to_str().unwrap(),
        "--script",
        script.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&out),
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(text.contains("we joined here"), "{text}");
    assert!(
        text.contains("tail reached"),
        "the guarded next's true arm must reach shot 3: {text}"
    );
    assert!(
        !text.contains("taking the b path"),
        "the unchosen branch arm must never render: {text}"
    );
    assert!(
        !text.contains("fallthrough content"),
        "the guarded next's false arm must not also render: {text}"
    );
}

/// SAME scene, guard FALSE this time: the guarded `::next` falls through to
/// \"fallthrough content\" and the FIRST `::end{reason=\"completed\"}` —
/// never reaching shot 3's `tail` mark or its OWN `reason=\"tailed\"` end —
/// the multi-end combination's other arm.
#[test]
fn guarded_next_false_arm_falls_through_to_its_own_end_reason() {
    let dir = temp_dir("next-join-false");
    write(&dir, "lute.project.yaml", PROJECT_YAML);
    write(&dir, "schedule.yaml", NEXT_SCHEDULE);
    write(&dir, "scenes/forward-jump.lute", NEXT_SCENE);
    let script = write(
        &dir,
        "route.play.yaml",
        "state:\n  run.blessed: false\nchoose:\n  pick: a\n",
    );
    let out = run(&[
        "play",
        dir.to_str().unwrap(),
        "--script",
        script.to_str().unwrap(),
    ]);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&out),
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(text.contains("we joined here"), "{text}");
    assert!(
        text.contains("fallthrough content"),
        "the guarded next's false arm must fall through: {text}"
    );
    assert!(
        !text.contains("tail reached"),
        "the false arm must never reach shot 3: {text}"
    );
}

// --- Task 4 §2 (dsl 0.15.0) — play consumes authored `meta.id` -------------

/// A schedule presents a scene declared with an authored `id:` (no legacy
/// triad), then a follow-up whose `after: visited("<that id>")` names it.
/// Pre-fix, `play`'s `scene_canonical_key` derived the visited key from
/// `meta.character`/`.season`/`.episode` — an authored-id doc has none, so
/// the follow-up's prerequisite was never satisfied and the placement was
/// filtered out. The follow-up must now play.
#[test]
fn play_records_the_authored_scene_id_in_the_visited_set() {
    let dir = temp_dir("authored-visited");
    write(&dir, "lute.project.yaml", PROJECT_YAML);
    write(
        &dir,
        "schedule.yaml",
        "\
clock:
  buckets: [morning, afternoon]
  ticksPerBucket: 10
  days: 1

lanes:
  user: { exclusive: true }

placements:
  - event: opener
    lane: user
    at: morning+0
    size: 3
    doc: scenes/opener.lute
  - event: followup
    lane: user
    at: afternoon+0
    size: 3
    doc: scenes/followup.lute
",
    );
    write(
        &dir,
        "scenes/opener.lute",
        "\
---
kind: scene
id: authored.opener
---

## Shot 1.

@hero: Opening under an authored id.
",
    );
    write(
        &dir,
        "scenes/followup.lute",
        "\
---
kind: scene
id: authored.followup
after: 'visited(\"authored.opener\")'
---

## Shot 1.

@hero: Follow-up gated on the authored id.
",
    );

    let out = run(&["play", dir.to_str().unwrap()]);
    assert!(
        out.status.success(),
        "stdout: {}\nstderr: {}",
        stdout(&out),
        stderr(&out)
    );
    let text = stdout(&out);
    assert!(
        text.contains("Opening under an authored id."),
        "opener must present: {text}"
    );
    assert!(
        text.contains("Follow-up gated on the authored id."),
        "the follow-up whose `after:` names the authored id must play: {text}",
    );
}
