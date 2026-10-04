//! Deterministic project semantic dependency graph.
use crate::{IdentityMetadata, ModelDocument, ProjectModel};
use lute_core_span::{Evidence, Span};
use lute_syntax::ast::{Arm, Node};
use lute_syntax::datalog::FactTerm;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

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
    /// Build the deterministic graph for one project model.
    pub fn build(model: &ProjectModel) -> Self {
        let mut g = Self::default();
        let project = NodeKey::new(NodeKind::Project, model.root().display().to_string());
        g.node(project.clone(), None, None);
        for doc in model.documents() {
            add_document(&mut g, model, doc, &project);
        }
        add_project_dependencies(&mut g, model);
        crate::derivation::add_derivations(&mut g, model);
        g.rebuild_adjacency();
        g
    }
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
    fn rebuild_adjacency(&mut self) {
        self.outgoing.clear();
        for (i, e) in self.edges.iter().enumerate() {
            self.outgoing.entry(e.source.clone()).or_default().push(i);
        }
    }
}

fn add_document(g: &mut SemanticGraph, model: &ProjectModel, d: &ModelDocument, project: &NodeKey) {
    let doc_id = d.folded.typed.id.clone().unwrap_or_else(|| {
        d.path
            .strip_prefix(model.root())
            .unwrap_or(&d.path)
            .to_string_lossy()
            .replace(std::path::MAIN_SEPARATOR, "/")
    });
    let document = NodeKey::new(NodeKind::Document, doc_id.clone());
    g.node(document.clone(), Some(d.path.clone()), Some(d.doc.span));
    g.edge(
        project.clone(),
        document.clone(),
        "contains",
        "project contains document",
        Some(d.path.clone()),
        Some(d.doc.span),
        Evidence::Proven,
    );
    let mut owners: Vec<(Span, NodeKey)> = Vec::new();
    let scene_owner = if d.folded.doc_kind == lute_check::DocKind::Scene {
        let scene = NodeKey::new(NodeKind::Scene, doc_id.clone());
        g.node(scene.clone(), Some(d.path.clone()), Some(d.doc.span));
        g.edge(
            document.clone(),
            scene.clone(),
            "contains",
            "document contains scene",
            Some(d.path.clone()),
            Some(d.doc.span),
            Evidence::Proven,
        );
        if let Some(beat) = &d.folded.typed.beat {
            if let Some(slot) = &beat.when {
                for query in lute_check::deps::slot_dependencies(&slot.raw, slot.span).queries {
                    let fk = fact_node(
                        &query.pattern.relation,
                        &query
                            .pattern
                            .args
                            .iter()
                            .map(|a| a.term.clone())
                            .collect::<Vec<_>>(),
                    );
                    g.edge(
                        fk,
                        scene.clone(),
                        "gates",
                        query.reason,
                        Some(d.path.clone()),
                        Some(query.span),
                        Evidence::Proven,
                    );
                }
            }
        }
        owners.push((d.doc.span, scene.clone()));
        Some(scene)
    } else {
        None
    };
    // Authored choices/options are source nodes in their own right.  Their
    // identity is document + owning branch/hub id + option id; it is stable
    // across unrelated source movement and is the spelling used by context
    // targets (for example `choice:scene.key:branch.option`).
    for shot in &d.doc.shots {
        let parent = NodeKey::new(NodeKind::Shot, format!("{doc_id}:{}", shot.heading));
        add_choice_nodes(g, d, &doc_id, &parent, &shot.body, &mut owners);
    }
    for q in &d.doc.quests {
        let parent = NodeKey::new(NodeKind::Quest, q.id.clone());
        add_choice_nodes(g, d, &doc_id, &parent, &q.body, &mut owners);
    }

    let mut line_nodes: Vec<(Span, NodeKey)> = Vec::new();
    for shot in &d.doc.shots {
        let key = NodeKey::new(NodeKind::Shot, format!("{doc_id}:{}", shot.heading));
        g.node(key.clone(), Some(d.path.clone()), Some(shot.span));
        g.edge(
            document.clone(),
            key.clone(),
            "contains",
            "document contains shot",
            Some(d.path.clone()),
            Some(shot.span),
            Evidence::Proven,
        );
        owners.push((shot.span, key));
    }
    for q in &d.doc.quests {
        let qk = NodeKey::new(NodeKind::Quest, q.id.clone());
        g.node(qk.clone(), Some(d.path.clone()), Some(q.span));
        g.edge(
            document.clone(),
            qk.clone(),
            "contains",
            "document contains quest",
            Some(d.path.clone()),
            Some(q.span),
            Evidence::Proven,
        );
        owners.push((q.span, qk.clone()));
        for n in &q.body {
            if let Node::Objective(o) = n {
                let ok = NodeKey::new(NodeKind::Objective, format!("{}.{}", q.id, o.id));
                g.node(ok.clone(), Some(d.path.clone()), Some(o.span));
                g.edge(
                    qk.clone(),
                    ok.clone(),
                    "contains",
                    "quest contains objective",
                    Some(d.path.clone()),
                    Some(o.span),
                    Evidence::Proven,
                );
                g.edge(
                    ok.clone(),
                    qk.clone(),
                    "completes",
                    "objective contributes to quest completion",
                    Some(d.path.clone()),
                    Some(o.span),
                    Evidence::Proven,
                );
                owners.push((o.span, ok.clone()));
                for (i, r) in o.rewards.iter().enumerate() {
                    let rk = NodeKey::new(NodeKind::Reward, format!("{}.{}#{}", q.id, o.id, i));
                    g.node(rk.clone(), Some(d.path.clone()), Some(r.span));
                    g.edge(
                        ok.clone(),
                        rk.clone(),
                        "contains",
                        "objective contains reward",
                        Some(d.path.clone()),
                        Some(r.span),
                        Evidence::Proven,
                    );
                    owners.push((r.span, rk));
                }
            }
        }
        g.edge(
            qk.clone(),
            NodeKey::new(NodeKind::State, format!("quest.{}.state", q.id)),
            "writes",
            "quest lifecycle state",
            Some(d.path.clone()),
            Some(q.span),
            Evidence::Proven,
        );
        for (i, r) in q.rewards.iter().enumerate() {
            let rk = NodeKey::new(NodeKind::Reward, format!("{}#{}", q.id, i));
            g.node(rk.clone(), Some(d.path.clone()), Some(r.span));
            g.edge(
                qk.clone(),
                rk.clone(),
                "contains",
                "quest contains reward",
                Some(d.path.clone()),
                Some(r.span),
                Evidence::Proven,
            );
            owners.push((r.span, rk));
        }
    }
    for e in &d.doc.entries {
        let ek = NodeKey::new(NodeKind::Entry, e.id.clone());
        g.node(ek.clone(), Some(d.path.clone()), Some(e.span));
        g.edge(
            document.clone(),
            ek.clone(),
            "contains",
            "document contains entry",
            Some(d.path.clone()),
            Some(e.span),
            Evidence::Proven,
        );
        owners.push((e.span, ek));
    }
    for b in &d.doc.beats {
        let bk = NodeKey::new(NodeKind::Beat, format!("{}.{}", doc_id, b.id));
        g.node(bk.clone(), Some(d.path.clone()), Some(b.span));
        g.edge(
            document.clone(),
            bk.clone(),
            "contains",
            "document contains bundle beat",
            Some(d.path.clone()),
            Some(b.span),
            Evidence::Proven,
        );
        if let Some((occasion, span)) = &b.on {
            let ok = NodeKey::new(NodeKind::Occasion, occasion.clone());
            g.edge(
                ok,
                bk.clone(),
                "gates",
                format!("occasion {occasion}"),
                Some(d.path.clone()),
                Some(*span),
                Evidence::Proven,
            );
        }
        if let Some((after, span)) = &b.after {
            if let Some(prefix) = after.strip_prefix("visited('") {
                if let Some(id) = prefix.strip_suffix("')") {
                    g.edge(
                        NodeKey::new(NodeKind::Scene, id),
                        bk.clone(),
                        "gates",
                        after.clone(),
                        Some(d.path.clone()),
                        Some(*span),
                        Evidence::Proven,
                    );
                }
            }
        }
        owners.push((b.span, bk));
    }
    // Compiler back-fills omitted host line codes, so source provenance is
    // needed to distinguish a durable authored code from that fallback.
    let authored_line_spans = authored_line_spans(&d.doc);
    // Expanded line spans belong to component files. Only authored line
    // spans may compete with host-document owners; expanded lines are anchored
    // at their outermost host use site for containment and source queries.
    let expansion_sites = component_use_sites(d.source_map.as_ref());
    for command in d
        .artifact
        .as_ref()
        .map(|a| &a.commands)
        .into_iter()
        .flatten()
    {
        if let lute_compile::Command::Line(line) = command {
            if let Some(info) = d
                .source_map
                .as_ref()
                .and_then(|m| m.by_addr.get(&line.addr))
            {
                let lk = NodeKey::new(NodeKind::Line, line.line_id.clone());
                let expanded = line.stamp.source.is_some();
                let host_span = if expanded {
                    expansion_sites.get(&line.addr).copied()
                } else {
                    Some(info.span)
                };
                g.node(lk.clone(), Some(d.path.clone()), host_span);
                if let Some(node) = g.nodes.get_mut(&lk) {
                    node.speaker = Some(line.speaker.clone());
                    node.identity = if let Some(source) = &line.stamp.source {
                        if source.stable {
                            IdentityMetadata::authored("line", line.line_id.clone())
                        } else {
                            IdentityMetadata::fallback("line", line.line_id.clone())
                        }
                    } else if authored_line_spans.contains(&(
                        info.span.byte_start,
                        info.span.byte_end,
                    )) {
                        IdentityMetadata::authored("line", line.line_id.clone())
                    } else {
                        IdentityMetadata::fallback("line", line.line_id.clone())
                    };
                }
                if let Some(span) = host_span {
                    line_nodes.push((span, lk.clone()));
                }
                if !expanded {
                    owners.push((info.span, lk));
                }
            }
        }
    }
    for (span, line) in &line_nodes {
        if let Some((_, owner)) = owners
            .iter()
            .filter(|(s, key)| {
                key.kind != NodeKind::Line
                    && s.byte_start <= span.byte_start
                    && s.byte_end >= span.byte_end
            })
            .min_by_key(|(s, _)| s.byte_end.saturating_sub(s.byte_start))
        {
            g.edge(
                owner.clone(),
                line.clone(),
                "contains",
                "containing node contains line",
                Some(d.path.clone()),
                Some(*span),
                Evidence::Proven,
            );
            if owner.kind == NodeKind::Shot {
                if let Some(scene) = &scene_owner {
                    g.edge(
                        scene.clone(),
                        line.clone(),
                        "contains",
                        "scene contains line",
                        Some(d.path.clone()),
                        Some(*span),
                        Evidence::Proven,
                    );
                }
            }
        }
    }
    let feed = lute_check::deps::collect_document_dependencies(&d.doc);
    for q in &d.doc.quests {
        for n in &q.body {
            let Node::Objective(o) = n else { continue };
            for (i, reward) in o.rewards.iter().enumerate() {
                let rk = NodeKey::new(NodeKind::Reward, format!("{}.{}#{}", q.id, o.id, i));
                if let Some(when) = &reward.when {
                    add_slot_dependency_edges(g, &rk, &d.path, when);
                }
            }
        }
    }
    for read in feed.reads {
        let owner = owner_for(&owners, read.span, &document);
        let sk = NodeKey::new(NodeKind::State, read.path.clone());
        g.node(sk.clone(), None, None);
        let evidence = if read.path.starts_with("quest.") {
            Evidence::Proven
        } else if lute_check::cel_paths::is_engine_owned_path(&read.path) {
            Evidence::Unknown
        } else {
            Evidence::Proven
        };
        if let Some(entry_id) = lute_check::cel_paths::reserved_entry_id(&read.path) {
            let ek = NodeKey::new(NodeKind::Entry, entry_id.to_string());
            g.edge(
                ek,
                owner.clone(),
                "discloses",
                read.reason.clone(),
                Some(d.path.clone()),
                Some(read.span),
                Evidence::Proven,
            );
        }
        if !feed
            .writes
            .iter()
            .any(|w| w.path == read.path && owner_for(&owners, w.span, &document) == owner)
        {
            g.edge(
                sk,
                owner,
                "reads",
                read.reason,
                Some(d.path.clone()),
                Some(read.span),
                evidence,
            );
        }
    }
    for write in feed.writes {
        let owner = owner_for(&owners, write.span, &document);
        let sk = NodeKey::new(NodeKind::State, write.path.clone());
        g.node(sk.clone(), None, None);
        let evidence = if lute_check::cel_paths::is_engine_owned_path(&write.path) {
            Evidence::Unknown
        } else {
            Evidence::Proven
        };
        g.edge(
            owner,
            sk,
            "writes",
            write.reason,
            Some(d.path.clone()),
            Some(write.span),
            evidence,
        );
    }
    for q in feed.queries {
        let owner = owner_for(&owners, q.span, &document);
        let terms = q
            .pattern
            .args
            .iter()
            .map(|a| a.term.clone())
            .collect::<Vec<_>>();
        let fk = fact_node(&q.pattern.relation, &terms);
        let rel = NodeKey::new(NodeKind::Relation, q.pattern.relation.clone());
        let reason = q.reason.clone();
        let evidence = if q.negated || terms.iter().any(|a| matches!(a, FactTerm::Param(_))) {
            Evidence::Heuristic
        } else {
            Evidence::Proven
        };
        g.edge(
            rel,
            owner.clone(),
            "queries",
            reason.clone(),
            Some(d.path.clone()),
            Some(q.span),
            evidence.clone(),
        );
        g.edge(
            fk,
            owner,
            "queries",
            reason,
            Some(d.path.clone()),
            Some(q.span),
            evidence,
        );
    }
    for w in feed.fact_writes {
        let owner = owner_for(&owners, w.span, &document);
        let terms = w
            .pattern
            .args
            .iter()
            .map(|a| a.term.clone())
            .collect::<Vec<_>>();
        let fk = fact_node(&w.pattern.relation, &terms);
        let reason = w.reason.clone();
        g.edge(
            owner,
            fk,
            if w.retract { "retracts" } else { "asserts" },
            reason,
            Some(d.path.clone()),
            Some(w.span),
            Evidence::Proven,
        );
    }
    let mut directive_effects = |dir: &lute_syntax::ast::Directive| {
        let owner = owner_for(&owners, dir.span, &document);
        if let Some(facts) = d.folded.env.rel_vocab.call_facts(dir) {
            for (pattern, asserted) in facts.writes() {
                let terms = pattern
                    .args
                    .iter()
                    .map(|a| a.term.clone())
                    .collect::<Vec<_>>();
                let fk = fact_node(&pattern.relation, &terms);
                let edge = if asserted { "asserts" } else { "retracts" };
                let reason = lute_check::directive_facts::pattern_text(pattern);
                g.edge(
                    owner.clone(),
                    fk,
                    edge,
                    reason,
                    Some(d.path.clone()),
                    Some(dir.span),
                    Evidence::Proven,
                );
            }
        }
        if let Some(decl) = d.folded.env.rel_vocab.effect_directives.get(&dir.tag) {
            if let Some(effects) = &decl.effects {
                for write in &effects.writes {
                    let Some(path) = resolve_effect_path(write, dir) else {
                        continue;
                    };
                    let state = NodeKey::new(NodeKind::State, path.clone());
                    g.edge(
                        owner.clone(),
                        state,
                        "writes",
                        format!("::{} writes {path}", dir.tag),
                        Some(d.path.clone()),
                        Some(dir.span),
                        if lute_check::cel_paths::is_engine_owned_path(&path) {
                            Evidence::Unknown
                        } else {
                            Evidence::Proven
                        },
                    );
                }
            }
        }
    };
    for shot in &d.doc.shots {
        lute_check::directive_facts::for_each_call(&shot.body, &mut directive_effects);
    }
    for quest in &d.doc.quests {
        lute_check::directive_facts::for_each_call(&quest.body, &mut directive_effects);
    }
    for entry in &d.doc.entries {
        lute_check::directive_facts::for_each_call(&entry.body, &mut directive_effects);
    }
    for beat in &d.doc.beats {
        lute_check::directive_facts::for_each_call(&beat.body, &mut directive_effects);
    }
    for u in feed.defs {
        let dk = NodeKey::new(NodeKind::Def, u.name.clone());
        g.edge(
            dk,
            owner_for(&owners, u.span, &document),
            "expands",
            format!("@{}", u.name),
            Some(d.path.clone()),
            Some(u.span),
            Evidence::Proven,
        );
    }
    for u in feed.components {
        let ck = NodeKey::new(NodeKind::Component, u.name.clone());
        g.edge(
            ck,
            owner_for(&owners, u.span, &document),
            "expands",
            format!("::use{{{}}}", u.name),
            Some(d.path.clone()),
            Some(u.span),
            Evidence::Proven,
        );
    }
    for gate in feed.gates {
        let is_occasion = d.input.snapshot.occasions.contains_key(&gate.target);
        let engine_owned = !gate.target.contains("visited") && !is_occasion;
        let target = if is_occasion {
            NodeKey::new(NodeKind::Occasion, gate.target.clone())
        } else if engine_owned {
            NodeKey::new(NodeKind::Engine, gate.target.clone())
        } else {
            NodeKey::new(NodeKind::Scene, gate.target.clone())
        };
        g.edge(
            target,
            owner_for(&owners, gate.span, &document),
            if is_occasion { "raises" } else { "gates" },
            gate.reason,
            Some(d.path.clone()),
            Some(gate.span),
            if engine_owned {
                Evidence::Unknown
            } else {
                Evidence::Proven
            },
        );
    }
    if let Some(map) = &d.source_map {
        for (k, r) in &map.rewards {
            let rk = NodeKey::new(NodeKind::Reward, k.clone());
            g.node(rk.clone(), Some(d.path.clone()), Some(r.span));
        }
    }
}

