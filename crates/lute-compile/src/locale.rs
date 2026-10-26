//! Locale bundle (dsl 0.8.0 §7) — the canonical `lineId -> locale -> text`
//! table `lute loc import` produces and `lute compile --locale-bundle` merges into an
//! already-addressed [`ExecutionIr`].
//!
//! ## Why the merge lives downstream of addressing
//! `lineId` is assigned by [`crate::address::assign_addresses`], the LAST pass
//! [`crate::compile_with_check`] runs before it builds the [`ExecutionIr`]. A
//! bundle is keyed on that identity (never on `addr`, which is a REGENERATED
//! position — spec §4.2/§12), so the merge can only run once the artifact
//! exists. [`merge_locales`] therefore takes a finished [`ExecutionIr`] rather
//! than threading a bundle through the pipeline: the compile signature, and
//! every byte a bundle-less compile emits, stay exactly as they were.
//!
//! ## What the merge does and does not touch
//! - `LineCmd.texts`, `ChoiceOption.texts`, `HubOption.texts`, (dsl 0.47.0
//!   §3.1) a prompted branch/hub's `promptTexts` and (0.51.0 §5.5) each
//!   `PluginCmd.localized.<attr>.texts` of a directive's `text` attribute are
//!   filled from the bundle entry matching that record's own `lineId` /
//!   `promptLineId` / `localized.<attr>.lineId`. All are
//!   `skip_serializing_if = "BTreeMap::is_empty"`, so a document with no
//!   matching entry is byte-identical to 0.7.0.
//! - `LineCmd.text` / the option `label` / a plugin's `fields.<attr>` are
//!   NEVER overwritten. They remain the
//!   SOURCE-language string (`contentLang`, dsl 0.8.0 §7), so a 0.7 consumer
//!   that ignores the new maps reads exactly what it read before.
//! - A bundle entry whose `lineId` matches no record in THIS document is
//!   silently ignored — one bundle legitimately spans a whole project.
//! - A record missing a locale the bundle DECLARES is [`W_L10N_MISSING`], one
//!   diagnostic per `(lineId, locale)` pair. A warning, so
//!   `--deny W-L10N-MISSING` promotes it in CI.
//! - dsl 0.37.0 §6: a line translation runs the source's inline grammar.
//!   Its plain derivation always lands in `texts`; a line whose source
//!   carries a modifier also gets `localeSegments`. A translation whose
//!   modifier multiset differs from the source's is [`E_L10N_MODIFIERS`]
//!   and is not merged.
//! - dsl 0.47.0 §3.2: a merged `(lineId, locale)` whose stored `sourceHashes`
//!   value differs from [`source_hash`] of the record's current plain source
//!   text is [`W_L10N_STALE`]. The translation is still merged.

use std::collections::BTreeMap;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use serde::Serialize;

use crate::ir::{Command, ExecutionIr, LineCmd};

/// A merged translation was made from a different source text than the
/// record's current one (dsl 0.47.0 §3.2): the source changed after the
/// export the translator worked from. Warning-grade; the text still merges.
pub const W_L10N_STALE: &str = "W-L10N-STALE";

/// A translatable record carries no text for a locale the bundle declares
/// (dsl 0.8.0 §7). Warning-grade: an untranslated build is still a valid
/// build, and CI opts into strictness with `--deny W-L10N-MISSING`.
pub const W_L10N_MISSING: &str = "W-L10N-MISSING";

/// A line translation whose inline modifier multiset (names, leaf/span form,
/// normalized attrs) differs from its source line's, or whose markup does not
/// parse (dsl 0.37.0 §6). Error-grade: the translation is not merged.
pub const E_L10N_MODIFIERS: &str = "E-L10N-MODIFIERS";

/// The `schemaVersion` [`LocaleBundle::to_json`] writes (dsl 0.47.0 §3.2:
/// `sourceHashes` joined the shape). [`LocaleBundle::parse`] also reads
/// [`LEGACY_BUNDLE_SCHEMA_VERSION`]; a bundle is a tool-produced artifact, so
/// any other version is rejected rather than guessed at.
pub const BUNDLE_SCHEMA_VERSION: i64 = 2;

