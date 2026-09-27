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
    snapshot: &lute_manifest::snapshot::CapabilitySnapshot,
) -> EntryRecord {
    let mut record = EntryRecord::default();
    let resolved = resolve_entry_series(doc_series, entries);
    let mut positions: BTreeMap<(&str, u32), &str> = BTreeMap::new();
    for (entry, resolved) in entries.iter().zip(&resolved) {
        check_entry_shape(entry, doc_series, &mut record.diags);
        if let Some(reread) = Reread::of(entry) {
            let read = entry_read_path(&entry.id);
            if let Some((what, span)) = first_write(&entry.body, snapshot, &read, false) {
                record.diags.push(diag(
                    W_ENTRY_WRITE_REREAD,
                    Severity::Warning,
                    reread.message(entry, &what),
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

/// The first `::set` / `::retract` of an entry body — or (HW27-12) call of
/// an effect-only directive declaring `writes` or `retracts`, applied the
/// same way — in document order, descending into `<match>` arms and
/// `<branch>` / `<hub>` choices: the directive's name and span. `::assert`
/// (and an `asserts`-only effect) is idempotent within a run and is not a
/// write that could be lost (see [`W_ENTRY_WRITE_REREAD`]). dsl 0.28.0
/// (T1-6): a write under a condition that holds only while `read`
/// (`entry.<id>.read`) is false — in its own `when=`, an enclosing `<when
/// test>` / `<choice when>`, or a `<match on="entry.<id>.read">` arm
/// `is="false"` — applies on the first read on purpose (`guarded`) and is
/// skipped.
fn first_write(
    nodes: &[lute_syntax::ast::Node],
    snapshot: &lute_manifest::snapshot::CapabilitySnapshot,
    read: &str,
    guarded: bool,
) -> Option<(String, Span)> {
    use lute_syntax::ast::{Arm, CelSlot, Node};
    let under =
        |slot: Option<&CelSlot>| guarded || slot.is_some_and(|s| first_read_only(&s.raw, read));
    nodes.iter().find_map(|node| match node {
        Node::Set(s) if !under(s.when.as_ref()) => Some(("::set".to_string(), s.span)),
        Node::Retract(r) if !under(r.when.as_ref()) => Some(("::retract".to_string(), r.span)),
        Node::Directive(d)
            if !under(d.when.as_ref())
                && crate::directive_facts::is_effect_only(snapshot, &d.tag)
                && snapshot
                    .directive(&d.tag)
                    .and_then(|decl| decl.effects.as_ref())
                    .is_some_and(|e| !e.writes.is_empty() || !e.retracts.is_empty()) =>
        {
            Some((format!("::{}", d.tag), d.span))
        }
        Node::Match(m) => {
            let on_read = m.subject.raw.trim() == read;
            m.arms.iter().find_map(|arm| match arm {
                Arm::When { is, test, body, .. } => {
                    let first_only =
                        on_read && is.as_ref().is_some_and(|p| p.raw.trim() == "false");
                    first_write(body, snapshot, read, first_only || under(Some(test)))
                }
                Arm::Otherwise { body, .. } => first_write(body, snapshot, read, guarded),
            })
        }
        Node::Branch(b) => b
            .choices
            .iter()
            .find_map(|c| first_write(&c.body, snapshot, read, under(c.when.as_ref()))),
        Node::Hub(h) => {
            let options = h
                .choices
                .iter()
                .find_map(|c| first_write(&c.body, snapshot, read, under(c.when.as_ref())));
            options.or_else(|| {
                h.bodies()
                    .skip(h.choices.len())
                    .find_map(|b| first_write(b, snapshot, read, guarded))
            })
        }
        _ => None,
    })
}

/// Whether condition `raw` holds only while `read` (`entry.<id>.read`) is
/// false: with every read of the path taken as `true` it decides false —
/// `!entry.tape.read`, `!(entry.tape.read)`, `entry.tape.read == false`, or
/// any of them in a conjunction.
fn first_read_only(raw: &str, read: &str) -> bool {
    let mask = lute_cel::cel_string_mask(raw);
    let bytes = raw.as_bytes();
    let joins = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'.';
    let mut probe = String::with_capacity(raw.len());
    let mut rest = 0;
    for (at, _) in raw.match_indices(read) {
        let end = at + read.len();
        if mask[at] || (at > 0 && joins(bytes[at - 1])) || bytes.get(end).is_some_and(|&c| joins(c))
        {
            continue;
        }
        probe.push_str(&raw[rest..at]);
        probe.push_str("true");
        rest = end;
    }
    if probe.is_empty() {
        return false;
    }
    probe.push_str(&raw[rest..]);
    crate::fact_env::guard_decides(&probe, false)
}

/// dsl 0.28.0 (T1-6): why an entry can be read more than once in a run —
/// its writes apply on the first read only ([`W_ENTRY_WRITE_REREAD`]).
enum Reread<'e> {
    /// No `on=`: a lookup entry, read whenever it is opened.
    Lookup,
    /// `on=` without `once`: every raise may present it again.
    NoOnce,
    /// A `once` shorter than the run (`day`, `slot`, `week`, `season:<n>`).
    Shorter(&'e str),
    /// `spentBy`: presented until the condition holds.
    SpentBy,
    /// `for=` without `once: run|user`: each member is presented again —
    /// every raise, or each `once` period.
    Members(Option<&'e str>),
}

impl<'e> Reread<'e> {
    /// How `entry` can be re-read in a run, or `None` when it is read at
    /// most once per run (per member, for a `for=` entry).
    fn of(entry: &'e Entry) -> Option<Self> {
        let once = entry
            .once
            .as_ref()
            .map(|(v, _)| v.trim())
            .filter(|v| *v != "false");
        let per_run = once.is_some_and(|o| o == "run" || o == "user");
        if entry.on.is_none() {
            Some(Self::Lookup)
        } else if entry.spent_by.is_some() {
            Some(Self::SpentBy)
        } else if entry.for_kind.is_some() {
            (!per_run).then_some(Self::Members(once))
        } else {
            match once {
                None => Some(Self::NoOnce),
                Some(_) if per_run => None,
                Some(o) => Some(Self::Shorter(o)),
            }
        }
    }

    /// The warning for `entry`'s first write `what` (`::set`, `::retract`,
    /// `::<directive>`): why it is read again, what that does to the write,
    /// and the remedies that work for this shape.
    fn message(&self, entry: &Entry, what: &str) -> String {
        let id = &entry.id;
        let text = |v: &Option<(String, Span)>| v.as_ref().map(|(s, _)| s.trim().to_string());
        let on = text(&entry.on).unwrap_or_default();
        let period = |o: &str| match o.strip_prefix("season:") {
            Some(n) => format!("`{n}` season"),
            None => o.to_string(),
        };
        let (why, lost) = match self {
            Self::Lookup => (
                "answers no occasion, so it is read whenever it is opened".to_string(),
                format!("its `{what}` applies on the first read in a run only"),
            ),
            Self::NoOnce => (
                format!("has no `once`, so every raise of `{on}` can present it again in a run"),
                format!("its `{what}` applies on the first read in a run only"),
            ),
            Self::Shorter(o) => (
                format!(
                    "has `once=\"{o}\"`, so it is presented again each {}",
                    period(o)
                ),
                format!(
                    "its `{what}` applies on the first {} only (an entry's writes apply on its \
                     first read in a run)",
                    period(o)
                ),
            ),
            Self::SpentBy => (
                "stays eligible until its `spentBy` holds, so it can be read again in a run"
                    .to_string(),
                format!("its `{what}` applies on the first read in a run only"),
            ),
            Self::Members(once) => (
                match once {
                    None => format!(
                        "is read per member, and with no `once` every raise of `{on}` can \
                         present a member again in a run"
                    ),
                    Some(o) => format!(
                        "is read per member, and `once=\"{o}\"` presents each member again each {}",
                        period(o)
                    ),
                },
                format!("its `{what}` applies on each member's first read in a run only"),
            ),
        };
        let guard = format!(
            "to keep the write to the first read on purpose, guard it with \
             `when=\"!entry.{id}.read\"`"
        );
        let remedies = match self {
            Self::Lookup => vec![
                guard,
                "a write that must apply every time belongs in a `<beat>` answering an occasion"
                    .to_string(),
            ],
            _ => {
                // An entry without `once` is repeatable, a `<beat>` without
                // `once` is spent for the run: the beat that repeats the
                // entry says `once="false"` (not beside `spentBy`, whose
                // unwritten period is `run` for both).
                let once = match text(&entry.once) {
                    Some(o) if o != "false" => Some(o),
                    _ if entry.spent_by.is_some() => None,
                    _ => Some("false".to_string()),
                };
                let mut attrs = format!("on=\"{on}\"");
                for (key, value) in [
                    ("target", text(&entry.target)),
                    ("for", text(&entry.for_kind)),
                    ("once", once),
                    (
                        "spentBy",
                        entry.spent_by.as_ref().map(|s| s.raw.trim().to_string()),
                    ),
                ] {
                    if let Some(v) = value {
                        attrs.push_str(&format!(" {key}=\"{v}\""));
                    }
                }
                let mut out = vec![format!(
                    "to apply it on every presentation, put the body in a `<beat {attrs}>` — a \
                     beat's writes apply each time it is presented"
                )];
                if !matches!(self, Self::SpentBy) {
                    out.push(match self {
                        Self::Members(_) => "for one application per member per run, write \
                                             `once=\"run\"` on the entry"
                            .to_string(),
                        _ => "for one application per run, write `once=\"run\"` on the entry"
                            .to_string(),
                    });
                }
                out.push(guard);
                out
            }
        };
        format!(
            "`<entry id=\"{id}\">` {why}, but {lost} — later reads skip it; {}",
            remedies.join("; ")
        )
    }
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
            || matches!(
                key,
                "when" | "spentBy" | "on" | "priority" | "once" | "also" | "share"
            )
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
                "`<entry id=\"{id}\">`: `id` must be one name — a letter, then letters, digits \
                 or `_` — since it is read as `entry.<id>.read` (dsl 0.19.0 §3)"
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
                crate::beats::malformed_target("`<entry>`", target, true),
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
