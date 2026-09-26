//! dsl 0.19.0 lore entries (§3, §5): `<entry>` attribute shape, per-document
//! identity (`E-ENTRY-ID-DUP`, `E-ENTRY-SERIES-ORDER`), and the reserved
//! `entry.<id>.read` decl fold.
//!
//! An entry is the lore mirror of a `<quest>`: [`check_entries`] runs in
//! `check::fold_env` beside `match_check::check_quest`, contributing its
//! diagnostics to the fold stream and its implicit reserved decls to the
//! schema — so a document's OWN entries type `entry.<id>.read` as `bool`
//! (default `false`) through the ordinary schema lookup. A read of an entry
//! another document declares is admitted by shape instead
//! ([`crate::cel_paths::is_reserved_entry_read`]), exactly as a foreign
//! `quest.<id>.state` is; `check-project` resolves the id
//! (`W-ENTRY-REF-UNKNOWN`, [`crate::project_check`]).

use std::collections::{BTreeMap, BTreeSet};

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_manifest::types::{Literal, Type};
use lute_syntax::ast::{AttrValue, Entry, Meta};

use crate::cel_paths::E_PATH_IDENT;
use crate::meta::{Namespace, StateDecl};

/// `<entry>` attribute shape (§3): missing/non-ident `id`, malformed
/// `target`, non-ident `category`/`series`, `order` that is not a
/// non-negative integer, `order` without `series`, or a non-string value for
/// any string attribute. Anchored at the attribute.
pub const E_ENTRY_ATTR: &str = "E-ENTRY-ATTR";
/// Two entries with the same `id` (§3) — per document in `lute check`,
/// project-wide in `check-project`.
pub const E_ENTRY_ID_DUP: &str = "E-ENTRY-ID-DUP";
/// Two entries with the same `(series, order)` (§3).
pub const E_ENTRY_SERIES_ORDER: &str = "E-ENTRY-SERIES-ORDER";
/// `entry.<id>.read` names an id no document in the project declares (§5,
/// `check-project` only).
pub const W_ENTRY_REF_UNKNOWN: &str = "W-ENTRY-REF-UNKNOWN";
/// dsl 0.26.0 §8 (T3-4): a repeatable entry beat — it answers an occasion
/// (`on=`) and has no `once`, so every raise may present it again in the
/// same run — whose body writes state (`::set`) or removes a fact
/// (`::retract`). Effects apply on the first read in a run only (dsl 0.19.0
/// §6), so a write meant to repeat is silently skipped. An `::assert` is
/// exempt: the fact it records holds for the rest of the run either way, so
/// asserting it once is the same as asserting it on every read. A lookup
/// entry (no `on=`) cannot take `once`, and applying its effects on the
/// first read is its design (0.19.0 D-C): no warning. Anchored at the first
/// such write.
pub const W_ENTRY_WRITE_REREAD: &str = "W-ENTRY-WRITE-REREAD";

/// What [`check_entries`] folds into the enclosing document: the reserved
/// `entry.<id>.read` decls (dsl 0.19.0 §5) plus every attribute/identity
/// diagnostic.
#[derive(Clone, Debug, Default)]
pub struct EntryRecord {
    pub decls: Vec<(String, StateDecl)>,
    pub diags: Vec<Diagnostic>,
}

/// The reserved read-flag path of entry `id` (dsl 0.19.0 §5).
pub fn entry_read_path(id: &str) -> String {
    format!("entry.{id}.read")
}

/// The reserved `entry.<id>.read` decl: `bool`, default `false`, run-tier
/// lifetime (dsl 0.19.0 §5 "resets with the run tier") — never maybe-unset.
pub fn entry_read_decl() -> StateDecl {
    StateDecl {
        ty: Type::Bool,
        default: Some(Literal::Bool(false)),
        namespace: Namespace::Run,
        owner: None,
    }
}

/// `order` (dsl 0.19.0 §3): a non-negative integer, `[0-9]+`, that fits the
/// IR's integer field. `None` for anything else (`E-ENTRY-ATTR`).
pub fn parse_entry_order(raw: &str) -> Option<u32> {
    if raw.is_empty() || !raw.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    raw.parse().ok()
}

