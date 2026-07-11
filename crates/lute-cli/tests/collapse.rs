//! Task 14 (dsl 0.4.0 §8.2/§8.3) — CLI presentation of root-cause collapse.
//! Mirrors `cli.rs`'s `temp_dir`/spawn-the-binary harness: one typo'd path
//! read 3x -> JSON carries `covered` on the primary; human output appends a
//! trailing `(+2 more: …)`; the summary count is by primaries (1 error).

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn temp_dir(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("lute-cli-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

const SCENE: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
@bianca: I sense a {{run.metHelpfuly}}\n\
@bianca: again, {{run.metHelpfuly}}\n\
@bianca: still, {{run.metHelpfuly}}\n";

#[test]
fn json_carries_covered_on_the_primary() {
    let dir = temp_dir("collapse-json");
    let file = dir.join("ep.lute");
    std::fs::write(&file, SCENE).unwrap();
    let out = Command::new(BIN)
        .args(["check", file.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    let v: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(v["ok"], false);
    let diags = v["diagnostics"].as_array().expect("diagnostics array");
    let undeclared: Vec<&serde_json::Value> = diags
        .iter()
        .filter(|d| d["code"] == "E-UNDECLARED")
        .collect();
    assert_eq!(
        undeclared.len(),
        1,
        "3 reads of one typo collapse to 1 primary in JSON, got {diags:?}"
    );
    let covered = undeclared[0]["covered"]
        .as_array()
        .expect("primary must carry a `covered` array");
    assert_eq!(covered.len(), 2, "2 follower spans, got {covered:?}");
}

#[test]
fn human_output_appends_more_suffix_and_counts_by_primaries() {
    let dir = temp_dir("collapse-human");
    let file = dir.join("ep.lute");
    std::fs::write(&file, SCENE).unwrap();
    let out = Command::new(BIN)
        .args(["check", file.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("(+2 more:"),
        "human line must append a trailing (+2 more: …): {stdout}"
    );
    // §8.3: counting is by primaries -- 3 collapsed reads still say 1 error.
    assert!(
        stdout.contains("failed:") && stdout.contains("(1 error(s)"),
        "trailing count must say exactly 1 error (collapsed), got: {stdout}"
    );
}
