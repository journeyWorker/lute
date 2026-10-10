//! `lute-runtime`'s JSON-string wasm binding.
//!
//! The binding owns no host policy. It only translates JSON strings and opaque
//! state handles to the public runtime API; expected input failures are values,
//! never panics.

use std::collections::BTreeMap;
use std::rc::Rc;

use lute_runtime::index::Bundle;
use lute_runtime::runtime::{Input, Output, Rejected, Runtime, Seed, Snapshot, State as RuntimeState};
use serde_json::Value;
use wasm_bindgen::prelude::*;

/// An opaque immutable state handle.
#[wasm_bindgen]
#[derive(Clone)]
pub struct State {
    inner: RuntimeState,
}

/// The result of beginning or stepping a program.
#[wasm_bindgen]
pub struct StepResult {
    ok: bool,
    state: Option<State>,
    output: Option<String>,
    rejected: Option<String>,
}

/// The result of restoring a snapshot.
#[wasm_bindgen]
pub struct RestoreResult {
    ok: bool,
    state: Option<State>,
    rejected: Option<String>,
}

/// The result of loading a bundle.
#[wasm_bindgen]
pub struct LoadResult {
    ok: bool,
    program: Option<Program>,
    rejected: Option<String>,
}

/// A loaded execution bundle.
#[wasm_bindgen]
#[derive(Clone)]
pub struct Program {
    runtime: Rc<Runtime>,
}

#[wasm_bindgen]
impl Program {
    /// Load a `project.index.json` and an object mapping artifact paths to IR.
    ///
    /// Bundle decoding and validation failures are returned as a rejected value
    /// rather than escaping into JavaScript as an exception.
    pub fn load(index: &str, artifacts: &str) -> LoadResult {
        let index = match serde_json::from_str::<Value>(index) {
            Ok(index) => index,
            Err(error) => {
                return LoadResult::failure(rejection(
                    "E-RUNTIME-IR-VERSION",
                    format!("invalid project.index.json: {error}"),
                ))
            }
        };
        let artifacts = match serde_json::from_str::<BTreeMap<String, Value>>(artifacts) {
            Ok(artifacts) => artifacts,
            Err(error) => {
                return LoadResult::failure(rejection(
                    "E-RUNTIME-IR-VERSION",
                    format!("invalid artifacts object: {error}"),
                ))
            }
        };
        match Runtime::load(Bundle::new(artifacts, index)) {
            Ok(runtime) => LoadResult::success(Self {
                runtime: Rc::new(runtime),
            }),
            Err(rejected) => LoadResult::failure(rejected),
        }
    }

    /// Begin a run from a JSON seed.
    pub fn begin(&self, seed: &str) -> StepResult {
        let seed = match serde_json::from_str::<Seed>(seed) {
            Ok(seed) => seed,
            Err(error) => return StepResult::begin_failure(input_error(error)),
        };
        match self.runtime.begin(seed) {
            Ok((state, output)) => StepResult::success(state, output),
            Err(rejected) => StepResult::begin_failure(rejected),
        }
    }

    /// Apply a JSON input without invalidating the supplied state handle.
    pub fn step(&self, state: &State, input: &str) -> StepResult {
        let input = match serde_json::from_str::<Input>(input) {
            Ok(input) => input,
            Err(error) => return StepResult::step_failure(state.clone(), input_error(error)),
        };
        match self.runtime.step(state.inner.clone(), input) {
            Ok((state, output)) => StepResult::success(state, output),
            Err((state, rejected)) => StepResult::step_failure(State { inner: state }, rejected),
        }
    }

    /// Serialize a state to its versioned snapshot JSON (keys sorted, the
    /// contract's canonical form).
    pub fn snapshot(&self, state: &State) -> String {
        canonical(&self.runtime.snapshot(&state.inner))
    }

    /// Restore a state from a versioned snapshot JSON.
    pub fn restore(&self, snapshot: &str) -> RestoreResult {
        match Snapshot::from_json(snapshot).and_then(|snapshot| self.runtime.restore(snapshot)) {
            Ok(state) => RestoreResult::success(state),
            Err(rejected) => RestoreResult::failure(rejected),
        }
    }

    /// The candidates of `occasion` (for `target`) in selection order, as a
    /// JSON array of `Candidate`.
    pub fn candidates(&self, state: &State, occasion: &str, target: Option<String>) -> String {
        canonical(&self.runtime.candidates(&state.inner, occasion, target.as_deref()))
    }

    /// The verdict of `beat` (for `member`), as `Candidate` JSON or `null`.
    pub fn eligibility(&self, state: &State, beat: &str, member: Option<String>) -> String {
        canonical(&self.runtime.eligibility(&state.inner, beat, member.as_deref()))
    }

    /// The clock position, as `ClockAt` JSON or `null` without a clock.
    pub fn clock(&self, state: &State) -> String {
        canonical(&self.runtime.clock(&state.inner))
    }

    /// Whether this state is terminal.
    pub fn terminal(&self, state: &State) -> bool {
        self.runtime.terminal(&state.inner)
    }

    /// The world view, as `WorldView` JSON (facts only when `with_facts`).
    pub fn view(&self, state: &State, with_facts: bool) -> String {
        canonical(&self.runtime.view(&state.inner, with_facts))
    }

    #[wasm_bindgen(getter)]
    pub fn fingerprint(&self) -> String {
        self.runtime.fingerprint().to_owned()
    }
}

