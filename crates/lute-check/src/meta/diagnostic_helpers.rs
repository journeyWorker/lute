use super::*;

pub(crate) fn err_at(code: &str, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity: Severity::Error,
        message,
        evidence: None,
        span,
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// The YAML shape of `v`, for the "but this is …" half of
/// [`state_decl_message`] and of `E-DEF-DECL` ([`crate::def_decl`]).
pub(crate) fn yaml_shape(v: &serde_yaml::Value) -> &'static str {
    match v {
        serde_yaml::Value::Null => "an empty value",
        serde_yaml::Value::Bool(_) => "a boolean",
        serde_yaml::Value::Number(_) => "a number",
        serde_yaml::Value::String(_) => "a bare string",
        serde_yaml::Value::Sequence(_) => "a list",
        serde_yaml::Value::Tagged(_) => "a tagged value",
        serde_yaml::Value::Mapping(_) => "a mapping",
    }
}

/// Infer the import-role kind of a KIND-LESS root document from its frontmatter
/// shape (dsl §9.2/§13): a document opened standalone that is actually a Schema
/// or Component fragment. Returns None for a genuine scene missing `kind:`.
pub fn infer_meta_kind_from_shape(meta: &Meta, has_body: bool) -> Option<MetaKind> {
    let value: serde_yaml::Value = serde_yaml::from_str(&meta.raw_yaml).ok()?;
    let map = value.as_mapping()?;
    let has = |k: &str| map.contains_key(yaml_key(k));
    if has("kind") {
        return None;
    }
    if COMPONENT_ONLY_KEYS.iter().any(|k| has(k)) {
        return Some(MetaKind::Component);
    }
    if !has_body && (has("state") || has("defs")) {
        return Some(MetaKind::Schema);
    }
    None
}

/// Describe a frontmatter value that has NO [`Literal`] representation.
pub(super) fn unrepresentable_yaml(v: &serde_yaml::Value) -> &'static str {
    match v {
        serde_yaml::Value::Null => "null",
        serde_yaml::Value::Tagged(_) => "a tagged value",
        serde_yaml::Value::Sequence(_) => "a sequence with an unrepresentable element",
        serde_yaml::Value::Mapping(_) => {
            "a mapping with a non-string key or an unrepresentable value"
        }
        _ => "an unrepresentable value",
    }
}

/// Find a scalar's written representation in frontmatter and return its span.
pub(crate) fn scalar_span(meta: &Meta, raw: &str) -> Span {
    let double = raw.replace('\\', "\\\\").replace('"', "\\\"");
    let single = raw.replace('\'', "''");
    let written = [raw, double.as_str(), single.as_str()]
        .into_iter()
        .find(|written| meta.raw_yaml.contains(*written))
        .unwrap_or(raw);
    meta_key_span(meta, written)
}

/// Turn a display name into a valid identifier for an id / state path segment.
pub fn ident_from_name(name: &str, fallback: &str) -> String {
    let mut out = String::new();
    let mut new_word = false;
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() {
            if out.is_empty() {
                out.push(ch.to_ascii_lowercase());
            } else if new_word {
                out.push(ch.to_ascii_uppercase());
            } else {
                out.push(ch);
            }
            new_word = false;
        } else {
            new_word = !out.is_empty();
        }
    }
    if out.is_empty() {
        return fallback.to_string();
    }
    if out.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        out.insert(0, 'q');
    }
    out
}

pub(crate) fn yaml_key(k: &str) -> serde_yaml::Value {
    serde_yaml::Value::String(k.to_string())
}
