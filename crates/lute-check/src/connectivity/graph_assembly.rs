use super::graph_nodes::{
    atom_edge_kind, scene_after, Anchor, ConnGraph, EdgeKind, FollowsDecl, HostGate, NodeId,
    NodeInfo, PrereqState, SceneAfter,
};
use super::{
    graph_sources::{accept_sources, detect_conn_cycles, start_sources, top_conjuncts, topo_sort},
    identity::{bundle_beat_after, bundle_beat_key_set},
};
use crate::raises::RaiseAnchors;
use crate::ProjectDoc;

use crate::meta::meta_key_span;
use crate::prereq::{parse_prereq, E_CONN_PROFILE};
use lute_core_span::{Diagnostic, Span};
use lute_manifest::semantics::prereq::atoms;
use lute_syntax::ast::{Node, Quest};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// An authored `after` / `follows` value at `span`: empty is no
/// declaration; one [`parse_prereq`] cannot read is malformed (its
/// [`E_CONN_PROFILE`] is the per-file check's).
fn declared(raw: &str, span: Span) -> FollowsDecl {
    if raw.is_empty() {
        return FollowsDecl::Absent;
    }
    parse_prereq(raw, span)
        .0
        .map_or(FollowsDecl::Malformed(E_CONN_PROFILE), FollowsDecl::Valid)
}

