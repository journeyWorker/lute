//! Locale bundle (dsl 0.8.0 §7) — the canonical `lineId -> locale -> text`
//! table `lute loc import` produces and `lute compile --locales` merges into an
//! already-addressed [`Artifact`].
//!
//! ## Why the merge lives downstream of addressing
//! `lineId` is assigned by [`crate::address::assign_addresses`], the LAST pass
//! [`crate::compile_with_check`] runs before it builds the [`Artifact`]. A
//! bundle is keyed on that identity (never on `addr`, which is a REGENERATED
//! position — spec §4.2/§12), so the merge can only run once the artifact
//! exists. [`merge_locales`] therefore takes a finished [`Artifact`] rather
//! than threading a bundle through the pipeline: the compile signature, and
//! every byte a bundle-less compile emits, stay exactly as they were.
//!
//! ## What the merge does and does not touch
//! - `LineCmd.texts`, `ChoiceOption.labels`, `HubOption.labels` are filled from
//!   the bundle entry matching that record's own `lineId`. All three are
//!   `skip_serializing_if = "BTreeMap::is_empty"`, so a document with no
//!   matching entry is byte-identical to 0.7.0.
//! - `LineCmd.text` / the option `label` are NEVER overwritten. They remain the
//!   SOURCE-language string (`contentLang`, dsl 0.8.0 §7), so a 0.7 consumer
//!   that ignores the new maps reads exactly what it read before.
//! - A bundle entry whose `lineId` matches no record in THIS document is
//!   silently ignored — one bundle legitimately spans a whole project.
//! - A record missing a locale the bundle DECLARES is [`W_L10N_MISSING`], one
//!   diagnostic per `(lineId, locale)` pair. A warning, so
//!   `--deny W-L10N-MISSING` promotes it in CI.

use std::collections::BTreeMap;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use serde::Serialize;

use crate::ir::{Artifact, Command};

/// A translatable record carries no text for a locale the bundle declares
/// (dsl 0.8.0 §7). Warning-grade: an untranslated build is still a valid
/// build, and CI opts into strictness with `--deny W-L10N-MISSING`.
pub const W_L10N_MISSING: &str = "W-L10N-MISSING";

/// The only `schemaVersion` [`LocaleBundle::parse`] accepts. Bumped only for a
/// breaking shape change; a bundle is a tool-produced artifact, so an unknown
/// version is rejected rather than guessed at.
pub const BUNDLE_SCHEMA_VERSION: i64 = 1;

/// The canonical locale bundle (dsl 0.8.0 §7).
///
/// `locales` is the DECLARED locale set — the axis the [`W_L10N_MISSING`]
/// completeness check runs over — sorted and deduplicated. `entries` maps
/// `lineId -> locale -> text`; both levels are [`BTreeMap`]s, so serialization
/// is key-sorted and byte-stable by construction.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocaleBundle {
    /// Every declared locale tag, sorted and deduplicated.
    pub locales: Vec<String>,
    /// `lineId -> (locale -> translated text)`.
    pub entries: BTreeMap<String, BTreeMap<String, String>>,
}

/// The wire shape, as a `Serialize` struct rather than a `serde_json::Value`
/// so the object key order is the DECLARATION order (`schemaVersion`,
/// `locales`, `entries`) instead of `serde_json`'s alphabetical `Map` order.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BundleWire<'a> {
    schema_version: i64,
    locales: &'a [String],
    entries: &'a BTreeMap<String, BTreeMap<String, String>>,
}

impl LocaleBundle {
    /// Build from `(lineId, locale, text)` triples, sorting/deduplicating the
    /// locale axis. A later triple for the same `(lineId, locale)` overwrites
    /// an earlier one — callers that must reject a duplicate (`lute loc
    /// import`'s `E-LOCALE-BUNDLE`) detect it BEFORE calling this.
    pub fn from_triples(triples: impl IntoIterator<Item = (String, String, String)>) -> Self {
        let mut entries: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        let mut locales: Vec<String> = Vec::new();
        for (line_id, locale, text) in triples {
            if !locales.contains(&locale) {
                locales.push(locale.clone());
            }
            entries.entry(line_id).or_default().insert(locale, text);
        }
        locales.sort();
        Self { locales, entries }
    }