/// The 0.8.0 bundle shape: no `sourceHashes`, so no stale check.
pub const LEGACY_BUNDLE_SCHEMA_VERSION: i64 = 1;

/// The `sourceHash` of a record's plain source text (dsl 0.47.0 §3.2):
/// `"sha256:<lowercase hex>"` over its UTF-8 bytes. `lute loc export` and the
/// stale check both call this, so the two can never disagree on the digest.
pub fn source_hash(text: &str) -> String {
    format!(
        "sha256:{}",
        lute_manifest::snapshot::sha256_hex(text.as_bytes())
    )
}

/// The canonical locale bundle (dsl 0.8.0 §7).
///
/// `locales` is the DECLARED locale set — the axis the [`W_L10N_MISSING`]
/// completeness check runs over — sorted and deduplicated. `entries` maps
/// `lineId -> locale -> text`; `source_hashes` maps `lineId -> locale ->
/// sourceHash` (dsl 0.47.0 §3.2) for the pairs whose import row carried one.
/// Every level is a [`BTreeMap`], so serialization is key-sorted and
/// byte-stable by construction.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocaleBundle {
    /// Every declared locale tag, sorted and deduplicated.
    pub locales: Vec<String>,
    /// `lineId -> (locale -> translated text)`.
    pub entries: BTreeMap<String, BTreeMap<String, String>>,
    /// `lineId -> (locale -> "sha256:…")`: the source text each translation
    /// was made from. Empty for a `schemaVersion: 1` bundle.
    pub source_hashes: BTreeMap<String, BTreeMap<String, String>>,
}

/// The wire shape, as a `Serialize` struct rather than a `serde_json::Value`
/// so the object key order is the DECLARATION order (`schemaVersion`,
/// `locales`, `entries`, `sourceHashes`) instead of `serde_json`'s
/// alphabetical `Map` order.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BundleWire<'a> {
    schema_version: i64,
    locales: &'a [String],
    entries: &'a BTreeMap<String, BTreeMap<String, String>>,
    source_hashes: &'a BTreeMap<String, BTreeMap<String, String>>,
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
        Self {
            locales,
            entries,
            source_hashes: BTreeMap::new(),
        }
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
            source_hashes: &self.source_hashes,
        })
        .expect("BundleWire -> JSON serialization is infallible (String/BTreeMap only)");
        s.push('\n');
        s
    }

    /// Parse canonical bundle JSON. Strict on purpose — a bundle is
    /// tool-produced, so a shape it did not produce is a mistake worth naming,
    /// never something to guess past.
    ///
    /// # Errors
    ///
    /// A [`LocaleBundleError`] naming the exact defect; the CLI wraps its text
    /// as `E-LOCALE-BUNDLE`.
    pub fn parse(text: &str) -> Result<Self, LocaleBundleError> {
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(LocaleBundleError::Json)?;
        let obj = value.as_object().ok_or(LocaleBundleError::RootNotObject)?;

        let version = match obj.get("schemaVersion").and_then(serde_json::Value::as_i64) {
            Some(v @ (BUNDLE_SCHEMA_VERSION | LEGACY_BUNDLE_SCHEMA_VERSION)) => v,
            Some(other) => return Err(LocaleBundleError::UnsupportedVersion(other)),
            None => return Err(LocaleBundleError::MissingVersion),
        };

        let mut locales = Vec::new();
        for (index, l) in obj
            .get("locales")
            .and_then(serde_json::Value::as_array)
            .ok_or(LocaleBundleError::MissingLocales)?
            .iter()
            .enumerate()
        {
            let tag = l
                .as_str()
                .ok_or(LocaleBundleError::LocaleNotString { index })?;
            if tag.is_empty() {
                return Err(LocaleBundleError::EmptyLocale { index });
            }
            locales.push(tag.to_string());
        }
        locales.sort();
        locales.dedup();

        let entries = string_table(obj, "entries")?;
        // A v1 bundle predates `sourceHashes`: whatever it carries, it has no
        // stale check. A v2 bundle always writes the key.
        let source_hashes = if version == BUNDLE_SCHEMA_VERSION {
            string_table(obj, "sourceHashes")?
        } else {
            BTreeMap::new()
        };

        Ok(Self {
            locales,
            entries,
            source_hashes,
        })
    }
}