/// `Ident ::= [A-Za-z] [A-Za-z0-9_-]*` (dsl 0.1.0 §4.4) — the `id`,
/// `category`, and `series` shape.
pub fn is_entry_ident(s: &str) -> bool {
    let mut bytes = s.bytes();
    matches!(bytes.next(), Some(b) if b.is_ascii_alphabetic())
        && bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// `target ::= Ident ("." Segment)*`, `Segment ::= [A-Za-z0-9_-]+` (dsl
/// 0.19.0 §3) — shape-only, never checked against a vocabulary.
pub fn is_entry_target(s: &str) -> bool {
    let mut segs = s.split('.');
    segs.next().is_some_and(is_entry_ident)
        && segs.all(|seg| {
            !seg.is_empty()
                && seg
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
        })
}

/// dsl 0.26.0 §5: `kind:<Ident>` — a beat or entry that answers its occasion
/// for every member of an entity kind. The kind name, when `s` is one.
pub fn kind_target(s: &str) -> Option<&str> {
    s.strip_prefix("kind:").filter(|k| is_entry_ident(k))
}

/// A beat's (scene, bundle beat, entry beat) `target`: a dotted id or a
/// `kind:<Ident>` (dsl 0.26.0 §5) — shape-only.
pub fn is_beat_target(s: &str) -> bool {
    is_entry_target(s) || kind_target(s).is_some()
}

/// One entry's RESOLVED series position (dsl 0.19.0 §2.1, §3, §7): the
/// `series` / `order` every consumer — the checker's `E-ENTRY-SERIES-ORDER`
/// (per document and project-wide), the compiled `entry` record, the
/// `ProjectIndex.entries` row, and `lute lore` — reads, whichever form the
/// author used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EntrySeries<'a> {
    /// The document-level `series:` when the document declares one; else the
    /// entry's own `series=` value verbatim (shape unchecked — a non-ident
    /// value is that entry's `E-ENTRY-ATTR`).
    pub series: Option<&'a str>,
    /// The entry's 1-based position among the document's entries under a
    /// document-level `series:`; else its own `order=` when that is a
    /// non-negative integer ([`parse_entry_order`]).
    pub order: Option<u32>,
    /// Where a duplicate-position diagnostic anchors: the entry's `order=`
    /// value when the position is attribute-declared, else the entry's `id`.
    pub anchor: Span,
}

impl<'a> EntrySeries<'a> {
    /// The well-formed `(series, order)` position — an `Ident` series and an
    /// order — `E-ENTRY-SERIES-ORDER` groups by. `None` when either is absent
    /// or malformed (that entry's own `E-ENTRY-ATTR`).
    pub fn position(&self) -> Option<(&'a str, u32)> {
        let series = self.series.filter(|s| is_entry_ident(s))?;
        Some((series, self.order?))
    }
}

/// Resolve every entry's `(series, order)` (dsl 0.19.0 §2.1 D-K, §3) — the
/// ONE resolution point. `doc_series` is the document's validated `series:`
/// ([`crate::meta::TypedMeta::series`], or [`document_series`] where no
/// typed frontmatter is at hand): when present, every entry resolves to that
/// series at its 1-based position in `entries`, whatever it authored itself
/// (authoring `series=`/`order=` there is `E-ENTRY-ATTR`, never a
/// precedence rule). Otherwise each entry resolves to its own attributes.
pub fn resolve_entry_series<'a>(
    doc_series: Option<&'a str>,
    entries: &'a [Entry],
) -> Vec<EntrySeries<'a>> {
    entries
        .iter()
        .enumerate()
        .map(|(i, entry)| match doc_series {
            Some(series) => EntrySeries {
                series: Some(series),
                order: u32::try_from(i + 1).ok(),
                anchor: entry.id_span,
            },
            None => EntrySeries {
                series: entry.series.as_ref().map(|(s, _)| s.as_str()),
                order: entry
                    .order
                    .as_ref()
                    .and_then(|(raw, _)| parse_entry_order(raw)),
                anchor: entry.order.as_ref().map_or(entry.id_span, |(_, sp)| *sp),
            },
        })
        .collect()
}

/// A lore document's validated `series:` read straight off its raw
/// frontmatter — for the project-wide passes and reports that hold a parsed
/// [`Document`] but no [`crate::meta::TypedMeta`]. The same predicate as the
/// typed lift (a string `Ident`); `series:` is never defaultable, so the raw
/// mapping is the whole truth.
pub fn document_series(meta: &Meta) -> Option<String> {
    let map: serde_yaml::Mapping = serde_yaml::from_str(&meta.raw_yaml).ok()?;
    map.get(serde_yaml::Value::String("series".to_string()))?
        .as_str()
        .filter(|s| is_entry_ident(s))
        .map(str::to_string)
}

