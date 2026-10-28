//! dsl 0.49.0 §6.1–§6.2, 0.53.0 §7.3: quests that can never start because
//! their `start` and in-script accept anchors wait on each other (a start
//! cycle) or on the quest itself.

use super::graph_assembly::QuestDecls;
use super::graph_nodes::{
    Anchor, ConnGraph, CyclePath, EdgeKind, HostGate, NodeId, NodeInfo, PrereqState, StartBlock,
};
use super::reachability::Reachability;
use crate::ProjectDoc;
use lute_core_span::{Diagnostic, Layer, Severity};
use lute_manifest::semantics::prereq::PrereqFormula;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;

/// dsl 0.49.0 §6.1, 0.53.0 §7.3: quests whose `start` and in-script accept
/// anchors wait on each other around a cycle that no source outside it
/// enters and no host route opens, so none of them can ever start.
pub const E_QUEST_START_CYCLE: &str = "E-QUEST-START-CYCLE";

/// Whether `anchor` gates when its quest can start: a `start` conjunct, the
/// subquest parent, or (dsl 0.53.0 §7.3) the accepts — `accept="external"`
/// is the only non-content acceptance (dsl 0.25.0 §5), and it is the
/// anchor's host route.
fn gates(anchor: &Anchor) -> bool {
    matches!(
        anchor.kind,
        EdgeKind::Start | EdgeKind::Subquest | EdgeKind::Accept
    )
}

/// The gating anchors of a node, `None` when it has none.
fn gating(info: &NodeInfo) -> Option<impl Iterator<Item = &Anchor>> {
    let PrereqState::Anchored(anchors) = &info.prereq else {
        return None;
    };
    anchors
        .iter()
        .any(gates)
        .then(|| anchors.iter().filter(|a| gates(a)))
}

/// Whether `anchor` opens once the quests in `can` can start: one of its
/// sources can, or its host route opens — always without `follows`, when
/// the formula holds over what can start with it, and always when the
/// formula cannot be read (dsl 0.53.0 §7.3).
fn opens(anchor: &Anchor, can: &BTreeSet<&NodeId>, nodes: &BTreeMap<NodeId, NodeInfo>) -> bool {
    anchor.from.iter().any(|s| can.contains(s))
        || match &anchor.host {
            None => false,
            Some(HostGate::Always | HostGate::Unreadable) => true,
            Some(HostGate::Follows(f)) => formula_opens(f, can, nodes),
        }
}

/// [`opens`] for a host `follows` formula: an atom holds when its node can
/// start, or names no graph node (a plain quest, an undeclared id — the
/// latter is `E-CONN-UNKNOWN-NODE`'s, never a deadlock).
fn formula_opens(
    f: &PrereqFormula,
    can: &BTreeSet<&NodeId>,
    nodes: &BTreeMap<NodeId, NodeInfo>,
) -> bool {
    let holds = |node: NodeId| !nodes.contains_key(&node) || can.contains(&&node);
    match f {
        PrereqFormula::Visited(key) => holds(NodeId::visited(key, nodes)),
        PrereqFormula::Completed(id) | PrereqFormula::Active(id) => {
            holds(NodeId::Quest(id.clone()))
        }
        PrereqFormula::And(l, r) => formula_opens(l, can, nodes) && formula_opens(r, can, nodes),
        PrereqFormula::Or(l, r) => formula_opens(l, can, nodes) || formula_opens(r, can, nodes),
    }
}

/// Whether `quest`'s own `start` anchors read `quest` itself.
fn reads_itself(quest: &NodeId, info: &NodeInfo) -> bool {
    matches!(&info.prereq, PrereqState::Anchored(anchors)
        if anchors.iter().any(|a| a.kind == EdgeKind::Start && a.from.contains(quest)))
}