#[wasm_bindgen]
impl StepResult {
    #[wasm_bindgen(getter)]
    pub fn ok(&self) -> bool {
        self.ok
    }

    #[wasm_bindgen(getter)]
    pub fn state(&self) -> Option<State> {
        self.state.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn output(&self) -> Option<String> {
        self.output.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn rejected(&self) -> Option<String> {
        self.rejected.clone()
    }
}

#[wasm_bindgen]
impl RestoreResult {
    #[wasm_bindgen(getter)]
    pub fn ok(&self) -> bool {
        self.ok
    }

    #[wasm_bindgen(getter)]
    pub fn state(&self) -> Option<State> {
        self.state.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn rejected(&self) -> Option<String> {
        self.rejected.clone()
    }
}

#[wasm_bindgen]
impl LoadResult {
    #[wasm_bindgen(getter)]
    pub fn ok(&self) -> bool {
        self.ok
    }

    #[wasm_bindgen(getter)]
    pub fn program(&self) -> Option<Program> {
        self.program.clone()
    }

    #[wasm_bindgen(getter)]
    pub fn rejected(&self) -> Option<String> {
        self.rejected.clone()
    }
}

#[cfg(feature = "test-panic")]
#[wasm_bindgen]
pub fn debug_panic() {
    panic!("debug panic requested");
}

impl State {
    fn from_runtime(inner: RuntimeState) -> Self {
        Self { inner }
    }
}

impl StepResult {
    fn success(state: RuntimeState, output: Output) -> Self {
        Self {
            ok: true,
            state: Some(State::from_runtime(state)),
            output: Some(canonical(&output)),
            rejected: None,
        }
    }

    fn begin_failure(rejected: Rejected) -> Self {
        Self {
            ok: false,
            state: None,
            output: None,
            rejected: Some(canonical(&rejected)),
        }
    }

    fn step_failure(state: State, rejected: Rejected) -> Self {
        Self {
            ok: false,
            state: Some(state),
            output: None,
            rejected: Some(canonical(&rejected)),
        }
    }
}

impl RestoreResult {
    fn success(state: RuntimeState) -> Self {
        Self {
            ok: true,
            state: Some(State::from_runtime(state)),
            rejected: None,
        }
    }

    fn failure(rejected: Rejected) -> Self {
        Self {
            ok: false,
            state: None,
            rejected: Some(canonical(&rejected)),
        }
    }
}

impl LoadResult {
    fn success(program: Program) -> Self {
        Self {
            ok: true,
            program: Some(program),
            rejected: None,
        }
    }

    fn failure(rejected: Rejected) -> Self {
        Self {
            ok: false,
            program: None,
            rejected: Some(canonical(&rejected)),
        }
    }
}

fn input_error(error: serde_json::Error) -> Rejected {
    rejection("E-RUNTIME-INPUT", format!("invalid input: {error}"))
}

fn rejection(code: &str, message: impl Into<String>) -> Rejected {
    Rejected {
        code: code.into(),
        message: message.into(),
    }
}

/// Contract JSON in its canonical form: through `serde_json::Value`, whose
/// maps sort their keys (spec 0.38.0 §9.3). Contract types always
/// serialize; a failure is a defect (a panic, so a trap in wasm).
fn canonical<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .expect("runtime contract types serialize")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const INDEX: &str = include_str!("../tests/fixtures/hub-once/project.index.json");
    const ARTIFACTS: &str = include_str!("../tests/fixtures/hub-once/artifacts.json");

    fn program() -> Program {
        let result = Program::load(INDEX, ARTIFACTS);
        assert!(result.ok, "load rejected: {:?}", result.rejected);
        result.program.expect("successful load has a program")
    }

    #[test]
    fn malformed_bundle_load_is_a_rejection_value() {
        let result = Program::load("{", "{}");
        assert!(!result.ok);
        let rejected = result.rejected.expect("load rejection JSON");
        let rejected: Rejected = serde_json::from_str(&rejected).expect("rejection object");
        assert_eq!(rejected.code, "E-RUNTIME-IR-VERSION");
    }

    #[test]
    fn rejected_step_returns_the_same_state_snapshot() {
        let program = program();
        let started = program.begin(r#"{"derive":true}"#);
        assert!(started.ok, "begin rejected: {:?}", started.rejected);
        let state = started.state.expect("successful begin has state");
        let before = program.snapshot(&state);
        let rejected = program.step(&state, "{");
        assert!(!rejected.ok);
        assert_eq!(
            rejected.rejected.as_deref().map(|s| s.contains("E-RUNTIME-INPUT")),
            Some(true)
        );
        let returned = rejected.state.expect("rejected step keeps state");
        assert_eq!(before, program.snapshot(&returned));
    }

    #[test]
    fn malformed_input_json_is_runtime_input_rejection() {
        let program = program();
        let started = program.begin(r#"{"derive":true}"#);
        assert!(started.ok);
        let state = started.state.expect("successful begin has state");
        let result = program.step(&state, r#"{"type":"not-an-input"}"#);
        assert!(!result.ok);
        let rejected = result.rejected.expect("input rejection JSON");
        let rejected: Rejected = serde_json::from_str(&rejected).expect("rejection object");
        assert_eq!(rejected.code, "E-RUNTIME-INPUT");
    }
}
