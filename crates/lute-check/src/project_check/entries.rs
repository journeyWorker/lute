use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use lute_core_span::{Diagnostic, Severity, Span};
use lute_syntax::ast::Document;
use crate::ProjectDoc;
use crate::cel_paths::reserved_entry_id;
use crate::lore::{document_series, resolve_entry_series, series_order_message, E_ENTRY_ID_DUP, E_ENTRY_SERIES_ORDER, W_ENTRY_REF_UNKNOWN};
use super::paths::{did_you_mean, referenced_paths};

/// Every non-empty `<entry id>` occurrence in `docs`, grouped by id — the
/// lore mirror of [`group_by_id`] (dsl 0.19.0 §3: entry ids are unique across
/// the project).
fn group_entries_by_id<'a>(docs: &'a [ProjectDoc<'a>]) -> BTreeMap<&'a str, Vec<(&'a Path, Span)>> {
    let mut by_id: BTreeMap<&str, Vec<(&Path, Span)>> = BTreeMap::new();
    for item in docs {
        let path = item.path;
        let doc = item.doc;
        for entry in &doc.entries {
            if entry.id.is_empty() {
                continue;
            }
            by_id
                .entry(entry.id.as_str())
                .or_default()
                .push((path, entry.id_span));
        }
    }
    by_id
}

/// Every well-formed RESOLVED `(series, order)` position in `docs` (dsl
/// 0.19.0 §2.1: a document-level `series:` supplies its entries' positions),
/// grouped by position, each occurrence carrying its entry id and its anchor
/// span (the one `crate::lore::check_entries` uses per document). `series`
/// holds each document's own validated `series:`, index-aligned with `docs`.
#[allow(clippy::type_complexity)]
fn group_entries_by_position<'a>(
    docs: &'a [ProjectDoc<'a>],
    series: &'a [Option<String>],
) -> BTreeMap<(&'a str, u32), Vec<(&'a Path, &'a str, Span)>> {
    let mut by_pos: BTreeMap<(&str, u32), Vec<(&Path, &str, Span)>> = BTreeMap::new();
    for (item, doc_series) in docs.iter().zip(series) {
        let path = item.path;
        let doc = item.doc;
        let resolved = resolve_entry_series(doc_series.as_deref(), &doc.entries);
        for (entry, resolved) in doc.entries.iter().zip(resolved) {
            if let Some(position) = resolved.position() {
                by_pos.entry(position).or_default().push((
                    path,
                    entry.id.as_str(),
                    resolved.anchor,
                ));
            }
        }
    }
    by_pos
}

