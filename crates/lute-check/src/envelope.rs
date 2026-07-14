//! Per-document guaranteed (`G`) / possible (`P`) write sets (connectivity
//! layer T8, dsl §4.3 "§C effect summary"). `G` is [`defassign`]'s own
//! end-of-document `Assigned` set — the must-write join (`intersect_all`) it
//! already computes to drive `E-MAYBE-UNSET` — filtered to the two monotonic
//! tiers the envelope lattice tracks. `P` is an INDEPENDENT flat,
//! path-insensitive scan of every `::set`/persist-sugar target reachable
//! ANYWHERE in the node stream: every `<branch>`/`<match>`/`<hub>`/`<on>`/
//! `<objective>` arm counts, including the non-dominating (may-only) writes
//! `defassign`'s `intersect_all` deliberately drops from `G` — so `P` is a
//! strict superset-shaped view of `G` (`P ⊇ G`), never derived from it.
//!
//! ## Tier scope (dsl §9.1, §4.3)
//! Both `G` and `P` are filtered to `run.*`/`user.*` only. The envelope
//! lattice assumes MONOTONIC writes; `quest.<id>.*` is engine-reserved
//! scratch that MAY be cleared mid-run (dsl 0.2.0 §5), so it is deliberately
//! excluded, along with `scene.*`/`app.*` (out of scope for this analysis).
//!
//! ## `::assert`/`::retract` are out of scope
//! A `::assert{…}`/`::retract{…}` target is a relational FACT (dsl 0.3.0 §5),
//! not a scalar `state:` path — `Node::Assert`/`Node::Retract` carry no
//! `state:` path at all, so they never contribute to either set here.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{
    Arm, AttrValue, Branch, Choice, ClipNode, Hub, Match, Node, Objective, On, Quest, Timeline,
};

use crate::connectivity::{ConnGraph, NodeId, PrereqState};
use crate::defassign::{check_definite_assignment, Assigned};
use crate::meta::{Namespace, StateSchema};
use crate::prereq::PrereqFormula;

/// `true` when `path` resolves to the `run.*`/`user.*` tier — the two
/// monotonic namespaces the envelope lattice tracks (dsl §4.3). Every other
/// tier (`scene.*`/`app.*`/`quest.*`) and any non-state-path string returns
/// `false`. `pub`: connectivity T11's `lute-cli` reconciliation pass reuses
/// this exact filter (never a re-derived tier check) when deciding which
/// per-file `E-MAYBE-UNSET` diagnostics are even eligible for envelope
/// reclassification — an out-of-scope read must never be touched.
pub fn in_envelope_scope(path: &str) -> bool {
    matches!(
        crate::meta::namespace_of(path),
        Some(Namespace::Run) | Some(Namespace::User)
    )
}

fn insert_in_scope(out: &mut BTreeSet<String>, path: &str) {
    if in_envelope_scope(path) {
        out.insert(path.to_string());
    }
}

/// The guaranteed-write set `G` for a document (dsl §4.3): every path
/// [`crate::defassign::check_definite_assignment`] proved assigned on EVERY
/// execution path through the whole-document node stream, filtered to
/// `run.*`/`user.*`. Reuses `defassign`'s own computed set verbatim — no new
/// lattice, this is purely the tier filter.
pub fn guaranteed(assigned: &Assigned) -> BTreeSet<String> {
    assigned.iter().filter(|p| in_envelope_scope(p)).cloned().collect()
}

/// The possible-write set `P` for a document (dsl §4.3): every `::set` /
/// `<choice persist>` target reachable ANYWHERE in `nodes`, path-insensitively
/// (every branch/match/hub/on/objective arm counts, regardless of whether it
/// dominates), filtered to `run.*`/`user.*`. Independent of
/// `check_definite_assignment` — a write that occurs in only ONE arm of a
/// `<branch>`/`<match>`/`<hub>` (never proven by `intersect_all`, so absent
/// from `G`) still appears here.
pub fn possible_writes(nodes: &[Node]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    scan_nodes(nodes, &mut out);
    out
}

fn scan_nodes(nodes: &[Node], out: &mut BTreeSet<String>) {
    for node in nodes {
        match node {
            Node::Set(set) => insert_in_scope(out, &set.path),
            Node::Branch(branch) => scan_branch(branch, out),
            Node::Hub(hub) => scan_hub(hub, out),
            Node::Match(m) => scan_match(m, out),
            Node::On(on) => scan_on(on, out),
            Node::Objective(o) => scan_objective(o, out),
            Node::Timeline(tl) => scan_timeline(tl, out),
            // Content lines carry only `{{…}}` reads; directives carry no
            // state write; `::assert`/`::retract` target relational facts,
            // not `state:` paths (dsl 0.3.0 §5) — none contribute to `P`.
            Node::Line(_) | Node::Directive(_) | Node::Assert(_) | Node::Retract(_) => {}
        }
    }
}

fn scan_branch(branch: &Branch, out: &mut BTreeSet<String>) {
    for choice in &branch.choices {
        scan_choice(choice, out);
    }
}

fn scan_hub(hub: &Hub, out: &mut BTreeSet<String>) {
    for choice in &hub.choices {
        scan_choice(choice, out);
    }
}

fn scan_choice(choice: &Choice, out: &mut BTreeSet<String>) {
    scan_choice_persist(choice, out);
    scan_nodes(&choice.body, out);
}

fn scan_match(m: &Match, out: &mut BTreeSet<String>) {
    for arm in &m.arms {
        match arm {
            Arm::When { body, .. } | Arm::Otherwise { body, .. } => scan_nodes(body, out),
        }
    }
}

fn scan_on(on: &On, out: &mut BTreeSet<String>) {
    scan_nodes(&on.body, out);
}

fn scan_objective(o: &Objective, out: &mut BTreeSet<String>) {
    scan_nodes(&o.body, out);
}

fn scan_timeline(tl: &Timeline, out: &mut BTreeSet<String>) {
    for track in &tl.tracks {
        for clip in &track.clips {
            if let ClipNode::Set(set) = &clip.node {
                insert_in_scope(out, &set.path);
            }
        }
    }
}

/// A `<choice persist="run" into="run.<path>" [value=…]>` (dsl §11.1.1) is
/// EXACTLY a `::set{run.<path> = value}` appended to the arm when the choice
/// is selected — the engine materializes the write (`check_choice_persist`,
/// check.rs owns validating the sugar's well-formedness; this only extracts
/// the target for `P`). `persist` is REQUIRED to be `"run"` (Rule 1,
/// §11.1.1), so `into` is always `run.*` when present — the tier filter is
/// applied anyway for a malformed/unvalidated document.
fn scan_choice_persist(choice: &Choice, out: &mut BTreeSet<String>) {
    let persists = choice.attrs.iter().any(|a| a.key == "persist");
    if !persists {
        return;
    }
    if let Some(into) = choice.attrs.iter().find(|a| a.key == "into") {
        if let AttrValue::Str(path) = &into.value {
            insert_in_scope(out, path);
        }
    }
}

/// The guaranteed-write must-set for ONE body's node stream taken ALONE (dsl
/// §4.3): reruns T8's write-only definite-assignment pass
/// (`check_definite_assignment`) as if `nodes` were a whole document on its
/// own — so a `<branch>`/`<match>` nested in the body still intersects its
/// own arms exactly as `guaranteed`/`G` does for a whole document — then
/// applies the same `run.*`/`user.*` tier filter. Diagnostics (and the
/// fall-back-to-entry-state read set, connectivity T11) are discarded: the
/// body was already validated by the real `check_definite_assignment` pass
/// over the whole quest document; this is a read-only re-derivation of its
/// guaranteed-write set, not a second source of diagnostics.
fn body_guaranteed(nodes: &[Node], schema: &StateSchema) -> BTreeSet<String> {
    let (_diags, writes, _reads) = check_definite_assignment(nodes, schema);
    guaranteed(&writes)
}

