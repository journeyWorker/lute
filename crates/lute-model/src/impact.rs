//! Reverse dependency impact queries over [`lute_semantic::SemanticGraph`].
use crate::ProjectModel;
use lute_core_span::{Evidence, Span};
use lute_manifest::fact::{parse_fact, DatalogError};
use lute_semantic::{fact_overlap, NodeKey, NodeKind, SemanticGraph};
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

/// Why [`ImpactTarget::parse`] refused a CLI target.
#[derive(Clone, Debug, PartialEq)]
pub enum ImpactTargetError {
    /// `fact:rel(…` without the closing `)`.
    UnclosedFact,
    /// `fact:rel(args)` that is not a fact pattern.
    MalformedFact(DatalogError),
    /// No `kind:` prefix.
    NotKindKey,
    /// A kind with no impact query, or an empty key.
    UnknownOrEmpty,
    /// A `state:` path with an empty segment.
    MalformedState,
}

impl std::fmt::Display for ImpactTargetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ImpactTargetError::UnclosedFact => "fact target must end with `)`",
            ImpactTargetError::MalformedFact(_) => "malformed fact target",
            ImpactTargetError::NotKindKey => "target must be kind:key",
            ImpactTargetError::UnknownOrEmpty => "unknown or empty impact target",
            ImpactTargetError::MalformedState => "malformed state target",
        })
    }
}

impl std::error::Error for ImpactTargetError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ImpactTargetError::MalformedFact(e) => Some(e),
            _ => None,
        }
    }
}

