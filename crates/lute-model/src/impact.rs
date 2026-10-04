//! Reverse dependency impact queries over [`crate::graph::SemanticGraph`].
use crate::graph::{fact_overlap, NodeKey, NodeKind, SemanticGraph};
use crate::ProjectModel;
use lute_core_span::{Evidence, Span};
use lute_syntax::datalog::parse_fact;
use serde::Serialize;
use std::collections::{BTreeMap, VecDeque};

/// A user-facing graph target, parsed from `kind:key` or `fact:relation(args)`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ImpactTarget {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key: Option<String>,
}
impl ImpactTarget {
    /// Parse a CLI target into its structured representation.
    pub fn parse(raw: &str) -> Result<Self, String> {
        if let Some((rel, rest)) = raw.strip_prefix("fact:").and_then(|s| s.split_once('(')) {
            let args = rest
                .strip_suffix(')')
                .ok_or_else(|| "fact target must end with `)`".to_string())?;
            let parsed = parse_fact(&format!("{rel}({args})"))
                .map_err(|_| "malformed fact target".to_string())?;
            return Ok(Self {
                kind: "fact".into(),
                relation: Some(parsed.relation),
                args: Some(
                    parsed
                        .args
                        .into_iter()
                        .map(|a| match a.term {
                            lute_syntax::datalog::FactTerm::Ident(s) => s,
                            lute_syntax::datalog::FactTerm::Bool(v) => v.to_string(),
                            lute_syntax::datalog::FactTerm::Wildcard => "_".into(),
                            lute_syntax::datalog::FactTerm::Param(s) => format!("@{s}"),
                            lute_syntax::datalog::FactTerm::Target => "occasion.target".into(),
                        })
                        .collect(),
                ),
                key: None,
            });
        }
        let (kind, key) = raw
            .split_once(':')
            .ok_or_else(|| "target must be kind:key".to_string())?;
        if !matches!(
            kind,
            "state"
                | "relation"
                | "scene"
                | "quest"
                | "objective"
                | "entry"
                | "beat"
                | "occasion"
                | "def"
                | "component"
        ) || key.trim().is_empty()
        {
            return Err("unknown or empty impact target".into());
        }
        if kind == "state" && key.split('.').any(|part| part.is_empty()) {
            return Err("malformed state target".into());
        }
        Ok(Self {
            kind: kind.into(),
            relation: None,
            args: None,
            key: Some(key.into()),
        })
    }
    /// Convert this target into its graph node key.
    pub fn node(&self) -> NodeKey {
        match self.kind.as_str() {
            "fact" => NodeKey::new(
                NodeKind::Fact,
                format!(
                    "{}({})",
                    self.relation.as_deref().unwrap_or_default(),
                    self.args.clone().unwrap_or_default().join(",")
                ),
            ),
            "state" => NodeKey::new(NodeKind::State, self.key.clone().unwrap_or_default()),
            "relation" => NodeKey::new(NodeKind::Relation, self.key.clone().unwrap_or_default()),
            "scene" => NodeKey::new(NodeKind::Scene, self.key.clone().unwrap_or_default()),
            "quest" => NodeKey::new(NodeKind::Quest, self.key.clone().unwrap_or_default()),
            "objective" => NodeKey::new(NodeKind::Objective, self.key.clone().unwrap_or_default()),
            "entry" => NodeKey::new(NodeKind::Entry, self.key.clone().unwrap_or_default()),
            "beat" => NodeKey::new(NodeKind::Beat, self.key.clone().unwrap_or_default()),
            "occasion" => NodeKey::new(NodeKind::Occasion, self.key.clone().unwrap_or_default()),
            "def" => NodeKey::new(NodeKind::Def, self.key.clone().unwrap_or_default()),
            "component" => NodeKey::new(NodeKind::Component, self.key.clone().unwrap_or_default()),
            _ => NodeKey::new(NodeKind::Engine, self.key.clone().unwrap_or_default()),
        }
    }
}
/// One edge in an impact explanation chain.
#[derive(Clone, Debug, Serialize)]
pub struct ImpactLink {
    pub edge: String,
    pub reason: String,
    pub file: Option<String>,
    pub line: Option<u32>,
    #[serde(rename = "sourceSpan", skip_serializing_if = "Option::is_none")]
    pub source_span: Option<Span>,
}
/// One affected graph node and its strongest explanation.
#[derive(Clone, Debug, Serialize)]
pub struct ImpactItem {
    pub key: String,
    pub evidence: Evidence,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,
    #[serde(rename = "lineId", skip_serializing_if = "Option::is_none")]
    pub line_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speaker: Option<String>,
    pub reasons: Vec<ImpactLink>,
}
/// The deterministic reverse-impact result.
#[derive(Clone, Debug, Serialize)]
pub struct ImpactReport {
    pub target: ImpactTarget,
    pub root: String,
    pub items: BTreeMap<String, Vec<ImpactItem>>,
}

