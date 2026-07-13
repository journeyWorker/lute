//! Project-wide `<quest id>` uniqueness across every parsed `.lute` document in
//! a directory (dsl 0.2.0 §6.3, the 0.2.0 F4 residual).
//!
//! `check()`'s own `E-QUEST-ID-DUP` (0.2.0 F4, [`crate::match_check::check_quest`]
//! and [`crate::schema_import::resolve_imports`]'s `imported_quest_ids`) only sees
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
//! also re-derives every collision `check()` already reports per-file (an
//! in-document repeat, or a redeclare against an import-reachable id). That
//! overlap is why `lute check-project`'s caller does NOT treat this pass as
//! the sole authority and blanket-strip every per-file `E-QUEST-ID-DUP`: an
//! import-graph collision can involve a doc OUTSIDE the walked directory
//! (`resolve_imports` sees it via the checked file's OWN `uses:`/`extends:`
//! graph; this pass never can, since it only ever looks at the files the
//! caller walked). Instead the caller keeps every per-file diagnostic and
//! uses [`colliding_occurrences`] to suppress ONLY the ones this pass
//! demonstrably re-reports (0.2.1 review F1), so a real collision is never
//! silently swallowed just because it also happens to be
//! project-wide-visible.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use lute_cel::CelArena;
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{Document, Node};

