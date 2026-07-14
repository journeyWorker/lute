//! Project-wide graph assembly across every parsed `.lute` document in a
//! directory (dsl 0.2.3 connectivity layer, T3+): first the canonical scene
//! identity key set ([`scene_key_set`]), then the checks built on it.
//!
//! Mirrors [`crate::project_check`]'s `<quest id>` project-wide pass: no
//! import-graph traversal, just a flat scan over every doc the caller
//! walked, scoped PER RESOLVED PROJECT ROOT by the caller (`lute-cli`'s
//! `by_root` grouping) — never pooled across the whole walked tree, since
//! two unrelated subprojects reusing the same `character`/`episodeId` is not
//! a collision.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::Document;

use crate::meta::{canonical_episode_key, meta_key_span, resolve_doc_kind, DocKind};
use crate::prereq::{atoms, parse_prereq, Atom, PrereqFormula};

/// dsl §2.3/§4.1 (§A dup): two scene documents resolve the SAME canonical
/// `{character}.{episodeId}` identity key.
pub const E_CONN_EPISODE_ID_DUP: &str = "E-CONN-EPISODE-ID-DUP";

fn diag(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: E_CONN_EPISODE_ID_DUP.to_string(),
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

/// Read `character`/`season`/`episode`/`episodeId` straight from a scene
/// doc's raw frontmatter mapping — the same ad-hoc lookup
/// `lute-compile::artifact_meta` uses, NOT `TypedMeta` (building that needs a
/// `CapabilitySnapshot` the project walk does not have, and `episodeId` is
/// not lifted into `TypedMeta` regardless). Returns `None` when the YAML
/// fails to parse, is not a mapping, or `character`/`season`/`episode` is
/// missing or the wrong type — a malformed identity triad already earns
/// `E-META-MISSING`/`E-META-PARSE` from the normal per-file `check()`; this
/// project-wide pass must never fabricate a degenerate key (e.g.
/// `.s00ep00`) for it, or unrelated malformed docs would cascade into a
/// bogus dup report.
fn scene_identity(doc: &Document) -> Option<(String, i64, i64, Option<String>)> {
    let value: serde_yaml::Value = serde_yaml::from_str(&doc.meta.raw_yaml).ok()?;
    let map = match value {
        serde_yaml::Value::Mapping(m) => m,
        _ => return None,
    };
    let key = |k: &str| serde_yaml::Value::String(k.to_string());
    let character = map.get(key("character"))?.as_str()?.to_string();
    if character.is_empty() {
        return None;
    }
    let season = map.get(key("season"))?.as_i64()?;
    let episode = map.get(key("episode"))?.as_i64()?;
    let episode_id = map.get(key("episodeId")).and_then(|v| v.as_str()).map(String::from);
    Some((character, season, episode, episode_id))
}

/// Every scene document in `docs`, grouped by its computed
/// [`canonical_episode_key`] — never by `character`/`episodeId` decomposed
/// back apart, so a collision via embedded `.` (e.g. `character="a"` +
/// `episodeId="b.c"` vs `character="a.b"` + `episodeId="c"`, both
/// `"a.b.c"`) is caught the same as an identical-pair repeat. Quest
/// documents (no `character`/`season`/`episode` triad) and any scene doc
/// missing/mistyping that triad contribute nothing (see [`scene_identity`]).
/// Anchored at each doc's `character:` key span (mirrors
/// `check_project_quest_ids`'s `id_span` anchor — the actual offending
/// identifier, not a synthetic location).
pub fn scene_key_set(docs: &[(PathBuf, Document)]) -> BTreeMap<String, Vec<(PathBuf, Span)>> {
    let mut by_key: BTreeMap<String, Vec<(PathBuf, Span)>> = BTreeMap::new();
    for (path, doc) in docs {
        if resolve_doc_kind(&doc.meta).0 != Some(DocKind::Scene) {
            continue;
        }
        let Some((character, season, episode, episode_id)) = scene_identity(doc) else {
            continue;
        };
        let key = canonical_episode_key(&character, season, episode, episode_id.as_deref());
        let span = meta_key_span(&doc.meta, "character");
        by_key.entry(key).or_default().push((path.clone(), span));
    }
    by_key
}

/// Every `E-CONN-EPISODE-ID-DUP` collision across `docs`' scene documents
/// (parallel to [`crate::project_check::check_project_quest_ids`]): for each
/// canonical key with 2+ occurrences, every occurrence past the first is one
/// diagnostic, anchored at that occurrence's own `character:` key span.
/// Callers MUST pre-scope `docs` to one resolved project root (`lute-cli`'s
/// `by_root` grouping) — this function itself performs no root scoping.
pub fn check_conn_episode_dup(docs: &[(PathBuf, Document)]) -> Vec<(PathBuf, Diagnostic)> {
    let mut out = Vec::new();
    for (key, occurrences) in scene_key_set(docs) {
        if occurrences.len() < 2 {
            continue;
        }
        let (first_file, _) = &occurrences[0];
        for (file, span) in &occurrences[1..] {
            let message = if file == first_file {
                format!(
                    "duplicate canonical episode key `{key}`; scene `character`+`episodeId` \
                     (or its `s{{season}}ep{{episode}}` default) must be unique project-wide \
                     (dsl §2.3)"
                )
            } else {
                format!(
                    "duplicate canonical episode key `{key}` across project files (`{}` and \
                     `{}`); scene identity must be unique project-wide (dsl §2.3)",
                    first_file.display(),
                    file.display()
                )
            };
            out.push((file.clone(), diag(message, *span)));
        }
    }
    out
}

/// `E-CONN-UNKNOWN-NODE` (dsl §2.3/§4.1 §A): an `after` prerequisite
/// formula's `visited(K)`/`completed(Q)` atom names a node that does not
/// exist anywhere in the project — `K` is not a key in [`scene_key_set`], or
/// `Q` is not a declared `<quest id>`. Exact-string lookup ONLY (never
/// decomposed back into `character`/`episodeId` parts, mirroring
/// [`scene_key_set`]'s own key identity) — Task 5 (DAG/cycle) builds its
/// graph on these resolved nodes, so a fuzzy or partial match here would
/// silently paper over a real typo.
pub const E_CONN_UNKNOWN_NODE: &str = "E-CONN-UNKNOWN-NODE";

fn unknown_node_diag(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: E_CONN_UNKNOWN_NODE.to_string(),
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

/// The `after:` frontmatter value straight from a scene doc's raw YAML
/// mapping — the SAME ad-hoc lookup [`scene_identity`] uses (not `TypedMeta`;
/// see its own doc comment on why this project-wide pass never builds one).
/// `None` on unparsable/non-mapping YAML or an absent/non-string `after` key.
fn scene_after(doc: &Document) -> Option<String> {
    let value: serde_yaml::Value = serde_yaml::from_str(&doc.meta.raw_yaml).ok()?;
    let map = match value {
        serde_yaml::Value::Mapping(m) => m,
        _ => return None,
    };
    map.get(serde_yaml::Value::String("after".to_string()))?
        .as_str()
        .map(String::from)
}

/// Every declared `<quest id>` across `docs` (parallel to
/// `project_check`'s own `group_by_id` traversal, flattened to a plain
/// existence set — [`resolve_nodes`] only ever needs membership, never an
/// occurrence list). An empty id is skipped (that document's own
/// `E-QUEST-ID-MISSING` problem, not a node this pass can meaningfully
/// index). Callers MUST pre-scope `docs` to one resolved project root, same
/// as [`scene_key_set`].
pub fn quest_id_set(docs: &[(PathBuf, Document)]) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for (_, doc) in docs {
        for quest in &doc.quests {
            if !quest.id.is_empty() {
                ids.insert(quest.id.clone());
            }
        }
    }
    ids
}

/// The nearest candidate to `needle` within `max_dist` edits (dsl 0.5.0 §2.2
/// "did you mean" convention — [`crate::cel_paths::nearest_declared_path`]'s
/// same shape but over a plain string set rather than a `StateSchema`).
/// `None` when nothing is close enough; an exact match (distance 0) never
/// reaches this helper — callers only compute a suggestion after a lookup
/// miss.
fn nearest_match<'a>(
    needle: &str,
    candidates: impl Iterator<Item = &'a str>,
    max_dist: usize,
) -> Option<&'a str> {
    candidates
        .map(|k| (k, crate::cel_paths::levenshtein(needle, k)))
        .filter(|&(_, d)| d > 0 && d <= max_dist)
        .min_by_key(|&(_, d)| d)
        .map(|(k, _)| k)
}

