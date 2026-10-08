use super::graph_nodes::*;
use super::graph_assembly::*;
use super::*;
use crate::ProjectDoc;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use cel_parser::ast::{operators as op, Expr};
use cel_parser::reference::Val;
use lute_core_span::Diagnostic;
use lute_syntax::ast::Node;
use crate::meta::{resolve_doc_kind, DocKind};
use lute_manifest::semantics::prereq::{atoms, Atom};

pub(super) fn top_conjuncts<'e>(e: &'e Expr, out: &mut Vec<&'e Expr>) {
    if let Expr::Call(c) = e {
        if c.func_name == op::LOGICAL_AND && c.target.is_none() && c.args.len() == 2 {
            top_conjuncts(&c.args[0].expr, out);
            top_conjuncts(&c.args[1].expr, out);
            return;
        }
    }
    out.push(e);
}

/// The sources one `start` conjunct anchors at ([`start_anchors`]): the
/// node it reads, or every source of an `||` whose every arm reads one.
pub(super) fn start_sources(
    e: &Expr,
    own: &str,
    decls: &QuestDecls<'_>,
    nodes: &BTreeMap<NodeId, NodeInfo>,
) -> Option<Vec<NodeId>> {
    match e {
        Expr::Call(c)
            if c.func_name == op::LOGICAL_OR
                && c.target.is_none()
                && c.args.len() == 2 =>
        {
            let mut from = start_sources(&c.args[0].expr, own, decls, nodes)?;
            for n in start_sources(&c.args[1].expr, own, decls, nodes)? {
                if !from.contains(&n) {
                    from.push(n);
                }
            }
            Some(from)
        }
        Expr::Call(_)
        | Expr::Comprehension(_)
        | Expr::Ident(_)
        | Expr::List(_)
        | Expr::Literal(_)
        | Expr::Map(_)
        | Expr::Select(_)
        | Expr::Struct(_)
        | Expr::Unspecified => start_source(e, own, decls, nodes).map(|n| vec![n]),
    }
}

/// The graph node one anchoring read names (see [`start_anchors`]).
pub(super) fn start_source(
    e: &Expr,
    own: &str,
    decls: &QuestDecls<'_>,
    nodes: &BTreeMap<NodeId, NodeInfo>,
) -> Option<NodeId> {
    let entry = |e: &Expr| {
        let path = crate::cel_paths::select_path(e)?;
        let id = lute_manifest::semantics::cel_paths::reserved_entry_id(&path)?;
        (crate::cel_paths::is_entry_ever_read(&path) && decls.entries.contains_key(id))
            .then(|| NodeId::Entry(id.to_string()))
    };
    let quest_state = |path: &Expr, state: &Expr| {
        let Expr::Literal(Val::String(state)) = state else {
            return None;
        };
        let path = crate::cel_paths::select_path(path)?;
        if !crate::cel_paths::is_reserved_quest_state(&path) || state.as_str() == "unset" {
            return None;
        }
        let id = path.split('.').nth(1)?;
        (id != own && decls.first.contains_key(id)).then(|| NodeId::Quest(id.to_string()))
    };
    let Expr::Call(c) = e else {
        return entry(e);
    };
    if let Some(key) = crate::cel_resolve::visited_call_target(c) {
        let node = NodeId::visited(key, nodes);
        return nodes.contains_key(&node).then_some(node);
    }
    if c.func_name != op::EQUALS || c.target.is_some() || c.args.len() != 2 {
        return None;
    }
    let (a, b) = (&c.args[0].expr, &c.args[1].expr);
    if matches!(b, Expr::Literal(Val::Boolean(true))) {
        return entry(a);
    }
    quest_state(a, b).or_else(|| quest_state(b, a))
}

/// dsl 0.24.0 §2: per accept-driven quest
/// ([`crate::accept::accept_driven_quests`]), every graph node whose body
/// `::accept`s it, in document order — the accepting scene, the accepting
/// bundle beat, or the accepting quest (which must be active for its body
/// to run). A lore entry's `::accept` anchors nothing (the entry is no
/// source until a `start` reads it); a quest never anchors itself.
pub(super) fn accept_sources<'a>(
    docs: &'a [ProjectDoc<'a>],
    nodes: &BTreeMap<NodeId, NodeInfo>,
) -> BTreeMap<&'a str, Vec<NodeId>> {
    let driven = crate::accept::accept_driven_quests(docs);
    let mut sites: BTreeMap<&str, Vec<NodeId>> = BTreeMap::new();
    let mut record = |d: &lute_syntax::ast::Directive, source: &NodeId, own: Option<&str>| {
        let Some((id, _)) = d.accept_quest() else {
            return;
        };
        let Some(id) = driven.get(id).copied() else {
            return;
        };
        if own == Some(id) {
            return;
        }
        let from = sites.entry(id).or_default();
        if !from.contains(source) {
            from.push(source.clone());
        }
    };
    for item in docs {
        let doc = item.doc;
        if let Some(source) = scene_key(item.meta)
            .map(NodeId::Scene)
            .filter(|s| nodes.contains_key(s))
        {
            for shot in &doc.sections {
                crate::accept::walk(&shot.body, &mut |d| record(d, &source, None));
            }
        }
        if resolve_doc_kind(&doc.meta).0 == Some(DocKind::Lore) {
            if let Some(doc_id) = bundle_id(item.meta) {
                for beat in doc
                    .beats
                    .iter()
                    .filter(|b| lute_manifest::ident::is_name(&b.id))
                {
                    let source = NodeId::Beat(crate::bundles::bundle_beat_key(&doc_id, &beat.id));
                    if nodes.contains_key(&source) {
                        crate::accept::walk(&beat.body, &mut |d| record(d, &source, None));
                    }
                }
            }
        }
        for quest in doc.quests.iter().filter(|q| !q.id.is_empty()) {
            let source = NodeId::Quest(quest.id.clone());
            crate::accept::walk(&quest.body, &mut |d| {
                record(d, &source, Some(quest.id.as_str()))
            });
        }
    }
    sites
}