/// `writesOnComplete(Q)` (dsl §4.3, spec lines 416/422-430):
/// `Atom completed(Q): G = P = writesOnComplete(Q)` — a SINGLE set doubles
/// as both the guaranteed AND possible side of the `completed(Q)` atom;
/// there is no separate possible-only variant. Union, across each REQUIRED
/// objective's body plus the `questComplete` `<on>` handler's body, of each
/// body's own guaranteed-write must-set (`body_guaranteed` — INTERSECT
/// within a body, since a body may itself contain a `<branch>`/`<match>`,
/// dsl §6.7). OPTIONAL objectives (`Objective::optional`) are skipped
/// before their body is ever walked — they need not fire for completion
/// (dsl 0.2.0 §6.4), so crediting their writes would be unsound. The
/// `questComplete` handler ALWAYS contributes, regardless of its own
/// `when` guard: spec line 428 treats it, exactly like every required
/// objective, as firing "independently and unconditionally-once on
/// completion" — that is the analysis's dominance assumption for this
/// atom, not the wider ECA dispatch grammar's `when` semantics elsewhere.
/// The bodies that qualify each fire unconditionally, so nothing narrows
/// between them — UNION across them. Filtered to `run.*`/`user.*` (same
/// tier scope as `guaranteed`/`possible_writes`; `body_guaranteed` already
/// applies it per body, so the union stays in scope for free). `Objective`/
/// `On` nodes only ever appear directly in a quest's own body, never nested
/// (grammar admission, dsl 0.2.0 §6.7 — mirrors `match_check::check_quest`'s
/// and `project_check`'s own top-level-only objective scans), so a single
/// top-level pass over `q.body` is exhaustive.
pub fn writes_on_complete(q: &Quest, schema: &StateSchema) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for node in &q.body {
        match node {
            Node::Objective(o) if !o.optional => out.extend(body_guaranteed(&o.body, schema)),
            Node::On(on) if on.event == "questComplete" => {
                out.extend(body_guaranteed(&on.body, schema));
            }
            _ => {}
        }
    }
    out
}

/// One graph node's computed envelope (dsl §4.3 propagation table, spec
/// lines 415-419): `guaranteed` — every `run.*`/`user.*` path proven set on
/// EVERY route reaching this node under its declared `after` graph;
/// `possible` — every path set on AT LEAST ONE such route. `guaranteed ⊆
/// possible` always holds by construction (§4.3 lines 458-463:
/// `guaranteed` licenses "no diagnostic", `possible` gates
/// `E-STATE-MAYBE-UNAVAILABLE`).
///
/// `D` (the project's schema-default `run.*`/`user.*` set, spec lines
/// 442-447) is UNIONED into BOTH sides of every node's envelope by
/// [`propagate`], AFTER the table computation below — including a
/// `completed(Q)`-only formula, whose own table entry (`G = P =
/// writesOnComplete(Q)`, spec line 416) does not mention `D` on its face —
/// because a schema-defaulted path is seeded at scene entry regardless of
/// which route reached this node (spec lines 445-447): `D ⊆ guaranteed(n)`
/// must hold at EVERY node, not just ones whose formula happens to route
/// through a `visited()` atom.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Env {
    pub guaranteed: BTreeSet<String>,
    pub possible: BTreeSet<String>,
}

/// Per-document effect data [`propagate`] needs to resolve `visited(Y)`/
/// `completed(Q)` atoms — pure data the caller (T11/T14 project wiring)
/// threads in from T8/T9's own passes; never recomputed here.
///
/// ## Contract: map KEY presence IS the resolvability signal
/// [`propagate`]'s taint check (see its doc comment) treats a MISSING key
/// as "this atom's effect source could not be resolved" — NOT as "resolved
/// to the empty set". The caller MUST insert an entry (even an empty pair/
/// empty set) for every scene/quest it successfully resolved, and MUST
/// OMIT the entry entirely for anything it could not resolve — an unknown
/// `visited`/`completed` target ([`crate::connectivity`]'s own
/// `E_CONN_UNKNOWN_NODE` concern) or an AMBIGUOUS quest id (2+
/// declarations, T6's `ambiguous_quest_ids`). Populating a key with a
/// guessed/partial write set for something unresolved would silently
/// under-approximate `possible` downstream.
#[derive(Clone, Debug, Default)]
pub struct PerDocEffects {
    /// T8's `(guaranteed, possible_writes)` pair for a SCENE's own
    /// document, keyed by the same canonical key its
    /// [`crate::connectivity::NodeId::Scene`] carries. A `visited(Y)` atom
    /// is tainted (see [`propagate`]) when `Y` has no entry here —
    /// INDEPENDENT of `g.nodes`/`g.edges`, since a resolved scene is
    /// always a graph node anyway (`assemble_graph` makes every scene a
    /// node unconditionally).
    pub scene: BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)>,
    /// T9's [`writes_on_complete`], keyed by quest id — the same id a
    /// `PrereqFormula::Completed`/[`crate::connectivity::NodeId::Quest`]
    /// carries. Present for every quest the caller resolved UNAMBIGUOUSLY,
    /// whether or not it declares `after` (`writesOnComplete` needs no
    /// `after` — deliberately NOT gated on `g.nodes`, since a plain quest
    /// with no `after` is a valid leaf dependency that is intentionally
    /// never a graph node, dsl §4.1/connectivity.rs `assemble_graph`'s own
    /// edge model). A `completed(Q)` atom is tainted when `Q` has no entry
    /// here — including an AMBIGUOUS quest id, which the caller MUST omit
    /// rather than pick one of its declarations' write sets arbitrarily.
    pub quest_writes_on_complete: BTreeMap<String, BTreeSet<String>>,
}

