//! The bare `lute scenario` graph view (and `--facts`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::process::ExitCode;

use crate::cmd_scenario::reach::unanchored_quests;
use crate::project::{ByRoot, DocGroup};

/// Group `g`'s nodes into deterministic topological WAVES (Kahn's
/// algorithm, but collecting every currently-zero-in-degree node as ONE
/// layer at a time rather than draining a ready-queue one node at a time
/// like [`lute_check::connectivity::assemble_graph`]'s own internal
/// `topo_sort`) — a presentation concern specific to `lute scenario`'s
/// graph view, layered here rather than in `lute-check` (which only needs
/// the flat order). A node stuck in a prerequisite cycle never becomes
/// ready and is simply absent from every layer (already `E-CONN-CYCLE`'s
/// problem, reported by `check-project`, not this read-only view's).
pub(crate) fn topo_layers(
    g: &lute_check::connectivity::ConnGraph,
) -> Vec<Vec<lute_check::connectivity::NodeId>> {
    let mut in_degree: BTreeMap<lute_check::connectivity::NodeId, usize> =
        g.nodes.keys().map(|id| (id.clone(), 0)).collect();
    for targets in g.edges.values() {
        for target in targets {
            *in_degree.entry(target.clone()).or_insert(0) += 1;
        }
    }
    let mut layers = Vec::new();
    loop {
        let mut ready: Vec<lute_check::connectivity::NodeId> = in_degree
            .iter()
            .filter(|&(_, &d)| d == 0)
            .map(|(id, _)| id.clone())
            .collect();
        if ready.is_empty() {
            break;
        }
        ready.sort();
        for id in &ready {
            in_degree.remove(id);
            if let Some(targets) = g.edges.get(id) {
                for target in targets {
                    if let Some(d) = in_degree.get_mut(target) {
                        *d -= 1;
                    }
                }
            }
        }
        layers.push(ready);
    }
    layers
}

