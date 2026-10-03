//! Names as the engine spells them, end to end: a quest `zero-coke-001`,
//! places `lab-b2` and `001` with a per-place counter, a fact
//! `at("lab-b2")`. Conditions reach them by a quoted index; every tool keys
//! on the canonical dotted path, and a seed, an expectation or an axis may
//! use either spelling.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/paths-030")
}

fn lute(args: &[&str]) -> Output {
    Command::new(BIN).args(args).output().unwrap()
}

fn text(o: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&o.stdout),
        String::from_utf8_lossy(&o.stderr)
    )
}

fn dir() -> String {
    fixture().display().to_string()
}

#[test]
fn the_project_checks_clean() {
    let out = lute(&["check-project", &dir()]);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(!t.contains("error ["), "{t}");
    assert!(!t.contains("warning ["), "{t}");
}

/// A play writes and reads the quoted members, seeds and expects them in
/// either spelling, completes the quest and plays the scene whose `when`
/// reads `quest["zero-coke-001"].state` and a rule over `run.visits[P]`.
#[test]
fn a_play_reads_and_writes_quoted_names() {
    let script = fixture().join("plays/p.play.yaml");
    let out = lute(&["play", &dir(), "--script", script.to_str().unwrap()]);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("Lab B2, visit 1."), "{t}");
    assert!(t.contains("Room 001, visit 1."), "{t}");
    assert!(t.contains("Lab B2, visit 2."), "{t}");
    assert!(t.contains("Wrapped after 2 visits."), "{t}");
}

/// A test seeds `quest["zero-coke-001"].state` and `run.visits["lab-b2"]`
/// and expects the member back under its quoted key.
#[test]
fn a_test_seeds_and_expects_quoted_keys() {
    let test = fixture().join("tests/wrap.test.yaml");
    let out = lute(&["test", test.to_str().unwrap()]);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
}

/// `--state` takes a bracket key and seeds the same path its dotted spelling
/// names; a guard that stays false names the path as a condition writes it.
#[test]
fn trace_seeds_a_bracket_state_key() {
    let scene = fixture().join("scenes/wrap.lute");
    let scene = scene.to_str().unwrap();
    let project = dir();
    let seeded = lute(&[
        "trace",
        scene,
        "--project",
        &project,
        "--state",
        "quest[\"zero-coke-001\"].state=complete",
        "--state",
        "run.visits[\"lab-b2\"]=2",
    ]);
    let t = text(&seeded);
    assert_eq!(seeded.status.code(), Some(0), "{t}");
    assert!(t.contains("Wrapped after 2 visits."), "{t}");

    // The dotted spelling is the same path.
    let dotted = lute(&[
        "trace",
        scene,
        "--project",
        &project,
        "--state",
        "quest.zero-coke-001.state=complete",
        "--state",
        "run.visits.lab-b2=2",
    ]);
    assert_eq!(text(&dotted), t);
}

/// The human outline and the calendar show a member the way a condition
/// reaches it.
#[test]
fn context_and_calendar_spell_quoted_members() {
    let scene = fixture().join("scenes/wrap.lute");
    let out = lute(&["context", scene.to_str().unwrap(), "--project", &dir()]);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("run.visits[\"lab-b2\"]: int"), "{t}");
    assert!(t.contains("run.visits[\"001\"]: int"), "{t}");
    assert!(t.contains("run.visits.hall: int"), "{t}");

    let out = lute(&[
        "calendar",
        &dir(),
        "--axis",
        "run.visits['lab-b2']=1..2",
        "--occasion",
        "wrap",
    ]);
    let t = text(&out);
    assert!(out.status.success(), "{t}");
    assert!(t.contains("run.visits[\"lab-b2\"]"), "{t}");
}