/// A prerequisite reference [`assemble_graph`] does not draw — what `lute
/// scenario` notes beside the graph instead of leaving a missing edge
/// unexplained (dsl 0.23.0 §1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OmittedRef {
    /// `completed(Q)` / `active(Q)` in node `from`'s `after`, where `Q` is a
    /// declared quest that is no graph node (no `after`, no anchor, no
    /// subquest tree).
    Lifecycle {
        from: NodeId,
        kind: EdgeKind,
        quest: String,
    },
    /// `visited('<scene>')` in a lifecycle condition of `quest`, which
    /// declares no `after`, that draws no edge — not a top-level `start`
    /// conjunct (dsl 0.25.0 §4). `slot` names where it is read: `start`,
    /// `fail`, or `objective <id> <done|visibleWhen|by|until>`. A condition
    /// read gates the quest; it is no anchor, and a `follows` copying it
    /// would replace the quest's real anchors (summer S1).
    Visited {
        quest: String,
        scene: String,
        slot: String,
    },
}

/// Every [`OmittedRef`] of one resolved root, graph-node order for
/// [`OmittedRef::Lifecycle`], then document order for
/// [`OmittedRef::Visited`]. `quest_ids` is [`quest_id_set`] over `docs`.
pub fn omitted_refs(
    docs: &[ProjectDoc<'_>],
    graph: &ConnGraph,
    quest_ids: &BTreeSet<String>,
) -> Vec<OmittedRef> {
    let mut out = Vec::new();
    for info in graph.nodes.values() {
        let PrereqState::Valid(formula) = &info.prereq else {
            continue;
        };
        for atom in atoms(formula) {
            let kind = atom_edge_kind(&atom);
            let (Atom::Completed(quest) | Atom::Active(quest)) = atom else {
                continue;
            };
            if quest_ids.contains(&quest)
                && !graph.nodes.contains_key(&NodeId::Quest(quest.clone()))
            {
                out.push(OmittedRef::Lifecycle {
                    from: info.id.clone(),
                    kind,
                    quest,
                });
            }
        }
    }
    for item in docs {
        let doc = item.doc;
        for quest in &doc.quests {
            if quest.follows.is_some() || quest.id.is_empty() {
                continue;
            }
            let mut slots: Vec<(String, &lute_syntax::ast::CelSlot)> = Vec::new();
            slots.extend(quest.start.iter().map(|s| ("start".to_string(), s)));
            slots.extend(quest.fail.iter().map(|s| ("fail".to_string(), s)));
            for node in &quest.body {
                if let Node::Objective(o) = node {
                    let named = |key: &str| format!("objective {} {key}", o.id);
                    slots.push((named("done"), &o.done));
                    slots.extend(o.visible_when.iter().map(|s| (named("visibleWhen"), s)));
                    slots.extend(o.by.iter().map(|s| (named("by"), s)));
                    slots.extend(o.until.iter().map(|s| (named("until"), s)));
                }
            }
            for (name, slot) in slots {
                if !slot.raw.contains(crate::cel_resolve::VISITED_FN) {
                    continue;
                }
                let mut arena = lute_cel::CelArena::default();
                let Some(root) = lute_cel::parse_slot_marked_refs(&mut arena, &slot.raw)
                    .and_then(|h| arena.get(h).cloned())
                else {
                    continue;
                };
                for scene in crate::cel_resolve::visited_targets(&root.expr) {
                    let from = NodeId::visited(&scene, &graph.nodes);
                    if graph
                        .edge_kinds_for(&from, &NodeId::Quest(quest.id.clone()))
                        .is_some()
                    {
                        continue;
                    }
                    out.push(OmittedRef::Visited {
                        quest: quest.id.clone(),
                        scene,
                        slot: name.clone(),
                    });
                }
            }
        }
    }
    out
}

/// Detect any directed cycle in `edges`, reporting each as [`E_CONN_CYCLE`]
/// into `diags`. Standard DFS 3-coloring, cloned from
/// `schema_import::detect_cycles` / `dfs_cycle` (`schema_import.rs:784-833`)
/// over [`NodeId`] rather than `PathBuf`. Nodes are visited in [`NodeId`]'s
/// own sorted (`BTreeMap`) order for a deterministic, order-independent
/// result; `edges`' `BTreeSet` targets are already sorted, so (unlike the
/// `Vec`-adjacency precedent) there is no separate neighbor sort step.
pub(super) fn detect_conn_cycles(
    nodes: &BTreeMap<NodeId, NodeInfo>,
    edges: &BTreeMap<NodeId, BTreeSet<NodeId>>,
    diags: &mut Vec<(PathBuf, Diagnostic)>,
) {
    let mut on_stack: BTreeSet<NodeId> = BTreeSet::new();
    let mut done: BTreeSet<NodeId> = BTreeSet::new();
    let mut stack: Vec<NodeId> = Vec::new();
    for start in nodes.keys() {
        if !done.contains(start) && !on_stack.contains(start) {
            dfs_conn_cycle(
                start,
                nodes,
                edges,
                &mut on_stack,
                &mut done,
                &mut stack,
                diags,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn dfs_conn_cycle(
    node: &NodeId,
    nodes: &BTreeMap<NodeId, NodeInfo>,
    edges: &BTreeMap<NodeId, BTreeSet<NodeId>>,
    on_stack: &mut BTreeSet<NodeId>,
    done: &mut BTreeSet<NodeId>,
    stack: &mut Vec<NodeId>,
    diags: &mut Vec<(PathBuf, Diagnostic)>,
) {
    on_stack.insert(node.clone());
    stack.push(node.clone());
    if let Some(targets) = edges.get(node) {
        for nbr in targets {
            if on_stack.contains(nbr) {
                // Back edge -> cycle: report the chain from `nbr` around to `node`.
                let start_idx = stack.iter().position(|n| n == nbr).unwrap_or(0);
                let chain = stack[start_idx..]
                    .iter()
                    .chain(std::iter::once(nbr))
                    .map(NodeId::to_string)
                    .collect::<Vec<_>>()
                    .join(" -> ");
                // The chain `stack[start_idx..]` is EXACTLY the nodes on this
                // cycle; it is used only to render the diagnostic message. The
                // gate's on/downstream-of-cycle test is decided separately by
                // topological-order exclusion (`node_cycle_degraded`), which
                // is complete where a back-edge stack slice under-approximates.
                let info = nodes
                    .get(nbr)
                    .expect("cycle target must be a graph node -- edges only ever target nodes");
                diags.push((
                    info.path.clone(),
                    cycle_diag(
                        format!("prerequisite cycle: {chain} (dsl §2.4/§4.1 §A)"),
                        info.span,
                    ),
                ));
            } else if !done.contains(nbr) {
                dfs_conn_cycle(nbr, nodes, edges, on_stack, done, stack, diags);
            }
        }
    }
    stack.pop();
    on_stack.remove(node);
    done.insert(node.clone());
}

/// Deterministic Kahn's-algorithm topological sort over `edges`. Runs
/// UNCONDITIONALLY even on a cyclic graph (spec §4.1 per-node cycle
/// recovery): any node that never reaches in-degree 0 — every cycle member
/// and everything transitively downstream of one — is simply omitted from
/// the returned order, never panicked on; every cycle-independent node is
/// still emitted with its prerequisites before it. Ties (multiple
/// zero-in-degree nodes ready at once) break on [`NodeId`]'s own `Ord` via a
/// `BTreeSet` ready queue — independent of `nodes`/`edges`' own insertion
/// order.
pub(super) fn topo_sort(
    nodes: &BTreeMap<NodeId, NodeInfo>,
    edges: &BTreeMap<NodeId, BTreeSet<NodeId>>,
) -> Vec<NodeId> {
    let mut in_degree: BTreeMap<NodeId, usize> = nodes.keys().map(|id| (id.clone(), 0)).collect();
    for targets in edges.values() {
        for target in targets {
            *in_degree.entry(target.clone()).or_insert(0) += 1;
        }
    }
    let mut ready: BTreeSet<NodeId> = in_degree
        .iter()
        .filter(|&(_, degree)| *degree == 0)
        .map(|(id, _)| id.clone())
        .collect();
    let mut order = Vec::with_capacity(nodes.len());
    while let Some(next) = ready.iter().next().cloned() {
        ready.remove(&next);
        if let Some(targets) = edges.get(&next) {
            for target in targets {
                if let Some(degree) = in_degree.get_mut(target) {
                    *degree -= 1;
                    if *degree == 0 {
                        ready.insert(target.clone());
                    }
                }
            }
        }
        order.push(next);
    }
    order
}

