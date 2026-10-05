use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use lute_core_span::Diagnostic;
use serde::{Serialize, Serializer};
use serde_json::{Map, Value};

/// A deterministic, owned semantic tree used while comparing model snapshots.
///
/// JSON is a transport format at the public boundary; keeping the comparison
/// tree separate prevents semantic code from depending on serde's dynamic API.
#[derive(Clone, Debug, PartialEq)]
enum SemanticValue {
    Null,
    Bool(bool),
    Number(serde_json::Number),
    String(String),
    Array(Vec<Self>),
    Object(BTreeMap<String, Self>),
}

impl From<Value> for SemanticValue {
    fn from(value: Value) -> Self {
        match value {
            Value::Null => Self::Null,
            Value::Bool(value) => Self::Bool(value),
            Value::Number(value) => Self::Number(value),
            Value::String(value) => Self::String(value),
            Value::Array(values) => Self::Array(values.into_iter().map(Self::from).collect()),
            Value::Object(values) => Self::Object(
                values
                    .into_iter()
                    .map(|(key, value)| (key, value.into()))
                    .collect(),
            ),
        }
    }
}

impl SemanticValue {
    fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    fn as_array(&self) -> Option<&[Self]> {
        match self {
            Self::Array(values) => Some(values),
            _ => None,
        }
    }

    fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Object(values) => values.get(key),
            _ => None,
        }
    }

    fn to_value(&self) -> Value {
        match self {
            Self::Null => Value::Null,
            Self::Bool(value) => Value::Bool(*value),
            Self::Number(value) => Value::Number(value.clone()),
            Self::String(value) => Value::String(value.clone()),
            Self::Array(values) => Value::Array(values.iter().map(Self::to_value).collect()),
            Self::Object(values) => Value::Object(
                values
                    .iter()
                    .map(|(key, value)| (key.clone(), value.to_value()))
                    .collect(),
            ),
        }
    }
}
use crate::revision::{ProjectRevision, RevisionError};
use crate::ProjectModel;
use lute_semantic::{NodeKey, NodeKind, SemanticGraph, SourceLocation};


#[derive(Clone, Debug, PartialEq)]
pub enum ChangeKind {
    Added,
    Removed,
    Moved,
    Renamed {
        from: NodeKey,
        to: NodeKey,
        declaration: Option<SourceLocation>,
    },
    Field(String),
    ConditionUnparsable,
}

impl ChangeKind {
    fn rank(&self) -> (u8, &str) {
        match self {
            Self::Added => (0, "added"),
            Self::Removed => (1, "removed"),
            Self::Renamed { .. } => (2, "renamed"),
            Self::Moved => (3, "moved"),
            Self::Field(name) => (4, name),
            Self::ConditionUnparsable => (5, "conditionUnparsable"),
        }
    }
    fn text(&self) -> String {
        match self {
            Self::Field(name) => name.clone(),
            _ => self.rank().1.to_string(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticChange {
    pub kind: ChangeKind,
    pub node: NodeKey,
    pub before: Option<Value>,
    pub after: Option<Value>,
    pub locations: Vec<SourceLocation>,
    /// True only for a removed/added save-shaped identity pair with no ledger
    /// mapping. This is deliberately a review signal, not a checker error.
    pub unmapped_identity: bool,
}

impl Serialize for SemanticChange {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Change<'a> {
            kind: String,
            node: String,
            before: &'a Option<Value>,
            after: &'a Option<Value>,
            locations: &'a [SourceLocation],
            #[serde(skip_serializing_if = "Option::is_none")]
            from: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            to: Option<String>,
            #[serde(skip_serializing_if = "Option::is_none")]
            declaration_location: Option<&'a SourceLocation>,
            #[serde(skip_serializing_if = "std::ops::Not::not")]
            unmapped_identity: bool,
        }
        let (from, to, declaration_location) = match &self.kind {
            ChangeKind::Renamed {
                from,
                to,
                declaration,
            } => (
                Some(from.canonical()),
                Some(to.canonical()),
                declaration.as_ref(),
            ),
            _ => (None, None, None),
        };
        Change {
            kind: self.kind.text(),
            node: self.node.canonical(),
            before: &self.before,
            after: &self.after,
            locations: &self.locations,
            from,
            to,
            declaration_location,
            unmapped_identity: self.unmapped_identity,
        }
        .serialize(serializer)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SemanticDiff {
    pub schema_version: &'static str,
    pub before: ProjectRevision,
    pub after: ProjectRevision,
    pub changes: Vec<SemanticChange>,
}

#[derive(Debug)]
pub enum DiffError {
    Revision(RevisionError),
    IncompleteModel(Vec<Diagnostic>),
}
impl std::fmt::Display for DiffError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Revision(e) => write!(f, "revision: {e}"),
            Self::IncompleteModel(ds) => write!(f, "incomplete model ({} diagnostics)", ds.len()),
        }
    }
}
impl std::error::Error for DiffError {}

fn strip_positional(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.remove("addr");
            map.remove("span");
            // These are addressing/provenance fields, not authored meaning.
            map.remove("body");
            // A CEL pair carries both the authored spelling and its lowered
            // expression. The expression is the semantic representation.
            map.remove("cel");
            map.remove("authored");
            for child in map.values_mut() {
                strip_positional(child);
            }
        }
        Value::Array(items) => {
            for child in items {
                strip_positional(child);
            }
        }
        _ => {}
    }
}

