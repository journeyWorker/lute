//! Spec 0.53.0 §3.10: a command reconciles each project root once,
//! assembles its scenario once and computes its reach fixpoint once; every
//! report reads the model's results instead of recomputing them.
//!
//! The witness is `LUTE_STATS=project`, one stderr line at exit:
//! `lute: stats: project reconciliations=<n> scenarios=<n> fixpoints=<n>`.

use std::path::{Path, PathBuf};
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn examples() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples")
}

/// `lute args…` with the counter on: its exit code and the counts.
fn passes(args: &[&str]) -> (Option<i32>, String) {
    let out = Command::new(BIN)
        .args(args)
        .env("LUTE_STATS", "project")
        .env("LUTE_TELEMETRY", "0")
        .output()
        .unwrap();
    let stderr = String::from_utf8_lossy(&out.stderr);
    let counts = stderr
        .lines()
        .find_map(|l| l.strip_prefix("lute: stats: project "))
        .unwrap_or_else(|| panic!("no stats line for {args:?}: {stderr}"))
        .to_string();
    (out.status.code(), counts)
}

fn once_per_root(roots: usize) -> String {
    format!("reconciliations={roots} scenarios={roots} fixpoints={roots}")
}

/// Every project-wide report on one root: one of each pass. Before 0.53 the
/// graph, `--json`, `--format dot`, `reach --node` and `envelope` views
/// assembled the scenario again, `beats` and `--endings` reconciled again,
/// and every model computed the fixpoint twice.
#[test]
fn every_report_runs_each_pass_once_per_root() {
    let p = examples().join("games/lantern-academy");
    let p = p.to_str().unwrap();
    let scene = format!("{p}/scenes/ren/confession.lute");
    let commands: Vec<Vec<&str>> = vec![
        vec!["check", "--project", p],
        vec!["scenario", "--project", p],
        vec!["scenario", "--json", "--project", p],
        vec!["scenario", "--format", "dot", "--project", p],
        vec![
            "scenario",
            "reach",
            "--project",
            p,
            "--node",
            "quest:fourLanterns",
        ],
        vec![
            "scenario",
            "reach",
            "--json",
            "--project",
            p,
            "--node",
            "quest:fourLanterns",
        ],
        vec![
            "scenario",
            "reach",
            "--project",
            p,
            "--node",
            "fourLanterns",
        ],
        vec![
            "scenario",
            "envelope",
            "--project",
            p,
            "--node",
            "quest:fourLanterns",
        ],
        vec![
            "scenario",
            "envelope",
            "--json",
            "--project",
            p,
            "--node",
            "quest:fourLanterns",
        ],
        vec!["scenario", "knowledge", "--project", p],
        vec!["scenario", "reach", "--endings", "--project", p],
        vec!["beats", "--project", p],
        vec!["constraints", "--project", p],
        vec!["impact", "--node", "quest:fourLanterns", "--project", p],
        vec!["trace", &scene, "--project", p],
    ];
    for args in &commands {
        let (code, counts) = passes(args);
        assert_eq!(code, Some(0), "{args:?}");
        assert_eq!(counts, once_per_root(1), "{args:?}");
    }
}

/// `lute test` over a project with tests (foreign quests, mocked facts) and
/// expectation plays: one model for the run. Before 0.53 each test naming a
/// quest another document declares built its own model, and the plays
/// compiled a second one.
#[test]
fn test_runs_each_pass_once_per_root() {
    let p = examples().join("games/lantern-academy");
    let (code, counts) = passes(&["test", "--project", p.to_str().unwrap()]);
    assert_eq!(code, Some(0));
    assert_eq!(counts, once_per_root(1));
}

/// `lute trace` whose report consults the project beyond the gate: an
/// accepted quest (the project's quest ids), a quest another document
/// declares (settled against them) and a mocked fact (the project's
/// producers). One model for the gate and every lookup. Before, the gate's
/// analysis model and the lookups' compiled model were two builds.
#[test]
fn trace_lookups_share_the_gate_model() {
    let p = examples().join("games/lantern-academy");
    let p = p.to_str().unwrap();
    let exam = format!("{p}/scenes/common/exam.lute");
    let commands: Vec<Vec<&str>> = vec![
        vec![
            "trace",
            &exam,
            "--project",
            p,
            "--accepted-quest",
            "studyGroup",
        ],
        vec!["trace", &exam, "--project", p],
        vec!["trace", &exam, "--project", p, "--fact", "locked(kai)"],
    ];
    for args in &commands {
        let (_, counts) = passes(args);
        assert_eq!(counts, once_per_root(1), "{args:?}");
    }
}

/// Over a walk of many roots the reports run each pass once per root:
/// exactly as many as `check --project` does.
#[test]
fn a_walk_runs_each_pass_once_per_root() {
    let dir = examples();
    let dir = dir.to_str().unwrap();
    let (_, check) = passes(&["check", "--project", dir]);
    let roots: usize = check
        .split_once(' ')
        .and_then(|(r, _)| r.strip_prefix("reconciliations="))
        .unwrap()
        .parse()
        .unwrap();
    assert!(roots > 1, "{check}");
    assert_eq!(check, once_per_root(roots));
    for args in [
        vec!["scenario", "--project", dir],
        vec!["scenario", "--json", "--project", dir],
        vec!["scenario", "--format", "dot", "--project", dir],
        vec!["scenario", "reach", "--endings", "--project", dir],
        vec!["beats", "--project", dir],
    ] {
        let (_, counts) = passes(&args);
        assert_eq!(counts, once_per_root(roots), "{args:?}");
    }
}
