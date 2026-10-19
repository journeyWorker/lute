//! 0.41.1 §4.2 (#83): a presented beat's `advances:` shows in the human
//! transcript — the movement in the `advance:` step's spelling, then every
//! clock raise it made or found closed, in raise order. `--events` is
//! unchanged.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/play-transcript-advances-041")
}

fn play(extra: &[&str]) -> (i32, String) {
    let dir = fixture();
    let out = Command::new(BIN)
        .arg("play")
        .arg("--project")
        .arg(&dir)
        .arg("--script")
        .arg(dir.join("advances.play.yaml"))
        .args(extra)
        .output()
        .unwrap();
    (
        out.status.code().unwrap(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

/// The work beat moves the clock from dusk past midnight: `night`
/// (`dayEnd`) presents, `morning` (`dayStart`) is closed by its gate, and
/// `tick` (the slot) presents — each at its place after the movement line.
#[test]
fn a_beat_advance_shows_its_movement_and_every_raise() {
    let (code, text) = play(&[]);
    assert_eq!(code, 0, "{text}");
    assert_eq!(
        text,
        "── step 1 · engine ──────────────\n  \
         set run.slot = \"dusk\"\n\
         ── step 2 · work ──────────────\n  \
         ✓ work [scene, priority 0]\n  \
         → work\n\
         @narrator: Work advances the clock.\n  \
         ↻ advance slot: day 1 dusk → day 2 dawn\n  \
         ↻ night (clock raise)\n\
         @narrator: Night falls.\n  \
         ↻ morning (clock raise — not raised: its `raisedWhen: run.ready` is false there since \
         `run.ready` is false; the clock moved on without it)\n  \
         ↻ tick (clock raise)\n  \
         set run.ticks = 1\n\
         @narrator: The slot is raised.\n\
         ── end: complete (2 steps) ──────────────\n"
    );
}

/// The event stream carries the movement as its `clock` event and gains no
/// transcript line.
#[test]
fn events_are_unchanged() {
    let (code, events) = play(&["--events"]);
    assert_eq!(code, 0, "{events}");
    assert!(
        events.contains(
            "{\"from\":{\"day\":1,\"slot\":1},\"to\":{\"day\":2,\"slot\":0},\"type\":\"clock\"}"
        ),
        "{events}"
    );
    assert!(!events.contains("↻"), "{events}");
}
