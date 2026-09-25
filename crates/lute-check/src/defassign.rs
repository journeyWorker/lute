//! Path-sensitive definite-assignment analysis (dsl §9.4).
//!
//! A forward data-flow walk over a shot's node stream that tracks, per execution
//! path, the set of state paths *provably assigned* at each point, and flags a
//! state read that is not provably set. Two diagnostics, kept distinct:
//!
//! - **`E-UNDECLARED`** — a `::set` *target* path whose tier exists but whose
//!   sub-path is absent from the inline `state:` schema (dsl §9.3/§9.4). Read
//!   sites inside CEL are NOT re-reported here: T4.3 ([`crate::cel_resolve`])
//!   already owns `E-UNDECLARED` / `E-CHOICELOG-READ` for reads.
//! - **`E-MAYBE-UNSET`** — a `scene.`/`run.`/`user.`/`app.` read of a declared
//!   path that has no schema `default`, no dominating `::set{p = …}` write, and
//!   no enclosing `has(p)`/`isSet(p)` guard on the current path.
//!
//! ## Flow model
//! The lattice element is an "assigned set" of dotted paths (each a *maximal*
//! chain, e.g. `run.x`). A path `p` is **proven** at a read when the schema decl
//! it resolves to carries a `default`, or when some assigned entry is `p` or an
//! ancestor of `p` (`run.x` proves `run.x.hp`). A write assigns exactly its
//! target path.
//!
//! - `=` is a pure write → assigns the target (a valid first assignment).
//! - `+=`/`-=`/`*=` **read the old value first** → the target is itself checked
//!   as a read before being assigned.
//! - An *arm-level* guard `has(p)`/`isSet(p)` in a `<when test>` / `<choice when>`
//!   condition adds `p` to the assigned set **within that arm only**. A `<match
//!   on>` SUBJECT guard is NOT a proof: the subject is checked purely as value
//!   reads, so a subject `has(p)`/`isSet(p)` never adds `p` to any arm base or
//!   the block-surviving set (a subject match may fall through, and proving `p`
//!   there leaks past a non-exhaustive match / survives `intersect_all`).
//! - `<branch>` `<choice>` arms and `<match>` `<when>`/`<otherwise>` arms **fork**
//!   the incoming set; the join after the block is the **intersection** of the
//!   arms' assigned-after sets — a path is assigned-after only if assigned on
//!   *every* path. A block that need not take any arm (a `<branch>` whose choices
//!   are all guarded, a `<match>` with no `<otherwise>`) contributes a possible
//!   fall-through, so the join is just the pre-block set.
//!
//! ## Tiers (dsl §9.1)
//! `scene.*`, `run.*`, `user.*`, and `app.*` all follow the SAME path-sensitive
//! proof rules: a read not provably assigned on the current path is
//! `E-MAYBE-UNSET` unless the decl is schema-defaulted or guarded (§9.4). A
//! `scene.*` read-before-write within the analyzed node stream therefore flags;
//! a defaulted scene decl is seeded at scene entry and stays safe.
//!
//! ## Cross-shot scope (dsl §9.1)
//! `scene.*` persists across shots within an episode and `run.*` persists across
//! the whole run, so a sound cross-shot analysis MUST drive this pass over the
//! WHOLE-DOCUMENT ordered node stream (all shots concatenated). This module
//! analyzes exactly the `&[Node]` slice it is given and does NOT reach across
//! shots itself; the document-level wiring is T4.9's responsibility. `app.*` is
//! engine-owned and read-only (§9.5, T4.5) but reads still follow the proof rules.
//!
//! ## Write vs. available lattices (connectivity T8 review, RevT8 P1)
//! This pass threads TWO PARALLEL lattices (`Flow`) through the walk under
//! IDENTICAL fork/join control-flow rules: `available` (writes ∪
//! `apply_condition` guard-proofs) drives `E-MAYBE-UNSET` exactly as before —
//! a read guard-proven present (`isSet(p)`/`has(p)`) stays accepted even
//! though nothing wrote `p`. `writes` tracks ONLY `::set`/persist-sugar WRITE
//! targets; [`check_definite_assignment`] returns `writes`' end-of-document
//! join as the envelope's guaranteed-write must-set `G` (`crate::envelope::
//! guaranteed`, dsl §4.3), so a guard proof can never leak into `G` without a
//! matching write — `G ⊆ possible_writes(P)` holds by construction.
//!
//! ## Spans (cel-parser 0.10.1 carry-forward, T3.1/T4.3)
//! Per-node CEL byte offsets are unavailable, so a read diagnostic falls back to
//! the enclosing slot's span; a target-path diagnostic uses the `::set` path span.
//! A read reached through a `@def` anchors at that `@def` token instead.
//!
//! ## Defs, assumptions, short-circuits (dsl 0.24.0)
//! - A `@def` is a macro: every use site — a CEL slot, a `{{@def}}`
//!   interpolation, a `::use` argument, a `<match on="@def">` subject — is
//!   checked on its EXPANDED text, so a maybe-unset read inside a def body is
//!   `E-MAYBE-UNSET` at the use, naming the def.
//! - A beat `when:` / bundle `<beat when>` / entry `when=` is an assumption
//!   for its body: its dominating guards seed the `available` set
//!   ([`check_definite_assignment`]'s `assume`).
//! - A presence guard proves the reads its short-circuit protects inside the
//!   expression ([`crate::cel_paths::PathUse::local`]) without proving the body.
//! - `prev.run.*` is one snapshot taken at run end: once any `prev.run.<p>` is
//!   proven present, every `prev.run.<q>` whose `run.<q>` has a `default` is
//!   present too (a defaulted `run.*` path always holds a value at run end).

use std::collections::{BTreeMap, BTreeSet};

use lute_cel::CelArena;
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::types::Type;
use lute_syntax::ast::{
    Arm, Attr, AttrValue, Branch, CelSlot, Choice, ClipNode, Hub, Interp, InterpKind, Match, Node,
    Objective, On, Set, Timeline,
};

use crate::cel_expand::{expand_cel, DefTable};
use crate::cel_paths::{
    collect_path_uses, is_entry_path, is_reserved_entry_read, is_reserved_quest_activated_at,
    is_reserved_quest_objective_done, is_reserved_quest_path, is_reserved_quest_state,
    is_state_path, PathRole,
};
use crate::meta::StateSchema;
// (no `Ctx` import — `check_definite_assignment`'s `_ctx` param was always
// dead; connectivity T11 dropped it so `lute-cli`'s project-wide envelope
// wiring needs no throwaway `Ctx`/`Env` construction at all.)

/// What a definite-assignment walk resolves names against: the folded state
/// schema, the def table every `@def` use expands through, and the def result
/// types a `<match on="@def">` subject takes its domain from (dsl 0.24.0).
pub struct Scope<'a> {
    pub schema: &'a StateSchema,
    pub defs: DefTable<'a>,
    pub def_types: &'a BTreeMap<String, Type>,
}

static NO_BODIES: BTreeMap<String, String> = BTreeMap::new();
static NO_PARAMS: BTreeMap<String, Vec<(String, Type)>> = BTreeMap::new();
static NO_TYPES: BTreeMap<String, Type> = BTreeMap::new();

impl<'a> Scope<'a> {
    /// A document's scope: its folded schema and merged def tables.
    pub fn of(folded: &'a crate::check::FoldedEnv) -> Self {
        Self {
            schema: &folded.env.state,
            defs: DefTable {
                bodies: &folded.def_bodies,
                params: &folded.env.def_params,
            },
            def_types: &folded.env.def_types,
        }
    }

    /// `schema` with no defs in scope.
    pub fn bare(schema: &'a StateSchema) -> Self {
        Self {
            schema,
            defs: DefTable {
                bodies: &NO_BODIES,
                params: &NO_PARAMS,
            },
            def_types: &NO_TYPES,
        }
    }
}

/// Set of provably-assigned state paths on the current execution path.
pub(crate) type Assigned = BTreeSet<String>;

/// Two parallel lattices threaded together through the walk (see the module
/// doc's "Write vs. available lattices"): `available` is the pre-existing
/// read-satisfaction set (writes ∪ guard-proofs); `writes` is the narrower
/// WRITE-only must-set exported for the envelope. Fork/join (branch/match
/// `intersect_flows`; hub/on/objective may-only fork+discard) apply to both
/// fields identically.
#[derive(Clone, Default)]
struct Flow {
    /// writes ∪ `apply_condition` guard-proofs — unchanged read-satisfaction
    /// lattice; drives `E-MAYBE-UNSET` exactly as before this change.
    available: Assigned,
    /// `::set` / `<choice into>` record WRITES only — the envelope guaranteed-
    /// write must-set (`crate::envelope::guaranteed`).
    writes: Assigned,
}