fn add_choice_nodes(
    g: &mut SemanticGraph,
    d: &ModelDocument,
    doc_id: &str,
    parent: &NodeKey,
    nodes: &[Node],
    owners: &mut Vec<(Span, NodeKey)>,
) {
    for node in nodes {
        match node {
            Node::Branch(branch) => {
                for choice in &branch.choices {
                    if choice.id.is_empty() {
                        continue;
                    }
                    let key = NodeKey::new(
                        NodeKind::Choice,
                        format!("{doc_id}:{}.{}", branch.id, choice.id),
                    );
                    if g.nodes.get(&key).is_some_and(|node| node.span != Some(choice.span)) {
                        g.ambiguous.insert(key.clone());
                    }
                    g.node(key.clone(), Some(d.path.clone()), Some(choice.span));
                    g.edge(
                        parent.clone(),
                        key.clone(),
                        "contains",
                        "branch contains authored choice",
                        Some(d.path.clone()),
                        Some(choice.span),
                        Evidence::Proven,
                    );
                    owners.push((choice.span, key.clone()));
                    add_choice_nodes(g, d, doc_id, &key, &choice.body, owners);
                }
            }
            Node::Hub(hub) => {
                let hub_id = hub.attrs.iter().find(|attr| attr.key == "id").and_then(|attr| match &attr.value {
                    lute_syntax::ast::AttrValue::Str(value) => Some(value.as_str()),
                    _ => None,
                }).unwrap_or("hub");
                for choice in &hub.choices {
                    if choice.id.is_empty() {
                        continue;
                    }
                    let key = NodeKey::new(
                        NodeKind::Choice,
                        format!("{doc_id}:{hub_id}.{}", choice.id),
                    );
                    if g.nodes.get(&key).is_some_and(|node| node.span != Some(choice.span)) {
                        g.ambiguous.insert(key.clone());
                    }
                    g.node(key.clone(), Some(d.path.clone()), Some(choice.span));
                    g.edge(
                        parent.clone(),
                        key.clone(),
                        "contains",
                        "hub contains authored option",
                        Some(d.path.clone()),
                        Some(choice.span),
                        Evidence::Proven,
                    );
                    owners.push((choice.span, key.clone()));
                    add_choice_nodes(g, d, doc_id, &key, &choice.body, owners);
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    let body = match arm {
                        lute_syntax::ast::Arm::When { body, .. } => body,
                        lute_syntax::ast::Arm::Otherwise { body, .. } => body,
                    };
                    add_choice_nodes(g, d, doc_id, parent, body, owners);
                }
            }
            Node::Objective(objective) => add_choice_nodes(g, d, doc_id, parent, &objective.body, owners),
            Node::On(on) => add_choice_nodes(g, d, doc_id, parent, &on.body, owners),
            _ => {}
        }
    }
}

