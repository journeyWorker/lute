use super::graph_nodes::*;
// Project-wide graph assembly across every parsed `.lute` document in a
// directory (dsl 0.2.3 connectivity layer, T3+): first the canonical scene
// identity key set ([`scene_key_set`]), then the checks built on it.
//
// Mirrors [`crate::project_check`]'s `<quest id>` project-wide pass: no
// import-graph traversal, just a flat scan over every doc the caller
// walked, scoped PER RESOLVED PROJECT ROOT by the caller (`lute-cli`'s
// `by_root` grouping) — never pooled across the whole walked tree, since
// two unrelated subprojects reusing the same `character`/`episodeId` is not
// a collision.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use crate::ProjectDoc;


use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::Document;

use crate::meta::{canonical_episode_key, meta_key_span, resolve_doc_kind, DocKind};

/// dsl §2.3/§4.1, dsl 0.15.0 §2/§6 (D-B), dsl 0.19.0 §2.1: two documents
/// resolve to the SAME document id — a scene's canonical scene id (authored
/// `id:` or the derived `{character}.{episodeId}` fallback) or a quest/lore
/// document's authored `id:`, all one project-wide namespace. The code stays
/// for tooling stability (breaks no downstream `--deny` config); the message
/// names the document id.
pub const E_CONN_EPISODE_ID_DUP: &str = "E-CONN-EPISODE-ID-DUP";

pub(super) fn diag(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: E_CONN_EPISODE_ID_DUP.to_string(),
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

/// A scene document's canonical identity as resolved from retained typed
/// frontmatter YAML (dsl 0.15.0 §2): the canonical scene key plus the
/// frontmatter key name the diagnostic anchor should point at — `id:` for an
/// authored key, `character:` for the derived fallback.
/// `{character}.{episodeId}` (with `episodeId` defaulting to
/// `s{season:02}ep{episode:02}` via [`canonical_episode_key`]) is
/// reconstructed; a scene doc missing/mistyping any of `character`/`season`/
/// `episode` earns `E-META-MISSING`/`E-META-PARSE` from the per-file
/// `check()` and this project-wide pass must never fabricate a degenerate
/// key (e.g. `.s00ep00`) for it, or unrelated malformed docs would cascade
/// into a bogus dup report.
pub(super) struct SceneIdentity {
    pub(super) key: String,
    anchor: &'static str,
}

pub(super) fn scene_identity(meta: &crate::meta::TypedMeta) -> Option<SceneIdentity> {
    // The AUTHORED frontmatter, never the defaults-overlaid typed fields: a
    // manifest default must not mint a scene key, and a malformed `id:` gives
    // no identity (its own `E-META-ID` anchors).
    let map = meta.yaml()?.as_mapping()?;
    let key = |k: &str| serde_yaml::Value::String(k.to_string());
    if let Some(raw) = map.get(key("id")).and_then(|v| v.as_str()) {
        return lute_manifest::ident::is_dotted_name(raw).then(|| SceneIdentity {
            key: raw.to_string(),
            anchor: "id",
        });
    }
    let character = map.get(key("character"))?.as_str()?;
    if character.is_empty() {
        return None;
    }
    let season = map.get(key("season"))?.as_i64()?;
    let episode = map.get(key("episode"))?.as_i64()?;
    let episode_id = map.get(key("episodeId")).and_then(|v| v.as_str());
    Some(SceneIdentity {
        key: canonical_episode_key(character, season, episode, episode_id),
        anchor: "character",
    })
}

/// Every scene document in `docs`, grouped by its canonical scene key
/// ([`canonical_episode_key`] for the derived fallback, or the authored
/// `id:` value verbatim — dsl 0.15.0 §2). Authored and derived keys share
/// ONE namespace, so a collision between them is a `E-CONN-EPISODE-ID-DUP`
/// exactly the same as an identical-pair repeat. Quest documents (no
/// canonical scene key) and any scene doc missing/mistyping its identity
/// contribute nothing (see [`scene_identity`]). Anchored at each doc's
/// canonical-key source: `id:` when authored, `character:` for the derived
/// triad.
pub fn scene_key_set(docs: &[ProjectDoc<'_>]) -> BTreeMap<String, Vec<(PathBuf, Span)>> {
    let mut by_key: BTreeMap<String, Vec<(PathBuf, Span)>> = BTreeMap::new();
    for item in docs {
        let path = item.path;
        let doc = item.doc;
        if crate::meta::authored_doc_kind(item.meta.yaml()) != Some(DocKind::Scene) {
            continue;
        }
        let Some(SceneIdentity { key, anchor }) = scene_identity(item.meta) else {
            continue;
        };
        let span = meta_key_span(&doc.meta, anchor);
        by_key.entry(key).or_default().push((path.to_path_buf(), span));
    }
    by_key
}

/// One scene document's canonical key — [`scene_key_set`]'s identity for a
/// single document; `None` for a non-scene or a scene whose identity cannot
/// be read.
pub fn scene_key(meta: &crate::meta::TypedMeta) -> Option<String> {
    if crate::meta::authored_doc_kind(meta.yaml()) != Some(DocKind::Scene) {
        return None;
    }
    let map = meta.yaml()?.as_mapping()?;
    let key = |k: &str| serde_yaml::Value::String(k.to_string());
    if let Some(raw) = map.get(key("id")).and_then(|v| v.as_str()) {
        return lute_manifest::ident::is_dotted_name(raw).then(|| raw.to_string());
    }
    let character = map.get(key("character"))?.as_str()?;
    let season = map.get(key("season"))?.as_i64()?;
    let episode = map.get(key("episode"))?.as_i64()?;
    let episode_id = map.get(key("episodeId")).and_then(|v| v.as_str());
    Some(canonical_episode_key(character, season, episode, episode_id))
}

/// A quest or lore document's authored, well-formed `id:` (dsl 0.19.0
/// §2.1), read off the raw frontmatter under the same `[A-Za-z0-9_.-]+` gate
/// as a scene's (a rejected id contributes nothing; its own `E-META-ID`
/// anchors). Without one the document has no id in the shared namespace —
/// its fallback index key (the first declared quest/entry id) is not a
/// document id.
pub fn bundle_id(meta: &crate::meta::TypedMeta) -> Option<String> {
    let map = meta.yaml()?.as_mapping()?;
    map.get(serde_yaml::Value::String("id".to_string()))?
        .as_str()
        .filter(|raw| lute_manifest::ident::is_dotted_name(raw))
        .map(str::to_string)
}

/// Every bundle beat's canonical id (dsl 0.23.0 §4, `<document id>.<beat
/// id>`) in `docs`, anchored at the beat's `id` — the keys `visited()`
/// resolves beside the scene keys. A lore document without a well-formed
/// `id:`, or a beat whose id is not a name, contributes nothing (its
/// own `E-BEAT-ATTR` anchors); a beat id repeated within one document counts
/// once (the per-file check reports the repeat).
pub fn bundle_beat_key_set(docs: &[ProjectDoc<'_>]) -> BTreeMap<String, Vec<(PathBuf, Span)>> {
    let mut by_key: BTreeMap<String, Vec<(PathBuf, Span)>> = BTreeMap::new();
    for item in docs {
        let path = item.path;
        let doc = item.doc;
        if doc.beats.is_empty() || resolve_doc_kind(&doc.meta).0 != Some(DocKind::Lore) {
            continue;
        }
        let Some(doc_id) = bundle_id(item.meta) else {
            continue;
        };
        let mut seen = BTreeSet::new();
        for beat in &doc.beats {
            if !lute_manifest::ident::is_name(&beat.id) || !seen.insert(beat.id.as_str()) {
                continue;
            }
            by_key
                .entry(crate::bundles::bundle_beat_key(&doc_id, &beat.id))
                .or_default()
                .push((path.to_path_buf(), beat.id_span));
        }
    }
    by_key
}

/// dsl 0.25.0 §3: the `after=` (raw text + value span) of the bundle beat
/// whose canonical id is `key`, declared in `doc` — its first declaration,
/// as [`bundle_beat_key_set`] anchors it.
pub fn bundle_beat_after<'d>(
    doc: &'d Document,
    meta: &crate::meta::TypedMeta,
    key: &str,
) -> Option<&'d (String, Span)> {
    bundle_beat(doc, meta, key)?.after.as_ref()
}

/// The first `<beat>` of `doc` whose canonical id is `key`.
pub(super) fn bundle_beat<'d>(
    doc: &'d Document,
    meta: &crate::meta::TypedMeta,
    key: &str,
) -> Option<&'d lute_syntax::ast::BundleBeat> {
    let doc_id = bundle_id(meta)?;
    doc.beats.iter().find(|b| crate::bundles::bundle_beat_key(&doc_id, &b.id) == key)
}