/// Run the §9.4 definite-assignment analysis over a shot's node stream.
///
/// Returns the diagnostics, the final end-of-document WRITE-ONLY `Assigned`
/// set — the must-write join (`intersect_flows`) of every execution path's
/// `::set`/record-sugar writes, i.e. every path provably WRITTEN on ALL
/// paths through `nodes` (guard-proofs, `isSet`/`has`, still drive
/// `E-MAYBE-UNSET` internally via the `available` lattice but are NOT part
/// of this set; the envelope layer's `crate::envelope::guaranteed`,
/// connectivity T8/§4.3, reuses it directly as its guaranteed-write set
/// `G`) — AND every value READ that fell back to entry state: declared,
/// non-`run.choiceLog.*`, with no schema `default` and no dominating LOCAL
/// write/guard at the read point — exactly the read set that earns
/// `E-MAYBE-UNSET` here, mirrored by path (connectivity T11/§4.3's
/// `crate::envelope::check_envelope` reclassifies exactly these reads
/// against a node's project-wide `Env` instead of re-deriving its own
/// read-collection walk — see [`check_read`]'s doc comment for why a
/// schema-defaulted read is safely excluded here too).
///
/// `assume` is the condition the whole node stream runs under (dsl 0.24.0): a
/// scene's beat `when:`, a bundle `<beat when>`, an entry `when=`. Its
/// dominating guards seed `available` — never `writes` — and its own reads
/// are that slot's own check ([`check_quest_guard_defassign`]), not repeated
/// here.
pub fn check_definite_assignment(
    nodes: &[Node],
    cx: &Scope<'_>,
    assume: Option<&CelSlot>,
) -> (Vec<Diagnostic>, Assigned, Vec<(String, Span)>) {
    let mut diags = Vec::new();
    let mut reads = Vec::new();
    let mut flow = Flow {
        available: assumed_present(assume, cx),
        writes: Assigned::new(),
    };
    walk_nodes(nodes, cx, &mut flow, &mut diags, &mut reads);
    (diags, flow.writes, reads)
}

/// The state paths `assume` proves present for the body it guards (its
/// dominating `isSet`/`has` guards, `@def`s expanded) — empty for `None`.
pub(crate) fn assumed_present(assume: Option<&CelSlot>, cx: &Scope<'_>) -> Assigned {
    let mut assigned = Assigned::new();
    if let Some(slot) = assume {
        apply_condition(slot, cx, &mut assigned, &mut Vec::new(), &mut Vec::new());
    }
    assigned
}

/// Whether `path` is provably present given the `assigned` set: an entry is
/// the path or an ancestor of it, or `path` is a `prev.run.*` mirror the
/// atomic run-end snapshot covers (see the module doc).
pub(crate) fn is_present(path: &str, assigned: &Assigned, schema: &StateSchema) -> bool {
    proven(path, assigned, &[], schema)
}

/// Recursively collect the subject `Span` of every domain-exhaustive
/// `<match>` reachable from `nodes` (arms recursed through exactly like
/// [`walk_nodes`]/[`walk_match`] above — `Branch`/`Hub` choice bodies,
/// `On`/`Objective` bodies, nested `Match` arm bodies; a `Timeline` clip can
/// only be a `Set`/`Directive`, never a nested `Match`, matching
/// `walk_timeline`'s own shape).
///
/// Mirrors `check.rs`'s own `Walker::walk` traversal, which collects this
/// SAME span set (`exhaustive_subject_spans`) to drive
/// `suppress_exhaustive_subject_reads` (T4.4/T4.6 carry-forward): a `<match
/// on>` subject that reads maybe-unset is nonetheless SAFE when the match is
/// exhaustive (every case, including "unset", is handled by an arm) — the
/// read can never escape unhandled, so standalone `check()` never reports
/// `E-MAYBE-UNSET` for it.
///
/// Exposed so any OTHER consumer that independently re-derives
/// `check_definite_assignment`'s raw `reads`/diagnostics (connectivity T11's
/// project-envelope reconciliation, `lute-cli::run_check_project`) can apply
/// the IDENTICAL exemption before treating a read as "entry-dependent" —
/// otherwise a subject read `check()` proves safe would wrongly re-enter the
/// project-wide read set and risk a false `E-STATE-MAYBE-UNAVAILABLE`,
/// violating the dsl §7 soundness invariant (a project run must never newly
/// error a file single-file `check` reports clean).
pub fn exhaustive_match_subject_spans(nodes: &[Node], cx: &Scope<'_>) -> Vec<Span> {
    let mut spans = Vec::new();
    collect_exhaustive_spans(nodes, cx, &mut spans);
    spans
}

fn collect_exhaustive_spans(nodes: &[Node], cx: &Scope<'_>, spans: &mut Vec<Span>) {
    for node in nodes {
        match node {
            Node::Branch(b) => {
                for choice in &b.choices {
                    collect_exhaustive_spans(&choice.body, cx, spans);
                }
            }
            Node::Hub(h) => {
                for choice in &h.choices {
                    collect_exhaustive_spans(&choice.body, cx, spans);
                }
            }
            Node::On(o) => collect_exhaustive_spans(&o.body, cx, spans),
            Node::Objective(o) => collect_exhaustive_spans(&o.body, cx, spans),
            Node::Match(m) => {
                let (subject, info) = resolve_subject(m, cx);
                if crate::match_check::is_exhaustive_resolved(
                    m,
                    subject.as_deref(),
                    &info,
                    cx.schema,
                ) {
                    spans.push(m.subject.span);
                }
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            collect_exhaustive_spans(body, cx, spans);
                        }
                    }
                }
            }
            Node::Line(_)
            | Node::Set(_)
            | Node::Directive(_)
            | Node::Timeline(_)
            | Node::Assert(_)
            | Node::Retract(_) => {}
        }
    }
}

/// `m`'s subject resolved for domain inference in `cx`
/// ([`crate::match_check::resolve_subject`]).
fn resolve_subject(m: &Match, cx: &Scope<'_>) -> (Option<String>, crate::match_check::DomainInfo) {
    crate::match_check::resolve_subject(m, &cx.defs, cx.def_types, cx.schema)
}

/// Definite-assignment for a quest's `start`/`fail` CEL guard (dsl 0.2.0 §6.3,
/// §9.4). These are evaluated at QUEST
/// ENTRY — nothing dominates them (they are the first thing the engine
/// evaluates), so the assigned set starts EMPTY, exactly like a fresh
/// [`check_definite_assignment`] call.
///
/// dsl 0.10.0 §12.2: intra-expression narrowing DOES run here. Until 0.10.0
/// this reused [`check_reads`], on the argument that `has(p)`/`isSet(p)` in a
/// quest-entry predicate "proves nothing, because there is no guarded body to
/// prove into". That conflated two different guarantees. There is indeed no
/// body — nothing here may leak into a caller's lattice, and nothing does: the
/// `Assigned` below is local and dropped on return. But `isSet(p) && p == 'x'`
/// short-circuits, so the guard dominates the read **inside the expression**,
/// which is a property of the expression and holds in every slot the language
/// has. [`apply_condition`] already implements exactly that, positionally, and
/// is what a content line's `when=` and an `<objective when>` have always used.
///
/// [`walk_objective`] applies the same narrowing to an `<objective done>`
/// inline rather than through this helper, because `done=` inherits the
/// enclosing flow's assigned set and must keep inheriting it.
///
/// Quest guards stay OUTSIDE the envelope's read-collection (connectivity T11,
/// dsl §4.4): only the diagnostics are returned.
pub fn check_quest_guard_defassign(slot: &CelSlot, cx: &Scope<'_>) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let mut assigned = Assigned::new();
    let mut reads = Vec::new();
    apply_condition(slot, cx, &mut assigned, &mut diags, &mut reads);
    diags
}

