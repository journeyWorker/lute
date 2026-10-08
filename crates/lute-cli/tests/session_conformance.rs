//! Session conformance (spec 0.38.0 §12): every `conformance/session/<case>`
//! — `lute play --events` reproduces `expected.jsonl`, and a runtime loaded
//! from the case's `lute compile --all` output, replaying `inputs.jsonl`,
//! reproduces every output byte-for-byte.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use lute_runtime::index::Bundle;
use lute_runtime::runtime::{Input, Seed};
use lute_runtime::Runtime;
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

fn lute() -> Command {
    Command::new(env!("CARGO_BIN_EXE_lute"))
}

/// `lute compile --all <project>` written to a scratch directory and read
/// back as a host reads it: `project.index.json` plus every artifact by its
/// index path.
fn compiled_bundle(project: &Path, scratch: &str) -> Bundle {
    let out = std::env::temp_dir().join(format!("lute-session-{}-{scratch}", std::process::id()));
    let _ = std::fs::remove_dir_all(&out);
    let status = lute()
        .args(["compile", "--all"])
        .arg(project)
        .arg("-o")
        .arg(&out)
        .output()
        .unwrap();
    assert!(
        status.status.success(),
        "{}: {}",
        project.display(),
        String::from_utf8_lossy(&status.stderr)
    );
    let read = |path: &Path| -> Value {
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    };
    let index = read(&out.join("project.index.json"));
    let artifacts: BTreeMap<String, Value> = index["documents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|doc| {
            let artifact = doc["artifact"].as_str().unwrap().to_string();
            let json = read(&out.join(&artifact));
            (artifact, json)
        })
        .collect();
    std::fs::remove_dir_all(&out).unwrap();
    Bundle::new(artifacts, index)
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
        let name = case.file_name().unwrap().to_string_lossy().to_string();
        let (project, script) = (case.join("project"), case.join("project/script.play.yaml"));
        let expected = std::fs::read_to_string(case.join("expected.jsonl")).unwrap();
        let got = lute()
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
            "{name}: {}",
            String::from_utf8_lossy(&got.stderr)
        );
        assert_eq!(
            String::from_utf8(got.stdout).unwrap(),
            expected,
            "{name}: CLI stream"
        );

        let runtime = Runtime::load(compiled_bundle(&project, &name))
            .unwrap_or_else(|r| panic!("{name}: {} {}", r.code, r.message));
        let inputs = lines(&case.join("inputs.jsonl"));
        let outputs: Vec<String> = lines(&case.join("expected.jsonl"))
            .iter()
            .map(|line| line["output"].to_string())
            .collect();
        assert_eq!(
            inputs.len(),
            outputs.len(),
            "{name}: inputs.jsonl and expected.jsonl lengths"
        );
        let seed: Seed = serde_json::from_value(inputs[0]["seed"].clone()).unwrap();
        let (mut state, output) = runtime.begin(seed).unwrap();
        assert_eq!(canonical(&output), outputs[0], "{name}: begin");
        for (k, line) in inputs.iter().enumerate().skip(1) {
            let input: Input = serde_json::from_value(line["input"].clone()).unwrap();
            let (next, output) = runtime
                .step(state, input)
                .unwrap_or_else(|(_, r)| panic!("{name} input {k}: {} {}", r.code, r.message));
            assert_eq!(canonical(&output), outputs[k], "{name}: input {k}");
            state = next;
        }
    }
}

/// §4.1: load refuses another IR major.minor and a command kind it does not
/// execute.
#[test]
fn load_refuses_another_ir_line_and_unknown_kinds() {
    let project =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../conformance/session/hub-once/project");
    let bundle = compiled_bundle(&project, "gate");
    assert!(Runtime::load(bundle.clone()).is_ok());

    let mut older = bundle.clone();
    older.index["irVersion"] = Value::String("0.37.0".into());
    let r = Runtime::load(older).err().expect("an IR 0.37 index loaded");
    assert_eq!(r.code, "E-RUNTIME-IR-VERSION", "{}", r.message);

    let mut newer = bundle;
    let artifact = newer.artifacts.values_mut().next().unwrap();
    artifact["commands"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({ "kind": "hologram" }));
    let r = Runtime::load(newer)
        .err()
        .expect("an unknown command kind loaded");
    assert_eq!(r.code, "E-RUNTIME-IR-VERSION", "{}", r.message);
    assert!(r.message.contains("hologram"), "{}", r.message);
}

/// The runtime gates on the IR the compiler stamps.
#[test]
fn the_runtime_executes_the_ir_the_compiler_emits() {
    assert_eq!(
        lute_runtime::runtime::IR_VERSION,
        lute_compile::LUTE_IR_VERSION
    );
}
