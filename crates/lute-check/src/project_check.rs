//! Project-wide `<quest id>` uniqueness across every parsed `.lute` document in
//! a directory (dsl 0.2.1 §6.3, the 0.2.0 F4 residual).
//!
//! `check()`'s own `E-QUEST-ID-DUP` (0.2.0 F4, [`crate::match_check::check_quest`]
//! + [`crate::schema_import::resolve_imports`]'s `imported_quest_ids`) only sees
//! a collision within ONE document, or between that document and files it
//! reaches through its OWN `uses:`/`extends:` import graph. Two quest docs that
//! declare the same id but are never linked by an import edge — the common case
//! for, say, two independently-authored side-quest files nobody `uses:`s
//! together — slip past every per-file `check()` call untouched. That is
//! exactly gap #3: quest ids are a flat, PROJECT-WIDE identity (§6.3, "like a
//! named `run.*` fact ... not an implementation leak"), not scoped to whatever
//! subgraph one document's frontmatter happens to import.
//!
//! [`check_project_quest_ids`] closes the gap by looking at every doc in the
//! project directly, with no import-graph traversal at all — so it naturally
//! also re-derives every collision `check()` already reports per-file. It is
//! meant to be the SOLE authority `lute check-project` uses for quest-id
//! uniqueness: the caller strips `E-QUEST-ID-DUP` from each file's own
//! `check()` result before folding it into a project report, so one real-world
//! collision is reported exactly once here — never a per-file copy AND a
//! project-wide copy of the same dup.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::Document;

/// `E-QUEST-ID-DUP`, [`Layer::Logic`] (matching `check_quest`'s own in-document
/// diagnostic — quest-id identity is a §9/§11-style logic concern regardless of
/// whether the repeat lives in one file or two).
fn diag(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: "E-QUEST-ID-DUP".to_string(),
        severity: Severity::Error,
        message,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
    }
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
pub fn check_project_quest_ids(docs: &[(PathBuf, Document)]) -> Vec<(PathBuf, Diagnostic)> {
    let mut by_id: BTreeMap<&str, Vec<(&Path, Span)>> = BTreeMap::new();
    for (path, doc) in docs {
        for quest in &doc.quests {
            if quest.id.is_empty() {
                continue;
            }
            by_id
                .entry(quest.id.as_str())
                .or_default()
                .push((path.as_path(), quest.id_span));
        }
    }

    let mut out = Vec::new();
    for (id, occurrences) in by_id {
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

#[cfg(test)]
mod tests {
    use super::*;
    use lute_syntax::ast::{Meta, Quest};

    fn span(line: u32) -> Span {
        Span {
            byte_start: (line as usize) * 10,
            byte_end: (line as usize) * 10 + 1,
            line,
            column: 1,
            utf16_range: (0, 0),
        }
    }

    fn quest(id: &str, id_line: u32) -> Quest {
        Quest {
            id: id.to_string(),
            id_span: span(id_line),
            title: None,
            start: None,
            fail: None,
            attrs: Vec::new(),
            body: Vec::new(),
            span: span(id_line),
        }
    }

    fn doc(quests: Vec<Quest>) -> Document {
        Document {
            meta: Meta {
                raw_yaml: String::new(),
                span: span(0),
            },
            title: None,
            shots: Vec::new(),
            quests,
            span: span(0),
        }
    }

    #[test]
    fn no_docs_yields_no_diagnostics() {
        assert!(check_project_quest_ids(&[]).is_empty());
    }

    #[test]
    fn distinct_ids_across_files_do_not_collide() {
        let docs = vec![
            (PathBuf::from("a.lute"), doc(vec![quest("alpha", 1)])),
            (PathBuf::from("b.lute"), doc(vec![quest("beta", 1)])),
        ];
        assert!(check_project_quest_ids(&docs).is_empty());
    }

    #[test]
    fn empty_id_never_collides_here() {
        let docs = vec![
            (PathBuf::from("a.lute"), doc(vec![quest("", 1)])),
            (PathBuf::from("b.lute"), doc(vec![quest("", 1)])),
        ];
        assert!(
            check_project_quest_ids(&docs).is_empty(),
            "an empty quest id is E-QUEST-ID-MISSING's problem, not this pass's"
        );
    }

    #[test]
    fn same_file_repeat_is_reported_without_naming_a_second_file() {
        let docs = vec![(
            PathBuf::from("a.lute"),
            doc(vec![quest("q", 1), quest("q", 5)]),
        )];
        let out = check_project_quest_ids(&docs);
        assert_eq!(out.len(), 1, "{out:?}");
        let (path, d) = &out[0];
        assert_eq!(path, Path::new("a.lute"));
        assert_eq!(d.code, "E-QUEST-ID-DUP");
        assert_eq!(d.span.line, 5, "anchored at the SECOND occurrence");
        assert!(
            !d.message.contains("across project files"),
            "an in-document repeat must not claim a cross-file collision: {}",
            d.message
        );
    }

    #[test]
    fn cross_file_collision_names_both_files_and_anchors_the_second() {
        let docs = vec![
            (PathBuf::from("a.lute"), doc(vec![quest("q", 1)])),
            (PathBuf::from("b.lute"), doc(vec![quest("q", 2)])),
        ];
        let out = check_project_quest_ids(&docs);
        assert_eq!(out.len(), 1, "{out:?}");
        let (path, d) = &out[0];
        assert_eq!(path, Path::new("b.lute"), "anchored in the SECOND file");
        assert_eq!(d.span.line, 2);
        assert!(d.message.contains("a.lute"), "{}", d.message);
        assert!(d.message.contains("b.lute"), "{}", d.message);
    }

    #[test]
    fn three_occurrences_flag_every_repeat_past_the_first() {
        // File A declares `q` twice (an in-document repeat); file B declares it
        // once more. Every occurrence PAST the first is flagged: A's 2nd (line
        // 5, same-file) and B's 1st (line 1, cross-file vs A).
        let docs = vec![
            (
                PathBuf::from("a.lute"),
                doc(vec![quest("q", 1), quest("q", 5)]),
            ),
            (PathBuf::from("b.lute"), doc(vec![quest("q", 1)])),
        ];
        let out = check_project_quest_ids(&docs);
        assert_eq!(out.len(), 2, "{out:?}");
        assert_eq!(out[0].0, Path::new("a.lute"));
        assert_eq!(out[0].1.span.line, 5);
        assert!(!out[0].1.message.contains("across project files"));
        assert_eq!(out[1].0, Path::new("b.lute"));
        assert_eq!(out[1].1.span.line, 1);
        assert!(out[1].1.message.contains("across project files"));
    }

    #[test]
    fn distinct_ids_are_independent_of_each_other() {
        let docs = vec![
            (
                PathBuf::from("a.lute"),
                doc(vec![quest("alpha", 1), quest("beta", 2)]),
            ),
            (
                PathBuf::from("b.lute"),
                doc(vec![quest("alpha", 1), quest("gamma", 2)]),
            ),
        ];
        let out = check_project_quest_ids(&docs);
        assert_eq!(out.len(), 1, "only `alpha` collides: {out:?}");
        assert_eq!(out[0].0, Path::new("b.lute"));
    }
}