/// Forward-walk a node sequence, threading the assigned set through in order.
fn walk_nodes(
    nodes: &[Node],
    cx: &Scope<'_>,
    flow: &mut Flow,
    diags: &mut Vec<Diagnostic>,
    reads: &mut Vec<(String, Span)>,
) {
    for node in nodes {
        match node {
            Node::Set(set) => walk_set(set, cx, flow, diags, reads),
            Node::Branch(branch) => walk_branch(branch, cx, flow, diags, reads),
            Node::Match(m) => walk_match(m, cx, flow, diags, reads),
            Node::Timeline(tl) => walk_timeline(tl, cx, flow, diags, reads),
            Node::Hub(hub) => walk_hub(hub, cx, flow, diags, reads),
            // A `{{path}}` interpolation on a content line is a state READ at the
            // line's position (dsl §7.6, §9.4): give it the SAME definite-
            // assignment treatment as a guard / `::set` read — a maybe-unset path
            // (declared, no default, no dominating write, no guard) is
            // `E-MAYBE-UNSET`. A `{{@def}}` reads what its expanded body reads
            // (dsl 0.24.0); `Reserved` interps carry no state path.
            // (`E-UNDECLARED` for the path and `E-UNDECLARED-REF` for the ref are
            // the cel-layer resolver's job, mirroring how guard reads split.)
            //
            // dsl 0.4.0 §7.2: a `when=` guard is a one-arm, NON-DOMINATING
            // construct — the line may or may not emit, exactly like an
            // `<on>`/`<hub>` arm (`walk_hub`/`walk_on` above): fork the
            // incoming set, let `apply_condition` prove reads for THIS
            // line's interps only, then DISCARD the fork (nothing folds back
            // past a line that may not show) — so `when="isSet(run.tip)"`
            // proves `{{run.tip}}`, but the outer set is untouched either way.
            Node::Line(line) => match &line.when {
                Some(when) => {
                    let mut fork = flow.available.clone();
                    apply_condition(when, cx, &mut fork, diags, reads);
                    check_interp_reads(&line.interps, cx, &fork, diags, reads);
                }
                None => check_interp_reads(&line.interps, cx, &flow.available, diags, reads),
            },
            // dsl 0.12.0: `::next{when=}` is a one-arm, NON-DOMINATING
            // construct — the SAME treatment a gated line's `when=` gets
            // above: fork, prove THIS guard's own reads via
            // `apply_condition`, then discard (a `::next` reads/writes no
            // state of its own, so unlike a line there is no further body
            // to check against the fork — no `interps` on a directive).
            // `None` for every other directive tag (only `next` ever
            // populates `.when`). A `::use` `@def` argument is spliced into
            // the component body, so it is read here, at the call.
            Node::Directive(d) => {
                if let Some(when) = &d.when {
                    let mut fork = flow.available.clone();
                    apply_condition(when, cx, &mut fork, diags, reads);
                }
                if d.tag == "use" {
                    check_use_arg_reads(&d.attrs, cx, &flow.available, diags, reads);
                }
            }
            Node::On(on) => walk_on(on, cx, flow, diags, reads),
            Node::Objective(o) => walk_objective(o, cx, flow, diags, reads),
            // Fact args are ground (entity ids / bools), never `state:` paths —
            // no definite-assignment read/write to track (0.3.0 T2; write
            // policy is Task 10).
            Node::Assert(_) | Node::Retract(_) => {}
        }
    }
}

/// The reads of a line's (or label's) `{{…}}` interpolations: a `{{path}}`
/// reads its path; a `{{@def}}` reads its expanded body, anchored at the
/// interpolation.
fn check_interp_reads(
    interps: &[Interp],
    cx: &Scope<'_>,
    assigned: &Assigned,
    diags: &mut Vec<Diagnostic>,
    reads: &mut Vec<(String, Span)>,
) {
    for interp in interps {
        match interp.kind {
            InterpKind::Path => check_read(
                &Use::plain(interp.raw.clone(), interp.span),
                cx,
                assigned,
                diags,
                reads,
            ),
            InterpKind::Ref => {
                for u in uses_of(&interp.raw, interp.span, cx) {
                    if u.role == PathRole::Read {
                        check_read(&u, cx, assigned, diags, reads);
                    }
                }
            }
            InterpKind::Reserved => {}
        }
    }
}

/// The reads of a `::use`'s `@def` arguments (dsl 0.24.0).
fn check_use_arg_reads(
    attrs: &[Attr],
    cx: &Scope<'_>,
    assigned: &Assigned,
    diags: &mut Vec<Diagnostic>,
    reads: &mut Vec<(String, Span)>,
) {
    for attr in attrs {
        if let AttrValue::Ref(slot) = &attr.value {
            check_reads(slot, cx, assigned, diags, reads);
        }
    }
}

/// A `::set{path op expr}` (dsl §7.3.4). The RHS reads are checked; a compound
/// op additionally reads the OLD target value; then the target is assigned —
/// into BOTH `flow.available` (read-satisfaction) and `flow.writes` (the
/// envelope guaranteed-write must-set): a `::set` is unconditionally a WRITE.
///
/// dsl 0.24.0 §1: a GUARDED `::set{… when="g"}` is a one-arm, NON-DOMINATING
/// write — the same treatment a gated line gets in [`walk_nodes`]: `g`'s
/// reads are checked and its `has(p)`/`isSet(p)` proofs narrow THIS set's
/// own RHS/compound reads on a discarded fork, and the target is assigned in
/// NEITHER lattice (the write may not happen, so a later read stays
/// maybe-unset and the path never enters the guaranteed-write set).
fn walk_set(
    set: &Set,
    cx: &Scope<'_>,
    flow: &mut Flow,
    diags: &mut Vec<Diagnostic>,
    reads: &mut Vec<(String, Span)>,
) {
    let guarded = set.when.as_ref().map(|when| {
        let mut fork = flow.available.clone();
        apply_condition(when, cx, &mut fork, diags, reads);
        fork
    });
    let available = guarded.as_ref().unwrap_or(&flow.available);
    // RHS value reads (guards here don't gate the arm; only their unset-safety).
    check_reads(&set.expr, cx, available, diags, reads);

    let target = &set.path;
    // dsl 0.24.0 §3/§4: `run.approval[@who]` in a component body names no
    // member until a `::use` binds it; each `::use` checks the bound member.
    if crate::component_effects::set_path_index(target).is_some() {
        return;
    }
    if is_state_path(target) {
        // Compound assignment reads the old value first (dsl §9.4).
        if set.op != "=" {
            check_read(&Use::plain(target.clone(), set.span), cx, available, diags, reads);
        }
        // The write target itself must be declared (T4.3 covers read sites; the
        // `::set` LHS path is this pass's responsibility). An `entry.*` target
        // is not "undeclared" but unwritable — `set_op`'s reserved-write
        // rejection (dsl 0.19.0 §5) is its one report.
        if !is_declared(target, cx.schema) && !is_entry_path(target) {
            let mut msg = format!("state path `{target}` is not declared in `state:` (dsl §9.4)");
            if let Some(sugg) = crate::cel_paths::nearest_declared_path(target, cx.schema, 2) {
                msg.push_str(&format!(" — did you mean `{sugg}`?"));
            }
            diags.push(diag("E-UNDECLARED", msg, set.path_span));
        }
        // Assign regardless of declaredness so later reads don't cascade —
        // but only an unguarded write is definite.
        if guarded.is_none() {
            flow.available.insert(target.clone());
            flow.writes.insert(target.clone());
        }
    }
}

/// `<choice into="run.<path>">` (dsl 0.6.0 §2) is EXACTLY a `::set{into =
/// value}` appended to the arm WHEN the choice is selected
/// (`envelope::scan_choice_record` mirrors this same sugar for `P`).
/// Well-formedness (declared `run.*` `into`, value policy, …) is
/// `check_choice_record`'s job (check.rs) — this only recovers the target path
/// when `into=` is a plain string. `into=` ALONE drives the record now (the
/// `persist=` attr was removed in 0.6.0). Applied AFTER the arm body walk (by
/// every caller) so it cannot retroactively satisfy a read of the same path
/// INSIDE the body; it enters both `available` and `writes` exactly like a
/// `::set`.
fn choice_record_target(choice: &Choice) -> Option<&str> {
    choice
        .attrs
        .iter()
        .find(|a| a.key == "into")
        .and_then(|into| match &into.value {
            AttrValue::Str(path) => Some(path.as_str()),
            _ => None,
        })
}

/// Apply a choice's record-sugar write (if any) to `flow`, AFTER its body has
/// already been walked by the caller — see [`choice_record_target`].
fn apply_choice_record(choice: &Choice, flow: &mut Flow) {
    if let Some(target) = choice_record_target(choice) {
        flow.available.insert(target.to_string());
        flow.writes.insert(target.to_string());
    }
}

/// A `<branch>`: each `<choice>` forks the incoming set; join = intersection when
/// some choice is unconditional (one arm always runs), else the pre-block set.
fn walk_branch(
    branch: &Branch,
    cx: &Scope<'_>,
    flow: &mut Flow,
    diags: &mut Vec<Diagnostic>,
    reads: &mut Vec<(String, Span)>,
) {
    let mut arm_finals: Vec<Flow> = Vec::new();
    let mut has_unconditional = false;
    for choice in &branch.choices {
        let mut arm = flow.clone();
        match &choice.when {
            Some(cond) => apply_condition(cond, cx, &mut arm.available, diags, reads),
            None => has_unconditional = true,
        }
        // §7.6: a `{{path}}` in the choice LABEL is a READ at the point the choice
        // is OFFERED — after its own `when` guard proves (a guarded choice's label
        // shows only when the guard holds), so check against the post-guard arm.
        check_label_reads(
            &choice.label,
            cx,
            &arm.available,
            choice.span,
            diags,
            reads,
        );
        walk_nodes(&choice.body, cx, &mut arm, diags, reads);
        apply_choice_record(choice, &mut arm);
        arm_finals.push(arm);
    }
    if has_unconditional && !arm_finals.is_empty() {
        *flow = intersect_flows(arm_finals);
    }
    // else: a guarded-only branch may fall through — keep the pre-block set.
}

