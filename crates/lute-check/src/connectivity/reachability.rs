use super::identity::scene_identity;
use super::*;
use crate::ProjectDoc;

use std::borrow::Cow;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{Arm, Assert, Node};
use lute_syntax::datalog::FactPattern;
use crate::check::CheckResult;
use crate::meta::{resolve_doc_kind, DocKind};
use crate::prereq::{atoms, PrereqFormula};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reachability {
    Reachable,
    Unreachable,
    Unknown,
}

/// dsl §4.1: a graph node has no satisfiable route from the project's
/// entry set — every path to it is provably blocked. UNLIKE
/// §4.2/§4.3/§4.4's envelope diagnostics, this one carries NO "under your
/// declared routes" hedge (design spec §2.6's one named exception): it is
/// a pure fact about the AUTHORED graph's own self-consistency (no route
/// exists in what you declared), never a claim about runtime engine
/// behavior — so the hedge would misrepresent it, not merely soften it.
pub const E_CONN_UNREACHABLE: &str = "E-CONN-UNREACHABLE";

/// dsl §4.1: a defensive cap on one node's `after` formula atom count — a
/// pragmatic guard against a pathological/degenerate formula, not the
/// primary soundness mechanism (the structural recursion itself is
/// already linear in formula size, design spec §2.4). [`MAX_FORMULA_ATOMS`]
/// (256) is generous for any realistic hand-authored `after` clause;
/// crossing it is itself a strong signal something degenerate (e.g.
/// machine-generated) reached the parser.
pub const E_CONN_FORMULA_TOO_COMPLEX: &str = "E-CONN-FORMULA-TOO-COMPLEX";

/// See [`E_CONN_FORMULA_TOO_COMPLEX`].
pub(super) const MAX_FORMULA_ATOMS: usize = 256;

pub(super) fn unreachable_diag(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: E_CONN_UNREACHABLE.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
        evidence: Some(lute_core_span::Evidence::Proven),
    }
}

pub(super) fn too_complex_diag(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: E_CONN_FORMULA_TOO_COMPLEX.to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
        evidence: Some(lute_core_span::Evidence::Proven),
    }
}