#[derive(Clone, Debug)]
enum CommandPath {
    Key(String),
    Index(usize),
}

#[derive(Clone, Debug)]
struct CommandHit {
    command: usize,
    order: usize,
    path: Vec<CommandPath>,
}

#[derive(Clone, Debug, Default)]
struct DocumentCommandIndex {
    commands: Vec<SemanticValue>,
    generic: BTreeMap<String, Vec<CommandHit>>,
    objectives: BTreeMap<String, Vec<CommandHit>>,
}

#[derive(Clone, Debug, Default)]
struct CommandIndex {
    documents: BTreeMap<PathBuf, DocumentCommandIndex>,
    choices: BTreeMap<PathBuf, BTreeMap<(usize, usize), Value>>,
    parse_errors: BTreeMap<PathBuf, Vec<lute_core_span::Span>>,
}

fn index_command(
    value: &SemanticValue,
    command: usize,
    path: &mut Vec<CommandPath>,
    index: &mut DocumentCommandIndex,
    order: &mut usize,
) {
    if let SemanticValue::Object(map) = value {
        let hit = CommandHit {
            command,
            order: *order,
            path: path.clone(),
        };
        *order += 1;
        if let Some(id) = map.get("id").and_then(SemanticValue::as_str) {
            index
                .generic
                .entry(id.to_string())
                .or_default()
                .push(hit.clone());
            if map.contains_key("done") {
                index
                    .objectives
                    .entry(id.to_string())
                    .or_default()
                    .push(hit.clone());
            }
        }
        for field in ["lineId", "key"] {
            if let Some(id) = map.get(field).and_then(SemanticValue::as_str) {
                index
                    .generic
                    .entry(id.to_string())
                    .or_default()
                    .push(hit.clone());
            }
        }
        for (key, child) in map {
            path.push(CommandPath::Key(key.clone()));
            index_command(child, command, path, index, order);
            path.pop();
        }
    } else if let SemanticValue::Array(items) = value {
        for (item, child) in items.iter().enumerate() {
            path.push(CommandPath::Index(item));
            index_command(child, command, path, index, order);
            path.pop();
        }
    }
}

fn command_index(model: &ProjectModel) -> CommandIndex {
    let mut index = CommandIndex::default();
    for document in model.documents() {
        index.parse_errors.insert(
            document.path.clone(),
            document
                .check
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "E-CEL-PARSE")
                .map(|diagnostic| diagnostic.span)
                .collect(),
        );
        let Some(artifact) = document.artifact.as_ref() else {
            continue;
        };
        if let Some(source_map) = document.source_map.as_ref() {
            for command in &artifact.commands {
                let lute_compile::Command::Choice(choice) = command else {
                    continue;
                };
                let Some(info) = source_map.by_addr.get(&choice.addr) else {
                    continue;
                };
                let Ok(value) = serde_json::to_value(choice) else {
                    continue;
                };
                let Some(options) = value.get("options").and_then(Value::as_array) else {
                    continue;
                };
                for (number, arm) in info.arms.iter().enumerate() {
                    let Some(mut option) = options.get(number).cloned() else {
                        continue;
                    };
                    strip_positional(&mut option);
                    if let Value::Object(map) = &mut option {
                        map.remove("target");
                    }
                    index
                        .choices
                        .entry(document.path.clone())
                        .or_default()
                        .insert((arm.span.byte_start, arm.span.byte_end), option);
                }
            }
        }
        let mut command_index = DocumentCommandIndex::default();
        let mut order = 0;
        for command in &artifact.commands {
            let Ok(value) = serde_json::to_value(command) else {
                continue;
            };
            let mut value = value.into();
            strip_positional_value(&mut value);
            let command_number = command_index.commands.len();
            index_command(
                &value,
                command_number,
                &mut Vec::new(),
                &mut command_index,
                &mut order,
            );
            command_index.commands.push(value);
        }
        index.documents.insert(document.path.clone(), command_index);
    }
    index
}