/// A `<hub>` (dsl §7.3.2, §11.1.3): hub arms have NO dominance relation among one
/// another (same join rule as `<match>` arms), so a write inside one arm is a
/// **may-write** at hub exit, never a definite assignment. Definite-assignment
/// therefore stays conservative — each choice's `when` guard and body are walked
/// on its own discarded fork (mirroring `walk_branch`: the guard's value reads are
/// still flagged), but nothing is folded back into the surviving set (a hub never
/// proves a path assigned past the block) — for EITHER lattice.
fn walk_hub(
    hub: &Hub,
    cx: &Scope<'_>,
    flow: &mut Flow,
    diags: &mut Vec<Diagnostic>,
    reads: &mut Vec<(String, Span)>,
) {
    for choice in &hub.choices {
        let mut arm = flow.clone();
        // Same guard-read check as `walk_branch` — a maybe-unset read inside a
        // choice `when` must not escape defassign. The arm is discarded, so a
        // guard-proven path never survives past the block (conservative).
        if let Some(cond) = &choice.when {
            apply_condition(cond, cx, &mut arm.available, diags, reads);
        }
        // Label reads (§7.6): checked against the post-guard arm, then discarded
        // with the rest of the fork.
        check_label_reads(
            &choice.label,
            cx,
            &arm.available,
            choice.span,
            diags,
            reads,
        );
        walk_nodes(&choice.body, cx, &mut arm, diags, reads);
        apply_choice_record(choice, &mut arm);
        // arm (and any record write) discarded — a hub never folds back.
    }
}

/// An `<on>` arm (dsl 0.2.0 §4.4): `<on>` arms have NO dominance relation among
/// one another (the same join rule as `<match>`/`<hub>` arms) — a write inside
/// one arm is a **may-write**, never a definite assignment. Mirrors
/// [`walk_hub`]: the `when` guard proves paths for THIS arm only, the body
/// walks on a forked, DISCARDED set — nothing folds back into the surviving
/// set (a path first written only inside `<on>` arms stays maybe-unset unless
/// every arm writes it or it carries a schema `default`).
fn walk_on(
    on: &On,
    cx: &Scope<'_>,
    flow: &Flow,
    diags: &mut Vec<Diagnostic>,
    reads: &mut Vec<(String, Span)>,
) {
    let mut arm = flow.clone();
    if let Some(cond) = &on.when {
        apply_condition(cond, cx, &mut arm.available, diags, reads);
    }
    walk_nodes(&on.body, cx, &mut arm, diags, reads);
}

/// An `<objective>` (dsl 0.2.0 §6.4): the body emits ONCE, when `done` first
/// holds — a discrete, non-dominating transition exactly like an `<on>` arm
/// (§4.4), so it gets the SAME may-write join as [`walk_on`]. `done` is a
/// value READ (like a `<match>` subject, [`walk_match`]) — it does not gate
/// the body, so it is checked via [`check_reads`], not [`apply_condition`].
/// `when` DOES gate visibility (mirrors a hub/branch choice guard) and proves
/// paths for this arm only.
fn walk_objective(
    o: &Objective,
    cx: &Scope<'_>,
    flow: &Flow,
    diags: &mut Vec<Diagnostic>,
    reads: &mut Vec<(String, Span)>,
) {
    let mut arm = flow.clone();
    // dsl 0.10.0 §12.2: `done=` is a quest-entry-shaped predicate like
    // `<quest start|fail>` — no body to prove into, but `isSet(p) && …`
    // short-circuits inside the expression, so the same intra-expression
    // narrowing applies. The narrowing is slot-LOCAL: `apply_condition` runs
    // against a CLONE of the inherited set, which is dropped here and never
    // reaches `arm.available` below.
    //
    // The clone starts from `arm.available` rather than from empty, which is
    // what keeps this a relaxation: a `::set` dominating the objective still
    // proves the read, exactly as `check_reads(&o.done, …, &arm.available, …)`
    // did before.
    let mut done_assigned = arm.available.clone();
    apply_condition(&o.done, cx, &mut done_assigned, diags, reads);
    // dsl 0.23.0 §2 / 0.24.0 §2.1: `by=` / `until=` are read like `done=` —
    // slot-local narrowing only.
    for deadline in o.by.iter().chain(&o.until) {
        let mut deadline_assigned = arm.available.clone();
        apply_condition(deadline, cx, &mut deadline_assigned, diags, reads);
    }
    if let Some(cond) = &o.when {
        apply_condition(cond, cx, &mut arm.available, diags, reads);
    }
    walk_nodes(&o.body, cx, &mut arm, diags, reads);
    // dsl 0.16.0 §2: an objective `reward.when` is a slot-LOCAL guard —
    // same intra-expression narrowing `done=`/`when=` get, discarded so it
    // can never leak into the surviving set (a reward may or may not fire,
    // like an `<on>` arm). Runs after the body to keep the walk order
    // byte-identical to `lute_syntax::walk::objective`.
    for r in &o.rewards {
        if let Some(cond) = &r.when {
            let mut fork = arm.available.clone();
            apply_condition(cond, cx, &mut fork, diags, reads);
        }
    }
}

/// Definite-assignment for a `<choice label>`'s `{{…}}` interpolations (dsl
/// §7.6, §9.4). Choice labels are String attrs (not in the AST like content-line
/// interps), so they are recovered via the shared [`crate::check::scan_label_interps`]
/// scan and read exactly like a content line's ([`check_interp_reads`]).
/// Undeclared paths are the cel-layer resolver's job, so `check_read` no-ops
/// on them here.
fn check_label_reads(
    label: &str,
    cx: &Scope<'_>,
    assigned: &Assigned,
    span: Span,
    diags: &mut Vec<Diagnostic>,
    reads: &mut Vec<(String, Span)>,
) {
    let interps = crate::check::scan_label_interps(label, span);
    check_interp_reads(&interps, cx, assigned, diags, reads);
}

/// A `<match>`: the `on=` subject is checked for value-reads (it dominates every
/// arm) but its position is NOT treated as a proving guard — a subject
/// `has(p)`/`isSet(p)` must never add `p` to the block-surviving set or the arm
/// bases (that would leak an unproven path past a non-exhaustive fall-through and
/// survive `intersect_all` on exhaustive matches). Each `<when>`/`<otherwise>`
/// still forks; join = intersection only when an `<otherwise>` makes the match
/// exhaustive. Arm-level `<when test>` guards keep proving (see `apply_condition`).
///
/// dsl 0.23.1 (ashen N3): an arm NARROWS a plain state-path subject. Inside
/// `<when is="x">` (no `unset` alternative) the subject equals a named value,
/// so it is set; once an arm has taken every unset value (`is="unset"` with no
/// narrowing `test`), every later arm and the `<otherwise>` see it set. The
/// proof is arm-local (`available` only, never `writes`). dsl 0.24.0: a
/// `@def` subject whose body is one state path narrows that path.
fn walk_match(
    m: &Match,
    cx: &Scope<'_>,
    flow: &mut Flow,
    diags: &mut Vec<Diagnostic>,
    reads: &mut Vec<(String, Span)>,
) {
    // Subject is a value-read check only; subject-position guards do NOT prove.
    check_reads(&m.subject, cx, &flow.available, diags, reads);

    let (resolved, info) = resolve_subject(m, cx);
    let exhaustive =
        crate::match_check::is_exhaustive_resolved(m, resolved.as_deref(), &info, cx.schema);
    let subject = resolved.filter(|p| is_declared(p, cx.schema) && !is_choicelog(p));
    let mut unset_taken = false;
    let mut arm_finals: Vec<Flow> = Vec::new();
    for arm in &m.arms {
        let mut branch = flow.clone();
        match arm {
            Arm::When { is, test, body, .. } => {
                if let Some(p) = &subject {
                    if unset_taken
                        || crate::match_check::is_pattern_proves_set(is.as_ref(), Some(p))
                    {
                        branch.available.insert(p.clone());
                    }
                }
                apply_condition(test, cx, &mut branch.available, diags, reads);
                walk_nodes(body, cx, &mut branch, diags, reads);
                unset_taken |= subject.as_deref().is_some_and(|p| {
                    crate::match_check::arm_takes_unset(is.as_ref(), &test.raw, Some(p), cx.schema)
                });
            }
            Arm::Otherwise { body, .. } => {
                if let (Some(p), true) = (&subject, unset_taken) {
                    branch.available.insert(p.clone());
                }
                walk_nodes(body, cx, &mut branch, diags, reads);
            }
        }
        arm_finals.push(branch);
    }
    // Fold the arms' assignments into the surviving set iff the match is
    // exhaustive (a covered finite/nullable domain, or an `<otherwise>`): every
    // path then flows through exactly one arm, so the intersection of arm-final
    // sets is provably assigned afterward. A non-exhaustive match may match
    // nothing, so its pre-block set survives unchanged (dsl §9.4/§11.2).
    if !arm_finals.is_empty() && exhaustive {
        *flow = intersect_flows(arm_finals);
    }
}

