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
//! 3. **`trace` MUST NOT execute engine machinery** beyond the project's
//!    own Datalog rules: no capability bridge, no dice, no scheduler. Since
//!    dsl 0.22.0 §6 (D-B) `trace`/`test` apply the seed facts and rules by
//!    default through [`datalog`] — the SAME stratified fixpoint the
//!    reference runner (`lute run`/`lute play`) uses, so the toolchain
//!    cannot disagree with itself. `derive: false` restores the 0.21
//!    lookup-only [`eval::FactStore`]. Every other answer the engine would
//!    compute is either supplied as a mock or reported
//!    [`value::Value::Unknown`].
//! 4. **Isolation is structural, not conventional.** This crate is wired
//!    ONLY into `lute-cli`. `lute-cel` stays parse-only (it holds no
//!    evaluator and MUST NOT gain one); `lute-check` and `lute-compile`
//!    depending on `lute-trace` is a conformance violation — enforced by
//!    `tests/quarantine.rs`, which reads every quarantined sibling's
//!    `Cargo.toml` directly and fails the build if any names `lute-trace`.
//! 5. **The evaluated subset (§4.3) is CLOSED.** [`eval::eval`] implements
//!    EXACTLY that subset; widening it — modeling narrative time, calling a
//!    bridge — is a spec revision, not a convenience.
//!
//! `lute_check::decide` is NOT this evaluator: it is a closed, total,
//! static constant-folder that reads no runtime state (spec §5.1). `D3`'s
//! ONE shared seam is `lute_check::apply_op` — the ground-operation
//! semantics written once in `lute-check` and lifted over
//! [`value::Value::Unknown`] here ([`eval::eval`]'s doc comment spells out
//! the K3 lift).

pub mod clock;
pub mod datalog;
pub mod eval;
pub mod mock;
pub mod quest_refs;
pub mod report;
pub mod value;
pub mod walk;

pub use eval::{eval, EffectiveState, EvalEnv, FactStore, Pat, Read};
pub use mock::{
    bridge_result_writes, merge, mock_subject, parse_bridges, parse_mock_surfaces, parse_mock_yaml,
    raise_judges, split_occasion, validate, validate_bridges, BridgeAnswer, MockSet, E_MOCK_SUBJECT,
    E_TRACE_ACCEPT, E_TRACE_BEAT, E_TRACE_CHOICE, E_TRACE_ENTRY, E_TRACE_EVENT, E_TRACE_MOCK_FACT,
    E_TRACE_MOCK_PARSE, E_TRACE_MOCK_TYPE, E_TRACE_MOCK_UNDECLARED, MOCK_TOP_KEYS,
    W_TRACE_MOCK_UNPRODUCIBLE,
};
pub use quest_refs::collect_referenced_reserved_quest_paths;
pub use report::{
    ComponentBoundary, Coverage, CoverageCount, Decision, GrantCredit, GrantReward, Seeds, Step,
    TraceExit,
    TraceReport, UnresolvedEntry,
};
pub use value::{UnresolvedAtom, Value};
pub use walk::{
    trace_beat, trace_beat_with_check, trace_document, trace_entries_with_check, trace_entry,
    trace_entry_with_check, trace_with_check, NOTE_BEAT_WHEN,
};
