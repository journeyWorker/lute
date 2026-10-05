use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use crate::ProjectDoc;

/// `E-QUEST-ID-DUP`, [`Layer::Logic`] (matching `check_quest`'s own in-document
/// diagnostic — quest-id identity is a §9/§11-style logic concern regardless of
/// whether the repeat lives in one file or two).
pub(crate) fn diag(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: "E-QUEST-ID-DUP".to_string(),
        severity: Severity::Error,
        message,
        evidence: None,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// Every non-empty `<quest id>` occurrence in `docs`, grouped by id — the
/// shared traversal behind both [`check_project_quest_ids`] (which flags
/// every occurrence past the group's first) and [`colliding_occurrences`]
/// (which needs every MEMBER of a colliding group, first occurrence
/// included). An empty id is skipped here too (see
/// [`check_project_quest_ids`]'s own doc comment on why).
pub(crate) fn group_by_id<'a>(docs: &'a [ProjectDoc<'a>]) -> BTreeMap<&'a str, Vec<(&'a Path, Span)>> {
    let mut by_id: BTreeMap<&str, Vec<(&Path, Span)>> = BTreeMap::new();
    for item in docs {
        let path = item.path;
        let doc = item.doc;
        for quest in &doc.quests {
            if quest.id.is_empty() {
                continue;
            }
            by_id
                .entry(quest.id.as_str())
                .or_default()
                .push((path, quest.id_span));
        }
    }
    by_id
}

/// Every `E-QUEST-ID-DUP` collision across `docs`, paired with the file each
/// diagnostic is anchored in (a plain `Diagnostic` carries no path — the caller
/// needs the pairing to print `path:line:col` or to group a JSON report by
/// file).
///
/// For each non-empty quest id, EVERY occurrence past the first — whether the
/// repeat lives in the SAME file (mirroring `check_quest`'s in-document dup,
/// dsl 0.2.0 §6.3) or in a DIFFERENT file with no import edge at all (the 0.2.1
/// residual this function exists for) — is one diagnostic, anchored at that
/// occurrence's own `id_span` (so an editor jump lands on the actual repeated
/// tag, not a synthetic location). "First" is `docs`' own order, so callers
/// MUST pass files pre-sorted (e.g. by path) for deterministic output; within
/// one file, occurrences are in AST/document order.
///
/// An empty id is skipped entirely — that document's own malformed-id problem
/// (`E-QUEST-ID-MISSING`, reported wherever THAT doc is directly checked), not
/// a collision this project-wide pass can meaningfully report (an empty string
/// is not an identity two authors could have intentionally, or even
/// accidentally in any interesting sense, collided on).
pub fn check_project_quest_ids(docs: &[ProjectDoc<'_>]) -> Vec<(PathBuf, Diagnostic)> {
    let mut out = Vec::new();
    for (id, occurrences) in group_by_id(docs) {
        if occurrences.len() < 2 {
            continue;
        }
        let (first_file, _) = occurrences[0];
        for &(file, span) in &occurrences[1..] {
            let message = if file == first_file {
                format!(
                    "duplicate `<quest id=\"{id}\">`; quest ids must be unique (dsl 0.2.0 §6.3)"
                )
            } else {
                format!(
                    "duplicate `<quest id=\"{id}\">` across project files (`{}` and `{}`); \
                     quest ids must be unique project-wide (dsl 0.2.0 §6.3)",
                    first_file.display(),
                    file.display()
                )
            };
            out.push((file.to_path_buf(), diag(message, span)));
        }
    }
    out
}

/// Every `(path, id_span)` occurrence in `docs` that belongs to a quest id
/// declared 2+ times among `docs` — i.e. every member of a group
/// [`check_project_quest_ids`] would flag (including the group's own FIRST
/// occurrence, which that function does NOT emit a diagnostic for, since it
/// is the baseline the rest collide against).
///
/// `lute check-project`'s caller (0.2.1 review F1) uses this to decide
/// whether a per-file `E-QUEST-ID-DUP` it kept from `check()` (an
/// in-document repeat, or a redeclare against an import-reachable id — both
/// anchored at THAT file's own `quest.id_span`, 0.2.0 F4) is a collision this
/// project-wide pass ALREADY reports once for: if the diagnostic's own
/// `(path, span)` is a member of this set, some OTHER occurrence of the same
/// id exists among the WALKED docs, so [`check_project_quest_ids`] is already
/// the single canonical report for that whole group — regardless of which
/// specific occurrence it happened to anchor ITS OWN diagnostic on (a
/// same-id-different-importer collision can anchor the per-file diagnostic on
/// a different file than the one `check_project_quest_ids` picks, since the
/// project pass always skips the group's first-by-path occurrence while the
/// per-file diagnostic fires wherever `check()`'s import resolution happened
/// to detect the redeclare — membership, not anchor equality, is the
/// correct test). A per-file diagnostic whose `(path, span)` is NOT a member
/// here came from a collision this pass structurally cannot see at all (an
/// import-graph collision reaching a doc outside the walked set) and MUST be
/// kept.
pub fn colliding_occurrences(docs: &[ProjectDoc<'_>]) -> Vec<(PathBuf, Span)> {
    let mut out = Vec::new();
    for occurrences in group_by_id(docs).into_values() {
        if occurrences.len() < 2 {
            continue;
        }
        out.extend(occurrences.into_iter().map(|(p, s)| (p.to_path_buf(), s)));
    }
    out
}
