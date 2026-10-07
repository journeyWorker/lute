use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

const BIN: &str = env!("CARGO_BIN_EXE_lute");

fn artifact(tag: &str, ir: serde_json::Value) -> PathBuf {
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!("lute-run-owned-{tag}-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("artifact.json");
    std::fs::write(&path, serde_json::to_vec(&ir).unwrap()).unwrap();
    path
}

fn run(ir: serde_json::Value) -> std::process::Output {
    let path = artifact("test", ir);
    Command::new(BIN).args(["run", path.to_str().unwrap()]).output().unwrap()
}

fn base() -> serde_json::Value {
    serde_json::json!({"irVersion":"0.37.0", "kind":"scene", "requiredSemantics":["lute.core/1"], "commands":[]})
}

#[test]
fn run_refuses_engine_owned_state_write_before_execution() {
    let mut ir = base();
    ir["state"] = serde_json::json!([{"path":"clock.day", "owner":"engine"}]);
    ir["commands"] = serde_json::json!([{"kind":"set", "position":"set", "path":"clock.day", "op":"=", "value":"1"}]);
    let out = run(ir);
    assert_eq!(out.status.code(), Some(1));
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(error.contains("E-RUN-OWNED-WRITE"), "{error}");
}

#[test]
fn run_refuses_reserved_relation_assert_before_execution() {
    let mut ir = base();
    ir["requiredSemantics"] = serde_json::json!(["lute.core/1", "lute.knowledge.facts/1"]);
    ir["relations"] = serde_json::json!([{"name":"engineFact", "reserved":true}]);
    ir["commands"] = serde_json::json!([{"kind":"assert", "position":"assert", "relation":"engineFact", "args":[]}]);
    let out = run(ir);
    assert_eq!(out.status.code(), Some(1));
    let error = String::from_utf8_lossy(&out.stderr);
    assert!(error.contains("E-RUN-OWNED-WRITE"), "{error}");
}
