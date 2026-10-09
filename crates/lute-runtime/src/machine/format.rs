//! Interpolation and value formatting: `{{…}}` placeholders, enum labels,
//! a placeholder's `format`, and a [`Value`]'s JSON and text renderings.

use serde_json::{json, Value as Json};

use super::Machine;
use crate::driver::Driver;
use lute_ir::LabelForms;
use crate::Value;

impl<D: Driver> Machine<D> {
    /// Render each `{{…}}` marker of `text`, left to right, ONCE against its
    /// placeholder (same index): a `path` with its value (reserved defaults
    /// included), a `ref` by evaluating its inlined def body (`expr.raw`,
    /// lute 0.21.1). A marker whose value is unknown (unset path, undecided
    /// value or ref) or a reserved token keeps its verbatim text
    /// (state-lifecycle.md). A placeholder's `format` (dsl 0.24.0 §4) applies
    /// to the value: a number hint to a number ([`formatted`]), a text hint
    /// to the text it renders as ([`texted`]). The line text and a modified
    /// line's `segments` splice from this one list by global marker index
    /// ([`substitute_markers`]), so a placeholder is evaluated once per line
    /// however the runs split it.
    pub(super) fn render_markers(
        &mut self,
        text: &str,
        placeholders: Option<&Vec<Json>>,
    ) -> Vec<String> {
        let phs: &[Json] = placeholders.map(Vec::as_slice).unwrap_or_default();
        let mut out = Vec::new();
        let mut rest = text;
        let mut it = phs.iter();
        while let Some(open) = rest.find("{{") {
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
                    match self.store.values.get(lute_manifest::semantics::beats::OCCASION_TARGET) {
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
                    let value = self.slot(ph.get("expr")).map(|s| self.eval_value(&s));
                    match value {
                        None | Some(Value::Unknown) => marker.to_string(),
                        Some(v) => formatted(ph, &v)
                            .unwrap_or_else(|| texted(ph, value_to_string(&v), None)),
                    }
                }
                _ => marker.to_string(),
            };
            out.push(rendered);
            rest = &rest[end..];
        }
        out
    }

    /// The first `{{occasion.payload.<field>}}` marker among `placeholders`
    /// whose field this raise left unset (dsl 0.28.0, T1-13).
    pub(super) fn unset_payload(&self, placeholders: Option<&Vec<Json>>) -> Option<String> {
        let prefix = format!("{}.", lute_manifest::semantics::occasion_bind::OCCASION_PAYLOAD);
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
            self.store.values.get(lute_manifest::semantics::beats::OCCASION_TARGET),
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
                    .pointer("/expr/cel")
                    .and_then(Json::as_str)
                    .is_some_and(lute_manifest::semantics::occasion_bind::mentions_target),
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

/// `text` with its `{{…}}` markers replaced, left to right, by
/// `rendered[*next..]` — `next` is the GLOBAL marker index the first marker
/// of `text` has, advanced past every marker replaced. A marker past the
/// list stays verbatim.
pub(super) fn substitute_markers(text: &str, rendered: &[String], next: &mut usize) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find("{{") {
        let Some(rel_close) = rest[open..].find("}}") else {
            break;
        };
        let end = open + rel_close + 2;
        out.push_str(&rest[..open]);
        match rendered.get(*next) {
            Some(r) => out.push_str(r),
            None => out.push_str(&rest[open..end]),
        }
        *next += 1;
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

/// A trace [`Value`] → JSON (integral numbers collapse to integers).
pub fn value_to_json(v: &Value) -> Json {
    match v {
        Value::Bool(b) => json!(b),
        Value::Int(n) => json!(n),
        Value::Double(n) => json!(n),
        Value::Str(s) => json!(s),
        Value::Unknown | Value::Error(_) => Json::Null,
    }
}

/// dsl 0.24.0 §4 / 0.25.0 §8 / 0.27.0 §7: a placeholder's `format` applied
/// to its value ([`lute_manifest::text::format_number`]) — `ordinal` renders a
/// number as an English ordinal (`3rd`, `11th`), `ordinalWord` as a word
/// (`third`) up to `twentieth`, `plural` as the placeholder's singular form
/// when the number is 1 and its plural form otherwise (`#` in a form is the
/// number). `None` when the placeholder carries no format or the value has
/// no such rendering (a fraction's ordinal): the value then renders
/// unchanged.
fn formatted(ph: &Json, v: &Value) -> Option<String> {
    match (ph.get("format").and_then(Json::as_str), v) {
        (Some(format), Value::Int(n)) => {
            let forms: Option<Vec<String>> = ph.get("forms").and_then(Json::as_array).map(|a| {
                a.iter().filter_map(|f| f.as_str().map(str::to_string)).collect()
            });
            lute_manifest::text::format_number(format, forms.as_deref(), *n as f64, &value_to_string(v))
        }
        (Some(format), Value::Double(n)) => {
            let forms: Option<Vec<String>> = ph.get("forms").and_then(Json::as_array).map(|a| {
                a.iter().filter_map(|f| f.as_str().map(str::to_string)).collect()
            });
            lute_manifest::text::format_number(format, forms.as_deref(), *n, &value_to_string(v))
        }
        _ => None,
    }
}

/// A placeholder's text hint (`capitalize`, `start`, `indefinite`) applied
/// to the text a value renders as ([`lute_manifest::text::format_text`]), with
/// the member's declared label `forms`; the text unchanged without one.
fn texted(ph: &Json, text: String, forms: Option<&LabelForms>) -> String {
    ph.get("format")
        .and_then(Json::as_str)
        .and_then(|format| {
            lute_manifest::text::format_text(
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
        Value::Int(n) => n.to_string(),
        Value::Double(n) => n.to_string(),
        Value::Str(s) => s.clone(),
        Value::Unknown | Value::Error(_) => "unset".to_string(),
    }
}
