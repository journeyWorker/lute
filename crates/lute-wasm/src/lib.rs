//! `lute-wasm` — the browser playground's WASM surface over the Lute core.
//!
//! A thin `wasm-bindgen` shell exposing the four "Try Lute" playground
//! functions — `check`, `compile`, `trace`, `version` — over a single
//! self-contained document validated against the built-in `lute.core`
//! capability snapshot (plugin §5). It owns NO validation, compile, or trace
//! logic: every function assembles a [`CheckInput`] exactly as the CLI's
//! `build_input` does for a core-only single-document run (project `None`,
//! providers `None`, no `uses:`/`components:` imports) and hands off to the
//! same library entry points the CLI wraps.
//!
//! # Playground v1 scope
//! ONE self-contained document, `lute.core` profile only — NO `uses:` schema
//! imports, NO `components:`, NO plugins/providers. The imports/components/
//! providers on the assembled input are always EMPTY; a document that declares
//! `uses:`/`components:` still parses and checks, but those imports resolve to
//! nothing here (the browser has no filesystem to resolve them against).
//!
//! # Error discipline (the JS contract)
//! Every exported function returns a JSON `String` and NEVER throws for user
//! input errors — invalid source or a malformed mock yields the structured
//! JSON the CLI would print. Only an internal panic traps; [`start`] installs
//! `console_error_panic_hook` so such a trap surfaces a readable JS console
//! error rather than an opaque `unreachable`.

use lute_check::{check, CheckInput, Mode};
use lute_compile::{compile, LUTE_IR_VERSION, LUTE_LANG_VERSION};
use lute_trace::{parse_mock_yaml, trace_document, TraceExit};
use lute_manifest::core::load_core_snapshot;
use wasm_bindgen::prelude::*;

/// Install the panic hook once, automatically on module init (wasm-bindgen
/// runs a `start` function after the generated `init()` resolves). A Rust
/// panic then prints a readable stack to the browser console instead of a
/// bare `RuntimeError: unreachable`.
#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
}

/// Assemble the core-only [`CheckInput`] for `src`, mirroring the CLI's
/// `build_input` with project `None` / providers `None` and no resolvable
/// imports: the `lute.core` snapshot (embedded, FS-free), an empty provider
/// set, empty schema/component imports, and Ci (batch) analysis mode.
fn build_input(src: &str) -> CheckInput {
    CheckInput {
        text: src.to_string(),
        uri: "playground.lute".to_string(),
        snapshot: load_core_snapshot(),
        providers: Default::default(),
        mode: Mode::Ci,
        imports: Default::default(),
        components: Default::default(),
    }
}

/// Statically validate `src` against `lute.core` and return the serialized
/// [`lute_check::CheckResult`] — the SAME JSON shape `lute check --json`
/// prints (`ok`, `diagnostics[]` each with `severity`/`code`/`message`/`span`,
/// plus the best-effort `resolved` view). Never throws: a parse/semantic error
/// is reported as diagnostics inside the result, not as an exception.
#[wasm_bindgen]
pub fn check_source(src: &str) -> String {
    let input = build_input(src);
    let result = check(&input);
    serde_json::to_string(&result)
        .unwrap_or_else(|e| error_json(&format!("failed to serialize check result: {e}")))
}

/// Compile `src` to its typed JSON IR artifact, gated on `check` (D6). Returns
/// `{"ok":true,"artifact":{…}}` with the full artifact object on a clean check,
/// or `{"ok":false,"diagnostics":[…]}` (same diagnostic shape as
/// [`check_source`]) when the gate fails. Never throws.
#[wasm_bindgen]
pub fn compile_source(src: &str) -> String {
    let input = build_input(src);
    let value = match compile(&input) {
        Ok(artifact) => {
            let artifact = serde_json::to_value(&artifact).unwrap_or(serde_json::Value::Null);
            serde_json::json!({ "ok": true, "artifact": artifact })
        }
        Err(diags) => {
            let diags = serde_json::to_value(&diags).unwrap_or(serde_json::Value::Null);
            serde_json::json!({ "ok": false, "diagnostics": diags })
        }
    };
    serde_json::to_string(&value)
        .unwrap_or_else(|e| error_json(&format!("failed to serialize compile result: {e}")))
}

/// Trace `src` under the mock state in `mock_yaml` (the SAME YAML the CLI's
/// `--mock` file accepts), returning `{"exit":"complete|refused|incomplete",
/// "report":{…}}`. On `complete`/`incomplete` `report` is the full
/// [`lute_trace::TraceReport`]; on `refused` `report` carries the
/// `diagnostics` the CLI would print. A malformed mock YAML is itself a
/// `refused` exit whose diagnostics hold the synthetic `E-TRACE-MOCK-PARSE`
/// message — never a throw.
#[wasm_bindgen]
pub fn trace_source(src: &str, mock_yaml: &str) -> String {
    let input = build_input(src);

    // A blank mock string is an empty mock set (no seeds), matching a `trace`
    // with no `--mock` file. A non-blank-but-malformed mock is a `refused`
    // exit carrying the parse diagnostic, never a throw.
    let mocks = if mock_yaml.trim().is_empty() {
        Default::default()
    } else {
        match parse_mock_yaml(mock_yaml) {
            Ok(m) => m,
            Err(d) => {
                let diags = serde_json::to_value([d]).unwrap_or(serde_json::Value::Null);
                let value =
                    serde_json::json!({ "exit": "refused", "report": { "diagnostics": diags } });
                return serde_json::to_string(&value).unwrap_or_else(|e| {
                    error_json(&format!("failed to serialize trace result: {e}"))
                });
            }
        }
    };

    let (report, exit) = trace_document(&input, mocks);
    let value = match exit {
        TraceExit::Complete => {
            let report = serde_json::to_value(&report).unwrap_or(serde_json::Value::Null);
            serde_json::json!({ "exit": "complete", "report": report })
        }
        TraceExit::Incomplete => {
            let report = serde_json::to_value(&report).unwrap_or(serde_json::Value::Null);
            serde_json::json!({ "exit": "incomplete", "report": report })
        }
        TraceExit::Refused(diags) => {
            let diags = serde_json::to_value(&diags).unwrap_or(serde_json::Value::Null);
            serde_json::json!({ "exit": "refused", "report": { "diagnostics": diags } })
        }
    };
    serde_json::to_string(&value)
        .unwrap_or_else(|e| error_json(&format!("failed to serialize trace result: {e}")))
}

/// The three independent version axes (docs/versioning.md), matching
/// `lute version --json`: `{"toolchain":"0.7.0","language":"0.7.0","ir":"0.7.0"}`.
/// The toolchain axis is this crate's workspace `CARGO_PKG_VERSION`.
#[wasm_bindgen]
pub fn version() -> String {
    let value = serde_json::json!({
        "toolchain": env!("CARGO_PKG_VERSION"),
        "language": LUTE_LANG_VERSION,
        "ir": LUTE_IR_VERSION,
    });
    serde_json::to_string(&value)
        .unwrap_or_else(|e| error_json(&format!("failed to serialize version: {e}")))
}

/// A minimal `{"error":"…"}` JSON fallback for the (practically impossible)
/// case that serializing a well-formed serde value itself fails — keeps the
/// no-throw contract even on the serialization error path.
fn error_json(message: &str) -> String {
    serde_json::to_string(&serde_json::json!({ "error": message }))
        .unwrap_or_else(|_| "{\"error\":\"serialization failed\"}".to_string())
}
