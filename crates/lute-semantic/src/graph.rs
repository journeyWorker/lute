//! Deterministic project semantic dependency graph.

use lute_core_span::{Evidence, Span};
use std::collections::{BTreeMap, BTreeSet};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::path::PathBuf;

use crate::IdentityMetadata;

/// The categories of nodes in the semantic dependency graph.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum NodeKind {
    Project,
    Document,
    Scene,
    Beat,
    Shot,
    Line,
    Choice,
    Quest,
    Objective,
    Reward,
    Entry,
    Occasion,
    Relation,
    State,
    Def,
    Component,
    Expanded,
    Fact,
    Clock,
    Engine,
}
impl NodeKind {
    /// Return the stable serialized kind name.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Document => "document",
            Self::Scene => "scene",
            Self::Beat => "beat",
            Self::Shot => "shot",
            Self::Line => "line",
            Self::Choice => "choice",
            Self::Quest => "quest",
            Self::Objective => "objective",
            Self::Reward => "reward",
            Self::Entry => "entry",
            Self::Occasion => "occasion",
            Self::Relation => "relation",
            Self::State => "state",
            Self::Def => "def",
            Self::Component => "component",
            Self::Expanded => "expanded",
            Self::Fact => "fact",
            Self::Clock => "clock",
            Self::Engine => "engine",
        }
    }
}
/// A stable kind/key identifier for a graph node.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct NodeKey {
    pub kind: NodeKind,
    pub key: String,
}
impl NodeKey {
    /// Construct a node key from its kind and key text.
    pub fn new(kind: NodeKind, key: impl Into<String>) -> Self {
        Self {
            kind,
            key: key.into(),
        }
    }
    /// Return the canonical `kind:key` spelling.
    pub fn canonical(&self) -> String {
        format!("{}:{}", self.kind.as_str(), self.key)
    }
}

impl Serialize for NodeKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.canonical().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for NodeKey {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        let (kind, key) = text
            .split_once(':')
            .ok_or_else(|| serde::de::Error::custom("node key must be `kind:key`"))?;
        let kind = match kind {
            "project" => NodeKind::Project,
            "document" => NodeKind::Document,
            "scene" => NodeKind::Scene,
            "beat" => NodeKind::Beat,
            "shot" => NodeKind::Shot,
            "line" => NodeKind::Line,
            "choice" => NodeKind::Choice,
            "quest" => NodeKind::Quest,
            "objective" => NodeKind::Objective,
            "reward" => NodeKind::Reward,
            "entry" => NodeKind::Entry,
            "occasion" => NodeKind::Occasion,
            "relation" => NodeKind::Relation,
            "state" => NodeKind::State,
            "def" => NodeKind::Def,
            "component" => NodeKind::Component,
            "expanded" => NodeKind::Expanded,
            "fact" => NodeKind::Fact,
            "clock" => NodeKind::Clock,
            "engine" => NodeKind::Engine,
            _ => return Err(serde::de::Error::custom(format!("unknown node kind `{kind}`"))),
        };
        if key.is_empty() {
            return Err(serde::de::Error::custom("node key is empty"));
        }
        Ok(Self::new(kind, key))
    }
}
/// A graph node with its declaration location, when known.
#[derive(Clone, Debug, PartialEq)]
pub struct GraphNode {
    pub id: NodeKey,
    pub file: Option<PathBuf>,
    pub span: Option<Span>,
    /// Source speaker for line nodes; absent for all other node kinds.
    pub speaker: Option<String>,
    /// Canonical identity metadata. `addr` and byte spans are never stored.
    pub identity: IdentityMetadata,
}
/// A directed dependency edge with source provenance.
#[derive(Clone, Debug, PartialEq)]
pub struct GraphEdge {
    pub source: NodeKey,
    pub target: NodeKey,
    pub kind: String,
    pub reason: String,
    pub file: Option<PathBuf>,
    pub span: Option<Span>,
    pub evidence: Evidence,
}
/// The complete semantic graph and its reverse adjacency index.
#[derive(Clone, Debug, Default)]
pub struct SemanticGraph {
    pub nodes: BTreeMap<NodeKey, GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub outgoing: BTreeMap<NodeKey, Vec<usize>>,
    /// Authored nodes whose canonical key occurs at multiple source spans.
    /// Consumers must refuse these keys rather than silently selecting one.
    pub ambiguous: BTreeSet<NodeKey>,
}

impl SemanticGraph {
    pub fn node(&mut self, id: NodeKey, file: Option<PathBuf>, span: Option<Span>) {
        if let Some(existing) = self.nodes.get(&id) {
            if existing.file.is_some()
                && file.is_some()
                && (existing.file != file || existing.span != span)
            {
                self.ambiguous.insert(id);
            }
            return;
        }
        self.nodes.insert(id.clone(), GraphNode {
            identity: IdentityMetadata::computed(id.kind.as_str(), id.key.clone()),
            id,
            file,
            span,
            speaker: None,
        });
    }

    /// Add a dependency edge and register both endpoint nodes.
    pub fn edge(
        &mut self,
        source: NodeKey,
        target: NodeKey,
        kind: impl Into<String>,
        reason: impl Into<String>,
        file: Option<PathBuf>,
        span: Option<Span>,
        evidence: Evidence,
    ) {
        if !self.nodes.contains_key(&source) {
            self.nodes.insert(
                source.clone(),
                GraphNode {
                    identity: IdentityMetadata::computed(source.kind.as_str(), source.key.clone()),
                    id: source.clone(),
                    file: file.clone(),
                    span,
                    speaker: None,
                },
            );
        }
        if !self.nodes.contains_key(&target) {
            self.nodes.insert(
                target.clone(),
                GraphNode {
                    identity: IdentityMetadata::computed(target.kind.as_str(), target.key.clone()),
                    id: target.clone(),
                    file: file.clone(),
                    span,
                    speaker: None,
                },
            );
        }
        self.edges.push(GraphEdge {
            source,
            target,
            kind: kind.into(),
            reason: reason.into(),
            file,
            span,
            evidence,
        });
    }

    pub fn rebuild_adjacency(&mut self) {
        self.outgoing.clear();
        for (index, edge) in self.edges.iter().enumerate() {
            self.outgoing
                .entry(edge.source.clone())
                .or_default()
                .push(index);
        }
    }
}