/// Why [`LocaleBundle::parse`] refused a bundle. `table` is `entries` or
/// `sourceHashes`.
#[derive(Debug)]
pub enum LocaleBundleError {
    /// The text is not JSON.
    Json(serde_json::Error),
    RootNotObject,
    /// A `schemaVersion` this toolchain does not read.
    UnsupportedVersion(i64),
    MissingVersion,
    MissingLocales,
    LocaleNotString {
        index: usize,
    },
    EmptyLocale {
        index: usize,
    },
    MissingTable {
        table: &'static str,
    },
    RowNotObject {
        table: &'static str,
        line_id: String,
    },
    EmptyCellLocale {
        table: &'static str,
        line_id: String,
    },
    CellNotString {
        table: &'static str,
        line_id: String,
        locale: String,
    },
}

impl std::fmt::Display for LocaleBundleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LocaleBundleError::Json(e) => write!(f, "not valid JSON: {e}"),
            LocaleBundleError::RootNotObject => f.write_str("root must be an object"),
            LocaleBundleError::UnsupportedVersion(other) => write!(
                f,
                "unsupported `schemaVersion` {other} (this toolchain reads \
                 {LEGACY_BUNDLE_SCHEMA_VERSION} and {BUNDLE_SCHEMA_VERSION})"
            ),
            LocaleBundleError::MissingVersion => {
                f.write_str("missing or non-integer `schemaVersion`")
            }
            LocaleBundleError::MissingLocales => f.write_str("missing or non-array `locales`"),
            LocaleBundleError::LocaleNotString { index } => {
                write!(f, "`locales[{index}]` is not a string")
            }
            LocaleBundleError::EmptyLocale { index } => {
                write!(f, "`locales[{index}]` is an empty locale tag")
            }
            LocaleBundleError::MissingTable { table } => {
                write!(f, "missing or non-object `{table}`")
            }
            LocaleBundleError::RowNotObject { table, line_id } => {
                write!(f, "`{table}[{line_id}]` is not an object")
            }
            LocaleBundleError::EmptyCellLocale { table, line_id } => {
                write!(f, "`{table}[{line_id}]` has an empty locale tag")
            }
            LocaleBundleError::CellNotString {
                table,
                line_id,
                locale,
            } => write!(f, "`{table}[{line_id}][{locale}]` is not a string"),
        }
    }
}

impl std::error::Error for LocaleBundleError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            LocaleBundleError::Json(e) => Some(e),
            _ => None,
        }
    }
}

/// One `lineId -> locale -> string` table of a bundle (`entries`,
/// `sourceHashes`), with the defect named exactly.
fn string_table(
    obj: &serde_json::Map<String, serde_json::Value>,
    table: &'static str,
) -> Result<BTreeMap<String, BTreeMap<String, String>>, LocaleBundleError> {
    let mut out: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
    for (line_id, per_locale) in obj
        .get(table)
        .and_then(serde_json::Value::as_object)
        .ok_or(LocaleBundleError::MissingTable { table })?
    {
        let map = per_locale
            .as_object()
            .ok_or_else(|| LocaleBundleError::RowNotObject {
                table,
                line_id: line_id.clone(),
            })?;
        let mut values = BTreeMap::new();
        for (locale, value) in map {
            if locale.is_empty() {
                return Err(LocaleBundleError::EmptyCellLocale {
                    table,
                    line_id: line_id.clone(),
                });
            }
            let value = value
                .as_str()
                .ok_or_else(|| LocaleBundleError::CellNotString {
                    table,
                    line_id: line_id.clone(),
                    locale: locale.clone(),
                })?;
            values.insert(locale.clone(), value.to_string());
        }
        out.insert(line_id.clone(), values);
    }
    Ok(out)
}