fn strip_positional_value(value: &mut SemanticValue) {
    if let SemanticValue::Object(map) = value {
        for key in ["addr", "span", "body", "cel", "authored"] {
            map.remove(key);
        }
        for child in map.values_mut() {
            strip_positional_value(child);
        }
    } else if let SemanticValue::Array(items) = value {
        for child in items {
            strip_positional_value(child);
        }
    }
}

fn hit_value(document: &DocumentCommandIndex, hit: &CommandHit) -> Option<SemanticValue> {
    let mut value = document.commands.get(hit.command)?.clone();
    for segment in &hit.path {
        value = match segment {
            CommandPath::Key(key) => value.get(key)?.clone(),
            CommandPath::Index(index) => value.as_array()?.get(*index)?.clone(),
        };
    }
    Some(value)
}

fn first_hit<'a>(hits: impl Iterator<Item = &'a CommandHit>) -> Option<&'a CommandHit> {
    hits.min_by_key(|hit| hit.order)
}

fn command_object(index: &CommandIndex, graph: &SemanticGraph, key: &NodeKey) -> Option<Value> {
    if key.kind == NodeKind::Document {
        return None;
    }
    let file = graph.nodes.get(key)?.file.as_ref()?;
    let document = index.documents.get(file)?;
    let wanted = key.key.rsplit(':').next().unwrap_or(&key.key);
    let suffix = wanted.rsplit('.').next().unwrap_or(wanted);
    let hit = if key.kind == NodeKind::Objective {
        first_hit(document.objectives.get(suffix)?.iter())
    } else {
        first_hit(
            document
                .generic
                .get(wanted)
                .into_iter()
                .flatten()
                .chain(document.generic.get(suffix).into_iter().flatten()),
        )
    }?;
    let mut found = hit_value(document, hit)?;
    if key.kind == NodeKind::Quest {
        if let SemanticValue::Object(map) = &mut found {
            map.remove("objectives");
            map.remove("rewards");
        }
    }
    Some(found.to_value())
}

fn reward_value(
    model: &ProjectModel,
    index: &CommandIndex,
    graph: &SemanticGraph,
    key: &NodeKey,
) -> Option<Value> {
    let node = graph.nodes.get(key)?;
    let file = node.file.as_ref()?;
    let document = model.documents().iter().find(|d| &d.path == file)?;
    let map = document.source_map.as_ref()?;
    let source = map
        .rewards
        .get(&key.key)
        .or_else(|| map.rewards.get(&format!("quest:{}", key.key)))?;
    let command_document = index.documents.get(file)?;
    let owner = source.owner.strip_prefix("quest:").unwrap_or(&source.owner);
    let owner = owner.rsplit('.').next().unwrap_or(owner);
    let hit = first_hit(command_document.generic.get(owner)?.iter())?;
    let object = hit_value(command_document, hit)?;
    object
        .get("rewards")
        .and_then(SemanticValue::as_array)?
        .get(source.declaration_index)
        .map(SemanticValue::to_value)
}

fn command_value(
    model: &ProjectModel,
    index: &CommandIndex,
    graph: &SemanticGraph,
    key: &NodeKey,
) -> Option<Value> {
    if key.kind == NodeKind::Reward {
        reward_value(model, index, graph, key)
    } else {
        command_object(index, graph, key)
    }
}