/// Check every `<entry>` of one document (dsl 0.19.0 §3, §5): per-entry
/// attribute shape and closure, per-document `E-ENTRY-ID-DUP` /
/// `E-ENTRY-SERIES-ORDER` over the RESOLVED positions (every occurrence past
/// the first, in document order), and one reserved `entry.<id>.read` decl
/// per entry whose id is a well-formed `Ident` (a missing or malformed id
/// makes the path unaddressable, as a missing quest id does).
///
/// `doc_series` is the document's validated `series:` (dsl 0.19.0 §2.1): an
/// entry that also authors `series=` or `order=` is `E-ENTRY-ATTR`.
///
/// `seen_ids` is the caller's id set, SEEDED with every import-reachable
/// entry id (`SchemaImports::imported_entry_ids`) exactly as `check_quest`'s
/// `seen_quests` is — redeclaring one is `E-ENTRY-ID-DUP` too.
pub fn check_entries(
    doc_series: Option<&str>,
    entries: &[Entry],
    seen_ids: &mut BTreeSet<String>,
) -> EntryRecord {
    let mut record = EntryRecord::default();
    let resolved = resolve_entry_series(doc_series, entries);
    let mut positions: BTreeMap<(&str, u32), &str> = BTreeMap::new();
    for (entry, resolved) in entries.iter().zip(&resolved) {
        check_entry_shape(entry, doc_series, &mut record.diags);
        if entry.on.is_some() && entry.once.is_none() {
            if let Some((what, span)) = first_write(&entry.body) {
                record.diags.push(diag(
                    W_ENTRY_WRITE_REREAD,
                    Severity::Warning,
                    format!(
                        "`<entry id=\"{}\">` has no `once`, so it can be presented again in a \
                         run, but its `{what}` applies on the first read in a run only (dsl \
                         0.19.0 §6); a write meant to repeat belongs in a `<beat once=\"false\">`, \
                         and an entry read once per run says so with `once=\"run\"`",
                        entry.id
                    ),
                    span,
                ));
            }
        }
        let id = entry.id.as_str();
        if !id.is_empty() {
            if !seen_ids.insert(id.to_string()) {
                record.diags.push(diag(
                    E_ENTRY_ID_DUP,
                    Severity::Error,
                    format!(
                        "duplicate `<entry id=\"{id}\">`; entry ids must be unique across the \
                         project (dsl 0.19.0 §3)"
                    ),
                    entry.id_span,
                ));
            }
            if is_entry_ident(id) {
                record.decls.push((entry_read_path(id), entry_read_decl()));
            }
        }
        if let Some((series, order)) = resolved.position() {
            if let Some(first) = positions.get(&(series, order)) {
                record.diags.push(diag(
                    E_ENTRY_SERIES_ORDER,
                    Severity::Error,
                    series_order_message(series, order, first, id),
                    resolved.anchor,
                ));
            } else {
                positions.insert((series, order), id);
            }
        }
    }
    record
}

pub(crate) fn series_order_message(series: &str, order: u32, first: &str, id: &str) -> String {
    format!(
        "`<entry id=\"{id}\">` repeats position {order} of series `{series}`, already held by \
         `<entry id=\"{first}\">`; each position in a series names one entry (dsl 0.19.0 §2.1, \
         §3)"
    )
}

/// The first `::set` / `::retract` of an entry body, in document order,
/// descending into `<match>` arms and `<branch>` / `<hub>` choices: the
/// directive's name and span. `::assert` is idempotent within a run and is
/// not a write that could be lost (see [`W_ENTRY_WRITE_REREAD`]).
fn first_write(nodes: &[lute_syntax::ast::Node]) -> Option<(&'static str, Span)> {
    use lute_syntax::ast::{Arm, Node};
    nodes.iter().find_map(|node| match node {
        Node::Set(s) => Some(("::set", s.span)),
        Node::Retract(r) => Some(("::retract", r.span)),
        Node::Match(m) => m.arms.iter().find_map(|arm| match arm {
            Arm::When { body, .. } | Arm::Otherwise { body, .. } => first_write(body),
        }),
        Node::Branch(b) => b.choices.iter().find_map(|c| first_write(&c.body)),
        Node::Hub(h) => h.choices.iter().find_map(|c| first_write(&c.body)),
        _ => None,
    })
}

