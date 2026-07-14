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

use std::collections::BTreeMap;
use std::path::PathBuf;

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::Document;

use crate::meta::{canonical_episode_key, meta_key_span, resolve_doc_kind, DocKind};

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