fn canonical_cel_text(raw: &str) -> String {
    let mut compact = String::with_capacity(raw.len());
    let mut quote = None;
    let mut escaped = false;
    for ch in raw.trim().chars() {
        if let Some(delimiter) = quote {
            compact.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == delimiter {
                quote = None;
            }
        } else if matches!(ch, '\'' | '"') {
            quote = Some(ch);
            compact.push(ch);
        } else if !ch.is_whitespace() {
            compact.push(ch);
        }
    }
    loop {
        if !compact.starts_with('(') || !compact.ends_with(')') {
            break;
        }
        let mut depth = 0usize;
        let mut closes_at_end = true;
        for (index, ch) in compact.chars().enumerate() {
            if ch == '(' {
                depth += 1;
            }
            if ch == ')' {
                depth = depth.saturating_sub(1);
                if depth == 0 && index + 1 != compact.len() {
                    closes_at_end = false;
                    break;
                }
            }
        }
        if closes_at_end {
            compact = compact[1..compact.len() - 1].to_string();
        } else {
            break;
        }
    }
    compact
}

fn edge_value(
    model: &ProjectModel,
    index: &CommandIndex,
    edge_index: &BTreeMap<NodeKey, Vec<usize>>,
    graph: &SemanticGraph,
    key: &NodeKey,
) -> Value {
    let mut map = Map::new();
    map.insert("node".into(), Value::String(canonical_key(key)));
    // Containment is structural and positional. Dependency edges are semantic
    // and are retained so writes/asserts/reads and guards remain observable.
    let mut edges = Vec::new();
    for edge_number in edge_index.get(key).into_iter().flatten() {
        let edge = &graph.edges[*edge_number];
        if edge.kind != "contains" {
            let reason = (!matches!(
                key.kind,
                NodeKind::State
                    | NodeKind::Fact
                    | NodeKind::Relation
                    | NodeKind::Engine
                    | NodeKind::Occasion
            ))
            .then(|| canonical_cel_text(&edge.reason));
            edges.push(serde_json::json!({
                "source": canonical_key(&edge.source), "target": canonical_key(&edge.target),
                "kind": edge.kind, "reason": reason
            }));
        }
    }
    edges.sort_by_key(|v| v.to_string());
    if !edges.is_empty() {
        map.insert("edges".into(), Value::Array(edges));
    }
    if key.kind == NodeKind::Project {
        if let Some(index) = model.index() {
            if let Ok(index) = serde_json::to_value(index) {
                map.insert("index".into(), index);
            }
        }
    } else if key.kind != NodeKind::Choice {
        if let Some(command) = command_value(model, index, graph, key) {
            map.insert("command".into(), command);
        }
    }
    if let Some(node) = graph.nodes.get(key) {
        if let Some(speaker) = &node.speaker {
            map.insert("speaker".into(), Value::String(speaker.clone()));
        }
        if key.kind == NodeKind::Choice {
            if let Some(choice) = choice_value(index, node.file.as_ref(), node.span) {
                map.insert("choice".into(), choice);
            }
        }
    }
    if let Some(node) = graph.nodes.get(key) {
        if let (Some(file), Some(span)) = (&node.file, node.span) {
            if index.parse_errors.get(file).is_some_and(|diagnostics| {
                diagnostics.iter().any(|diagnostic| {
                    diagnostic.byte_start < span.byte_end && span.byte_start < diagnostic.byte_end
                })
            }) {
                map.insert("conditionUnparsable".into(), Value::Bool(true));
            }
        }
    }
    Value::Object(map)
}

fn canonical_key(key: &NodeKey) -> String {
    if key.kind == NodeKind::Project {
        "project:root".to_string()
    } else {
        key.canonical()
    }
}

fn choice_value(
    index: &CommandIndex,
    file: Option<&PathBuf>,
    span: Option<lute_core_span::Span>,
) -> Option<Value> {
    let (file, span) = (file?, span?);
    index
        .choices
        .get(file)?
        .get(&(span.byte_start, span.byte_end))
        .cloned()
}