/// A `<timeline>`: tracks nominally run in parallel; treat clip `::set`s as
/// writes and duration/set reads as reads, folded in stream order (conservative
/// for "was it ever set").
fn walk_timeline(
    tl: &Timeline,
    cx: &Scope<'_>,
    flow: &mut Flow,
    diags: &mut Vec<Diagnostic>,
    reads: &mut Vec<(String, Span)>,
) {
    if let Some(dur) = &tl.duration {
        check_reads(dur, cx, &flow.available, diags, reads);
    }
    for track in &tl.tracks {
        for clip in &track.clips {
            match &clip.node {
                ClipNode::Set(set) => walk_set(set, cx, flow, diags, reads),
                ClipNode::Directive(d) if d.tag == "use" => {
                    check_use_arg_reads(&d.attrs, cx, &flow.available, diags, reads);
                }
                ClipNode::Directive(_) => {}
            }
        }
    }
}

/// Evaluate a condition/guard slot: value reads are checked, then guard paths
/// (`has(p)`/`isSet(p)`) are added to the (arm-local) assigned set. Guard paths
/// are added AFTER checking reads so a guard never masks a value read of the
/// same slot; a guard only proves the path for the guarded body. A read its
/// own short-circuit protects ([`crate::cel_paths::PathUse::local`]) is proven
/// there.
fn apply_condition(
    slot: &CelSlot,
    cx: &Scope<'_>,
    assigned: &mut Assigned,
    diags: &mut Vec<Diagnostic>,
    reads: &mut Vec<(String, Span)>,
) {
    for u in slot_uses(slot, cx) {
        match u.role {
            PathRole::Read => check_read(&u, cx, assigned, diags, reads),
            PathRole::Guard => {
                // A guard on an undeclared path is a read-site concern (T4.3).
                if is_declared(&u.path, cx.schema) && !is_choicelog(&u.path) {
                    assigned.insert(u.path);
                }
            }
            // A non-dominating presence test (under `||`/`!`/`?:`) proves
            // nothing for the body; its short-circuit reads carry it locally.
            PathRole::WeakGuard => {}
        }
    }
}

/// Check every value read in `slot` (guards are ignored — they tolerate unset).
fn check_reads(
    slot: &CelSlot,
    cx: &Scope<'_>,
    assigned: &Assigned,
    diags: &mut Vec<Diagnostic>,
    reads: &mut Vec<(String, Span)>,
) {
    for u in slot_uses(slot, cx) {
        if u.role == PathRole::Read {
            check_read(&u, cx, assigned, diags, reads);
        }
    }
}

/// Classify one value read; emit `E-MAYBE-UNSET` for an unproven read. ALSO
/// records the read into `reads` (path + span) in the SAME branch — i.e.
/// exactly when a read falls back to entry state, is undefaulted, and would
/// earn `E-MAYBE-UNSET`. This is the read set connectivity T11's
/// `crate::envelope::check_envelope` reclassifies against a node's
/// PROJECT-WIDE `Env` (dsl §4.3) instead of re-deriving its own walk: a
/// path locally proven (write/guard, the `proven` check below) is THIS
/// pass's own concern, never the envelope's (soundness invariant, dsl §7 —
/// a path locally `::set` before the read stays defassign's problem). A
/// schema-defaulted path (`has_default`) is excluded from `reads` for the
/// same reason it never earns `E-MAYBE-UNSET`: `D ⊆ Guaranteed(X)` at
/// EVERY node (dsl §4.3 spec lines 442-457), so a defaulted read would
/// classify clean regardless — omitting it here changes no downstream
/// diagnostic, only avoids a redundant lookup.
fn check_read(
    u: &Use,
    cx: &Scope<'_>,
    assigned: &Assigned,
    diags: &mut Vec<Diagnostic>,
    reads: &mut Vec<(String, Span)>,
) {
    let path = u.path.as_str();
    // `run.choiceLog.*` reads are T4.3's territory; an undeclared (non-reserved)
    // path is ALSO T4.3's territory (`E-UNDECLARED`) -- but a reserved
    // `quest.<id>.*` read is always declared (dsl 0.2.0 §5.2, mirrors
    // `is_declared` below), so it falls through to `has_default`, which
    // treats every reserved quest shape as definite.
    if is_choicelog(path) || !is_declared(path, cx.schema) {
        return;
    }
    if has_default(path, cx.schema) || proven(path, assigned, &u.local, cx.schema) {
        return;
    }
    let through = u
        .via
        .as_deref()
        .map(|def| format!(", read through `@{def}`"))
        .unwrap_or_default();
    reads.push((path.to_string(), u.span));
    diags.push(diag(
        "E-MAYBE-UNSET",
        format!(
            "state path `{path}` may be read before it is set{through} \
             (no default, no dominating `::set`, no guard) (dsl §9.4)"
        ),
        u.span,
    ));
}

/// One state-path use at a use site: its role, the paths its enclosing
/// short-circuits prove there, where a diagnostic about it anchors, and the
/// `@def` it was read through (`None` for the site's own text).
struct Use {
    path: String,
    role: PathRole,
    local: Vec<String>,
    span: Span,
    via: Option<String>,
}

impl Use {
    /// A bare read of `path` (a `{{path}}`, a compound `::set` target).
    fn plain(path: String, span: Span) -> Self {
        Self {
            path,
            role: PathRole::Read,
            local: Vec::new(),
            span,
            via: None,
        }
    }
}

/// A slot's path uses, `@def`s expanded ([`uses_of`]).
fn slot_uses(slot: &CelSlot, cx: &Scope<'_>) -> Vec<Use> {
    uses_of(&slot.raw, slot.span, cx)
}

/// The path uses of `raw` — a CEL slot's text or a `{{@def}}` referent —
/// with every `@def` expanded (dsl 0.24.0: a def is a macro, so its body's
/// reads are reads at the use site), all anchored at `span`. A read the
/// site's own text does not make is attributed to the first top-level `@def`
/// whose expansion makes it. A slot whose defs cannot be expanded (a cycle, a
/// bodiless component param) is read as written; that failure is another
/// pass's diagnostic.
///
/// Re-parses into a fresh arena: the check entrypoint takes no arena, and per
/// T4.3 the AST is structure-only, so a throwaway parse yields identical
/// `Select`/`Ident` chains.
fn uses_of(raw: &str, span: Span, cx: &Scope<'_>) -> Vec<Use> {
    let refs: Vec<lute_cel::RefUse> = lute_cel::scan_refs(raw)
        .into_iter()
        .filter(|r| !r.is_dollar)
        .collect();
    let expanded = if refs.is_empty() {
        None
    } else {
        expand_cel(raw, &cx.defs, Some("$"), &mut Vec::new()).ok()
    };
    let Some(expanded) = expanded else {
        return parse_uses(raw)
            .into_iter()
            .map(|u| Use {
                path: u.path,
                role: u.role,
                local: u.local,
                span,
                via: None,
            })
            .collect();
    };
    let own: Vec<String> = parse_uses(raw).into_iter().map(|u| u.path).collect();
    // Top-level refs only: a ref nested in another's `(args)` expands with it.
    let calls: Vec<(usize, usize)> = refs
        .iter()
        .filter_map(|r| r.call.as_ref())
        .map(|c| (c.span.byte_start, c.span.byte_end))
        .collect();
    let top: Vec<(&str, Vec<String>)> = refs
        .iter()
        .filter(|r| {
            !calls
                .iter()
                .any(|&(s, e)| s <= r.span.byte_start && r.span.byte_end <= e)
        })
        .map(|r| {
            let end = r.call.as_ref().map_or(r.span.byte_end, |c| c.span.byte_end);
            let paths = expand_cel(&raw[r.span.byte_start..end], &cx.defs, Some("$"), &mut Vec::new())
                .map(|text| parse_uses(&text).into_iter().map(|u| u.path).collect())
                .unwrap_or_default();
            (r.name.as_str(), paths)
        })
        .collect();
    parse_uses(&expanded)
        .into_iter()
        .map(|u| {
            let via = (u.role == PathRole::Read && !own.contains(&u.path))
                .then(|| {
                    top.iter()
                        .find(|(_, paths)| paths.contains(&u.path))
                        .map(|(name, _)| (*name).to_string())
                })
                .flatten();
            Use {
                path: u.path,
                role: u.role,
                local: u.local,
                span,
                via,
            }
        })
        .collect()
}