/// Task 6 (dsl §4.1): PROVABLE per-node reachability from the project's
/// entry set, one memoized pass over [`ConnGraph::topo_order`] — linear in
/// total formula size, no route enumeration (design spec §2.4).
///
/// `quest_ids` is the FULL declared `<quest id>` set for this resolved
/// project root (T4 [`quest_id_set`]) — spec-required so `completed(Q)`
/// consults every declared quest, not merely the `after`-opted-in subset
/// [`ConnGraph::nodes`] admits (Task 6 review): a declared PLAIN (no-`after`)
/// quest that is alive still reads `Reachable`, never a false `Unknown`.
///
/// `ambiguous_quest_ids` is every quest id with MORE THAN ONE declaration in
/// this root (Task 6 review-2) — a duplicate id might carry one dead and one
/// alive declaration (locally via [`unreachable_quest_ids`] OR structurally
/// via this very graph's own node reachability), and neither source can
/// pick the "right" one. Provable-only discipline demands `Unknown` for it,
/// checked BEFORE both the lifecycle and graph-reach checks below — so an
/// ambiguous id's OWN graph node (if one of its declarations opted into
/// `after`) still gets its own real memoized reachability in the returned
/// map, but every OTHER formula's `completed(Q)` reference to it reads
/// `Unknown` regardless.
///
/// `unreachable_quests` is the caller-supplied set of quest ids that are
/// THEMSELVES provably unable to complete (dsl 0.4.0 §5.3's
/// `E-QUEST-UNREACHABLE`/`E-OBJECTIVE-UNSATISFIABLE` signal) — a
/// `completed(Q)` atom reads `Q`'s own quest-lifecycle reachability, a
/// DIFFERENT engine from this graph's, so it is threaded in rather than
/// recomputed here (see [`unreachable_quest_ids`] for the real
/// project-wide extraction `lute-cli` wires in).
///
/// Recursion over each `Valid` formula (design spec §4.1, widened to
/// three-valued per the provable-only discipline):
/// - [`PrereqState::Absent`] ⇒ [`Reachability::Reachable`] — an absent
///   `after` is a graph ENTRY point, trivially reachable.
/// - [`PrereqState::Invalid`] ⇒ [`Reachability::Unknown`] — a malformed
///   formula already earns `E-CONN-PROFILE` once (T1); this pass must
///   never additionally GUESS reachable or unreachable for it.
/// - [`PrereqState::Valid`]`(f)`, recursing over `f`:
///   - `visited(Y)`: `Y` a known [`NodeId::Scene`] (else bundle beat,
///     [`NodeId::visited`]) ⇒ its own (already
///     memoized, since it always precedes this node in `topo_order`)
///     reachability; not a known node ⇒ `Unknown` (that miss is
///     `E-CONN-UNKNOWN-NODE`'s problem, T4 — it must never CASCADE into a
///     false `E-CONN-UNREACHABLE`).
///   - `completed(Q)` AND `active(Q)` (lang 0.8.0 — IDENTICAL here; see
///     below for why the weaker atom earns no weaker verdict), by
///     precedence:
///     1. `Q ∉ quest_ids` (undeclared) ⇒ `Unknown`.
///     2. `Q ∈ ambiguous_quest_ids` (>1 declaration) ⇒ `Unknown`.
///     3. `Q ∈ unreachable_quests` ⇒ `Unreachable` (quest lifecycle,
///        tracked OUTSIDE this graph).
///     4. `NodeId::Quest(Q) ∈ nodes` (an `after`-declaring quest already
///        memoized above) ⇒ its memoized reachability (TRANSITIVE).
///     5. else (a declared PLAIN quest, not unreachable) ⇒ `Reachable`.
///
///     Case 3 is the only one where `active` could conceivably be weaker,
///     and it is not: `unreachable_quests` carries
///     [`crate::reachability::E_QUEST_UNREACHABLE`], whose BOTH roots
///     (dsl 0.4.0 §5.3) already preclude the `active` state — a `start`
///     that decides false never activates the quest at all, and a `fail`
///     that decides true fails it at the first evaluation instant (0.2
///     §6.3 precedence), so it is never observably `active` either.
///     `Unreachable` is therefore PROVEN for `active(Q)` too, not guessed.
///   - `And`: `Unreachable` iff EITHER arm is `Unreachable` (checked
///     first — it dominates); else `Reachable` iff BOTH `Reachable`; else
///     `Unknown`.
///   - `Or`: `Reachable` iff EITHER arm is `Reachable` (checked first —
///     it dominates, even against an `Unreachable` other arm); else
///     `Unreachable` iff BOTH `Unreachable`; else `Unknown`.
///
/// - [`PrereqState::Anchored`]`(anchors)` ⇒ the `And` over the anchors of
///   the `Or` over each anchor's sources (a quest source through the same
///   ambiguous / `unreachable_quests` precedence), except that
///   `Unreachable` reads `Unknown` (see [`PrereqState::Anchored`]).
///
/// A node whose formula's flattened atom count exceeds
/// [`MAX_FORMULA_ATOMS`] earns [`E_CONN_FORMULA_TOO_COMPLEX`] instead of
/// being evaluated at all (its own reachability is `Unknown`).
///
/// [`E_CONN_UNREACHABLE`] fires ONLY for a node this pass computes
/// `Unreachable` — never for `Unknown` (provable-only, never a false
/// positive). A node missing from `topo_order` (graph has a cycle,
/// [`E_CONN_CYCLE`] already reported by T5) gets no reachability entry
/// and no diagnostic here.
pub fn check_reachability(
    g: &ConnGraph,
    quest_ids: &BTreeSet<String>,
    ambiguous_quest_ids: &BTreeSet<String>,
    unreachable_quests: &BTreeSet<String>,
) -> (BTreeMap<NodeId, Reachability>, Vec<(PathBuf, Diagnostic)>) {
    let mut reach: BTreeMap<NodeId, Reachability> = BTreeMap::new();
    let mut diags = Vec::new();

    for id in &g.topo_order {
        let Some(info) = g.nodes.get(id) else {
            continue;
        };
        let r = match &info.prereq {
            PrereqState::Absent => Reachability::Reachable,
            PrereqState::Invalid => Reachability::Unknown,
            PrereqState::Valid(f) => {
                let count = atoms(f).len();
                if count > MAX_FORMULA_ATOMS {
                    diags.push((
                        info.path.clone(),
                        too_complex_diag(
                            format!(
                                "{id}'s `after` formula has {count} atoms, over the \
                                 {MAX_FORMULA_ATOMS}-atom complexity cap"
                            ),
                            info.span,
                        ),
                    ));
                    Reachability::Unknown
                } else {
                    eval_reach(
                        f,
                        g,
                        &reach,
                        quest_ids,
                        ambiguous_quest_ids,
                        unreachable_quests,
                    )
                }
            }
            // Synthesized anchors never prove a quest dead: the engine may
            // accept it outside any `::accept` (dsl 0.24.0 §2), and `after`
            // stays the declared route (dsl 0.25.0 §4). Every anchor must
            // hold; one source of each suffices.
            PrereqState::Anchored(anchors) => {
                let source = |n: &NodeId| match n {
                    NodeId::Quest(q) if ambiguous_quest_ids.contains(q) => Reachability::Unknown,
                    NodeId::Quest(q) if unreachable_quests.contains(q) => Reachability::Unreachable,
                    _ => reach.get(n).copied().unwrap_or(Reachability::Unknown),
                };
                let all = anchors.iter().fold(Reachability::Reachable, |all, a| {
                    let any = a
                        .from
                        .iter()
                        .fold(Reachability::Unreachable, |any, n| or_reach(any, source(n)));
                    and_reach(all, any)
                });
                match all {
                    Reachability::Unreachable => Reachability::Unknown,
                    r => r,
                }
            }
        };
        if r == Reachability::Unreachable {
            diags.push((
                info.path.clone(),
                unreachable_diag(
                    format!("{id} has no satisfiable route from the project's entry set"),
                    info.span,
                ),
            ));
        }
        reach.insert(id.clone(), r);
    }

    (reach, diags)
}