/// The project's prerequisite graph. `raises` holds the root's raise anchors
/// ([`crate::raises::RaiseTable::anchors`]; empty without declared raises).
pub fn assemble_graph(
    docs: &[ProjectDoc<'_>],
    key_set: &BTreeMap<String, Vec<(PathBuf, Span)>>,
    _quest_ids: &BTreeSet<String>,
    raises: &RaiseAnchors,
) -> (ConnGraph, Vec<(PathBuf, Diagnostic)>) {
    let by_path: BTreeMap<&Path, &ProjectDoc<'_>> =
        docs.iter().map(|item| (item.path, item)).collect();

    let mut nodes: BTreeMap<NodeId, NodeInfo> = BTreeMap::new();

    // Scene nodes: every canonical key T3 resolved, anchored at its FIRST
    // occurrence (a same-key repeat past that is E-CONN-EPISODE-ID-DUP's own
    // problem, T3 -- never this pass's).
    for (key, occurrences) in key_set {
        let Some((path, span)) = occurrences.first() else {
            continue;
        };
        let follows = by_path
            .get(path.as_path())
            .map_or(FollowsDecl::Absent, |item| match scene_after(item.meta) {
                SceneAfter::Absent => FollowsDecl::Absent,
                SceneAfter::NonString => FollowsDecl::Malformed(E_CONN_PROFILE),
                SceneAfter::String(after) => {
                    declared(&after, meta_key_span(&item.doc.meta, "after"))
                }
            });
        nodes.insert(
            NodeId::Scene(key.clone()),
            NodeInfo {
                id: NodeId::Scene(key.clone()),
                path: path.clone(),
                prereq: follows.activation(),
                follows,
                span: *span,
            },
        );
    }

    // Bundle beat nodes: every canonical beat key, anchored at its first
    // occurrence (a repeat is E-CONN-EPISODE-ID-DUP's problem). dsl 0.25.0
    // §3: a beat's `after=` is its prerequisite, exactly as a scene's
    // `after:`; without one it is an entry point.
    for (key, occurrences) in bundle_beat_key_set(docs) {
        let Some((path, span)) = occurrences.into_iter().next() else {
            continue;
        };
        let follows = match by_path
            .get(path.as_path())
            .and_then(|item| bundle_beat_after(item.doc, item.meta, &key))
        {
            None => FollowsDecl::Absent,
            Some((after, after_span)) => declared(after, *after_span),
        };
        let id = NodeId::Beat(key);
        nodes.insert(
            id.clone(),
            NodeInfo {
                id,
                path,
                prereq: follows.activation(),
                follows,
                span,
            },
        );
    }

    // Quest nodes: EVERY nonempty-`after`-declaring quest (dsl §2.1's second
    // `after` surface) is admitted as a node, full stop -- regardless of the
    // caller-supplied `_quest_ids` set (Task 5 review fix: that set may be
    // stale/filtered relative to `docs`; gating SOURCE node admission on it
    // could silently drop a quest -- and its edges/cycles -- from the graph).
    for item in docs {
        let path = item.path;
        let doc = item.doc;
        for quest in &doc.quests {
            let Some(after) = &quest.follows else {
                continue;
            };
            if quest.id.is_empty() {
                continue;
            }
            let follows = declared(after, quest.follows_span);
            nodes.insert(
                NodeId::Quest(quest.id.clone()),
                NodeInfo {
                    id: NodeId::Quest(quest.id.clone()),
                    path: path.to_path_buf(),
                    prereq: follows.activation(),
                    follows,
                    span: quest.id_span,
                },
            );
        }
    }

    // Anchored quest nodes (dsl 0.24.0 §2, 0.25.0 §4): a quest with no
    // `after` on any declaration is anchored by its subquest parent, its
    // `start` conjuncts and the nodes whose body `::accept`s it. Each source
    // quest or entry — and every subquest parent — joins the graph too, an
    // entry point unless anchored itself. Their edges go in below, after the
    // authored ones. dsl 0.53.0 §7.1: a quest that declares `follows` is
    // already a node; its accept anchor becomes its activation and its
    // declaration stays.
    let decls = QuestDecls::new(docs);
    for (id, anchors) in quest_anchors(docs, &decls, &nodes) {
        let Some(&(path, quest)) = decls.first.get(id.as_str()) else {
            continue;
        };
        let node = NodeId::Quest(id);
        if let Some(info) = nodes.get_mut(&node) {
            info.prereq = PrereqState::Anchored(anchors);
            continue;
        }
        nodes.insert(
            node.clone(),
            NodeInfo {
                id: node,
                path: path.to_path_buf(),
                follows: FollowsDecl::Absent,
                prereq: PrereqState::Anchored(anchors),
                span: quest.id_span,
            },
        );
    }
    // 0.51.0 §4.5: a scene or bundle beat without `after:` answering a
    // `raisedBy: declared` occasion is anchored by the nodes whose declared
    // raises reach it — set before the pass below, so a quest that raises
    // becomes a node there. One whose `after:` is written keeps it as its
    // declared route (the quest rule).
    for (node, from) in raises.iter() {
        if !nodes
            .get(node)
            .is_some_and(|info| matches!(info.prereq, PrereqState::Absent))
        {
            continue;
        }
        let mut sources: Vec<NodeId> = Vec::new();
        for f in from {
            let graphed = nodes.contains_key(&f.source)
                || matches!(&f.source, NodeId::Quest(id) if decls.first.contains_key(id.as_str()));
            if graphed && !sources.contains(&f.source) {
                sources.push(f.source.clone());
            }
        }
        if let Some(info) = nodes.get_mut(node).filter(|_| !sources.is_empty()) {
            info.prereq = PrereqState::Anchored(vec![Anchor {
                kind: EdgeKind::Raises,
                from: sources,
                host: None,
            }]);
        }
    }
    let sources: Vec<NodeId> = nodes
        .values()
        .filter_map(|info| match &info.prereq {
            PrereqState::Anchored(anchors) => Some(anchors),
            _ => None,
        })
        .flatten()
        .flat_map(|a| a.from.iter().cloned())
        .chain(decls.parents.values().map(|p| NodeId::Quest(p.to_string())))
        .collect();
    for source in sources {
        if nodes.contains_key(&source) {
            continue;
        }
        let (path, span) = match &source {
            NodeId::Quest(id) => match decls.first.get(id.as_str()) {
                Some(&(path, quest)) => (path, quest.id_span),
                None => continue,
            },
            NodeId::Entry(id) => match decls.entries.get(id.as_str()) {
                Some(&(path, span)) => (path, span),
                None => continue,
            },
            NodeId::Scene(_) | NodeId::Beat(_) => continue,
        };
        nodes.insert(
            source.clone(),
            NodeInfo {
                id: source,
                path: path.to_path_buf(),
                follows: FollowsDecl::Absent,
                prereq: PrereqState::Absent,
                span,
            },
        );
    }

    // Edges: flattened union of atoms per formula -- `atom_target -> n` iff
    // `atom_target` is itself a node above (never a bare-string cross-check
    // against key_set/quest_ids -- membership in `nodes`, typed, is the only
    // question here). Each edge also records the ATOM KIND(s) that justify it
    // (lang 0.8.0 `EdgeKind`) -- `completed(Q)` and `active(Q)` both land on
    // the same `NodeId::Quest(Q)` and are otherwise indistinguishable once
    // flattened, but the envelope pass and `lute scenario` both need to tell
    // them apart. The DAG shape itself is UNCHANGED by the kind: an `active`
    // edge constrains ordering exactly as a `completed` edge does, so cycle
    // detection below stays exactly as strong.
    let mut edges: BTreeMap<NodeId, BTreeSet<NodeId>> = BTreeMap::new();
    let mut edge_kinds: BTreeMap<NodeId, BTreeMap<NodeId, BTreeSet<EdgeKind>>> = BTreeMap::new();
    for info in nodes.values() {
        let FollowsDecl::Valid(formula) = &info.follows else {
            continue;
        };
        for atom in atoms(formula) {
            let target = NodeId::of_atom(&atom, &nodes);
            if nodes.contains_key(&target) {
                add_edge(
                    &mut edges,
                    &mut edge_kinds,
                    target,
                    &info.id,
                    atom_edge_kind(&atom),
                );
            }
        }
    }
    // Synthesized edges go in after every authored one — subquest, then
    // `start`, then `::accept`, then raises. A subquest, `::accept` or raise
    // edge goes in only where it closes no cycle: a source downstream of
    // the node it anchors (a scene whose `after` needs the quest, a scene
    // that raises its own occasion again) cannot be the node's way in, and
    // an anchor is no `after` clause for `E-CONN-CYCLE` to report. Such an
    // anchor stays on the node; reachability reads it `Unknown`. A child
    // that declares its own `after` still hangs off its parent (dsl 0.25.0
    // §4). dsl 0.49.0 §6.1: every `start` edge is drawn; one that closes a
    // cycle (a self-read included) is a back edge, which orders nothing.
    let mut back_edges: BTreeMap<NodeId, BTreeSet<NodeId>> = BTreeMap::new();
    for (child, parent) in &decls.parents {
        let (from, to) = (
            NodeId::Quest(parent.to_string()),
            NodeId::Quest(child.to_string()),
        );
        if nodes.contains_key(&from)
            && nodes.contains_key(&to)
            && !reaches(&edges, &back_edges, &to, &from)
        {
            add_edge(&mut edges, &mut edge_kinds, from, &to, EdgeKind::Subquest);
        }
    }
    for kind in [EdgeKind::Start, EdgeKind::Accept, EdgeKind::Raises] {
        for info in nodes.values() {
            let PrereqState::Anchored(anchors) = &info.prereq else {
                continue;
            };
            for from in anchors
                .iter()
                .filter(|a| a.kind == kind)
                .flat_map(|a| &a.from)
            {
                if !nodes.contains_key(from) {
                    continue;
                }
                let closes = reaches(&edges, &back_edges, &info.id, from);
                if closes && kind != EdgeKind::Start {
                    continue;
                }
                let drawn = edges.get(from).is_some_and(|t| t.contains(&info.id));
                if closes && !drawn {
                    back_edges
                        .entry(from.clone())
                        .or_default()
                        .insert(info.id.clone());
                }
                add_edge(&mut edges, &mut edge_kinds, from.clone(), &info.id, kind);
            }
        }
    }

    let mut diags = Vec::new();
    detect_conn_cycles(&nodes, &edges, &back_edges, &mut diags);

    // Per-node cycle recovery (spec §4.1): build the order UNCONDITIONALLY.
    // Kahn's algorithm below never frees a cycle member (its in-degree never
    // reaches 0) nor anything transitively downstream of one, so those nodes
    // are simply omitted — every cycle-INDEPENDENT node keeps its slot and a
    // sound verdict. `detect_conn_cycles` above still emits `E-CONN-CYCLE`;
    // we only stop blanking the whole root's order.
    let topo_order = topo_sort(&nodes, &edges, &back_edges);

    // `start_blocked` needs reachability: callers run
    // `resolve_start_blocks` on the final verdicts (dsl 0.49.0 §6.1).
    (
        ConnGraph {
            nodes,
            edges,
            edge_kinds,
            topo_order,
            back_edges,
            start_blocked: BTreeMap::new(),
        },
        diags,
    )
}

/// Record the `from -> to` edge with `kind` in both [`ConnGraph`] maps.
pub(crate) fn add_edge(
    edges: &mut BTreeMap<NodeId, BTreeSet<NodeId>>,
    edge_kinds: &mut BTreeMap<NodeId, BTreeMap<NodeId, BTreeSet<EdgeKind>>>,
    from: NodeId,
    to: &NodeId,
    kind: EdgeKind,
) {
    edges.entry(from.clone()).or_default().insert(to.clone());
    edge_kinds
        .entry(from)
        .or_default()
        .entry(to.clone())
        .or_default()
        .insert(kind);
}

/// Whether `to` is `from` or reachable from it along the ordering `edges`
/// (every edge but a `back` one).
pub(crate) fn reaches(
    edges: &BTreeMap<NodeId, BTreeSet<NodeId>>,
    back: &BTreeMap<NodeId, BTreeSet<NodeId>>,
    from: &NodeId,
    to: &NodeId,
) -> bool {
    let mut seen = BTreeSet::new();
    let mut stack = vec![from];
    while let Some(n) = stack.pop() {
        if n == to {
            return true;
        }
        if seen.insert(n) {
            let skip = back.get(n);
            stack.extend(
                edges
                    .get(n)
                    .into_iter()
                    .flatten()
                    .filter(|t| !skip.is_some_and(|s| s.contains(*t))),
            );
        }
    }
    false
}

/// The quests and entries of one resolved root as [`assemble_graph`]
/// anchors them (dsl 0.24.0 §2, 0.25.0 §4).
pub(crate) struct QuestDecls<'a> {
    /// Each quest id's first declaration, with its document.
    pub(crate) first: BTreeMap<&'a str, (&'a Path, &'a Quest)>,
    /// Every quest id some declaration gives an `after` — its declared
    /// route replaces the synthesized subquest and `start` anchors.
    with_after: BTreeSet<&'a str>,
    /// Every quest id some declaration marks `accept="external"` (dsl
    /// 0.25.0 §5): the host may grant it.
    external: BTreeSet<&'a str>,
    /// Declared subquest child → the first declared parent whose
    /// `<objective quest=…>` names it. A self-reference is
    /// `E-QUEST-TREE-CYCLE`'s and makes no child here.
    parents: BTreeMap<&'a str, &'a str>,
    /// Each lore entry id's first declaration: its document and id span.
    pub(crate) entries: BTreeMap<&'a str, (&'a Path, Span)>,
}