    /// Serialize to the canonical bundle JSON — pretty-printed and newline
    /// terminated, matching every other artifact this toolchain writes.
    /// Deterministic: both maps are [`BTreeMap`]s and `locales` is sorted, so
    /// the same bundle always renders the same bytes.
    pub fn to_json(&self) -> String {
        let mut s = serde_json::to_string_pretty(&BundleWire {
            schema_version: BUNDLE_SCHEMA_VERSION,
            locales: &self.locales,
            entries: &self.entries,
        })
        .expect("BundleWire -> JSON serialization is infallible (String/BTreeMap only)");
        s.push('\n');
        s
    }

    /// Parse canonical bundle JSON. `Err` carries a human message naming the
    /// exact defect; the CLI wraps it as `E-LOCALE-BUNDLE`. Strict on purpose —
    /// a bundle is tool-produced, so a shape it did not produce is a mistake
    /// worth naming, never something to guess past.
    pub fn parse(text: &str) -> Result<Self, String> {
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|e| format!("not valid JSON: {e}"))?;
        let obj = value
            .as_object()
            .ok_or_else(|| "root must be an object".to_string())?;

        match obj.get("schemaVersion").and_then(serde_json::Value::as_i64) {
            Some(BUNDLE_SCHEMA_VERSION) => {}
            Some(other) => {
                return Err(format!(
                    "unsupported `schemaVersion` {other} (this toolchain reads {BUNDLE_SCHEMA_VERSION})"
                ))
            }
            None => return Err("missing or non-integer `schemaVersion`".to_string()),
        }

        let mut locales = Vec::new();
        for (i, l) in obj
            .get("locales")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| "missing or non-array `locales`".to_string())?
            .iter()
            .enumerate()
        {
            let tag = l
                .as_str()
                .ok_or_else(|| format!("`locales[{i}]` is not a string"))?;
            if tag.is_empty() {
                return Err(format!("`locales[{i}]` is an empty locale tag"));
            }
            locales.push(tag.to_string());
        }
        locales.sort();
        locales.dedup();

        let mut entries: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for (line_id, per_locale) in obj
            .get("entries")
            .and_then(serde_json::Value::as_object)
            .ok_or_else(|| "missing or non-object `entries`".to_string())?
        {
            let map = per_locale
                .as_object()
                .ok_or_else(|| format!("`entries[{line_id}]` is not an object"))?;
            let mut texts = BTreeMap::new();
            for (locale, text) in map {
                if locale.is_empty() {
                    return Err(format!("`entries[{line_id}]` has an empty locale tag"));
                }
                let text = text
                    .as_str()
                    .ok_or_else(|| format!("`entries[{line_id}][{locale}]` is not a string"))?;
                texts.insert(locale.clone(), text.to_string());
            }
            entries.insert(line_id.clone(), texts);
        }

        Ok(Self { locales, entries })
    }
}

/// Merge `bundle` into `artifact`, returning one [`W_L10N_MISSING`] warning per
/// `(lineId, locale)` pair the document needs and the bundle does not carry.
///
/// Runs over the FINISHED artifact, i.e. strictly downstream of
/// [`crate::address::assign_addresses`] — every `lineId` is final. Iteration is
/// command order (= `addr` order) then declared-locale order, so the diagnostic
/// stream is deterministic.
///
/// The record's own `text`/`label` is left untouched; only the additive
/// `texts`/`labels` maps are written. Never panics.
pub fn merge_locales(artifact: &mut Artifact, bundle: &LocaleBundle) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for cmd in &mut artifact.commands {
        match cmd {
            Command::Line(l) => bind(&l.line_id, &mut l.texts, bundle, &mut diags),
            Command::Choice(c) => {
                for o in &mut c.options {
                    bind(&o.line_id, &mut o.labels, bundle, &mut diags);
                }
            }
            Command::Hub(h) => {
                for o in &mut h.options {
                    bind(&o.line_id, &mut o.labels, bundle, &mut diags);
                }
            }
            _ => {}
        }
    }
    diags
}