fn add_slot_dependency_edges(
    g: &mut SemanticGraph,
    owner: &NodeKey,
    path: &std::path::Path,
    slot: &lute_syntax::ast::CelSlot,
) {
    let feed = lute_check::deps::slot_dependencies(&slot.raw, slot.span);
    for read in feed.reads {
        let evidence = if lute_check::cel_paths::is_engine_owned_path(&read.path) {
            Evidence::Unknown
        } else {
            Evidence::Proven
        };
        g.edge(
            NodeKey::new(NodeKind::State, read.path),
            owner.clone(),
            "reads",
            read.reason,
            Some(path.to_path_buf()),
            Some(read.span),
            evidence,
        );
    }
    for query in feed.queries {
        let terms = query
            .pattern
            .args
            .iter()
            .map(|a| a.term.clone())
            .collect::<Vec<_>>();
        let evidence = if query.negated
            || terms.iter().any(|a| matches!(a, FactTerm::Param(_)))
        {
            Evidence::Heuristic
        } else {
            Evidence::Proven
        };
        g.edge(
            fact_node(&query.pattern.relation, &terms),
            owner.clone(),
            "queries",
            query.reason,
            Some(path.to_path_buf()),
            Some(query.span),
            evidence,
        );
    }
}

fn resolve_effect_path(
    write: &lute_manifest::schema::WriteDecl,
    dir: &lute_syntax::ast::Directive,
) -> Option<String> {
    let mut parts = vec![write.scope.clone()];
    for segment in &write.path {
        match segment {
            lute_manifest::types::PathSegment::Literal(value) => parts.push(value.clone()),
            lute_manifest::types::PathSegment::FromAttr { from_attr } => {
                let value = dir.attrs.iter().find_map(|attr| {
                    if attr.key != from_attr.name {
                        return None;
                    }
                    match &attr.value {
                        lute_syntax::ast::AttrValue::Str(value) => Some(value.clone()),
                        _ => None,
                    }
                })?;
                parts.push(value);
            }
        }
    }
    Some(lute_check::target_writes::join_path(&parts))
}