impl<'a> QuestDecls<'a> {
    pub(crate) fn new(docs: &'a [ProjectDoc<'a>]) -> Self {
        let mut first: BTreeMap<&str, (&Path, &Quest)> = BTreeMap::new();
        let mut with_after = BTreeSet::new();
        let mut external = BTreeSet::new();
        let mut entries = BTreeMap::new();
        for item in docs {
            let path = item.path;
            let doc = item.doc;
            for quest in doc.quests.iter().filter(|q| !q.id.is_empty()) {
                first.entry(quest.id.as_str()).or_insert((path, quest));
                if quest.follows.is_some() {
                    with_after.insert(quest.id.as_str());
                }
                if quest.accepted_externally() {
                    external.insert(quest.id.as_str());
                }
            }
            for entry in doc.entries.iter().filter(|e| !e.id.is_empty()) {
                entries
                    .entry(entry.id.as_str())
                    .or_insert((path, entry.id_span));
            }
        }
        let mut parents = BTreeMap::new();
        for item in docs {
            let doc = item.doc;
            for quest in doc.quests.iter().filter(|q| !q.id.is_empty()) {
                for node in &quest.body {
                    let Node::Objective(o) = node else { continue };
                    if let Some(child) = o.quest.as_deref() {
                        if let Some((&child, _)) = first.get_key_value(child) {
                            if child != quest.id {
                                parents.entry(child).or_insert(quest.id.as_str());
                            }
                        }
                    }
                }
            }
        }
        QuestDecls {
            first,
            with_after,
            external,
            parents,
            entries,
        }
    }
}

