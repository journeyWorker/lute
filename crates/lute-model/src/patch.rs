//! Strict, staged source patches.
//!
//! Patch application deliberately builds a complete second model before touching
//! the caller's tree.  The staging directory is an implementation detail and is
//! removed on every return path; the source tree is only changed after all
//! checks, diffing, and preserve assertions have passed.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

use crate::{diff_models, ChangeKind, ModelOptions, NodeKey, NodeKind, ProjectModel, ProjectRevision, SemanticChange};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchBase {
    pub project: String,
    #[serde(default)]
    pub files: BTreeMap<PathBuf, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatchEdit {
    ReplaceNode { node: NodeKey, text: String },
    InsertBefore { node: NodeKey, text: String },
    InsertAfter { node: NodeKey, text: String },
    ReplaceAttr { node: NodeKey, attr: String, value: String },
    RemoveNode { node: NodeKey },
    CreateFile { path: PathBuf, text: String },
    MoveFile { from: PathBuf, to: PathBuf },
    ReplaceText { file: PathBuf, rev: String, span: Span, text: String },
}

impl Serialize for PatchEdit {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serde_json::Map::new();
        let (op, fields) = match self {
            Self::ReplaceNode { node, text } => ("replaceNode", serde_json::json!({"node": node, "text": text})),
            Self::InsertBefore { node, text } => ("insertBefore", serde_json::json!({"node": node, "text": text})),
            Self::InsertAfter { node, text } => ("insertAfter", serde_json::json!({"node": node, "text": text})),
            Self::ReplaceAttr { node, attr, value } => ("replaceAttr", serde_json::json!({"node": node, "attr": attr, "value": value})),
            Self::RemoveNode { node } => ("removeNode", serde_json::json!({"node": node})),
            Self::CreateFile { path, text } => ("createFile", serde_json::json!({"path": path, "text": text})),
            Self::MoveFile { from, to } => ("moveFile", serde_json::json!({"from": from, "to": to})),
            Self::ReplaceText { file, rev, span, text } => ("replaceText", serde_json::json!({"file": file, "rev": rev, "span": span, "text": text})),
        };
        map.insert("op".into(), Value::String(op.into()));
        if let Value::Object(fields) = fields { map.extend(fields); }
        Value::Object(map).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PatchEdit {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(deserializer)?;
        let object = value.as_object().ok_or_else(|| de::Error::custom("edit must be an object"))?;
        let op = object.get("op").and_then(Value::as_str).ok_or_else(|| de::Error::custom("edit requires string `op`"))?;
        let known = |allowed: &[&str]| -> Result<(), D::Error> {
            if object.keys().any(|k| k != "op" && !allowed.contains(&k.as_str())) {
                return Err(de::Error::custom(format!("unknown field in {op}")));
            }
            Ok(())
        };
        let node = || parse_node(object.get("node"), "node");
        let text = || required_string(object.get("text"), "text");
        match op {
            "replaceNode" => { known(&["node", "text"])?; Ok(Self::ReplaceNode { node: node()?, text: text()? }) }
            "insertBefore" => { known(&["node", "text"])?; Ok(Self::InsertBefore { node: node()?, text: text()? }) }
            "insertAfter" => { known(&["node", "text"])?; Ok(Self::InsertAfter { node: node()?, text: text()? }) }
            "replaceAttr" => { known(&["node", "attr", "value"])?; Ok(Self::ReplaceAttr { node: node()?, attr: required_string(object.get("attr"), "attr")?, value: required_string(object.get("value"), "value")? }) }
            "removeNode" => { known(&["node"])?; Ok(Self::RemoveNode { node: node()? }) }
            "createFile" => { known(&["path", "text"])?; Ok(Self::CreateFile { path: required_path(object.get("path"), "path")?, text: text()? }) }
            "moveFile" => { known(&["from", "to"])?; Ok(Self::MoveFile { from: required_path(object.get("from"), "from")?, to: required_path(object.get("to"), "to")? }) }
            "replaceText" => {
                known(&["file", "rev", "span", "text"])?;
                let span = object.get("span").cloned().ok_or_else(|| de::Error::missing_field("span"))?;
                Ok(Self::ReplaceText { file: required_path(object.get("file"), "file")?, rev: required_string(object.get("rev"), "rev")?, span: serde_json::from_value(span).map_err(de::Error::custom)?, text: text()? })
            }
            _ => Err(de::Error::custom(format!("unknown patch operation `{op}`"))),
        }
    }
}

fn required_string<E: de::Error>(value: Option<&Value>, name: &str) -> Result<String, E> {
    value.and_then(Value::as_str).map(str::to_owned).ok_or_else(|| de::Error::custom(format!("missing field `{name}`")))
}
fn required_path<E: de::Error>(value: Option<&Value>, name: &str) -> Result<PathBuf, E> {
    let text = required_string(value, name)?;
    let path = PathBuf::from(text);
    safe_relative(&path).map_err(de::Error::custom)
}
fn parse_node<E: de::Error>(value: Option<&Value>, name: &str) -> Result<NodeKey, E> {
    let text = required_string(value, name)?;
    parse_node_key(&text).map_err(de::Error::custom)
}

impl Serialize for NodeKey {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> { self.canonical().serialize(serializer) }
}
impl<'de> Deserialize<'de> for NodeKey {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = String::deserialize(d)?;
        parse_node_key(&text).map_err(de::Error::custom)
    }
}