fn add_project_dependencies(g: &mut SemanticGraph, model: &ProjectModel) {
    let mut defs_seen = BTreeSet::new();
    let mut occasions_seen = BTreeSet::new();
    for d in model.documents() {
        for (name, body) in &d.folded.def_bodies {
            let Some(path) = d.input.imports.def_origins.get(name).cloned() else {
                continue;
            };
            let Some(origin) = d.folded.env.rel_vocab.origins.defs.get(name) else {
                continue;
            };
            let span = origin.span;
            let def = NodeKey::new(NodeKind::Def, name.clone());
            if !defs_seen.insert(name.clone()) {
                continue;
            }
            g.node(def.clone(), Some(path.clone()), Some(span));
            for read in lute_check::deps::slot_dependencies(body, span).reads {
                let state = NodeKey::new(NodeKind::State, read.path);
                g.edge(
                    state,
                    def.clone(),
                    "reads",
                    read.reason,
                    Some(path.clone()),
                    Some(span),
                    Evidence::Proven,
                );
            }
            for query in lute_check::deps::slot_dependencies(body, span).queries {
                let terms = query
                    .pattern
                    .args
                    .iter()
                    .map(|a| a.term.clone())
                    .collect::<Vec<_>>();
                g.edge(
                    fact_node(&query.pattern.relation, &terms),
                    def.clone(),
                    "queries",
                    query.reason,
                    Some(path.clone()),
                    Some(span),
                    Evidence::Proven,
                );
            }
        }
        for (name, occasion) in &d.input.snapshot.occasions {
            let Some(raw) = occasion.raised_when.as_deref() else {
                continue;
            };
            if !occasions_seen.insert(name.clone()) {
                continue;
            }
            let Some(origin) = d.input.imports.plugin_origins.gates.get(name) else {
                continue;
            };
            let path = origin.file.clone();
            let span = origin.span;
            let occasion_node = NodeKey::new(NodeKind::Occasion, name.clone());
            g.node(occasion_node.clone(), Some(path.clone()), Some(span));
            let feed = lute_check::deps::slot_dependencies(raw, span);
            for query in feed.queries {
                let terms = query
                    .pattern
                    .args
                    .iter()
                    .map(|a| a.term.clone())
                    .collect::<Vec<_>>();
                g.edge(
                    fact_node(&query.pattern.relation, &terms),
                    occasion_node.clone(),
                    "gates",
                    query.reason,
                    Some(path.clone()),
                    Some(span),
                    Evidence::Proven,
                );
            }
            for read in feed.reads {
                g.edge(
                    NodeKey::new(NodeKind::State, read.path),
                    occasion_node.clone(),
                    "gates",
                    read.reason,
                    Some(path.clone()),
                    Some(span),
                    Evidence::Proven,
                );
            }
            for use_ in feed.defs {
                g.edge(
                    NodeKey::new(NodeKind::Def, use_.name),
                    occasion_node.clone(),
                    "gates",
                    format!("@{name}"),
                    Some(path.clone()),
                    Some(span),
                    Evidence::Proven,
                );
            }
        }
    }
    let Some(config) = model.manifest() else {
        return;
    };
    for (index, chain) in config.defaults.chapters().iter().enumerate() {
        let Some(occasion) =
            (!chain.on.is_empty()).then(|| NodeKey::new(NodeKind::Occasion, chain.on.clone()))
        else {
            continue;
        };
        let Some(origin) = config.chapter_origins.get(index) else {
            continue;
        };
        g.node(
            occasion.clone(),
            Some(origin.file.clone()),
            origin.on,
        );
        for scene_id in &chain.scenes {
            let scene = NodeKey::new(NodeKind::Scene, scene_id.clone());
            let Some(span) = origin.scenes.get(scene_id).copied() else {
                continue;
            };
            g.edge(
                occasion.clone(),
                scene,
                "raises",
                format!("occasion {} raises answering scene", chain.on),
                Some(origin.file.clone()),
                Some(span),
                Evidence::Proven,
            );
        }
    }
}