/// Exact-lookup every atom flattened out of `formula` (T1 [`atoms`]) against
/// `key_set` (`Atom::Visited`) / `quest_ids` (`Atom::Completed`); a miss
/// pushes one [`E_CONN_UNKNOWN_NODE`] anchored at `span` — the SOURCE
/// formula's span (the scene's `after:` key span, or the quest's
/// `after_span`), never a synthetic per-atom location (`PrereqFormula`
/// carries none).
fn check_formula_atoms(
    formula: &PrereqFormula,
    span: Span,
    path: &Path,
    key_set: &BTreeMap<String, Vec<(PathBuf, Span)>>,
    quest_ids: &BTreeSet<String>,
    out: &mut Vec<(PathBuf, Diagnostic)>,
) {
    for atom in atoms(formula) {
        match atom {
            Atom::Visited(key) => {
                if !key_set.contains_key(&key) {
                    let mut message = format!(
                        "unknown node: no scene resolves to key `{key}` (`visited`, dsl §2.3/§4.1)"
                    );
                    if let Some(sugg) = nearest_match(&key, key_set.keys().map(String::as_str), 2) {
                        message.push_str(&format!(" — did you mean `{sugg}`?"));
                    }
                    out.push((path.to_path_buf(), unknown_node_diag(message, span)));
                }
            }
            Atom::Completed(id) => {
                if !quest_ids.contains(&id) {
                    let mut message = format!(
                        "unknown node: no quest declares id `{id}` (`completed`, dsl §2.3/§4.1)"
                    );
                    if let Some(sugg) = nearest_match(&id, quest_ids.iter().map(String::as_str), 2) {
                        message.push_str(&format!(" — did you mean `{sugg}`?"));
                    }
                    out.push((path.to_path_buf(), unknown_node_diag(message, span)));
                }
            }
        }
    }
}

