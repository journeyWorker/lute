use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn temporary(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("lute-bench-{name}-{}", std::process::id()))
}

#[test]
fn binary_emits_schema_and_200ms_sample() {
    let output = temporary("report.json");
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let status = Command::new(env!("CARGO_BIN_EXE_lute-bench"))
        .args([
            "--root",
            root.to_str().unwrap(),
            "--tier",
            "tiny",
            "--phase",
            "cold-load",
            "--samples",
            "1",
            "--json",
            output.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(status.success());
    let report: serde_json::Value = serde_json::from_str(&fs::read_to_string(&output).unwrap()).unwrap();
    assert_eq!(report["schemaVersion"], "0.36.0.bench");
    let sample = &report["samples"][0];
    assert_eq!(sample["exit"], "ok");
    assert!(sample["iterations"].as_u64().unwrap() > 0);
    assert!(sample["elapsedMs"].as_f64().unwrap() >= 200.0);
    fs::remove_file(output).unwrap();
}

fn fixture(path: &PathBuf, value: f64) {
    let report = serde_json::json!({
        "schemaVersion": "0.36.0.bench",
        "runner": {"os": "test", "arch": "test", "cpu": "test"},
        "corpus": {"tiny": {"project": "ledger", "projectRevision": "rev"}},
        "samples": [{
            "tier": "tiny", "project": "ledger", "phase": "analysis", "sample": 0,
            "testCount": 1, "iterations": 1, "elapsedMs": 200.0,
            "perIterationUs": value, "exit": "ok"
        }]
    });
    fs::write(path, serde_json::to_vec(&report).unwrap()).unwrap();
}

#[test]
fn comparator_accepts_110_and_rejects_111() {
    let base = temporary("base.json");
    let head = temporary("head.json");
    fixture(&base, 100.0);
    fixture(&head, 110.0);
    let script = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../scripts/compare-bench.py");
    let accepted = Command::new("python3")
        .args([script.to_str().unwrap(), base.to_str().unwrap(), head.to_str().unwrap()])
        .status()
        .unwrap();
    assert!(accepted.success());
    fixture(&head, 111.0);
    let rejected = Command::new("python3")
        .args([script.to_str().unwrap(), base.to_str().unwrap(), head.to_str().unwrap()])
        .status()
        .unwrap();
    assert_eq!(rejected.code(), Some(1));
    fs::remove_file(base).unwrap();
    fs::remove_file(head).unwrap();
}