fn model_values(
    model: &ProjectModel,
    index: &CommandIndex,
    graph: &SemanticGraph,
) -> BTreeMap<NodeKey, SemanticValue> {
    let mut edge_index = BTreeMap::<NodeKey, Vec<usize>>::new();
    for (number, edge) in graph.edges.iter().enumerate() {
        edge_index
            .entry(edge.source.clone())
            .or_default()
            .push(number);
        edge_index
            .entry(edge.target.clone())
            .or_default()
            .push(number);
    }
    let mut values = BTreeMap::new();
    for key in graph.nodes.keys() {
        if key.kind == NodeKind::Reward
            && !key.key.starts_with("quest:")
            && graph.nodes.contains_key(&NodeKey::new(
                NodeKind::Reward,
                format!("quest:{}", key.key),
            ))
        {
            continue;
        }
        let normalized = if key.kind == NodeKind::Project {
            NodeKey::new(NodeKind::Project, "root")
        } else {
            key.clone()
        };
        values.insert(
            normalized,
            edge_value(model, index, &edge_index, graph, key).into(),
        );
    }
    values
}
fn locations(model: &ProjectModel, graph: &SemanticGraph, key: &NodeKey) -> Vec<SourceLocation> {
    let Some(node) = graph.nodes.get(key) else {
        return Vec::new();
    };
    let Some(file) = node.file.as_ref() else {
        return Vec::new();
    };
    let file = relative_node_path(model.root(), file);
    node.span
        .map(|span| vec![SourceLocation { file, span }])
        .unwrap_or_default()
}
fn relative_node_path(root: &std::path::Path, path: &std::path::Path) -> PathBuf {
    let canonical_root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let canonical_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    canonical_path
        .strip_prefix(&canonical_root)
        .map(PathBuf::from)
        .unwrap_or(canonical_path)
}
fn semantic_node_value(value: &SemanticValue) -> String {
    let mut value = value.to_value();
    if let Value::Object(map) = &mut value {
        map.remove("node");
    }
    value.to_string()
}

fn wire_node_value(value: &Value) -> String {
    let mut value = value.clone();
    if let Value::Object(map) = &mut value {
        map.remove("node");
    }
    value.to_string()
}
fn reward_ambiguities(
    before: &BTreeMap<NodeKey, SemanticValue>,
    after: &BTreeMap<NodeKey, SemanticValue>,
) -> BTreeSet<NodeKey> {
    let mut before_counts = BTreeMap::<(String, String), usize>::new();
    let mut after_counts = BTreeMap::<(String, String), Vec<NodeKey>>::new();
    for (key, value) in before
        .iter()
        .filter(|(key, _)| key.kind == NodeKind::Reward)
    {
        let owner = key
            .key
            .rsplit_once('#')
            .map_or(key.key.as_str(), |(owner, _)| owner);
        *before_counts
            .entry((owner.to_string(), semantic_node_value(value)))
            .or_default() += 1;
    }
    for (key, value) in after.iter().filter(|(key, _)| key.kind == NodeKind::Reward) {
        let owner = key
            .key
            .rsplit_once('#')
            .map_or(key.key.as_str(), |(owner, _)| owner);
        after_counts
            .entry((owner.to_string(), semantic_node_value(value)))
            .or_default()
            .push(key.clone());
    }
    after_counts
        .into_iter()
        .filter(|(identity, keys)| {
            keys.len() > 1 && before_counts.get(identity).copied().unwrap_or_default() != keys.len()
        })
        .flat_map(|(_, mut keys)| {
            keys.sort();
            keys.into_iter().take(1)
        })
        .collect()
}

fn parse_error_locations(model: &ProjectModel, relative_file: &PathBuf) -> Vec<SourceLocation> {
    model
        .documents()
        .iter()
        .filter(|document| {
            document
                .path
                .strip_prefix(model.root())
                .unwrap_or(&document.path)
                == relative_file
        })
        .flat_map(|document| {
            document
                .check
                .diagnostics
                .iter()
                .filter(|diagnostic| diagnostic.code == "E-CEL-PARSE")
                .map(|diagnostic| SourceLocation {
                    file: relative_file.clone(),
                    span: diagnostic.span,
                })
        })
        .collect()
}

fn has_unparsable(value: &SemanticValue) -> bool {
    let text = value.to_value().to_string();
    text.contains("E-CEL-") || text.contains("conditionUnparsable")
}