impl ImpactTarget {
    /// Parse a CLI target into its structured representation.
    ///
    /// # Errors
    ///
    /// [`ImpactTargetError`]: `raw` is neither `kind:key` for a queryable
    /// kind nor `fact:relation(args)`.
    pub fn parse(raw: &str) -> Result<Self, ImpactTargetError> {
        if let Some((rel, rest)) = raw.strip_prefix("fact:").and_then(|s| s.split_once('(')) {
            let args = rest
                .strip_suffix(')')
                .ok_or(ImpactTargetError::UnclosedFact)?;
            let parsed =
                parse_fact(&format!("{rel}({args})")).map_err(ImpactTargetError::MalformedFact)?;
            return Ok(Self {
                kind: "fact".into(),
                relation: Some(parsed.relation),
                args: Some(
                    parsed
                        .args
                        .into_iter()
                        .map(|a| match a.term {
                            lute_manifest::fact::FactTerm::Ident(s) => s,
                            lute_manifest::fact::FactTerm::Bool(v) => v.to_string(),
                            lute_manifest::fact::FactTerm::Wildcard => "_".into(),
                            lute_manifest::fact::FactTerm::Param(s) => format!("@{s}"),
                            lute_manifest::fact::FactTerm::Target => "occasion.target".into(),
                        })
                        .collect(),
                ),
                key: None,
            });
        }
        let (kind, key) = raw.split_once(':').ok_or(ImpactTargetError::NotKindKey)?;
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
            return Err(ImpactTargetError::UnknownOrEmpty);
        }
        if kind == "state" && key.split('.').any(str::is_empty) {
            return Err(ImpactTargetError::MalformedState);
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
    query_graph(
        model.graph(),
        model.root().to_string_lossy().to_string(),
        target,
    )
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
            if edge.source.kind == NodeKind::Fact
                && fact_overlap(&edge.source.key, &node.key).is_none()
            {
                continue;
            }
            if edge.target == edge.source {
                continue;
            }
            let next_ev = weaker(&ev, &edge.evidence);
            let mut ep = edges.clone();
            ep.push(idx);
            let replace = best.get(&edge.target).is_none_or(|(old, n, oldp, _)| {
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
            (
                Some(node.key.clone()),
                declaration.and_then(|n| n.speaker.clone()),
            )
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
        groups
            .get_mut(group)
            .expect("`groups` is seeded with every group name above")
            .push(ImpactItem {
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
                        out.push_str(&format!(":{line}"));
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
    use lute_semantic::{GraphEdge, GraphNode, IdentityMetadata};

    /// 0.49.1: the typed refusal renders the 0.49.0 `lute impact` text; a
    /// malformed fact keeps the fact parser's error as its `source()`.
    #[test]
    fn parse_error_text_is_the_0_49_0_message() {
        let text = |raw: &str| ImpactTarget::parse(raw).unwrap_err().to_string();
        assert_eq!(text("fact:knows(kai"), "fact target must end with `)`");
        assert_eq!(text("nokind"), "target must be kind:key");
        assert_eq!(text("shot:x"), "unknown or empty impact target");
        assert_eq!(text("state: "), "unknown or empty impact target");
        assert_eq!(text("state:a..b"), "malformed state target");
        let malformed = ImpactTarget::parse("fact:knows(f(x))").unwrap_err();
        assert_eq!(malformed.to_string(), "malformed fact target");
        let ImpactTargetError::MalformedFact(cause) = &malformed else {
            panic!("expected a fact parse error, got {malformed:?}");
        };
        assert_eq!(
            std::error::Error::source(&malformed).map(ToString::to_string),
            Some(cause.to_string())
        );
    }

    #[test]
    fn reverse_closure_terminates_on_cycles_and_excludes_self_reads() {
        let a = NodeKey::new(NodeKind::State, "a");
        let b = NodeKey::new(NodeKind::State, "b");
        let mut graph = SemanticGraph::default();
        graph.nodes.insert(
            a.clone(),
            GraphNode {
                id: a.clone(),
                file: None,
                span: None,
                speaker: None,
                identity: IdentityMetadata::computed("state", "a"),
            },
        );
        graph.nodes.insert(
            b.clone(),
            GraphNode {
                id: b.clone(),
                file: None,
                span: None,
                speaker: None,
                identity: IdentityMetadata::computed("state", "b"),
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

    /// A project whose `porter` scene (a kind beat on `visit`) makes the
    /// declared raises `calls` of `cutscene`, which only declared raises
    /// raise and `chapter2` answers for `scene.chapter2`.
    fn raise_project(tag: &str, calls: &str) -> (lute_scratch::ScratchDir, ProjectModel) {
        let dir = lute_scratch::ScratchDir::named(tag);
        let write = |rel: &str, text: &str| {
            let path = dir.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(
            "lute.project.yaml",
            "pluginsDir: plugins/\ndefaultProfile: game\nprofiles:\n  game:\n    plugins:\n      \
             acme.stage: true\ndefaults:\n  contentLang: en\n  uses: [world.schema.yaml]\n",
        );
        write(
            "plugins/acme.stage/plugin.yaml",
            "id: acme.stage\nversion: 0.1.0\nkind: capability\ndepends:\n  - { id: lute.core, \
             range: \"^0.0.1\" }\nexports:\n  directives: directives/\n  occasions: occasions/\n",
        );
        write(
            "plugins/acme.stage/occasions/o.yaml",
            "occasions:\n  cutscene: { target: { prefix: scene, entity: cutsceneId }, raisedBy: \
             declared }\n  visit: { target: { prefix: scene, entity: cutsceneId } }\n",
        );
        write(
            "plugins/acme.stage/directives/d.yaml",
            "directives:\n  - name: playScene\n    attrs:\n      - { name: scene, type: { \
             entity: cutsceneId }, default: chapter2 }\n    effects: { raises: [{ occasion: \
             cutscene, target: \"@scene\" }] }\n",
        );
        write(
            "world.schema.yaml",
            "entities:\n  cutsceneId: { members: [porter, chapter2] }\n",
        );
        write(
            "scenes/porter.lute",
            &format!(
                "---\nkind: scene\nid: porter\non: visit\ntarget: \"kind:cutsceneId\"\n---\n\n\
                 ## Shot 1.\n\n@narrator: Porter.\n\n{calls}\n"
            ),
        );
        write(
            "scenes/chapter2.lute",
            "---\nkind: scene\nid: chapter2\non: cutscene\ntarget: scene.chapter2\n---\n\n\
             ## Shot 1.\n\n@narrator: Two.\n",
        );
        let model = ProjectModel::build_single_root(&dir, &crate::ModelOptions::default()).unwrap();
        (dir, model)
    }

    /// `scene:chapter2` in `scene:porter`'s impact: its evidence and the
    /// `raises` reasons of its explanation.
    fn raised_chapter2(model: &ProjectModel) -> (Evidence, Vec<String>) {
        let report = query(model, &ImpactTarget::parse("scene:porter").unwrap());
        let item = report
            .items
            .values()
            .flatten()
            .find(|item| item.key == "scene:chapter2")
            .unwrap_or_else(|| panic!("no scene:chapter2 in {}", human(&report)));
        let reasons = item
            .reasons
            .iter()
            .map(|r| format!("{}: {}", r.edge, r.reason))
            .collect();
        (item.evidence.clone(), reasons)
    }

    /// 0.52.0 §8.2 (Review052 F4): a `[raises]` edge every backing site of
    /// which has a static target is `Proven`.
    #[test]
    fn a_static_raise_edge_is_proven() {
        let (_dir, model) = raise_project("impact-raise-static", "::playScene");
        assert_eq!(
            raised_chapter2(&model),
            (
                Evidence::Proven,
                vec!["raises: scene(porter) raises scene(chapter2)".to_string()]
            )
        );
    }

    /// A `[raises]` edge whose only backing site raises `occasion.target`
    /// (standing for every answerer) is `Heuristic`.
    #[test]
    fn a_dynamic_only_raise_edge_is_heuristic() {
        let (_dir, model) =
            raise_project("impact-raise-dynamic", "::playScene{scene=occasion.target}");
        assert_eq!(
            raised_chapter2(&model),
            (
                Evidence::Heuristic,
                vec![
                    "raises: scene(porter) may raise scene(chapter2) (target `occasion.target`, \
                     every answerer)"
                        .to_string()
                ]
            )
        );
    }

    /// A `[raises]` edge backed by a static and a dynamic site is `Proven`:
    /// one static site suffices.
    #[test]
    fn a_mixed_raise_edge_is_proven() {
        let (_dir, model) = raise_project(
            "impact-raise-mixed",
            "::playScene{scene=occasion.target}\n::playScene",
        );
        assert_eq!(
            raised_chapter2(&model),
            (
                Evidence::Proven,
                vec!["raises: scene(porter) raises scene(chapter2)".to_string()]
            )
        );
    }
}