/// Recurse [`check_reachability`]'s tri-state lattice directly over `f`'s
/// AST shape (never route enumeration — see [`check_reachability`]'s doc
/// comment for the full per-case rules). `reach` holds every node already
/// memoized earlier in `topo_order`.
pub(super) fn eval_reach(
    f: &PrereqFormula,
    g: &ConnGraph,
    reach: &BTreeMap<NodeId, Reachability>,
    quest_ids: &BTreeSet<String>,
    ambiguous_quest_ids: &BTreeSet<String>,
    unreachable_quests: &BTreeSet<String>,
) -> Reachability {
    match f {
        PrereqFormula::Visited(key) => {
            let target = NodeId::visited(key, &g.nodes);
            if g.nodes.contains_key(&target) {
                reach.get(&target).copied().unwrap_or(Reachability::Unknown)
            } else {
                Reachability::Unknown
            }
        }
        // Graph semantics are IDENTICAL for both quest-lifecycle atoms — see
        // `check_reachability`'s doc comment for why `unreachable_quests`
        // soundly covers `active(Q)` too (lang 0.8.0).
        PrereqFormula::Completed(id) | PrereqFormula::Active(id) => {
            // Unknown for two distinct reasons, same verdict: the id names no
            // declared quest at all, or it names more than one (ambiguous), so
            // no single node's reachability can answer for it.
            if !quest_ids.contains(id) || ambiguous_quest_ids.contains(id) {
                Reachability::Unknown
            } else if unreachable_quests.contains(id) {
                Reachability::Unreachable
            } else if g.nodes.contains_key(&NodeId::Quest(id.clone())) {
                reach
                    .get(&NodeId::Quest(id.clone()))
                    .copied()
                    .unwrap_or(Reachability::Unknown)
            } else {
                Reachability::Reachable
            }
        }
        PrereqFormula::And(l, r) => and_reach(
            eval_reach(
                l,
                g,
                reach,
                quest_ids,
                ambiguous_quest_ids,
                unreachable_quests,
            ),
            eval_reach(
                r,
                g,
                reach,
                quest_ids,
                ambiguous_quest_ids,
                unreachable_quests,
            ),
        ),
        PrereqFormula::Or(l, r) => or_reach(
            eval_reach(
                l,
                g,
                reach,
                quest_ids,
                ambiguous_quest_ids,
                unreachable_quests,
            ),
            eval_reach(
                r,
                g,
                reach,
                quest_ids,
                ambiguous_quest_ids,
                unreachable_quests,
            ),
        ),
    }
}

pub(super) fn and_reach(a: Reachability, b: Reachability) -> Reachability {
    if a == Reachability::Unreachable || b == Reachability::Unreachable {
        Reachability::Unreachable
    } else if a == Reachability::Reachable && b == Reachability::Reachable {
        Reachability::Reachable
    } else {
        Reachability::Unknown
    }
}

