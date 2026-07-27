//! dsl 0.8.0 `::end` in the reference engine (`lute run`, `src/runner.rs`):
//! the `end` record stops the walk, surfaces its `reason`, and still exits
//! `complete` — behaviorally identical to running off the end of `commands`.
//!
//! Also replays the `conformance/end-reason` fixture against its checked-in
//! `expected.json`, so the frozen third-party contract is enforced in CI and
//! not only by the README's manual loop.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-end-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// Compile `source` in a scratch dir and run it (optionally under `mock`),
/// returning the `--json` machine transcript.
fn compile_and_run(tag: &str, source: &str, mock: Option<&str>) -> serde_json::Value {
    let dir = temp_dir(tag);
    let src = dir.join("source.lute");
    let art = dir.join("artifact.json");
    std::fs::write(&src, source).unwrap();

    let out = Command::new(BIN)
        .args(["compile", src.to_str().unwrap(), "-o", art.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "compile: {}", String::from_utf8_lossy(&out.stderr));

    let mut args = vec!["run".to_string(), art.to_str().unwrap().to_string()];
    if let Some(m) = mock {
        let mp = dir.join("mock.yaml");
        std::fs::write(&mp, m).unwrap();
        args.push("--mock".into());
        args.push(mp.to_str().unwrap().to_string());
    }
    args.push("--json".into());
    let out = Command::new(BIN).args(&args).output().unwrap();
    assert!(out.status.success(), "run: {}", String::from_utf8_lossy(&out.stderr));
    serde_json::from_slice(&out.stdout).unwrap()
}

const HDR: &str = "---\nkind: scene\nluteVersion: \"0.8.0\"\ncharacter: hero\nseason: 1\n\
    episode: 1\ntitle: T\n---\n\n## Shot 1.\n\n";

fn kinds(v: &serde_json::Value) -> Vec<&str> {
    v["commands"].as_array().unwrap().iter().map(|c| c["kind"].as_str().unwrap()).collect()
}

#[test]
fn end_stops_the_walk_and_surfaces_its_reason() {
    let v = compile_and_run(
        "linear",
        &format!("{HDR}@narrator: seen\n::end{{reason=\"completed\"}}\n@narrator: unreachable\n"),
        None,
    );
    assert_eq!(kinds(&v), ["line", "end"], "{v}");
    assert_eq!(v["commands"][1]["reason"], "completed");
    assert_eq!(v["exit"], "complete", "an `end` walk is COMPLETE, not incomplete");
}

/// The whole point of the record: identical to falling off the end of the
/// command array — same `exit`, same absence of later records — except that
/// the reason is surfaced.
#[test]
fn ending_matches_falling_off_the_end_except_for_the_reason() {
    let ended = compile_and_run("ended", &format!("{HDR}@narrator: seen\n::end\n"), None);
    let fell_off = compile_and_run("felloff", &format!("{HDR}@narrator: seen\n"), None);
    assert_eq!(ended["exit"], fell_off["exit"]);
    assert_eq!(ended["state"], fell_off["state"]);
    assert_eq!(kinds(&ended), ["line", "end"]);
    assert_eq!(kinds(&fell_off), ["line"]);
    // No `reason` authored -> the key is present but null, never a fabricated value.
    assert!(ended["commands"][1]["reason"].is_null(), "{ended}");
}

/// An `end` inside a hub option body unwinds the BOUNDED segment and the hub
/// loop with it: no further option is presented and the converge never runs.
#[test]
fn end_inside_a_hub_option_terminates_the_whole_walk() {
    let v = compile_and_run(
        "hub",
        &format!(
            "{HDR}<hub id=\"camp\">\n\
             <choice id=\"talk\" label=\"Talk\">\n@hero: A word?\n::end{{reason=\"walked off\"}}\n</choice>\n\
             <choice id=\"go\" label=\"Leave\" exit>\n@hero: Later.\n</choice>\n\
             </hub>\n\
             @narrator: after the hub\n"
        ),
        Some("choose:\n  camp: [talk, go]\n"),
    );
    assert_eq!(kinds(&v), ["hub", "line", "end"], "{v}");
    assert_eq!(v["commands"][2]["reason"], "walked off");
    assert_eq!(v["exit"], "complete");
}

/// Regression guard for the byte-stability contract: a document using none of
/// the 0.8.0 feature must produce a transcript with no `end` record at all.
#[test]
fn a_walk_without_end_is_untouched() {
    let v = compile_and_run("plain", &format!("{HDR}@narrator: a\n@narrator: b\n"), None);
    assert_eq!(kinds(&v), ["line", "line"]);
    assert_eq!(v["exit"], "complete");
}

/// The frozen third-party contract: `conformance/end-reason` replays
/// byte-identically to its checked-in `expected.json` (conformance/README.md).
#[test]
fn conformance_end_reason_replays_byte_identically() {
    let dir = Path::new("../../conformance/end-reason");
    let out = Command::new(BIN)
        .args([
            "run",
            dir.join("artifact.json").to_str().unwrap(),
            "--mock",
            dir.join("mock.yaml").to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(out.status.success(), "run: {}", String::from_utf8_lossy(&out.stderr));
    let got = String::from_utf8(out.stdout).unwrap();
    let expected = std::fs::read_to_string(dir.join("expected.json")).unwrap();
    assert_eq!(got, expected, "transcript must match the frozen fixture byte for byte");
}
