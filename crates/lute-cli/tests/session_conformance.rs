//! Session conformance (spec 0.38.0 §12): every `conformance/session/<case>`
//! — `lute play --events` reproduces `expected.jsonl`, and replaying
//! `inputs.jsonl` through the runtime library reproduces every output
//! byte-for-byte.

use std::path::{Path, PathBuf};
use std::process::Command;

use lute_cli::events;
use lute_runtime::runtime::{Input, Seed};
use serde_json::Value;

fn cases() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/session");
    let mut out: Vec<PathBuf> = std::fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|case| case.join("project/script.play.yaml").is_file())
        .collect();
    out.sort();
    out
}

/// The stream's serialization: through `serde_json::Value`, whose maps sort
/// their keys.
fn canonical(output: &lute_runtime::runtime::Output) -> String {
    serde_json::to_value(output).unwrap().to_string()
}

fn lines(path: &Path) -> Vec<Value> {
    std::fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}

#[test]
fn session_event_streams_and_runtime_replay_match() {
    let cases = cases();
    assert!(cases.len() >= 10, "session cases missing");
    for case in cases {
        let (project, script) = (case.join("project"), case.join("project/script.play.yaml"));
        let expected = std::fs::read_to_string(case.join("expected.jsonl")).unwrap();
        let got = Command::new(env!("CARGO_BIN_EXE_lute"))
            .arg("play")
            .arg(&project)
            .arg("--script")
            .arg(&script)
            .arg("--events")
            .output()
            .unwrap();
        assert_eq!(
            got.status.code(),
            Some(0),
            "{}: {}",
            case.display(),
            String::from_utf8_lossy(&got.stderr)
        );
        assert_eq!(
            String::from_utf8(got.stdout).unwrap(),
            expected,
            "{}: CLI stream",
            case.display()
        );

        let runtime = events::script_run(&project, &script, false)
            .unwrap()
            .runtime;
        let inputs = lines(&case.join("inputs.jsonl"));
        let outputs: Vec<String> = lines(&case.join("expected.jsonl"))
            .iter()
            .map(|line| serde_json::to_string(&line["output"]).unwrap())
            .collect();
        assert_eq!(
            inputs.len(),
            outputs.len(),
            "{}: inputs.jsonl and expected.jsonl lengths",
            case.display()
        );
        let seed: Seed = serde_json::from_value(inputs[0]["seed"].clone()).unwrap();
        let (mut state, output) = runtime.begin(seed).unwrap();
        assert_eq!(canonical(&output), outputs[0], "{}: begin", case.display());
        for (k, line) in inputs.iter().enumerate().skip(1) {
            let input: Input = serde_json::from_value(line["input"].clone()).unwrap();
            let (next, output) = runtime.step(state, input).unwrap_or_else(|(_, r)| {
                panic!("{} input {k}: {} {}", case.display(), r.code, r.message)
            });
            assert_eq!(
                canonical(&output),
                outputs[k],
                "{}: input {k}",
                case.display()
            );
            state = next;
        }
    }
}