/// Build an impact report from the model's semantic graph.
pub fn query(model: &ProjectModel, target: &ImpactTarget) -> ImpactReport {
    query_graph(model.graph(), model.root().to_string_lossy().to_string(), target)
}
/// Traverse a graph from `target`, retaining the strongest explanation paths.
pub fn query_graph(graph: &SemanticGraph, root: String, target: &ImpactTarget) -> ImpactReport {
    let root_node = target.node();
    let mut starts = Vec::new();
    for key in graph.nodes.keys() {
        if key == &root_node
            || (key.kind == NodeKind::Fact
                && root_node.kind == NodeKind::Fact
                && fact_overlap(&key.key, &root_node.key).is_some())
        {
            starts.push(key.clone());
        }
    }
    if starts.is_empty() {
        starts.push(root_node.clone());
    }
    let mut best: BTreeMap<NodeKey, (Evidence, usize, Vec<NodeKey>, Vec<usize>)> = BTreeMap::new();
    let mut queue = VecDeque::new();
    for s in starts {
        queue.push_back((s, Evidence::Proven, Vec::new(), Vec::new()));
    }
    while let Some((node, ev, path, edges)) = queue.pop_front() {
        let mut path2 = path.clone();
        path2.push(node.clone());
        let Some(adj) = graph.outgoing.get(&node) else {
            continue;
        };
        for &idx in adj {
            let edge = &graph.edges[idx];
            if edge.source.kind == NodeKind::Fact {
                if fact_overlap(&edge.source.key, &node.key).is_none() {
                    continue;
                }
            }
            if edge.target == edge.source {
                continue;
            }
            let next_ev = weaker(&ev, &edge.evidence);
            let mut ep = edges.clone();
            ep.push(idx);
            let replace = best.get(&edge.target).map_or(true, |(old, n, oldp, _)| {
                rank(&next_ev) > rank(old)
                    || (rank(&next_ev) == rank(old)
                        && (ep.len() < *n || (ep.len() == *n && path_key(&path2) < path_key(oldp))))
            });
            if replace {
                best.insert(
                    edge.target.clone(),
                    (next_ev.clone(), ep.len(), path2.clone(), ep.clone()),
                );
                queue.push_back((edge.target.clone(), next_ev, path2.clone(), ep));
            }
        }
    }
    let mut groups: BTreeMap<String, Vec<ImpactItem>> = BTreeMap::from([
        ("lines".into(), vec![]),
        ("quests".into(), vec![]),
        ("objectives".into(), vec![]),
        ("rewards".into(), vec![]),
        ("disclosures".into(), vec![]),
        ("beats".into(), vec![]),
        ("downstream".into(), vec![]),
    ]);
    for (node, (ev, _, _, edge_idxs)) in best {
        if node == root_node {
            continue;
        }
        let group = match node.kind {
            NodeKind::Line => "lines",
            NodeKind::Quest => "quests",
            NodeKind::Objective => "objectives",
            NodeKind::Reward => "rewards",
            NodeKind::Entry => "disclosures",
            NodeKind::Beat => "disclosures",
            NodeKind::Scene | NodeKind::Shot | NodeKind::Occasion => "beats",
            NodeKind::Fact | NodeKind::State | NodeKind::Relation | NodeKind::Engine => {
                "downstream"
            }
            _ => "downstream",
        };
        let declaration = graph.nodes.get(&node);
        let file = declaration
            .and_then(|n| n.file.as_ref())
            .map(|p| display_file(p, &root));
        let line = declaration.and_then(|n| n.span.map(|s| s.line));
        let (line_id, speaker) = if node.kind == NodeKind::Line {
            (Some(node.key.clone()), declaration.and_then(|n| n.speaker.clone()))
        } else {
            (None, None)
        };
        let reasons = edge_idxs
            .into_iter()
            .map(|i| {
                let e = &graph.edges[i];
                ImpactLink {
                    edge: e.kind.clone(),
                    reason: e.reason.clone(),
                    file: e.file.as_ref().map(|p| display_file(p, &root)),
                    line: e.span.map(|s| s.line),
                    source_span: e.span,
                }
            })
            .collect();
        groups.get_mut(group).unwrap().push(ImpactItem {
            key: node.canonical(),
            evidence: ev,
            file,
            line,
            line_id,
            speaker,
            reasons,
        });
    }
    for values in groups.values_mut() {
        values.sort_by(|a, b| a.key.cmp(&b.key));
    }
    ImpactReport {
        target: target.clone(),
        root,
        items: groups,
    }
}
fn rank(e: &Evidence) -> u8 {
    match e {
        Evidence::Proven => 5,
        Evidence::Witnessed => 4,
        Evidence::Bounded { .. } => 3,
        Evidence::Heuristic => 2,
        Evidence::Unknown => 1,
    }
}
fn weaker(a: &Evidence, b: &Evidence) -> Evidence {
    if rank(a) <= rank(b) {
        a.clone()
    } else {
        b.clone()
    }
}
fn path_key(p: &[NodeKey]) -> String {
    p.iter()
        .map(NodeKey::canonical)
        .collect::<Vec<_>>()
        .join("\u{1f}")
}

