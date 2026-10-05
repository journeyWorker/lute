use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{Document, Node};
use crate::ProjectDoc;
use crate::cel_paths::is_reserved_quest_path;
use super::paths::referenced_paths;
use super::quest_tree::E_QUEST_REF_UNKNOWN;

/// dsl 0.5.1 §1.4: a `check-project` reference to a reserved
/// `quest.<id>.state` / `quest.<id>.objectives.<oid>.done` path whose
/// `<id>` (or `<oid>`, under a project-defined quest) no quest document in
/// the walked directory defines — when that directory is not a whole project
/// (no `lute.project.yaml` at its root), so the quest may live outside it.
/// Over a whole project the same reference is [`E_QUEST_REF_UNKNOWN`].
pub const W_QUEST_REF_UNKNOWN: &str = "W-QUEST-REF-UNKNOWN";

/// [`W_QUEST_REF_UNKNOWN`] (a [`Severity::Warning`]: the quest may be defined
/// outside the walked directory, dsl 0.5.1 §1.4) or, when the walk covers
/// the whole project, [`E_QUEST_REF_UNKNOWN`] (an error: no quest anywhere
/// answers the read, so it can never be true). [`Layer::Logic`], matching
/// [`diag`]'s quest-id concern.
fn ref_diag(message: String, span: Span, whole_project: bool) -> Diagnostic {
    let (code, severity) = if whole_project {
        (E_QUEST_REF_UNKNOWN, Severity::Error)
    } else {
        (W_QUEST_REF_UNKNOWN, Severity::Warning)
    };
    Diagnostic {
        code: code.to_string(),
        severity,
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

/// DEFINED quest ids and their DEFINED objective ids across every doc in
/// `docs`. Objectives are found by scanning `quest.body` for
/// `Node::Objective` — grammar admission guarantees they appear only
/// directly in a quest body, never nested (mirrors `match_check`'s own
/// `check_quest` scan). An empty quest/objective id is skipped: that
/// document's own missing-id problem (`E-QUEST-ID-MISSING`/
/// `E-OBJECTIVE-ID-MISSING`, reported wherever it is directly `check()`-ed),
/// not a definition this project-wide pass can meaningfully index.
pub(crate) fn defined_quests<'a>(docs: &'a [ProjectDoc<'a>]) -> BTreeMap<&'a str, BTreeSet<&'a str>> {
    let mut out: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for item in docs {
        let doc = item.doc;
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
pub(crate) fn referenced_reserved_paths(
    doc: &Document,
    meta: &crate::meta::TypedMeta,
) -> BTreeMap<String, Span> {
    referenced_paths(doc, meta, is_reserved_quest_path, |path| {
        path.strip_prefix("quest.")
            .and_then(|rest| rest.split('.').next())
            .map(|id| ("quest.".len(), id.len()))
    })
}

/// ` — did you mean `x`?` over `known`, or nothing when none is close. An
/// id with a `.` is refused where it is declared (`E-PATH-IDENT`), so it is
/// never the spelling to read: when it is the close one, the hint says to
/// rename it.
pub(crate) fn did_you_mean<'a>(id: &str, known: impl Iterator<Item = &'a str>) -> String {
    match lute_manifest::suggest::nearest(id, known, 2) {
        Some(near) if near.contains('.') => format!(
            " — `{near}` is close, but a `.` makes it no id: rename it (for example `{}`)",
            near.split('.')
                .enumerate()
                .map(|(i, s)| match (i, s.chars().next()) {
                    (0, _) | (_, None) => s.to_string(),
                    (_, Some(f)) => f.to_uppercase().chain(s.chars().skip(1)).collect(),
                })
                .collect::<String>()
        ),
        Some(near) => format!(" — did you mean `{near}`?"),
        None => String::new(),
    }
}

fn unknown_quest_message(path: &str, id: &str, hint: &str, whole_project: bool) -> String {
    let why = if whole_project {
        ""
    } else {
        " — a typo, or a quest defined outside this walked directory"
    };
    format!("`{path}` references quest `{id}`, which no project quest defines{hint}{why}")
}

fn unknown_objective_message(path: &str, quest_id: &str, oid: &str) -> String {
    format!(
        "`{path}` references objective `{oid}` on quest `{quest_id}`, which does not declare \
         that objective (dsl 0.5.1 §1.4)"
    )
}

/// When `id` names a quest DOCUMENT (its frontmatter `id:` is `id`,
/// `quest.<id>`, or ends in `.<id>`) rather than a `<quest>`: the sentence
/// naming the quests it declares — the doc-id/quest-id confusion.
fn quest_doc_hint(docs: &[ProjectDoc<'_>], id: &str) -> Option<String> {
    docs.iter().find_map(|item| {
        let doc = item.doc;
        let quests: Vec<&str> = doc
            .quests
            .iter()
            .map(|q| q.id.as_str())
            .filter(|q| !q.is_empty())
            .collect();
        if quests.is_empty() {
            return None;
        }
        let Some(doc_id) = item.meta.id.as_deref() else {
            return None;
        };
        let named = doc_id == id || doc_id.rsplit('.').next() == Some(id);
        named.then(|| {
            let list = quests
                .iter()
                .map(|q| format!("`{q}`"))
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "; `{doc_id}` is a quest document's `id:`, not a quest — its quest is {list}, \
                 read as `quest.{}.state`",
                quests[0]
            )
        })
    })
}

/// dsl 0.5.1 §1.4: verify every reserved `quest.<id>` (and
/// `quest.<id>.objectives.<oid>`) reference across `docs` resolves to a
/// quest (and objective) DEFINED by some quest document among `docs`. A
/// referenced quest `<id>` no project quest defines — or a referenced
/// objective `<oid>` under a quest `docs` DOES define, but that quest does
/// not itself declare `<oid>` — is one diagnostic, naming the referencing
/// document and the exact path (the mistyped-quest-id catch:
/// `quest.heits.state` when the project defines `heist`), and a quest
/// document's `id:` written where its quest id belongs. `whole_project`
/// (the walk covers a `lute.project.yaml` root) makes it
/// [`E_QUEST_REF_UNKNOWN`]; else [`W_QUEST_REF_UNKNOWN`]. Only ever called
/// from `check-project`; single-file `check()` has no such graph and MUST
/// NOT emit either code.
pub fn check_project_quest_refs(
    docs: &[ProjectDoc<'_>],
    whole_project: bool,
) -> Vec<(PathBuf, Diagnostic)> {
    let defined = defined_quests(docs);
    let unknown = |ref_path: &str, id: &str| {
        let hint =
            quest_doc_hint(docs, id).unwrap_or_else(|| did_you_mean(id, defined.keys().copied()));
        unknown_quest_message(ref_path, id, &hint, whole_project)
    };
    let mut out = Vec::new();
    for item in docs {
        let path = item.path;
        let doc = item.doc;
        for (ref_path, span) in referenced_reserved_paths(doc, item.meta) {
            let segs: Vec<&str> = ref_path.split('.').collect();
            let message = match segs.as_slice() {
                // dsl 0.8.0 §5 / 0.24.0 §2: the narrative-time anchor and the
                // failure reason carry no objective segment, so their only
                // project-wide obligation is that the quest id resolves.
                ["quest", id, "state" | "activatedAt" | "failedBy"] => {
                    (!defined.contains_key(id)).then(|| unknown(&ref_path, id))
                }
                ["quest", id, "objectives", oid, "done" | "failed"] => match defined.get(id) {
                    None => Some(unknown(&ref_path, id)),
                    Some(objectives) => (!objectives.contains(oid))
                        .then(|| unknown_objective_message(&ref_path, id, oid)),
                },
                _ => unreachable!(
                    "referenced_reserved_paths only ever yields is_reserved_quest_path shapes"
                ),
            };
            if let Some(message) = message {
                out.push((path.to_path_buf(), ref_diag(message, span, whole_project)));
            }
        }
    }
    out
}