use crate::cel_paths::{collect_path_uses, is_reserved_quest_path};

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
fn group_by_id<'a>(docs: &'a [(PathBuf, Document)]) -> BTreeMap<&'a str, Vec<(&'a Path, Span)>> {
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
pub fn check_project_quest_ids(docs: &[(PathBuf, Document)]) -> Vec<(PathBuf, Diagnostic)> {
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
pub fn colliding_occurrences(docs: &[(PathBuf, Document)]) -> Vec<(PathBuf, Span)> {
    let mut out = Vec::new();
    for occurrences in group_by_id(docs).into_values() {
        if occurrences.len() < 2 {
            continue;
        }
        out.extend(occurrences.into_iter().map(|(p, s)| (p.to_path_buf(), s)));
    }
    out
}

/// dsl 0.5.1 §1.4: a `check-project` reference to a reserved
/// `quest.<id>.state` / `quest.<id>.objectives.<oid>.done` path whose
/// `<id>` (or `<oid>`, under a project-defined quest) no quest document in
/// the walked project defines.
pub const W_QUEST_REF_UNKNOWN: &str = "W-QUEST-REF-UNKNOWN";

/// [`W_QUEST_REF_UNKNOWN`], [`Layer::Logic`] (matching [`diag`]'s quest-id
/// concern), [`Severity::Warning`] — the reference is shape-legal, and the
/// quest may be defined outside the walked project or added later (dsl
/// 0.5.1 §1.4), so this must never flip a per-file `ok` verdict to error.
fn ref_diag(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: W_QUEST_REF_UNKNOWN.to_string(),
        severity: Severity::Warning,
        message,
        span,
        layer: Layer::Logic,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// DEFINED quest ids and their DEFINED objective ids across every doc in
/// `docs`. Objectives are found by scanning `quest.body` for
/// `Node::Objective` — grammar admission guarantees they appear only
/// directly in a quest body, never nested (mirrors `match_check`'s own
/// `check_quest` scan). An empty quest/objective id is skipped: that
/// document's own missing-id problem (`E-QUEST-ID-MISSING`/
/// `E-OBJECTIVE-ID-MISSING`, reported wherever it is directly `check()`-ed),
/// not a definition this project-wide pass can meaningfully index.
fn defined_quests(docs: &[(PathBuf, Document)]) -> BTreeMap<&str, BTreeSet<&str>> {
    let mut out: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (_, doc) in docs {
        for quest in &doc.quests {
            if quest.id.is_empty() {
                continue;
            }
            let objectives = out.entry(quest.id.as_str()).or_default();
            for node in &quest.body {
                if let Node::Objective(o) = node {
                    if !o.id.is_empty() {
                        objectives.insert(o.id.as_str());
                    }
                }
            }
        }
    }
    out
}

/// Every reserved quest path (`quest.<id>.state` /
/// `quest.<id>.objectives.<oid>.done`) `doc` REFERENCES, paired with the
/// [`Span`] of the enclosing [`lute_syntax::ast::CelSlot`] the reference was
/// found in — post-parse path-level spans are unavailable, so the caller
/// anchors on the enclosing slot (the same convention `cel_paths`'s other
/// callers use, e.g. `defassign`). Each slot's raw text is re-parsed fresh
/// into a scratch [`CelArena`] (mirrors `lute-trace`'s
/// `quest_refs::collect_referenced_reserved_quest_paths` — the analogous
/// collector for `trace`'s single-document `--state` admission, dsl 0.5.1
/// §1.1); a slot that fails to parse contributes nothing (already reported
/// elsewhere by the normal CEL-parse pass). Deduplicated by path — a path
/// read twice in one document gets ONE diagnostic, anchored at its FIRST
/// slot in [`lute_syntax::walk::for_each_cel_slot`]'s canonical pre-order.
fn referenced_reserved_paths(doc: &Document) -> BTreeMap<String, Span> {
    let mut out = BTreeMap::new();
    lute_syntax::walk::for_each_cel_slot(doc, &mut |slot| {
        let raw = slot.raw.trim();
        if raw.is_empty() {
            return;
        }
        let mut arena = CelArena::default();
        let Ok(handle) = lute_cel::parse_slot(&mut arena, raw, 0) else {
            return;
        };
        let Some(rec) = arena.get(handle) else {
            return;
        };
        for use_ in collect_path_uses(&rec.expr) {
            if is_reserved_quest_path(&use_.path) {
                out.entry(use_.path).or_insert(slot.span);
            }
        }
    });
    out
}

fn unknown_quest_message(path: &str, id: &str) -> String {
    format!(
        "`{path}` references quest `{id}`, which no project quest defines (dsl 0.5.1 §1.4) \
         — a typo, or a quest defined outside this walked directory"
    )
}

fn unknown_objective_message(path: &str, quest_id: &str, oid: &str) -> String {
    format!(
        "`{path}` references objective `{oid}` on quest `{quest_id}`, which does not declare \
         that objective (dsl 0.5.1 §1.4)"
    )
}

/// dsl 0.5.1 §1.4: `W-QUEST-REF-UNKNOWN` — verify every reserved
/// `quest.<id>` (and `quest.<id>.objectives.<oid>`) reference across `docs`
/// resolves to a quest (and objective) DEFINED by some quest document among
/// `docs`. A referenced quest `<id>` no project quest defines — or a
/// referenced objective `<oid>` under a quest `docs` DOES define, but that
/// quest does not itself declare `<oid>` — is one warning, naming the
/// referencing document and the exact path (the mistyped-quest-id catch:
/// `quest.heits.state` when the project defines `heist`). Only ever called
/// from `check-project` (the whole-project quest graph this pass needs);
/// single-file `check()` has no such graph and MUST NOT emit this code (dsl
/// 0.5.1 §1.4).
pub fn check_project_quest_refs(docs: &[(PathBuf, Document)]) -> Vec<(PathBuf, Diagnostic)> {
    let defined = defined_quests(docs);
    let mut out = Vec::new();
    for (path, doc) in docs {
        for (ref_path, span) in referenced_reserved_paths(doc) {
            let segs: Vec<&str> = ref_path.split('.').collect();
            match segs.as_slice() {
                ["quest", id, "state"] => {
                    if !defined.contains_key(id) {
                        out.push((
                            path.clone(),
                            ref_diag(unknown_quest_message(&ref_path, id), span),
                        ));
                    }
                }
                ["quest", id, "objectives", oid, "done"] => match defined.get(id) {
                    None => out.push((
                        path.clone(),
                        ref_diag(unknown_quest_message(&ref_path, id), span),
                    )),
                    Some(objectives) => {
                        if !objectives.contains(oid) {
                            out.push((
                                path.clone(),
                                ref_diag(unknown_objective_message(&ref_path, id, oid), span),
                            ));
                        }
                    }
                },
                _ => unreachable!(
                    "referenced_reserved_paths only ever yields is_reserved_quest_path shapes"
                ),
            }
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

    #[test]
    fn colliding_occurrences_empty_when_no_docs_collide() {
        let docs = vec![
            (PathBuf::from("a.lute"), doc(vec![quest("alpha", 1)])),
            (PathBuf::from("b.lute"), doc(vec![quest("beta", 1)])),
        ];
        assert!(colliding_occurrences(&docs).is_empty(), "{docs:?}");
    }

    #[test]
    fn colliding_occurrences_includes_the_groups_first_member_too() {
        // `check_project_quest_ids` never emits a diagnostic for the group's
        // FIRST occurrence (a.lute's) -- `colliding_occurrences` still must
        // report it as a member, since the caller needs to recognize a
        // per-file diagnostic anchored on EITHER file as covered.
        let docs = vec![
            (PathBuf::from("a.lute"), doc(vec![quest("q", 1)])),
            (PathBuf::from("b.lute"), doc(vec![quest("q", 2)])),
        ];
        let out = colliding_occurrences(&docs);
        assert_eq!(out.len(), 2, "{out:?}");
        assert!(
            out.contains(&(PathBuf::from("a.lute"), span(1))),
            "{out:?}"
        );
        assert!(
            out.contains(&(PathBuf::from("b.lute"), span(2))),
            "{out:?}"
        );
    }

    #[test]
    fn colliding_occurrences_ignores_empty_ids() {
        let docs = vec![
            (PathBuf::from("a.lute"), doc(vec![quest("", 1)])),
            (PathBuf::from("b.lute"), doc(vec![quest("", 1)])),
        ];
        assert!(colliding_occurrences(&docs).is_empty(), "{docs:?}");
    }

    // --- `check_project_quest_refs` (dsl 0.5.1 §1.4) ------------------------

    fn parsed(text: &str) -> Document {
        let (doc, diags) = lute_syntax::parse(text);
        assert!(diags.is_empty(), "fixture must parse clean: {diags:?}");
        doc
    }

    fn quest_doc(quest_id: &str, objective_id: &str) -> Document {
        parsed(&format!(
            "---\nkind: quest\n---\n<quest id=\"{quest_id}\">\n\
             <objective id=\"{objective_id}\" done=\"true\"/>\n</quest>\n"
        ))
    }

    fn scene_doc_matching(subject: &str) -> Document {
        parsed(&format!(
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
             <match on=\"{subject}\">\n<when is=\"true\">\n@x: a\n</when>\n\
             <otherwise>\n@x: b\n</otherwise>\n</match>\n"
        ))
    }

    #[test]
    fn quest_refs_no_docs_yields_no_diagnostics() {
        assert!(check_project_quest_refs(&[]).is_empty());
    }

    #[test]
    fn quest_refs_known_quest_and_objective_yield_no_warning() {
        let docs = vec![
            (PathBuf::from("heist.lute"), quest_doc("heist", "steal")),
            (
                PathBuf::from("scene.lute"),
                scene_doc_matching("quest.heist.state"),
            ),
        ];
        assert!(check_project_quest_refs(&docs).is_empty(), "{docs:?}");
    }

    #[test]
    fn quest_refs_known_objective_under_known_quest_yields_no_warning() {
        let docs = vec![
            (PathBuf::from("heist.lute"), quest_doc("heist", "steal")),
            (
                PathBuf::from("scene.lute"),
                scene_doc_matching("quest.heist.objectives.steal.done"),
            ),
        ];
        assert!(check_project_quest_refs(&docs).is_empty(), "{docs:?}");
    }

    #[test]
    fn quest_refs_flags_typo_d_quest_id() {
        let docs = vec![
            (PathBuf::from("heist.lute"), quest_doc("heist", "steal")),
            (
                PathBuf::from("scene.lute"),
                scene_doc_matching("quest.heits.state"),
            ),
        ];
        let out = check_project_quest_refs(&docs);
        assert_eq!(out.len(), 1, "{out:?}");
        let (path, d) = &out[0];
        assert_eq!(path, Path::new("scene.lute"), "names the referencing doc");
        assert_eq!(d.code, "W-QUEST-REF-UNKNOWN");
        assert_eq!(d.severity, Severity::Warning);
        assert!(d.message.contains("quest.heits.state"), "{}", d.message);
        assert!(d.message.contains("heits"), "{}", d.message);
    }

    #[test]
    fn quest_refs_flags_unknown_objective_under_a_known_quest() {
        let docs = vec![
            (PathBuf::from("heist.lute"), quest_doc("heist", "steal")),
            (
                PathBuf::from("scene.lute"),
                scene_doc_matching("quest.heist.objectives.bogus.done"),
            ),
        ];
        let out = check_project_quest_refs(&docs);
        assert_eq!(out.len(), 1, "{out:?}");
        let (path, d) = &out[0];
        assert_eq!(path, Path::new("scene.lute"));
        assert_eq!(d.code, "W-QUEST-REF-UNKNOWN");
        assert_eq!(d.severity, Severity::Warning);
        assert!(
            d.message.contains("quest.heist.objectives.bogus.done"),
            "{}",
            d.message
        );
        assert!(d.message.contains("bogus"), "{}", d.message);
    }

    #[test]
    fn quest_refs_deduplicates_repeated_reads_in_one_document() {
        let scene = parsed(
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n\
             <match on=\"quest.heits.state\">\n\
             <when is=\"active\" test=\"quest.heits.state\">\n@x: a\n</when>\n\
             <otherwise>\n@x: b\n</otherwise>\n</match>\n",
        );
        let docs = vec![
            (PathBuf::from("heist.lute"), quest_doc("heist", "steal")),
            (PathBuf::from("scene.lute"), scene),
        ];
        let out = check_project_quest_refs(&docs);
        assert_eq!(out.len(), 1, "one path read twice is one warning: {out:?}");
    }

    #[test]
    fn quest_refs_ignores_ordinary_declared_paths() {
        let scene = parsed(
            "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
             state:\n  run.flag: { type: bool, default: false }\n---\n## Shot 1.\n\
             <match on=\"run.flag\">\n<when is=\"true\">\n@x: a\n</when>\n\
             <otherwise>\n@x: b\n</otherwise>\n</match>\n",
        );
        let docs = vec![(PathBuf::from("scene.lute"), scene)];
        assert!(check_project_quest_refs(&docs).is_empty(), "{docs:?}");
    }
}
