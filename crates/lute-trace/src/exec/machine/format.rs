//! Interpolation and value formatting: `{{…}}` placeholders, enum labels,
//! a placeholder's `format`, and a [`Value`]'s JSON and text renderings.

use serde_json::{json, Value as Json};

use super::Machine;
use crate::exec::driver::Driver;
use crate::Value;

impl<D: Driver> Machine<D> {
    /// Substitute `{{…}}` markers: a `path` with its value (reserved
    /// defaults included), a `ref` by evaluating its inlined def body
    /// (`expr.raw`, lute 0.21.1). A marker whose value is unknown (unset
    /// path, undecided value or ref) or a reserved token keeps its verbatim
    /// text (state-lifecycle.md). A placeholder's `format` (dsl 0.24.0 §4)
    /// applies to the value: `ordinal` renders a number as an English
    /// ordinal ([`formatted`]).
    pub(super) fn interpolate(&mut self, text: &str, placeholders: Option<&Vec<Json>>) -> String {
        let Some(phs) = placeholders else {
            return text.to_string();
        };
        if phs.is_empty() {
            return text.to_string();
        }
        let mut out = String::new();
        let mut rest = text;
        let mut it = phs.iter();
        while let Some(open) = rest.find("{{") {
            out.push_str(&rest[..open]);
            let Some(rel_close) = rest[open..].find("}}") else {
                break;
            };
            let end = open + rel_close + 2;
            let marker = &rest[open..end];
            let rendered = match it.next() {
                Some(ph) if ph.get("kind").and_then(Json::as_str) == Some("path") => {
                    let path = ph.get("path").and_then(Json::as_str).unwrap_or("");
                    match self.store.eval(path).0 {
                        Value::Unknown => marker.to_string(),
                        v => formatted(ph, &v).unwrap_or_else(|| self.path_text(path, &v)),
                    }
                }
                // Prerelease N8 / dsl 0.27.0 §7: the raised member of a kind
                // beat, by its cast display name when it is a cast id, else
                // by its kind's label, else the id.
                Some(ph) if ph.get("kind").and_then(Json::as_str) == Some("occasionTarget") => {
                    match self.store.values.get(lute_check::beats::OCCASION_TARGET) {
                        Some(Value::Str(m)) => self
                            .display_names
                            .get(m)
                            .or_else(|| {
                                let kind = ph.get("entityKind").and_then(Json::as_str)?;
                                self.store.kind_labels.get(kind)?.get(m)
                            })
                            .cloned()
                            .unwrap_or_else(|| m.clone()),
                        Some(Value::Unknown) | None => marker.to_string(),
                        Some(v) => value_to_string(v),
                    }
                }
                Some(ph) if ph.get("kind").and_then(Json::as_str) == Some("ref") => {
                    let raw = ph.pointer("/expr/raw").and_then(Json::as_str).unwrap_or("");
                    match self.eval_raw(raw) {
                        Value::Unknown => marker.to_string(),
                        v => formatted(ph, &v).unwrap_or_else(|| value_to_string(&v)),
                    }
                }
                _ => marker.to_string(),
            };
            out.push_str(&rendered);
            rest = &rest[end..];
        }
        out.push_str(rest);
        out
    }

    /// A `{{path}}` value as text: an enum member with a declared label
    /// renders the label (dsl 0.24.0 §1) — `prev.run.X` shares `run.X`'s —
    /// anything else its plain value.
    fn path_text(&self, path: &str, v: &Value) -> String {
        if let Value::Str(s) = v {
            let labels = self.store.labels.get(path).or_else(|| {
                path.strip_prefix("prev.")
                    .and_then(|p| self.store.labels.get(p))
            });
            if let Some(label) = labels.and_then(|l| l.get(s)) {
                return label.clone();
            }
        }
        value_to_string(v)
    }
}

/// A trace [`Value`] → JSON (integral numbers collapse to integers).
pub fn value_to_json(v: &Value) -> Json {
    match v {
        Value::Bool(b) => json!(b),
        Value::Num(n) => {
            if n.fract() == 0.0 && n.is_finite() && n.abs() < 9.007e15 {
                json!(*n as i64)
            } else {
                json!(n)
            }
        }
        Value::Str(s) => json!(s),
        Value::Unknown => Json::Null,
    }
}

/// dsl 0.24.0 §4 / 0.25.0 §8 / 0.27.0 §7: a placeholder's `format` applied
/// to its value ([`lute_syntax::ast::format_number`]) — `ordinal` renders a
/// number as an English ordinal (`3rd`, `11th`), `ordinalWord` as a word
/// (`third`) up to `twentieth`, `plural` as the placeholder's singular form
/// when the number is 1 and its plural form otherwise (`#` in a form is the
/// number). `None` when the placeholder carries no format or the value has
/// no such rendering (a fraction's ordinal): the value then renders
/// unchanged.
fn formatted(ph: &Json, v: &Value) -> Option<String> {
    match (ph.get("format").and_then(Json::as_str), v) {
        (Some(format), Value::Num(n)) => {
            let forms: Option<Vec<String>> = ph.get("forms").and_then(Json::as_array).map(|a| {
                a.iter()
                    .filter_map(|f| f.as_str().map(str::to_string))
                    .collect()
            });
            lute_syntax::ast::format_number(format, forms.as_deref(), *n, &value_to_string(v))
        }
        _ => None,
    }
}

pub fn value_to_string(v: &Value) -> String {
    match v {
        Value::Bool(b) => b.to_string(),
        Value::Num(n) => {
            if n.fract() == 0.0 && n.is_finite() && n.abs() < 9.007e15 {
                (*n as i64).to_string()
            } else {
                n.to_string()
            }
        }
        Value::Str(s) => s.clone(),
        Value::Unknown => "unset".to_string(),
    }
}