/// dsl 0.49.0 §6.1–§6.2: sets [`ConnGraph::start_blocked`] of `g` from the
/// reachability `reach` that [`super::check_reachability`] computed over it
/// with `unreachable_quests`, and returns one [`E_QUEST_START_CYCLE`] per
/// strongly connected set of deadlocked quests.
///
/// Reachability never depends on *can-start* — a dead anchor reads
/// `Unknown`, never `Unreachable` — so the two reach a joint fixed point
/// when this runs once on the final `reach`: a node the lifecycle proves
/// unreachable is no way in.
pub fn resolve_start_blocks(
    g: &mut ConnGraph,
    docs: &[ProjectDoc<'_>],
    reach: &BTreeMap<NodeId, Reachability>,
    unreachable_quests: &BTreeSet<String>,
) -> Vec<(PathBuf, Diagnostic)> {
    let decls = QuestDecls::new(docs);
    let mut diags = Vec::new();
    let routable = |id: &NodeId| {
        reach.get(id) != Some(&Reachability::Unreachable)
            && !matches!(id, NodeId::Quest(q) if unreachable_quests.contains(q))
    };
    let blocked = start_blocks(&g.nodes, &g.topo_order, &decls, &routable, &mut diags);
    g.start_blocked = blocked;
    diags
}

/// The least fixed point of *can-start* (dsl 0.49.0 §6.1) and, for every
/// quest outside it that a start cycle or a self-read blocks, the
/// [`StartBlock`]. Pushes one [`E_QUEST_START_CYCLE`] per strongly connected
/// set of deadlocked quests that do not read themselves, at its least
/// quest's `start`, or at its `id` when it has none (an accept-driven quest,
/// dsl 0.53.0 §7.3).
///
/// A node can start (is a way in) only when `routable`. Without gating
/// anchors it also has to be in `topo_order` (not on or past an `after`
/// cycle); with them, every gating anchor has to open ([`opens`]).
fn start_blocks(
    nodes: &BTreeMap<NodeId, NodeInfo>,
    topo_order: &[NodeId],
    decls: &QuestDecls<'_>,
    routable: &dyn Fn(&NodeId) -> bool,
    diags: &mut Vec<(PathBuf, Diagnostic)>,
) -> BTreeMap<NodeId, StartBlock> {
    let mut can: BTreeSet<&NodeId> = topo_order
        .iter()
        .filter(|id| nodes.get(*id).is_some_and(|info| gating(info).is_none()) && routable(id))
        .collect();
    let gated: Vec<(&NodeId, &NodeInfo)> = topo_order
        .iter()
        .filter_map(|id| nodes.get_key_value(id))
        .filter(|(_, info)| gating(info).is_some())
        .collect();
    // `topo_order` puts every ordering source first, so only a back edge or
    // an undrawn accept edge needs another pass; each pass that adds
    // nothing ends it.
    loop {
        let mut grew = false;
        for &(id, info) in &gated {
            // A lifecycle-dead quest is no way in, whatever its anchors
            // read (F4-3).
            if can.contains(id) || !routable(id) {
                continue;
            }
            let starts = gating(info)
                .into_iter()
                .flatten()
                .all(|a| opens(a, &can, nodes));
            if starts {
                can.insert(id);
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }

    // The waiting graph over the quests that cannot start: `dependent ->
    // source` with the anchor's kind, for each blocked source of a gating
    // anchor (the same anchors the fixed point reads; the first kind of a
    // pair in anchor order).
    let blocked: BTreeSet<&NodeId> = nodes
        .iter()
        .filter(|(id, info)| gating(info).is_some() && !can.contains(id))
        .map(|(id, _)| id)
        .collect();
    let mut waits: BTreeMap<&NodeId, BTreeMap<&NodeId, EdgeKind>> = BTreeMap::new();
    for &id in &blocked {
        for anchor in gating(&nodes[id]).into_iter().flatten() {
            for source in anchor.from.iter().filter(|s| blocked.contains(s)) {
                waits
                    .entry(id)
                    .or_default()
                    .entry(source)
                    .or_insert(anchor.kind);
            }
        }
    }

    let mut out = BTreeMap::new();
    let mut reported: BTreeSet<&NodeId> = BTreeSet::new();
    for &id in &blocked {
        let info = &nodes[id];
        if reads_itself(id, info) {
            out.insert(id.clone(), StartBlock::SelfRead);
            continue;
        }
        let Some(path) = shortest_cycle(id, &waits) else {
            continue;
        };
        if !reported.contains(id) {
            // `blocked` is in `NodeId` order, so `id` is the least member of
            // its strongly connected set not yet reported.
            let members = strongly_connected(id, &waits);
            reported.extend(members.iter().copied());
            let span = match id {
                NodeId::Quest(q) => decls
                    .first
                    .get(q.as_str())
                    .and_then(|(_, quest)| quest.start.as_ref())
                    .map_or(info.span, |s| s.span),
                _ => info.span,
            };
            diags.push((
                info.path.clone(),
                start_cycle_diag(
                    format!(
                        "quest start cycle: {path} — each waits for the next (a `start` for its \
                         condition, an accept for the `::accept` or reward that grants it) and no \
                         source outside the cycle starts any of them; start one of them from \
                         something outside the cycle, or let the host grant one with \
                         `accept=\"external\"`"
                    ),
                    span,
                ),
            ));
        }
        out.insert(id.clone(), StartBlock::Cycle(path));
    }

    // Quests that can start only through blocked ones (dsl 0.49.0 §6.1
    // "downstream only through deadlocked sources").
    loop {
        let mut grew = false;
        for &id in &blocked {
            if out.contains_key(id) {
                continue;
            }
            let behind = gating(&nodes[id]).into_iter().flatten().find_map(|a| {
                a.from
                    .iter()
                    .all(|s| out.contains_key(s))
                    .then(|| a.from.first().cloned())
                    .flatten()
            });
            if let Some(source) = behind {
                out.insert(id.clone(), StartBlock::Behind(source));
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }
    out
}

/// The shortest cycle through `start` along `waits` that is not its
/// self-loop, in waiting order with each step's anchor kind; `None` when
/// there is none.
fn shortest_cycle(
    start: &NodeId,
    waits: &BTreeMap<&NodeId, BTreeMap<&NodeId, EdgeKind>>,
) -> Option<CyclePath> {
    let mut parent: BTreeMap<&NodeId, &NodeId> = BTreeMap::new();
    let mut queue: VecDeque<&NodeId> = VecDeque::new();
    for n in waits.get(start).into_iter().flat_map(BTreeMap::keys) {
        if *n != start && !parent.contains_key(n) {
            parent.insert(n, start);
            queue.push_back(n);
        }
    }
    while let Some(n) = queue.pop_front() {
        for &m in waits.get(n).into_iter().flat_map(BTreeMap::keys) {
            if m == start {
                let mut chain = vec![n];
                let mut at = n;
                while at != start {
                    at = parent[at];
                    chain.push(at);
                }
                // `chain` runs back from `n` to `start`; in waiting order
                // it is `start`, …, `n`, and `n` waits on `start`.
                chain.reverse();
                let steps = chain
                    .iter()
                    .enumerate()
                    .map(|(i, &node)| {
                        let to = chain.get(i + 1).copied().unwrap_or(start);
                        (node.clone(), waits[node][to])
                    })
                    .collect();
                return Some(CyclePath { steps });
            }
            if m != start && !parent.contains_key(m) {
                parent.insert(m, n);
                queue.push_back(m);
            }
        }
    }
    None
}

/// Every node reachable from `start` along `waits` that reaches it back.
fn strongly_connected<'a>(
    start: &'a NodeId,
    waits: &BTreeMap<&'a NodeId, BTreeMap<&'a NodeId, EdgeKind>>,
) -> BTreeSet<&'a NodeId> {
    let forward = reachable(start, waits);
    forward
        .iter()
        .copied()
        .filter(|n| reachable(n, waits).contains(start))
        .collect()
}

fn reachable<'a>(
    from: &'a NodeId,
    waits: &BTreeMap<&'a NodeId, BTreeMap<&'a NodeId, EdgeKind>>,
) -> BTreeSet<&'a NodeId> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![from];
    while let Some(n) = stack.pop() {
        for &m in waits.get(n).into_iter().flat_map(BTreeMap::keys) {
            if seen.insert(m) {
                stack.push(m);
            }
        }
    }
    seen
}

fn start_cycle_diag(message: String, span: lute_core_span::Span) -> Diagnostic {
    Diagnostic {
        code: E_QUEST_START_CYCLE.to_string(),
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
