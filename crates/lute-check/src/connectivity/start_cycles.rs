//! dsl 0.49.0 §6.1–§6.2: quests that can never start because their `start`
//! anchors wait on each other (a start cycle) or on the quest itself.

use super::graph_assembly::QuestDecls;
use super::graph_nodes::{Anchor, ConnGraph, EdgeKind, NodeId, NodeInfo, PrereqState, StartBlock};
use super::reachability::Reachability;
use crate::ProjectDoc;
use lute_core_span::{Diagnostic, Layer, Severity};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;

/// dsl 0.49.0 §6.1: quests whose `start` anchors wait on each other around
/// a cycle that no source outside it enters, so none of them can ever start.
pub const E_QUEST_START_CYCLE: &str = "E-QUEST-START-CYCLE";

/// Whether `anchor` gates when its quest can start: a `start` conjunct or
/// the subquest parent. An `::accept` anchor does not (the engine may accept
/// the quest outside any `::accept`; accept deadlocks are out of scope,
/// dsl 0.49.0 §14).
fn gates(anchor: &Anchor) -> bool {
    matches!(anchor.kind, EdgeKind::Start | EdgeKind::Subquest)
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
/// set of deadlocked quests that do not read themselves, at its least quest's
/// `start`.
///
/// A node without gating anchors can start (is a way in) when it is in
/// `topo_order` (not on or past an `after` cycle) and `routable`.
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
    // `topo_order` puts every ordering source first, so only a back edge
    // needs another pass; each pass that adds nothing ends it.
    loop {
        let mut grew = false;
        for &(id, info) in &gated {
            if can.contains(id) {
                continue;
            }
            let starts = gating(info)
                .into_iter()
                .flatten()
                .all(|a| a.from.iter().any(|s| can.contains(s)));
            if starts {
                can.insert(id);
                grew = true;
            }
        }
        if !grew {
            break;
        }
    }

    // The anchor graph over the quests that cannot start: `source ->
    // dependent` for each blocked source of a gating anchor (the same
    // anchors the fixed point reads).
    let blocked: BTreeSet<&NodeId> = nodes
        .iter()
        .filter(|(id, info)| gating(info).is_some() && !can.contains(id))
        .map(|(id, _)| id)
        .collect();
    let mut next: BTreeMap<&NodeId, BTreeSet<&NodeId>> = BTreeMap::new();
    for &id in &blocked {
        for source in gating(&nodes[id])
            .into_iter()
            .flatten()
            .flat_map(|a| &a.from)
            .filter(|s| blocked.contains(s))
        {
            next.entry(source).or_default().insert(id);
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
        let Some(path) = shortest_cycle(id, &next) else {
            continue;
        };
        if !reported.contains(id) {
            // `blocked` is in `NodeId` order, so `id` is the least member of
            // its strongly connected set not yet reported.
            let members = strongly_connected(id, &next);
            reported.extend(members.iter().copied());
            let rendered: Vec<String> = path.iter().map(ToString::to_string).collect();
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
                        "quest start cycle: {} — each `start` waits for the next and no source \
                         outside the cycle starts any of them; start one of them from something \
                         outside the cycle, or grant it with `accept=\"external\"`",
                        rendered.join(" -> ")
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

/// The shortest cycle through `start` along `next` that is not its
/// self-loop, `start -> … -> start`; `None` when there is none.
fn shortest_cycle(
    start: &NodeId,
    next: &BTreeMap<&NodeId, BTreeSet<&NodeId>>,
) -> Option<Vec<NodeId>> {
    let mut parent: BTreeMap<&NodeId, &NodeId> = BTreeMap::new();
    let mut queue: VecDeque<&NodeId> = VecDeque::new();
    for n in next.get(start).into_iter().flatten() {
        if *n != start && !parent.contains_key(n) {
            parent.insert(n, start);
            queue.push_back(n);
        }
    }
    while let Some(n) = queue.pop_front() {
        for &m in next.get(n).into_iter().flatten() {
            if m == start {
                let mut chain = Vec::new();
                let mut at = n;
                while at != start {
                    chain.push(at.clone());
                    at = parent[at];
                }
                chain.reverse();
                let mut path = Vec::with_capacity(chain.len() + 2);
                path.push(start.clone());
                path.extend(chain);
                path.push(start.clone());
                return Some(path);
            }
            if m != start && !parent.contains_key(m) {
                parent.insert(m, n);
                queue.push_back(m);
            }
        }
    }
    None
}

/// Every node reachable from `start` along `next` that reaches it back.
fn strongly_connected<'a>(
    start: &'a NodeId,
    next: &BTreeMap<&'a NodeId, BTreeSet<&'a NodeId>>,
) -> BTreeSet<&'a NodeId> {
    let forward = reachable(start, next);
    forward
        .iter()
        .copied()
        .filter(|n| reachable(n, next).contains(start))
        .collect()
}

fn reachable<'a>(
    from: &'a NodeId,
    next: &BTreeMap<&'a NodeId, BTreeSet<&'a NodeId>>,
) -> BTreeSet<&'a NodeId> {
    let mut seen = BTreeSet::new();
    let mut stack = vec![from];
    while let Some(n) = stack.pop() {
        for &m in next.get(n).into_iter().flatten() {
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