/// Return source spans of host lines carrying an authored `code` attribute.
/// The compiler allocates omitted codes after lowering, so checking the
/// preserved syntax tree is the only way to retain authored-vs-fallback
/// provenance without changing the serialized IR.
fn authored_line_spans(doc: &lute_syntax::ast::Document) -> BTreeSet<(usize, usize)> {
    fn walk(nodes: &[Node], out: &mut BTreeSet<(usize, usize)>) {
        for node in nodes {
            match node {
                Node::Line(line) => {
                    if line.attrs.iter().any(|attr| attr.key == "code") {
                        out.insert((line.span.byte_start, line.span.byte_end));
                    }
                }
                Node::Branch(branch) => {
                    for choice in &branch.choices {
                        walk(&choice.body, out);
                    }
                }
                Node::Hub(hub) => {
                    for body in hub.bodies() {
                        walk(body, out);
                    }
                    if let Some(on_return) = &hub.on_return {
                        walk(&on_return.body, out);
                    }
                }
                Node::Match(m) => {
                    for arm in &m.arms {
                        match arm {
                            Arm::When { body, .. } | Arm::Otherwise { body, .. } => {
                                walk(body, out);
                            }
                        }
                    }
                }
                Node::Objective(objective) => walk(&objective.body, out),
                Node::On(on) => walk(&on.body, out),
                Node::Directive(_)
                | Node::Set(_)
                | Node::Timeline(_)
                | Node::Assert(_)
                | Node::Retract(_) => {}
            }
        }
    }
    let mut out = BTreeSet::new();
    for shot in &doc.shots {
        walk(&shot.body, &mut out);
    }
    for quest in &doc.quests {
        walk(&quest.body, &mut out);
    }
    for entry in &doc.entries {
        walk(&entry.body, &mut out);
    }
    for beat in &doc.beats {
        walk(&beat.body, &mut out);
    }
    out
}