pub(super) fn or_reach(a: Reachability, b: Reachability) -> Reachability {
    if a == Reachability::Reachable || b == Reachability::Reachable {
        Reachability::Reachable
    } else if a == Reachability::Unreachable && b == Reachability::Unreachable {
        Reachability::Unreachable
    } else {
        Reachability::Unknown
    }
}

/// Every declared `<quest id>` occurring in MORE THAN ONE `<quest>`
/// declaration across `docs` (Task 6 review-2): a shared id already earns
/// its own `E-QUEST-ID-DUP` elsewhere, but [`check_reachability`]'s
/// `completed(Q)` needs this set SEPARATELY — an ambiguous id might carry
/// one dead declaration and one alive one (either locally, via
/// [`unreachable_quest_ids`], or structurally, via one declaration's own
/// opted-in graph-node reachability), and neither signal alone can pick
/// the "right" declaration. Provable-only discipline: `completed(Q)` for
/// an ambiguous `Q` is always `Unknown`, never guessed either way. An
/// empty id is skipped (that document's own `E-QUEST-ID-MISSING` problem).
pub fn ambiguous_quest_ids(docs: &[ProjectDoc<'_>]) -> BTreeSet<String> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for item in docs {
        let document = item.doc;
        for quest in &document.quests {
            if quest.id.is_empty() {
                continue;
            }
            *counts.entry(quest.id.clone()).or_insert(0) += 1;
        }
    }
    counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(id, _)| id)
        .collect()
}

