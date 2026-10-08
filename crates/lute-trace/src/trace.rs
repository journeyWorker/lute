//! `lute trace` / `lute test` / `trace_source` on the one walker
//! (`docs/design/runtime-unification.md` §3, S4): the document is gated,
//! its mocks validated, then COMPILED ([`lute_compile::compile_mapped`]) and
//! the artifact executed by [`Machine`] with a [`TraceDriver`] — the same
//! walk `lute run` and `lute play` make, with trace's policies (§3.3):
//! scripted `choose:` decisions else an automatic pick, a refusal of a pick
//! that is not offered, the mock's `bridges:` answers, and a halt at an
//! undecidable arm or menu while quest-level unknowns are recorded and the
//! walk goes on. The driver turns the Machine's records into the
//! [`TraceReport`] through the compile-time [`SourceMap`] (spans, authored
//! texts, rendered guards).
//!
//! ## Pipeline (§4.3)
//! 1. The caller's check verdict — any `Error` → [`TraceExit::Refused`].
//! 2. Parse + `lute_cel::fill_document` + `lute_check::fold_env` (the same
//!    re-derivation `lute_compile::compile` performs after its own gate).
//! 3. [`crate::mock::validate`] (and the `--entry` / `--beat` id checks) —
//!    any `E-TRACE-*` → `Refused`.
//! 4. `normalize_document` + `expand_document`, as compile runs them: the
//!    static notes and the AST-only report texts (a line's authored
//!    delivery, a `::jump` label) read this tree.
//! 5. `compile_mapped` → artifact + source map; the Machine walks it.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use lute_check::cel_expand::DefTable;
use lute_check::{CheckInput, CheckResult, FoldedEnv};
use lute_compile::source_map::{ArmSource, SourceInfo, SourceMarker};
use lute_compile::SourceMap;
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{Arm, AttrValue, Document, Line, Node};
use lute_syntax::datalog::FactTerm;
use serde_json::Value as Json;

use lute_runtime::session::{
    ExecProject, Premise, Verdict as SessionVerdict, World as SessionWorld,
};
use lute_runtime::{
    self as exec, guard_premise, BridgeCall, BridgeReply, Driver, Forced, GuardRead, Machine,
    Menu, MenuKind, OnUnknown, Pick, Seed, SiteKind, UnknownSite, Verdict,
};
use lute_runtime::BridgeAnswer;
use crate::mock::{self, MockSet, W_TRACE_MOCK_UNPRODUCIBLE};
use crate::report::{
    self, ComponentBoundary, ComponentSite, Coverage, CoverageCount, Decision, GrantCredit,
    GrantReward, Seeds, Step, TraceExit, TraceReport, UnresolvedEntry,
};
use lute_runtime::value::{UnresolvedAtom, Value};
use lute_compile::index::BeatKind;

mod ast;
mod driver_core;
mod driver_helpers;
mod driver_observe;
mod driver_records;
mod driver_trait;
mod judge;
mod notes;
mod pipeline;
mod walk;
mod world;

use ast::{key, AstIndex, Head, MenuSeen, TraceContext};
use driver_helpers::{bare_decision, json_text, plugin_call};
use judge::{authored_when, eligible_of, judge, judging_project, note_premise, premise_text};
use notes::{
    beat_when_note, derived_read_notes, empty_report, line_delivery, mock_unproducible_notes,
    occasion_notes, prereq_condition, render_atom, reserved_quest_notes, seed_fact_notes,
    seeds_summary, unmatched_event_notes,
};
use walk::{carried, present_beat, present_entry, raised_member, run_walk, Walked};
use world::{choice_diag, finish, is_true, logic_diag, terminal_at, Finish, World};

pub(crate) use ast::TraceDriver;
pub(crate) use notes::unverified_quest_note_head;
pub use notes::{NOTE_ACCEPT_SPENT, NOTE_BEAT_WHEN};
pub use pipeline::{
    trace_beat, trace_beat_with_check, trace_document, trace_entries_with_check, trace_entry,
    trace_entry_with_check, trace_with_check,
};

/// A condition held as source text (a document's beat `when`, a `--where`
/// filter), lowered to the IR's `expr` exactly as `lute compile` lowers it:
/// the executor evaluates `expr` only and never parses CEL (spec 0.38.0
/// §13). `None` for text outside the profile — what the checker already
/// refused.
pub fn lowered(raw: &str) -> Option<std::sync::Arc<exec::Slot>> {
    let expr = serde_json::to_value(lute_compile::expr::lower_expr(raw)?).ok()?;
    exec::Slot::of(&serde_json::json!({ "cel": raw, "expr": expr }))
}
