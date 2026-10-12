//! dsl 0.40.1 §§2–3: the runtime's read-only condition queries over a bundle
//! `lute compile --all` emits — `evaluate` of an IR `exprNode` and `def` of a
//! project def the index carries.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use lute_runtime::index::Bundle;
use lute_runtime::runtime::{Evaluation, Input, Seed, State};
use lute_runtime::{Runtime, Value};
use serde_json::{json, Value as Json};

fn project() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../lute-runtime-wasm/tests/fixtures/evaluate")
}

/// `lute compile --all` of the evaluate fixture, read back as a host reads it.
fn compiled_bundle(scratch: &str) -> Bundle {
    compiled(&project(), scratch)
}

/// `lute compile --all` of `project`, read back as a host reads it.
fn compiled(project: &Path, scratch: &str) -> Bundle {
    let scratch = lute_scratch::ScratchDir::named(scratch);
    let out = scratch.join("bundle");
    let status = Command::new(env!("CARGO_BIN_EXE_lute"))
        .args(["compile", "--all"])
        .arg(project)
        .arg("-o")
        .arg(&out)
        .output()
        .unwrap();
    assert!(status.status.success(), "{}", String::from_utf8_lossy(&status.stderr));
    let read = |path: &Path| -> Json {
        serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
    };
    let index = read(&out.join("project.index.json"));
    let artifacts: BTreeMap<String, Json> = index["documents"]
        .as_array()
        .unwrap()
        .iter()
        .map(|doc| {
            let artifact = doc["artifact"].as_str().unwrap().to_string();
            let json = read(&out.join(&artifact));
            (artifact, json)
        })
        .collect();
    Bundle::new(artifacts, index)
}

fn expr(cel: &str) -> Json {
    serde_json::to_value(lute_compile::expr::lower_expr(cel).expect("CEL in the profile")).unwrap()
}

fn value(runtime: &Runtime, state: &State, cel: &str) -> Value {
    runtime
        .evaluate(state, &expr(cel))
        .unwrap_or_else(|r| panic!("{cel}: {} {}", r.code, r.message))
        .value
}

fn def(runtime: &Runtime, state: &State, name: &str) -> Value {
    runtime
        .def(state, name)
        .unwrap_or_else(|r| panic!("{name}: {} {}", r.code, r.message))
        .value
}

/// The index carries every project def with scalar parameters, its body
/// `@def`-expanded, each parameter a `param` node (nested `@def`s
/// included), `params` in declaration order and `host` when declared; a def
/// with a list parameter stays out.
#[test]
fn the_index_carries_project_defs_expanded_with_params_and_host() {
    let bundle = compiled_bundle("evaluate-index");
    let defs = bundle.index["defs"].as_object().expect("index defs");
    assert_eq!(
        defs.keys().map(String::as_str).collect::<Vec<_>>(),
        ["atLeast", "atLeastTwice", "hallOpen", "perCount", "ready", "readyTwice", "score"]
    );
    assert_eq!(
        defs["readyTwice"]["cel"],
        "(holds('owned', ['key']) && visited('hall')) && run.count >= 2"
    );
    assert_eq!(defs["readyTwice"]["authored"], "@ready && run.count >= 2");
    assert_eq!(
        defs["atLeast"],
        json!({
            "cel": "run.count >= n",
            "expr": {"op": ">=", "l": {"path": "run.count"}, "r": {"param": "n"}},
            "params": [{"name": "n", "type": "int"}]
        })
    );
    assert_eq!(defs["atLeastTwice"]["cel"], "(run.count >= m) && run.count >= m * 2");
    assert_eq!(
        defs["atLeastTwice"]["expr"],
        json!({"op": "&&",
            "l": {"op": ">=", "l": {"path": "run.count"}, "r": {"param": "m"}},
            "r": {"op": ">=", "l": {"path": "run.count"},
                "r": {"op": "*", "l": {"param": "m"}, "r": {"int": 2}}}})
    );
    assert_eq!(defs["hallOpen"]["host"], true);
    assert!(defs["ready"].get("host").is_none() && defs["ready"].get("params").is_none());
}