fn parse_canonical_key(raw: &str) -> Option<NodeKey> {
    let (kind, key) = raw.split_once(':')?;
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
        _ => return None,
    };
    (!key.is_empty()).then(|| NodeKey::new(kind, key))
}

fn ledger_declaration(model: &ProjectModel, from: &str) -> Option<SourceLocation> {
    let config = model.manifest()?;
    let entry = config
        .identity_renames
        .iter()
        .find(|entry| entry.rename.from == from)?;
    let span = entry.span?;
    Some(SourceLocation {
        file: PathBuf::from("lute.project.yaml"),
        span,
    })
}

fn save_shaped(kind: NodeKind) -> bool {
    !matches!(
        kind,
        NodeKind::Project | NodeKind::Engine | NodeKind::Component
    )
}
fn field_name(node: &NodeKey, before: &SemanticValue, after: &SemanticValue) -> ChangeKind {
    if has_unparsable(before) || has_unparsable(after) {
        return ChangeKind::ConditionUnparsable;
    }
    let changed = |name: &str| before.get(name) != after.get(name);
    let command_changed = |name: &str| {
        before.get("command").and_then(|value| value.get(name))
            != after.get("command").and_then(|value| value.get(name))
    };
    let edge_changed = |kinds: &[&str]| {
        let filtered = |value: &SemanticValue| {
            value
                .get("edges")
                .and_then(SemanticValue::as_array)
                .into_iter()
                .flatten()
                .filter(|edge| {
                    kinds.contains(
                        &edge
                            .get("kind")
                            .and_then(SemanticValue::as_str)
                            .unwrap_or(""),
                    )
                })
                .cloned()
                .collect::<Vec<_>>()
        };
        filtered(before) != filtered(after)
    };
    let guard_changed = command_changed("when") || command_changed("condition") || changed("guard");
    let effect_changed = edge_changed(&["writes", "asserts", "retracts"]);
    let name = match node.kind {
        NodeKind::Project => {
            if before.get("index").and_then(|v| v.get("requiredSemantics"))
                != after.get("index").and_then(|v| v.get("requiredSemantics"))
            {
                "requiredSemantics"
            } else {
                "vocabulary"
            }
        }
        NodeKind::Line => {
            if command_changed("text") {
                "lineText"
            } else if changed("speaker") {
                "speaker"
            } else if guard_changed && effect_changed {
                "guard"
            } else if guard_changed {
                "guard"
            } else if effect_changed {
                "effects"
            } else {
                "guard"
            }
        }
        NodeKind::Choice => {
            if changed("choice")
                && before.get("choice").and_then(|v| v.get("when"))
                    != after.get("choice").and_then(|v| v.get("when"))
            {
                "guard"
            } else {
                "choiceEffects"
            }
        }
        NodeKind::Reward => "reward",
        NodeKind::Quest | NodeKind::Objective | NodeKind::Beat | NodeKind::Entry => "scheduling",
        NodeKind::Engine => "enginePath",
        NodeKind::State | NodeKind::Relation | NodeKind::Fact => "vocabulary",
        _ => "semantic",
    };
    ChangeKind::Field(name.to_string())
}

