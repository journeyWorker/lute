use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use lute_core_span::{Diagnostic, Span};
use serde::{Serialize, Serializer};
use serde_json::{Map, Value};

use crate::{NodeKey, NodeKind, ProjectModel};
use crate::revision::{ProjectRevision, RevisionError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLocation {
    pub file: PathBuf,
    pub span: Span,
}

impl Serialize for SourceLocation {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        #[derive(Serialize)]
        struct Location<'a> { file: &'a PathBuf, span: Span }
        Location { file: &self.file, span: self.span }.serialize(serializer)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChangeKind {
    Added,
    Removed,
    Moved,
    Field(String),
    ConditionUnparsable,
}

impl ChangeKind {
    fn rank(&self) -> (u8, &str) {
        match self {
            Self::Added => (0, "added"),
            Self::Removed => (1, "removed"),
            Self::Moved => (2, "moved"),
            Self::Field(name) => (3, name),
            Self::ConditionUnparsable => (4, "conditionUnparsable"),
        }
    }
    fn text(&self) -> String {
        match self { Self::Field(name) => name.clone(), _ => self.rank().1.to_string() }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SemanticChange {
    pub kind: ChangeKind,
    pub node: NodeKey,
    pub before: Option<Value>,
    pub after: Option<Value>,
    pub locations: Vec<SourceLocation>,
}

impl Serialize for SemanticChange {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where S: Serializer {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Change<'a> {
            kind: String,
            node: String,
            before: &'a Option<Value>,
            after: &'a Option<Value>,
            locations: &'a [SourceLocation],
        }
        Change { kind: self.kind.text(), node: self.node.canonical(), before: &self.before, after: &self.after, locations: &self.locations }.serialize(serializer)
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
            for child in map.values_mut() { strip_positional(child); }
        }
        Value::Array(items) => for child in items { strip_positional(child); },
        _ => {}
    }
}

fn command_object(model: &ProjectModel, key: &NodeKey) -> Option<Value> {
    if key.kind == NodeKind::Document { return None; }
    let graph = model.graph();
    let document = model.documents().iter().find(|d| {
        let relative = d.path.strip_prefix(model.root()).unwrap_or(&d.path)
            .to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/");
        graph.nodes.get(key).and_then(|n| n.file.as_ref()).is_some_and(|f| f == &d.path)
            || (key.kind == NodeKind::Document && key.key == relative)
    })?;
    let ir = document.artifact.as_ref()?;
    let wanted = key.key.rsplit(':').next().unwrap_or(&key.key);
    let suffix = wanted.rsplit('.').next().unwrap_or(wanted);
    for command in &ir.commands {
        let mut value = serde_json::to_value(command).ok()?;
        strip_positional(&mut value);
        let found = match key.kind {
            NodeKind::Objective => find_object_with_id(&value, suffix, true),
            NodeKind::Quest | NodeKind::Entry | NodeKind::Beat => {
                find_command_object(&value, wanted, suffix)
            }
            _ => find_command_object(&value, wanted, suffix),
        };
        if let Some(mut found) = found {
            // Child declarations have their own graph nodes. A parent value
            // must not change merely because a child was inserted or edited.
            if key.kind == NodeKind::Quest {
                if let Value::Object(map) = &mut found {
                    map.remove("objectives");
                    map.remove("rewards");
                }
            }
            return Some(found);
        }
    }
    None
}

fn find_object_with_id(value: &Value, id: &str, require_objective_shape: bool) -> Option<Value> {
    if let Value::Object(map) = value {
        if map.get("id").and_then(Value::as_str) == Some(id)
            && (!require_objective_shape || map.contains_key("done"))
        {
            return Some(value.clone());
        }
        for child in map.values() {
            if let Some(found) = find_object_with_id(child, id, require_objective_shape) {
                return Some(found);
            }
        }
    } else if let Value::Array(items) = value {
        for child in items {
            if let Some(found) = find_object_with_id(child, id, require_objective_shape) {
                return Some(found);
            }
        }
    }
    None
}

fn reward_value(model: &ProjectModel, key: &NodeKey) -> Option<Value> {
    let graph = model.graph();
    let node = graph.nodes.get(key)?;
    let file = node.file.as_ref()?;
    let document = model.documents().iter().find(|d| &d.path == file)?;
    let map = document.source_map.as_ref()?;
    let source = map.rewards.get(&key.key)
        .or_else(|| map.rewards.get(&format!("quest:{}", key.key)))?;
    let ir = document.artifact.as_ref()?;
    let owner = source.owner.strip_prefix("quest:").unwrap_or(&source.owner);
    for command in &ir.commands {
        let mut value = serde_json::to_value(command).ok()?;
        strip_positional(&mut value);
        if let Some(object) = find_object_with_id(&value, owner.rsplit('.').next().unwrap_or(owner), false) {
            if let Some(rewards) = object.get("rewards").and_then(Value::as_array) {
                if let Some(reward) = rewards.get(source.declaration_index) {
                    return Some(reward.clone());
                }
            }
        }
    }
    None
}

fn command_value(model: &ProjectModel, key: &NodeKey) -> Option<Value> {
    if key.kind == NodeKind::Reward {
        reward_value(model, key)
    } else {
        command_object(model, key)
    }
}

fn canonical_cel_text(raw: &str) -> String {
    let mut compact = String::with_capacity(raw.len());
    let mut quote = None;
    let mut escaped = false;
    for ch in raw.trim().chars() {
        if let Some(delimiter) = quote {
            compact.push(ch);
            if escaped { escaped = false; }
            else if ch == '\\' { escaped = true; }
            else if ch == delimiter { quote = None; }
        } else if matches!(ch, '\'' | '"') {
            quote = Some(ch);
            compact.push(ch);
        } else if !ch.is_whitespace() {
            compact.push(ch);
        }
    }
    loop {
        if !compact.starts_with('(') || !compact.ends_with(')') { break; }
        let mut depth = 0usize;
        let mut closes_at_end = true;
        for (index, ch) in compact.chars().enumerate() {
            if ch == '(' { depth += 1; }
            if ch == ')' {
                depth = depth.saturating_sub(1);
                if depth == 0 && index + 1 != compact.len() {
                    closes_at_end = false;
                    break;
                }
            }
        }
        if closes_at_end { compact = compact[1..compact.len() - 1].to_string(); }
        else { break; }
    }
    compact
}

fn edge_value(model: &ProjectModel, key: &NodeKey) -> Value {
    let graph = model.graph();
    let mut map = Map::new();
    map.insert("node".into(), Value::String(canonical_key(key)));
    // Containment is structural and positional. Dependency edges are semantic
    // and are retained so writes/asserts/reads and guards remain observable.
    let mut edges = Vec::new();
    for edge in &graph.edges {
        if (edge.source == *key || edge.target == *key) && edge.kind != "contains" {
            let reason = (!matches!(key.kind, NodeKind::State | NodeKind::Fact | NodeKind::Relation | NodeKind::Engine | NodeKind::Occasion))
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
        if let Some(command) = command_value(model, key) {
            map.insert("command".into(), command);
        }
    }
    if let Some(node) = graph.nodes.get(key) {
        if let Some(speaker) = &node.speaker { map.insert("speaker".into(), Value::String(speaker.clone())); }
        if key.kind == NodeKind::Choice {
            if let Some(choice) = choice_value(model, node.file.as_ref(), node.span) {
                map.insert("choice".into(), choice);
            }
        }
    }
    if let Some(node) = graph.nodes.get(key) {
        if let (Some(file), Some(span)) = (&node.file, node.span) {
            if model.documents().iter().any(|document| {
                &document.path == file
                    && document.check.diagnostics.iter().any(|diagnostic| {
                        diagnostic.code == "E-CEL-PARSE"
                            && diagnostic.span.byte_start < span.byte_end
                            && span.byte_start < diagnostic.span.byte_end
                    })
            }) {
                map.insert("conditionUnparsable".into(), Value::Bool(true));
            }
        }
    }
    Value::Object(map)
}


fn find_command_object(value: &Value, wanted: &str, suffix: &str) -> Option<Value> {
    if let Value::Object(map) = value {
        let matches = map.get("lineId").and_then(Value::as_str).is_some_and(|v| v == wanted)
            || map.get("id").and_then(Value::as_str).is_some_and(|v| v == wanted || v == suffix)
            || map.get("key").and_then(Value::as_str).is_some_and(|v| v == wanted);
        if matches { return Some(value.clone()); }
        for child in map.values() {
            if let Some(found) = find_command_object(child, wanted, suffix) { return Some(found); }
        }
    } else if let Value::Array(items) = value {
        for child in items {
            if let Some(found) = find_command_object(child, wanted, suffix) { return Some(found); }
        }
    }
    None
}

fn canonical_key(key: &NodeKey) -> String {
    if key.kind == NodeKind::Project { "project:root".to_string() } else { key.canonical() }
}


fn choice_value(model: &ProjectModel, file: Option<&PathBuf>, span: Option<lute_core_span::Span>) -> Option<Value> {
    let (file, span) = (file?, span?);
    let document = model.documents().iter().find(|document| &document.path == file)?;
    let artifact = document.artifact.as_ref()?;
    let source_map = document.source_map.as_ref()?;
    for command in &artifact.commands {
        let lute_compile::Command::Choice(choice) = command else { continue };
        let info = source_map.by_addr.get(&choice.addr)?;
        let index = info.arms.iter().position(|arm| arm.span == span)?;
        let value = serde_json::to_value(choice).ok()?;
        let mut option = value.get("options").and_then(Value::as_array).and_then(|options| options.get(index)).cloned()?;
        strip_positional(&mut option);
        if let Value::Object(map) = &mut option { map.remove("target"); }
        return Some(option);
    }
    None
}

fn model_values(model: &ProjectModel) -> BTreeMap<NodeKey, Value> {
    let graph = model.graph();
    let mut values = BTreeMap::new();
    for key in graph.nodes.keys() {
        if key.kind == NodeKind::Reward && !key.key.starts_with("quest:")
            && graph.nodes.contains_key(&NodeKey::new(NodeKind::Reward, format!("quest:{}", key.key)))
        {
            continue;
        }
        let normalized = if key.kind == NodeKind::Project { NodeKey::new(NodeKind::Project, "root") } else { key.clone() };
        values.insert(normalized, edge_value(model, key));
    }
    values
}
fn locations(model: &ProjectModel, key: &NodeKey) -> Vec<SourceLocation> {

    let graph = model.graph();
    let Some(node) = graph.nodes.get(key) else { return Vec::new() };
    let Some(file) = node.file.as_ref() else { return Vec::new() };
    let file = file.strip_prefix(model.root()).unwrap_or(file).to_path_buf();
    node.span.map(|span| vec![SourceLocation { file, span }]).unwrap_or_default()
}
fn semantic_node_value(value: &Value) -> String {
    let mut value = value.clone();
    if let Value::Object(map) = &mut value { map.remove("node"); }
    value.to_string()
}
fn reward_ambiguities(
    before: &BTreeMap<NodeKey, Value>,
    after: &BTreeMap<NodeKey, Value>,
) -> BTreeSet<NodeKey> {
    let mut before_counts = BTreeMap::<(String, String), usize>::new();
    let mut after_counts = BTreeMap::<(String, String), Vec<NodeKey>>::new();
    for (key, value) in before.iter().filter(|(key, _)| key.kind == NodeKind::Reward) {
        let owner = key.key.rsplit_once('#').map_or(key.key.as_str(), |(owner, _)| owner);
        *before_counts.entry((owner.to_string(), semantic_node_value(value))).or_default() += 1;
    }
    for (key, value) in after.iter().filter(|(key, _)| key.kind == NodeKind::Reward) {
        let owner = key.key.rsplit_once('#').map_or(key.key.as_str(), |(owner, _)| owner);
        after_counts.entry((owner.to_string(), semantic_node_value(value))).or_default().push(key.clone());
    }
    after_counts.into_iter()
        .filter(|(identity, keys)| keys.len() > 1 && before_counts.get(identity).copied().unwrap_or_default() != keys.len())
        .flat_map(|(_, mut keys)| {
            keys.sort();
            keys.into_iter().take(1)
        })
        .collect()
}

fn parse_error_locations(model: &ProjectModel, relative_file: &PathBuf) -> Vec<SourceLocation> {
    model.documents().iter()
        .filter(|document| {
            document.path.strip_prefix(model.root()).unwrap_or(&document.path)
                == relative_file
        })
        .flat_map(|document| document.check.diagnostics.iter()
            .filter(|diagnostic| diagnostic.code == "E-CEL-PARSE")
            .map(|diagnostic| SourceLocation {
                file: relative_file.clone(),
                span: diagnostic.span,
            }))
        .collect()
}

fn has_unparsable(value: &Value) -> bool {
    value.to_string().contains("E-CEL-") || value.to_string().contains("conditionUnparsable")
}

fn field_name(node: &NodeKey, before: &Value, after: &Value) -> ChangeKind {
    if has_unparsable(before) || has_unparsable(after) { return ChangeKind::ConditionUnparsable; }
    let changed = |name: &str| before.get(name) != after.get(name);
    let command_changed = |name: &str| {
        before.get("command").and_then(|v| v.get(name))
            != after.get("command").and_then(|v| v.get(name))
    };
    let edge_has = |kind: &str| {
        [before, after].iter().any(|value| {
            value.get("edges").and_then(Value::as_array).is_some_and(|edges| {
                edges.iter().any(|edge| edge.get("kind").and_then(Value::as_str) == Some(kind))
            })
        })
    };
    let name = match node.kind {
        NodeKind::Project => {
            if before.get("index").and_then(|v| v.get("requiredSemantics"))
                != after.get("index").and_then(|v| v.get("requiredSemantics"))
            { "requiredSemantics" } else { "vocabulary" }
        }
        NodeKind::Line => {
            if command_changed("text") { "lineText" }
            else if changed("speaker") { "speaker" }
            else if edge_has("writes") || edge_has("asserts") || edge_has("retracts") { "effects" }
            else { "guard" }
        }
        NodeKind::Choice => {
            if changed("choice") && before.get("choice").and_then(|v| v.get("when"))
                != after.get("choice").and_then(|v| v.get("when"))
            { "guard" } else { "choiceEffects" }
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
    let before_values = model_values(before);
    let after_values = model_values(after);
    let before_graph = before.graph();
    let after_graph = after.graph();
    let mut keys = BTreeSet::new();
    keys.extend(before_values.keys().cloned());
    keys.extend(after_values.keys().cloned());
    let ambiguous_rewards = reward_ambiguities(&before_values, &after_values);
    let mut changes = Vec::new();
    let mut paired_lines = BTreeMap::new();
    let removed_lines: Vec<_> = before_values.keys()
        .filter(|key| key.kind == NodeKind::Line && !after_values.contains_key(*key))
        .cloned().collect();
    let added_lines: Vec<_> = after_values.keys()
        .filter(|key| key.kind == NodeKind::Line && !before_values.contains_key(*key))
        .cloned().collect();
    for old in removed_lines {
        let old_locations = locations(before, &old);
        let Some(old_loc) = old_locations.first() else { continue };
        let Some(new) = added_lines.iter().find(|candidate| {
            let candidate_locations = locations(after, candidate);
            candidate_locations.first().is_some_and(|location| {
                location.file == old_loc.file && location.span.line == old_loc.span.line
            })
        }) else { continue };
        paired_lines.insert(old, new.clone());
    }
    let paired_new: BTreeSet<_> = paired_lines.values().cloned().collect();
    for key in keys {
        if paired_lines.contains_key(&key) || paired_new.contains(&key)
            || (key.kind == NodeKind::Reward && ambiguous_rewards.iter().any(|ambiguous| {
                let owner = |reward: &NodeKey| reward.key.rsplit_once('#').map_or_else(|| reward.key.clone(), |(owner, _)| owner.to_string());
                owner(&key) == owner(ambiguous)
                    && after_values.get(&key).map(semantic_node_value) == after_values.get(ambiguous).map(semantic_node_value)
            })) { continue; }
        let b = before_values.get(&key);
        let a = after_values.get(&key);
        let mut kind = match (b, a) {
            (None, Some(_)) => Some(ChangeKind::Added),
            (Some(_), None) => Some(ChangeKind::Removed),
            (Some(b), Some(a)) if b != a => Some(field_name(&key, b, a)),
            _ => None,
        };
        let mut after_value = a.cloned();
        let mut parse_locs = Vec::new();
        if matches!(kind, Some(ChangeKind::Removed)) {
            if let Some(file) = before_graph.nodes.get(&key).and_then(|node| node.file.as_ref()) {
                let relative = file.strip_prefix(before.root()).unwrap_or(file).to_path_buf();
                parse_locs = parse_error_locations(after, &relative);
                if !parse_locs.is_empty() {
                    kind = Some(ChangeKind::ConditionUnparsable);
                    after_value = Some(serde_json::json!({ "conditionUnparsable": true }));
                }
            }
        }
        if let Some(kind) = kind {
            let before_loc = locations(before, &key);
            let mut after_loc = locations(after, &key);
            after_loc.extend(parse_locs);
            let mut locs = before_loc.clone();
            locs.extend(after_loc);
            changes.push(SemanticChange { kind, node: key, before: b.cloned(), after: after_value, locations: locs });
        } else if b.is_some() && a.is_some() {
            let bl = before_graph.nodes.get(&key).and_then(|n| n.file.as_ref()).map(|p| p.strip_prefix(before.root()).unwrap_or(p).to_path_buf());
            let al = after_graph.nodes.get(&key).and_then(|n| n.file.as_ref()).map(|p| p.strip_prefix(after.root()).unwrap_or(p).to_path_buf());
            if bl != al {
                let mut locs = locations(before, &key);
                locs.extend(locations(after, &key));
                changes.push(SemanticChange { kind: ChangeKind::Moved, node: key, before: b.cloned(), after: a.cloned(), locations: locs });
            }
        }
    }
    for key in &ambiguous_rewards {
        let Some(after_value) = after_values.get(key) else { continue };
        let before_value = before_values.get(key).cloned();
        let mut locs = locations(before, key);
        locs.extend(locations(after, key));
        changes.push(SemanticChange {
            kind: ChangeKind::Field("rewardAmbiguous".into()),
            node: key.clone(),
            before: before_value,
            after: Some(after_value.clone()),
            locations: locs,
        });
    }
    for (old, new) in paired_lines {
        let (Some(before_value), Some(after_value)) = (before_values.get(&old), after_values.get(&new)) else { continue };
        let kind = if before_value.get("command").and_then(|value| value.get("lineId"))
            != after_value.get("command").and_then(|value| value.get("lineId"))
            && before_value.get("command").and_then(|value| value.get("text"))
                == after_value.get("command").and_then(|value| value.get("text"))
            && before_value.get("speaker") == after_value.get("speaker")
        {
            ChangeKind::Field("lineId".into())
        } else {
            field_name(&old, before_value, after_value)
        };
        let mut locs = locations(before, &old);
        locs.extend(locations(after, &new));
        changes.push(SemanticChange {
            kind,
            node: old,
            before: Some(before_value.clone()),
            after: Some(after_value.clone()),
            locations: locs,
        });
    }
    changes.sort_by(|a, b| {
        let location_key = |change: &SemanticChange| change.locations.first()
            .map(|location| (location.file.to_string_lossy().into_owned(), location.span.byte_start))
            .unwrap_or_default();
        a.node.cmp(&b.node)
            .then_with(|| a.kind.rank().cmp(&b.kind.rank()))
            .then_with(|| location_key(a).cmp(&location_key(b)))
    });
    Ok(SemanticDiff { schema_version: "0.35.0.diff", before: before.revisions().clone(), after: after.revisions().clone(), changes })
}

pub fn human(diff: &SemanticDiff) -> String {
    let mut out = format!("before {}\nafter {}\n", diff.before.sha256, diff.after.sha256);
    for change in &diff.changes {
        out.push_str(&format!("{} {}\n", change.kind.text(), change.node.canonical()));
    }
    out
}

#[cfg(test)]
mod semantic_field_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn classifier_pins_remaining_semantic_field_kinds() {
        let before = json!({});
        let required_after = json!({"index": {"requiredSemantics": ["a"]}});
        assert_eq!(field_name(&NodeKey::new(NodeKind::Project, "root"), &before, &required_after), ChangeKind::Field("requiredSemantics".into()));
        assert_eq!(field_name(&NodeKey::new(NodeKind::Engine, "clock.day"), &before, &json!({"x": 1})), ChangeKind::Field("enginePath".into()));
        assert_eq!(field_name(&NodeKey::new(NodeKind::State, "run.flag"), &before, &json!({"x": 1})), ChangeKind::Field("vocabulary".into()));
        let effect_before = json!({"edges": [{"kind": "asserts"}]});
        let effect_after = json!({"edges": [{"kind": "retracts"}]});
        assert_eq!(field_name(&NodeKey::new(NodeKind::Line, "l"), &effect_before, &effect_after), ChangeKind::Field("effects".into()));
    }
}