/// Resolve component-local source coordinates to the outer host use site.
/// Begin/end/body markers are compile provenance, not inferred span overlap.
fn component_use_sites(
    map: Option<&lute_compile::source_map::SourceMap>,
) -> BTreeMap<String, Span> {
    use lute_compile::source_map::ComponentBoundary;
    let mut sites = BTreeMap::new();
    let Some(map) = map else { return sites };
    let mut stack: Vec<(Span, bool)> = Vec::new();
    let mut unit = "";
    for (addr, info) in &map.by_addr {
        let current_unit = addr.split('.').next().unwrap_or(addr);
        if current_unit != unit {
            stack.clear();
            unit = current_unit;
        }
        for marker in &info.before {
            match marker.component {
                Some(ComponentBoundary::Begin) => {
                    let host = stack.first().map_or(marker.span, |(span, _)| *span);
                    stack.push((host, false));
                }
                Some(ComponentBoundary::End) => { stack.pop(); }
                Some(ComponentBoundary::Body) => {
                    if let Some((_, body)) = stack.last_mut() { *body = true; }
                }
                Some(ComponentBoundary::BodyEnd) => {
                    if let Some((_, body)) = stack.last_mut() { *body = false; }
                }
                None => {}
            }
        }
        if let Some((span, false)) = stack.last() {
            sites.insert(addr.clone(), *span);
        }
    }
    sites
}
 

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_source_offsets_do_not_steal_host_dependency_owners() {
        fn copy_tree(from: &std::path::Path, to: &std::path::Path) {
            std::fs::create_dir_all(to).unwrap();
            for entry in std::fs::read_dir(from).unwrap() {
                let entry = entry.unwrap();
                let target = to.join(entry.file_name());
                if entry.file_type().unwrap().is_dir() {
                    copy_tree(&entry.path(), &target);
                } else {
                    std::fs::copy(entry.path(), target).unwrap();
                }
            }
        }
        let root = std::env::temp_dir().join(format!("lute-component-owners-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        copy_tree(
            &PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/examples/games/seven-days"),
            &root,
        );
        let before = ProjectModel::build_single_root(&root, &crate::ModelOptions::default()).unwrap();
        let file = root.join("lore/post.lute");
        let source = std::fs::read_to_string(&file).unwrap();
        let shifted = source.replacen("\n<beat ", &format!("\n// {}\n<beat ", "layout".repeat(64)), 1);
        std::fs::write(&file, shifted).unwrap();
        let after = ProjectModel::build_single_root(&root, &crate::ModelOptions::default()).unwrap();
        let projection = |model: &ProjectModel| {
            model.graph().edges.iter().map(|edge| {
                (edge.source.clone(), edge.target.clone(), edge.kind.clone(), edge.reason.clone())
            }).collect::<BTreeSet<_>>()
        };
        assert_eq!(projection(&before), projection(&after));
        let graph = after.graph();
        assert!(graph.edges.iter().any(|edge| {
            edge.kind == "queries"
                && edge.source == NodeKey::new(NodeKind::Relation, "present")
                && edge.target == NodeKey::new(NodeKind::Beat, "post.machine")
        }));
        assert!(graph.edges.iter().any(|edge| {
            edge.kind == "contains"
                && edge.source == NodeKey::new(NodeKind::Beat, "post.mum")
                && edge.target == NodeKey::new(NodeKind::Line, "post.mum.postcard#use-001.narrator_0010")
        }));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn fact_overlap_distinguishes_ground_and_parameterized_terms() {
        assert_eq!(
            fact_overlap("slew(regent)", "slew(regent)"),
            Some(Evidence::Proven)
        );
        assert_eq!(
            fact_overlap("slew(_)", "slew(regent)"),
            Some(Evidence::Proven)
        );
        assert_eq!(
            fact_overlap("slew(@foe)", "slew(regent)"),
            Some(Evidence::Heuristic)
        );
        assert_eq!(fact_overlap("slew(eel)", "slew(regent)"), None);
    }
    #[test]
    fn slot_dependency_edges_classify_engine_and_parameter_evidence() {
        let owner = NodeKey::new(NodeKind::Beat, "scene.b");
        let mut graph = SemanticGraph::default();
        let path = std::path::Path::new("scene.lute");
        let span = Span {
            byte_start: 0,
            byte_end: 1,
            line: 1,
            column: 1,
            utf16_range: (0, 1),
        };
        let slot = lute_syntax::ast::CelSlot::raw(
            lute_syntax::ast::CelKind::Condition,
            "quest.q.state == 'active' && holds('knows', [@who])".into(),
            span,
        );
        add_slot_dependency_edges(&mut graph, &owner, path, &slot);
        assert!(graph.edges.iter().any(|edge| {
            edge.source.key == "quest.q.state" && edge.evidence == Evidence::Unknown
        }));
        assert!(graph.edges.iter().any(|edge| {
            edge.source.key == "knows(@who)" && edge.evidence == Evidence::Heuristic
        }));
    }
    #[test]
    fn authored_choice_key_survives_insertion_before_it() {
        let parent = NodeKey::new(NodeKind::Shot, "scene:shot");
        let existing = NodeKey::new(NodeKind::Choice, "scene:branch.coffee");
        let inserted = NodeKey::new(NodeKind::Choice, "scene:branch.before");
        let mut graph = SemanticGraph::default();
        graph.edge(
            parent.clone(),
            existing.clone(),
            "contains",
            "branch contains authored choice",
            Some("scene.lute".into()),
            None,
            Evidence::Proven,
        );
        let before = graph
            .edges
            .iter()
            .find(|edge| edge.target == existing)
            .cloned()
            .unwrap();
        graph.edge(
            parent,
            inserted,
            "contains",
            "branch contains authored choice",
            Some("scene.lute".into()),
            None,
            Evidence::Proven,
        );
        assert_eq!(
            graph
                .edges
                .iter()
                .find(|edge| edge.target == existing)
                .cloned(),
            Some(before)
        );
    }
 
    #[test]
    fn duplicate_authored_key_is_marked_ambiguous() {
        let key = NodeKey::new(NodeKind::Choice, "scene:branch.coffee");
        let span = |start| Span {
            byte_start: start,
            byte_end: start + 1,
            line: 1,
            column: start as u32 + 1,
            utf16_range: (start as u32, start as u32 + 1),
        };
        let mut graph = SemanticGraph::default();
        graph.node(key.clone(), Some("scene.lute".into()), Some(span(1)));
        graph.node(key.clone(), Some("scene.lute".into()), Some(span(9)));
        assert!(graph.ambiguous.contains(&key));
    }
}
fn owner_for(owners: &[(Span, NodeKey)], span: Span, fallback: &NodeKey) -> NodeKey {
    owners
        .iter()
        .filter(|(candidate, _)| {
            candidate.byte_start <= span.byte_start && candidate.byte_end >= span.byte_end
        })
        .min_by_key(|(candidate, _)| candidate.byte_end - candidate.byte_start)
        .map(|(_, key)| key.clone())
        .unwrap_or_else(|| fallback.clone())
}

/// Build the canonical graph key for a fact pattern.
pub fn fact_node(relation: &str, args: &[FactTerm]) -> NodeKey {
    NodeKey::new(NodeKind::Fact, format_fact(relation, args))
}

/// Render a fact pattern using the graph's stable argument spelling.
pub fn format_fact(relation: &str, args: &[FactTerm]) -> String {
    let rendered = args
        .iter()
        .map(|arg| match arg {
            FactTerm::Ident(value) => value.clone(),
            FactTerm::Bool(value) => value.to_string(),
            FactTerm::Wildcard => "_".to_string(),
            FactTerm::Param(value) => format!("@{value}"),
            FactTerm::Target => "occasion.target".to_string(),
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("{relation}({rendered})")
}

/// Determine whether two fact patterns overlap and classify the evidence.
pub fn fact_overlap(a: &str, b: &str) -> Option<Evidence> {
    let pa = lute_syntax::datalog::parse_fact(a).ok()?;
    let pb = lute_syntax::datalog::parse_fact(b).ok()?;
    if pa.relation != pb.relation || pa.args.len() != pb.args.len() {
        return None;
    }
    let mut heuristic = false;
    for (left, right) in pa.args.iter().zip(pb.args.iter()) {
        match (&left.term, &right.term) {
            (FactTerm::Ident(a), FactTerm::Ident(b)) if a != b => return None,
            (FactTerm::Bool(a), FactTerm::Bool(b)) if a != b => return None,
            (FactTerm::Param(_), _) | (_, FactTerm::Param(_)) => heuristic = true,
            _ => {}
        }
    }
    Some(if heuristic {
        Evidence::Heuristic
    } else {
        Evidence::Proven
    })
}