/// Compute every graph node's [`Env`] by memoized structural recursion over
/// its `after` formula, one linear pass over `g.topo_order` (dsl §4.3 spec
/// lines 411-419): `visited(Y)`/`completed(Q)` atoms, `&&`/`||`
/// composition, and the `Absent` entry base case `G = P = D`. Returns the
/// per-node envelope map alongside a TAINTED node set (envelope-specific —
/// see below, deliberately NOT [`crate::connectivity::check_reachability`]'s
/// `Unknown` classification).
///
/// ## Tainted nodes (soundness for `PrereqState::Invalid` + unresolvable atoms)
/// A node with a malformed `after` ([`PrereqState::Invalid`], already
/// reported once as `E-CONN-PROFILE`) has no well-defined envelope — its
/// `possible` set cannot be trusted to be complete. Rather than invent an
/// unbounded/"all paths possible" sentinel, this marks the node TAINTED and
/// gives it a placeholder `D`/`D` envelope (never trusted downstream). A
/// node is ALSO tainted when its formula contains an atom whose effect
/// source `per_doc` cannot resolve at all — `visited(Y)` with `Y` absent
/// from `per_doc.scene`, or `completed(Q)` with `Q` absent from
/// `per_doc.quest_writes_on_complete` (an unknown target, or a quest id
/// the caller deliberately omitted as ambiguous — see [`PerDocEffects`]'s
/// contract) — silently contributing nothing for such an atom would
/// UNDER-APPROXIMATE `possible`. And a node whose formula references an
/// ALREADY-tainted node — via ANY `visited`/`completed` atom, on EITHER
/// side of `&&` OR `||` — is tainted too. Every tainted node gets a
/// placeholder envelope rather than running the table math on an
/// unreliable/incomplete input. Callers (T11) MUST skip emitting
/// `E-STATE-MAYBE-UNAVAILABLE` for any node in the returned tainted set.
///
/// This is DELIBERATELY stricter than reachability's tri-state lattice:
/// `check_reachability`'s `Or` recovers a clean `Reachable` arm even when
/// the other is `Unknown` (dominance — ANY reachable route makes the node
/// reachable). The envelope's `||` UNIONS `possible` across BOTH arms
/// (spec line 418) — `visited(InvalidY) || visited(LiveZ)`'s `possible`
/// necessarily folds in `InvalidY`'s unreliable contribution, so the WHOLE
/// node must be tainted even though `check_reachability` would call it
/// `Reachable`. Taint propagation here must NOT reuse `check_reachability`'s
/// `Unknown` set.
///
/// Memoized: each node's `Env` is computed once, reusing already-computed
/// predecessor envelopes from `envs` — sound because every `visited`/
/// `completed` target that is itself a graph node precedes `n` in
/// `topo_order` ([`crate::connectivity::assemble_graph`]'s own edge
/// invariant). Linear in total formula size across the graph.
pub fn propagate(
    g: &ConnGraph,
    per_doc: &PerDocEffects,
    d: &BTreeSet<String>,
) -> (BTreeMap<NodeId, Env>, BTreeSet<NodeId>) {
    let mut envs: BTreeMap<NodeId, Env> = BTreeMap::new();
    let mut tainted: BTreeSet<NodeId> = BTreeSet::new();

    for id in &g.topo_order {
        let Some(info) = g.nodes.get(id) else { continue };
        let (mut env, is_tainted) = match &info.prereq {
            PrereqState::Absent => (Env::default(), false),
            PrereqState::Invalid => (Env::default(), true),
            PrereqState::Valid(f) => {
                if formula_tainted(f, per_doc, &tainted) {
                    (Env::default(), true)
                } else {
                    (eval_formula(f, per_doc, &envs), false)
                }
            }
        };
        env.guaranteed.extend(d.iter().cloned());
        env.possible.extend(d.iter().cloned());
        if is_tainted {
            tainted.insert(id.clone());
        }
        envs.insert(id.clone(), env);
    }

    (envs, tainted)
}

/// `true` iff `f` contains an atom that is itself unresolvable — a
/// `visited(Y)` with `Y` absent from `per_doc.scene`, or a `completed(Q)`
/// with `Q` absent from `per_doc.quest_writes_on_complete` (see
/// [`PerDocEffects`]'s contract: key ABSENCE is the resolvability signal,
/// deliberately never gated on `g.nodes` — a plain no-`after` quest is a
/// valid resolvable leaf that is intentionally never a graph node) — OR
/// references (via any `visited`/`completed` atom, through ANY `&&`/`||`
/// nesting) a node already in `tainted`. See [`propagate`]'s doc comment
/// for why this must NOT reuse `check_reachability`'s
/// `Or`-recovers-a-clean-arm logic — taint propagates through both
/// operators identically here.
fn formula_tainted(f: &PrereqFormula, per_doc: &PerDocEffects, tainted: &BTreeSet<NodeId>) -> bool {
    match f {
        PrereqFormula::Visited(key) => {
            !per_doc.scene.contains_key(key) || tainted.contains(&NodeId::Scene(key.clone()))
        }
        PrereqFormula::Completed(id) => {
            !per_doc.quest_writes_on_complete.contains_key(id) || tainted.contains(&NodeId::Quest(id.clone()))
        }
        PrereqFormula::And(l, r) | PrereqFormula::Or(l, r) => {
            formula_tainted(l, per_doc, tainted) || formula_tainted(r, per_doc, tainted)
        }
    }
}

/// The table computation itself (dsl §4.3 spec lines 415-418), assuming `f`
/// references no tainted node ([`propagate`] checks that first):
/// `visited(Y)` unions `Y`'s own memoized envelope with its document's T8
/// write sets; `completed(Q)` is `writesOnComplete(Q)` (T9) on both sides;
/// `&&` unions both sides; `||` intersects `guaranteed` but UNIONS
/// `possible` (a route through either arm still makes a write possible).
fn eval_formula(f: &PrereqFormula, per_doc: &PerDocEffects, envs: &BTreeMap<NodeId, Env>) -> Env {
    match f {
        PrereqFormula::Visited(key) => {
            let target = NodeId::Scene(key.clone());
            let mut env = envs.get(&target).cloned().unwrap_or_default();
            if let Some((doc_g, doc_p)) = per_doc.scene.get(key) {
                env.guaranteed.extend(doc_g.iter().cloned());
                env.possible.extend(doc_p.iter().cloned());
            }
            env
        }
        PrereqFormula::Completed(id) => {
            let writes = per_doc.quest_writes_on_complete.get(id).cloned().unwrap_or_default();
            Env { guaranteed: writes.clone(), possible: writes }
        }
        PrereqFormula::And(l, r) => {
            let el = eval_formula(l, per_doc, envs);
            let er = eval_formula(r, per_doc, envs);
            Env {
                guaranteed: el.guaranteed.union(&er.guaranteed).cloned().collect(),
                possible: el.possible.union(&er.possible).cloned().collect(),
            }
        }
        PrereqFormula::Or(l, r) => {
            let el = eval_formula(l, per_doc, envs);
            let er = eval_formula(r, per_doc, envs);
            Env {
                guaranteed: el.guaranteed.intersection(&er.guaranteed).cloned().collect(),
                possible: el.possible.union(&er.possible).cloned().collect(),
            }
        }
    }
}

/// The entry base case `D` (dsl §4.3 spec lines 442-448): every `run.*`/
/// `user.*` path in `schema` carrying a schema `default`. Reused verbatim
/// from the schema import/merge layer's own resolved [`StateSchema`] — a
/// caller (T11's `lute-cli` `check-project` wiring) unions this across
/// every document's own resolved schema in one resolved project root to
/// get the PROJECT-RESOLVED `D` [`propagate`] expects: "a `run.*`/`user.*`
/// path with a schema default is already seeded/assigned at scene entry by
/// `defassign`'s own existing rule (`has_default` in `defassign.rs`)" —
/// cross-schema default conflicts are already that layer's own job, so
/// this never re-derives or re-resolves anything, only filters+extracts.
pub fn schema_defaults(schema: &StateSchema) -> BTreeSet<String> {
    schema
        .decls
        .iter()
        .filter(|(path, decl)| decl.default.is_some() && in_envelope_scope(path))
        .map(|(path, _)| path.clone())
        .collect()
}

/// dsl §2.3/§4.3: the `E-STATE-MAYBE-UNAVAILABLE` envelope diagnostic
/// (error grade, `check_envelope`'s error branch, ships in `check-project`
/// BY DEFAULT). See [`check_envelope`].
pub const E_STATE_MAYBE_UNAVAILABLE: &str = "E-STATE-MAYBE-UNAVAILABLE";