/// T6/T7 wiring: every `<quest>` in `docs` whose declaration was flagged
/// [`crate::reachability::E_QUEST_UNREACHABLE`] (dsl 0.4.0 §5.3) by the
/// per-file `check()` pass on that SAME file — the exact set
/// [`check_reachability`] expects as its `unreachable_quests` parameter.
///
/// Matched by `Quest.span` (the diagnostic's own anchor —
/// `reachability.rs`'s `check_quest_reach` pushes `E-QUEST-UNREACHABLE` at
/// `quest.span` verbatim) rather than id text alone: two DIFFERENT quests
/// sharing an id (a separate `E-QUEST-ID-DUP` problem) must never be
/// conflated by this lookup. This span correspondence is exact (not a
/// heuristic) because `docs` and `file_results` are both derived from
/// parsing the SAME source text — a deterministic parse yields identical
/// byte spans every time.
///
/// A quest id with MORE THAN ONE declaration in `docs` ([`ambiguous_quest_ids`],
/// Task 6 review-2) is OMITTED here entirely, even when the SPECIFIC
/// matched declaration is itself flagged `E-QUEST-UNREACHABLE`: collapsing
/// per-declaration spans down to the shared string id would otherwise
/// wrongly mark a quest id "provably unreachable" when a DIFFERENT
/// declaration of that same id is alive — provable-only means an ambiguous
/// id's lifecycle is `Unknown`, never `Unreachable`, here.
///
/// `file_results` is the caller's own per-file `check()` output; a path in
/// `docs` this pass has no matching entry for (or a quest whose id is
/// empty — that quest's own `E-QUEST-ID-MISSING` problem) contributes
/// nothing, never a panic.
pub fn unreachable_quest_ids(
    docs: &[ProjectDoc<'_>],
    file_results: &[(PathBuf, CheckResult)],
) -> BTreeSet<String> {
    let ambiguous = ambiguous_quest_ids(docs);
    let mut out = BTreeSet::new();
    // First result per path — `find`'s answer, without a scan per document.
    let mut results: std::collections::HashMap<&Path, &CheckResult> =
        std::collections::HashMap::with_capacity(file_results.len());
    for (p, r) in file_results {
        results.entry(p.as_path()).or_insert(r);
    }
    for item in docs {
        let path = item.path;
        let document = item.doc;
        let Some(result) = results.get(path) else {
            continue;
        };
        for quest in &document.quests {
            if quest.id.is_empty() || ambiguous.contains(&quest.id) {
                continue;
            }
            let flagged = result.diagnostics.iter().any(|d| {
                d.code == crate::reachability::E_QUEST_UNREACHABLE && d.span == quest.span
            });
            if flagged {
                out.insert(quest.id.clone());
            }
        }
    }
    out
}

/// dsl 0.4.0 §4.2/§B: every relation name with at least one `::assert{R(…)}`
/// site inside a node this root's [`check_reachability`] pass did NOT prove
/// [`Reachability::Unreachable`] — the relation-level projection of
/// [`live_assert_sites`], read by `producible()`'s base case (c) (spec §4.2's
/// "a node that is `E-CONN-UNREACHABLE`-clean").
///
/// Callers MUST pre-scope `docs`/`reach`/`ambiguous_quest_ids`/
/// `unreachable_quests` to ONE resolved project root (`lute-cli`'s `by_root`
/// grouping) — an assert site in one root can never seed a relation in a
/// sibling root's `producible()` walk.
pub fn live_assert_relations(
    docs: &[ProjectDoc<'_>],
    reach: &BTreeMap<NodeId, Reachability>,
    ambiguous_quest_ids: &BTreeSet<String>,
    unreachable_quests: &BTreeSet<String>,
    effects: &crate::directive_facts::EffectDirectives,
) -> BTreeSet<String> {
    live_assert_sites(
        docs,
        reach,
        ambiguous_quest_ids,
        unreachable_quests,
        effects,
    )
    .into_iter()
    .filter(|(_, p)| !p.relation.is_empty())
    .map(|(_, p)| p.relation.clone())
    .collect()
}

/// Every asserted fact pattern — each `::assert` site and (dsl 0.27.0 §4)
/// each directive call's declared `effects.asserts`, resolved at the call
/// ([`collect_asserted`]) — with its document, inside a node this root's
/// [`check_reachability`] pass did NOT prove [`Reachability::Unreachable`] —
/// the reachability gate both `producible()` (relation names,
/// [`live_assert_relations`]) and the dsl 0.20.0 may set
/// (`crate::fact_env::MaySet`, ground facts) seed from. `Reachable` AND
/// `Unknown` both count: provable-only discipline demands an impossibility
/// be a PROVEN fact before a guard over it is flagged dead, so a node this
/// pass cannot resolve either way (`Unknown`, OR one this graph has no entry
/// for at all — e.g. inside an `E-CONN-CYCLE`, or a scene whose identity
/// triad this pass could not even compute) must never be treated as dead by
/// omission; only a node PROVABLY `Unreachable` excludes its assert sites.
///
/// A scene assert site's hosting `NodeId::Scene` is its own
/// [`canonical_episode_key`] (every scene is always a `ConnGraph` node —
/// `Absent` `after` included, so `reach` always has an entry UNLESS the
/// graph itself had a cycle). A quest-body assert site's hosting
/// `NodeId::Quest` mirrors [`check_reachability`]'s own `completed(Q)`
/// precedence (T6): ambiguous (2+ declarations) reads `Unknown`; a
/// caller-supplied `E-QUEST-UNREACHABLE` id reads `Unreachable`; an
/// `after`-declaring quest already has a memoized `reach` entry; a plain
/// (no-`after`) quest not otherwise dead defaults `Reachable`. A lore
/// entry's assert site (dsl 0.19.0 §4) has no hosting node at all — lore
/// documents are not part of the scene/quest graph, and the engine presents
/// an entry whenever it chooses — so it is never proven `Unreachable` and
/// always counts as live.
///
/// Parse-failed asserts (D13's empty-relation sentinel) are included; each
/// consumer skips them. Same root-scoping contract as
/// [`live_assert_relations`].
pub fn live_assert_sites<'d>(
    docs: &'d [ProjectDoc<'d>],
    reach: &BTreeMap<NodeId, Reachability>,
    ambiguous_quest_ids: &BTreeSet<String>,
    unreachable_quests: &BTreeSet<String>,
    effects: &crate::directive_facts::EffectDirectives,
) -> Vec<(&'d Path, Cow<'d, FactPattern>)> {
    let mut out = Vec::new();
    for item in docs {
        let path = item.path;
        let doc = item.doc;
        let mut sites = Vec::new();
        if resolve_doc_kind(&doc.meta).0 == Some(DocKind::Scene) {
            let node_reach =
                scene_identity(item.meta).and_then(|ident| reach.get(&NodeId::Scene(ident.key)).copied());
            if assert_site_is_live(node_reach) {
                for shot in &doc.sections {
                    collect_asserted(&shot.body, effects, &mut sites);
                }
            }
        }
        for quest in &doc.quests {
            let node_reach = if ambiguous_quest_ids.contains(&quest.id) {
                Some(Reachability::Unknown)
            } else if unreachable_quests.contains(&quest.id) {
                Some(Reachability::Unreachable)
            } else {
                Some(
                    reach
                        .get(&NodeId::Quest(quest.id.clone()))
                        .copied()
                        .unwrap_or(Reachability::Reachable),
                )
            };
            if assert_site_is_live(node_reach) {
                collect_asserted(&quest.body, effects, &mut sites);
            }
        }
        for entry in &doc.entries {
            collect_asserted(&entry.body, effects, &mut sites);
        }
        for beat in &doc.beats {
            collect_asserted(&beat.body, effects, &mut sites);
        }
        out.extend(sites.into_iter().map(|p| (path, p)));
    }
    out
}

