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

use std::collections::BTreeSet;

use lute_cel::CelArena;
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{
    Arm, AttrValue, Branch, CelSlot, Choice, ClipNode, Hub, InterpKind, Match, Node, Objective,
    On, Set, Timeline,
};

use crate::cel_paths::{
    collect_path_uses, is_reserved_quest_objective_done, is_reserved_quest_path, is_state_path,
    PathRole,
};
use crate::meta::StateSchema;
// (no `Ctx` import — `check_definite_assignment`'s `_ctx` param was always
// dead; connectivity T11 dropped it so `lute-cli`'s project-wide envelope
// wiring needs no throwaway `Ctx`/`Env` construction at all.)

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
pub fn check_definite_assignment(
    nodes: &[Node],
    schema: &StateSchema,
) -> (Vec<Diagnostic>, Assigned, Vec<(String, Span)>) {
    let mut diags = Vec::new();
    let mut reads = Vec::new();
    let mut flow = Flow::default();
    walk_nodes(nodes, schema, &mut flow, &mut diags, &mut reads);
    (diags, flow.writes, reads)
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
pub fn exhaustive_match_subject_spans(nodes: &[Node], schema: &StateSchema) -> Vec<Span> {
    let mut spans = Vec::new();
    collect_exhaustive_spans(nodes, schema, &mut spans);
    spans
}

fn collect_exhaustive_spans(nodes: &[Node], schema: &StateSchema, spans: &mut Vec<Span>) {
    for node in nodes {
        match node {
            Node::Branch(b) => {
                for choice in &b.choices {
                    collect_exhaustive_spans(&choice.body, schema, spans);
                }
            }
            Node::Hub(h) => {
                for choice in &h.choices {
                    collect_exhaustive_spans(&choice.body, schema, spans);
                }
            }
            Node::On(o) => collect_exhaustive_spans(&o.body, schema, spans),
            Node::Objective(o) => collect_exhaustive_spans(&o.body, schema, spans),
            Node::Match(m) => {
                if crate::match_check::is_exhaustive(m, schema) {
                    spans.push(m.subject.span);
                }
                for arm in &m.arms {
                    match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                            collect_exhaustive_spans(body, schema, spans);
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

/// Definite-assignment for a quest's `start`/`fail` CEL guard (dsl 0.2.0 §6.3,
/// §9.4). These are evaluated at QUEST ENTRY — nothing dominates them (they are
/// the first thing the engine evaluates for the quest), so the assigned set
/// starts EMPTY, exactly like a fresh [`check_definite_assignment`] call. They
/// get the SAME read-role treatment a `<match on>` SUBJECT gets ([`walk_match`]
/// via [`check_reads`]): a value-read check only — `has(p)`/`isSet(p)` here
/// proves nothing (there is no guarded body for a quest-entry predicate to
/// prove into), so [`check_reads`], not [`apply_condition`], is reused. Quest
/// guards stay OUTSIDE the envelope's read-collection (connectivity T11, dsl
/// §4.4): only the diagnostics are returned — a quest guard's fall-back-to-
/// entry-state read set is never an envelope concern (Main clarification:
/// `E-STATE-MAYBE-UNAVAILABLE` classifies SCENE-node reads only; quest reads
/// stay this function's existing territory, unchanged).
pub fn check_quest_guard_defassign(slot: &CelSlot, schema: &StateSchema) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    let assigned = Assigned::new();
    let mut reads = Vec::new();
    check_reads(slot, schema, &assigned, &mut diags, &mut reads);
    diags
}

/// Forward-walk a node sequence, threading the assigned set through in order.
fn walk_nodes(
    nodes: &[Node],
    schema: &StateSchema,
    flow: &mut Flow,
    diags: &mut Vec<Diagnostic>,
    reads: &mut Vec<(String, Span)>,
) {
    for node in nodes {
        match node {
            Node::Set(set) => walk_set(set, schema, flow, diags, reads),
            Node::Branch(branch) => walk_branch(branch, schema, flow, diags, reads),
            Node::Match(m) => walk_match(m, schema, flow, diags, reads),
            Node::Timeline(tl) => walk_timeline(tl, schema, flow, diags, reads),
            Node::Hub(hub) => walk_hub(hub, schema, flow, diags, reads),
            // A `{{path}}` interpolation on a content line is a state READ at the
            // line's position (dsl §7.6, §9.4): give it the SAME definite-
            // assignment treatment as a guard / `::set` read — a maybe-unset path
            // (declared, no default, no dominating write, no guard) is
            // `E-MAYBE-UNSET`. `Ref`/`Reserved` interps carry no state path.
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
                    apply_condition(when, schema, &mut fork, diags, reads);
                    for interp in &line.interps {
                        if interp.kind == InterpKind::Path {
                            check_read(&interp.raw, schema, &fork, interp.span, diags, reads);
                        }
                    }
                }
                None => {
                    for interp in &line.interps {
                        if interp.kind == InterpKind::Path {
                            check_read(&interp.raw, schema, &flow.available, interp.span, diags, reads);
                        }
                    }
                }
            },
            Node::Directive(_) => {}
            Node::On(on) => walk_on(on, schema, flow, diags, reads),
            Node::Objective(o) => walk_objective(o, schema, flow, diags, reads),
            // Fact args are ground (entity ids / bools), never `state:` paths —
            // no definite-assignment read/write to track (0.3.0 T2; write
            // policy is Task 10).
            Node::Assert(_) | Node::Retract(_) => {}
        }
    }
}

/// A `::set{path op expr}` (dsl §7.3.4). The RHS reads are checked; a compound
/// op additionally reads the OLD target value; then the target is assigned —
/// into BOTH `flow.available` (read-satisfaction) and `flow.writes` (the
/// envelope guaranteed-write must-set): a `::set` is unconditionally a WRITE.
fn walk_set(
    set: &Set,
    schema: &StateSchema,
    flow: &mut Flow,
    diags: &mut Vec<Diagnostic>,
    reads: &mut Vec<(String, Span)>,
) {
    // RHS value reads (guards here don't gate the arm; only their unset-safety).
    check_reads(&set.expr, schema, &flow.available, diags, reads);

    let target = &set.path;
    if is_state_path(target) {
        // Compound assignment reads the old value first (dsl §9.4).
        if set.op != "=" {
            check_read(target, schema, &flow.available, set.span, diags, reads);
        }
        // The write target itself must be declared (T4.3 covers read sites; the
        // `::set` LHS path is this pass's responsibility).
        if !is_declared(target, schema) {
            let mut msg = format!("state path `{target}` is not declared in `state:` (dsl §9.4)");
            if let Some(sugg) = crate::cel_paths::nearest_declared_path(target, schema, 2) {
                msg.push_str(&format!(" — did you mean `{sugg}`?"));
            }
            diags.push(diag("E-UNDECLARED", msg, set.path_span));
        }
        // Assign regardless of declaredness so later reads don't cascade.
        flow.available.insert(target.clone());
        flow.writes.insert(target.clone());
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
    choice.attrs.iter().find(|a| a.key == "into").and_then(|into| match &into.value {
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
    schema: &StateSchema,
    flow: &mut Flow,
    diags: &mut Vec<Diagnostic>,
    reads: &mut Vec<(String, Span)>,
) {
    let mut arm_finals: Vec<Flow> = Vec::new();
    let mut has_unconditional = false;
    for choice in &branch.choices {
        let mut arm = flow.clone();
        match &choice.when {
            Some(cond) => apply_condition(cond, schema, &mut arm.available, diags, reads),
            None => has_unconditional = true,
        }
        // §7.6: a `{{path}}` in the choice LABEL is a READ at the point the choice
        // is OFFERED — after its own `when` guard proves (a guarded choice's label
        // shows only when the guard holds), so check against the post-guard arm.
        check_label_reads(&choice.label, schema, &arm.available, choice.span, diags, reads);
        walk_nodes(&choice.body, schema, &mut arm, diags, reads);
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
    schema: &StateSchema,
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
            apply_condition(cond, schema, &mut arm.available, diags, reads);
        }
        // Label reads (§7.6): checked against the post-guard arm, then discarded
        // with the rest of the fork.
        check_label_reads(&choice.label, schema, &arm.available, choice.span, diags, reads);
        walk_nodes(&choice.body, schema, &mut arm, diags, reads);
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
    schema: &StateSchema,
    flow: &Flow,
    diags: &mut Vec<Diagnostic>,
    reads: &mut Vec<(String, Span)>,
) {
    let mut arm = flow.clone();
    if let Some(cond) = &on.when {
        apply_condition(cond, schema, &mut arm.available, diags, reads);
    }
    walk_nodes(&on.body, schema, &mut arm, diags, reads);
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
    schema: &StateSchema,
    flow: &Flow,
    diags: &mut Vec<Diagnostic>,
    reads: &mut Vec<(String, Span)>,
) {
    let mut arm = flow.clone();
    check_reads(&o.done, schema, &arm.available, diags, reads);
    if let Some(cond) = &o.when {
        apply_condition(cond, schema, &mut arm.available, diags, reads);
    }
    walk_nodes(&o.body, schema, &mut arm, diags, reads);
}

/// Definite-assignment for a `<choice label>`'s `{{path}}` interpolations (dsl
/// §7.6, §9.4). Choice labels are String attrs (not in the AST like content-line
/// interps), so they are recovered via the shared [`crate::check::scan_label_interps`]
/// scan. Only `Path` interps carry a state path; a declared-but-maybe-unset label
/// read (no default, no dominating write, no guard) is `E-MAYBE-UNSET`. Undeclared
/// paths and `Ref`/`Reserved` interps are the cel-layer resolver's job (mirroring
/// content-line interps), so `check_read` no-ops on them here.
fn check_label_reads(
    label: &str,
    schema: &StateSchema,
    assigned: &Assigned,
    span: Span,
    diags: &mut Vec<Diagnostic>,
    reads: &mut Vec<(String, Span)>,
) {
    for interp in crate::check::scan_label_interps(label, span) {
        if interp.kind == InterpKind::Path {
            check_read(&interp.raw, schema, assigned, interp.span, diags, reads);
        }
    }
}

/// A `<match>`: the `on=` subject is checked for value-reads (it dominates every
/// arm) but its position is NOT treated as a proving guard — a subject
/// `has(p)`/`isSet(p)` must never add `p` to the block-surviving set or the arm
/// bases (that would leak an unproven path past a non-exhaustive fall-through and
/// survive `intersect_all` on exhaustive matches). Each `<when>`/`<otherwise>`
/// still forks; join = intersection only when an `<otherwise>` makes the match
/// exhaustive. Arm-level `<when test>` guards keep proving (see `apply_condition`).
fn walk_match(
    m: &Match,
    schema: &StateSchema,
    flow: &mut Flow,
    diags: &mut Vec<Diagnostic>,
    reads: &mut Vec<(String, Span)>,
) {
    // Subject is a value-read check only; subject-position guards do NOT prove.
    check_reads(&m.subject, schema, &flow.available, diags, reads);

    let mut arm_finals: Vec<Flow> = Vec::new();
    for arm in &m.arms {
        let mut branch = flow.clone();
        match arm {
            Arm::When { test, body, .. } => {
                apply_condition(test, schema, &mut branch.available, diags, reads);
                walk_nodes(body, schema, &mut branch, diags, reads);
            }
            Arm::Otherwise { body, .. } => {
                walk_nodes(body, schema, &mut branch, diags, reads);
            }
        }
        arm_finals.push(branch);
    }
    // Fold the arms' assignments into the surviving set iff the match is
    // exhaustive (a covered finite/nullable domain, or an `<otherwise>`): every
    // path then flows through exactly one arm, so the intersection of arm-final
    // sets is provably assigned afterward. A non-exhaustive match may match
    // nothing, so its pre-block set survives unchanged (dsl §9.4/§11.2).
    if !arm_finals.is_empty() && crate::match_check::is_exhaustive(m, schema) {
        *flow = intersect_flows(arm_finals);
    }
}

/// A `<timeline>`: tracks nominally run in parallel; treat clip `::set`s as
/// writes and duration/set reads as reads, folded in stream order (conservative
/// for "was it ever set").
fn walk_timeline(
    tl: &Timeline,
    schema: &StateSchema,
    flow: &mut Flow,
    diags: &mut Vec<Diagnostic>,
    reads: &mut Vec<(String, Span)>,
) {
    if let Some(dur) = &tl.duration {
        check_reads(dur, schema, &flow.available, diags, reads);
    }
    for track in &tl.tracks {
        for clip in &track.clips {
            if let ClipNode::Set(set) = &clip.node {
                walk_set(set, schema, flow, diags, reads);
            }
        }
    }
}

/// Evaluate a condition/guard slot: value reads are checked, then guard paths
/// (`has(p)`/`isSet(p)`) are added to the (arm-local) assigned set. Guard paths
/// are added AFTER checking reads so a guard never masks a value read of the
/// same slot; a guard only proves the path for the guarded body.
fn apply_condition(
    slot: &CelSlot,
    schema: &StateSchema,
    assigned: &mut Assigned,
    diags: &mut Vec<Diagnostic>,
    reads: &mut Vec<(String, Span)>,
) {
    for use_ in slot_uses(slot) {
        match use_.role {
            PathRole::Read => check_read(&use_.path, schema, assigned, slot.span, diags, reads),
            PathRole::Guard => {
                // A guard on an undeclared path is a read-site concern (T4.3).
                if is_declared(&use_.path, schema) && !is_choicelog(&use_.path) {
                    assigned.insert(use_.path);
                }
            }
            // A non-dominating presence test (under `||`/`!`) proves nothing.
            PathRole::WeakGuard => {}
        }
    }
}

/// Check every value read in `slot` (guards are ignored — they tolerate unset).
fn check_reads(
    slot: &CelSlot,
    schema: &StateSchema,
    assigned: &Assigned,
    diags: &mut Vec<Diagnostic>,
    reads: &mut Vec<(String, Span)>,
) {
    for use_ in slot_uses(slot) {
        if use_.role == PathRole::Read {
            check_read(&use_.path, schema, assigned, slot.span, diags, reads);
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
    path: &str,
    schema: &StateSchema,
    assigned: &Assigned,
    span: Span,
    diags: &mut Vec<Diagnostic>,
    reads: &mut Vec<(String, Span)>,
) {
    // `run.choiceLog.*` reads are T4.3's territory; an undeclared (non-reserved)
    // path is ALSO T4.3's territory (`E-UNDECLARED`) -- but a reserved
    // `quest.<id>.state`/`quest.<id>.objectives.<oid>.done` read is always
    // declared (dsl 0.2.0 §5.2, mirrors `is_declared` below), so it falls
    // through and participates in the maybe-unset proof like any other tier.
    if is_choicelog(path) || !is_declared(path, schema) {
        return;
    }
    if has_default(path, schema) || proven(path, assigned) {
        return;
    }
    reads.push((path.to_string(), span));
    diags.push(diag(
        "E-MAYBE-UNSET",
        format!(
            "state path `{path}` may be read before it is set \
             (no default, no dominating `::set`, no guard) (dsl §9.4)"
        ),
        span,
    ));
}

/// Reconstruct a slot's path uses by re-parsing its raw CEL into a fresh arena.
/// (The check entrypoint takes no arena; per T4.3 the AST is structure-only, so a
/// throwaway parse is sound and yields identical `Select`/`Ident` chains.)
fn slot_uses(slot: &CelSlot) -> Vec<crate::cel_paths::PathUse> {
    if slot.raw.trim().is_empty() {
        return Vec::new();
    }
    let mut arena = CelArena::default();
    match lute_cel::parse_slot(&mut arena, &slot.raw, slot.span.byte_start) {
        Ok(handle) => arena
            .get(handle)
            .map(|root| collect_path_uses(&root.expr))
            .unwrap_or_default(),
        Err(_) => Vec::new(), // malformed CEL already reported in Phase 3.
    }
}

/// A path is proven when some assigned entry is it or an ancestor of it
/// (`run.x` proves `run.x` and `run.x.hp`; a write to `run.x.a` does NOT prove
/// the parent `run.x`).
fn proven(path: &str, assigned: &Assigned) -> bool {
    assigned
        .iter()
        .any(|a| path == a || path.starts_with(&format!("{a}.")))
}

/// A path is declared when it exactly matches a `state:` key or is a
/// descendant field of one (`run.player` declared => `run.player.hp` reads are
/// ok), OR is a RESERVED `quest.<id>.*` path (dsl 0.2.0 §5.2): those are
/// implicitly declared UNCONDITIONALLY, independent of whether THIS
/// document's own `<quest>` fold populated the schema (a foreign-quest read
/// is always legal, never `E-UNDECLARED`) — see `cel_resolve::is_declared`,
/// which applies the identical rule for T4.3's read-site check.
fn is_declared(path: &str, schema: &StateSchema) -> bool {
    is_reserved_quest_path(path)
        || schema
            .decls
            .keys()
            .any(|k| path == k || path.starts_with(&format!("{k}.")))
}

/// True when the schema decl that `path` resolves to (exact or nearest ancestor)
/// carries a `default` — the engine seeds it at scene entry (dsl §9.3) — OR
/// `path` is the reserved `quest.<id>.objectives.<oid>.done` shape (dsl 0.2.0
/// §5.2/§6.4): `check_quest` always seeds that decl with `default: false`
/// (`match_check::check_quest`), so a read is DEFINITE even when this
/// document never locally folds a `<quest>` for that id (a foreign-quest
/// read must mirror the same synthetic decl the owning document's own fold
/// would produce). `quest.<id>.state` carries NO default (its `unset`
/// member is proven only via `<match>` exhaustiveness, dsl 0.2.0 §5.2) so it
/// is deliberately excluded here.
fn has_default(path: &str, schema: &StateSchema) -> bool {
    is_reserved_quest_objective_done(path)
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


    #[test]
    fn definite_assignment_returns_final_assigned_set() {
        // The end-of-document `Assigned` set is now returned alongside diags —
        // the envelope layer's `guaranteed()` (T8/§4.3) reuses this exact set.
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.x: { type: number }\n---\n## Shot 1.\n::set{run.x = 1}\n";
        let (nodes, schema) = fixture(src);
        let (errs, assigned, _reads) = check_definite_assignment(&nodes, &schema);
        assert!(errs.is_empty(), "unexpected diagnostics: {errs:?}");
        assert!(assigned.contains("run.x"));
    }

    #[test]
    fn run_path_no_default_read_is_maybe_unset() {
        // `run.metHelpfully` declared without a default; read in a guarded arm's
        // body with no prior `::set` and no guard on THIS path.
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.metHelpfully: { type: bool }\n  run.gate: { type: bool, default: false }\n---\n## Shot 1.\n<match on=\"run.gate\">\n<when test=\"run.gate\">\n::set{run.gate = run.metHelpfully}\n</when>\n</match>\n";
        let (nodes, schema) = fixture(src);
        let (errs, _assigned, _reads) = check_definite_assignment(&nodes, &schema);
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
        let (errs, _assigned, _reads) = check_definite_assignment(&nodes, &schema);
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
        let (errs, _assigned, _reads) = check_definite_assignment(&nodes, &schema);
        assert!(
            errs.iter().any(|e| e.code == "E-MAYBE-UNSET"),
            "compound += reads old value, expected E-MAYBE-UNSET, got {errs:?}"
        );
    }

    // ---- Finding 1: subject-guard leak (dsl §9.4) ---------------------------

    #[test]
    fn g1_subject_isset_guard_nonexhaustive_leaks() {
        // `<match on="isSet(run.x)">` is a SUBJECT guard; a non-exhaustive match
        // may fall through, so the subject guard must NOT prove `run.x` past the
        // block. A later read of `run.x` is therefore maybe-unset.
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.x: { type: number }\n  run.out: { type: number }\n---\n## Shot 1.\n<match on=\"isSet(run.x)\">\n<when test=\"true\">\n@narrator: hi\n</when>\n</match>\n::set{run.out = run.x}\n";
        let (nodes, schema) = fixture(src);
        let (errs, _assigned, _reads) = check_definite_assignment(&nodes, &schema);
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
        let (errs, _assigned, _reads) = check_definite_assignment(&nodes, &schema);
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
        let (errs, _assigned, _reads) = check_definite_assignment(&nodes, &schema);
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
        let (errs, _assigned, _reads) = check_definite_assignment(&nodes, &schema);
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
        let (errs, _assigned, _reads) = check_definite_assignment(&nodes, &schema);
        assert!(
            !errs.iter().any(|e| e.code == "E-MAYBE-UNSET"),
            "dominating scene write should prove the path, got {errs:?}"
        );
    }

    // ---- RevT8 P1 Fix 1: guard-proof vs. write (connectivity T8 review) ----

    #[test]
    fn g3_exhaustive_arm_guard_proves_read_but_never_enters_write_set() {
        // Both arms of an EXHAUSTIVE bool match guard on `isSet(run.x)` (an
        // arm-level, dominating guard — NOT a subject guard) but neither
        // WRITES `run.x`. The guard-proof must still satisfy a read after the
        // match (diagnostic behavior unchanged) but must NOT survive into the
        // returned WRITE-ONLY set — that write-only set is the envelope's `G`
        // (`crate::envelope::guaranteed`), which must never claim a path is
        // guaranteed WRITTEN when nothing ever wrote it (RevT8 P1).
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.flag: { type: bool, default: false }\n  run.x: { type: number }\n  run.out: { type: number }\n---\n## Shot 1.\n<match on=\"run.flag\">\n<when is=\"true\" test=\"isSet(run.x)\">\n@narrator: a\n</when>\n<when is=\"false\" test=\"isSet(run.x)\">\n@narrator: b\n</when>\n</match>\n::set{run.out = run.x}\n";
        let (nodes, schema) = fixture(src);
        let (errs, assigned, _reads) = check_definite_assignment(&nodes, &schema);
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
        let (errs, assigned, _reads) = check_definite_assignment(&nodes, &schema);
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
        let (errs, _assigned, _reads) = check_definite_assignment(&nodes, &schema);
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
        let (errs, assigned, _reads) = check_definite_assignment(&nodes, &schema);
        assert!(errs.is_empty(), "unexpected diagnostics: {errs:?}");
        assert!(
            assigned.contains("run.x"),
            "an exhaustive per-arm record should join the guaranteed WRITE set, got {assigned:?}"
        );
    }
}