/// Merge `bundle` into `artifact`, returning one [`W_L10N_MISSING`] warning per
/// `(lineId, locale)` pair the document needs and the bundle does not carry,
/// one [`W_L10N_STALE`] warning per merged pair translated from a different
/// source text, and one [`E_L10N_MODIFIERS`] error per refused line
/// translation.
///
/// Runs over the FINISHED execution IR, i.e. strictly downstream of
/// [`crate::address::assign_addresses`] — every `lineId` is final. Iteration is
/// command order (= `position` order) then declared-locale order, so the diagnostic
/// stream is deterministic.
///
/// The record's own `text` is left untouched; only the additive
/// `texts` / `localeSegments` maps are written. Never panics.
pub fn merge_locales(artifact: &mut ExecutionIr, bundle: &LocaleBundle) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for cmd in &mut artifact.commands {
        match cmd {
            Command::Line(l) => bind_line(l, bundle, &mut diags),
            // dsl 0.47.0 §3.1: the prompt first — the export's row order.
            Command::Choice(c) => {
                if let (Some(id), Some(prompt)) = (&c.prompt_line_id, &c.prompt) {
                    bind(id, prompt, &mut c.prompt_texts, bundle, &mut diags);
                }
                for o in &mut c.options {
                    bind(&o.line_id, &o.text, &mut o.texts, bundle, &mut diags);
                }
            }
            Command::Hub(h) => {
                if let (Some(id), Some(prompt)) = (&h.prompt_line_id, &h.prompt) {
                    bind(id, prompt, &mut h.prompt_texts, bundle, &mut diags);
                }
                for o in &mut h.options {
                    bind(&o.line_id, &o.text, &mut o.texts, bundle, &mut diags);
                }
            }
            // 0.51.0 §5.5: each `text` attribute against its source
            // `fields.<attr>` (no inline markup: an option label's rule).
            Command::Plugin(p) => {
                for (attr, text) in &mut p.localized {
                    let source = p
                        .fields
                        .get(attr)
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default();
                    bind(&text.line_id, source, &mut text.texts, bundle, &mut diags);
                }
            }
            _ => {}
        }
    }
    diags
}

/// Fill ONE option label's or prompt's locale map from the bundle and report
/// its gaps and stale translations. `source` is the record's plain source
/// text (the stale check's input). Shares [`missing_diag`] and
/// [`StaleCheck`] with [`bind_line`], so the arms can never diverge on the
/// diagnostic wording.
fn bind(
    line_id: &str,
    source: &str,
    target: &mut BTreeMap<String, String>,
    bundle: &LocaleBundle,
    diags: &mut Vec<Diagnostic>,
) {
    let entry = bundle.entries.get(line_id);
    let mut stale = StaleCheck::new(bundle, line_id, source);
    // An entry MAY carry a locale outside `bundle.locales` (a hand-written
    // bundle); that text is still real data, so it merges. Only the
    // completeness CHECK is scoped to the declared axis.
    if let Some(texts) = entry {
        for (locale, text) in texts {
            target.insert(locale.clone(), text.clone());
            stale.check(locale, diags);
        }
    }
    for locale in &bundle.locales {
        if entry.is_none_or(|t| !t.contains_key(locale)) {
            diags.push(missing_diag(locale, line_id));
        }
    }
}