#[test]
fn evaluate_reads_state_facts_visited_and_defs() {
    let runtime = Runtime::load(compiled_bundle("evaluate-values")).unwrap();
    let seed = Seed {
        facts: vec!["owned(key)".into()],
        ..Seed::default()
    };
    let (start, _) = runtime.begin(seed).unwrap();

    assert_eq!(value(&runtime, &start, "run.count == 0"), Value::Bool(true));
    assert_eq!(value(&runtime, &start, "user.flag"), Value::Bool(false));
    assert_eq!(value(&runtime, &start, "run.count + 3"), Value::Int(3));
    assert_eq!(value(&runtime, &start, "holds('owned', ['key'])"), Value::Bool(true));
    assert_eq!(value(&runtime, &start, "holds('owned', ['map'])"), Value::Bool(false));
    assert_eq!(value(&runtime, &start, "count('owned', ['_'])"), Value::Int(1));
    assert_eq!(value(&runtime, &start, "visited('hall')"), Value::Bool(false));
    assert_eq!(def(&runtime, &start, "ready"), Value::Bool(false));
    assert_eq!(def(&runtime, &start, "score"), Value::Int(1));

    let visit: Input = serde_json::from_value(json!({"type": "raiseOccasion", "occasion": "visit"}))
        .unwrap();
    let (visited, _) = runtime.step(start.clone(), visit).map_err(|(_, r)| r).unwrap();
    assert_eq!(value(&runtime, &visited, "visited('hall')"), Value::Bool(true));
    assert_eq!(def(&runtime, &visited, "ready"), Value::Bool(true));
    assert_eq!(def(&runtime, &visited, "readyTwice"), Value::Bool(false));
    assert_eq!(def(&runtime, &visited, "score"), Value::Int(11));
    assert_eq!(def(&runtime, &visited, "perCount"), Value::Int(10));
    // The earlier state is untouched.
    assert_eq!(def(&runtime, &start, "ready"), Value::Bool(false));
    assert_eq!(def(&runtime, &start, "hallOpen"), Value::Bool(true));
}

#[test]
fn unknowns_errors_and_bad_queries() {
    let runtime = Runtime::load(compiled_bundle("evaluate-errors")).unwrap();
    let (state, _) = runtime.begin(Seed::default()).unwrap();

    let unknown = runtime.evaluate(&state, &expr("run.undeclared > 1")).unwrap();
    assert_eq!(
        unknown,
        Evaluation {
            value: Value::Unknown,
            unresolved: vec!["state path `run.undeclared` has no value".into()],
        }
    );

    let error = runtime.def(&state, "perCount").unwrap_err();
    assert_eq!(error.code, "E-RUNTIME-EVAL", "{}", error.message);
    assert!(error.message.contains("division by zero"), "{}", error.message);
    let error = runtime.evaluate(&state, &expr("int('x')")).unwrap_err();
    assert_eq!(error.code, "E-RUNTIME-EVAL", "{}", error.message);

    // A def with params is not a condition on its own; a def the index
    // leaves out and an unknown name are not one.
    for name in ["atLeast", "anyOf", "missing"] {
        let r = runtime.def(&state, name).unwrap_err();
        assert_eq!(r.code, "E-RUNTIME-INPUT", "{name}: {}", r.message);
    }
    // Not exprNodes: a non-array `args`, two variants' keys on one node, a
    // malformed node nested inside valid ones — rejected, never evaluated.
    for node in [
        json!({"op": "&&", "l": {"bool": true}, "r": {"nope": 1}}),
        json!({"call": "now", "args": 42}),
        json!({"bool": true, "path": "user.flag"}),
        json!({"op": "||", "l": {"bool": false},
            "r": {"list": [{"call": "visited", "args": [{"string": "hall", "int": 1}]}]}}),
    ] {
        let r = runtime.evaluate(&state, &node).unwrap_err();
        assert_eq!(r.code, "E-RUNTIME-INPUT", "{node}: {}", r.message);
    }
}

/// Documents with different `uses:` see `gated` spelled differently (`@base`
/// against the written-out body) but expanded alike — one def, exported with
/// the first document's spelling — `split` with different bodies, and
/// `within` with one body but different parameter lists, so the index omits
/// both.
#[test]
fn defs_documents_see_differently_are_exported_by_expanded_body_and_params() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/evaluate-defs");
    let bundle = compiled(&fixture, "evaluate-defs");
    let defs = bundle.index["defs"].as_object().expect("index defs");
    assert_eq!(
        defs.keys().map(String::as_str).collect::<Vec<_>>(),
        ["atLeast", "base", "gated"]
    );
    assert_eq!(defs["gated"]["authored"], "@base");
    assert_eq!(defs["gated"]["expr"], defs["base"]["expr"]);

    let runtime = Runtime::load(bundle).unwrap();
    let (state, _) = runtime.begin(Seed::default()).unwrap();
    assert_eq!(def(&runtime, &state, "gated"), Value::Bool(false));
    for name in ["split", "within"] {
        assert_eq!(runtime.def(&state, name).unwrap_err().code, "E-RUNTIME-INPUT");
    }
}
