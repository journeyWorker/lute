use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::Node;
use crate::ProjectDoc;
pub const E_OBJECTIVE_UNSATISFIABLE_SUBQUEST: &str = "E-OBJECTIVE-UNSATISFIABLE";

use super::quest_refs::defined_quests;
use super::quest_tiers::{quest_tier, tier_mix_diag, subquest_rearm_diag};

/// dsl 2026-08-31 §4 (subquest design): the child quest a
/// `<objective quest="c">` names is not declared by any quest document in
/// the walked project. Errors, unlike `W_QUEST_REF_UNKNOWN` (which is a
/// warning on `quest.<id>.state`-style READS): a `quest=` reference IS the
/// tree — an unknown child leaves the parent objective with no completion
/// source, since the synthesized predicate `quest.c.state == 'complete'`
/// can never fire on a quest nothing ever activates.
pub const E_QUEST_REF_UNKNOWN: &str = "E-QUEST-REF-UNKNOWN";

/// dsl 2026-08-31 §4: one quest is referenced by `<objective quest=>` from
/// two DIFFERENT parent quests. Tree, not DAG (design table row "Shape").
pub const E_QUEST_MULTI_PARENT: &str = "E-QUEST-MULTI-PARENT";

/// dsl 2026-08-31 §4: parent→child edges close a cycle. Self-reference is
/// a length-1 cycle; §2/§4 note that when parent and child share a document
/// the per-file `check` catches it early — this pass is the cross-file
/// safety net (and re-derives the same-doc case incidentally).
pub const E_QUEST_TREE_CYCLE: &str = "E-QUEST-TREE-CYCLE";

/// One parent→child edge harvested from an `<objective quest="c">` — the
/// shared shape behind [`check_project_quest_tree`] and
/// [`check_project_subquest_unsatisfiable`], both of which need the same
/// `(parent, child, path, span, required, objective_id)` tuple. Owned
/// strings because the caller's `docs` slice is borrowed as a whole; a
/// borrowed `parent: &str` would tie every edge to a single doc-slice
/// lifetime and force every helper to thread it, for zero real win over
/// the tiny per-objective clone.
#[derive(Debug, Clone)]
struct SubquestEdge {
    parent: String,
    child: String,
    path: PathBuf,
    span: Span,
    required: bool,
    objective_id: String,
}