/// The comma-joined atom-kind token(s) justifying the `from -> to` edge
/// (lang 0.8.0) — `visited`, `completed`, `active`, or a combination when one
/// formula reaches the same node through more than one atom. Rendered from
/// `ConnGraph::edge_kinds_for`'s `BTreeSet`, so the order is `EdgeKind`'s own
/// and the output is deterministic. A `?` marks the impossible case of an
/// `edges` pair with no recorded kind — never fabricates a kind it cannot
/// read off the graph.
pub(crate) fn edge_kinds_text(
    graph: &lute_check::connectivity::ConnGraph,
    from: &lute_check::connectivity::NodeId,
    to: &lute_check::connectivity::NodeId,
) -> String {
    match graph.edge_kinds_for(from, to) {
        Some(kinds) => kinds
            .iter()
            .map(|k| k.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        None => "?".to_string(),
    }
}

fn print_graph_for_root(
    out: &mut String,
    root: &Path,
    graph: &lute_check::connectivity::ConnGraph,
    unanchored: &[lute_check::connectivity::NodeId],
    when_visited: &[(lute_check::connectivity::NodeId, Vec<String>)],
    facts: Option<&FactGraph>,
) {
    outln!(out, "project root: {}", root.display());
    if graph.nodes.is_empty() {
        outln!(out, "  (no scene/quest nodes)");
        print_unanchored(out, unanchored, when_visited);
        return;
    }
    let layers = topo_layers(facts.map_or(graph, |f| &f.layered));
    if facts.is_some() {
        outln!(out, "  topological layers (after: and fact edges):");
    } else {
        outln!(out, "  topological layers:");
    }
    for (i, layer) in layers.iter().enumerate() {
        let names: Vec<String> = layer.iter().map(|n| n.to_string()).collect();
        outln!(out, "    layer {i}: {}", names.join(", "));
    }
    let layered: BTreeSet<lute_check::connectivity::NodeId> =
        layers.iter().flatten().cloned().collect();
    if layered.len() < graph.nodes.len() {
        let stuck: Vec<String> = graph
            .nodes
            .keys()
            .filter(|id| !layered.contains(id))
            .map(|n| n.to_string())
            .collect();
        outln!(
            out,
            "    (unlayered -- part of a prerequisite cycle, E-CONN-CYCLE): {}",
            stuck.join(", ")
        );
    }
    outln!(out, "  edges (prerequisite -> dependent) [atom kind(s)]:");
    let mut printed_any = false;
    for (from, targets) in &graph.edges {
        for to in targets {
            outln!(
                out,
                "    {from} -> {to} [{}]",
                edge_kinds_text(graph, from, to)
            );
            printed_any = true;
        }
    }
    if !printed_any {
        outln!(out, "    (none)");
    }
    if let Some(facts) = facts {
        outln!(out, "  fact edges (producer -> reader) [asserted fact]:");
        if facts.edges.is_empty() {
            outln!(out, "    (none)");
        }
        for (e, layered) in &facts.edges {
            let note = if *layered {
                ""
            } else if graph.nodes.contains_key(&e.from) {
                " (not layered: it closes a cycle)"
            } else {
                " (not layered: the producer is no graph node)"
            };
            outln!(
                out,
                "    {} -> {} [{}]{note}",
                e.from,
                e.to,
                fact_edge_label(e)
            );
        }
    }
    print_unanchored(out, unanchored, when_visited);
}

/// dsl 0.21.0 §7a.5: a quest without `follows=` is in no layer and on no edge,
/// and used to be absent from this report entirely. Named here instead.
/// dsl 0.25.0 §3: so is a beat whose `when` reads `visited()` but that
/// declares no `after` — gated, yet drawn as an entry point — with the
/// `after` that would draw its edge ([`when_visited_hint`]).
fn print_unanchored(
    out: &mut String,
    unanchored: &[lute_check::connectivity::NodeId],
    when_visited: &[(lute_check::connectivity::NodeId, Vec<String>)],
) {
    if unanchored.is_empty() && when_visited.is_empty() {
        return;
    }
    outln!(
        out,
        "  unanchored (no `after` / `follows` — available from the start of play; no \
         prerequisites in this graph):"
    );
    for node in unanchored {
        outln!(out, "    {node}");
    }
    for (node, ids) in when_visited {
        outln!(out, "    {node} — {}", when_visited_hint(node, ids));
    }
}

/// dsl 0.25.0 §3: the hint for a beat gated by `visited()` conjuncts of its
/// `when` that draw no edge — the `after` to write instead.
pub(crate) fn when_visited_hint(node: &lute_check::connectivity::NodeId, ids: &[String]) -> String {
    let reads: Vec<String> = ids.iter().map(|k| format!("visited('{k}')")).collect();
    let formula = reads.join(" && ");
    let write = match node {
        lute_check::connectivity::NodeId::Scene(_) => format!("`after: \"{formula}\"`"),
        _ => format!("`after=\"{formula}\"`"),
    };
    format!(
        "its `when` reads {}, which gates it but draws no edge; write {write} to anchor it",
        reads.join(", ")
    )
}

/// dsl 0.23.0 §1: the prerequisite references the graph does not draw —
/// counted and named, so a missing edge is explained where it is missed. A
/// quest's edges come from its `follows`, its subquest tree, its top-level
/// `start` conjuncts and its `::accept`s (dsl 0.24.0 §2, 0.25.0 §4).
fn print_omitted(out: &mut String, omitted: &[lute_check::connectivity::OmittedRef]) {
    use lute_check::connectivity::OmittedRef;
    if omitted.is_empty() {
        return;
    }
    outln!(
        out,
        "  note: {} `visited()`/`completed()`/`active()` reference(s) not drawn — a quest's \
         edges come from its `follows`, its subquest tree, its `start` conjuncts and its \
         `::accept`s:",
        omitted.len()
    );
    for r in omitted {
        match r {
            OmittedRef::Lifecycle { from, kind, quest } => outln!(
                out,
                "    {from} -> {}(\"{quest}\") — quest({quest}) is on no edge (no `follows`, tree, \
                 `start` anchor or `::accept`)",
                kind.as_str()
            ),
            // A condition read gates the quest but is no anchor; copying it
            // into `follows` would replace the quest's real anchors (its
            // `::accept`s, tree, `start`) with a possibly backwards edge, so
            // the note never suggests it (summer S1).
            OmittedRef::Visited { quest, scene, slot } => outln!(
                out,
                "    quest({quest}) reads visited('{scene}') in its {slot} — a condition read, \
                 not an anchor"
            ),
        }
    }
}

pub(crate) fn run_scenario_graph(out: &mut String, by_root: &ByRoot, facts: bool) -> ExitCode {
    if by_root.is_empty() {
        outln!(out, "lute: no .lute files found");
        return ExitCode::SUCCESS;
    }
    for (root, group_full) in by_root {
        let docs: Vec<lute_check::ProjectDoc<'_>> = group_full
            .iter()
            .map(|(p, d, folded)| lute_check::ProjectDoc::new(p, d, &folded.typed))
            .collect();
        let key_set = lute_check::connectivity::scene_key_set(&docs);
        let quest_ids = lute_check::connectivity::quest_id_set(&docs);
        let (graph, _cycle_diags) =
            lute_check::connectivity::assemble_graph(&docs, &key_set, &quest_ids);
        let when_visited = lute_check::connectivity::when_visited_unanchored(&docs, &graph);
        let fact_graph = facts.then(|| FactGraph::of(group_full, &docs, &graph));
        print_graph_for_root(
            out,
            root,
            &graph,
            &unanchored_quests(&quest_ids, &graph),
            &when_visited,
            fact_graph.as_ref(),
        );
        print_omitted(
            out,
            &lute_check::connectivity::omitted_refs(&docs, &graph, &quest_ids),
        );
    }
    ExitCode::SUCCESS
}

/// dsl 0.26.0 §8 (T3-11, `lute scenario --facts`): the root's fact-producer
/// edges ([`lute_check::fact_edges::fact_edges`]) and the graph the layers
/// are drawn from — `after:` edges plus every fact edge between two graph
/// nodes, each added in edge order only where it closes no cycle (a fact
/// edge is a "may", so a cycle through one says nothing about `after:`).
pub(crate) struct FactGraph {
    /// Every edge, with whether it is layered.
    pub edges: Vec<(lute_check::fact_edges::FactEdge, bool)>,
    pub layered: lute_check::connectivity::ConnGraph,
}

impl FactGraph {
    pub(crate) fn of(
        group_full: &DocGroup,
        docs: &[lute_check::ProjectDoc<'_>],
        graph: &lute_check::connectivity::ConnGraph,
    ) -> Self {
        use lute_check::connectivity::NodeId;
        let foldeds: Vec<&lute_check::FoldedEnv> = group_full.iter().map(|(_, _, f)| f).collect();
        let mut layered = graph.clone();
        let reaches = |g: &lute_check::connectivity::ConnGraph, from: &NodeId, to: &NodeId| {
            let mut seen = BTreeSet::new();
            let mut stack = vec![from];
            while let Some(n) = stack.pop() {
                if n == to {
                    return true;
                }
                if seen.insert(n) {
                    stack.extend(g.edges.get(n).into_iter().flatten());
                }
            }
            false
        };
        let edges = lute_check::fact_edges::fact_edges(docs, &foldeds, graph)
            .into_iter()
            .map(|e| {
                let joins = graph.nodes.contains_key(&e.from)
                    && graph.nodes.contains_key(&e.to)
                    && (layered
                        .edges
                        .get(&e.from)
                        .is_some_and(|t| t.contains(&e.to))
                        || !reaches(&layered, &e.to, &e.from));
                if joins {
                    layered
                        .edges
                        .entry(e.from.clone())
                        .or_default()
                        .insert(e.to.clone());
                }
                (e, joins)
            })
            .collect();
        FactGraph { edges, layered }
    }
}

/// One fact edge's bracket: the asserted fact, and the gate's derived fact
/// it serves.
pub(crate) fn fact_edge_label(e: &lute_check::fact_edges::FactEdge) -> String {
    match &e.via {
        Some(via) => format!("{}, via {via}", e.fact),
        None => e.fact.clone(),
    }
}
