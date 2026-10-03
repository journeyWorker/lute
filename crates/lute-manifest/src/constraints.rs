use std::collections::BTreeMap;
use std::ops::Range;

use serde_yaml::Value;
use crate::yaml_text::{yaml_span, YamlStep};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstraintSeverity { Error, Warning, Info }
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstraintKind { Reachable, Completable, SpeaksOnlyWhen, NoSingleSlotProgress }
impl ConstraintKind { pub fn as_str(self) -> &'static str { match self { Self::Reachable => "reachable", Self::Completable => "completable", Self::SpeaksOnlyWhen => "speaksOnlyWhen", Self::NoSingleSlotProgress => "noSingleSlotProgress" } } }

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstraintDecl {
    pub id: String,
    pub kind: ConstraintKind,
    pub severity: ConstraintSeverity,
    pub node: Option<String>, pub quest: Option<String>, pub speaker: Option<String>, pub when: Option<String>,
    pub span: Range<usize>, pub field_spans: BTreeMap<String, Range<usize>>,
}

pub fn parse_constraints(raw: Option<&Value>, text: &str) -> (Vec<ConstraintDecl>, Vec<super::project::ResolveDiag>) {
    let mut out = Vec::new(); let mut diags = Vec::new();
    let Some(raw) = raw else { return (out, diags) };
    let Some(items) = raw.as_sequence() else { diags.push(err("`constraints:` must be a list", field_span(text, "constraints"))); return (out, diags); };
    let mut ids = std::collections::BTreeSet::new();
    for (idx, item) in items.iter().enumerate() {
        let item_path = [YamlStep::Key("constraints"), YamlStep::Item(idx)];
        let mut decl_span = yaml_span(text, &item_path)
            .map(|span| span.byte_start..span.byte_end)
            .unwrap_or_else(|| field_span(text, "constraints"));
        let Some(map) = item.as_mapping() else { diags.push(err("constraint declaration must be a mapping", decl_span)); continue; };
        let mut fields = BTreeMap::new();
        for (k, _) in map {
            if let Some(k) = k.as_str() {
                let key_path = [YamlStep::Key("constraints"), YamlStep::Item(idx), YamlStep::Key(k)];
                let span = yaml_span(text, &key_path)
                    .map(|span| span.byte_start..span.byte_end)
                    .unwrap_or_else(|| decl_span.clone());
                let value_path = [YamlStep::Key("constraints"), YamlStep::Item(idx), YamlStep::Key(k), YamlStep::Value];
                if let Some(value) = yaml_span(text, &value_path) {
                    decl_span.end = decl_span.end.max(value.byte_end);
                }
                fields.insert(k.to_string(), span);
            }
        }
        let get = |name: &str| map.get(Value::String(name.into()));
        let id = match get("id").and_then(Value::as_str).map(str::trim).filter(|s| !s.is_empty()) {
            Some(v) => v.to_string(),
            None => {
                diags.push(err(
                    "constraint declaration requires a non-empty `id`",
                    fields.get("id").cloned().unwrap_or_else(|| decl_span.clone()),
                ));
                continue;
            }
        };
        if !ids.insert(id.clone()) {
            diags.push(err(
                format!("duplicate constraint id `{id}`"),
                fields.get("id").cloned().unwrap_or_else(|| decl_span.clone()),
            ));
            continue;
        }
        let kind = match get("kind").and_then(Value::as_str) {
            Some("reachable") => ConstraintKind::Reachable,
            Some("completable") => ConstraintKind::Completable,
            Some("speaksOnlyWhen") => ConstraintKind::SpeaksOnlyWhen,
            Some("noSingleSlotProgress") => ConstraintKind::NoSingleSlotProgress,
            Some(other) => {
                diags.push(err(
                    format!("unknown constraint kind `{other}`"),
                    fields.get("kind").cloned().unwrap_or_else(|| decl_span.clone()),
                ));
                continue;
            }
            None => {
                diags.push(err(
                    "constraint declaration requires `kind`",
                    fields.get("kind").cloned().unwrap_or_else(|| decl_span.clone()),
                ));
                continue;
            }
        };
        let severity = match get("severity").and_then(Value::as_str).unwrap_or("error") {
            "error" => ConstraintSeverity::Error,
            "warning" => ConstraintSeverity::Warning,
            "info" => ConstraintSeverity::Info,
            other => {
                diags.push(err(
                    format!("invalid constraint severity `{other}`"),
                    fields.get("severity").cloned().unwrap_or_else(|| decl_span.clone()),
                ));
                continue;
            }
        };
        let allowed = match kind {
            ConstraintKind::Reachable => &["id", "kind", "severity", "node"][..],
            ConstraintKind::Completable | ConstraintKind::NoSingleSlotProgress => {
                &["id", "kind", "severity", "quest"][..]
            }
            ConstraintKind::SpeaksOnlyWhen => &["id", "kind", "severity", "speaker", "when"][..],
        };
        let mut malformed = false;
        for key in map.keys().filter_map(Value::as_str).filter(|key| !allowed.contains(key)) {
            malformed = true;
            diags.push(err(
                format!("unknown key `{key}` in constraint declaration"),
                fields.get(key).cloned().unwrap_or_else(|| decl_span.clone()),
            ));
        }
        if malformed {
            continue;
        }
        let req = |name: &str| {
            get(name)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        };
        let node = req("node");
        let quest = req("quest");
        let speaker = req("speaker");
        let when = req("when");
        let required_fields: &[&str] = match kind {
            ConstraintKind::Reachable => &["node"],
            ConstraintKind::Completable | ConstraintKind::NoSingleSlotProgress => &["quest"],
            ConstraintKind::SpeaksOnlyWhen => &["speaker", "when"],
        };
        let mut malformed = false;
        for field in required_fields {
            if get(field).is_some() && req(field).is_none() {
                diags.push(err(
                    format!("constraint field `{field}` must be a non-empty string"),
                    fields.get(*field).cloned().unwrap_or_else(|| decl_span.clone()),
                ));
                malformed = true;
            }
        }
        if malformed {
            continue;
        }
        let missing = match kind {
            ConstraintKind::Reachable => node.is_none(),
            ConstraintKind::Completable | ConstraintKind::NoSingleSlotProgress => quest.is_none(),
            ConstraintKind::SpeaksOnlyWhen => speaker.is_none() || when.is_none(),
        };
        if missing {
            diags.push(err(
                format!("constraint `{}` is missing a required field", kind.as_str()),
                decl_span.clone(),
            ));
            continue;
        }
        if let Some(n) = &node {
            if !valid_node_ref(n) {
                diags.push(err(
                    format!("bad constraint node reference `{n}`"),
                    fields.get("node").cloned().unwrap_or_else(|| decl_span.clone()),
                ));
                continue;
            }
        }
        out.push(ConstraintDecl {
            id,
            kind,
            severity,
            node,
            quest,
            speaker,
            when,
            span: decl_span,
            field_spans: fields,
        });
    }
    (out, diags)
}
fn valid_node_ref(s: &str) -> bool { let Some((kind, id)) = s.split_once(':') else { return false }; matches!(kind, "scene"|"beat"|"entry"|"quest"|"objective"|"line") && !id.trim().is_empty() }
fn err(message: impl Into<String>, span: Range<usize>) -> super::project::ResolveDiag { super::project::ResolveDiag { code: "E-CONSTRAINT-DECL".into(), message: message.into(), span: Some(span) } }
fn field_span(text: &str, key: &str) -> Range<usize> {
    yaml_span(text, &[YamlStep::Key(key)])
        .map(|span| span.byte_start..span.byte_end)
        .unwrap_or(0..key.len().min(text.len()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_typed_declarations_and_retains_field_spans() {
        let text = "constraints:\n  - id: reach\n    kind: reachable\n    node: scene:hub\n";
        let value: Value = serde_yaml::from_str(text).unwrap();
        let (items, diags) = parse_constraints(value.get("constraints"), text);
        assert!(diags.is_empty(), "{diags:?}");
        assert_eq!(items[0].id, "reach");
        assert!(items[0].field_spans["node"].start > 0);
    }
    #[test]
    fn malformed_declarations_are_field_anchored() {
        let text = "constraints:\n  - id: bad\n    kind: nope\n    node: not-a-ref\n";
        let value: Value = serde_yaml::from_str(text).unwrap();
        let (_, diags) = parse_constraints(value.get("constraints"), text);
        assert_eq!(diags[0].code, "E-CONSTRAINT-DECL");
        assert!(diags[0].span.as_ref().unwrap().start >= text.find("kind").unwrap());
    }
    #[test]
    fn flow_declarations_and_errors_anchor_each_mapping_field() {
        let text = "constraints: [{id: a, kind: reachable, node: scene:first}, {id: b, kind: reachable, node: scene:second}, {id: bad, kind: nope}]\n";
        let value: Value = serde_yaml::from_str(text).unwrap();
        let (items, diags) = parse_constraints(value.get("constraints"), text);
        assert_eq!(items.len(), 2);
        assert_eq!(&text[items[0].field_spans["node"].clone()], "node");
        assert_eq!(items[1].span.start, text.find("{id: b").unwrap());
        assert_eq!(items[1].field_spans["node"].start, text.find("node: scene:second").unwrap());
        assert_eq!(diags[0].span.as_ref().unwrap().start, text.find("kind: nope").unwrap());
    }
}
