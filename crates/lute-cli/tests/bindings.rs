use std::path::Path;
use std::process::Command;

fn bin() -> &'static str { env!("CARGO_BIN_EXE_lute") }
fn fixture(name: &str) -> String { Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures").join(name).to_string_lossy().into_owned() }

#[test]
fn clean_fixture_has_no_binding_diagnostics() {
    let out = Command::new(bin()).args(["check-project", &fixture("bindings-clean"), "--json"]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["project_diagnostics"].as_array().unwrap().len(), 0, "{value}");
}

#[test]
fn problem_fixture_json_covers_key_file_missing_and_unused() {
    let out = Command::new(bin()).args(["check-project", &fixture("bindings-problems"), "--json"]).output().unwrap();
    assert_eq!(out.status.code(), Some(1));
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    let diagnostics = value["project_diagnostics"].as_array().unwrap();
    for code in ["E-BINDING-KEY", "E-BINDING-FILE", "W-BINDING-MISSING", "W-BINDING-UNUSED"] {
        assert!(diagnostics.iter().any(|d| d["code"] == code), "missing {code}: {value}");
    }
    let key = diagnostics.iter().find(|d| d["code"] == "E-BINDING-KEY").unwrap();
    assert!(key["path"].as_str().unwrap().ends_with("crates/lute-cli/tests/fixtures/bindings-problems/player.bindings.yaml"));
    assert!(key["span"]["line"].as_u64().unwrap() > 0);
}
