//! `lute init --template beats` / `lute new scene --on` (dsl 0.22.0 §13).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-scaffold-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn lute(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn init_beats(tag: &str) -> PathBuf {
    let proj = temp_dir(tag).join("proj");
    let out = lute(&["init", proj.to_str().unwrap(), "--template", "beats"]);
    assert!(out.status.success(), "{}", text(&out));
    proj
}

/// The beats scaffold passes `check-project`, `test` and `play` out of the
/// box — the three commands its README and play script promise.
#[test]
fn beats_template_checks_tests_and_plays_clean() {
    let proj = init_beats("beats");
    let p = proj.to_str().unwrap();
    for rel in [
        "plugins/game.occasions/plugin.yaml",
        "plays/first-day.play.yaml",
        "tests/mara-first.test.yaml",
    ] {
        assert!(proj.join(rel).is_file(), "missing {rel}");
    }
    let check = lute(&["check-project", p]);
    assert_eq!(check.status.code(), Some(0), "{}", text(&check));
    let test = lute(&["test", p, "--project", p]);
    assert_eq!(test.status.code(), Some(0), "{}", text(&test));
    let script = proj.join("plays/first-day.play.yaml");
    let play = lute(&["play", p, "--script", script.to_str().unwrap()]);
    assert_eq!(play.status.code(), Some(0), "{}", text(&play));
}

fn scene(proj: &Path, rel: &str) -> String {
    std::fs::read_to_string(proj.join("scenes").join(rel)).unwrap()
}

/// `lute new scene --on` writes a beat with an `id:`, omits what the
/// manifest's `defaults:` supplies, lands under the project root even when
/// run from a subdirectory, and still checks clean.
#[test]
fn new_scene_on_writes_a_beat_that_respects_defaults() {
    let proj = init_beats("new-on");
    let from_subdir = proj.join("scenes/talk");
    let out = lute(&[
        "new",
        "scene",
        "talk/tomas-first",
        "--on",
        "talk",
        "--target",
        "npc.tomas",
        "--dir",
        from_subdir.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    let beat = scene(&proj, "talk/tomas-first.lute");
    assert!(beat.contains("\nid: talk.tomasFirst\n"), "{beat}");
    assert!(beat.contains("\non: talk\ntarget: npc.tomas\n"), "{beat}");
    assert!(!beat.contains("luteVersion") && !beat.contains("uses:"), "{beat}");
    assert!(!beat.contains("character:"), "no legacy identity triple: {beat}");

    let check = lute(&["check-project", proj.to_str().unwrap()]);
    assert_eq!(check.status.code(), Some(0), "{}", text(&check));
}

/// An undeclared occasion or an out-of-domain target is refused (exit 2)
/// and leaves no file behind.
#[test]
fn new_scene_on_refuses_what_the_project_does_not_declare() {
    let proj = init_beats("new-refuse");
    let d = proj.to_str().unwrap();
    let out = lute(&["new", "scene", "x", "--on", "tallk", "--dir", d]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(text(&out).contains("did you mean `talk`?"), "{}", text(&out));
    assert!(!proj.join("scenes/x.lute").exists());

    let out = lute(&["new", "scene", "y", "--on", "talk", "--target", "npc.oskar", "--dir", d]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(text(&out).contains("npc.oskar"), "{}", text(&out));
    assert!(!proj.join("scenes/y.lute").exists());

    let out = lute(&["new", "scene", "z", "--on", "hubVisit", "--target", "npc.mara", "--dir", d]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(!proj.join("scenes/z.lute").exists());
}

/// Outside a project `lute new` says so; `--on` has nothing to answer there
/// and is refused.
#[test]
fn new_outside_a_project_says_so() {
    let dir = temp_dir("new-outside");
    let d = dir.to_str().unwrap();
    let out = lute(&["new", "scene", "intro", "--on", "talk", "--dir", d]);
    assert_eq!(out.status.code(), Some(2), "{}", text(&out));
    assert!(text(&out).contains("not inside a Lute project"), "{}", text(&out));
    assert!(!dir.join("scenes/intro.lute").exists());

    let out = lute(&["new", "scene", "intro", "--dir", d]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("not inside a Lute project"),
        "{}",
        text(&out)
    );
    let intro = scene(&dir, "intro.lute");
    assert!(intro.contains("\nid: intro\n") && intro.contains("luteVersion:"), "{intro}");
}
