//! Expr-eval conformance suite (IR addendum A7).
//!
//! The compiled artifact carries a portable `expr` AST on every CEL slot so an
//! engine ships NO CEL parser (see `lute_compile::expr`). This suite is the
//! **durable, portable conformance contract** for that closed Lute-CEL profile:
//! a set of self-describing fixtures — `expr` tree + a flat `state` snapshot →
//! `expected` value — and a minimal reference tree-walk evaluator that proves
//! them self-consistent. Any runtime evaluator (Plan E's pure-Dart tree-walker,
//! an FFI-wrapped Rust one, …) that passes this fixture set conforms.
//!
//! Two invariants are asserted per fixture:
//!   1. **Semantics.** The reference evaluator walks the *raw* `expr` JSON
//!      (`serde_json::Value`), exactly as a runtime SDK consuming the wire
//!      artifact would — NOT via the `Serialize`-only `ExprNode` enum — and its
//!      result equals `expected`.
//!   2. **Compiler agreement** (fixtures with a `cel` source only). The
//!      compiler emits precisely this tree:
//!      `serde_json::to_value(lower_expr(cel).unwrap()) == expr`.
//!      `<when is>`-synthesized shapes (C2 `synth_arm_expr`) have no `cel`
//!      source and are hand-written; they skip invariant 2.
//!
//! Numeric model: all numbers are f64 (Lute-CEL double model). Literals and
//! `state` numbers alike are compared as f64.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{Map, Number, Value};

/// One conformance fixture. Unknown fields (e.g. a human-readable `desc`) are
/// intentionally ignored so files can carry documentation.
#[derive(Debug, Deserialize)]
struct Fixture {
    /// Stable case name (used in failure messages).
    name: String,
    /// Optional source CEL: when present, the compiler cross-check runs.
    #[serde(default)]
    cel: Option<String>,
    /// The portable `expr` AST under test (raw wire JSON).
    expr: Value,
    /// Flat map of dotted path -> JSON value. `has`/`isSet` are true iff the
    /// key is present; all numbers are read as f64.
    state: Map<String, Value>,
    /// The value the `expr` must evaluate to under `state`.
    expected: Value,
}

/// Reference tree-walk evaluator for the closed Lute-CEL profile. Walks the raw
/// `expr` JSON — the wire contract an engine consumes. Total and deterministic;
/// any shape outside the profile panics with a descriptive message (proving the
/// fixtures never drift out of the profile).
fn eval(expr: &Value, state: &Map<String, Value>) -> Value {
    let obj = expr
        .as_object()
        .unwrap_or_else(|| panic!("expr node must be a JSON object, got: {expr}"));

    if let Some(v) = obj.get("lit") {
        return v.clone();
    }
    if let Some(p) = obj.get("path") {
        let key = as_key(p, "path");
        return state
            .get(key)
            .cloned()
            .unwrap_or_else(|| panic!("state is missing referenced path '{key}'"));
    }
    if let Some(p) = obj.get("has") {
        return Value::Bool(state.contains_key(as_key(p, "has")));
    }
    if let Some(p) = obj.get("isSet") {
        return Value::Bool(state.contains_key(as_key(p, "isSet")));
    }
    if let Some(list) = obj.get("list") {
        let elems = list
            .as_array()
            .unwrap_or_else(|| panic!("`list` must be a JSON array, got: {list}"));
        return Value::Array(elems.iter().map(|e| eval(e, state)).collect());
    }
    if let Some(cond) = obj.get("cond") {
        let taken = if as_bool(&eval(cond, state)) {
            obj.get("then").expect("`cond` node missing `then`")
        } else {
            obj.get("else").expect("`cond` node missing `else`")
        };
        return eval(taken, state);
    }
    if let Some(op) = obj.get("op") {
        let op = op
            .as_str()
            .unwrap_or_else(|| panic!("`op` must be a string, got: {op}"));
        let l = obj.get("l").expect("operator node missing `l`");
        let has_r = obj.contains_key("r");

        // Unary operators carry only `l`.
        if op == "!" && !has_r {
            return Value::Bool(!as_bool(&eval(l, state)));
        }
        if op == "-" && !has_r {
            return number(-as_f64(&eval(l, state)));
        }

        // Boolean binaries short-circuit before evaluating the right operand.
        if op == "&&" {
            if !as_bool(&eval(l, state)) {
                return Value::Bool(false);
            }
            let r = obj.get("r").expect("`&&` node missing `r`");
            return Value::Bool(as_bool(&eval(r, state)));
        }
        if op == "||" {
            if as_bool(&eval(l, state)) {
                return Value::Bool(true);
            }
            let r = obj.get("r").expect("`||` node missing `r`");
            return Value::Bool(as_bool(&eval(r, state)));
        }

        let r = obj.get("r").expect("binary operator node missing `r`");
        let a = eval(l, state);
        let b = eval(r, state);
        return match op {
            "==" => Value::Bool(json_eq(&a, &b)),
            "!=" => Value::Bool(!json_eq(&a, &b)),
            "<" => Value::Bool(as_f64(&a) < as_f64(&b)),
            "<=" => Value::Bool(as_f64(&a) <= as_f64(&b)),
            ">" => Value::Bool(as_f64(&a) > as_f64(&b)),
            ">=" => Value::Bool(as_f64(&a) >= as_f64(&b)),
            "+" => number(as_f64(&a) + as_f64(&b)),
            "-" => number(as_f64(&a) - as_f64(&b)),
            "*" => number(as_f64(&a) * as_f64(&b)),
            "/" => number(as_f64(&a) / as_f64(&b)),
            "in" => {
                let elems = b
                    .as_array()
                    .unwrap_or_else(|| panic!("`in` right operand must be a list, got: {b}"));
                Value::Bool(elems.iter().any(|e| json_eq(e, &a)))
            }
            other => panic!("operator '{other}' is outside the closed Lute-CEL profile"),
        };
    }

    panic!("expr node is outside the closed Lute-CEL profile: {expr}");
}