/// A line's translations (dsl 0.37.0 §6): each runs the inline grammar; one
/// whose modifier multiset matches the source's merges its plain derivation
/// into `texts` (and, for a modified source line, its segments into
/// `localeSegments`), any other is an [`E_L10N_MODIFIERS`] error. The
/// completeness check is [`bind`]'s.
fn bind_line(l: &mut LineCmd, bundle: &LocaleBundle, diags: &mut Vec<Diagnostic>) {
    let entry = bundle.entries.get(&l.line_id);
    let mut stale = StaleCheck::new(bundle, &l.line_id, &l.text);
    for (locale, text) in entry.into_iter().flatten() {
        let (nodes, parse_diags) = lute_syntax::parse_inline_text(text);
        if let Some(bad) = parse_diags.iter().find(|d| d.severity == Severity::Error) {
            let why = format!(
                "its inline markup does not parse ({}: {})",
                bad.code, bad.message
            );
            diags.push(modifiers_diag(locale, &l.line_id, &why));
            continue;
        }
        let modifiers = lute_syntax::ast::inline_modifier_multiset(&nodes);
        if modifiers != l.modifiers {
            let why = format!(
                "its inline modifiers {} differ from the source's {}",
                render_multiset(&modifiers),
                render_multiset(&l.modifiers)
            );
            diags.push(modifiers_diag(locale, &l.line_id, &why));
            continue;
        }
        l.texts
            .insert(locale.clone(), lute_syntax::ast::inline_plain_text(&nodes));
        if !l.modifiers.is_empty() {
            l.locale_segments
                .insert(locale.clone(), crate::lower::inline_segments(&nodes));
        }
        stale.check(locale, diags);
    }
    for locale in &bundle.locales {
        if entry.is_none_or(|t| !t.contains_key(locale)) {
            diags.push(missing_diag(locale, &l.line_id));
        }
    }
}

/// The [`W_L10N_STALE`] check for one record: the stored hashes of its
/// `lineId` and its current source hash, computed only when a stored hash
/// exists to compare against.
struct StaleCheck<'a> {
    line_id: &'a str,
    source: &'a str,
    stored: Option<&'a BTreeMap<String, String>>,
    current: Option<String>,
}

impl<'a> StaleCheck<'a> {
    fn new(bundle: &'a LocaleBundle, line_id: &'a str, source: &'a str) -> Self {
        Self {
            line_id,
            source,
            stored: bundle.source_hashes.get(line_id),
            current: None,
        }
    }

    /// Report a merged `locale` translation whose stored hash differs from
    /// the current source's. No stored hash, no check.
    fn check(&mut self, locale: &str, diags: &mut Vec<Diagnostic>) {
        let Some(stored) = self.stored.and_then(|m| m.get(locale)) else {
            return;
        };
        let source = self.source;
        let current = self.current.get_or_insert_with(|| source_hash(source));
        if stored != current {
            diags.push(stale_diag(locale, self.line_id));
        }
    }
}

/// A modifier multiset as `[pause{s=0.5}, emphasis×2]` (`[]` when empty).
fn render_multiset(set: &BTreeMap<String, usize>) -> String {
    let items: Vec<String> = set
        .iter()
        .map(|(k, n)| {
            if *n == 1 {
                k.clone()
            } else {
                format!("{k}×{n}")
            }
        })
        .collect();
    format!("[{}]", items.join(", "))
}

fn modifiers_diag(locale: &str, line_id: &str, why: &str) -> Diagnostic {
    Diagnostic {
        code: E_L10N_MODIFIERS.to_string(),
        severity: Severity::Error,
        message: format!("`{locale}` text for `{line_id}` is not merged: {why}"),
        ..missing_diag(locale, line_id)
    }
}

