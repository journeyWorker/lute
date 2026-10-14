//! dsl 0.43.0 §6 (#92): local, opt-out telemetry — one record per
//! invocation in the state directory, nothing but the specified fields, off
//! under `LUTE_TELEMETRY=0` / `DO_NOT_TRACK`, and never visible in the
//! command's output or exit code.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn state_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("lute-telemetry-043-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// `lute args` with telemetry on and its state under `dir`.
fn lute_recording(dir: &Path, args: &[&str], extra: &[(&str, &str)]) -> Output {
    let mut command = Command::new(BIN);
    command
        .args(args)
        .env_remove("LUTE_TELEMETRY")
        .env_remove("DO_NOT_TRACK")
        .env("XDG_STATE_HOME", dir)
        .env("HOME", dir)
        .env("LOCALAPPDATA", dir);
    for (key, value) in extra {
        command.env(key, value);
    }
    command.output().unwrap()
}

fn records(dir: &Path) -> Vec<Value> {
    let Ok(text) = std::fs::read_to_string(dir.join("lute").join("telemetry.jsonl")) else {
        return Vec::new();
    };
    text.lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect()
}

#[test]
fn the_repository_runs_its_own_binaries_opted_out() {
    // `.cargo/config.toml` `[env]` reaches every test process (and so every
    // `lute` it spawns) under both `cargo test` and `cargo nextest`.
    assert_eq!(std::env::var("LUTE_TELEMETRY").as_deref(), Ok("0"));
    let dir = state_dir("inherited");
    let out = Command::new(BIN)
        .args(["version"])
        .env("XDG_STATE_HOME", &dir)
        .env("HOME", &dir)
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(records(&dir).is_empty());
}

#[test]
fn each_invocation_appends_one_record_of_the_specified_fields() {
    let dir = state_dir("record");
    let without = Command::new(BIN)
        .args(["version", "--json"])
        .output()
        .unwrap();
    let with = lute_recording(&dir, &["version", "--json"], &[]);
    assert_eq!(with.stdout, without.stdout, "recording changes no output");
    assert_eq!(with.stderr, without.stderr);
    let missing = lute_recording(&dir, &["check", "--json", "no/such/file.lute"], &[]);
    assert_eq!(missing.status.code(), Some(2));
    let typo = lute_recording(&dir, &["chek"], &[]);
    assert_eq!(typo.status.code(), Some(2));
    let nested = lute_recording(
        &dir,
        &[
            "scenario",
            "no/such/dir",
            "reach",
            "scene:x",
            "--format",
            "json",
        ],
        &[],
    );
    assert_eq!(nested.status.code(), Some(2));

    let all = records(&dir);
    assert_eq!(all.len(), 4, "{all:?}");
    for record in &all {
        let mut keys: Vec<&str> = record
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            ["command", "durationMs", "exit", "json", "lute", "ts", "v"]
        );
        assert_eq!(record["v"], 1);
        assert_eq!(record["lute"], env!("CARGO_PKG_VERSION"));
        assert!(record["ts"].as_u64().unwrap() > 0);
        assert!(record["durationMs"].is_u64());
    }
    assert_eq!(
        (&all[0]["command"], &all[0]["json"], &all[0]["exit"]),
        (&"version".into(), &true.into(), &0.into())
    );
    assert_eq!(
        (&all[1]["command"], &all[1]["json"], &all[1]["exit"]),
        (&"check".into(), &true.into(), &2.into())
    );
    assert_eq!(
        (&all[2]["command"], &all[2]["exit"]),
        (&Value::Null, &2.into())
    );
    assert_eq!(all[3]["command"], "scenario reach");
    let text = std::fs::read_to_string(dir.join("lute").join("telemetry.jsonl")).unwrap();
    assert!(
        !text.contains("no/such"),
        "no argument value is recorded: {text}"
    );
}

#[test]
fn opt_out_variables_record_nothing() {
    for env in [
        ("LUTE_TELEMETRY", "0"),
        ("LUTE_TELEMETRY", "off"),
        ("LUTE_TELEMETRY", "False"),
        ("DO_NOT_TRACK", "1"),
    ] {
        let dir = state_dir(&format!("optout-{}-{}", env.0, env.1));
        assert!(lute_recording(&dir, &["version"], &[env]).status.success());
        assert!(records(&dir).is_empty(), "{env:?}");
    }
}

#[test]
fn an_unwritable_state_directory_changes_nothing() {
    let dir = state_dir("unwritable");
    std::fs::write(dir.join("lute"), "a file where the directory would go").unwrap();
    let without = Command::new(BIN).args(["version"]).output().unwrap();
    let with = lute_recording(&dir, &["version"], &[]);
    assert_eq!(with.status.code(), without.status.code());
    assert_eq!(with.stdout, without.stdout);
    assert_eq!(with.stderr, without.stderr);
}

#[test]
fn concurrent_invocations_never_interleave_a_line() {
    let dir = state_dir("concurrent");
    let children: Vec<_> = (0..16)
        .map(|_| {
            Command::new(BIN)
                .args(["version", "--json"])
                .env_remove("LUTE_TELEMETRY")
                .env_remove("DO_NOT_TRACK")
                .env("XDG_STATE_HOME", &dir)
                .env("HOME", &dir)
                .spawn()
                .unwrap()
        })
        .collect();
    for mut child in children {
        assert!(child.wait().unwrap().success());
    }
    assert_eq!(records(&dir).len(), 16);
}
