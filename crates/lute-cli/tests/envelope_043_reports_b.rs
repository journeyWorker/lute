//! dsl 0.43.0 §3–§4 (#89, #90): the reports of trace, play, test, scenario,
//! beats, refs, lore, loc report and doctor carry the envelope; run, play
//! --events and loc export stay unwrapped; failures in JSON mode print the
//! error envelope with the human message unchanged on stderr.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn lute(args: &[&str]) -> Output {
    Command::new(BIN)
        .current_dir(root())
        .args(args)
        .output()
        .unwrap()
}

fn json(output: &Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|e| {
        panic!(
            "{e}: {}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    })
}

/// A report: the envelope's header, `ok` agreeing with the exit code.
fn report(args: &[&str], command: &str, version: &str) -> Value {
    let output = lute(args);
    let value = json(&output);
    assert_eq!(value["command"], command, "{args:?}");
    assert_eq!(value["schemaVersion"], version, "{args:?}");
    assert_eq!(
        value["ok"],
        output.status.code() == Some(0),
        "{args:?}: {value}"
    );
    value
}

#[test]
fn reports_carry_the_envelope() {
    let arcia = "docs/examples/arcia-project";
    report(&["beats", arcia, "--json"], "beats", "0.43.0.beats");
    report(
        &["refs", arcia, "--reward", "ITEM", "--json"],
        "refs",
        "0.43.0.refs",
    );
    report(&["lore", "docs/examples", "--json"], "lore", "0.43.0.lore");
    report(
        &["loc", "report", arcia, "--json"],
        "loc report",
        "0.43.0.loc-report",
    );
    report(&["test", arcia, "--json"], "test", "0.43.0.test");
    report(
        &["scenario", arcia, "--format", "json"],
        "scenario",
        "0.43.0.scenario",
    );
    let doctor = report(
        &["doctor", "docs/examples", "--json"],
        "doctor",
        "0.43.0.doctor",
    );
    assert!(doctor["checks"].is_object(), "doctor keeps {{dir, checks}}");
    assert_eq!(
        doctor["checks"]["telemetry"]["detail"],
        "off (LUTE_TELEMETRY)"
    );
    let league = "docs/examples/games/monster-league";
    let play = report(
        &[
            "play",
            league,
            "--script",
            &format!("{league}/plays/north.play.yaml"),
            "--json",
        ],
        "play",
        "0.43.0.play",
    );
    assert!(play["steps"].is_array());
    report(
        &[
            "trace",
            "conformance/choice-basic/source.lute",
            "--mock",
            "conformance/choice-basic/mock.yaml",
            "--json",
        ],
        "trace",
        "0.43.0.trace",
    );
}

#[test]
fn a_trace_refusal_lists_its_diagnostics_in_the_envelope() {
    let dir = std::env::temp_dir().join(format!("lute-043-trace-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("broken.lute");
    std::fs::write(
        &file,
        "---\nkind: scene\nid: broken\n---\n## A\n::set{run.nowhere = 1}\n",
    )
    .unwrap();
    let output = lute(&["trace", file.to_str().unwrap(), "--json"]);
    assert_eq!(output.status.code(), Some(1));
    let value = json(&output);
    assert_eq!(value["schemaVersion"], "0.43.0.trace");
    assert_eq!(value["ok"], false);
    assert!(
        !value["diagnostics"].as_array().unwrap().is_empty(),
        "{value}"
    );
}

#[test]
fn artifacts_and_streams_stay_unwrapped() {
    let run = json(&lute(&[
        "run",
        "conformance/choice-basic/artifact.json",
        "--mock",
        "conformance/choice-basic/mock.yaml",
        "--json",
    ]));
    assert!(
        run.get("command").is_none() && run.get("schemaVersion").is_none(),
        "{run}"
    );
    assert!(run["irVersion"].is_string());

    let rows = json(&lute(&["loc", "export", "docs/examples/arcia-project"]));
    assert!(rows.is_array());

    let session = "conformance/session/hub-once/project";
    let events = lute(&[
        "play",
        session,
        "--script",
        &format!("{session}/script.play.yaml"),
        "--events",
    ]);
    assert!(events.status.success());
    for line in String::from_utf8(events.stdout).unwrap().lines() {
        let event: Value = serde_json::from_str(line).unwrap();
        assert!(event.get("schemaVersion").is_none() && event.get("command").is_none());
    }
}

/// A JSON-mode failure: the error envelope on stdout, the same human text on
/// stderr as without `--json`, exit 2.
fn error(args: &[&str], command: &str, code: &str) -> Value {
    let output = lute(args);
    assert_eq!(output.status.code(), Some(2), "{args:?}");
    let value = json(&output);
    assert_eq!(value["schemaVersion"], "0.43.0.error", "{args:?}");
    assert_eq!(value["ok"], false);
    assert_eq!(value["command"], command);
    assert_eq!(value["error"]["code"], code, "{args:?}: {value}");
    let human: Vec<&str> = args
        .iter()
        .copied()
        .filter(|a| *a != "--json" && *a != "json" && *a != "--format")
        .collect();
    let plain = lute(&human);
    assert_eq!(plain.status.code(), Some(2), "{human:?}");
    assert_eq!(
        plain.stderr, output.stderr,
        "stderr is unchanged by JSON mode: {args:?}"
    );
    assert!(plain.stdout.is_empty(), "{human:?}");
    value
}

#[test]
fn failures_in_json_mode_are_error_envelopes() {
    let missing = "/no/such/lute-dir";
    for (args, command) in [
        (vec!["refs", missing, "--reward", "ITEM", "--json"], "refs"),
        (vec!["lore", missing, "--json"], "lore"),
        (vec!["loc", "report", missing, "--json"], "loc report"),
        (vec!["check-project", missing, "--json"], "check-project"),
        (vec!["doctor", missing, "--json"], "doctor"),
        (vec!["compile", "--json", "/no/such.lute"], "compile"),
        (vec!["trace", "/no/such.lute", "--json"], "trace"),
    ] {
        let value = error(&args, command, "E-CLI-IO");
        assert!(
            value["error"]["path"]
                .as_str()
                .unwrap()
                .starts_with("/no/such"),
            "{value}"
        );
    }
    let usage = error(
        &[
            "beats",
            "docs/examples/arcia-project",
            "--occasion",
            "nope",
            "--json",
        ],
        "beats",
        "E-CLI-USAGE",
    );
    assert_eq!(usage["error"]["argument"], "--occasion");
    let usage = error(
        &["compile", "--all", "--json", "docs/examples/arcia-project"],
        "compile",
        "E-CLI-USAGE",
    );
    assert!(usage["error"]["argument"].is_string());
    error(
        &[
            "scenario",
            "docs/examples/arcia-project",
            "reach",
            "scene:nope",
            "--format",
            "json",
        ],
        "scenario reach",
        "E-CLI-USAGE",
    );
}