/// dsl 0.25.0 §3: every beat node of `graph` (a scene beat or a bundle
/// beat) with no `after` whose `when` has top-level `visited('<id>')`
/// conjuncts naming a node of the graph — each such conjunct gates the beat
/// but draws no edge. `lute scenario` lists these on its unanchored list and
/// suggests moving them to `after=` / `after:`. The ids come in source
/// order; an unparseable `when` (the per-file check's) contributes nothing.
pub fn when_visited_unanchored(
    docs: &[ProjectDoc<'_>],
    graph: &ConnGraph,
) -> Vec<(NodeId, Vec<String>)> {
    let by_path: BTreeMap<&Path, &ProjectDoc<'_>> =
        docs.iter().map(|item| (item.path, item)).collect();
    let mut out = Vec::new();
    for (id, info) in &graph.nodes {
        if !matches!(info.prereq, PrereqState::Absent) {
            continue;
        }
        let Some(item) = by_path.get(info.path.as_path()) else {
            continue;
        };
        let doc = item.doc;
        let when = match id {
            NodeId::Scene(_) => scene_frontmatter_str(item.meta, "when"),
            NodeId::Beat(key) => bundle_beat(doc, item.meta, key)
                .and_then(|b| b.when.as_ref())
                .map(|w| w.raw.clone()),
            _ => None,
        };
        let Some(when) = when.filter(|w| w.contains(crate::cel_resolve::VISITED_FN)) else {
            continue;
        };
        let mut arena = lute_cel::CelArena::default();
        let Some(root) =
            lute_cel::parse_slot_marked_refs(&mut arena, &when).and_then(|h| arena.get(h).cloned())
        else {
            continue;
        };
        let mut ids = Vec::new();
        visited_conjuncts(&root.expr, &mut ids);
        ids.retain(|k| graph.nodes.contains_key(&NodeId::visited(k, &graph.nodes)));
        if !ids.is_empty() {
            out.push((id.clone(), ids));
        }
    }
    out
}

/// The `visited('<id>')` calls among the top-level `&&` conjuncts of `e`.
pub(super) fn visited_conjuncts(e: &cel_parser::ast::Expr, out: &mut Vec<String>) {
    let cel_parser::ast::Expr::Call(c) = e else {
        return;
    };
    if c.func_name == cel_parser::ast::operators::LOGICAL_AND
        && c.target.is_none()
        && c.args.len() == 2
    {
        visited_conjuncts(&c.args[0].expr, out);
        visited_conjuncts(&c.args[1].expr, out);
    } else if let Some(k) = crate::cel_resolve::visited_call_target(c) {
        out.push(k.to_string());
    }
}

/// Every document id in `docs`, grouped by id in `docs` order (dsl 0.19.0
/// §2.1): each scene's canonical scene key (as [`scene_key_set`]), each
/// quest or lore document's authored `id:` ([`bundle_id`], anchored at that
/// key), and each bundle beat's canonical id (dsl 0.23.0 §4,
/// [`bundle_beat_key_set`]) — one project-wide namespace, since `visited()`
/// reads scene and bundle beat ids alike. Only the dup check reads this;
/// `visited(K)` resolution stays on [`scene_key_set`] (+ bundle beat keys in
/// a condition slot), since a quest or lore document is not a scene node.
pub(super) fn document_id_set(docs: &[ProjectDoc<'_>]) -> BTreeMap<String, Vec<(PathBuf, Span)>> {
    let mut by_id: BTreeMap<String, Vec<(PathBuf, Span)>> = BTreeMap::new();
    for item in docs {
        let path = item.path;
        let doc = item.doc;
        let (key, anchor) = match resolve_doc_kind(&doc.meta).0 {
            Some(DocKind::Scene) => match scene_identity(item.meta) {
                Some(SceneIdentity { key, anchor }) => (key, anchor),
                None => continue,
            },
            Some(DocKind::Quest | DocKind::Lore) => match bundle_id(item.meta) {
                Some(id) => (id, "id"),
                None => continue,
            },
            None => continue,
        };
        by_id
            .entry(key)
            .or_default()
            .push((path.to_path_buf(), meta_key_span(&doc.meta, anchor)));
    }
    for (key, occurrences) in bundle_beat_key_set(docs) {
        by_id.entry(key).or_default().extend(occurrences);
    }
    by_id
}

/// Every `E-CONN-EPISODE-ID-DUP` collision across `docs` (parallel to
/// [`crate::project_check::check_project_quest_ids`]): for each document id
/// ([`document_id_set`]) with 2+ occurrences, every occurrence past the first
/// is one diagnostic, anchored at that occurrence's own id source — `id:`
/// when authored, `character:` for a scene's derived triad. Callers MUST
/// pre-scope `docs` to one resolved project root (`lute-cli`'s `by_root`
/// grouping) — this function itself performs no root scoping.
///
/// The code stays `E-CONN-EPISODE-ID-DUP` for tooling stability (dsl 0.15.0
/// §2/§6 D-B): renaming would break downstream `--deny` configs and log
/// tooling to convey no new information. The MESSAGE generalises to
/// `document id` (dsl 0.19.0 §2.1): scene ids (authored or derived) and
/// quest/lore document ids share one namespace.
pub fn check_conn_episode_dup(docs: &[ProjectDoc<'_>]) -> Vec<(PathBuf, Diagnostic)> {
    let mut out = Vec::new();
    for (key, occurrences) in document_id_set(docs) {
        if occurrences.len() < 2 {
            continue;
        }
        let (first_file, _) = &occurrences[0];
        for (file, span) in &occurrences[1..] {
            let message = if file == first_file {
                format!(
                    "duplicate document id `{key}`; a document's `id:` (or a scene's \
                     `{{character}}.{{episodeId}}` fallback, or a bundle beat's `<document \
                     id>.<beat id>`) must be unique project-wide across scene, quest, and lore \
                     documents (dsl 0.15.0 §2, dsl 0.19.0 §2.1, dsl 0.23.0 §4)"
                )
            } else {
                format!(
                    "duplicate document id `{key}` across project files (`{}` and `{}`); a \
                     document's `id:` (or a scene's `{{character}}.{{episodeId}}` fallback, or a \
                     bundle beat's `<document id>.<beat id>`) must be unique project-wide across \
                     scene, quest, and lore documents (dsl 0.15.0 §2, dsl 0.19.0 §2.1, dsl \
                     0.23.0 §4)",
                    first_file.display(),
                    file.display()
                )
            };
            out.push((file.clone(), diag(message, *span)));
        }
    }
    out
}