/// The path uses of CEL text, or none when it does not parse (malformed CEL
/// is already reported in Phase 3).
fn parse_uses(text: &str) -> Vec<crate::cel_paths::PathUse> {
    if text.trim().is_empty() {
        return Vec::new();
    }
    let mut arena = CelArena::default();
    match lute_cel::parse_slot(&mut arena, text, 0) {
        Ok(handle) => arena
            .get(handle)
            .map(|root| collect_path_uses(&root.expr))
            .unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

/// A path is proven when some assigned entry — or some path its own
/// short-circuit proves (`local`) — is it or an ancestor of it (`run.x` proves
/// `run.x` and `run.x.hp`; a write to `run.x.a` does NOT prove the parent
/// `run.x`). dsl 0.24.0: `prev.run.*` is one snapshot, copied at run end from
/// every `run.*` value, and a `run.*` path with a `default` always holds a
/// value then — so once ANY `prev.run.*` path is proven present, a run has
/// ended and every `prev.run.<q>` whose `run.<q>` is defaulted is present.
fn proven(path: &str, assigned: &Assigned, local: &[String], schema: &StateSchema) -> bool {
    let covers = |a: &String| {
        path.strip_prefix(a.as_str())
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
    };
    if assigned.iter().any(covers) || local.iter().any(covers) {
        return true;
    }
    let Some(run_path) = path.strip_prefix("prev.") else {
        return false;
    };
    let snapshot_taken = assigned
        .iter()
        .chain(local)
        .any(|a| a.starts_with("prev.run."));
    snapshot_taken && run_path.starts_with("run.") && has_default(run_path, schema)
}

/// Whether evaluating `expr` from entry state may read an unset path: a
/// declared, undefaulted read its own short-circuits do not prove (a
/// `<match on="@def">` domain's `unset` case, dsl 0.24.0).
pub(crate) fn may_read_unset(expr: &cel_parser::ast::Expr, schema: &StateSchema) -> bool {
    collect_path_uses(expr).iter().any(|u| {
        u.role == PathRole::Read
            && !is_choicelog(&u.path)
            && is_declared(&u.path, schema)
            && !has_default(&u.path, schema)
            && !proven(&u.path, &Assigned::new(), &u.local, schema)
    })
}

/// A path is declared when it exactly matches a `state:` key or is a
/// descendant field of one (`run.player` declared => `run.player.hp` reads are
/// ok), OR is a RESERVED `quest.<id>.*` path (dsl 0.2.0 §5.2): those are
/// implicitly declared UNCONDITIONALLY, independent of whether THIS
/// document's own `<quest>` fold populated the schema (a foreign-quest read
/// is always legal, never `E-UNDECLARED`) — see `cel_resolve::is_declared`,
/// which applies the identical rule for T4.3's read-site check.
pub(crate) fn is_declared(path: &str, schema: &StateSchema) -> bool {
    is_reserved_quest_path(path)
        || is_reserved_entry_read(path)
        || schema
            .decls
            .keys()
            .any(|k| path == k || path.starts_with(&format!("{k}.")))
}

/// True when the schema decl that `path` resolves to (exact or nearest ancestor)
/// carries a `default` — the engine seeds it at scene entry (dsl §9.3) — OR
/// `path` is one of the two reserved quest shapes whose read is DEFINITE by
/// construction even when this document never locally folds a `<quest>` for
/// that id (a foreign-quest read must mirror the same synthetic decl the
/// owning document's own fold would produce):
///
/// * `quest.<id>.objectives.<oid>.done` (dsl 0.2.0 §5.2/§6.4) — `check_quest`
///   always seeds it with `default: false` (`match_check::check_quest`).
/// * `quest.<id>.activatedAt` (dsl 0.8.0 §5) — `Type::NarrativeTime` is
///   OPAQUE: no literal inhabits it, so it can carry no `default:`, AND the
///   only guard forms `check_read` accepts (`isSet(p)`/`has(p)`) are
///   themselves `E-TEMPORAL-ARG` on a narrative-time operand
///   ([`crate::temporal`], dsl 0.3.0 §6). A maybe-unset verdict here would
///   therefore be UNDISCHARGEABLE — every read of the anchor would error with
///   no author remedy — so the engine-populated anchor is treated as definite
///   and the `unset` case is left to the runtime, exactly as `validAt`'s own
///   semantics require. This is the one place `activatedAt` deliberately
///   diverges from `quest.<id>.state`.
/// * `quest.<id>.state` (0.21.1 T1-1) — an ALWAYS-ASSIGNED lifecycle enum,
///   `unset | active | complete | failed`: the engine writes `unset` for every
///   quest before activation, so a read is definite and `'unset'` is a value
///   (`crate::cel_paths::is_reserved_quest_state`). This used to be
///   `E-MAYBE-UNSET` on every read, with a message about `::set`.
/// * `quest.<id>.failedBy` / `quest.<id>.objectives.<oid>.failed` (dsl
///   0.24.0 §2) — engine-derived and always assigned: `failedBy` holds
///   `unset` until the quest fails, `failed` is `false` until the objective
///   fails.
///
/// `entry.<id>.read` (dsl 0.19.0 §5) always defaults to
/// `false` — the decl `crate::lore::entry_read_decl` folds for a local entry
/// — so a foreign entry's flag is definite too.
fn has_default(path: &str, schema: &StateSchema) -> bool {
    is_reserved_quest_objective_done(path)
        || is_reserved_quest_activated_at(path)
        || is_reserved_quest_state(path)
        || crate::cel_paths::is_reserved_quest_failed_by(path)
        || crate::cel_paths::is_reserved_quest_objective_failed(path)
        || is_reserved_entry_read(path)
        || schema
            .decls
            .iter()
            .filter(|(k, _)| path == k.as_str() || path.starts_with(&format!("{k}.")))
            .any(|(_, decl)| decl.default.is_some())
}

fn is_choicelog(path: &str) -> bool {
    path == "run.choiceLog" || path.starts_with("run.choiceLog.")
}

/// Intersection of every arm's assigned-after set (a path survives only if
/// assigned on every arm). Never called with an empty vec.
fn intersect_all(mut sets: Vec<Assigned>) -> Assigned {
    let mut acc = sets.pop().unwrap_or_default();
    for s in sets {
        acc.retain(|p| s.contains(p));
    }
    acc
}

/// Intersection of every arm's `Flow` — BOTH lattices fold under the
/// IDENTICAL join rule (`intersect_all`), so `writes` only ever grows via a
/// path `::set`/persisted on EVERY arm, exactly mirroring how `available`
/// folds. Never called with an empty vec.
fn intersect_flows(flows: Vec<Flow>) -> Flow {
    let (available, writes): (Vec<Assigned>, Vec<Assigned>) =
        flows.into_iter().map(|f| (f.available, f.writes)).unzip();
    Flow {
        available: intersect_all(available),
        writes: intersect_all(writes),
    }
}

/// Build a `Layer::Logic` error diagnostic (def-assignment is a §9 logic check).
fn diag(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lute_cel::fill_document;
    use lute_syntax::parse;

    /// Build `(nodes, schema)` from a `.lute` snippet: parse the DSL, fill every
    /// CEL slot's AST, and lift the inline `state:` schema from frontmatter.
    fn fixture(src: &str) -> (Vec<Node>, StateSchema) {
        let (mut doc, _pd) = parse(src);
        let mut arena = CelArena::default();
        let _ = fill_document(&mut arena, &mut doc);
        let (meta, _md) = crate::parse_meta(
            &doc.meta,
            &lute_manifest::snapshot::CapabilitySnapshot::default(),
        );
        let nodes = doc
            .shots
            .into_iter()
            .next()
            .map(|s| s.body)
            .unwrap_or_default();
        (nodes, meta.state)
    }
    /// A `StateSchema` declaring exactly `path`, typed and WITHOUT a default —
    /// the shape `E-MAYBE-UNSET` exists for.
    fn schema_with_undefaulted(path: &str) -> StateSchema {
        let src = format!(
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  \
             {path}: {{ type: string }}\n---\n## Shot 1.\n"
        );
        fixture(&src).1
    }

    /// A condition [`CelSlot`] over `raw`. `slot_uses` re-parses from `raw`, so
    /// the slot needs nothing but its text and its span.
    fn condition_slot(raw: &str) -> CelSlot {
        CelSlot::raw(
            lute_syntax::ast::CelKind::Condition,
            raw.to_string(),
            lute_core_span::Span {
                byte_start: 0,
                byte_end: raw.len(),
                line: 1,
                column: 1,
                utf16_range: (0, 0),
            },
        )
    }

    /// 0.10.0 §12.2: intra-expression `&&` narrowing runs in EVERY CEL slot.
    /// A quest-entry predicate has no guarded body to prove into — that is why
    /// `check_reads` was chosen over `apply_condition` — but a guard to the LEFT
    /// of `&&` in the SAME expression dominates the read to its right, and that
    /// is a property of the expression, not of the slot.
    #[test]
    fn quest_guard_narrows_within_the_expression() {
        let schema = schema_with_undefaulted("run.ending");
        let slot = condition_slot("isSet(run.ending) && run.ending == 'a'");
        let diags = check_quest_guard_defassign(&slot, &Scope::bare(&schema));
        assert!(
            diags.is_empty(),
            "`isSet(p) && …` is ok in every slot as of §12.2; got {:?}",
            diags.iter().map(|d| &d.code).collect::<Vec<_>>()
        );
    }

    /// 0.21.1 T1-1: `quest.<id>.state` is always assigned (`unset` until the
    /// quest activates), so reading it — even comparing it to `'unset'` — is
    /// never `E-MAYBE-UNSET`. It used to be, on every read, local or foreign.
    #[test]
    fn quest_state_read_is_definite() {
        let schema = StateSchema::default();
        for raw in [
            "quest.q.state == 'active'",
            "quest.q.state == 'unset'",
            "quest.q.state != 'complete' && quest.q.state != 'failed'",
        ] {
            let diags = check_quest_guard_defassign(&condition_slot(raw), &Scope::bare(&schema));
            assert!(
                diags.iter().all(|d| d.code != "E-MAYBE-UNSET"),
                "{raw}: {diags:?}"
            );
        }
    }

    /// The quest-state exemption is scoped to `quest.<id>.state`. An ordinary
    /// undefaulted path keeps the definite-assignment error and message.
    #[test]
    fn ordinary_path_keeps_the_definite_assignment_message() {
        let schema = schema_with_undefaulted("run.ending");
        let slot = condition_slot("run.ending == 'a'");
        let diags = check_quest_guard_defassign(&slot, &Scope::bare(&schema));
        let d = diags
            .iter()
            .find(|d| d.code == "E-MAYBE-UNSET")
            .expect("one");
        assert!(
            d.message.contains("no dominating `::set`"),
            "got {}",
            d.message
        );
    }

    /// §12.2 relaxes; it does not blind. An UNGUARDED read in the same slot is
    /// still `E-MAYBE-UNSET`.
    #[test]
    fn quest_guard_still_reports_an_unguarded_read() {
        let schema = schema_with_undefaulted("run.ending");
        let slot = condition_slot("run.ending == 'a'");
        let diags = check_quest_guard_defassign(&slot, &Scope::bare(&schema));
        assert!(
            diags.iter().any(|d| d.code == "E-MAYBE-UNSET"),
            "an unguarded read is unchanged; got {diags:?}"
        );
    }

    /// A presence test under `||` proves nothing and must stay a `WeakGuard`:
    /// this is short-circuit narrowing, not "any isSet anywhere in the slot".
    #[test]
    fn quest_guard_does_not_narrow_under_or() {
        let schema = schema_with_undefaulted("run.ending");
        let slot = condition_slot("isSet(run.ending) || run.ending == 'a'");
        let diags = check_quest_guard_defassign(&slot, &Scope::bare(&schema));
        assert!(
            diags.iter().any(|d| d.code == "E-MAYBE-UNSET"),
            "`||` does not dominate its right operand; got {diags:?}"
        );
    }

    #[test]
    fn definite_assignment_returns_final_assigned_set() {
        // The end-of-document `Assigned` set is now returned alongside diags —
        // the envelope layer's `guaranteed()` (T8/§4.3) reuses this exact set.
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.x: { type: number }\n---\n## Shot 1.\n::set{run.x = 1}\n";
        let (nodes, schema) = fixture(src);
        let (errs, assigned, _reads) = check_definite_assignment(&nodes, &Scope::bare(&schema), None);
        assert!(errs.is_empty(), "unexpected diagnostics: {errs:?}");
        assert!(assigned.contains("run.x"));
    }

    #[test]
    fn run_path_no_default_read_is_maybe_unset() {
        // `run.metHelpfully` declared without a default; read in a guarded arm's
        // body with no prior `::set` and no guard on THIS path.
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.metHelpfully: { type: bool }\n  run.gate: { type: bool, default: false }\n---\n## Shot 1.\n<match on=\"run.gate\">\n<when test=\"run.gate\">\n::set{run.gate = run.metHelpfully}\n</when>\n</match>\n";
        let (nodes, schema) = fixture(src);
        let (errs, _assigned, _reads) = check_definite_assignment(&nodes, &Scope::bare(&schema), None);
        assert!(
            errs.iter().any(|e| e.code == "E-MAYBE-UNSET"),
            "expected E-MAYBE-UNSET, got {errs:?}"
        );
    }

    #[test]
    fn dominating_write_proves_path() {
        // `::set{run.x = 1}` dominates the later read `run.x` in the `<when>` test.
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.x: { type: number }\n---\n## Shot 1.\n::set{run.x = 1}\n<match on=\"run.x\">\n<when test=\"run.x > 0\">\n@narrator: hi\n</when>\n</match>\n";
        let (nodes, schema) = fixture(src);
        let (errs, _assigned, _reads) = check_definite_assignment(&nodes, &Scope::bare(&schema), None);
        assert!(
            !errs.iter().any(|e| e.code == "E-MAYBE-UNSET"),
            "dominating write should prove the path, got {errs:?}"
        );
    }

    #[test]
    fn compound_assign_first_reads_old_value() {
        // `run.x += 1` reads the old value first; `run.x` has no default and no
        // prior write -> the old-value read is maybe-unset.
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.x: { type: number }\n---\n## Shot 1.\n::set{run.x += 1}\n";
        let (nodes, schema) = fixture(src);
        let (errs, _assigned, _reads) = check_definite_assignment(&nodes, &Scope::bare(&schema), None);
        assert!(
            errs.iter().any(|e| e.code == "E-MAYBE-UNSET"),
            "compound += reads old value, expected E-MAYBE-UNSET, got {errs:?}"
        );
    }

    // ---- dsl 0.23.1 (ashen N3): a match arm narrows its own subject ---------

    /// The E-MAYBE-UNSET messages of `body` over a maybe-unset enum `run.mood`
    /// and number `run.rival`, in source order — `<match on>` subject reads
    /// excluded (`check.rs` settles those by exhaustiveness).
    fn narrowing_errs(body: &str) -> Vec<String> {
        let src = format!(
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  \
             run.mood: {{ type: {{ enum: [calm, tense] }} }}\n  run.rival: {{ type: number }}\n---\n\
             ## Shot 1.\n{body}\n"
        );
        let (nodes, schema) = fixture(&src);
        let subjects: Vec<Span> = nodes
            .iter()
            .filter_map(|n| match n {
                Node::Match(m) => Some(m.subject.span),
                _ => None,
            })
            .collect();
        let (errs, _assigned, _reads) = check_definite_assignment(&nodes, &Scope::bare(&schema), None);
        errs.into_iter()
            .filter(|e| e.code == "E-MAYBE-UNSET" && !subjects.contains(&e.span))
            .map(|e| e.message)
            .collect()
    }

    #[test]
    fn is_arm_proves_its_subject_set() {
        let errs = narrowing_errs(
            "<match on=\"run.mood\">\n<when is=\"calm|tense\">\n@narrator: Mood {{run.mood}}.\n</when>\n\
             <when is=\"unset\">\n@narrator: none.\n</when>\n</match>",
        );
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn arms_after_an_unset_arm_see_the_subject_set() {
        let errs = narrowing_errs(
            "<match on=\"run.rival\">\n<when is=\"unset\">\n@narrator: none.\n</when>\n\
             <when test=\"$ > 0\">\n@narrator: Rival {{run.rival}}.\n</when>\n\
             <otherwise>\n@narrator: Low {{run.rival}}.\n</otherwise>\n</match>",
        );
        assert!(errs.is_empty(), "{errs:?}");
    }

    #[test]
    fn narrowing_stops_where_unset_can_still_arrive() {
        // No unset arm: `<otherwise>` may see unset. An `is="unset"` arm
        // with a `test` does not take every unset value. After the match the
        // subject is as unknown as before.
        let errs = narrowing_errs(
            "<match on=\"run.rival\">\n<when is=\"1..\">\n@narrator: {{run.rival}}.\n</when>\n\
             <otherwise>\n@narrator: Maybe {{run.rival}}.\n</otherwise>\n</match>\n\
             <match on=\"run.mood\">\n<when is=\"unset\" test=\"1 > 0\">\n@narrator: a.\n</when>\n\
             <otherwise>\n@narrator: {{run.mood}}.\n</otherwise>\n</match>\n\
             @narrator: After {{run.mood}}.",
        );
        assert_eq!(errs.len(), 3, "{errs:?}");
        assert!(errs[0].contains("`run.rival`"), "{errs:?}");
        assert!(errs[1].contains("`run.mood`") && errs[2].contains("`run.mood`"), "{errs:?}");
    }

    // ---- Finding 1: subject-guard leak (dsl §9.4) ---------------------------

    #[test]
    fn g1_subject_isset_guard_nonexhaustive_leaks() {
        // `<match on="isSet(run.x)">` is a SUBJECT guard; a non-exhaustive match
        // may fall through, so the subject guard must NOT prove `run.x` past the
        // block. A later read of `run.x` is therefore maybe-unset.
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.x: { type: number }\n  run.out: { type: number }\n---\n## Shot 1.\n<match on=\"isSet(run.x)\">\n<when test=\"true\">\n@narrator: hi\n</when>\n</match>\n::set{run.out = run.x}\n";
        let (nodes, schema) = fixture(src);
        let (errs, _assigned, _reads) = check_definite_assignment(&nodes, &Scope::bare(&schema), None);
        assert!(
            errs.iter().any(|e| e.code == "E-MAYBE-UNSET"),
            "subject isSet-guard must not prove run.x past a non-exhaustive match, got {errs:?}"
        );
    }

    #[test]
    fn g2_subject_has_guard_exhaustive_leaks() {
        // `<match on="has(run.x)">` with an `<otherwise>` is exhaustive, but no
        // arm writes `run.x`; the subject guard must NOT survive `intersect_all`.
        // A later read of `run.x` is maybe-unset.
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.x: { type: number }\n  run.out: { type: number }\n---\n## Shot 1.\n<match on=\"has(run.x)\">\n<when test=\"true\">\n@narrator: a\n</when>\n<otherwise>\n@narrator: b\n</otherwise>\n</match>\n::set{run.out = run.x}\n";
        let (nodes, schema) = fixture(src);
        let (errs, _assigned, _reads) = check_definite_assignment(&nodes, &Scope::bare(&schema), None);
        assert!(
            errs.iter().any(|e| e.code == "E-MAYBE-UNSET"),
            "subject has-guard must not survive intersect_all, got {errs:?}"
        );
    }

    // ---- Finding 2: scene.* read-before-write (dsl §9.4) --------------------

    #[test]
    fn j2_scene_read_before_write_is_maybe_unset() {
        // A non-defaulted `scene.s` read before any write follows ordinary
        // path-sensitive analysis (§9.4) -> maybe-unset.
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  scene.s: { type: number }\n  scene.out: { type: number }\n---\n## Shot 1.\n::set{scene.out = scene.s}\n";
        let (nodes, schema) = fixture(src);
        let (errs, _assigned, _reads) = check_definite_assignment(&nodes, &Scope::bare(&schema), None);
        assert!(
            errs.iter().any(|e| e.code == "E-MAYBE-UNSET"),
            "non-defaulted scene.s read before write should flag, got {errs:?}"
        );
    }

    #[test]
    fn j1_defaulted_scene_read_is_ok() {
        // A schema-defaulted `scene.d` read is seeded at scene entry -> no error.
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  scene.d: { type: number, default: 0 }\n  scene.out: { type: number }\n---\n## Shot 1.\n::set{scene.out = scene.d}\n";
        let (nodes, schema) = fixture(src);
        let (errs, _assigned, _reads) = check_definite_assignment(&nodes, &Scope::bare(&schema), None);
        assert!(
            !errs.iter().any(|e| e.code == "E-MAYBE-UNSET"),
            "defaulted scene.d read should be safe, got {errs:?}"
        );
    }

    #[test]
    fn scene_write_then_read_is_ok() {
        // A dominating `::set{scene.s = 1}` proves the later read -> no error.
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  scene.s: { type: number }\n  scene.out: { type: number }\n---\n## Shot 1.\n::set{scene.s = 1}\n::set{scene.out = scene.s}\n";
        let (nodes, schema) = fixture(src);
        let (errs, _assigned, _reads) = check_definite_assignment(&nodes, &Scope::bare(&schema), None);
        assert!(
            !errs.iter().any(|e| e.code == "E-MAYBE-UNSET"),
            "dominating scene write should prove the path, got {errs:?}"
        );
    }

    // ---- RevT8 P1 Fix 1: guard-proof vs. write (connectivity T8 review) ----

    #[test]
    fn g3_exhaustive_arm_guard_proves_read_but_never_enters_write_set() {
        // Two arms of an exhaustive match guard on `isSet(run.x)` (an
        // arm-level, dominating guard — NOT a subject guard) and neither
        // WRITES `run.x`; the `<otherwise>` writes it. `<otherwise>` makes the
        // match exhaustive (a guarded `is=` arm covers nothing it cannot
        // decide — 0.21.1 T1-12), so every path is either guard-proven or
        // written: a read after the match is satisfied (diagnostic behavior
        // unchanged). But the guard-proofs must NOT survive into the returned
        // WRITE-ONLY set — that write-only set is the envelope's `G`
        // (`crate::envelope::guaranteed`), which must never claim a path is
        // guaranteed WRITTEN when two of three arms never wrote it (RevT8 P1).
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.flag: { type: bool, default: false }\n  run.x: { type: number }\n  run.out: { type: number }\n---\n## Shot 1.\n<match on=\"run.flag\">\n<when is=\"true\" test=\"isSet(run.x)\">\n@narrator: a\n</when>\n<when is=\"false\" test=\"isSet(run.x)\">\n@narrator: b\n</when>\n<otherwise>\n::set{run.x = 1}\n</otherwise>\n</match>\n::set{run.out = run.x}\n";
        let (nodes, schema) = fixture(src);
        let (errs, assigned, _reads) = check_definite_assignment(&nodes, &Scope::bare(&schema), None);
        assert!(
            !errs.iter().any(|e| e.code == "E-MAYBE-UNSET"),
            "exhaustive arm-level isSet guard should still prove the later read, got {errs:?}"
        );
        assert!(
            !assigned.contains("run.x"),
            "a guard-proof with no write must not enter the guaranteed WRITE set, got {assigned:?}"
        );
    }

    // ---- RevT8 P1 Fix 2: `<choice into>` record sugar is an arm-flow write --

    #[test]
    fn record_sugar_write_satisfies_a_later_read() {
        // `<choice into="run.x">` (dsl 0.6.0 §2) is sugar for an appended
        // `::set{run.x = …}` on selection. A sole/unconditional choice always
        // runs, so its record write must satisfy a read AFTER the branch
        // exactly like an ordinary `::set` would — and land in the returned
        // guaranteed WRITE set. `into=` alone drives the record now.
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.x: { type: number }\n  run.out: { type: number }\n---\n## Shot 1.\n<branch id=\"b\">\n<choice id=\"c1\" label=\"L1\" into=\"run.x\" value=\"1\">\n@narrator: pick\n</choice>\n</branch>\n::set{run.out = run.x}\n";
        let (nodes, schema) = fixture(src);
        let (errs, assigned, _reads) = check_definite_assignment(&nodes, &Scope::bare(&schema), None);
        assert!(
            !errs.iter().any(|e| e.code == "E-MAYBE-UNSET"),
            "unconditional record should satisfy the later read (no false positive), got {errs:?}"
        );
        assert!(
            assigned.contains("run.x"),
            "record target should join the guaranteed WRITE set, got {assigned:?}"
        );
    }

    #[test]
    fn record_sugar_does_not_satisfy_a_read_inside_its_own_body() {
        // The record write is applied AFTER the choice body (mirrors the
        // engine appending the write on selection) — it must NOT retroactively
        // satisfy a read of the same path INSIDE that same body.
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.x: { type: number }\n  run.out: { type: number }\n---\n## Shot 1.\n<branch id=\"b\">\n<choice id=\"c1\" label=\"L1\" into=\"run.x\" value=\"1\">\n::set{run.out = run.x}\n</choice>\n</branch>\n";
        let (nodes, schema) = fixture(src);
        let (errs, _assigned, _reads) = check_definite_assignment(&nodes, &Scope::bare(&schema), None);
        assert!(
            errs.iter().any(|e| e.code == "E-MAYBE-UNSET"),
            "a read inside the recording choice's own body must still flag, got {errs:?}"
        );
    }

    #[test]
    fn exhaustive_record_on_every_branch_arm_enters_guaranteed_writes() {
        // Every `<branch>` arm records `run.x` (one guarded, one unconditional
        // so the branch is exhaustive) -> `run.x` must join the returned
        // guaranteed WRITE set, exactly like an exhaustive `::set`.
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.flag: { type: bool, default: false }\n  run.x: { type: number }\n---\n## Shot 1.\n<branch id=\"b\">\n<choice id=\"c1\" label=\"L1\" when=\"run.flag\" into=\"run.x\" value=\"1\">\n@narrator: a\n</choice>\n<choice id=\"c2\" label=\"L2\" into=\"run.x\" value=\"2\">\n@narrator: b\n</choice>\n</branch>\n";
        let (nodes, schema) = fixture(src);
        let (errs, assigned, _reads) = check_definite_assignment(&nodes, &Scope::bare(&schema), None);
        assert!(errs.is_empty(), "unexpected diagnostics: {errs:?}");
        assert!(
            assigned.contains("run.x"),
            "an exhaustive per-arm record should join the guaranteed WRITE set, got {assigned:?}"
        );
    }
}