/// Compare two fully-built model snapshots by canonical NodeKey. Compiler
/// addresses and source spans never participate in semantic equality.
pub fn diff_models(before: &ProjectModel, after: &ProjectModel) -> Result<SemanticDiff, DiffError> {
    let before_graph = before.graph();
    let after_graph = after.graph();
    let before_index = command_index(before);
    let after_index = command_index(after);
    let before_values = model_values(before, &before_index, &before_graph);
    let after_values = model_values(after, &after_index, &after_graph);
    let mut keys = BTreeSet::new();
    keys.extend(before_values.keys().cloned());
    keys.extend(after_values.keys().cloned());
    let ambiguous_rewards = reward_ambiguities(&before_values, &after_values);
    let mut changes = Vec::new();
    let mut handled = BTreeSet::new();

    // The validated ledger is the only source of rename matching. Apply it
    for rename in after.identity_renames() {
        let Some(from) = parse_canonical_key(&rename.from) else {
            continue;
        };
        let Some(to) = parse_canonical_key(&rename.to) else {
            continue;
        };
        let (Some(old), Some(new)) = (before_values.get(&from), after_values.get(&to)) else {
            continue;
        };
        let mut locs = locations(before, &before_graph, &from);
        locs.extend(locations(after, &after_graph, &to));
        changes.push(SemanticChange {
            kind: ChangeKind::Renamed {
                from: from.clone(),
                to: to.clone(),
                declaration: ledger_declaration(after, &rename.from),
            },
            node: to.clone(),
            before: Some(old.to_value()),
            after: Some(new.to_value()),
            locations: locs,
            unmapped_identity: false,
        });
        handled.insert(from);
        handled.insert(to);
    }

    for key in keys {
        if handled.contains(&key) {
            continue;
        }
        if key.kind == NodeKind::Reward
            && ambiguous_rewards.iter().any(|ambiguous| {
                let owner = |reward: &NodeKey| {
                    reward
                        .key
                        .rsplit_once('#')
                        .map_or_else(|| reward.key.clone(), |(owner, _)| owner.to_string())
                };
                owner(&key) == owner(ambiguous)
                    && after_values.get(&key).map(semantic_node_value)
                        == after_values.get(ambiguous).map(semantic_node_value)
            })
        {
            continue;
        }
        let b = before_values.get(&key);
        let a = after_values.get(&key);
        let mut kind = match (b, a) {
            (None, Some(_)) => Some(ChangeKind::Added),
            (Some(_), None) => Some(ChangeKind::Removed),
            (Some(b), Some(a)) if b != a => Some(field_name(&key, b, a)),
            _ => None,
        };
        let mut after_value = a.map(SemanticValue::to_value);
        let mut parse_locs = Vec::new();
        if matches!(kind, Some(ChangeKind::Removed)) {
            if let Some(file) = before_graph
                .nodes
                .get(&key)
                .and_then(|node| node.file.as_ref())
            {
                let relative = file
                    .strip_prefix(before.root())
                    .unwrap_or(file)
                    .to_path_buf();
                parse_locs = parse_error_locations(after, &relative);
                if !parse_locs.is_empty() {
                    kind = Some(ChangeKind::ConditionUnparsable);
                    after_value = Some(serde_json::json!({ "conditionUnparsable": true }));
                }
            }
        }
        if let Some(kind) = kind {
            let before_loc = locations(before, &before_graph, &key);
            let mut after_loc = locations(after, &after_graph, &key);
            after_loc.extend(parse_locs);
            let mut locs = before_loc;
            locs.extend(after_loc);
            changes.push(SemanticChange {
                kind,
                node: key,
                before: b.map(SemanticValue::to_value),
                after: after_value,
                locations: locs,
                unmapped_identity: false,
            });
        } else if b.is_some() && a.is_some() {
            let bl = before_graph
                .nodes
                .get(&key)
                .and_then(|node| node.file.as_ref())
                .map(|path| relative_node_path(before.root(), path));
            let al = after_graph
                .nodes
                .get(&key)
                .and_then(|node| node.file.as_ref())
                .map(|path| relative_node_path(after.root(), path));
            if bl != al {
                let mut locs = locations(before, &before_graph, &key);
                locs.extend(locations(after, &after_graph, &key));
                changes.push(SemanticChange {
                    kind: ChangeKind::Moved,
                    node: key,
                    before: b.map(SemanticValue::to_value),
                    after: a.map(SemanticValue::to_value),
                    locations: locs,
                    unmapped_identity: false,
                });
            }
        }
    }
    for key in &ambiguous_rewards {
        let Some(after_value) = after_values.get(key) else {
            continue;
        };
        let before_value = before_values.get(key).map(SemanticValue::to_value);
        let mut locs = locations(before, &before_graph, key);
        locs.extend(locations(after, &after_graph, key));
        changes.push(SemanticChange {
            kind: ChangeKind::Field("rewardAmbiguous".into()),
            node: key.clone(),
            before: before_value,
            after: Some(after_value.to_value()),
            locations: locs,
            unmapped_identity: false,
        });
    }

    // Pair semantically equivalent key changes first, then use balanced
    // same-kind pairing when references make the values differ.
    let removed: Vec<usize> = changes
        .iter()
        .enumerate()
        .filter(|(_, c)| matches!(c.kind, ChangeKind::Removed) && save_shaped(c.node.kind))
        .map(|(i, _)| i)
        .collect();
    let added: Vec<usize> = changes
        .iter()
        .enumerate()
        .filter(|(_, c)| matches!(c.kind, ChangeKind::Added) && save_shaped(c.node.kind))
        .map(|(i, _)| i)
        .collect();
    for ri in &removed {
        let Some(old) = changes[*ri].before.as_ref().map(wire_node_value) else {
            continue;
        };
        let Some(ai) = added.iter().copied().find(|ai| {
            changes[*ai].node.kind == changes[*ri].node.kind
                && changes[*ai].after.as_ref().map(wire_node_value) == Some(old.clone())
        }) else {
            continue;
        };
        changes[*ri].unmapped_identity = true;
        changes[ai].unmapped_identity = true;
    }
    let mut removed_by_kind = BTreeMap::<NodeKind, Vec<usize>>::new();
    let mut added_by_kind = BTreeMap::<NodeKind, Vec<usize>>::new();
    for index in removed {
        removed_by_kind
            .entry(changes[index].node.kind)
            .or_default()
            .push(index);
    }
    for index in added {
        added_by_kind
            .entry(changes[index].node.kind)
            .or_default()
            .push(index);
    }
    for (kind, mut old) in removed_by_kind {
        let Some(mut new) = added_by_kind.remove(&kind) else {
            continue;
        };
        if old.len() != new.len() {
            continue;
        }
        old.sort_by_key(|index| changes[*index].node.clone());
        new.sort_by_key(|index| changes[*index].node.clone());
        for (ri, ai) in old.into_iter().zip(new) {
            changes[ri].unmapped_identity = true;
            changes[ai].unmapped_identity = true;
        }
    }
    changes.sort_by(|a, b| {
        let location_key = |change: &SemanticChange| {
            change
                .locations
                .first()
                .map(|location| {
                    (
                        location.file.to_string_lossy().into_owned(),
                        location.span.byte_start,
                    )
                })
                .unwrap_or_default()
        };
        a.node
            .cmp(&b.node)
            .then_with(|| a.kind.rank().cmp(&b.kind.rank()))
            .then_with(|| location_key(a).cmp(&location_key(b)))
    });
    Ok(SemanticDiff {
        schema_version: "0.36.0.diff",
        before: before.revisions().clone(),
        after: after.revisions().clone(),
        changes,
    })
}

