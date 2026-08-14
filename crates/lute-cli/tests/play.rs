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

    let out_a = run(&["play", dir.to_str().unwrap(), "--script", script_a.to_str().unwrap()]);
    assert!(out_a.status.success(), "route A: {}", stderr(&out_a));
    let text_a = stdout(&out_a);
    assert!(text_a.contains("Route A meeting."), "{text_a}");
    assert!(text_a.contains("Went left."), "{text_a}");
    assert!(text_a.contains("chosen: left"), "{text_a}");
    assert!(!text_a.contains("Route B meeting."), "route A transcript must not play route B's doc: {text_a}");

    let out_b = run(&["play", dir.to_str().unwrap(), "--script", script_b.to_str().unwrap()]);
    assert!(out_b.status.success(), "route B: {}", stderr(&out_b));
    let text_b = stdout(&out_b);
    assert!(text_b.contains("Route B meeting."), "{text_b}");
    assert!(!text_b.contains("Route A meeting."), "route B transcript must not play route A's doc: {text_b}");

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
    let default_out = run(&["play", dir.to_str().unwrap(), "--script", script_a.to_str().unwrap()]);
    assert!(default_out.status.success(), "{}", stderr(&default_out));
    let default_text = stdout(&default_out);
    assert!(!default_text.contains("nera"), "world scene must be hidden under default --lanes user: {default_text}");

    let all_out = run(&["play", dir.to_str().unwrap(), "--script", script_a.to_str().unwrap(), "--lanes", "all"]);
    assert!(all_out.status.success(), "{}", stderr(&all_out));
    let all_text = stdout(&all_out);
    assert!(all_text.contains("· world · nera/main"), "{all_text}");
    assert!(all_text.contains("World event fires."), "{all_text}");
    // World placement fires strictly after `errand`'s dialogue in the
    // transcript (drain happens after the covering user placement).
    let errand_pos = all_text.find("Running an errand.").expect("errand line present");
    let world_pos = all_text.find("World event fires.").expect("world line present");
    assert!(errand_pos < world_pos, "world event must interleave AFTER errand: {all_text}");
}

#[test]
fn cold_open_presentation_override_rewinds_and_flags_the_world_drain() {
    let dir = fixture("rewind");
    let script_a = dir.join("routes/a.play.yaml");
    let out = run(&["play", dir.to_str().unwrap(), "--script", script_a.to_str().unwrap(), "--lanes", "all"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);

    // `confinement` (presentation: 0) plays FIRST despite its story tick
    // being on day 2, well after `meet`/`errand`'s day-1 ticks.
    let confinement_pos = text.find("Cold open flashback.").expect("confinement present");
    let meet_pos = text.find("Route A meeting.").expect("meet present");
    assert!(confinement_pos < meet_pos, "confinement must present before meet: {text}");

    // The presentation jump backward is marked as a rewind.
    assert!(text.contains('\u{23EA}'), "expected a rewind marker (⏪) in: {text}");
    assert!(text.contains("(rewind"), "{text}");

    // The world event drains inside the rewound segment and is flagged.
    assert!(text.contains("W-SCHED-WORLD-IN-FLASHBACK"), "{text}");
}

#[test]
fn incomplete_unscripted_choice_exits_three_and_names_the_hub_and_options() {
    let dir = fixture("incomplete");
    let out = run(&["play", dir.to_str().unwrap(), "--state", "run.route=a"]);
    assert_eq!(out.status.code(), Some(3), "stdout: {}\nstderr: {}", stdout(&out), stderr(&out));
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
    let out = run(&["play", dir.to_str().unwrap(), "--state", "run.route=a", "--auto", "first"]);
    assert!(out.status.success(), "stdout: {}\nstderr: {}", stdout(&out), stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("chosen: left"), "`first` must pick the first declared option: {text}");
}

#[test]
fn variant_gap_exits_one_and_names_the_event() {
    let dir = fixture("gap");
    // Neither `meet` variant's guard (`run.route == 'a'|'b'`) is satisfiable.
    let out = run(&["play", dir.to_str().unwrap(), "--state", "run.route=c"]);
    assert_eq!(out.status.code(), Some(1), "stdout: {}\nstderr: {}", stdout(&out), stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("E-SCHED-VARIANT-GAP"), "{text}");
    assert!(text.contains("meet"), "{text}");
}

#[test]
fn steps_stops_after_n_presented_placements() {
    let dir = fixture("steps");
    let script_a = dir.join("routes/a.play.yaml");
    let out = run(&["play", dir.to_str().unwrap(), "--script", script_a.to_str().unwrap(), "--steps", "2"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let text = stdout(&out);
    assert!(text.contains("stopped after 2 step"), "{text}");
    // Only the first two presented placements (confinement, meet) ran —
    // `errand` (the third) never appears.
    assert!(text.contains("Cold open flashback."), "{text}");
    assert!(text.contains("Route A meeting."), "{text}");
    assert!(!text.contains("Running an errand."), "--steps 2 must stop before errand: {text}");
}

#[test]
fn json_output_is_valid_and_carries_scene_records() {
    let dir = fixture("json");
    let script_a = dir.join("routes/a.play.yaml");
    let out = run(&["play", dir.to_str().unwrap(), "--script", script_a.to_str().unwrap(), "--json"]);
    assert!(out.status.success(), "{}", stderr(&out));
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).expect("valid JSON");
    assert_eq!(v["exit"], "complete");
    let scenes = v["scenes"].as_array().expect("scenes array");
    assert!(scenes.len() >= 3, "{v}");
    assert_eq!(scenes[0]["event"], "confinement");
    let meet = scenes.iter().find(|s| s["event"] == "meet").expect("meet scene present");
    assert_eq!(meet["doc"], "scenes/meet/routeA.lute");
    assert_eq!(meet["lane"], "user");
}

#[test]
fn same_seeds_and_script_produce_byte_identical_output() {
    let dir = fixture("determinism");
    let script_a = dir.join("routes/a.play.yaml");
    let args = ["play", dir.to_str().unwrap(), "--script", script_a.to_str().unwrap(), "--lanes", "all"];
    let out1 = run(&args);
    let out2 = run(&args);
    assert!(out1.status.success() && out2.status.success());
    assert_eq!(out1.stdout, out2.stdout, "same seeds + script must produce byte-identical output");
}

#[test]
fn a_project_with_no_schedule_yaml_is_a_hard_error() {
    // Design v2 §2: there is no `after:`-graph fallback — a project without
    // a schedule cannot select a route, so `play` refuses outright.
    let dir = temp_dir("no-schedule");
    write(&dir, "lute.project.yaml", PROJECT_YAML);
    write(&dir, "scenes/only.lute", CONFINEMENT);
    let out = run(&["play", dir.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2), "stdout: {}\nstderr: {}", stdout(&out), stderr(&out));
    assert!(stderr(&out).contains("no schedule.yaml"), "{}", stderr(&out));
}

#[test]
fn a_project_that_fails_to_compile_refuses_to_play() {
    let dir = fixture("badcompile");
    // Corrupt `errand`'s content so the whole-project compile gate fails.
    write(&dir, "scenes/errand/main.lute", "---\nkind: scene\ncharacter: errand\nseason: 1\nepisode: 1\n---\n\n@undeclaredspeakerwithnotext\n");
    let out = run(&["play", dir.to_str().unwrap(), "--state", "run.route=a"]);
    assert_eq!(out.status.code(), Some(1), "stdout: {}\nstderr: {}", stdout(&out), stderr(&out));
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