/// The [`PrereqState::Anchored`] anchors of every quest, in the order
/// subquest parent, `start` conjuncts ([`start_anchors`]), accepts
/// ([`accept_sources`]). A quest that declares `follows` gets no subquest
/// or `start` anchor (its declared route replaces them, dsl 0.25.0 §4) but
/// keeps its accept anchor (dsl 0.53.0 §7.1: an in-script accept activates
/// it whatever `follows` says). The accept anchor of an accept-driven
/// `accept="external"` quest also holds the host route ([`host_gate`]). An
/// accept-driven quest that declares `follows` always gets its accept
/// anchor, even with no source and no host route: `follows` is never its
/// route, so nothing then activates it. One with no in-script source, no
/// host route and no `follows` is no anchor (the quest stays unanchored,
/// available from the start of play). A quest none of them anchors is
/// absent. `nodes` holds the scene and bundle beat nodes the sources
/// resolve against, and the nodes of the quests that declare `follows`.
pub(crate) fn quest_anchors(
    docs: &[ProjectDoc<'_>],
    decls: &QuestDecls<'_>,
    nodes: &BTreeMap<NodeId, NodeInfo>,
) -> BTreeMap<String, Vec<Anchor>> {
    let driven = crate::accept::accept_driven_quests(docs);
    let mut accepts = accept_sources(docs, &driven, nodes);
    let mut out = BTreeMap::new();
    for (&id, &(_, quest)) in &decls.first {
        let mut anchors = Vec::new();
        if !decls.with_after.contains(id) {
            if let Some(parent) = decls.parents.get(id) {
                anchors.push(Anchor {
                    kind: EdgeKind::Subquest,
                    from: vec![NodeId::Quest(parent.to_string())],
                    host: None,
                });
            }
            anchors.extend(start_anchors(quest, decls, nodes));
        }
        let from = accepts.remove(id).unwrap_or_default();
        let host = (decls.external.contains(id) && driven.contains(id))
            .then(|| host_gate(nodes.get(&NodeId::Quest(id.to_string()))));
        let routed = !from.is_empty() || host.as_ref().is_some_and(|h| *h != HostGate::Always);
        if routed || (driven.contains(id) && decls.with_after.contains(id)) {
            anchors.push(Anchor {
                kind: EdgeKind::Accept,
                from,
                host,
            });
        }
        if !anchors.is_empty() {
            out.insert(id.to_string(), anchors);
        }
    }
    out
}

/// dsl 0.53.0 §7.1: the host route of an `accept="external"` quest whose
/// node (one exists when it declares `follows`) is `info`.
fn host_gate(info: Option<&NodeInfo>) -> HostGate {
    match info.map(|i| &i.follows) {
        None | Some(FollowsDecl::Absent) => HostGate::Always,
        Some(FollowsDecl::Valid(f)) => HostGate::Follows(f.clone()),
        Some(FollowsDecl::Malformed(_)) => HostGate::Unreadable,
    }
}

/// dsl 0.25.0 §4: one [`EdgeKind::Start`] anchor per top-level `&&`
/// conjunct of `quest`'s `start` that reads a graph node — `visited('K')`
/// (a scene or bundle beat node), `entry.X.everRead` (bare or `== true`, a
/// declared entry) or `quest.Y.state == '<state>'` (any declared quest —
/// `quest` itself too, a self-source, dsl 0.49.0 §6.2 — any state but
/// `unset`) — or is an `||` of such reads (one
/// anchor, several sources). Every other conjunct gates without anchoring;
/// an unparseable `start` is the per-file check's and anchors nothing.
pub(crate) fn start_anchors(
    quest: &Quest,
    decls: &QuestDecls<'_>,
    nodes: &BTreeMap<NodeId, NodeInfo>,
) -> Vec<Anchor> {
    let Some(start) = &quest.start else {
        return Vec::new();
    };
    let mut arena = lute_cel::CelArena::default();
    let Some(root) = lute_cel::parse_slot_marked_refs(&mut arena, &start.raw)
        .and_then(|h| arena.get(h).cloned())
    else {
        return Vec::new();
    };
    let mut conjuncts = Vec::new();
    top_conjuncts(&root.expr, &mut conjuncts);
    conjuncts
        .into_iter()
        .filter_map(|c| start_sources(c, decls, nodes))
        .map(|from| Anchor {
            kind: EdgeKind::Start,
            from,
            host: None,
        })
        .collect()
}