/// Fill ONE translatable record's locale map from the bundle and report its
/// gaps. Shared by the line and option arms so the two can never diverge on
/// either the merge rule or the diagnostic wording.
fn bind(
    line_id: &str,
    target: &mut BTreeMap<String, String>,
    bundle: &LocaleBundle,
    diags: &mut Vec<Diagnostic>,
) {
    let entry = bundle.entries.get(line_id);
    // An entry MAY carry a locale outside `bundle.locales` (a hand-written
    // bundle); that text is still real data, so it merges. Only the
    // completeness CHECK is scoped to the declared axis.
    if let Some(texts) = entry {
        for (locale, text) in texts {
            target.insert(locale.clone(), text.clone());
        }
    }
    for locale in &bundle.locales {
        if entry.is_none_or(|t| !t.contains_key(locale)) {
            diags.push(missing_diag(locale, line_id));
        }
    }
}

/// One [`W_L10N_MISSING`] warning. The artifact carries no source spans (the
/// merge runs on lowered records, not the AST), so the span is the document
/// origin — the same total, never-panicking treatment
/// [`crate::address`]'s own `E-COMPILE-INTERNAL` uses.
fn missing_diag(locale: &str, line_id: &str) -> Diagnostic {
    Diagnostic {
        code: W_L10N_MISSING.to_string(),
        severity: Severity::Warning,
        message: format!("no `{locale}` text for `{line_id}`"),
        span: Span {
            byte_start: 0,
            byte_end: 0,
            line: 1,
            column: 1,
            utf16_range: (0, 0),
        },
        layer: Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bundle() -> LocaleBundle {
        LocaleBundle::from_triples([
            ("x.a_0010".to_string(), "ja-JP".to_string(), "こんにちは".to_string()),
            ("x.a_0010".to_string(), "en-US".to_string(), "hello".to_string()),
            ("x.b.pick".to_string(), "ja-JP".to_string(), "選ぶ".to_string()),
        ])
    }

    #[test]
    fn from_triples_sorts_and_dedupes_the_locale_axis() {
        let b = bundle();
        assert_eq!(b.locales, vec!["en-US".to_string(), "ja-JP".to_string()]);
        assert_eq!(b.entries.len(), 2);
        assert_eq!(b.entries["x.a_0010"]["ja-JP"], "こんにちは");
    }

    #[test]
    fn json_round_trips_byte_identically() {
        let b = bundle();
        let json = b.to_json();
        let back = LocaleBundle::parse(&json).expect("canonical bundle re-parses");
        assert_eq!(back, b);
        assert_eq!(back.to_json(), json, "round trip must be byte-stable");
        // Declaration order, not alphabetical: `schemaVersion` first.
        assert!(json.starts_with("{\n  \"schemaVersion\": 1,\n  \"locales\": ["), "{json}");
        assert!(json.ends_with("\n"));
    }

    #[test]
    fn parse_rejects_a_malformed_bundle() {
        assert!(LocaleBundle::parse("not json").is_err());
        assert!(LocaleBundle::parse("[]").is_err());
        assert!(LocaleBundle::parse(r#"{"locales":[],"entries":{}}"#).is_err());
        assert!(
            LocaleBundle::parse(r#"{"schemaVersion":2,"locales":[],"entries":{}}"#).is_err(),
            "an unknown schemaVersion is refused, never guessed at"
        );
        assert!(
            LocaleBundle::parse(r#"{"schemaVersion":1,"locales":[""],"entries":{}}"#).is_err(),
            "an empty locale tag is a defect"
        );
        assert!(
            LocaleBundle::parse(r#"{"schemaVersion":1,"locales":["ja"],"entries":{"a":{"ja":7}}}"#)
                .is_err(),
            "a non-string translation is a defect"
        );
    }
}