fn parse_node_key(text: &str) -> Result<NodeKey, String> {
    let (kind, key) = text.split_once(':').ok_or_else(|| "node key must be `kind:key`".to_string())?;
    let kind = match kind {
        "project" => NodeKind::Project, "document" => NodeKind::Document, "scene" => NodeKind::Scene,
        "beat" => NodeKind::Beat, "shot" => NodeKind::Shot, "line" => NodeKind::Line,
        "choice" => NodeKind::Choice, "quest" => NodeKind::Quest, "objective" => NodeKind::Objective,
        "reward" => NodeKind::Reward, "entry" => NodeKind::Entry, "occasion" => NodeKind::Occasion,
        "relation" => NodeKind::Relation, "state" => NodeKind::State, "def" => NodeKind::Def,
        "component" => NodeKind::Component, "expanded" => NodeKind::Expanded, "fact" => NodeKind::Fact,
        "clock" => NodeKind::Clock, "engine" => NodeKind::Engine,
        _ => return Err(format!("unknown node kind `{kind}`")),
    };
    if key.is_empty() { return Err("node key is empty".into()); }
    Ok(NodeKey::new(kind, key))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Preserve {
    Ids(Vec<NodeKey>),
    LineIds,
    VoiceKeys,
    ChoiceEffects(Vec<NodeKey>),
    Rewards(Vec<NodeKey>),
    HostContracts,
    Conditions(Vec<NodeKey>),
    Reachability(Vec<NodeKey>),
    Constraints,
}
impl<'de> Deserialize<'de> for Preserve {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = Value::deserialize(d)?;
        let object = value.as_object().ok_or_else(|| de::Error::custom("preserve item must be an object"))?;
        if object.len() != 1 { return Err(de::Error::custom("preserve item must have exactly one field")); }
        let (key, value) = object.iter().next().unwrap();
        let nodes = || serde_json::from_value::<Vec<NodeKey>>(value.clone()).map_err(de::Error::custom);
        let strings = || serde_json::from_value::<Vec<String>>(value.clone()).map_err(de::Error::custom);
        match key.as_str() {
            "ids" => Ok(Self::Ids(nodes()?)), "lineIds" => { strings()?; Ok(Self::LineIds) }, "voiceKeys" => { strings()?; Ok(Self::VoiceKeys) },
            "choiceEffects" => Ok(Self::ChoiceEffects(nodes()?)), "rewards" => Ok(Self::Rewards(nodes()?)),
            "hostContracts" => if value == &Value::Bool(true) { Ok(Self::HostContracts) } else { Err(de::Error::custom("hostContracts must be true")) },
            "conditions" => Ok(Self::Conditions(nodes()?)), "reachability" => Ok(Self::Reachability(nodes()?)),
            "constraints" => if value == &Value::Bool(true) { Ok(Self::Constraints) } else { Err(de::Error::custom("constraints must be true")) },
            _ => Err(de::Error::custom(format!("unknown preserve kind `{key}`"))),
        }
    }
}
impl Serialize for Preserve {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::Ids(v) => serde_json::json!({"ids":v}).serialize(s), Self::LineIds => serde_json::json!({"lineIds":[]}).serialize(s),
            Self::VoiceKeys => serde_json::json!({"voiceKeys":[]}).serialize(s), Self::ChoiceEffects(v) => serde_json::json!({"choiceEffects":v}).serialize(s),
            Self::Rewards(v) => serde_json::json!({"rewards":v}).serialize(s), Self::HostContracts => serde_json::json!({"hostContracts":true}).serialize(s),
            Self::Conditions(v) => serde_json::json!({"conditions":v}).serialize(s), Self::Reachability(v) => serde_json::json!({"reachability":v}).serialize(s),
            Self::Constraints => serde_json::json!({"constraints":true}).serialize(s),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchRequest {
    pub base: PatchBase,
    #[serde(default)]
    pub targets: Vec<NodeKey>,
    #[serde(default)]
    pub edits: Vec<PatchEdit>,
    #[serde(default)]
    pub preserve: Vec<Preserve>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PatchRefusal {
    Stale { expected: String, actual: String },
    Target { node: NodeKey, reason: String },
    Edit { index: usize, reason: String },
    Check(Vec<Diagnostic>),
    Preserve(Vec<(String, SemanticChange)>),
    Io { path: PathBuf, message: String },
}
impl PatchRefusal {
    pub fn code(&self) -> &'static str { match self { Self::Stale { .. } => "E-PATCH-STALE", Self::Target { .. } => "E-PATCH-TARGET", Self::Edit { .. } => "E-PATCH-EDIT", Self::Check(_) => "E-PATCH-CHECK", Self::Preserve(_) => "E-PATCH-PRESERVE", Self::Io { .. } => "" } }
    pub fn message(&self) -> String {
        match self {
            Self::Stale { expected, actual } => format!("base revision differs (expected {expected}, actual {actual})"),
            Self::Target { node, reason } => format!("{}: {reason}", node.canonical()),
            Self::Edit { index, reason } => format!("edit {index}: {reason}"),
            Self::Check(ds) => format!("staged model has {} check error(s)", ds.len()),
            Self::Preserve(changes) => format!("{} preserve violation(s)", changes.len()),
            Self::Io { path, message } => format!("{}: {message}", path.display()),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchReport {
    pub before: ProjectRevision,
    pub after: ProjectRevision,
    pub diff: crate::SemanticDiff,
    pub writes: Vec<PathBuf>,
}

fn safe_relative(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() { return Err(format!("path `{}` must be relative", path.display())); }
    let mut out = PathBuf::new();
    for c in path.components() {
        match c { Component::Normal(p) => out.push(p), Component::CurDir => {}, Component::ParentDir | Component::RootDir | Component::Prefix(_) => return Err(format!("path `{}` escapes project root", path.display())) }
    }
    if out.as_os_str().is_empty() { return Err("path must not be empty".into()); }
    Ok(PathBuf::from(out.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/")))
}
fn digest_equal(expected: &str, actual: &str) -> bool { expected.strip_prefix("sha256:").unwrap_or(expected) == actual.strip_prefix("sha256:").unwrap_or(actual) }
fn rel(root: &Path, path: &Path) -> PathBuf { PathBuf::from(path.strip_prefix(root).unwrap_or(path).to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/")) }

struct Stage { root: PathBuf }
impl Drop for Stage { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.root); } }
fn copy_tree(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst).map_err(|e| e.to_string())?;
    for entry in std::fs::read_dir(src).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?; let from = entry.path(); let to = dst.join(entry.file_name());
        let kind = std::fs::symlink_metadata(&from).map_err(|e| e.to_string())?;
        if kind.is_dir() { copy_tree(&from, &to)?; } else if kind.is_file() { std::fs::copy(&from, &to).map_err(|e| e.to_string())?; }
        else { return Err(format!("unsupported special file {}", from.display())); }
    }
    Ok(())
}
fn make_stage(root: &Path) -> Result<Stage, String> {
    static NEXT_STAGE: AtomicU64 = AtomicU64::new(0);
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| e.to_string())?.as_nanos();
    let serial = NEXT_STAGE.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("lute-patch-{}-{stamp}-{serial}", std::process::id()));
    copy_tree(root, &path)?; Ok(Stage { root: path })
}

fn errors(model: &ProjectModel) -> Vec<Diagnostic> {
    let mut out = Vec::new();
    for document in model.documents() { out.extend(document.check.diagnostics.iter().filter(|d| d.severity == Severity::Error).cloned()); }
    out.extend(model.project_diagnostics().iter().filter(|(_, d)| d.severity == Severity::Error).map(|(_, d)| d.clone())); out
}
fn span_valid(span: Span, len: usize) -> bool { span.byte_start <= span.byte_end && span.byte_end <= len }
fn line_bounds(text: &str, regions: &[Span]) -> Option<(usize, usize)> {
    if regions.is_empty() { return None; }
    let start = regions.iter().map(|s| s.byte_start).min()?; let end = regions.iter().map(|s| s.byte_end).max()?;
    let line_start = text[..start.min(text.len())].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_end = text[end.min(text.len())..].find('\n').map(|i| end.min(text.len()) + i + 1).unwrap_or(text.len()); Some((line_start, line_end))
}
fn format_touched(source: &str, regions: &[Span], yaml: bool) -> Result<String, String> {
    if yaml { return Ok(lute_syntax::format_yaml_source(source).text); }
    let full = lute_syntax::format_source(source, &lute_syntax::FormatOptions { regions: regions.to_vec() }).map_err(|e| format!("{e:?}"))?.text;
    let Some((start, end)) = line_bounds(source, regions) else { return Ok(full); };
    if start == 0 && end == source.len() { return Ok(full); }
    let start_line = source[..start].bytes().filter(|b| *b == b'\n').count();
    let count = source[start..end].bytes().filter(|b| *b == b'\n').count().max(1);
    let mut formatted_lines = full.split_inclusive('\n').collect::<Vec<_>>();
    if !full.ends_with('\n') { formatted_lines.push(&full[full.len()..]); }
    let end_line = (start_line + count).min(formatted_lines.len());
    let mut out = String::new();
    out.push_str(&source[..start]);
    if start_line < formatted_lines.len() { out.extend(formatted_lines[start_line..end_line].iter().copied()); }
    let used_end = formatted_lines[..end_line].iter().map(|x| x.len()).sum::<usize>();
    let suffix_at = used_end.min(full.len());
    // When formatting changes line count, prefer the original suffix: untouched
    // bytes are part of the patch contract.
    let _ = suffix_at;
    out.push_str(&source[end..]);
    Ok(out)
}

#[derive(Clone)]
struct TextEdit { file: PathBuf, start: usize, end: usize, text: String, index: usize, region: Span }

fn attr_range(text: &str, span: Span, attr: &str) -> Option<(usize, usize)> {
    let body = &text[span.byte_start..span.byte_end];
    let needle = format!("{attr}="); let at = body.find(&needle)? + span.byte_start + needle.len();
    let bytes = text.as_bytes(); let mut end = at;
    if bytes.get(at) == Some(&b'"') || bytes.get(at) == Some(&b'\'') { let q = bytes[at]; end += 1; while end < text.len() { if bytes[end] == q && bytes[end - 1] != b'\\' { end += 1; break; } end += 1; } }
    Some((at, end))
}

fn duplicate_authored_target(model: &ProjectModel, node: &NodeKey) -> bool {
    let count = match node.kind {
        NodeKind::Document | NodeKind::Scene => model.documents().iter().filter(|d| d.folded.typed.id.as_deref() == Some(node.key.as_str())).count(),
        NodeKind::Quest => model.documents().iter().flat_map(|d| &d.doc.quests).filter(|q| q.id == node.key).count(),
        NodeKind::Entry => model.documents().iter().flat_map(|d| &d.doc.entries).filter(|e| e.id == node.key).count(),
        _ => 0,
    };
    count > 1
}
pub fn apply_patch(root: &Path, patch: PatchRequest, dry_run: bool) -> Result<PatchReport, PatchRefusal> {
    let before_model = ProjectModel::build_single_root(root, &ModelOptions::default()).map_err(|e| PatchRefusal::Io { path: root.to_path_buf(), message: e.to_string() })?;
    apply_patch_to(&before_model, patch, dry_run)
}

/// Apply a patch against an already-built model.
///
/// The model is the authoritative snapshot used for base validation, target
/// lookup, and the before side of the semantic diff. The source tree is still
/// staged and rebuilt once for the after side.
pub fn apply_patch_to(before_model: &ProjectModel, patch: PatchRequest, dry_run: bool) -> Result<PatchReport, PatchRefusal> {
    let root = before_model.root();
    let opts = ModelOptions::default();
    let before = before_model.revisions().clone();
    if !digest_equal(&patch.base.project, &before.sha256) { return Err(PatchRefusal::Stale { expected: patch.base.project, actual: format!("sha256:{}", before.sha256) }); }
    for (path, expected) in &patch.base.files {
        let path = safe_relative(path).map_err(|e| PatchRefusal::Io { path: path.clone(), message: e })?;
        let actual = before.files.get(&path).map(|f| f.sha256.as_str()).unwrap_or("");
        if !digest_equal(expected, actual) { return Err(PatchRefusal::Stale { expected: expected.clone(), actual: format!("sha256:{actual}") }); }
    }

    let graph = before_model.graph();
    let mut targets = Vec::new();
    for node in &patch.targets {
        let Some(item) = graph.nodes.get(node) else { return Err(PatchRefusal::Target { node: node.clone(), reason: "unknown target".into() }); };
        if graph.ambiguous.contains(node) || duplicate_authored_target(&before_model, node) { return Err(PatchRefusal::Target { node: node.clone(), reason: "ambiguous target".into() }); }
        if matches!(node.kind, NodeKind::Project | NodeKind::Fact | NodeKind::State | NodeKind::Relation | NodeKind::Engine | NodeKind::Occasion | NodeKind::Expanded) || item.file.is_none() || item.span.is_none() {
            return Err(PatchRefusal::Target { node: node.clone(), reason: "target has no editable source anchor".into() });
        }
        targets.push((node.clone(), item.file.clone().unwrap(), item.span.unwrap()));
    }

    let stage = make_stage(root).map_err(|e| PatchRefusal::Io { path: root.to_path_buf(), message: e })?;
    let mut edits = Vec::new();
    let mut moves = Vec::new();
    for (index, operation) in patch.edits.iter().enumerate() {
        let node_span = |node: &NodeKey| -> Result<(PathBuf, Span), PatchRefusal> {
            let Some(item) = graph.nodes.get(node) else { return Err(PatchRefusal::Target { node: node.clone(), reason: "unknown target".into() }); };
            let (Some(file), Some(span)) = (item.file.clone(), item.span) else { return Err(PatchRefusal::Target { node: node.clone(), reason: "target has no editable source anchor".into() }); };
            if graph.ambiguous.contains(node) || duplicate_authored_target(&before_model, node) { return Err(PatchRefusal::Target { node: node.clone(), reason: "ambiguous target".into() }); }
            Ok((file, span))
        };
        let scope = |file: &Path, span: Span| -> bool { targets.iter().any(|(_, f, s)| f == file && s.byte_start <= span.byte_start && s.byte_end >= span.byte_end) };
        match operation {
            PatchEdit::ReplaceNode { node, .. } | PatchEdit::InsertBefore { node, .. } | PatchEdit::InsertAfter { node, .. } | PatchEdit::RemoveNode { node } => {
                let (file, span) = node_span(node)?; let path = rel(root, &file); let source = std::fs::read_to_string(stage.root.join(&path)).map_err(|e| PatchRefusal::Io { path: path.clone(), message: e.to_string() })?;
                if !span_valid(span, source.len()) || !scope(&file, span) { return Err(PatchRefusal::Edit { index, reason: "edit is outside declared target".into() }); }
                let (start, end, replacement) = match operation {
                    PatchEdit::ReplaceNode { text, .. } => (span.byte_start, span.byte_end, text.clone()),
                    PatchEdit::InsertBefore { text, .. } => (span.byte_start, span.byte_start, text.clone()),
                    PatchEdit::InsertAfter { text, .. } => (span.byte_end, span.byte_end, text.clone()),
                    PatchEdit::RemoveNode { .. } => (span.byte_start, span.byte_end, String::new()), _ => unreachable!(),
                };
                edits.push(TextEdit { file: path, start, end, text: replacement, index, region: span });
            }
            PatchEdit::ReplaceAttr { node, attr, value } => {
                let (file, span) = node_span(node)?; let path = rel(root, &file); let source = std::fs::read_to_string(stage.root.join(&path)).map_err(|e| PatchRefusal::Io { path: path.clone(), message: e.to_string() })?;
                if !scope(&file, span) { return Err(PatchRefusal::Edit { index, reason: "edit is outside declared target".into() }); }
                let Some((start, end)) = attr_range(&source, span, attr) else { return Err(PatchRefusal::Edit { index, reason: format!("attribute `{attr}` not found") }); };
                let replacement = match source.as_bytes().get(start) {
                    Some(b'"') => format!("\"{}\"", value.replace('"', "\\\"")),
                    Some(b'\'') => format!("'{}'", value.replace('\'', "\\'")),
                    _ => value.clone(),
                };
                edits.push(TextEdit { file: path, start, end, text: replacement, index, region: span });
            }
            PatchEdit::ReplaceText { file, rev, span, text } => {
                let path = safe_relative(file).map_err(|e| PatchRefusal::Edit { index, reason: e })?; let abs = root.join(&path);
                let actual = before.files.get(&path).map(|f| f.sha256.as_str()).unwrap_or(""); if !digest_equal(rev, actual) { return Err(PatchRefusal::Edit { index, reason: "file revision does not match".into() }); }
                let source = std::fs::read_to_string(stage.root.join(&path)).map_err(|e| PatchRefusal::Io { path: path.clone(), message: e.to_string() })?;
                if !span_valid(*span, source.len()) || !source.is_char_boundary(span.byte_start) || !source.is_char_boundary(span.byte_end) || !scope(&abs, *span) { return Err(PatchRefusal::Edit { index, reason: "replaceText span is outside declared target".into() }); }
                edits.push(TextEdit { file: path.clone(), start: span.byte_start, end: span.byte_end, text: text.clone(), index, region: *span });
            }
            PatchEdit::CreateFile { path, text } => {
                let path = safe_relative(path).map_err(|e| PatchRefusal::Edit { index, reason: e })?; if stage.root.join(&path).exists() { return Err(PatchRefusal::Edit { index, reason: "file already exists".into() }); }
                if let Some(parent) = stage.root.join(&path).parent() { std::fs::create_dir_all(parent).map_err(|e| PatchRefusal::Io { path: path.clone(), message: e.to_string() })?; }
                let formatted = if path.extension().is_some_and(|x| x == "lute") {
                    lute_syntax::format_source(text, &lute_syntax::FormatOptions::default()).map_err(|e| PatchRefusal::Edit { index, reason: format!("{e:?}") })?.text
                } else {
                    lute_syntax::format_yaml_source(text).text
                };
                std::fs::write(stage.root.join(&path), formatted).map_err(|e| PatchRefusal::Io { path: path.clone(), message: e.to_string() })?;
            }
            PatchEdit::MoveFile { from, to } => {
                let from = safe_relative(from).map_err(|e| PatchRefusal::Edit { index, reason: e })?; let to = safe_relative(to).map_err(|e| PatchRefusal::Edit { index, reason: e })?;
                if !stage.root.join(&from).is_file() || stage.root.join(&to).exists() { return Err(PatchRefusal::Edit { index, reason: "invalid move source or destination".into() }); }
                moves.push((from, to));
        }
    }
    }
    let mut by_file: BTreeMap<PathBuf, Vec<TextEdit>> = BTreeMap::new(); for edit in edits { by_file.entry(edit.file.clone()).or_default().push(edit); }
    for (file, list) in &mut by_file {
        list.sort_by_key(|e| (e.start, e.end, e.index));
        for pair in list.windows(2) { if pair[0].end > pair[1].start || (pair[0].start == pair[1].start && pair[0].end == pair[1].end) { return Err(PatchRefusal::Edit { index: pair[1].index, reason: format!("overlapping edits in {}", file.display()) }); } }
        let mut source = std::fs::read_to_string(stage.root.join(file)).map_err(|e| PatchRefusal::Io { path: file.clone(), message: e.to_string() })?;
        for edit in list.iter().rev() {
            if !span_valid(Span { byte_start: edit.start, byte_end: edit.end, ..edit.region }, source.len()) || !source.is_char_boundary(edit.start) || !source.is_char_boundary(edit.end) {
                return Err(PatchRefusal::Edit { index: edit.index, reason: "invalid byte span".into() });
            }
            source.replace_range(edit.start..edit.end, &edit.text);
        }
        // Node spans come from the pre-edit model. Map each touched region through
        // all replacements before formatting: otherwise an inserted/deleted line
        // leaves the old end offset inside the replacement and the formatter
        // splices a suffix from the wrong byte.
        let map_offset = |offset: usize| {
            let mut mapped = offset;
            for edit in list.iter() {
                if edit.end <= offset {
                    mapped = (mapped as isize + edit.text.len() as isize - (edit.end - edit.start) as isize) as usize;
                } else if edit.start <= offset && offset <= edit.end {
                    mapped = edit.start + edit.text.len();
                }
            }
            mapped
        };
        let regions: Vec<_> = list.iter().map(|edit| Span {
            byte_start: map_offset(edit.region.byte_start),
            byte_end: map_offset(edit.region.byte_end),
            ..edit.region
        }).collect();
        source = format_touched(&source, &regions, !file.extension().is_some_and(|x| x == "lute")).map_err(|e| PatchRefusal::Edit { index: list[0].index, reason: e })?;
        std::fs::write(stage.root.join(file), source).map_err(|e| PatchRefusal::Io { path: file.clone(), message: e.to_string() })?;
    }
    for (from, to) in &moves {
        if let Some(path) = stale_move_reference(&stage.root, from).map_err(|message| PatchRefusal::Io { path: from.clone(), message })? {
            return Err(PatchRefusal::Check(vec![patch_diagnostic(format!("moved file `{}` is still referenced from `{}`", from.display(), path.display()))]));
        }
        if let Some(parent) = stage.root.join(to).parent() { std::fs::create_dir_all(parent).map_err(|e| PatchRefusal::Io { path: to.clone(), message: e.to_string() })?; }
        std::fs::rename(stage.root.join(from), stage.root.join(to)).map_err(|e| PatchRefusal::Io { path: from.clone(), message: e.to_string() })?;
    }

    let after_model = ProjectModel::build_single_root(&stage.root, &opts).map_err(|e| PatchRefusal::Io { path: stage.root.clone(), message: e.to_string() })?;
    let before_errors = errors(&before_model); let after_errors = errors(&after_model);
    let new_errors: Vec<_> = after_errors.into_iter().filter(|d| !before_errors.iter().any(|old| old.code == d.code && old.message == d.message)).collect();
    if !new_errors.is_empty() { return Err(PatchRefusal::Check(new_errors)); }
    let diff = diff_models(&before_model, &after_model).map_err(|e| PatchRefusal::Io { path: root.to_path_buf(), message: e.to_string() })?;
    let violations = patch.preserve.iter().flat_map(|item| {
        let name = match item {
            Preserve::Ids(_) => "ids", Preserve::LineIds => "lineIds", Preserve::VoiceKeys => "voiceKeys",
            Preserve::ChoiceEffects(_) => "choiceEffects", Preserve::Rewards(_) => "rewards",
            Preserve::HostContracts => "hostContracts", Preserve::Conditions(_) => "conditions",
            Preserve::Reachability(_) => "reachability", Preserve::Constraints => "constraints",
        }.to_string();
        preserve_violations(std::slice::from_ref(item), &diff, &before_model, &after_model)
            .into_iter().map(move |change| (name.clone(), change))
    }).collect::<Vec<_>>();
    if !violations.is_empty() {
        return Err(PatchRefusal::Preserve(violations));
    }
    let mut writes = changed_files(root, &stage.root).map_err(|e| PatchRefusal::Io { path: root.to_path_buf(), message: e })?;
    writes.sort();
    let report = PatchReport { before, after: after_model.revisions().clone(), diff, writes: writes.clone() };
    if !dry_run { atomic_commit(root, &stage.root, &writes).map_err(|(path, message)| PatchRefusal::Io { path, message })?; }
    Ok(report)
}

fn patch_diagnostic(message: String) -> Diagnostic {
    Diagnostic {
        code: "E-PATCH-CHECK".into(),
        severity: Severity::Error,
        message,
        span: Span { byte_start: 0, byte_end: 0, line: 1, column: 1, utf16_range: (0, 0) },
        layer: Layer::Staging,
        evidence: None,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}
fn stale_move_reference(root: &Path, from: &Path) -> Result<Option<PathBuf>, String> {
    let from = safe_relative(from)?;
    let from_text = from.to_string_lossy().replace(std::path::MAIN_SEPARATOR, "/");
    let from_name = from.file_name().and_then(|x| x.to_str()).unwrap_or("");
    for path in all_files(root)? {
        let full = root.join(&path);
        if path.extension().is_some_and(|extension| extension == "md") { continue; }
        let Ok(bytes) = std::fs::read(&full) else { continue };
        let text = String::from_utf8_lossy(&bytes);
        let referenced = if path.file_name().and_then(|x| x.to_str()) == Some("lute.project.yaml") {
            text.lines().any(|line| line.contains(&from_text) || line.contains(from_name))
        } else if path.extension().is_some_and(|extension| extension == "yaml" || extension == "yml") {
            text.lines().any(|line| line.contains(&from_text))
        } else {
            text.lines().any(|line| line.contains(&from_text))
        };
        if referenced { return Ok(Some(path)); }
    }
    Ok(None)
}
fn all_files(root: &Path) -> Result<BTreeSet<PathBuf>, String> {
    let mut out = BTreeSet::new(); let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() { for entry in std::fs::read_dir(&dir).map_err(|e| e.to_string())? { let e = entry.map_err(|e| e.to_string())?; let p = e.path(); if p.is_dir() { stack.push(p); } else if p.is_file() { out.insert(rel(root, &p)); } } }
    Ok(out)
}
fn changed_files(root: &Path, stage: &Path) -> Result<Vec<PathBuf>, String> {
    let mut set = all_files(root)?; set.extend(all_files(stage)?); let mut out = Vec::new();
    for path in set { let a = std::fs::read(root.join(&path)).ok(); let b = std::fs::read(stage.join(&path)).ok(); if a != b { out.push(path); } } Ok(out)
}
fn atomic_commit(root: &Path, stage: &Path, writes: &[PathBuf]) -> Result<(), (PathBuf, String)> {
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| (root.to_path_buf(), e.to_string()))?.as_nanos();
    let originals: BTreeMap<PathBuf, Option<Vec<u8>>> = writes.iter().map(|path| (path.clone(), std::fs::read(root.join(path)).ok())).collect();
    let mut temp = Vec::new();
    for (i, path) in writes.iter().enumerate() {
        let src = stage.join(path);
        if src.is_file() {
            let dst = root.join(path);
            if let Some(parent) = dst.parent() { std::fs::create_dir_all(parent).map_err(|e| (path.clone(), e.to_string()))?; }
            let t = dst.with_extension(format!("lute-patch-{stamp}-{i}.tmp"));
            std::fs::copy(&src, &t).map_err(|e| (path.clone(), e.to_string()))?;
            temp.push((path.clone(), t));
        }
    }
    for (path, t) in &temp {
        if let Err(e) = std::fs::rename(t, root.join(path)) {
            for (restore, bytes) in &originals {
                let target = root.join(restore);
                match bytes {
                    Some(bytes) => { let _ = std::fs::write(target, bytes); }
                    None => { let _ = std::fs::remove_file(target); }
                }
            }
            for (_, tmp) in &temp { let _ = std::fs::remove_file(tmp); }
            return Err((path.clone(), e.to_string()));
        }
    }
    for path in writes {
        if !stage.join(path).exists() {
            if let Err(error) = std::fs::remove_file(root.join(path)) {
                for (restore, bytes) in &originals {
                    let target = root.join(restore);
                    match bytes { Some(bytes) => { let _ = std::fs::write(target, bytes); }, None => { let _ = std::fs::remove_file(target); } }
                }
                return Err((path.clone(), error.to_string()));
            }
        }
    }
    Ok(())
}

fn constraint_values(model: &ProjectModel) -> BTreeMap<String, Value> {
    let Some(project) = model.manifest() else { return BTreeMap::new(); };
    let docs: Vec<_> = model.documents().iter().map(|d| (d.path.clone(), d.doc.clone())).collect();
    let foldeds: Vec<_> = model.documents().iter().map(|d| &d.folded).collect();
    let slots = lute_check::clock_positions::project_objective_slot_results(&docs, &foldeds);
    let Some(scenario) = model.reconciled().scenarios.get(model.root()) else { return BTreeMap::new(); };
    crate::constraints::evaluate_constraints_with_foldeds(model.root(), project, &docs, &foldeds, scenario, &slots).into_iter().map(|r| (r.id.clone(), serde_json::to_value(r).unwrap())).collect()
}
fn reachability_value(model: &ProjectModel, node: &NodeKey) -> Option<Value> {
    let scenario = model.reconciled().scenarios.get(model.root())?;
    let id = match node.kind {
        NodeKind::Scene => lute_check::connectivity::NodeId::Scene(node.key.clone()),
        NodeKind::Quest => lute_check::connectivity::NodeId::Quest(node.key.clone()),
        NodeKind::Beat => lute_check::connectivity::NodeId::Beat(node.key.clone()),
        _ => return None,
    };
    scenario.reach.get(&id).map(|verdict| serde_json::json!(format!("{verdict:?}")))
}
fn preserve_node_matches(
    claim: &NodeKey,
    changed: &NodeKey,
    before: &ProjectModel,
    after: &ProjectModel,
) -> bool {
    if claim == changed
        || (claim.kind == changed.kind
            && (changed.key.strip_prefix("quest:") == Some(claim.key.as_str())
                || claim.key.strip_prefix("quest:") == Some(changed.key.as_str())))
    {
        return true;
    }
    after.identity_renames().iter().any(|rename| {
        (rename.from == claim.canonical() && rename.to == changed.canonical())
            || (rename.to == claim.canonical() && rename.from == changed.canonical())
    }) && (before.graph().nodes.contains_key(claim) || after.graph().nodes.contains_key(claim))
        && (before.graph().nodes.contains_key(changed) || after.graph().nodes.contains_key(changed))
}
fn component_fingerprints(model: &ProjectModel) -> BTreeMap<NodeKey, String> {
    let mut out = BTreeMap::new();
    for document in model.documents() {
        let Some(artifact) = document.artifact.as_ref() else { continue };
        for command in &artifact.commands {
            let lute_compile::Command::Line(line) = command else { continue };
            let Some(source) = line.stamp.source.as_ref() else { continue };
            let key = NodeKey::new(NodeKind::Line, line.line_id.clone());
            let instance = source.scope.rsplit('/').next()
                .and_then(|segment| segment.rsplit_once('#').map(|(_, key)| key))
                .unwrap_or_default();
            let fingerprint = serde_json::json!({
                "component": source.component,
                "key": instance,
                "scope": source.scope,
                "sourceOrigin": format!("{}:{}", source.component, line.line_id),
            }).to_string();
            out.insert(key, fingerprint);
        }
    }
    out
}

fn preserve_violations(items: &[Preserve], diff: &crate::SemanticDiff, before: &ProjectModel, after: &ProjectModel) -> Vec<SemanticChange> {
    let mut out = Vec::new();
    let mapped = |claim: &NodeKey| -> Option<NodeKey> {
        after.identity_renames().iter().find_map(|rename| {
            let raw = if rename.from == claim.canonical() { &rename.to }
                else if rename.to == claim.canonical() { &rename.from } else { return None };
            let (kind, key) = raw.split_once(':')?;
            let kind = match kind {
                "quest" => NodeKind::Quest, "scene" => NodeKind::Scene, "document" => NodeKind::Document,
                "entry" => NodeKind::Entry, "beat" => NodeKind::Beat, "line" => NodeKind::Line,
                "objective" => NodeKind::Objective, "choice" => NodeKind::Choice, "reward" => NodeKind::Reward,
                _ => return None,
            };
            Some(NodeKey::new(kind, key))
        })
    };
    for item in items {
        match item {
            Preserve::Ids(nodes) => {
                let mut fingerprints = None;
                for node in nodes {
                    let target = if after.graph().nodes.contains_key(node) {
                        Some(node.clone())
                    } else {
                        mapped(node).filter(|mapped| after.graph().nodes.contains_key(mapped))
                    };
                    let Some(target) = target else {
                        out.extend(diff.changes.iter().filter(|c| preserve_node_matches(node, &c.node, before, after)).cloned());
                        continue;
                    };
                    let (before_prints, after_prints) = fingerprints
                        .get_or_insert_with(|| (component_fingerprints(before), component_fingerprints(after)));
                    if let Some(fingerprint) = before_prints.get(node) {
                        if after_prints.values().filter(|candidate| *candidate == fingerprint).count() > 1 {
                            out.push(SemanticChange {
                                kind: ChangeKind::Field("componentAmbiguous".into()),
                                node: target,
                                before: Some(serde_json::json!({"fingerprint": fingerprint})),
                                after: Some(serde_json::json!({"ambiguous": true, "fingerprint": fingerprint})),
                                locations: vec![],
                                unmapped_identity: false,
                            });
                        }
                    }
                }
            }
            Preserve::LineIds => out.extend(diff.changes.iter().filter(|c| c.before.as_ref().is_some_and(|v| v.get("command").and_then(|v| v.get("lineId")).is_some()) && c.before.as_ref().and_then(|v| v.get("command")).and_then(|v| v.get("lineId")) != c.after.as_ref().and_then(|v| v.get("command")).and_then(|v| v.get("lineId"))).cloned()),
            Preserve::VoiceKeys => out.extend(diff.changes.iter().filter(|c| c.before.as_ref().is_some_and(|v| v.get("command").and_then(|v| v.get("voiceKey")).is_some()) && c.before.as_ref().and_then(|v| v.get("command")).and_then(|v| v.get("voiceKey")) != c.after.as_ref().and_then(|v| v.get("command")).and_then(|v| v.get("voiceKey"))).cloned()),
            Preserve::ChoiceEffects(nodes) => out.extend(diff.changes.iter().filter(|c| nodes.iter().any(|node| preserve_node_matches(node, &c.node, before, after)) && matches!(&c.kind, ChangeKind::Field(f) if ["effects", "writes", "asserts", "grants", "choiceEffects"].iter().any(|x| f.contains(x)))).cloned()),
            Preserve::Rewards(nodes) => out.extend(diff.changes.iter().filter(|c| nodes.iter().any(|node| preserve_node_matches(node, &c.node, before, after)) && (c.node.kind == NodeKind::Reward || matches!(&c.kind, ChangeKind::Field(f) if f.contains("reward")))).cloned()),
            Preserve::HostContracts => out.extend(diff.changes.iter().filter(|c| matches!(&c.kind, ChangeKind::Field(f) if { let f=f.to_ascii_lowercase(); f.contains("host") || f.contains("contract") || f.contains("required") || f.contains("environment") || f == "enginepath" || (f == "vocabulary" && matches!(c.node.kind, NodeKind::Project | NodeKind::Engine | NodeKind::Relation | NodeKind::State)) })).cloned()),
            Preserve::Conditions(nodes) => out.extend(diff.changes.iter().filter(|c| nodes.iter().any(|node| preserve_node_matches(node, &c.node, before, after)) && (matches!(c.kind, ChangeKind::ConditionUnparsable) || matches!(&c.kind, ChangeKind::Field(f) if f.to_ascii_lowercase().contains("condition") || f.to_ascii_lowercase().contains("when") || f.to_ascii_lowercase().contains("guard") || f == "scheduling"))).cloned()),
            Preserve::Reachability(nodes) => for node in nodes {
                let old = reachability_value(before, node);
                let mapped_node = mapped(node).unwrap_or_else(|| node.clone());
                let new = reachability_value(after, &mapped_node);
                if old != new { out.push(SemanticChange { kind: ChangeKind::Field("reachability".into()), node: mapped_node, before: old, after: new, locations: vec![], unmapped_identity: false }); }
            },
            Preserve::Constraints => {
                let old = constraint_values(before);
                let new = constraint_values(after);
                for (id, value) in &old {
                    let after_value = new.get(id);
                    if value.get("verdict") == Some(&Value::String("holds".into())) && after_value.and_then(|v| v.get("verdict")) != Some(&Value::String("holds".into())) {
                        out.push(SemanticChange { kind: ChangeKind::Field("constraints".into()), node: NodeKey::new(NodeKind::Project, id), before: Some(value.clone()), after: after_value.cloned(), locations: vec![], unmapped_identity: false });
                    }
                }
            }
        }
    }
    out.sort_by_key(|c| (c.node.clone(), format!("{:?}", c.kind))); out.dedup_by(|a,b| a.node == b.node && a.kind == b.kind); out
}

impl fmt::Display for PatchRefusal { fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { match self { Self::Io { path, message } => write!(f, "error: {}: {message}", path.display()), _ => write!(f, "{}: {}", self.code(), self.message()) } } }
impl std::error::Error for PatchRefusal {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn node_keys_are_strict() { assert!(parse_node_key("scene:x").is_ok()); assert!(parse_node_key("bogus:x").is_err()); assert!(parse_node_key("scene:").is_err()); }
    #[test]
    fn preserve_kinds_report_their_semantic_fields() {
        let root = std::env::temp_dir().join(format!("lute-patch-preserve-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        std::fs::write(root.join("scene.lute"), "---\nkind: scene\nid: hall\n---\n\n## Opening\n\n@narrator: Hello\n").unwrap();
        let model = ProjectModel::build_single_root(&root, &ModelOptions::default()).unwrap();
        let line = NodeKey::new(NodeKind::Line, "line");
        let change = |node: NodeKey, kind: ChangeKind, before: Value, after: Value| SemanticChange { node, kind, before: Some(before), after: Some(after), locations: vec![], unmapped_identity: false };
        let revision = model.revisions().clone();
        let make_diff = |change| crate::SemanticDiff { schema_version: "0.35.0.diff", before: revision.clone(), after: revision.clone(), changes: vec![change] };
        let cases = vec![
            (Preserve::Ids(vec![NodeKey::new(NodeKind::Line, "missing")]), change(NodeKey::new(NodeKind::Line, "missing"), ChangeKind::Removed, serde_json::json!({}), Value::Null)),
            (Preserve::LineIds, change(line.clone(), ChangeKind::Field("lineId".into()), serde_json::json!({"command":{"lineId":"a"}}), serde_json::json!({"command":{"lineId":"b"}}))),
            (Preserve::VoiceKeys, change(line.clone(), ChangeKind::Field("voiceKey".into()), serde_json::json!({"command":{"voiceKey":"a"}}), serde_json::json!({"command":{"voiceKey":"b"}}))),
            (Preserve::ChoiceEffects(vec![NodeKey::new(NodeKind::Choice, "c")]), change(NodeKey::new(NodeKind::Choice, "c"), ChangeKind::Field("choiceEffects".into()), serde_json::json!({}), serde_json::json!({}))),
            (Preserve::Rewards(vec![NodeKey::new(NodeKind::Reward, "r")]), change(NodeKey::new(NodeKind::Reward, "r"), ChangeKind::Field("reward".into()), serde_json::json!({}), serde_json::json!({}))),
            (Preserve::HostContracts, change(NodeKey::new(NodeKind::Project, "root"), ChangeKind::Field("requiredSemantics".into()), serde_json::json!({}), serde_json::json!({}))),
            (Preserve::Conditions(vec![line.clone()]), change(line.clone(), ChangeKind::Field("guard".into()), serde_json::json!({}), serde_json::json!({}))),
        ];
        for (item, change) in cases { assert!(!preserve_violations(&[item], &make_diff(change), &model, &model).is_empty()); }
        let _ = std::fs::remove_dir_all(root);
    }
    #[test]
    fn preserve_host_contracts_catches_required_semantics_and_engine_path() {
        let root = std::env::temp_dir().join(format!("lute-patch-host-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        std::fs::write(root.join("scene.lute"), "---\nkind: scene\nid: hall\n---\n\n## Opening\n@narrator: Hello\n").unwrap();
        let model = ProjectModel::build_single_root(&root, &ModelOptions::default()).unwrap();
        let revision = model.revisions().clone();
        let make = |node: NodeKey, field: &str| crate::SemanticDiff { schema_version: "0.35.0.diff", before: revision.clone(), after: revision.clone(), changes: vec![SemanticChange {
            node, kind: ChangeKind::Field(field.into()), before: Some(serde_json::json!({})), after: Some(serde_json::json!({})), locations: vec![], unmapped_identity: false,
        }] };
        for (name, diff) in [
            ("requiredSemantics", make(NodeKey::new(NodeKind::Project, "root"), "requiredSemantics")),
            ("owner:engine path", make(NodeKey::new(NodeKind::Engine, "clock.day"), "enginePath")),
        ] {
            assert!(!preserve_violations(&[Preserve::HostContracts], &diff, &model, &model).is_empty(), "{name}");
        }
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn preserve_reachability_reports_a_node_that_becomes_unreachable() {
        let before_root = std::env::temp_dir().join(format!("lute-patch-reach-before-{}", std::process::id()));
        let after_root = std::env::temp_dir().join(format!("lute-patch-reach-after-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&before_root);
        let _ = std::fs::create_dir_all(&after_root);
        let before_text = "---\nkind: scene\nid: hall\nwhen: \"true\"\n---\n\n## Opening\n@narrator: Hi\n";
        std::fs::write(before_root.join("scene.lute"), before_text).unwrap();
        let _ = std::fs::remove_file(after_root.join("scene.lute"));
        let before = ProjectModel::build_single_root(&before_root, &ModelOptions::default()).unwrap();
        let after = ProjectModel::build_single_root(&after_root, &ModelOptions::default()).unwrap();
        let node = NodeKey::new(NodeKind::Scene, "hall");
        let diff = crate::SemanticDiff { schema_version: "0.35.0.diff", before: before.revisions().clone(), after: after.revisions().clone(), changes: vec![] };
        assert_ne!(reachability_value(&before, &node), reachability_value(&after, &node), "before={:?} after={:?} keys={:?}", reachability_value(&before, &node), reachability_value(&after, &node), before.reconciled().scenarios.values().map(|scenario| scenario.reach.keys().collect::<Vec<_>>()).collect::<Vec<_>>());
        assert!(!preserve_violations(&[Preserve::Reachability(vec![node])], &diff, &before, &after).is_empty());
        let _ = std::fs::remove_dir_all(before_root);
        let _ = std::fs::remove_dir_all(after_root);
    }
    #[test]
    fn preserve_constraints_reports_a_verdict_that_gets_worse() {
        let before_root = std::env::temp_dir().join(format!("lute-patch-constraints-before-{}", std::process::id()));
        let after_root = std::env::temp_dir().join(format!("lute-patch-constraints-after-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&before_root);
        let _ = std::fs::create_dir_all(&after_root);
        std::fs::write(before_root.join("lute.project.yaml"), "defaultProfile: default\nprofiles:\n  default: {}\nconstraints:\n  - id: hall-reachable\n    kind: reachable\n    node: scene:hall\n").unwrap();
        std::fs::write(before_root.join("scene.lute"), "---\nkind: scene\nid: hall\n---\n\n## Opening\n@narrator: Hi\n").unwrap();
        let before = ProjectModel::build_single_root(&before_root, &ModelOptions::default()).unwrap();
        let after = ProjectModel::build_single_root(&after_root, &ModelOptions::default()).unwrap();
        let old = constraint_values(&before);
        assert!(before.manifest().is_some(), "manifest missing");
        assert_eq!(old.get("hall-reachable").and_then(|value| value.get("verdict")), Some(&Value::String("holds".into())));
        let diff = crate::SemanticDiff { schema_version: "0.35.0.diff", before: before.revisions().clone(), after: after.revisions().clone(), changes: vec![] };
        assert!(!preserve_violations(&[Preserve::Constraints], &diff, &before, &after).is_empty());
        let _ = std::fs::remove_dir_all(before_root);
        let _ = std::fs::remove_dir_all(after_root);
    }
    #[test]
    fn markdown_path_mentions_do_not_block_moves() {
        let root = std::env::temp_dir().join(format!("lute-patch-move-ref-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&root);
        std::fs::write(root.join("lute.project.yaml"), "defaultProfile: default\nprofiles:\n  default: {}\n").unwrap();
        std::fs::write(root.join("scene.lute"), "---\nkind: scene\nid: hall\n---\n\n## Hall\n@narrator: Hi\n").unwrap();
        std::fs::write(root.join("README.md"), "The old path is scene.lute, for prose only.\n").unwrap();
        assert_eq!(stale_move_reference(&root, Path::new("scene.lute")).unwrap(), None);
        std::fs::write(root.join("lute.project.yaml"), "defaults:\n  uses: [scene.lute]\n").unwrap();
        assert_eq!(stale_move_reference(&root, Path::new("scene.lute")).unwrap(), Some(PathBuf::from("lute.project.yaml")));
        let _ = std::fs::remove_dir_all(root);
    }
}
