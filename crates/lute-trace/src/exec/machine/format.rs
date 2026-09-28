//! Interpolation and value formatting: `{{…}}` placeholders, enum labels,
//! a placeholder's `format`, and a [`Value`]'s JSON and text renderings.

use serde_json::{json, Value as Json};

use super::Machine;
use crate::exec::driver::Driver;
use crate::exec::store::LabelForms;
use crate::Value;

impl<D: Driver> Machine<D> {
    /// Substitute `{{…}}` markers: a `path` with its value (reserved
    /// defaults included), a `ref` by evaluating its inlined def body
    /// (`expr.raw`, lute 0.21.1). A marker whose value is unknown (unset
    /// path, undecided value or ref) or a reserved token keeps its verbatim
    /// text (state-lifecycle.md). A placeholder's `format` (dsl 0.24.0 §4)
    /// applies to the value: a number hint to a number ([`formatted`]), a
    /// text hint to the text it renders as ([`texted`]).
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
                    match self.store.eval_path(path).0 {
                        Value::Unknown => marker.to_string(),
                        v => formatted(ph, &v).unwrap_or_else(|| {
                            let forms = self.path_forms(path, &v);
                            texted(ph, self.path_text(path, &v), forms)
                        }),
                    }
                }
                // The raised member of a kind beat, by its kind's label when
                // the kind declares one (a speaker head keeps the cast name),
                // else by its cast display name when it is a cast id, else
                // the id.
                Some(ph) if ph.get("kind").and_then(Json::as_str) == Some("occasionTarget") => {
                    match self.store.values.get(lute_check::beats::OCCASION_TARGET) {
                        Some(Value::Str(m)) => {
                            let kind = ph.get("entityKind").and_then(Json::as_str);
                            match kind.and_then(|k| self.store.kind_labels.get(k)?.get(m)) {
                                Some(label) => texted(
                                    ph,
                                    label.clone(),
                                    kind.and_then(|k| self.store.kind_label_forms.get(k)?.get(m)),
                                ),
                                None => {
                                    texted(ph, self.display_names.get(m).unwrap_or(m).clone(), None)
                                }
                            }
                        }
                        Some(Value::Unknown) | None => marker.to_string(),
                        Some(v) => value_to_string(v),
                    }
                }
                Some(ph) if ph.get("kind").and_then(Json::as_str) == Some("ref") => {
                    let raw = ph.pointer("/expr/raw").and_then(Json::as_str).unwrap_or("");
                    match self.eval_raw(raw) {
                        Value::Unknown => marker.to_string(),
                        v => formatted(ph, &v)
                            .unwrap_or_else(|| texted(ph, value_to_string(&v), None)),
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

    /// The first `{{occasion.payload.<field>}}` marker among `placeholders`
    /// whose field this raise left unset (dsl 0.28.0, T1-13).
    pub(super) fn unset_payload(&self, placeholders: Option<&Vec<Json>>) -> Option<String> {
        let prefix = format!("{}.", lute_check::occasion_bind::OCCASION_PAYLOAD);
        placeholders?
            .iter()
            .filter(|ph| ph.get("kind").and_then(Json::as_str) == Some("path"))
            .filter_map(|ph| ph.get("path").and_then(Json::as_str))
            .find(|path| {
                path.starts_with(&prefix)
                    && matches!(self.store.read(path), crate::eval::Read::Unset)
            })
            .map(str::to_string)
    }

    /// Whether `placeholders` read `occasion.target` — `{{occasion.target}}`
    /// in any form, `{{run.count[occasion.target]}}` — with no member bound
    /// (a trace that did not mock the raise).
    pub(super) fn unbound_target(&self, placeholders: Option<&Vec<Json>>) -> bool {
        if matches!(
            self.store.values.get(lute_check::beats::OCCASION_TARGET),
            Some(crate::Value::Str(_))
        ) {
            return false;
        }
        placeholders
            .into_iter()
            .flatten()
            .any(|ph| match ph.get("kind").and_then(Json::as_str) {
                Some("occasionTarget") => true,
                Some("ref") => ph
                    .pointer("/expr/raw")
                    .and_then(Json::as_str)
                    .is_some_and(lute_check::occasion_bind::mentions_target),
                _ => false,
            })
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

    /// The declared label forms of a `{{path}}` value's member, when the
    /// path is typed against an entity kind that declares them.
    fn path_forms(&self, path: &str, v: &Value) -> Option<&LabelForms> {
        let Value::Str(s) = v else { return None };
        self.store
            .label_forms
            .get(path)
            .or_else(|| {
                path.strip_prefix("prev.")
                    .and_then(|p| self.store.label_forms.get(p))
            })?
            .get(s)
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

/// A placeholder's text hint (`capitalize`, `start`, `indefinite`) applied
/// to the text a value renders as ([`lute_syntax::ast::format_text`]), with
/// the member's declared label `forms`; the text unchanged without one.
fn texted(ph: &Json, text: String, forms: Option<&LabelForms>) -> String {
    ph.get("format")
        .and_then(Json::as_str)
        .and_then(|format| {
            lute_syntax::ast::format_text(
                format,
                &text,
                forms.and_then(|f| f.start.as_deref()),
                forms.and_then(|f| f.indefinite.as_deref()),
            )
        })
        .unwrap_or(text)
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