/// A `path`/`has`/`isSet` payload must be a string key.
fn as_key<'a>(v: &'a Value, kind: &str) -> &'a str {
    v.as_str()
        .unwrap_or_else(|| panic!("`{kind}` payload must be a string, got: {v}"))
}

/// Coerce to bool or panic — the profile has no truthiness beyond real bools.
fn as_bool(v: &Value) -> bool {
    v.as_bool()
        .unwrap_or_else(|| panic!("expected a boolean value, got: {v}"))
}

/// Coerce any JSON number (int or float) to f64 — the Lute-CEL double model.
fn as_f64(v: &Value) -> f64 {
    v.as_f64()
        .unwrap_or_else(|| panic!("expected a numeric value, got: {v}"))
}

/// Wrap an f64 result as a JSON number.
fn number(x: f64) -> Value {
    Value::Number(Number::from_f64(x).expect("arithmetic produced a non-finite number"))
}

/// Equality across the profile: numbers compare as f64 (so state `5` == lit
/// `5.0`); everything else is structural JSON equality.
fn json_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(_), Value::Number(_)) => a.as_f64() == b.as_f64(),
        _ => a == b,
    }
}

/// Directory holding the conformance fixtures, resolved from the crate root so
/// the suite is independent of the test's working directory.
fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/expr_eval")
}

/// Load every `*.json` fixture, sorted by file name for determinism.
fn load_fixtures() -> Vec<(String, Fixture)> {
    let dir = fixtures_dir();
    let mut entries: BTreeMap<String, Fixture> = BTreeMap::new();
    for entry in fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read fixtures dir {}: {e}", dir.display()))
    {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let file = path.file_name().unwrap().to_string_lossy().into_owned();
        let raw = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("cannot read fixture {}: {e}", path.display()));
        let fx: Fixture = serde_json::from_str(&raw)
            .unwrap_or_else(|e| panic!("cannot parse fixture {file}: {e}"));
        entries.insert(file, fx);
    }
    entries.into_iter().collect()
}

#[test]
fn reference_evaluator_matches_every_fixture() {
    let fixtures = load_fixtures();
    assert!(
        !fixtures.is_empty(),
        "no expr-eval fixtures found in {} — the conformance suite would be a no-op",
        fixtures_dir().display()
    );

    for (file, fx) in &fixtures {
        let got = eval(&fx.expr, &fx.state);
        assert!(
            json_eq(&got, &fx.expected),
            "fixture {file} ({}): eval mismatch\n  expr:     {}\n  state:    {}\n  expected: {}\n  got:      {}",
            fx.name,
            fx.expr,
            Value::Object(fx.state.clone()),
            fx.expected,
            got,
        );
    }

    eprintln!("expr-eval conformance: {} fixtures passed", fixtures.len());
}

#[test]
fn compiler_emits_the_fixture_tree_for_cel_sources() {
    let fixtures = load_fixtures();
    let mut checked = 0usize;

    for (file, fx) in &fixtures {
        let Some(cel) = &fx.cel else {
            continue;
        };
        let node = lute_compile::expr::lower_expr(cel).unwrap_or_else(|| {
            panic!("fixture {file} ({}): lower_expr returned None for cel {cel:?}", fx.name)
        });
        let lowered = serde_json::to_value(&node).expect("serialize lowered expr");
        assert_eq!(
            lowered, fx.expr,
            "fixture {file} ({}): compiler-emitted expr tree does not match the fixture\n  cel: {cel:?}",
            fx.name,
        );
        checked += 1;
    }

    assert!(
        checked > 0,
        "no `cel`-bearing fixtures cross-checked — the compiler-agreement invariant would be a no-op"
    );
    eprintln!("expr-eval conformance: {checked} cel-bearing fixtures cross-checked against lower_expr");
}