/// Every fact pattern a node stream asserts: each `::assert`'s, then —
/// dsl 0.27.0 §4 — each directive call's declared `effects.asserts`,
/// resolved at the call ([`crate::directive_facts::collect_call_facts`]).
pub fn collect_asserted<'d>(
    nodes: &'d [Node],
    effects: &crate::directive_facts::EffectDirectives,
    out: &mut Vec<Cow<'d, FactPattern>>,
) {
    let mut asserts = Vec::new();
    collect_asserts(nodes, &mut asserts);
    out.extend(asserts.into_iter().map(|a| Cow::Borrowed(&a.pattern)));
    let mut calls = Vec::new();
    crate::directive_facts::collect_call_facts(nodes, effects, &mut calls);
    out.extend(
        calls
            .into_iter()
            .flat_map(|(_, f)| f.asserts)
            .map(Cow::Owned),
    );
}

/// Every `::assert{R(…)}` relation name, per document — the producer half of
/// the producer→consumer edge `lute scenario`'s facts section renders
/// (#15, T4.7).
///
/// Unlike [`live_assert_relations`] this applies NO reachability gate: it
/// answers "which file writes this relation", a question about the source
/// rather than about the route, so an assert on a currently-dead route is
/// still the answer to "where do I go to change this". Callers MUST pre-scope
/// `docs` to one resolved root.
pub fn assert_relations_per_doc(
    docs: &[ProjectDoc<'_>],
    effects: &crate::directive_facts::EffectDirectives,
) -> BTreeMap<PathBuf, BTreeSet<String>> {
    let mut out = BTreeMap::new();
    for item in docs {
        let path = item.path;
        let doc = item.doc;
        let mut sites = Vec::new();
        for shot in &doc.sections {
            collect_asserted(&shot.body, effects, &mut sites);
        }
        for quest in &doc.quests {
            collect_asserted(&quest.body, effects, &mut sites);
        }
        for entry in &doc.entries {
            collect_asserted(&entry.body, effects, &mut sites);
        }
        for beat in &doc.beats {
            collect_asserted(&beat.body, effects, &mut sites);
        }
        let rels: BTreeSet<String> = sites
            .into_iter()
            .filter(|p| !p.relation.is_empty())
            .map(|p| p.relation.clone())
            .collect();
        if !rels.is_empty() {
            out.insert(path.to_path_buf(), rels);
        }
    }
    out
}

/// See [`live_assert_relations`]: `None` (identity/graph unresolvable — a
/// malformed scene triad, or a node absent from a cyclic graph's `reach`
/// map) counts as live, exactly like `Some(Reachability::Unknown)` — only a
/// PROVEN [`Reachability::Unreachable`] excludes.
pub(super) fn assert_site_is_live(r: Option<Reachability>) -> bool {
    !matches!(r, Some(Reachability::Unreachable))
}

/// Recursively collect every `::assert` site of a node stream — mirrors
/// `reachability.rs`'s `walk_reach` recursion shape (match-arm /
/// branch-choice / hub-choice / on / objective bodies). Also the producer
/// half of `lute scenario knowledge` (dsl 0.23.0 §1).
pub fn collect_asserts<'d>(nodes: &'d [Node], out: &mut Vec<&'d Assert>) {
    for node in nodes {
        match node {
            Node::Assert(a) => out.push(a),
            Node::Match(m) => {
                for arm in &m.arms {
                    let body = match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => body,
                    };
                    collect_asserts(body, out);
                }
            }
            Node::Branch(b) => {
                for choice in &b.choices {
                    collect_asserts(&choice.body, out);
                }
            }
            Node::Hub(h) => {
                for b in h.bodies() {
                    collect_asserts(b, out);
                }
            }
            Node::On(o) => collect_asserts(&o.body, out),
            Node::Objective(o) => collect_asserts(&o.body, out),
            Node::Line(_)
            | Node::Directive(_)
            | Node::Set(_)
            | Node::Timeline(_)
            | Node::Retract(_) => {}
        }
    }
}
