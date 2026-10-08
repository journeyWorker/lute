use super::graph_nodes::*;
use super::{graph_sources::*, identity::*};
use crate::ProjectDoc;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use lute_core_span::{Diagnostic, Span};
use lute_syntax::ast::{Quest, Node};
use crate::meta::meta_key_span;
use crate::prereq::parse_prereq;
use lute_manifest::semantics::prereq::atoms;

pub fn assemble_graph(
    docs: &[ProjectDoc<'_>],
    key_set: &BTreeMap<String, Vec<(PathBuf, Span)>>,
    _quest_ids: &BTreeSet<String>,
) -> (ConnGraph, Vec<(PathBuf, Diagnostic)>) {
    let by_path: BTreeMap<&Path, &ProjectDoc<'_>> = docs.iter().map(|item| (item.path, item)).collect();

    let mut nodes: BTreeMap<NodeId, NodeInfo> = BTreeMap::new();

    // Scene nodes: every canonical key T3 resolved, anchored at its FIRST
    // occurrence (a same-key repeat past that is E-CONN-EPISODE-ID-DUP's own
    // problem, T3 -- never this pass's).
    for (key, occurrences) in key_set {
        let Some((path, span)) = occurrences.first() else {
            continue;
        };
        let prereq =
            by_path
                .get(path.as_path())
                .map_or(PrereqState::Absent, |item| match scene_after(item.meta) {
                    SceneAfter::Absent => PrereqState::Absent,
                    SceneAfter::NonString => PrereqState::Invalid,
                    SceneAfter::String(after) if after.is_empty() => PrereqState::Absent,
                    SceneAfter::String(after) => {
                        let after_span = meta_key_span(&item.doc.meta, "after");
                        match parse_prereq(&after, after_span).0 {
                            Some(f) => PrereqState::Valid(f),
                            None => PrereqState::Invalid,
                        }
                    }
                });
        nodes.insert(
            NodeId::Scene(key.clone()),
            NodeInfo {
                id: NodeId::Scene(key.clone()),
                path: path.clone(),
                prereq,
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
        let prereq = match by_path
            .get(path.as_path())
            .and_then(|item| bundle_beat_after(item.doc, item.meta, &key))
        {
            None => PrereqState::Absent,
            Some((after, _)) if after.is_empty() => PrereqState::Absent,
            Some((after, after_span)) => match parse_prereq(after, *after_span).0 {
                Some(f) => PrereqState::Valid(f),
                None => PrereqState::Invalid,
            },
        };
        let id = NodeId::Beat(key);
        nodes.insert(
            id.clone(),
            NodeInfo {
                id,
                path,
                prereq,
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
            let prereq = if after.is_empty() {
                PrereqState::Absent
            } else {
                match parse_prereq(after, quest.follows_span).0 {
                    Some(f) => PrereqState::Valid(f),
                    None => PrereqState::Invalid,
                }
            };
            nodes.insert(
                NodeId::Quest(quest.id.clone()),
                NodeInfo {
                    id: NodeId::Quest(quest.id.clone()),
                    path: path.to_path_buf(),
                    prereq,
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
    // authored ones.
    let decls = QuestDecls::new(docs);
    for (id, anchors) in quest_anchors(docs, &decls, &nodes) {
        let Some(&(path, quest)) = decls.first.get(id.as_str()) else {
            continue;
        };
        let node = NodeId::Quest(id);
        nodes.insert(
            node.clone(),
            NodeInfo {
                id: node,
                path: path.to_path_buf(),
                prereq: PrereqState::Anchored(anchors),
                span: quest.id_span,
            },
        );
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
        let PrereqState::Valid(formula) = &info.prereq else {
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
    // `start`, then `::accept` — and each only where it closes no cycle: a
    // source downstream of the quest it anchors (a scene whose `after` needs
    // the quest) cannot be the quest's way in, and an anchor is no `after`
    // clause for `E-CONN-CYCLE` to report. Such an anchor stays on the node;
    // reachability reads it `Unknown` (it is never ordered before the
    // quest). A child that declares its own `after` still hangs off its
    // parent (dsl 0.25.0 §4).
    for (child, parent) in &decls.parents {
        let (from, to) = (
            NodeId::Quest(parent.to_string()),
            NodeId::Quest(child.to_string()),
        );
        if nodes.contains_key(&from) && nodes.contains_key(&to) && !reaches(&edges, &to, &from) {
            add_edge(&mut edges, &mut edge_kinds, from, &to, EdgeKind::Subquest);
        }
    }
    for kind in [EdgeKind::Start, EdgeKind::Accept] {
        for info in nodes.values() {
            let PrereqState::Anchored(anchors) = &info.prereq else {
                continue;
            };
            for from in anchors
                .iter()
                .filter(|a| a.kind == kind)
                .flat_map(|a| &a.from)
            {
                if nodes.contains_key(from) && !reaches(&edges, &info.id, from) {
                    add_edge(&mut edges, &mut edge_kinds, from.clone(), &info.id, kind);
                }
            }
        }
    }

    let mut diags = Vec::new();
    detect_conn_cycles(&nodes, &edges, &mut diags);

    // Per-node cycle recovery (spec §4.1): build the order UNCONDITIONALLY.
    // Kahn's algorithm below never frees a cycle member (its in-degree never
    // reaches 0) nor anything transitively downstream of one, so those nodes
    // are simply omitted — every cycle-INDEPENDENT node keeps its slot and a
    // sound verdict. `detect_conn_cycles` above still emits `E-CONN-CYCLE`;
    // we only stop blanking the whole root's order.
    let topo_order = topo_sort(&nodes, &edges);

    (
        ConnGraph {
            nodes,
            edges,
            edge_kinds,
            topo_order,
        },
        diags,
    )
}

/// Record the `from -> to` edge with `kind` in both [`ConnGraph`] maps.
pub(super) fn add_edge(
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

/// Whether `to` is `from` or reachable from it along `edges`.
pub(super) fn reaches(edges: &BTreeMap<NodeId, BTreeSet<NodeId>>, from: &NodeId, to: &NodeId) -> bool {
    let mut seen = BTreeSet::new();
    let mut stack = vec![from];
    while let Some(n) = stack.pop() {
        if n == to {
            return true;
        }
        if seen.insert(n) {
            stack.extend(edges.get(n).into_iter().flatten());
        }
    }
    false
}

/// The quests and entries of one resolved root as [`assemble_graph`]
/// anchors them (dsl 0.24.0 §2, 0.25.0 §4).
pub(super) struct QuestDecls<'a> {
    /// Each quest id's first declaration, with its document.
    pub(crate) first: BTreeMap<&'a str, (&'a Path, &'a Quest)>,
    /// Every quest id some declaration gives an `after` — its declared
    /// route replaces the synthesized anchors.
    with_after: BTreeSet<&'a str>,
    /// Declared subquest child → the first declared parent whose
    /// `<objective quest=…>` names it. A self-reference is
    /// `E-QUEST-TREE-CYCLE`'s and makes no child here.
    parents: BTreeMap<&'a str, &'a str>,
    /// Each lore entry id's first declaration: its document and id span.
    pub(crate) entries: BTreeMap<&'a str, (&'a Path, Span)>,
}

impl<'a> QuestDecls<'a> {
    fn new(docs: &'a [ProjectDoc<'a>]) -> Self {
        let mut first: BTreeMap<&str, (&Path, &Quest)> = BTreeMap::new();
        let mut with_after = BTreeSet::new();
        let mut entries = BTreeMap::new();
        for item in docs {
            let path = item.path;
            let doc = item.doc;
            for quest in doc.quests.iter().filter(|q| !q.id.is_empty()) {
                first
                    .entry(quest.id.as_str())
                    .or_insert((path, quest));
                if quest.follows.is_some() {
                    with_after.insert(quest.id.as_str());
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
            parents,
            entries,
        }
    }
}

/// The [`PrereqState::Anchored`] anchors of every quest that declares no
/// `after` (dsl 0.24.0 §2, 0.25.0 §4), in the order subquest parent,
/// `start` conjuncts ([`start_anchors`]), `::accept`s ([`accept_sources`]).
/// A quest none of them anchors is absent. `nodes` holds the scene and
/// bundle beat nodes the sources resolve against.
pub(super) fn quest_anchors(
    docs: &[ProjectDoc<'_>],
    decls: &QuestDecls<'_>,
    nodes: &BTreeMap<NodeId, NodeInfo>,
) -> BTreeMap<String, Vec<Anchor>> {
    let mut accepts = accept_sources(docs, nodes);
    let mut out = BTreeMap::new();
    for (&id, &(_, quest)) in &decls.first {
        if decls.with_after.contains(id) {
            continue;
        }
        let mut anchors = Vec::new();
        if let Some(parent) = decls.parents.get(id) {
            anchors.push(Anchor {
                kind: EdgeKind::Subquest,
                from: vec![NodeId::Quest(parent.to_string())],
            });
        }
        anchors.extend(start_anchors(quest, decls, nodes));
        if let Some(from) = accepts.remove(id) {
            anchors.push(Anchor {
                kind: EdgeKind::Accept,
                from,
            });
        }
        if !anchors.is_empty() {
            out.insert(id.to_string(), anchors);
        }
    }
    out
}

/// dsl 0.25.0 §4: one [`EdgeKind::Start`] anchor per top-level `&&`
/// conjunct of `quest`'s `start` that reads a graph node — `visited('K')`
/// (a scene or bundle beat node), `entry.X.everRead` (bare or `== true`, a
/// declared entry) or `quest.Y.state == '<state>'` (a declared quest other
/// than `quest`, any state but `unset`) — or is an `||` of such reads (one
/// anchor, several sources). Every other conjunct gates without anchoring;
/// an unparseable `start` is the per-file check's and anchors nothing.
pub(super) fn start_anchors(
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
        .filter_map(|c| start_sources(c, &quest.id, decls, nodes))
        .map(|from| Anchor {
            kind: EdgeKind::Start,
            from,
        })
        .collect()
}