/// One entry's attribute shape (`E-ENTRY-ATTR`, `E-PATH-IDENT`) and closure
/// (`E-UNKNOWN-ATTR`). Under a document-level `series:` (`doc_series`, dsl
/// 0.19.0 §2.1) the entry's own `series=` / `order=` is itself the fault —
/// its position is its place in the file — so their value shape is not
/// checked on top.
fn check_entry_shape(entry: &Entry, doc_series: Option<&str>, diags: &mut Vec<Diagnostic>) {
    let attr_diag =
        |message: String, span: Span| diag(E_ENTRY_ATTR, Severity::Error, message, span);
    // A permitted key left in the residual list carried a non-string value
    // (`order=@n`, a bare `target`) — the parser extracts only quoted strings.
    // `when` is never residual: `take_cel` accepts every value shape. The beat
    // keys `on`/`priority` report their own shape (`E-BEAT-ATTR`, below).
    let mut residual_id = false;
    for attr in &entry.attrs {
        let key = attr.key.as_str();
        if !crate::logic_attrs::ENTRY_ATTRS.contains(&key)
            || matches!(key, "when" | "on" | "priority" | "once" | "also" | "share")
        {
            continue;
        }
        if matches!(attr.value, AttrValue::Str(_)) {
            // A repeated key: the parser took the first occurrence.
            continue;
        }
        residual_id |= key == "id";
        diags.push(attr_diag(
            format!("`<entry>` attribute `{key}` must be a quoted string (dsl 0.19.0 §3)"),
            attr.span,
        ));
    }
    crate::logic_attrs::check_entry_attrs(entry, diags);
    crate::beats::check_entry_beat_attrs(entry, diags);

    let id = entry.id.as_str();
    if id.is_empty() {
        if !residual_id {
            diags.push(attr_diag(
                "`<entry>` has no `id`; an entry id is required (dsl 0.19.0 §3)".to_string(),
                entry.id_span,
            ));
        }
    } else if !is_entry_ident(id) {
        diags.push(attr_diag(
            format!(
                "`<entry id=\"{id}\">`: `id` must be an identifier \
                 (`[A-Za-z][A-Za-z0-9_-]*`, dsl 0.19.0 §3)"
            ),
            entry.id_span,
        ));
    } else if id.contains('-') {
        // §8.4 CelIdent alignment, exactly as a quest id: the entry id is a
        // CEL-facing segment of `entry.<id>.read`. The decl still folds so a
        // read does not cascade to `E-UNDECLARED`.
        diags.push(diag(
            E_PATH_IDENT,
            Severity::Error,
            format!("entry id `{id}` has a `-`; CEL-facing names forbid `-` (dsl §8.4)"),
            entry.id_span,
        ));
    }
    if let Some((target, span)) = &entry.target {
        if kind_target(target).is_some() && entry.on.is_none() {
            diags.push(attr_diag(
                format!(
                    "`<entry>` `target=\"{target}\"` answers an occasion for every member of a \
                     kind, so the entry needs `on=` (dsl 0.26.0 §5)"
                ),
                *span,
            ));
        } else if !is_beat_target(target) {
            diags.push(attr_diag(
                format!(
                    "`<entry>` `target=\"{target}\"` is malformed; a target is a dotted id \
                     `Ident (\".\" Segment)*` with `Segment ::= [A-Za-z0-9_-]+`, e.g. \
                     `item.rusty_key`, or `kind:<entity kind>` (dsl 0.19.0 §3, 0.26.0 §5)"
                ),
                *span,
            ));
        }
    }
    if let Some((v, span)) = &entry.category {
        if !is_entry_ident(v) {
            diags.push(attr_diag(
                format!(
                    "`<entry>` `category=\"{v}\"` must be an identifier \
                     (`[A-Za-z][A-Za-z0-9_-]*`, dsl 0.19.0 §3)"
                ),
                *span,
            ));
        }
    }
    if let Some(doc_series) = doc_series {
        for (key, value) in [("series", &entry.series), ("order", &entry.order)] {
            if let Some((v, span)) = value {
                diags.push(attr_diag(
                    format!(
                        "`<entry>` `{key}=\"{v}\"`: this document declares `series: \
                         {doc_series}`; entries are ordered by position in the file, so an \
                         entry carries no `series`/`order` of its own (dsl 0.19.0 §2.1)"
                    ),
                    *span,
                ));
            }
        }
        return;
    }
    if let Some((v, span)) = &entry.series {
        if !is_entry_ident(v) {
            diags.push(attr_diag(
                format!(
                    "`<entry>` `series=\"{v}\"` must be an identifier \
                     (`[A-Za-z][A-Za-z0-9_-]*`, dsl 0.19.0 §3)"
                ),
                *span,
            ));
        }
    }
    if let Some((raw, span)) = &entry.order {
        if parse_entry_order(raw).is_none() {
            diags.push(attr_diag(
                format!(
                    "`<entry>` `order=\"{raw}\"` must be a non-negative integer \
                     (dsl 0.19.0 §3)"
                ),
                *span,
            ));
        }
        let has_series = entry.series.is_some()
            || entry
                .attrs
                .iter()
                .any(|a| a.key == "series" && !matches!(a.value, AttrValue::Str(_)));
        if !has_series {
            diags.push(attr_diag(
                "`<entry>` `order` requires `series`; an order is a position within a series \
                 (dsl 0.19.0 §3)"
                    .to_string(),
                *span,
            ));
        }
    }
}

/// A `Layer::Logic` diagnostic, matching `check_quest`'s own identity
/// diagnostics.
pub(crate) fn diag(code: &str, severity: Severity, message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: code.to_string(),
        severity,
        message,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}
