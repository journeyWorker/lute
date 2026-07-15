//! `lute-trace` — the tree's ONLY expression evaluator (dsl 0.4.0 §4).
//!
//! # The D1 quarantine (normative, dsl 0.4.0 §4.2)
//!
//! `lute trace` answers the writer's question *"if the state were X, what
//! would this scene do?"* without the engine — bounded, three-valued
//! (Kleene/K3), authoring-only, and the weakest of the three legs of the
//! authoring loop (`check` proves, `compile` emits, `trace` *explains*): it
//! holds NO authority. That boundary is restated here as hard conformance
//! rules:
//!
//! 1. **`trace` MUST NOT feed `check` or `compile`.** No diagnostic, no
//!    check verdict, and no artifact byte may depend on whether `trace` ran
//!    or on anything it produced (spec §3 B5).
//! 2. **`trace` output is NEVER a static guarantee.** The §5 codes, computed
//!    by the checker alone, are the only static reachability surface —
//!    nothing `trace` reports may be cited, cached, or consumed as proof of
//!    reachability, coverage, or correctness.
//! 3. **`trace` MUST NOT execute engine machinery.** No Datalog fixpoint (a
//!    `derive: true` relation is never computed — [`eval::FactStore`]'s
//!    bounded scan is pattern LOOKUP, not derivation), no capability
//!    bridge, no dice, no scheduler. Every answer the engine would compute
//!    is either supplied as a mock or reported [`value::Value::Unknown`].
//! 4. **Isolation is structural, not conventional.** This crate is wired
//!    ONLY into `lute-cli`. `lute-cel` stays parse-only (it holds no
//!    evaluator and MUST NOT gain one); `lute-check` and `lute-compile`
//!    depending on `lute-trace` is a conformance violation — enforced by
//!    `tests/quarantine.rs`, which reads every quarantined sibling's
//!    `Cargo.toml` directly and fails the build if any names `lute-trace`.
//! 5. **The evaluated subset (§4.3) is CLOSED.** [`eval::eval`] implements
//!    EXACTLY that subset; widening it — "conveniently" deriving relations
//!    or modeling narrative time — is a spec revision, not a convenience.
//!
//! `lute_check::decide` is NOT this evaluator: it is a closed, total,
//! static constant-folder that reads no runtime state (spec §5.1). `D3`'s
//! ONE shared seam is `lute_check::apply_op` — the ground-operation
//! semantics written once in `lute-check` and lifted over
//! [`value::Value::Unknown`] here ([`eval::eval`]'s doc comment spells out
//! the K3 lift).

pub mod eval;
pub mod mock;
pub mod quest_refs;
pub mod report;
pub mod value;
pub mod walk;

pub use eval::{eval, EffectiveState, EvalEnv, FactStore, Pat, Read};
pub use mock::{
    merge, parse_mock_yaml, validate, MockSet, E_TRACE_ACCEPT, E_TRACE_CHOICE, E_TRACE_EVENT,
    E_TRACE_MOCK_FACT, E_TRACE_MOCK_TYPE, E_TRACE_MOCK_UNDECLARED, W_TRACE_MOCK_UNPRODUCIBLE,
};
pub use quest_refs::collect_referenced_reserved_quest_paths;
pub use report::{
    ComponentBoundary, Coverage, CoverageCount, Decision, Seeds, Step, TraceExit, TraceReport,
    UnresolvedEntry,
};
pub use value::{UnresolvedAtom, Value};
pub use walk::{trace_document, trace_with_check};