fn maybe_unavailable_error(path: &str, span: Span) -> Diagnostic {
    Diagnostic {
        code: E_STATE_MAYBE_UNAVAILABLE.to_string(),
        severity: Severity::Error,
        message: format!(
            "state path `{path}` may be unavailable under your declared routes — no \
             declared `after` route sets it before this read (dsl §4.3)"
        ),
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

fn maybe_unavailable_warning(path: &str, span: Span) -> Diagnostic {
    Diagnostic {
        code: E_STATE_MAYBE_UNAVAILABLE.to_string(),
        severity: Severity::Warning,
        message: format!(
            "state path `{path}` is set under your declared routes on SOME routes \
             reaching this node, but not every one — not yet guaranteed (dsl §4.3)"
        ),
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// dsl §4.3:458-467 — the envelope diagnostic. Classifies every SCENE-node
/// read [`check_definite_assignment`] could not prove LOCALLY (its third
/// return value — a read that fell back to entry state, see that
/// function's own doc comment) against that node's project-wide [`Env`]
/// (from [`propagate`]):
///
/// - `P ∈ Guaranteed(X)` → no diagnostic — safe under EVERY declared route.
/// - `P ∉ Possible(X)` → [`E_STATE_MAYBE_UNAVAILABLE`], ERROR grade, ships
///   in `check-project` BY DEFAULT — no declared route ever sets `P`
///   before `X`.
/// - `P ∈ Possible(X) \ Guaranteed(X)` → [`E_STATE_MAYBE_UNAVAILABLE`],
///   WARNING grade, default-suppressed — set on SOME but not every
///   declared route reaching `X`. Callers (T11's `check-project` wiring)
///   MUST filter this out of the default project diagnostics themselves
///   (`Severity::Warning` vs `Severity::Error`) and route it to `lute
///   scenario envelope` (T14) instead — this function does not suppress
///   it, both grades are returned together so a caller never has to
///   re-derive the classification to recover the warning set.
///
/// ## Soundness invariant (dsl §7): why `reads_per_scene` must be T3's
/// "falls back to entry state" set, never every read
/// A path locally `::set`/guard-proven BEFORE the read within the SAME
/// document is [`check_definite_assignment`]'s own concern (`E-MAYBE-UNSET`
/// standalone), never this pass's — a scene `check()` reports clean
/// standalone for it regardless of the project's envelope, so classifying
/// it here too could newly error a file `check()` proved clean. A
/// schema-defaulted read is `∈ D ⊆ Guaranteed(X)` at every node (`D` is
/// unioned into both sides of every [`Env`] by [`propagate`]) — clean
/// either way, so it is harmless whether or not a caller includes it (T3's
/// own `check_read` excludes it as a matter of course). A read that DOES
/// fall back to entry state and ISN'T defaulted is EXACTLY what
/// `check_definite_assignment` flags `E-MAYBE-UNSET` standalone — so
/// erroring it here does NOT violate the invariant (single-file `check`
/// was never clean for it); reclassifying it as `∈ Guaranteed(X)` (via an
/// upstream project-proven route) only ever SUPPRESSES relative to
/// standalone, the same suppress-only direction the design spec's
/// `E-QUEST-ID-DUP` precedent already establishes (§5).
///
/// ## Tainted nodes skipped entirely
/// A node in `tainted` ([`propagate`]'s own taint set) has an unreliable
/// `D`/`D` placeholder [`Env`], never a real bound (see [`propagate`]'s doc
/// comment) — this function emits NO diagnostic for a tainted node's
/// reads, provable-only, exactly mirroring [`propagate`]'s own doc comment
/// instruction to callers.
///
/// ## Scene-only (Main clarification, connectivity T11)
/// `reads_per_scene` is keyed by [`NodeId::Scene`] canonical key ONLY —
/// quest reads stay [`crate::defassign::check_quest_guard_defassign`]'s
/// existing territory, unchanged; a quest's own `Env` (even an
/// `after`-opted-in one, dsl §4.4) is `lute scenario envelope`'s (T14)
/// INVENTORY surface, never a `check-project` diagnostic here (design spec
/// lines 37-45, 540-546). This function never looks at [`NodeId::Quest`]
/// at all.
pub fn check_envelope(
    g: &ConnGraph,
    envs: &BTreeMap<NodeId, Env>,
    tainted: &BTreeSet<NodeId>,
    reads_per_scene: &BTreeMap<String, Vec<(String, Span)>>,
) -> Vec<(PathBuf, Diagnostic)> {
    let mut out = Vec::new();
    for (key, reads) in reads_per_scene {
        let id = NodeId::Scene(key.clone());
        if tainted.contains(&id) {
            continue;
        }
        let (Some(env), Some(info)) = (envs.get(&id), g.nodes.get(&id)) else {
            continue;
        };
        for (path, span) in reads {
            if !in_envelope_scope(path) || env.guaranteed.contains(path) {
                continue;
            }
            let diag = if env.possible.contains(path) {
                maybe_unavailable_warning(path, *span)
            } else {
                maybe_unavailable_error(path, *span)
            };
            out.push((info.path.clone(), diag));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lute_cel::{fill_document, CelArena};
    use lute_syntax::parse;
    use crate::connectivity::NodeInfo;

    fn shot_nodes(src: &str) -> Vec<Node> {
        let (mut doc, _pd) = parse(src);
        let mut arena = CelArena::default();
        let _ = fill_document(&mut arena, &mut doc);
        doc.shots.into_iter().next().map(|s| s.body).unwrap_or_default()
    }

    fn quest_nodes(src: &str) -> Vec<Node> {
        let (mut doc, _pd) = parse(src);
        let mut arena = CelArena::default();
        let _ = fill_document(&mut arena, &mut doc);
        doc.quests.into_iter().next().map(|q| q.body).unwrap_or_default()
    }

    /// Mirrors `defassign::tests::fixture` — parses + fills CEL + lifts the
    /// inline `state:` schema, needed only by the `guaranteed()`/divergence
    /// tests below (`possible_writes` needs no schema).
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

    /// Like `fixture`, but returns the first `<quest>` declaration itself
    /// (not just its flattened node stream) plus the doc's `state:` schema —
    /// `writes_on_complete` needs the `Quest` struct to read
    /// `Objective::optional`/`On::event`/`On::when`, which the flat node
    /// list `quest_nodes` returns loses.
    fn quest_fixture(src: &str) -> (Quest, StateSchema) {
        let (mut doc, _pd) = parse(src);
        let mut arena = CelArena::default();
        let _ = fill_document(&mut arena, &mut doc);
        let (meta, _md) = crate::parse_meta(
            &doc.meta,
            &lute_manifest::snapshot::CapabilitySnapshot::default(),
        );
        let quest = doc
            .quests
            .into_iter()
            .next()
            .expect("quest_fixture src must declare a <quest>");
        (quest, meta.state)
    }


    #[test]
    fn possible_writes_collects_across_branch_hub_match_run_user_only() {
        // `run.a` (branch choice), `user.b` (nested match `<when>`) collected;
        // `scene.skip` (match `<otherwise>`), `quest.q1.d` (top-level `::set`)
        // excluded by the run/user tier filter.
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n<branch id=\"b\">\n<choice id=\"c1\" label=\"L1\">\n::set{run.a = 1}\n<match on=\"run.a\">\n<when test=\"run.a > 0\">\n::set{user.b = 2}\n</when>\n<otherwise>\n::set{scene.skip = 1}\n</otherwise>\n</match>\n</choice>\n<choice id=\"c2\" label=\"L2\">\n@narrator: skip\n</choice>\n</branch>\n<hub id=\"h\">\n<choice id=\"hc\" label=\"HL\">\n::set{run.c = 3}\n</choice>\n</hub>\n::set{quest.q1.d = 4}\n";
        let nodes = shot_nodes(src);
        let p = possible_writes(&nodes);
        assert_eq!(
            p,
            BTreeSet::from(["run.a".to_string(), "run.c".to_string(), "user.b".to_string()])
        );
    }

    #[test]
    fn possible_writes_excludes_assert_and_includes_persist_sugar() {
        // `<choice persist="run" into="run.p">` is sugar for a `::set` — counts.
        // `::assert{…}` targets a relational fact, not a state path — excluded.
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n<branch id=\"b\">\n<choice id=\"c1\" label=\"L1\" persist=\"run\" into=\"run.p\">\n::assert{ seen(x) }\n</choice>\n</branch>\n";
        let nodes = shot_nodes(src);
        let p = possible_writes(&nodes);
        assert_eq!(p, BTreeSet::from(["run.p".to_string()]));
    }

    #[test]
    fn possible_writes_collects_across_on_and_objective_arms() {
        let src = "---\nkind: quest\n---\n<quest id=\"q\">\n<on event=\"questComplete\">\n::set{run.done = 1}\n</on>\n<objective id=\"o\" done=\"run.done\">\n::set{user.progress = 1}\n</objective>\n</quest>\n";
        let nodes = quest_nodes(src);
        let p = possible_writes(&nodes);
        assert_eq!(
            p,
            BTreeSet::from(["run.done".to_string(), "user.progress".to_string()])
        );
    }

    #[test]
    fn guaranteed_filters_assigned_to_run_user_tier() {
        let mut assigned: Assigned = Assigned::new();
        assigned.insert("scene.s".to_string());
        assigned.insert("run.r".to_string());
        assigned.insert("user.u".to_string());
        assigned.insert("app.a".to_string());
        assigned.insert("quest.q1.state".to_string());
        assert_eq!(
            guaranteed(&assigned),
            BTreeSet::from(["run.r".to_string(), "user.u".to_string()])
        );
    }

    #[test]
    fn possible_writes_diverges_from_guaranteed_for_guarded_only_write() {
        // `run.a` is written in exactly ONE `<branch>` arm (guarded by
        // `run.flag`), never on the sibling `c2` arm — `intersect_all` drops
        // it from `check_definite_assignment`'s final `Assigned` set, so it
        // must be ABSENT from `guaranteed(G)`. `possible_writes` walks every
        // arm regardless of dominance, so `run.a` MUST still be present —
        // proving `P` captures may-only writes `G` deliberately discards.
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.flag: { type: bool, default: false }\n  run.a: { type: number }\n---\n## Shot 1.\n<branch id=\"b\">\n<choice id=\"c1\" label=\"L1\" when=\"run.flag\">\n::set{run.a = 1}\n</choice>\n<choice id=\"c2\" label=\"L2\">\n@narrator: skip\n</choice>\n</branch>\n";
        let (nodes, schema) = fixture(src);
        let (errs, assigned, _reads) = check_definite_assignment(&nodes, &schema);
        assert!(errs.is_empty(), "unexpected diagnostics: {errs:?}");

        let g = guaranteed(&assigned);
        let p = possible_writes(&nodes);
        assert!(
            !g.contains("run.a"),
            "guarded-only write must NOT survive intersect_all into G, got {g:?}"
        );
        assert!(
            p.contains("run.a"),
            "guarded-only write must still be a possible write in P, got {p:?}"
        );
        assert!(p.is_superset(&g), "P must be a superset of G, P={p:?} G={g:?}");
    }

    #[test]
    fn guaranteed_excludes_arm_guard_proof_with_no_matching_write() {
        // RevT8 P1: both arms of an exhaustive bool match guard-prove
        // `run.x` (`test="isSet(run.x)"`, arm-level, dominating) but NEITHER
        // writes it. Before the fix `defassign`'s mixed `Assigned` exported
        // this guard-proof into `G`, while `possible_writes` (writes only)
        // never saw it -> `G ⊄ P`. `run.x` must now be ABSENT from `G`, and
        // `P` must remain a superset (both empty for this path here).
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.flag: { type: bool, default: false }\n  run.x: { type: number }\n  run.out: { type: number }\n---\n## Shot 1.\n<match on=\"run.flag\">\n<when is=\"true\" test=\"isSet(run.x)\">\n@narrator: a\n</when>\n<when is=\"false\" test=\"isSet(run.x)\">\n@narrator: b\n</when>\n</match>\n::set{run.out = run.x}\n";
        let (nodes, schema) = fixture(src);
        let (errs, assigned, _reads) = check_definite_assignment(&nodes, &schema);
        assert!(
            errs.is_empty(),
            "guard-proven read should not flag E-MAYBE-UNSET, got {errs:?}"
        );

        let g = guaranteed(&assigned);
        let p = possible_writes(&nodes);
        assert!(
            !g.contains("run.x"),
            "a guard-proof with no write must NOT enter G, got {g:?}"
        );
        assert!(p.is_superset(&g), "P must be a superset of G, P={p:?} G={g:?}");
    }

    #[test]
    fn guaranteed_includes_persist_target_written_on_every_arm() {
        // RevT8 P1 Fix 2: `<choice persist>` is an arm-flow WRITE — an
        // exhaustive per-arm persist of `run.x` must join `G`, exactly like
        // an exhaustive `::set`, and `P` (which already counted persist
        // sugar) must stay a superset.
        let src = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  run.flag: { type: bool, default: false }\n  run.x: { type: number }\n---\n## Shot 1.\n<branch id=\"b\">\n<choice id=\"c1\" label=\"L1\" when=\"run.flag\" persist=\"run\" into=\"run.x\" value=\"1\">\n@narrator: a\n</choice>\n<choice id=\"c2\" label=\"L2\" persist=\"run\" into=\"run.x\" value=\"2\">\n@narrator: b\n</choice>\n</branch>\n";
        let (nodes, schema) = fixture(src);
        let (errs, assigned, _reads) = check_definite_assignment(&nodes, &schema);
        assert!(errs.is_empty(), "unexpected diagnostics: {errs:?}");

        let g = guaranteed(&assigned);
        let p = possible_writes(&nodes);
        assert!(
            g.contains("run.x"),
            "an exhaustive per-arm persist should join G, got {g:?}"
        );
        assert!(p.is_superset(&g), "P must be a superset of G, P={p:?} G={g:?}");
    }

    #[test]
    fn writes_on_complete_intersects_within_body_and_unions_across_bodies() {
        // `run.done` is written on BOTH `<branch>` arms of the required
        // objective's body — `body_guaranteed`'s own `intersect_all` walk
        // must keep it. `run.b` is written on only ONE arm (`c1`) — it must
        // NOT survive that same intersect. The unconditional `questComplete`
        // `<on>` body's `run.flag` write is a SEPARATE body — union, not
        // intersect, brings it into the result alongside `run.done`.
        let src = "---\nkind: quest\nstate:\n  run.done: { type: number }\n  run.b: { type: number }\n  run.flag: { type: number }\n---\n<quest id=\"q\">\n<objective id=\"o1\" done=\"run.done\">\n<branch id=\"br\">\n<choice id=\"c1\" label=\"L1\">\n::set{run.done = 1}\n::set{run.b = 1}\n</choice>\n<choice id=\"c2\" label=\"L2\">\n::set{run.done = 1}\n</choice>\n</branch>\n</objective>\n<on event=\"questComplete\">\n::set{run.flag = 1}\n</on>\n</quest>\n";
        let (q, schema) = quest_fixture(src);
        let w = writes_on_complete(&q, &schema);
        assert!(w.contains("run.done"), "both-arm write must be guaranteed, got {w:?}");
        assert!(w.contains("run.flag"), "unconditional questComplete body must union in, got {w:?}");
        assert!(!w.contains("run.b"), "one-arm-only write must NOT survive intersect, got {w:?}");
    }

    #[test]
    fn writes_on_complete_excludes_optional_objective() {
        // `o2` is OPTIONAL — its unconditional `run.opt` write must be
        // excluded from the union entirely; `o1` is required and still
        // contributes `run.req`.
        let src = "---\nkind: quest\nstate:\n  run.req: { type: number }\n  run.opt: { type: number }\n---\n<quest id=\"q\">\n<objective id=\"o1\" done=\"run.req\">\n::set{run.req = 1}\n</objective>\n<objective id=\"o2\" done=\"run.opt\" optional>\n::set{run.opt = 1}\n</objective>\n</quest>\n";
        let (q, schema) = quest_fixture(src);
        let w = writes_on_complete(&q, &schema);
        assert!(w.contains("run.req"), "required objective write must be present, got {w:?}");
        assert!(!w.contains("run.opt"), "optional objective write must be excluded, got {w:?}");
    }

    #[test]
    fn writes_on_complete_filters_to_run_user_tier() {
        // `quest.q.scratch` (required objective body) and `scene.bar`
        // (questComplete body) are both unconditional writes, but neither
        // tier is in envelope scope (dsl §4.3) — only `run.keep` may survive.
        let src = "---\nkind: quest\nstate:\n  run.keep: { type: number }\n---\n<quest id=\"q\">\n<objective id=\"o1\" done=\"run.keep\">\n::set{run.keep = 1}\n::set{quest.q.scratch = 1}\n</objective>\n<on event=\"questComplete\">\n::set{scene.bar = 1}\n</on>\n</quest>\n";
        let (q, schema) = quest_fixture(src);
        let w = writes_on_complete(&q, &schema);
        assert_eq!(w, BTreeSet::from(["run.keep".to_string()]));
    }

    #[test]
    fn writes_on_complete_includes_guarded_quest_complete_handler() {
        // `<on event="questComplete" when="run.flag">` still counts as
        // firing unconditionally-once on completion (dsl §4.3 spec line
        // 428) — the `when` gates the wider ECA dispatch grammar, not this
        // analysis's dominance assumption, so `run.g` IS guaranteed.
        let src = "---\nkind: quest\nstate:\n  run.flag: { type: number }\n  run.g: { type: number }\n---\n<quest id=\"q\">\n<on event=\"questComplete\" when=\"run.flag\">\n::set{run.g = 1}\n</on>\n</quest>\n";
        let (q, schema) = quest_fixture(src);
        let w = writes_on_complete(&q, &schema);
        assert!(w.contains("run.g"), "guarded questComplete write must still be guaranteed, got {w:?}");
    }

    #[test]
    fn writes_on_complete_includes_unconditional_quest_complete_handler() {
        // No `when` at all — the handler always fires on `questComplete`, so
        // its write IS guaranteed.
        let src = "---\nkind: quest\nstate:\n  run.g: { type: number }\n---\n<quest id=\"q\">\n<on event=\"questComplete\">\n::set{run.g = 1}\n</on>\n</quest>\n";
        let (q, schema) = quest_fixture(src);
        let w = writes_on_complete(&q, &schema);
        assert!(w.contains("run.g"), "unconditional questComplete write must be guaranteed, got {w:?}");
    }

    // ---- T10: propagate (Env{guaranteed, possible}) ------------------

    fn dummy_span() -> lute_core_span::Span {
        lute_core_span::Span {
            byte_start: 0,
            byte_end: 1,
            line: 1,
            column: 1,
            utf16_range: (0, 0),
        }
    }

    fn node(id: NodeId, prereq: PrereqState) -> NodeInfo {
        NodeInfo {
            id: id.clone(),
            path: std::path::PathBuf::from(format!("{id}.lute")),
            prereq,
            span: dummy_span(),
        }
    }

    /// `edges` is deliberately left empty — [`propagate`] only ever reads
    /// `g.nodes`/`g.topo_order` (mirrors `check_reachability`'s own usage);
    /// tests supply `topo_order` directly since fixtures are small enough
    /// to hand-order by construction.
    fn graph(infos: Vec<NodeInfo>, topo_order: Vec<NodeId>) -> ConnGraph {
        ConnGraph {
            nodes: infos.into_iter().map(|n| (n.id.clone(), n)).collect(),
            edges: BTreeMap::new(),
            topo_order,
        }
    }

    #[test]
    fn invalid_node_is_tainted_with_placeholder_d_env() {
        let d = BTreeSet::from(["run.def".to_string()]);
        let bad = NodeId::Scene("bad".into());
        let g = graph(vec![node(bad.clone(), PrereqState::Invalid)], vec![bad.clone()]);
        let (envs, tainted) = propagate(&g, &PerDocEffects::default(), &d);
        assert!(tainted.contains(&bad));
        assert_eq!(envs[&bad].guaranteed, d);
        assert_eq!(envs[&bad].possible, d);
    }

    #[test]
    fn visited_of_invalid_node_is_tainted() {
        let bad = NodeId::Scene("bad".into());
        let n = NodeId::Scene("n".into());
        let g = graph(
            vec![
                node(bad.clone(), PrereqState::Invalid),
                node(n.clone(), PrereqState::Valid(PrereqFormula::Visited("bad".into()))),
            ],
            vec![bad.clone(), n.clone()],
        );
        // `bad` has a real `per_doc.scene` entry (resolvable) — the ONLY
        // reason `n` must end up tainted here is that `bad` is ITSELF
        // tainted (its `PrereqState::Invalid`), isolating that rule from
        // the separate "unresolvable atom" rule tested below.
        let mut per_doc = PerDocEffects::default();
        per_doc.scene.insert("bad".to_string(), (BTreeSet::new(), BTreeSet::new()));
        let (_envs, tainted) = propagate(&g, &per_doc, &BTreeSet::new());
        assert!(tainted.contains(&bad));
        assert!(tainted.contains(&n), "a formula referencing a tainted node must itself be tainted");
    }

    #[test]
    fn visited_of_unresolvable_key_is_tainted() {
        // `n`'s formula references `visited("ghost")`, but `ghost` has NO
        // `per_doc.scene` entry at all (unknown/unresolved target,
        // E-CONN-UNKNOWN-NODE's own separate concern) — contributing
        // nothing for it would silently under-approximate `possible`, so
        // `n` must be tainted even though NO node's `PrereqState` is
        // `Invalid` anywhere in this graph.
        let n = NodeId::Scene("n".into());
        let g = graph(
            vec![node(n.clone(), PrereqState::Valid(PrereqFormula::Visited("ghost".into())))],
            vec![n.clone()],
        );
        let (_envs, tainted) = propagate(&g, &PerDocEffects::default(), &BTreeSet::new());
        assert!(tainted.contains(&n), "visited() of an unresolvable key must taint the node");
    }

    #[test]
    fn completed_of_unresolvable_quest_is_tainted() {
        // `n`'s formula references `completed("ghost_quest")`, but
        // `ghost_quest` has NO `quest_writes_on_complete` entry (unknown or
        // ambiguous quest id — [`PerDocEffects`]'s contract requires the
        // caller to omit it, never guess) — must taint, same rule as a
        // missing `visited` target.
        let n = NodeId::Scene("n".into());
        let g = graph(
            vec![node(n.clone(), PrereqState::Valid(PrereqFormula::Completed("ghost_quest".into())))],
            vec![n.clone()],
        );
        let (_envs, tainted) = propagate(&g, &PerDocEffects::default(), &BTreeSet::new());
        assert!(tainted.contains(&n), "completed() of an unresolvable quest id must taint the node");
    }

    #[test]
    fn or_with_unresolvable_arm_taints_even_with_a_clean_arm() {
        // Same shape as `or_with_one_invalid_arm_taints_the_whole_node`,
        // but the "bad" arm is unresolvable (missing `per_doc.scene` entry)
        // rather than `PrereqState::Invalid` — `||`'s UNION-both-arms
        // `possible` rule means a clean `live` arm must NOT let the node
        // recover; `live` itself stays untainted.
        let live = NodeId::Scene("live".into());
        let n = NodeId::Scene("n".into());
        let f = PrereqFormula::Or(
            Box::new(PrereqFormula::Visited("ghost".into())),
            Box::new(PrereqFormula::Visited("live".into())),
        );
        let g = graph(
            vec![node(live.clone(), PrereqState::Absent), node(n.clone(), PrereqState::Valid(f))],
            vec![live.clone(), n.clone()],
        );
        let mut per_doc = PerDocEffects::default();
        per_doc.scene.insert("live".to_string(), (BTreeSet::new(), BTreeSet::new()));
        let (_envs, tainted) = propagate(&g, &per_doc, &BTreeSet::new());
        assert!(!tainted.contains(&live), "live's own node must stay untainted");
        assert!(tainted.contains(&n), "|| with an unresolvable arm must taint the whole node, even with a clean arm");
    }

    #[test]
    fn or_with_one_invalid_arm_taints_the_whole_node() {
        // KEY divergence from `check_reachability`: `Or`'s dominance lets a
        // clean arm recover `Reachable` there, but the envelope's `||`
        // UNIONS `possible` across BOTH arms (spec line 418), so
        // `InvalidY`'s unreliable contribution always leaks into the whole
        // node's `possible` — it must be tainted even though `live`'s own
        // arm is untainted.
        let bad = NodeId::Scene("bad".into());
        let live = NodeId::Scene("live".into());
        let n = NodeId::Scene("n".into());
        let f = PrereqFormula::Or(
            Box::new(PrereqFormula::Visited("bad".into())),
            Box::new(PrereqFormula::Visited("live".into())),
        );
        let g = graph(
            vec![
                node(bad.clone(), PrereqState::Invalid),
                node(live.clone(), PrereqState::Absent),
                node(n.clone(), PrereqState::Valid(f)),
            ],
            vec![bad.clone(), live.clone(), n.clone()],
        );
        // Both `bad` and `live` have real `per_doc.scene` entries — the
        // ONLY reason `n` must end up tainted here is `bad`'s OWN
        // `PrereqState::Invalid`, isolating this rule from the separate
        // "unresolvable atom" rule (`or_with_unresolvable_arm_taints_even_
        // with_a_clean_arm`, above).
        let mut per_doc = PerDocEffects::default();
        per_doc.scene.insert("bad".to_string(), (BTreeSet::new(), BTreeSet::new()));
        per_doc.scene.insert("live".to_string(), (BTreeSet::new(), BTreeSet::new()));
        let (_envs, tainted) = propagate(&g, &per_doc, &BTreeSet::new());
        assert!(!tainted.contains(&live), "live's own arm must stay untainted");
        assert!(tainted.contains(&n), "|| with an invalid arm must taint the whole node");
    }

    #[test]
    fn and_with_one_invalid_arm_taints_the_whole_node() {
        let bad = NodeId::Scene("bad".into());
        let live = NodeId::Scene("live".into());
        let n = NodeId::Scene("n".into());
        let f = PrereqFormula::And(
            Box::new(PrereqFormula::Visited("bad".into())),
            Box::new(PrereqFormula::Visited("live".into())),
        );
        let g = graph(
            vec![
                node(bad.clone(), PrereqState::Invalid),
                node(live.clone(), PrereqState::Absent),
                node(n.clone(), PrereqState::Valid(f)),
            ],
            vec![bad.clone(), live.clone(), n.clone()],
        );
        let mut per_doc = PerDocEffects::default();
        per_doc.scene.insert("bad".to_string(), (BTreeSet::new(), BTreeSet::new()));
        per_doc.scene.insert("live".to_string(), (BTreeSet::new(), BTreeSet::new()));
        let (_envs, tainted) = propagate(&g, &per_doc, &BTreeSet::new());
        assert!(tainted.contains(&n), "&& with an invalid arm must taint the whole node");
    }


    #[test]
    fn completed_only_node_still_carries_d() {
        let d = BTreeSet::from(["run.def".to_string()]);
        let n = NodeId::Scene("n".into());
        let g = graph(
            vec![node(n.clone(), PrereqState::Valid(PrereqFormula::Completed("q".into())))],
            vec![n.clone()],
        );
        let mut per_doc = PerDocEffects::default();
        per_doc
            .quest_writes_on_complete
            .insert("q".to_string(), BTreeSet::from(["run.done".to_string()]));
        let (envs, tainted) = propagate(&g, &per_doc, &d);
        assert!(!tainted.contains(&n));
        let env = &envs[&n];
        assert!(env.guaranteed.contains("run.def"), "D must survive a completed(Q)-only node, got {:?}", env.guaranteed);
        assert!(env.guaranteed.contains("run.done"));
        assert_eq!(env.guaranteed, env.possible, "completed(Q): G=P=writesOnComplete(Q), D unioned into both");
    }

    #[test]
    fn or_intersects_guaranteed_union_possible() {
        let a = NodeId::Scene("a".into());
        let b = NodeId::Scene("b".into());
        let n = NodeId::Scene("n".into());
        let f = PrereqFormula::Or(
            Box::new(PrereqFormula::Visited("a".into())),
            Box::new(PrereqFormula::Visited("b".into())),
        );
        let g = graph(
            vec![
                node(a.clone(), PrereqState::Absent),
                node(b.clone(), PrereqState::Absent),
                node(n.clone(), PrereqState::Valid(f)),
            ],
            vec![a.clone(), b.clone(), n.clone()],
        );
        let mut per_doc = PerDocEffects::default();
        per_doc.scene.insert(
            "a".to_string(),
            (
                BTreeSet::from(["run.a".to_string(), "run.x".to_string()]),
                BTreeSet::from(["run.a".to_string(), "run.x".to_string()]),
            ),
        );
        per_doc.scene.insert(
            "b".to_string(),
            (
                BTreeSet::from(["run.b".to_string(), "run.x".to_string()]),
                BTreeSet::from(["run.b".to_string(), "run.x".to_string()]),
            ),
        );
        let (envs, tainted) = propagate(&g, &per_doc, &BTreeSet::new());
        assert!(!tainted.contains(&n));
        let env = &envs[&n];
        assert!(env.guaranteed.contains("run.x"), "in both arms");
        assert!(!env.guaranteed.contains("run.a"), "only one arm");
        assert!(!env.guaranteed.contains("run.b"), "only one arm");
        assert!(env.possible.contains("run.a") && env.possible.contains("run.b") && env.possible.contains("run.x"));
    }

    #[test]
    fn and_unions_guaranteed_and_possible() {
        let a = NodeId::Scene("a".into());
        let b = NodeId::Scene("b".into());
        let n = NodeId::Scene("n".into());
        let f = PrereqFormula::And(
            Box::new(PrereqFormula::Visited("a".into())),
            Box::new(PrereqFormula::Visited("b".into())),
        );
        let g = graph(
            vec![
                node(a.clone(), PrereqState::Absent),
                node(b.clone(), PrereqState::Absent),
                node(n.clone(), PrereqState::Valid(f)),
            ],
            vec![a.clone(), b.clone(), n.clone()],
        );
        let mut per_doc = PerDocEffects::default();
        per_doc.scene.insert(
            "a".to_string(),
            (BTreeSet::from(["run.a".to_string()]), BTreeSet::from(["run.a".to_string()])),
        );
        per_doc.scene.insert(
            "b".to_string(),
            (BTreeSet::from(["run.b".to_string()]), BTreeSet::from(["run.b".to_string()])),
        );
        let (envs, _tainted) = propagate(&g, &per_doc, &BTreeSet::new());
        let env = &envs[&n];
        assert!(env.guaranteed.contains("run.a") && env.guaranteed.contains("run.b"));
        assert!(env.possible.contains("run.a") && env.possible.contains("run.b"));
    }

    #[test]
    fn default_set_survives_every_node() {
        let d = BTreeSet::from(["run.def".to_string()]);
        let a = NodeId::Scene("a".into());
        let n = NodeId::Scene("n".into());
        let g = graph(
            vec![
                node(a.clone(), PrereqState::Absent),
                node(n.clone(), PrereqState::Valid(PrereqFormula::Visited("a".into()))),
            ],
            vec![a.clone(), n.clone()],
        );
        let mut per_doc = PerDocEffects::default();
        per_doc.scene.insert("a".to_string(), (BTreeSet::new(), BTreeSet::new()));
        let (envs, tainted) = propagate(&g, &per_doc, &d);
        assert!(tainted.is_empty(), "a fully-resolved graph must never taint");
        assert!(envs.values().all(|e| e.guaranteed.contains("run.def")));
        assert!(envs.values().all(|e| e.possible.contains("run.def")));
    }

    // ---- algebraic identity (spec lines 432-440), property-based -----

    /// Minimal deterministic PRNG (splitmix64) — this workspace has no
    /// `rand`/`proptest` dependency; good enough for a bounded, reproducible
    /// sweep over small formula ASTs.
    struct Rng(u64);
    impl Rng {
        fn next_u64(&mut self) -> u64 {
            self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = self.0;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        }
        fn next_range(&mut self, n: usize) -> usize {
            (self.next_u64() % (n as u64)) as usize
        }
        fn next_bool(&mut self) -> bool {
            self.next_u64() & 1 == 0
        }
    }

    fn random_formula(rng: &mut Rng, atom_names: &[&str], depth: usize) -> PrereqFormula {
        if depth == 0 || rng.next_range(3) == 0 {
            let name = atom_names[rng.next_range(atom_names.len())];
            return PrereqFormula::Visited(name.to_string());
        }
        let l = random_formula(rng, atom_names, depth - 1);
        let r = random_formula(rng, atom_names, depth - 1);
        if rng.next_bool() {
            PrereqFormula::And(Box::new(l), Box::new(r))
        } else {
            PrereqFormula::Or(Box::new(l), Box::new(r))
        }
    }

    fn random_atom_envs(
        rng: &mut Rng,
        atom_names: &[&str],
        universe: &[&str],
    ) -> BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)> {
        let mut out = BTreeMap::new();
        for name in atom_names {
            let mut p = BTreeSet::new();
            for path in universe {
                if rng.next_bool() {
                    p.insert((*path).to_string());
                }
            }
            let mut g = BTreeSet::new();
            for path in &p {
                if rng.next_bool() {
                    g.insert(path.clone());
                }
            }
            out.insert((*name).to_string(), (g, p));
        }
        out
    }

    /// Brute-force per-route reference (spec lines 432-440): an atom's own
    /// route family is exactly its two extremes `{G_atom, P_atom}` — every
    /// concrete route through it writes AT LEAST `G_atom` and AT MOST
    /// `P_atom`, so intersecting `{G_atom, P_atom}` recovers `G_atom` and
    /// unioning recovers `P_atom`. `X && Y`'s routes are the cross product
    /// `{aᵢ ∪ bⱼ}` (both fire unconditionally); `X || Y`'s are the plain
    /// union `routes(X) ∪ routes(Y)` (exactly one arm fires).
    fn bruteforce_routes(
        f: &PrereqFormula,
        atoms: &BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)>,
    ) -> Vec<BTreeSet<String>> {
        match f {
            PrereqFormula::Visited(key) | PrereqFormula::Completed(key) => {
                let (g_atom, p_atom) = atoms.get(key).cloned().unwrap_or_default();
                vec![g_atom, p_atom]
            }
            PrereqFormula::And(l, r) => {
                let rl = bruteforce_routes(l, atoms);
                let rr = bruteforce_routes(r, atoms);
                let mut out = Vec::with_capacity(rl.len() * rr.len());
                for a in &rl {
                    for b in &rr {
                        out.push(a.union(b).cloned().collect());
                    }
                }
                out
            }
            PrereqFormula::Or(l, r) => {
                let mut out = bruteforce_routes(l, atoms);
                out.extend(bruteforce_routes(r, atoms));
                out
            }
        }
    }

    /// Guaranteed = intersect ALL routes; Possible = union ALL routes.
    fn bruteforce_env(
        f: &PrereqFormula,
        atoms: &BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)>,
    ) -> (BTreeSet<String>, BTreeSet<String>) {
        let routes = bruteforce_routes(f, atoms);
        let mut guaranteed: Option<BTreeSet<String>> = None;
        let mut possible: BTreeSet<String> = BTreeSet::new();
        for r in &routes {
            possible.extend(r.iter().cloned());
            guaranteed = Some(match guaranteed {
                None => r.clone(),
                Some(acc) => acc.intersection(r).cloned().collect(),
            });
        }
        (guaranteed.unwrap_or_default(), possible)
    }

    fn graph_for_formula(atom_names: &[&str], f: PrereqFormula) -> (ConnGraph, NodeId) {
        let mut infos = Vec::new();
        let mut topo = Vec::new();
        for name in atom_names {
            let id = NodeId::Scene((*name).to_string());
            infos.push(node(id.clone(), PrereqState::Absent));
            topo.push(id);
        }
        let n = NodeId::Scene("n_test".to_string());
        infos.push(node(n.clone(), PrereqState::Valid(f)));
        topo.push(n.clone());
        (graph(infos, topo), n)
    }

    #[test]
    fn structural_recursion_equals_bruteforce_per_route() {
        let atom_names = ["x0", "x1", "x2"];
        let universe = ["run.p0", "run.p1", "run.p2", "run.p3"];
        let d = BTreeSet::new();
        for seed in 0..500u64 {
            let mut rng = Rng(seed.wrapping_mul(0x2545_F491_4F6C_DD1D).wrapping_add(1));
            let atom_envs = random_atom_envs(&mut rng, &atom_names, &universe);
            let f = random_formula(&mut rng, &atom_names, 3);

            let (expected_g, expected_p) = bruteforce_env(&f, &atom_envs);

            let mut per_doc = PerDocEffects::default();
            for (name, (g, p)) in &atom_envs {
                per_doc.scene.insert(name.clone(), (g.clone(), p.clone()));
            }
            let (graph, n) = graph_for_formula(&atom_names, f.clone());
            let (envs, tainted) = propagate(&graph, &per_doc, &d);
            assert!(!tainted.contains(&n), "seed {seed}: an all-`Absent`/`Valid` fixture must never taint");
            let env = &envs[&n];
            assert_eq!(env.guaranteed, expected_g, "seed {seed}: formula {f:?}");
            assert_eq!(env.possible, expected_p, "seed {seed}: formula {f:?}");
        }
    }
}