/// Resolve every `after` prerequisite formula in `docs` — BOTH surfaces
/// (dsl §2.1): a scene document's frontmatter `after:` key, AND every
/// `<quest after="…">` attribute (a quest pack declares its prerequisite
/// there instead) — against the known project node sets. `key_set` (T3
/// [`scene_key_set`]) and `quest_ids` ([`quest_id_set`]) are supplied by the
/// caller so both are computed exactly once per resolved project root
/// (`lute-cli`'s `by_root` grouping), never recomputed per-doc here.
///
/// Grammar-invalid `after` text already earns `E-CONN-PROFILE` from the
/// per-file `check()` pass (T2) — [`crate::prereq::parse_prereq`] returning
/// `None` here is silently skipped, never double-reported.
pub fn resolve_nodes(
    docs: &[(PathBuf, Document)],
    key_set: &BTreeMap<String, Vec<(PathBuf, Span)>>,
    quest_ids: &BTreeSet<String>,
) -> Vec<(PathBuf, Diagnostic)> {
    let mut out = Vec::new();
    for (path, doc) in docs {
        if resolve_doc_kind(&doc.meta).0 == Some(DocKind::Scene) {
            if let Some(after) = scene_after(doc) {
                let after_span = meta_key_span(&doc.meta, "after");
                let (formula, _) = parse_prereq(&after, after_span);
                if let Some(formula) = formula {
                    check_formula_atoms(&formula, after_span, path, key_set, quest_ids, &mut out);
                }
            }
        }
        for quest in &doc.quests {
            if let Some(after) = &quest.after {
                let (formula, _) = parse_prereq(after, quest.after_span);
                if let Some(formula) = formula {
                    check_formula_atoms(&formula, quest.after_span, path, key_set, quest_ids, &mut out);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use lute_syntax::ast::Meta;

    fn span(line: u32) -> Span {
        Span {
            byte_start: (line as usize) * 10,
            byte_end: (line as usize) * 10 + 1,
            line,
            column: 1,
            utf16_range: (0, 0),
        }
    }

    fn doc(raw_yaml: &str) -> Document {
        Document {
            meta: Meta {
                raw_yaml: raw_yaml.to_string(),
                span: span(0),
            },
            title: None,
            shots: Vec::new(),
            quests: Vec::new(),
            span: span(0),
        }
    }

    #[test]
    fn identical_pair_in_same_root_is_dup() {
        let raw = "kind: scene\ncharacter: bianca\nseason: 1\nepisode: 1\n";
        let docs = vec![
            (PathBuf::from("a.lute"), doc(raw)),
            (PathBuf::from("b.lute"), doc(raw)),
        ];
        let out = check_conn_episode_dup(&docs);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].1.code, "E-CONN-EPISODE-ID-DUP");
    }

    #[test]
    fn distinct_keys_do_not_collide() {
        let docs = vec![
            (
                PathBuf::from("a.lute"),
                doc("kind: scene\ncharacter: bianca\nseason: 1\nepisode: 1\n"),
            ),
            (
                PathBuf::from("b.lute"),
                doc("kind: scene\ncharacter: bianca\nseason: 1\nepisode: 2\n"),
            ),
        ];
        assert!(check_conn_episode_dup(&docs).is_empty());
    }

    #[test]
    fn cross_pair_join_collision_is_caught() {
        // character="a", episodeId="b.c"  vs  character="a.b", episodeId="c"  → same "a.b.c"
        let docs = vec![
            (
                PathBuf::from("a.lute"),
                doc("kind: scene\ncharacter: a\nseason: 1\nepisode: 1\nepisodeId: b.c\n"),
            ),
            (
                PathBuf::from("b.lute"),
                doc("kind: scene\ncharacter: a.b\nseason: 1\nepisode: 1\nepisodeId: c\n"),
            ),
        ];
        assert_eq!(check_conn_episode_dup(&docs).len(), 1);
    }

    /// Regression (review note): a scene doc missing/mistyping
    /// `character`/`season`/`episode` must never fall back to a degenerate
    /// key (e.g. `.s00ep00`) — two such malformed docs must NOT collide.
    /// That doc's own missing-key problem is `E-META-MISSING`'s job, from
    /// the normal per-file `check()`, not this project-wide pass.
    #[test]
    fn missing_identity_keys_never_fabricate_a_dup() {
        let docs = vec![
            (
                PathBuf::from("a.lute"),
                doc("kind: scene\nseason: 1\nepisode: 1\n"),
            ),
            (
                PathBuf::from("b.lute"),
                doc("kind: scene\nseason: 1\nepisode: 1\n"),
            ),
        ];
        assert!(check_conn_episode_dup(&docs).is_empty());
    }
}
