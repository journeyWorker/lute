//! Interpolation and value formatting: `{{…}}` placeholders, enum labels,
//! a placeholder's `format`, and a [`Value`]'s JSON and text renderings.
//!
//! IR text (a line's `text`, a segment run's `text`, a merged translation)
//! is read by lute 0.47.1 §3: scanning left to right, a run of n
//! backslashes directly before `{{` stands for ⌊n/2⌋ literal backslashes;
//! if n is odd the `{{` is literal text (consumed as one unit before the
//! next `{{` is looked for), if even it opens a marker that ends at the
//! first `}}` after it. Every other byte is literal. The rendered text holds
//! the decoded characters, never the encoding backslashes.

use serde_json::{json, Value as Json};

use super::{Machine, Site as UnknownAt};
use crate::driver::{Driver, MenuText, SiteKind};
use crate::{UnresolvedAtom, Value};
use lute_ir::LabelForms;

impl<D: Driver> Machine<D> {
    /// A display text and its translations, rendered as a line's are — the
    /// one rendering path of lines, directive `text` attributes (0.51.0
    /// §5.6) and menu labels and prompts (0.52.0 §6.3): `raw`'s markers once
    /// each against `placeholders` ([`Self::render_markers`]) spliced into
    /// the text, and each translation of `texts` (locale → text) with the
    /// value the source gave its marker of the same referent (dsl 0.47.0
    /// §2.2). The rendered markers are returned too, for a line's
    /// `segments`. It consults no halt rule: whether a text stops the walk
    /// ([`Self::text_halts`]) is the caller's to ask before it renders.
    pub(crate) fn render_text(
        &mut self,
        raw: &str,
        placeholders: Option<&Vec<Json>>,
        texts: Option<&Json>,
    ) -> (MenuText, Vec<String>) {
        let rendered = self.render_markers(raw, placeholders);
        let text = substitute_markers(raw, &rendered, &mut 0);
        let texts = texts
            .and_then(Json::as_object)
            .map(|locales| {
                locales
                    .iter()
                    .filter_map(|(locale, t)| {
                        Some((
                            locale.clone(),
                            substitute_by_marker(t.as_str()?, raw, &rendered),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        (MenuText { text, texts }, rendered)
    }

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
    pub(crate) fn render_markers(
        &mut self,
        text: &str,
        placeholders: Option<&Vec<Json>>,
    ) -> Vec<String> {
        let phs: &[Json] = placeholders.map(Vec::as_slice).unwrap_or_default();
        let mut out = Vec::new();
        let mut it = phs.iter();
        for marker in markers(text) {
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
                    match self
                        .store
                        .values
                        .get(lute_manifest::semantics::beats::OCCASION_TARGET)
                    {
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
        }
        out
    }

    /// Whether rendering a text whose markers are `placeholders` (a line's,
    /// or a directive `text` attribute's, 0.51.0 §5.6) at command `at`
    /// stops the walk: a payload field the raise did not carry (dsl 0.28.0,
    /// T1-13) or a kind beat's `{{occasion.target}}` with no member bound
    /// has no text to print, and the driver decides whether the walk stops
    /// there. When it does not, the marker renders verbatim.
    pub(crate) fn text_halts(&mut self, placeholders: Option<&Vec<Json>>, at: &str) -> bool {
        if let Some(path) = self.unset_payload(placeholders) {
            let atoms = [UnresolvedAtom::Path(path.clone())];
            self.unresolved.extend(atoms.iter().cloned());
            if self.at_unknown(UnknownAt::new(SiteKind::Payload, &path, at), &path, &atoms) {
                return true;
            }
        }
        if self.unbound_target(placeholders) {
            let target = lute_manifest::semantics::beats::OCCASION_TARGET;
            let atoms = [UnresolvedAtom::Path(target.to_string())];
            self.unresolved.extend(atoms.iter().cloned());
            if self.at_unknown(
                UnknownAt::new(SiteKind::OccasionTarget, target, at),
                target,
                &atoms,
            ) {
                return true;
            }
        }
        false
    }

    /// The first `{{occasion.payload.<field>}}` marker among `placeholders`
    /// whose field this raise left unset (dsl 0.28.0, T1-13).
    pub(crate) fn unset_payload(&self, placeholders: Option<&Vec<Json>>) -> Option<String> {
        let prefix = format!(
            "{}.",
            lute_manifest::semantics::occasion_bind::OCCASION_PAYLOAD
        );
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
    pub(crate) fn unbound_target(&self, placeholders: Option<&Vec<Json>>) -> bool {
        if matches!(
            self.store
                .values
                .get(lute_manifest::semantics::beats::OCCASION_TARGET),
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
pub(crate) fn substitute_markers(text: &str, rendered: &[String], next: &mut usize) -> String {
    splice_markers(text, |_| {
        let value = rendered.get(*next).map(String::as_str);
        *next += 1;
        value
    })
}

/// `text` (a translation) with each `{{…}}` marker replaced by the value
/// the source line rendered for its marker with the same referent — the
/// inner text trimmed, as the parser reads it, so `{{ run.a }}` matches
/// `{{run.a}}` (dsl 0.47.0 §2.2). `source` is the line's IR text and
/// `rendered` its markers' values in order, so a translator's reordering
/// keeps each value with its marker. A marker the source does not have
/// stays as written. A literal `{{` (written `\{{`, lute 0.47.1 §3) is no
/// marker, in the translation or the source.
pub(crate) fn substitute_by_marker(text: &str, source: &str, rendered: &[String]) -> String {
    splice_markers(text, |marker| {
        let referent = referent_of(marker);
        markers(source)
            .zip(rendered)
            .find(|(source_marker, _)| referent_of(source_marker) == referent)
            .map(|(_, value)| value.as_str())
    })
}

/// One `{{` of IR text: the backslash run before it starts at `run`, the
/// `{{` at `open`; `end` is past the `}}` of a marker, past the `{{` of a
/// literal opener.
struct Site {
    run: usize,
    open: usize,
    end: usize,
    marker: bool,
}

/// The `{{` sites of IR text, left to right (module docs). A marker with no
/// `}}` after it ends the scan: the rest of the text is read as written.
fn sites(text: &str) -> impl Iterator<Item = Site> + '_ {
    let mut at = 0;
    std::iter::from_fn(move || {
        let open = at + text.get(at..)?.find("{{")?;
        let b = text.as_bytes();
        let mut run = open;
        while run > at && b[run - 1] == b'\\' {
            run -= 1;
        }
        let site = if (open - run) % 2 == 1 {
            Site {
                run,
                open,
                end: open + 2,
                marker: false,
            }
        } else {
            let rel_close = text[open + 2..].find("}}")?;
            Site {
                run,
                open,
                end: open + 2 + rel_close + 2,
                marker: true,
            }
        };
        at = site.end;
        Some(site)
    })
}

/// The `{{…}}` markers of IR `text`, left to right, as written.
fn markers(text: &str) -> impl Iterator<Item = &str> {
    sites(text)
        .filter(|site| site.marker)
        .map(|site| &text[site.open..site.end])
}

/// A `{{…}}` marker's inner text, trimmed.
fn referent_of(marker: &str) -> &str {
    marker[2..marker.len() - 2].trim()
}

/// IR `text` decoded, with each marker, left to right, replaced by
/// `value(marker)`; a marker with no value stays verbatim.
fn splice_markers<'a>(text: &str, mut value: impl FnMut(&str) -> Option<&'a str>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for site in sites(text) {
        out.push_str(&text[at..site.run]);
        for _ in 0..(site.open - site.run) / 2 {
            out.push('\\');
        }
        if site.marker {
            let marker = &text[site.open..site.end];
            out.push_str(value(marker).unwrap_or(marker));
        } else {
            out.push_str("{{");
        }
        at = site.end;
    }
    out.push_str(&text[at..]);
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
                a.iter()
                    .filter_map(|f| f.as_str().map(str::to_string))
                    .collect()
            });
            lute_manifest::text::format_number(
                format,
                forms.as_deref(),
                *n as f64,
                &value_to_string(v),
            )
        }
        (Some(format), Value::Double(n)) => {
            let forms: Option<Vec<String>> = ph.get("forms").and_then(Json::as_array).map(|a| {
                a.iter()
                    .filter_map(|f| f.as_str().map(str::to_string))
                    .collect()
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