/// One [`W_L10N_STALE`] warning (dsl 0.47.0 §3.2).
fn stale_diag(locale: &str, line_id: &str) -> Diagnostic {
    Diagnostic {
        code: W_L10N_STALE.to_string(),
        message: format!(
            "`{locale}` text for `{line_id}` was translated from an older source text; \
             re-export with `lute loc export` and update the translation"
        ),
        ..missing_diag(locale, line_id)
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
        evidence: None,
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
            (
                "x.a_0010".to_string(),
                "ja-JP".to_string(),
                "こんにちは".to_string(),
            ),
            (
                "x.a_0010".to_string(),
                "en-US".to_string(),
                "hello".to_string(),
            ),
            (
                "x.b.pick".to_string(),
                "ja-JP".to_string(),
                "選ぶ".to_string(),
            ),
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
        assert!(
            json.starts_with("{\n  \"schemaVersion\": 2,\n  \"locales\": ["),
            "{json}"
        );
        assert!(json.ends_with("\n"));
    }

    #[test]
    fn parse_rejects_a_malformed_bundle() {
        LocaleBundle::parse("not json").unwrap_err();
        LocaleBundle::parse("[]").unwrap_err();
        LocaleBundle::parse(r#"{"locales":[],"entries":{}}"#).unwrap_err();
        assert!(
            LocaleBundle::parse(r#"{"schemaVersion":3,"locales":[],"entries":{}}"#).is_err(),
            "an unknown schemaVersion is refused, never guessed at"
        );
        assert!(
            LocaleBundle::parse(r#"{"schemaVersion":2,"locales":[],"entries":{}}"#).is_err(),
            "a v2 bundle always carries `sourceHashes`"
        );
        assert!(
            LocaleBundle::parse(
                r#"{"schemaVersion":2,"locales":[],"entries":{},"sourceHashes":{"a":{"ja":1}}}"#
            )
            .is_err(),
            "a non-string hash is a defect"
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

    /// 0.49.1: the typed refusal renders the 0.49.0 `E-LOCALE-BUNDLE` text,
    /// and a JSON refusal keeps the parser's error as its `source()`.
    #[test]
    fn parse_error_text_is_the_0_49_0_message() {
        let text = |json: &str| LocaleBundle::parse(json).unwrap_err().to_string();
        let not_json = LocaleBundle::parse("not json").unwrap_err();
        assert!(not_json.to_string().starts_with("not valid JSON: "));
        assert!(std::error::Error::source(&not_json).is_some());
        assert_eq!(text("[]"), "root must be an object");
        assert_eq!(
            text(r#"{"locales":[]}"#),
            "missing or non-integer `schemaVersion`"
        );
        assert_eq!(
            text(r#"{"schemaVersion":3}"#),
            "unsupported `schemaVersion` 3 (this toolchain reads 1 and 2)"
        );
        assert_eq!(
            text(r#"{"schemaVersion":1}"#),
            "missing or non-array `locales`"
        );
        assert_eq!(
            text(r#"{"schemaVersion":1,"locales":[1]}"#),
            "`locales[0]` is not a string"
        );
        assert_eq!(
            text(r#"{"schemaVersion":1,"locales":["ja",""]}"#),
            "`locales[1]` is an empty locale tag"
        );
        assert_eq!(
            text(r#"{"schemaVersion":2,"locales":[],"entries":{}}"#),
            "missing or non-object `sourceHashes`"
        );
        assert_eq!(
            text(r#"{"schemaVersion":1,"locales":[],"entries":{"a":1}}"#),
            "`entries[a]` is not an object"
        );
        assert_eq!(
            text(r#"{"schemaVersion":1,"locales":[],"entries":{"a":{"":"x"}}}"#),
            "`entries[a]` has an empty locale tag"
        );
        assert_eq!(
            text(r#"{"schemaVersion":1,"locales":[],"entries":{"a":{"ja":7}}}"#),
            "`entries[a][ja]` is not a string"
        );
    }

    /// dsl 0.47.0 §3.2: v2 writes `sourceHashes` last, always (`{}` when
    /// empty); a v1 bundle still reads, with no hashes.
    #[test]
    fn v2_writes_source_hashes_and_v1_still_reads() {
        let mut b = bundle();
        let empty = b.to_json();
        assert!(
            empty.ends_with("  },\n  \"sourceHashes\": {}\n}\n"),
            "{empty}"
        );
        b.source_hashes
            .entry("x.b.pick".to_string())
            .or_default()
            .insert("ja-JP".to_string(), source_hash("Pick"));
        let json = b.to_json();
        let back = LocaleBundle::parse(&json).expect("v2 re-parses");
        assert_eq!(back, b);
        assert_eq!(back.to_json(), json);
        let v1 = LocaleBundle::parse(
            r#"{"schemaVersion":1,"locales":["ja-JP"],"entries":{"x.b.pick":{"ja-JP":"選ぶ"}}}"#,
        )
        .expect("a v1 bundle is still read");
        assert!(v1.source_hashes.is_empty());
        assert_eq!(v1.entries["x.b.pick"]["ja-JP"], "選ぶ");
    }

    #[test]
    fn source_hash_is_prefixed_lowercase_sha256() {
        assert_eq!(
            source_hash(""),
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