fn display_file(path: &std::path::Path, root: &str) -> String {
    let root_path = std::path::Path::new(root);
    let normalized_path = path
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/");
    let normalized_root = root_path
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/");
    if let Some(relative) = normalized_path.strip_prefix(&(normalized_root.clone() + "/")) {
        return relative.to_string();
    }
    if path.is_absolute() {
        let absolute_root =
            std::fs::canonicalize(root).unwrap_or_else(|_| std::path::PathBuf::from(root));
        let absolute_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if let Ok(relative) = absolute_path.strip_prefix(&absolute_root) {
            return relative
                .to_string_lossy()
                .replace(std::path::MAIN_SEPARATOR, "/");
        }
    }
    normalized_path
}

/// Render an impact report in the stable human-readable format.
pub fn human(report: &ImpactReport) -> String {
    let mut out = String::new();
    for (group, items) in &report.items {
        if items.is_empty() {
            continue;
        }
        out.push_str(group);
        out.push('\n');
        for item in items {
            out.push_str(&format!(
                "  {} [{}]\n",
                item.key,
                evidence_name(&item.evidence)
            ));
            for r in &item.reasons {
                if let Some(file) = &r.file {
                    out.push_str(&format!("    - {}: {} ({}", r.edge, r.reason, file));
                    if let Some(line) = r.line {
                        out.push_str(&format!(":{}", line));
                    }
                    out.push_str(")\n");
                } else {
                    out.push_str(&format!("    - {}: {}\n", r.edge, r.reason));
                }
            }
        }
    }
    if out.is_empty() {
        out.push_str("no affected items\n");
    }
    out
}
fn evidence_name(e: &Evidence) -> &'static str {
    match e {
        Evidence::Proven => "proven",
        Evidence::Witnessed => "witnessed",
        Evidence::Bounded { .. } => "bounded",
        Evidence::Heuristic => "heuristic",
        Evidence::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{GraphEdge, SemanticGraph};

    #[test]
    fn reverse_closure_terminates_on_cycles_and_excludes_self_reads() {
        let a = NodeKey::new(NodeKind::State, "a");
        let b = NodeKey::new(NodeKind::State, "b");
        let mut graph = SemanticGraph::default();
        graph.nodes.insert(
            a.clone(),
            crate::graph::GraphNode {
                id: a.clone(),
                file: None,
                span: None,
                speaker: None,
            },
        );
        graph.nodes.insert(
            b.clone(),
            crate::graph::GraphNode {
                id: b.clone(),
                file: None,
                span: None,
                speaker: None,
            },
        );
        graph.edges = vec![
            GraphEdge {
                source: a.clone(),
                target: a.clone(),
                kind: "reads".into(),
                reason: "self".into(),
                file: None,
                span: None,
                evidence: Evidence::Proven,
            },
            GraphEdge {
                source: a.clone(),
                target: b.clone(),
                kind: "reads".into(),
                reason: "a feeds b".into(),
                file: None,
                span: None,
                evidence: Evidence::Proven,
            },
            GraphEdge {
                source: b.clone(),
                target: a.clone(),
                kind: "reads".into(),
                reason: "b feeds a".into(),
                file: None,
                span: None,
                evidence: Evidence::Proven,
            },
        ];
        graph.outgoing.insert(a.clone(), vec![0, 1]);
        graph.outgoing.insert(b.clone(), vec![2]);
        let target = ImpactTarget::parse("state:a").unwrap();
        let report = query_graph(&graph, "test".into(), &target);
        assert!(report
            .items
            .values()
            .flatten()
            .any(|item| item.key == "state:b"));
        assert!(!report
            .items
            .values()
            .flatten()
            .any(|item| item.key == "state:a"));
    }
}