pub fn human(diff: &SemanticDiff) -> String {
    let mut out = format!(
        "before {}\nafter {}\n",
        diff.before.sha256, diff.after.sha256
    );
    for change in &diff.changes {
        match &change.kind {
            ChangeKind::Renamed { from, to, .. } => out.push_str(&format!(
                "renamed {} -> {}",
                from.canonical(),
                to.canonical()
            )),
            _ => out.push_str(&format!(
                "{} {}",
                change.kind.text(),
                change.node.canonical()
            )),
        }
        if change.unmapped_identity {
            out.push_str(" [unmappedIdentity]");
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod semantic_field_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn classifier_pins_remaining_semantic_field_kinds() {
        let before: SemanticValue = json!({}).into();
        let required_after: SemanticValue = json!({"index": {"requiredSemantics": ["a"]}}).into();
        let engine_after: SemanticValue = json!({"x": 1}).into();
        let state_after: SemanticValue = json!({"x": 1}).into();
        let effect_before: SemanticValue = json!({"edges": [{"kind": "asserts"}]}).into();
        let effect_after: SemanticValue = json!({"edges": [{"kind": "retracts"}]}).into();
        assert_eq!(
            field_name(
                &NodeKey::new(NodeKind::Project, "root"),
                &before,
                &required_after,
            ),
            ChangeKind::Field("requiredSemantics".into())
        );
        assert_eq!(
            field_name(
                &NodeKey::new(NodeKind::Engine, "clock.day"),
                &before,
                &engine_after,
            ),
            ChangeKind::Field("enginePath".into())
        );
        assert_eq!(
            field_name(
                &NodeKey::new(NodeKind::State, "run.flag"),
                &before,
                &state_after,
            ),
            ChangeKind::Field("vocabulary".into())
        );
        assert_eq!(
            field_name(
                &NodeKey::new(NodeKind::Line, "l"),
                &effect_before,
                &effect_after,
            ),
            ChangeKind::Field("effects".into())
        );
    }
}