/// Each document's validated `series:` ([`document_series`]), index-aligned
/// with `docs` — the owner [`group_entries_by_position`] borrows from.
fn documents_series(docs: &[ProjectDoc<'_>]) -> Vec<Option<String>> {
    docs.iter()
        .map(|item| document_series(item.meta))
        .collect()
}

/// dsl 0.19.0 §3, project-wide: [`E_ENTRY_ID_DUP`] for every `<entry id>`
/// occurrence past its id's first, and [`E_ENTRY_SERIES_ORDER`] for every
/// RESOLVED `(series, order)` position occurrence past its first (§2.1: a
/// document-level `series:` colliding with attribute-declared positions
/// elsewhere is caught here) — whether the repeat lives in the same file or
/// in another file of the walked root, with no import edge needed (the
/// [`check_project_quest_ids`] shape). Each diagnostic is paired with the
/// file it is anchored in; "first" is `docs`' own order, so callers MUST pass
/// files pre-sorted. An empty id and a malformed/absent series or order are
/// skipped: those are the document's own `E-ENTRY-ATTR`.
pub fn check_project_entry_ids(docs: &[ProjectDoc<'_>]) -> Vec<(PathBuf, Diagnostic)> {
    let mut out = Vec::new();
    for (id, occurrences) in group_entries_by_id(docs) {
        let Some(&(first_file, _)) = occurrences.first() else {
            continue;
        };
        for &(file, span) in &occurrences[1..] {
            let message = if file == first_file {
                format!(
                    "duplicate `<entry id=\"{id}\">`; entry ids must be unique across the \
                     project (dsl 0.19.0 §3)"
                )
            } else {
                format!(
                    "duplicate `<entry id=\"{id}\">` across project files (`{}` and `{}`); \
                     entry ids must be unique across the project (dsl 0.19.0 §3)",
                    first_file.display(),
                    file.display()
                )
            };
            out.push((
                file.to_path_buf(),
                crate::lore::diag(E_ENTRY_ID_DUP, Severity::Error, message, span),
            ));
        }
    }
    let doc_series = documents_series(docs);
    for ((series, order), occurrences) in group_entries_by_position(docs, &doc_series) {
        let Some(&(first_file, first_id, _)) = occurrences.first() else {
            continue;
        };
        for &(file, id, span) in &occurrences[1..] {
            let mut message = series_order_message(series, order, first_id, id);
            if file != first_file {
                message.push_str(&format!(" — in `{}`", first_file.display()));
            }
            out.push((
                file.to_path_buf(),
                crate::lore::diag(E_ENTRY_SERIES_ORDER, Severity::Error, message, span),
            ));
        }
    }
    out
}

/// Every `(path, span)` occurrence belonging to a colliding entry-id group
/// (anchored at `id_span`) or a colliding resolved `(series, order)` group
/// (anchored at the `order` value span, or the entry id under a
/// document-level `series:`) among `docs` — first occurrence included. The
/// lore mirror of [`colliding_occurrences`]: `check-project` suppresses a
/// per-file `E-ENTRY-ID-DUP` / `E-ENTRY-SERIES-ORDER` whose `(path, span)` is
/// a member here, because [`check_project_entry_ids`] already reports that
/// group once; one that is NOT a member (an import-graph collision reaching
/// outside the walked set) is kept.
pub fn colliding_entry_occurrences(docs: &[ProjectDoc<'_>]) -> Vec<(PathBuf, Span)> {
    let mut out = Vec::new();
    for occurrences in group_entries_by_id(docs).into_values() {
        if occurrences.len() >= 2 {
            out.extend(occurrences.into_iter().map(|(p, s)| (p.to_path_buf(), s)));
        }
    }
    let doc_series = documents_series(docs);
    for occurrences in group_entries_by_position(docs, &doc_series).into_values() {
        if occurrences.len() >= 2 {
            out.extend(
                occurrences
                    .into_iter()
                    .map(|(p, _, s)| (p.to_path_buf(), s)),
            );
        }
    }
    out
}

/// Every reserved `entry.<id>.read` / `entry.<id>.everRead` path `doc`
/// references, paired with where it was first found — the lore twin of
/// [`referenced_reserved_paths`] ([`referenced_paths`]: the scene beat's
/// frontmatter `when:` included, anchored on the id).
fn referenced_entry_reads(
    doc: &Document,
    meta: &crate::meta::TypedMeta,
) -> BTreeMap<String, Span> {
    referenced_paths(
        doc,
        meta,
        |path| reserved_entry_id(path).is_some(),
        |path| reserved_entry_id(path).map(|id| ("entry.".len(), id.len())),
    )
}

/// dsl 0.19.0 §5: [`W_ENTRY_REF_UNKNOWN`] — every `entry.<id>.read` read
/// across `docs` whose `<id>` no document among `docs` declares (the
/// mistyped-entry-id catch). A warning, [`Layer::Logic`], exactly like
/// [`W_QUEST_REF_UNKNOWN`]: the read is shape-legal and the entry may live
/// outside the walked project. `check-project` only; single-file `check()`
/// has no project and MUST NOT emit it.
pub fn check_project_entry_refs(docs: &[ProjectDoc<'_>]) -> Vec<(PathBuf, Diagnostic)> {
    let declared: BTreeSet<&str> = docs
        .iter()
        .flat_map(|item| item.doc.entries.iter())
        .filter(|e| !e.id.is_empty())
        .map(|e| e.id.as_str())
        .collect();
    let mut out = Vec::new();
    for item in docs {
        let path = item.path;
        let doc = item.doc;
        for (ref_path, span) in referenced_entry_reads(doc, item.meta) {
            let Some(id) = reserved_entry_id(&ref_path) else {
                continue;
            };
            if declared.contains(id) {
                continue;
            }
            let hint = did_you_mean(id, declared.iter().copied());
            out.push((
                path.to_path_buf(),
                crate::lore::diag(
                    W_ENTRY_REF_UNKNOWN,
                    Severity::Warning,
                    format!(
                        "`{ref_path}` references entry `{id}`, which no project lore document \
                         declares{hint} (dsl 0.19.0 §5) — a typo, or an entry declared outside \
                         this walked directory"
                    ),
                    span,
                ),
            ));
        }
    }
    out
}