/// Every `<objective quest="c">` occurrence in `docs`, harvested in
/// document/quest/objective order (so downstream "first occurrence"
/// decisions inherit the caller's `docs` order, exactly as
/// [`check_project_quest_ids`] does with `group_by_id`). An empty parent
/// quest id, an empty child reference, or an empty objective id is skipped
/// — those are their own document's missing-id problems
/// (`E-QUEST-ID-MISSING`/`E-OBJECTIVE-ID-MISSING`, reported per-file), not
/// an edge this project-wide pass can meaningfully name.
fn subquest_edges(docs: &[ProjectDoc<'_>]) -> Vec<SubquestEdge> {
    let mut out = Vec::new();
    for item in docs {
        let path = item.path;
        let doc = item.doc;
        for quest in &doc.quests {
            if quest.id.is_empty() {
                continue;
            }
            for node in &quest.body {
                if let Node::Objective(o) = node {
                    let Some(child) = o.quest.as_deref() else {
                        continue;
                    };
                    if child.is_empty() || o.id.is_empty() {
                        continue;
                    }
                    out.push(SubquestEdge {
                        parent: quest.id.clone(),
                        child: child.to_string(),
                        path: path.to_path_buf(),
                        span: o.quest_span,
                        required: !o.optional,
                        objective_id: o.id.clone(),
                    });
                }
            }
        }
    }
    out
}

/// Boilerplate constructor for the three tree diagnostics — all
/// [`Layer::Logic`] (matching every other quest-identity diagnostic here),
/// all `Severity::Error` (a broken tree is a hard authoring fault: an
/// unknown reference, a two-parent DAG, or a cycle each yields an artifact
/// whose derived completion is undefined).
pub(crate) fn tree_diag(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        evidence: None,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// DFS body for [`check_project_quest_tree`]'s cycle pass. Standard
/// three-color walk (WHITE=absent, GRAY=on the current path,
/// BLACK=finished) — a `GRAY` neighbour is a back-edge, i.e. the exact
/// edge that closes a cycle, so it is the natural anchor: an editor jump
/// lands on the `<objective quest=...>` whose addition would break the
/// tree. Cycles are deduplicated by their canonical (sorted-node) set —
/// walking from a different start MUST NOT report the same ring twice —
/// which is the reason we exit `dfs` even for `Gray` neighbours WITHOUT
/// propagating an error return: reporting is a side effect at the discovery
/// site, control just unwinds.
fn dfs_cycle<'a>(
    u: &'a str,
    adj: &BTreeMap<&'a str, Vec<&'a SubquestEdge>>,
    color: &mut BTreeMap<&'a str, u8>,
    stack: &mut Vec<&'a str>,
    seen_cycles: &mut BTreeSet<Vec<&'a str>>,
    out: &mut Vec<(PathBuf, Diagnostic)>,
) {
    color.insert(u, 1);
    stack.push(u);
    if let Some(edges) = adj.get(u) {
        for e in edges {
            let v = e.child.as_str();
            let c = color.get(v).copied().unwrap_or(0);
            if c == 0 {
                dfs_cycle(v, adj, color, stack, seen_cycles, out);
            } else if c == 1 {
                // Back-edge: cycle is stack[pos(v)..] plus the closing edge back to v.
                let Some(start) = stack.iter().position(|n| *n == v) else {
                    continue;
                };
                let ring: Vec<&str> = stack[start..].to_vec();
                let mut canon: Vec<&str> = ring.clone();
                canon.sort();
                canon.dedup();
                if !seen_cycles.insert(canon) {
                    continue;
                }
                let mut pretty: Vec<&str> = ring.clone();
                pretty.push(v);
                let arrow = pretty.join(" → ");
                let msg = if ring.len() == 1 {
                    format!(
                        "`<objective id=\"{oid}\" quest=\"{child}\">` on quest `{parent}` is a \
                         self-reference; the parent→child graph must be acyclic (tree, not DAG) \
                         (dsl 2026-08-31 §4)",
                        oid = e.objective_id,
                        child = e.child,
                        parent = e.parent,
                    )
                } else {
                    format!(
                        "`<objective id=\"{oid}\" quest=\"{child}\">` on quest `{parent}` closes a \
                         subquest cycle {arrow}; the parent→child graph must be acyclic (tree, \
                         not DAG) (dsl 2026-08-31 §4)",
                        oid = e.objective_id,
                        child = e.child,
                        parent = e.parent,
                    )
                };
                out.push((e.path.clone(), tree_diag(E_QUEST_TREE_CYCLE, msg, e.span)));
            }
        }
    }
    color.insert(u, 2);
    stack.pop();
}

/// dsl 2026-08-31 §4: subquest **tree** structural checks. Runs three
/// passes over the parent→child graph implied by every
/// `<objective quest="c">` in `docs`:
///
///  1. [`E_QUEST_REF_UNKNOWN`] — the child names no quest defined by any
///     doc in `docs`. Anchored at the referencing objective's
///     `quest_span`. Same "walked directory" caveat as
///     [`check_project_quest_refs`]: a child defined in a sibling root
///     naturally reads as unknown here, which is the point.
///  2. [`E_QUEST_MULTI_PARENT`] — the same child is referenced by two
///     DIFFERENT parent quest ids. Mirrors
///     [`check_project_quest_ids`]'s "flag every occurrence past the
///     first" shape (`docs` order is the tie-breaker, so callers MUST
///     pass files pre-sorted for deterministic output); every subsequent
///     edge whose parent differs from the first-seen parent gets one
///     diagnostic anchored at THAT edge's own objective. Two objectives
///     inside the SAME parent quest that both `quest=` the same child are
///     the parent's own duplicate-edge issue, not a multi-parent problem;
///     they are silently deduplicated here (only distinct parents count).
///  3. [`E_QUEST_TREE_CYCLE`] — the parent→child graph closes a cycle
///     (self-reference is a length-1 cycle). One diagnostic per cycle
///     (deduped by node set — DFS from a different root MUST NOT
///     re-report the same ring), anchored at the back-edge — the exact
///     `<objective quest=...>` whose addition breaks the tree. Edges
///     whose child is undefined are excluded from the cycle graph: they
///     already earn [`E_QUEST_REF_UNKNOWN`] and there is no ambiguity for
///     them to close a cycle against.
///
/// The `<objective quest= / done=>` mutual exclusion
/// ([`crate::match_check`]'s `E-OBJECTIVE-QUEST-DONE`, dsl 2026-08-31 §1)
/// and same-document unknown-child are the per-file `check()`'s job —
/// this pass is deliberately silent on both, so its output stays a
/// diff-friendly project-wide superset without doubling every per-file
/// error.
pub fn check_project_quest_tree(docs: &[ProjectDoc<'_>]) -> Vec<(PathBuf, Diagnostic)> {
    let defined = defined_quests(docs);
    let edges = subquest_edges(docs);
    let mut out = Vec::new();

    // 1) Unknown-child references.
    for e in &edges {
        if !defined.contains_key(e.child.as_str()) {
            out.push((
                e.path.clone(),
                tree_diag(
                    E_QUEST_REF_UNKNOWN,
                    format!(
                        "`<objective id=\"{oid}\" quest=\"{child}\">` on quest `{parent}` \
                         references child quest `{child}`, which no project quest defines \
                         (dsl 2026-08-31 §4) — a typo, or a quest defined outside this walked \
                         directory",
                        oid = e.objective_id,
                        parent = e.parent,
                        child = e.child,
                    ),
                    e.span,
                ),
            ));
        }
    }

    // 2) Multi-parent references.
    let mut by_child: BTreeMap<&str, Vec<&SubquestEdge>> = BTreeMap::new();
    for e in &edges {
        by_child.entry(e.child.as_str()).or_default().push(e);
    }
    for (child, occurrences) in &by_child {
        let distinct_parents: BTreeSet<&str> =
            occurrences.iter().map(|e| e.parent.as_str()).collect();
        if distinct_parents.len() < 2 {
            continue;
        }
        let first_parent = occurrences[0].parent.as_str();
        for e in occurrences {
            if e.parent.as_str() == first_parent {
                continue;
            }
            out.push((
                e.path.clone(),
                tree_diag(
                    E_QUEST_MULTI_PARENT,
                    format!(
                        "quest `{child}` is referenced as a subquest from two different parents \
                         (`{first_parent}` and `{}`); a quest must have at most one parent \
                         (tree, not DAG, dsl 2026-08-31 §4)",
                        e.parent,
                    ),
                    e.span,
                ),
            ));
        }
    }

    // 3) Cycles.
    let mut adj: BTreeMap<&str, Vec<&SubquestEdge>> = BTreeMap::new();
    for e in &edges {
        if defined.contains_key(e.child.as_str()) {
            adj.entry(e.parent.as_str()).or_default().push(e);
        }
    }
    // First-appearance node order (not `adj`'s `BTreeMap` sort) is what
    // makes back-edge anchoring predictable: DFS from the earliest-declared
    // parent means the closing edge lands on the LATER quest whose
    // `<objective quest=...>` completes the ring — the same edge an author
    // would most recently have added, and the natural place to jump to fix
    // the cycle. A `BTreeSet` sort would let the alphabetically-earliest
    // node steal DFS root and flip the anchor onto an earlier edge for no
    // authoring reason.
    let mut nodes: Vec<&str> = Vec::new();
    let mut seen_nodes: BTreeSet<&str> = BTreeSet::new();
    for e in &edges {
        if !defined.contains_key(e.child.as_str()) {
            continue;
        }
        if seen_nodes.insert(e.parent.as_str()) {
            nodes.push(e.parent.as_str());
        }
        if seen_nodes.insert(e.child.as_str()) {
            nodes.push(e.child.as_str());
        }
    }
    let mut color: BTreeMap<&str, u8> = BTreeMap::new();
    let mut stack: Vec<&str> = Vec::new();
    let mut seen_cycles: BTreeSet<Vec<&str>> = BTreeSet::new();
    for &start in &nodes {
        if color.get(start).copied().unwrap_or(0) != 0 {
            continue;
        }
        dfs_cycle(
            start,
            &adj,
            &mut color,
            &mut stack,
            &mut seen_cycles,
            &mut out,
        );
    }
    // 4) Tier mixing across documents (the same-document case is the
    //    per-file `check()`'s [`check_doc_quest_tiers`]).
    let mut tiers: BTreeMap<&str, (&Path, &str)> = BTreeMap::new();
    for item in docs {
        let path = item.path;
        let doc = item.doc;
        for q in &doc.quests {
            tiers
                .entry(q.id.as_str())
                .or_insert((path, quest_tier(q)));
        }
    }
    for e in &edges {
        let (Some(&(child_path, child_tier)), Some(&(_, parent_tier))) =
            (tiers.get(e.child.as_str()), tiers.get(e.parent.as_str()))
        else {
            continue;
        };
        if child_path != e.path.as_path() && child_tier != parent_tier {
            out.push((
                e.path.clone(),
                tier_mix_diag(&e.parent, parent_tier, &e.child, child_tier, e.span),
            ));
        }
    }
    // 5) A subquest's `rearm=` across documents (the same-document case is
    //    the per-file `check()`'s [`check_doc_quest_rearm`]).
    let rearms: BTreeMap<&str, (&Path, Span)> = docs
        .iter()
        .flat_map(|item| {
            item.doc.quests.iter().filter_map(move |q| {
                Some((q.id.as_str(), (item.path, q.rearm.as_ref()?.span)))
            })
        })
        .collect();
    let mut seen_children = BTreeSet::new();
    for e in &edges {
        let Some(&(child_path, span)) = rearms.get(e.child.as_str()) else {
            continue;
        };
        if child_path != e.path.as_path() && seen_children.insert(e.child.as_str()) {
            out.push((
                child_path.to_path_buf(),
                subquest_rearm_diag(&e.child, &e.parent, span),
            ));
        }
    }

    out
}

/// dsl 2026-08-31 §4 (extension): propagate `E-QUEST-UNREACHABLE` one edge
/// up the subquest tree — a REQUIRED `<objective quest="c">` whose child
/// `c` is unreachable can never complete (§2.1 makes the objective's
/// completion predicate `quest.c.state == 'complete'`), so it earns
/// [`E_OBJECTIVE_UNSATISFIABLE_SUBQUEST`] anchored at its own `quest_span`.
///
/// `unreachable_quests` is whatever set the CLI has already proven dead —
/// today `ConnFixpoint::unreachable_quests` (union of per-file
/// `E-QUEST-UNREACHABLE`, dead-`start` liveness, and dead-required-objective
/// propagation). An `optional` objective's failed child never gates parent
/// completion (§2.1), so this pass is deliberately silent on them —
/// diagnostic parity with the existing §5.3 rule.
pub fn check_project_subquest_unsatisfiable(
    docs: &[ProjectDoc<'_>],
    unreachable_quests: &BTreeSet<String>,
) -> Vec<(PathBuf, Diagnostic)> {
    let mut out = Vec::new();
    for e in subquest_edges(docs) {
        if !e.required {
            continue;
        }
        if !unreachable_quests.contains(e.child.as_str()) {
            continue;
        }
        out.push((
            e.path.clone(),
            Diagnostic {
                code: E_OBJECTIVE_UNSATISFIABLE_SUBQUEST.to_string(),
                severity: Severity::Error,
                message: format!(
                    "required `<objective id=\"{oid}\" quest=\"{child}\">` on quest `{parent}` \
                     can never complete: the referenced child quest `{child}` is unreachable \
                     (`E-QUEST-UNREACHABLE`), so its synthesized completion predicate \
                     `quest.{child}.state == 'complete'` (dsl 2026-08-31 §2.1) never fires. \
                     Fix the child, or mark this objective `optional`.",
                    oid = e.objective_id,
                    parent = e.parent,
                    child = e.child,
                ),
                evidence: None,
                span: e.span,
                layer: Layer::Logic,
                fixits: Vec::new(),
                provenance: None,
                covered: Vec::new(),
                related: Vec::new(),
            },
        ));
    }
    out
}

/// dsl 0.22.0 §7: an `<on event="questFailed">` on a quest that can never
/// fail (`check-project` only).
pub const W_QUEST_HANDLER_DEAD: &str = "W-QUEST-HANDLER-DEAD";

/// [`W_QUEST_HANDLER_DEAD`] over one resolved project root: every
/// `<on event="questFailed">` of a quest with no way to reach `failed` — no
/// authored `fail`, no REQUIRED objective with a `by=` deadline (dsl 0.23.0
/// §2: a missed required deadline fails its quest), no REQUIRED
/// `<objective quest="c">` whose child can itself fail (a failing required
/// child fails its parent), and no parent quest anywhere in `docs` (a
/// terminating parent cascade-fails its still-active children). Project-wide
/// because the parent edge can live in another document. A warning at the
/// handler's `event` value: the body is dead code, not an error.
pub fn check_project_quest_handlers(docs: &[ProjectDoc<'_>]) -> Vec<(PathBuf, Diagnostic)> {
    let edges = subquest_edges(docs);
    // Quests that fail on their own: an authored `fail`, a required `by=`
    // deadline, or a REQUIRED child that fails on its own (a child
    // cascade-failed by its parent's end cannot fail that parent). Fixpoint,
    // cycle-safe.
    let nonempty = |slot: &lute_syntax::ast::CelSlot| !slot.raw.trim().is_empty();
    let mut fails: BTreeSet<&str> = docs
        .iter()
        .flat_map(|item| &item.doc.quests)
        .filter(|q| {
            q.fail.as_ref().is_some_and(nonempty)
                || q.body.iter().any(|n| {
                    matches!(n, Node::Objective(o) if !o.optional
                        && (o.by.as_ref().is_some_and(nonempty) || o.until.as_ref().is_some_and(nonempty)))
                })
        })
        .map(|q| q.id.as_str())
        .collect();
    loop {
        let grown: Vec<&str> = edges
            .iter()
            .filter(|e| e.required && fails.contains(e.child.as_str()))
            .map(|e| e.parent.as_str())
            .filter(|p| !fails.contains(p))
            .collect();
        if grown.is_empty() {
            break;
        }
        fails.extend(grown);
    }
    let mut out = Vec::new();
    for item in docs {
        let path = item.path;
        let doc = item.doc;
        for quest in &doc.quests {
            let has_parent = edges.iter().any(|e| e.child == quest.id);
            if quest.id.is_empty() || fails.contains(quest.id.as_str()) || has_parent {
                continue;
            }
            for node in &quest.body {
                let Node::On(on) = node else { continue };
                if on.event != "questFailed" {
                    continue;
                }
                out.push((
                    path.to_path_buf(),
                    Diagnostic {
                        code: W_QUEST_HANDLER_DEAD.to_string(),
                        severity: Severity::Warning,
                        message: format!(
                            "`<on event=\"questFailed\">` never runs: quest `{}` cannot fail — \
                             it has no `fail` condition, no required objective with a `by=` \
                             deadline, no required subquest that can fail (a failing required \
                             child fails it), and no parent quest whose end would cascade to \
                             it; add a `fail=` condition or remove the handler (dsl 0.22.0 §7)",
                            quest.id
                        ),
                        evidence: None,
                        span: on.event_span,
                        layer: Layer::Logic,
                        fixits: Vec::new(),
                        provenance: None,
                        covered: Vec::new(),
                        related: Vec::new(),
                    },
                ));
            }
        }
    }
    out
}
