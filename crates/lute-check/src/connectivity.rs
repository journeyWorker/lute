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
use std::fmt;
use std::path::{Path, PathBuf};

use cel_parser::ast::{operators as op, Expr};
use cel_parser::reference::Val;
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{Arm, Assert, CelKind, Document, Node, Quest};

use crate::check::CheckResult;
use crate::meta::{
    canonical_episode_key, is_valid_scene_id_raw, meta_key_span, resolve_doc_kind, DocKind,
};
use crate::prereq::{atoms, parse_prereq, Atom, PrereqFormula};

/// dsl §2.3/§4.1, dsl 0.15.0 §2/§6 (D-B), dsl 0.19.0 §2.1: two documents
/// resolve to the SAME document id — a scene's canonical scene id (authored
/// `id:` or the derived `{character}.{episodeId}` fallback) or a quest/lore
/// document's authored `id:`, all one project-wide namespace. The code stays
/// for tooling stability (breaks no downstream `--deny` config); the message
/// names the document id.
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

/// A scene document's canonical identity as resolved from raw frontmatter
/// (dsl 0.15.0 §2): the canonical scene key plus the frontmatter key name
/// the diagnostic anchor should point at — `id:` for an authored key,
/// `character:` for the derived `{character}.{episodeId}` fallback.
struct SceneIdentity {
    key: String,
    /// The frontmatter key the diagnostic must anchor at. Constant so both
    /// [`scene_key_set`] and any future consumer report the same shape.
    anchor: &'static str,
}

/// Resolve a scene document's canonical key straight off its raw frontmatter
/// mapping — the same ad-hoc lookup `lute-compile::artifact_meta` uses, NOT
/// `TypedMeta` (building that needs a `CapabilitySnapshot` the project walk
/// does not have). dsl 0.15.0 §2: authored `id:` wins entire when present and
/// well-formed (same `[A-Za-z0-9_.-]+` gate as [`crate::meta::parse_meta`],
/// so a rejected id contributes no key here — its own `E-META-ID` is the
/// anchoring diagnostic). Otherwise the derived legacy join
/// `{character}.{episodeId}` (with `episodeId` defaulting to
/// `s{season:02}ep{episode:02}` via [`canonical_episode_key`]) is
/// reconstructed; a scene doc missing/mistyping any of `character`/`season`/
/// `episode` earns `E-META-MISSING`/`E-META-PARSE` from the per-file
/// `check()` and this project-wide pass must never fabricate a degenerate
/// key (e.g. `.s00ep00`) for it, or unrelated malformed docs would cascade
/// into a bogus dup report.
fn scene_identity(doc: &Document) -> Option<SceneIdentity> {
    let value: serde_yaml::Value = serde_yaml::from_str(&doc.meta.raw_yaml).ok()?;
    let serde_yaml::Value::Mapping(map) = value else {
        return None;
    };
    let key = |k: &str| serde_yaml::Value::String(k.to_string());
    if let Some(raw) = map.get(key("id")).and_then(|v| v.as_str()) {
        return is_valid_scene_id_raw(raw).then(|| SceneIdentity {
            key: raw.to_string(),
            anchor: "id",
        });
    }
    let character = map.get(key("character"))?.as_str()?.to_string();
    if character.is_empty() {
        return None;
    }
    let season = map.get(key("season"))?.as_i64()?;
    let episode = map.get(key("episode"))?.as_i64()?;
    let episode_id = map
        .get(key("episodeId"))
        .and_then(|v| v.as_str())
        .map(String::from);
    Some(SceneIdentity {
        key: canonical_episode_key(&character, season, episode, episode_id.as_deref()),
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
pub fn scene_key_set(docs: &[(PathBuf, Document)]) -> BTreeMap<String, Vec<(PathBuf, Span)>> {
    let mut by_key: BTreeMap<String, Vec<(PathBuf, Span)>> = BTreeMap::new();
    for (path, doc) in docs {
        if resolve_doc_kind(&doc.meta).0 != Some(DocKind::Scene) {
            continue;
        }
        let Some(SceneIdentity { key, anchor }) = scene_identity(doc) else {
            continue;
        };
        let span = meta_key_span(&doc.meta, anchor);
        by_key.entry(key).or_default().push((path.clone(), span));
    }
    by_key
}

/// One scene document's canonical key — [`scene_key_set`]'s identity for a
/// single document; `None` for a non-scene or a scene whose identity cannot
/// be read.
pub fn scene_key(doc: &Document) -> Option<String> {
    if resolve_doc_kind(&doc.meta).0 != Some(DocKind::Scene) {
        return None;
    }
    scene_identity(doc).map(|s| s.key)
}

/// A quest or lore document's authored, well-formed `id:` (dsl 0.19.0
/// §2.1), read off the raw frontmatter under the same `[A-Za-z0-9_.-]+` gate
/// as a scene's (a rejected id contributes nothing; its own `E-META-ID`
/// anchors). Without one the document has no id in the shared namespace —
/// its fallback index key (the first declared quest/entry id) is not a
/// document id.
pub fn bundle_id(doc: &Document) -> Option<String> {
    let serde_yaml::Value::Mapping(map) =
        serde_yaml::from_str::<serde_yaml::Value>(&doc.meta.raw_yaml).ok()?
    else {
        return None;
    };
    map.get(serde_yaml::Value::String("id".to_string()))?
        .as_str()
        .filter(|raw| is_valid_scene_id_raw(raw))
        .map(str::to_string)
}

/// Every bundle beat's canonical id (dsl 0.23.0 §4, `<document id>.<beat
/// id>`) in `docs`, anchored at the beat's `id` — the keys `visited()`
/// resolves beside the scene keys. A lore document without a well-formed
/// `id:`, or a beat whose id is not an identifier, contributes nothing (its
/// own `E-BEAT-ATTR` anchors); a beat id repeated within one document counts
/// once (the per-file check reports the repeat).
pub fn bundle_beat_key_set(docs: &[(PathBuf, Document)]) -> BTreeMap<String, Vec<(PathBuf, Span)>> {
    let mut by_key: BTreeMap<String, Vec<(PathBuf, Span)>> = BTreeMap::new();
    for (path, doc) in docs {
        if doc.beats.is_empty() || resolve_doc_kind(&doc.meta).0 != Some(DocKind::Lore) {
            continue;
        }
        let Some(doc_id) = bundle_id(doc) else {
            continue;
        };
        let mut seen = BTreeSet::new();
        for beat in &doc.beats {
            if !crate::lore::is_entry_ident(&beat.id) || !seen.insert(beat.id.as_str()) {
                continue;
            }
            by_key
                .entry(crate::bundles::bundle_beat_key(&doc_id, &beat.id))
                .or_default()
                .push((path.clone(), beat.id_span));
        }
    }
    by_key
}

/// dsl 0.25.0 §3: the `after=` (raw text + value span) of the bundle beat
/// whose canonical id is `key`, declared in `doc` — its first declaration,
/// as [`bundle_beat_key_set`] anchors it.
pub fn bundle_beat_after<'d>(doc: &'d Document, key: &str) -> Option<&'d (String, Span)> {
    bundle_beat(doc, key)?.after.as_ref()
}

/// The first `<beat>` of `doc` whose canonical id is `key`.
fn bundle_beat<'d>(doc: &'d Document, key: &str) -> Option<&'d lute_syntax::ast::BundleBeat> {
    let doc_id = bundle_id(doc)?;
    let beat_id = key.strip_prefix(doc_id.as_str())?.strip_prefix('.')?;
    doc.beats.iter().find(|b| b.id == beat_id)
}

/// dsl 0.25.0 §3: every beat node of `graph` (a scene beat or a bundle
/// beat) with no `after` whose `when` has top-level `visited('<id>')`
/// conjuncts naming a node of the graph — each such conjunct gates the beat
/// but draws no edge. `lute scenario` lists these on its unanchored list and
/// suggests moving them to `after=` / `after:`. The ids come in source
/// order; an unparseable `when` (the per-file check's) contributes nothing.
pub fn when_visited_unanchored(
    docs: &[(PathBuf, Document)],
    graph: &ConnGraph,
) -> Vec<(NodeId, Vec<String>)> {
    let by_path: BTreeMap<&Path, &Document> = docs.iter().map(|(p, d)| (p.as_path(), d)).collect();
    let mut out = Vec::new();
    for (id, info) in &graph.nodes {
        if !matches!(info.prereq, PrereqState::Absent) {
            continue;
        }
        let Some(doc) = by_path.get(info.path.as_path()) else {
            continue;
        };
        let when = match id {
            NodeId::Scene(_) => scene_frontmatter_str(doc, "when"),
            NodeId::Beat(key) => bundle_beat(doc, key)
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
fn visited_conjuncts(e: &cel_parser::ast::Expr, out: &mut Vec<String>) {
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
fn document_id_set(docs: &[(PathBuf, Document)]) -> BTreeMap<String, Vec<(PathBuf, Span)>> {
    let mut by_id: BTreeMap<String, Vec<(PathBuf, Span)>> = BTreeMap::new();
    for (path, doc) in docs {
        let (key, anchor) = match resolve_doc_kind(&doc.meta).0 {
            Some(DocKind::Scene) => match scene_identity(doc) {
                Some(SceneIdentity { key, anchor }) => (key, anchor),
                None => continue,
            },
            Some(DocKind::Quest | DocKind::Lore) => match bundle_id(doc) {
                Some(id) => (id, "id"),
                None => continue,
            },
            None => continue,
        };
        by_id
            .entry(key)
            .or_default()
            .push((path.clone(), meta_key_span(&doc.meta, anchor)));
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
pub fn check_conn_episode_dup(docs: &[(PathBuf, Document)]) -> Vec<(PathBuf, Diagnostic)> {
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

/// `E-CONN-UNKNOWN-NODE` (dsl §2.3/§4.1 §A): an `after` prerequisite
/// formula's `visited(K)`/`completed(Q)`/`active(Q)` atom names a node that
/// does not exist anywhere in the project — `K` is not a key in
/// [`scene_key_set`], or
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

/// The raw `after:` frontmatter shape read straight off a scene doc's YAML
/// mapping — the SAME ad-hoc lookup [`scene_identity`] uses (not `TypedMeta`;
/// see its own doc comment on why this project-wide pass never builds one).
/// Distinguishes an ABSENT key from a PRESENT-but-non-string one (Task 5
/// review-2 fix): `.as_str()` alone collapsed both into `None`, so a
/// malformed `after: 42` was silently classified the same as no `after` at
/// all — see [`SceneAfter`].
enum SceneAfter {
    /// No `after:` key at all (or the frontmatter itself failed to parse /
    /// wasn't a mapping) — a valid entry node.
    Absent,
    /// `after:` present and its YAML value IS a string (possibly empty).
    String(String),
    /// `after:` present but its YAML value is NOT a string (int/bool/seq/
    /// map/null) — malformed, must classify as `PrereqState::Invalid`.
    NonString,
}

fn scene_after(doc: &Document) -> SceneAfter {
    let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&doc.meta.raw_yaml) else {
        return SceneAfter::Absent;
    };
    let serde_yaml::Value::Mapping(map) = value else {
        return SceneAfter::Absent;
    };
    match map.get(serde_yaml::Value::String("after".to_string())) {
        None => SceneAfter::Absent,
        Some(serde_yaml::Value::String(s)) => SceneAfter::String(s.clone()),
        Some(_) => SceneAfter::NonString,
    }
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
    lute_manifest::suggest::nearest(needle, candidates, max_dist)
}

/// Exact-lookup every atom flattened out of `formula` (T1 [`atoms`]) against
/// `key_set` (`Atom::Visited`) / `quest_ids` (`Atom::Completed`,
/// `Atom::Active` — both name a QUEST, lang 0.8.0); a miss
/// pushes one [`E_CONN_UNKNOWN_NODE`] anchored at `span` — the SOURCE
/// formula's span (the scene's `after:` key span, or the quest's
/// `after_span`), never a synthetic per-atom location (`PrereqFormula`
/// carries none). `quest_after`: the formula is a `<quest after=…>` — its
/// miss says how to drop `after=` instead (dsl 0.24.0 §2).
fn check_formula_atoms(
    formula: &PrereqFormula,
    span: Span,
    path: &Path,
    key_set: &SceneKeys<'_>,
    quest_ids: &BTreeSet<String>,
    quest_after: bool,
    out: &mut Vec<(PathBuf, Diagnostic)>,
) {
    let hint = quest_after.then_some(QUEST_AFTER_HINT);
    for atom in atoms(formula) {
        // `completed`/`active` (lang 0.8.0) BOTH name a quest and both resolve
        // against the SAME declared-quest set — `active(Q)` is a weaker CLAIM
        // about `Q`, never a weaker existence requirement. Only the function
        // name quoted back in the message differs.
        let (id, func) = match &atom {
            Atom::Visited(key) => {
                check_scene_key(key, "dsl §2.3/§4.1", span, path, key_set, hint, out);
                continue;
            }
            Atom::Completed(id) => (id, "completed"),
            Atom::Active(id) => (id, "active"),
        };
        if !quest_ids.contains(id) {
            let mut message =
                format!("unknown node: no quest declares id `{id}` (`{func}`, dsl §2.3/§4.1)");
            if let Some(sugg) = nearest_match(id, quest_ids.iter().map(String::as_str), 2) {
                message.push_str(&format!(" — did you mean `{sugg}`?"));
            }
            if let Some(hint) = hint {
                message.push_str(hint);
            }
            out.push((path.to_path_buf(), unknown_node_diag(message, span)));
        }
    }
}

/// The tail of an unresolvable `<quest after=…>` miss (dsl 0.24.0 §2): a
/// quest has no `when`, and an accept-driven quest needs no `after=` to be
/// drawn — it is anchored at every `::accept` of it.
const QUEST_AFTER_HINT: &str = "; if the quest is taken on by `::accept`, drop `after=` — an \
     accept-driven quest is anchored at every document that accepts it (dsl 0.24.0 §2)";

/// One `visited(K)` target against the project's scene and bundle beat keys
/// (a bundle beat is a graph node, dsl 0.24.0 §2): a miss is
/// [`E_CONN_UNKNOWN_NODE`] at `span`, with a "did you mean" when a key is
/// close and `hint` appended when given. `cite` names the surface the call
/// came from.
///
/// A miss is NOT reported when the key set is incomplete
/// ([`SceneKeys::complete`]): some document in the root has a frontmatter that
/// does not parse, so its id is unreadable and `K` may well be it. That
/// document already fails with `E-META-PARSE`; reporting every reference to
/// it as unknown would cascade one YAML slip into errors in files that are
/// fine (0.21.1 T3-8, seven F3).
fn check_scene_key(
    key: &str,
    cite: &str,
    span: Span,
    path: &Path,
    key_set: &SceneKeys<'_>,
    hint: Option<&str>,
    out: &mut Vec<(PathBuf, Diagnostic)>,
) {
    if key_set.keys.contains_key(key) || key_set.bundles.contains_key(key) || !key_set.complete {
        return;
    }
    let mut message = format!(
        "unknown node: no scene or bundle beat resolves to key `{key}` (`visited`, {cite})"
    );
    let candidates = key_set
        .keys
        .keys()
        .chain(key_set.bundles.keys())
        .map(String::as_str);
    if let Some(sugg) = nearest_match(key, candidates, 2) {
        message.push_str(&format!(" — did you mean `{sugg}`?"));
    }
    if let Some(hint) = hint {
        message.push_str(hint);
    }
    out.push((path.to_path_buf(), unknown_node_diag(message, span)));
}

/// The project's scene keys ([`scene_key_set`]) plus whether they are all of
/// them: `complete` is false when some document's frontmatter does not parse
/// ([`crate::meta::frontmatter_parses`]) — a scene whose id cannot be read.
struct SceneKeys<'a> {
    keys: &'a BTreeMap<String, Vec<(PathBuf, Span)>>,
    /// dsl 0.23.0 §4: the bundle beat canonical ids ([`bundle_beat_key_set`]).
    bundles: BTreeMap<String, Vec<(PathBuf, Span)>>,
    complete: bool,
}

/// dsl 0.21.0 §7a.1: every `visited('<scene id>')` call in a condition slot
/// of `doc` — the body's CEL slots (quest `start` / `fail`, objective
/// `done`, entry `when`, line / branch `when=`, `<when test>`, …) and a
/// scene beat's frontmatter `when:` — resolved against the scene keys
/// exactly as an `after:` `visited` atom is. Each slot is re-parsed here
/// (the project walk holds no CEL arena); an unparseable slot is the
/// per-file check's `E-CEL-PARSE`, never re-reported.
fn check_visited_calls(
    doc: &Document,
    path: &Path,
    key_set: &SceneKeys<'_>,
    out: &mut Vec<(PathBuf, Diagnostic)>,
) {
    let check_raw = |raw: &str, span: Span, out: &mut Vec<(PathBuf, Diagnostic)>| {
        let mut arena = lute_cel::CelArena::default();
        let Some(root) =
            lute_cel::parse_slot_marked_refs(&mut arena, raw).and_then(|h| arena.get(h).cloned())
        else {
            return;
        };
        for key in crate::cel_resolve::visited_targets(&root.expr) {
            check_scene_key(&key, "dsl 0.21.0 §7a.1", span, path, key_set, None, out);
        }
    };
    lute_syntax::walk::for_each_cel_slot(doc, &mut |slot| {
        if slot.kind == CelKind::Condition && slot.raw.contains(crate::cel_resolve::VISITED_FN) {
            check_raw(&slot.raw, slot.span, out);
        }
    });
    if resolve_doc_kind(&doc.meta).0 == Some(DocKind::Scene) {
        if let Some(when) = scene_frontmatter_str(doc, "when") {
            if when.contains(crate::cel_resolve::VISITED_FN) {
                check_raw(&when, crate::beats::top_value_span(&doc.meta, "when"), out);
            }
        }
    }
}

/// A top-level string value of a scene's frontmatter (the [`scene_after`]
/// lookup, for keys whose non-string shape another pass owns).
fn scene_frontmatter_str(doc: &Document, key: &str) -> Option<String> {
    let value: serde_yaml::Value = serde_yaml::from_str(&doc.meta.raw_yaml).ok()?;
    value.get(key)?.as_str().map(str::to_string)
}

/// Resolve every `after` prerequisite formula in `docs` — BOTH surfaces
/// (dsl §2.1): a scene document's frontmatter `after:` key, AND every
/// `<quest after="…">` attribute (a quest pack declares its prerequisite
/// there instead) — against the known project node sets, and every
/// condition slot's `visited('<scene id>')` call (dsl 0.21.0 §7a.1) against
/// the scene keys. `key_set` (T3
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
    let key_set = &SceneKeys {
        keys: key_set,
        bundles: bundle_beat_key_set(docs),
        complete: docs
            .iter()
            .all(|(_, doc)| crate::meta::frontmatter_parses(&doc.meta)),
    };
    let mut out = Vec::new();
    for (path, doc) in docs {
        if resolve_doc_kind(&doc.meta).0 == Some(DocKind::Scene) {
            if let SceneAfter::String(after) = scene_after(doc) {
                let after_span = meta_key_span(&doc.meta, "after");
                let (formula, _) = parse_prereq(&after, after_span);
                if let Some(formula) = formula {
                    check_formula_atoms(
                        &formula, after_span, path, key_set, quest_ids, false, &mut out,
                    );
                }
            }
        }
        for quest in &doc.quests {
            if let Some(after) = &quest.after {
                let (formula, _) = parse_prereq(after, quest.after_span);
                if let Some(formula) = formula {
                    check_formula_atoms(
                        &formula,
                        quest.after_span,
                        path,
                        key_set,
                        quest_ids,
                        true,
                        &mut out,
                    );
                }
            }
        }
        // dsl 0.25.0 §3: a bundle beat's `after=`, like a scene's `after:`.
        for beat in &doc.beats {
            let Some((after, span)) = beat.after.as_ref().filter(|(a, _)| !a.is_empty()) else {
                continue;
            };
            if let Some(formula) = parse_prereq(after, *span).0 {
                check_formula_atoms(&formula, *span, path, key_set, quest_ids, false, &mut out);
            }
        }
        check_visited_calls(doc, path, key_set, &mut out);
    }
    out
}

/// A graph node identity (connectivity layer, Task 5): a scene's canonical
/// [`scene_key_set`] identity key and a quest's `<quest id>` are SEPARATE
/// namespaces — dsl §2.3 imposes no cross-kind uniqueness, so the SAME
/// string can legitimately name both a scene and a quest at once.
/// [`ConnGraph`] keys on this typed identity rather than a bare `String`,
/// which would silently collide the two into one node.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum NodeId {
    /// `visited(K)` target: `K` is a [`scene_key_set`] canonical key.
    Scene(String),
    /// `completed(Q)`/`active(Q)` target: `Q` is a `<quest id>` that
    /// declares `after`, is anchored ([`PrereqState::Anchored`]), or is the
    /// source of an anchor (a subquest parent, a `quest.Q.state` start
    /// conjunct, an accepting quest body). Any other quest is never a
    /// [`ConnGraph`] node, see [`assemble_graph`]. BOTH lifecycle atoms
    /// resolve to the same node; the atom they came from is recorded
    /// separately as an [`EdgeKind`] (lang 0.8.0).
    Quest(String),
    /// A bundle beat (dsl 0.23.0 §4), keyed `<document id>.<beat id>`
    /// ([`bundle_beat_key_set`]). A bundle beat declares no `after`, so it is
    /// always an entry node: its occasion presents it whenever eligible. It
    /// is a legal predecessor (dsl 0.24.0 §2): `visited(K)` in an `after`
    /// targets it when no scene has key `K` (the two share one id
    /// namespace, `E-CONN-EPISODE-ID-DUP`), and an `::accept` in its body
    /// anchors the accepted quest.
    Beat(String),
    /// dsl 0.25.0 §4: a lore entry `X` some quest's `start` reads as
    /// `entry.X.everRead` — the source of that [`EdgeKind::Start`] edge. An
    /// entry node has no `after` (the engine presents an entry whenever it
    /// chooses), so it is always an entry point; only anchoring entries are
    /// nodes. Keyed by entry id, anchored at the entry's declaration.
    Entry(String),
}

impl NodeId {
    /// The node a `visited(K)` atom names in `nodes`: the scene `K`, else
    /// the bundle beat `K` (dsl 0.24.0 §2); the scene id when neither is a
    /// node (an unknown key, `E-CONN-UNKNOWN-NODE`'s).
    pub fn visited<V>(key: &str, nodes: &BTreeMap<NodeId, V>) -> NodeId {
        let beat = NodeId::Beat(key.to_string());
        if !nodes.contains_key(&NodeId::Scene(key.to_string())) && nodes.contains_key(&beat) {
            beat
        } else {
            NodeId::Scene(key.to_string())
        }
    }

    /// The node `atom` names in `nodes` ([`Self::visited`] for `visited`).
    pub fn of_atom<V>(atom: &Atom, nodes: &BTreeMap<NodeId, V>) -> NodeId {
        match atom {
            Atom::Visited(key) => NodeId::visited(key, nodes),
            Atom::Completed(id) | Atom::Active(id) => NodeId::Quest(id.clone()),
        }
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeId::Scene(key) => write!(f, "scene({key})"),
            NodeId::Quest(id) => write!(f, "quest({id})"),
            NodeId::Beat(key) => write!(f, "beat({key})"),
            NodeId::Entry(id) => write!(f, "entry({id})"),
        }
    }
}

/// One [`ConnGraph`] node: its identity, the file it was declared in, its
/// parsed `after` prerequisite state ([`PrereqState`] — `Absent` for a
/// scene/quest with no `after:` key at all, `Valid` for one whose CEL text
/// parsed, `Invalid` for one present-but-malformed —
/// [`crate::prereq::E_CONN_PROFILE`] already reports the malformed case once,
/// from T2's per-file `check()`; only `Absent`/`Invalid` nodes here
/// contribute no incoming edges; `Anchored` for a quest without `after`
/// anchored by its tree, `start` or `::accept`s), and the span this node is
/// anchored at for diagnostics (a scene's `character:` key span — the SAME
/// span [`scene_key_set`] stores; a quest's or entry's `id_span`).
#[derive(Clone, Debug)]
pub struct NodeInfo {
    pub id: NodeId,
    pub path: PathBuf,
    pub prereq: PrereqState,
    pub span: Span,
}

/// The resolved state of a node's `after` prerequisite (Task 5 review fix):
/// `Option<PrereqFormula>` conflated an ABSENT `after` (a valid entry node)
/// with a PRESENT-but-malformed one (`parse_prereq` returning `None`) — both
/// collapsed to `None`, so downstream reachability/envelope passes (Task
/// 6/10) could not tell "no prerequisite" from "unparseable prerequisite"
/// and would silently treat a malformed doc as a clean entry node.
#[derive(Clone, Debug)]
pub enum PrereqState {
    /// No `after` key/attribute declared at all — a valid entry node.
    Absent,
    /// `after` present and [`parse_prereq`] resolved it.
    Valid(PrereqFormula),
    /// `after` present but [`parse_prereq`] returned `None` (malformed CEL,
    /// already reported once as `E-CONN-PROFILE` by T2's per-file `check()`).
    Invalid,
    /// dsl 0.24.0 §2, 0.25.0 §4: no `after` declared on a quest, which is
    /// anchored by what the project says about it instead — each
    /// [`Anchor`] one necessary condition of its activation, in the order
    /// subquest, `start` conjuncts, `::accept`s. Synthesized, never authored
    /// (so never `E-CONN-PROFILE`/`-UNKNOWN-NODE` material). It never proves
    /// the quest `Unreachable`: the engine may accept a quest outside any
    /// `::accept` (dsl 0.21.0 §7a.3, 0.25.0 §5), and `after` stays the
    /// declared route, so a dead anchor reads `Unknown`.
    Anchored(Vec<Anchor>),
}

/// One synthesized prerequisite of a quest that declares no `after` (dsl
/// 0.24.0 §2, 0.25.0 §4): the quest cannot activate before one of `from`
/// is reached. Drawn as `from -> quest` edges of `kind`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Anchor {
    /// [`EdgeKind::Subquest`] (the parent, active before its child),
    /// [`EdgeKind::Start`] (one top-level `start` conjunct), or
    /// [`EdgeKind::Accept`] (every node whose body `::accept`s the quest).
    pub kind: EdgeKind,
    /// The alternatives, in source order — every one a graph node.
    pub from: Vec<NodeId>,
}

impl PrereqState {
    /// Every node this prerequisite names, for `lute scenario reach`'s
    /// referenced list: each atom's target of an authored formula (a target
    /// may be no node — an undeclared id, a plain quest), every anchor
    /// source of an anchored quest.
    pub fn referenced<V>(&self, nodes: &BTreeMap<NodeId, V>) -> BTreeSet<NodeId> {
        match self {
            PrereqState::Valid(f) => atoms(f).iter().map(|a| NodeId::of_atom(a, nodes)).collect(),
            PrereqState::Anchored(anchors) => anchors
                .iter()
                .flat_map(|a| a.from.iter().cloned())
                .collect(),
            PrereqState::Absent | PrereqState::Invalid => BTreeSet::new(),
        }
    }
}

/// Every edge in a [`ConnGraph`] is also tagged with the ATOM it came from
/// (lang 0.8.0, [`ConnGraph::edge_kinds`]). The tag is presentation- and
/// envelope-relevant only: the DAG itself treats all three identically —
/// each says "the prerequisite node must be reached before the dependent
/// one" — so reachability ([`check_reachability`]) and cycle detection
/// ([`E_CONN_CYCLE`]) are deliberately blind to it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum EdgeKind {
    /// From a `visited(K)` atom; the prerequisite is a [`NodeId::Scene`].
    Visited,
    /// From a `completed(Q)` atom; the prerequisite quest reached `complete`.
    Completed,
    /// From an `active(Q)` atom (lang 0.8.0); the prerequisite quest reached
    /// `active` — STRICTLY WEAKER than [`Self::Completed`], and the
    /// difference is load-bearing in [`crate::envelope`], which may not
    /// assume the quest's completion writes landed.
    Active,
    /// dsl 0.24.0 §2: an accept anchor — the prerequisite node's body
    /// `::accept`s the dependent accept-driven quest ([`PrereqState::Anchored`]).
    Accept,
    /// dsl 0.25.0 §4: a `start` anchor — a top-level conjunct of the
    /// dependent quest's `start` reads the prerequisite: `visited(K)`,
    /// `entry.X.everRead`, or `quest.Y.state == …`.
    Start,
    /// dsl 0.25.0 §4: the prerequisite quest is the dependent's subquest
    /// parent (`<objective quest=…>`) — the child is never active before it.
    Subquest,
}

impl EdgeKind {
    /// The stable lowercase token naming this edge kind — the SAME text the
    /// source atom's function uses, so `lute scenario`'s text/dot/json views
    /// can print it without re-deriving a second vocabulary.
    pub fn as_str(self) -> &'static str {
        match self {
            EdgeKind::Visited => "visited",
            EdgeKind::Completed => "completed",
            EdgeKind::Active => "active",
            EdgeKind::Accept => "accept",
            EdgeKind::Start => "start",
            EdgeKind::Subquest => "subquest",
        }
    }
}

impl fmt::Display for EdgeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The [`EdgeKind`] an [`Atom`] contributes.
fn atom_edge_kind(atom: &Atom) -> EdgeKind {
    match atom {
        Atom::Visited(_) => EdgeKind::Visited,
        Atom::Completed(_) => EdgeKind::Completed,
        Atom::Active(_) => EdgeKind::Active,
    }
}

/// The project-wide topological-precedence DAG (dsl §2.4 graph 1): every
/// scene plus every `after`-declaring quest as a node, a flattened
/// `prerequisite -> dependent` edge per formula atom that targets another
/// graph node, and `topo_order`, a deterministic Kahn's-algorithm ordering
/// (ties broken by [`NodeId`]'s own `Ord`). Per-node cycle recovery (spec
/// §4.1): `topo_order` contains every node that is NOT on or downstream of a
/// prerequisite cycle — a cycle member never reaches in-degree 0 and is
/// omitted, as is anything transitively downstream of one. The exclusion is
/// PER-NODE, not per-root: cycle-independent nodes keep their slots and their
/// sound verdicts even when [`assemble_graph`] also reported
/// [`E_CONN_CYCLE`]. A node's ABSENCE from `topo_order` (equivalently, from
/// the `reach`/`envs` maps built over it) is the per-node cyclic/downstream
/// signal; downstream consumers degrade conservatively on it, never trust a
/// verdict they cannot derive.
#[derive(Clone, Debug, Default)]
pub struct ConnGraph {
    pub nodes: BTreeMap<NodeId, NodeInfo>,
    pub edges: BTreeMap<NodeId, BTreeSet<NodeId>>,
    /// Which atom kind(s) justify each `prerequisite -> dependent` edge in
    /// [`Self::edges`] (lang 0.8.0), keyed identically to `edges` so a
    /// renderer walking `edges` can look the kinds up by reference, without
    /// materializing a key. A SET, not a single kind: one formula may
    /// reference the same quest through both `active(Q)` and `completed(Q)`
    /// (`active("q") || completed("q")`), and collapsing that to one kind
    /// would silently drop the stronger or the weaker justification. The two
    /// maps are built in one pass and stay in exact correspondence: every
    /// `edges` pair has a nonempty entry here and vice versa.
    pub edge_kinds: BTreeMap<NodeId, BTreeMap<NodeId, BTreeSet<EdgeKind>>>,
    pub topo_order: Vec<NodeId>,
}

impl ConnGraph {
    /// The [`EdgeKind`]s justifying the `from -> to` edge, or `None` when no
    /// such edge exists. Sorted by [`EdgeKind`]'s own `Ord` (a `BTreeSet`), so
    /// every renderer built on it is deterministic for free.
    pub fn edge_kinds_for(&self, from: &NodeId, to: &NodeId) -> Option<&BTreeSet<EdgeKind>> {
        self.edge_kinds.get(from).and_then(|by_dep| by_dep.get(to))
    }
}

/// dsl §2.4 (graph 1) / §4.1 (§A cycle): the topological-precedence DAG over
/// scenes + `after`-declaring quests contains a directed cycle — no
/// evaluation order can satisfy every `after` clause simultaneously.
pub const E_CONN_CYCLE: &str = "E-CONN-CYCLE";

/// Construct an [`E_CONN_CYCLE`] diagnostic. Public so the CLI project gate
/// (`lute-cli`'s `project_gate_result`) can reuse the SAME constructor/code to
/// synthesize a TARGET-anchored cycle diagnostic for a target that is on or
/// downstream of a cycle but whose own file carries no anchored diagnostic
/// (spec §5 — the gate decides by topological-order exclusion).
pub fn cycle_diag(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: E_CONN_CYCLE.to_string(),
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

/// Assemble the project-wide [`ConnGraph`] (dsl §2.4 graph 1) and detect any
/// `after`-precedence cycle (`E-CONN-CYCLE`, §4.1 §A).
///
/// Node/edge model (Task 5 spec):
/// - **Nodes**: every scene ([`scene_key_set`]'s `key_set`, as
///   `NodeId::Scene`) PLUS every quest that declares an `after` attribute
///   (`NodeId::Quest`) PLUS every quest without `after` that a subquest
///   parent, a `start` conjunct or an accepting node's `::accept` anchors
///   (dsl 0.24.0 §2, 0.25.0 §4, [`PrereqState::Anchored`]) PLUS every
///   source of such an anchor and every subquest parent (a quest, or an
///   entry a `start` reads as `entry.X.everRead`, `NodeId::Entry`; an entry
///   point unless anchored itself) — any other quest is NEVER a node — PLUS
///   every bundle beat (`NodeId::Beat`).
/// - **Edges**: flattened, over-approximating (ignoring `&&`/`||` position)
///   — for each atom `p` in node `n`'s formula, add `p -> n` IFF `p` is
///   itself a node in this graph. `visited(K)` targets `NodeId::Scene(K)`,
///   else the bundle beat `NodeId::Beat(K)` ([`NodeId::visited`]);
///   `completed(Q)` and `active(Q)` (lang 0.8.0) BOTH target
///   `NodeId::Quest(Q)`, ONLY when `Q` is itself a quest node — either
///   lifecycle atom on a plain quest is a LEAF dependency (Task 6's
///   quest-lifecycle signal, never a DAG edge here). The two are
///   structurally IDENTICAL edges; which atom produced each is recorded
///   out-of-band in [`ConnGraph::edge_kinds`]. Synthesized edges —
///   [`EdgeKind::Subquest`] parent → child (every child, `after` or not),
///   then [`EdgeKind::Start`], then [`EdgeKind::Accept`] anchors — are
///   added after the authored ones and only where they close no cycle.
///
/// `key_set` (T3 [`scene_key_set`]) is supplied by the caller — computed
/// once per resolved project root (`lute-cli`'s `by_root` grouping), same
/// convention as [`resolve_nodes`]. `quest_ids` (T4 [`quest_id_set`]) is
/// accepted for call-site symmetry with [`resolve_nodes`] but is NOT
/// consulted here (Task 5 review fix): quest-node ADMISSION is decided from
/// `docs` alone — gating it on the (potentially stale/filtered) `quest_ids`
/// set could silently drop a quest, and its edges/cycles, from the graph.
/// A `completed(Q)` EDGE target resolves via plain `nodes` membership,
/// never `quest_ids` either.
/// An unknown atom target (neither a scene key nor a declared quest id at
/// all) is [`E_CONN_UNKNOWN_NODE`]'s problem (T4's [`resolve_nodes`]), not
/// this pass's — it simply contributes no edge here.
pub fn assemble_graph(
    docs: &[(PathBuf, Document)],
    key_set: &BTreeMap<String, Vec<(PathBuf, Span)>>,
    _quest_ids: &BTreeSet<String>,
) -> (ConnGraph, Vec<(PathBuf, Diagnostic)>) {
    let by_path: BTreeMap<&Path, &Document> = docs.iter().map(|(p, d)| (p.as_path(), d)).collect();

    let mut nodes: BTreeMap<NodeId, NodeInfo> = BTreeMap::new();

    // Scene nodes: every canonical key T3 resolved, anchored at its FIRST
    // occurrence (a same-key repeat past that is E-CONN-EPISODE-ID-DUP's own
    // problem, T3 -- never this pass's).
    for (key, occurrences) in key_set {
        let Some((path, span)) = occurrences.first() else {
            continue;
        };
        let prereq =
            by_path
                .get(path.as_path())
                .map_or(PrereqState::Absent, |doc| match scene_after(doc) {
                    SceneAfter::Absent => PrereqState::Absent,
                    SceneAfter::NonString => PrereqState::Invalid,
                    SceneAfter::String(after) if after.is_empty() => PrereqState::Absent,
                    SceneAfter::String(after) => {
                        let after_span = meta_key_span(&doc.meta, "after");
                        match parse_prereq(&after, after_span).0 {
                            Some(f) => PrereqState::Valid(f),
                            None => PrereqState::Invalid,
                        }
                    }
                });
        nodes.insert(
            NodeId::Scene(key.clone()),
            NodeInfo {
                id: NodeId::Scene(key.clone()),
                path: path.clone(),
                prereq,
                span: *span,
            },
        );
    }

    // Bundle beat nodes: every canonical beat key, anchored at its first
    // occurrence (a repeat is E-CONN-EPISODE-ID-DUP's problem). dsl 0.25.0
    // §3: a beat's `after=` is its prerequisite, exactly as a scene's
    // `after:`; without one it is an entry point.
    for (key, occurrences) in bundle_beat_key_set(docs) {
        let Some((path, span)) = occurrences.into_iter().next() else {
            continue;
        };
        let prereq = match by_path
            .get(path.as_path())
            .and_then(|doc| bundle_beat_after(doc, &key))
        {
            None => PrereqState::Absent,
            Some((after, _)) if after.is_empty() => PrereqState::Absent,
            Some((after, after_span)) => match parse_prereq(after, *after_span).0 {
                Some(f) => PrereqState::Valid(f),
                None => PrereqState::Invalid,
            },
        };
        let id = NodeId::Beat(key);
        nodes.insert(
            id.clone(),
            NodeInfo {
                id,
                path,
                prereq,
                span,
            },
        );
    }

    // Quest nodes: EVERY nonempty-`after`-declaring quest (dsl §2.1's second
    // `after` surface) is admitted as a node, full stop -- regardless of the
    // caller-supplied `_quest_ids` set (Task 5 review fix: that set may be
    // stale/filtered relative to `docs`; gating SOURCE node admission on it
    // could silently drop a quest -- and its edges/cycles -- from the graph).
    for (path, doc) in docs {
        for quest in &doc.quests {
            let Some(after) = &quest.after else { continue };
            if quest.id.is_empty() {
                continue;
            }
            let prereq = if after.is_empty() {
                PrereqState::Absent
            } else {
                match parse_prereq(after, quest.after_span).0 {
                    Some(f) => PrereqState::Valid(f),
                    None => PrereqState::Invalid,
                }
            };
            nodes.insert(
                NodeId::Quest(quest.id.clone()),
                NodeInfo {
                    id: NodeId::Quest(quest.id.clone()),
                    path: path.clone(),
                    prereq,
                    span: quest.id_span,
                },
            );
        }
    }

    // Anchored quest nodes (dsl 0.24.0 §2, 0.25.0 §4): a quest with no
    // `after` on any declaration is anchored by its subquest parent, its
    // `start` conjuncts and the nodes whose body `::accept`s it. Each source
    // quest or entry — and every subquest parent — joins the graph too, an
    // entry point unless anchored itself. Their edges go in below, after the
    // authored ones.
    let decls = QuestDecls::new(docs);
    for (id, anchors) in quest_anchors(docs, &decls, &nodes) {
        let Some(&(path, quest)) = decls.first.get(id.as_str()) else {
            continue;
        };
        let node = NodeId::Quest(id);
        nodes.insert(
            node.clone(),
            NodeInfo {
                id: node,
                path: path.to_path_buf(),
                prereq: PrereqState::Anchored(anchors),
                span: quest.id_span,
            },
        );
    }
    let sources: Vec<NodeId> = nodes
        .values()
        .filter_map(|info| match &info.prereq {
            PrereqState::Anchored(anchors) => Some(anchors),
            _ => None,
        })
        .flatten()
        .flat_map(|a| a.from.iter().cloned())
        .chain(decls.parents.values().map(|p| NodeId::Quest(p.to_string())))
        .collect();
    for source in sources {
        if nodes.contains_key(&source) {
            continue;
        }
        let (path, span) = match &source {
            NodeId::Quest(id) => match decls.first.get(id.as_str()) {
                Some(&(path, quest)) => (path, quest.id_span),
                None => continue,
            },
            NodeId::Entry(id) => match decls.entries.get(id.as_str()) {
                Some(&(path, span)) => (path, span),
                None => continue,
            },
            NodeId::Scene(_) | NodeId::Beat(_) => continue,
        };
        nodes.insert(
            source.clone(),
            NodeInfo {
                id: source,
                path: path.to_path_buf(),
                prereq: PrereqState::Absent,
                span,
            },
        );
    }

    // Edges: flattened union of atoms per formula -- `atom_target -> n` iff
    // `atom_target` is itself a node above (never a bare-string cross-check
    // against key_set/quest_ids -- membership in `nodes`, typed, is the only
    // question here). Each edge also records the ATOM KIND(s) that justify it
    // (lang 0.8.0 `EdgeKind`) -- `completed(Q)` and `active(Q)` both land on
    // the same `NodeId::Quest(Q)` and are otherwise indistinguishable once
    // flattened, but the envelope pass and `lute scenario` both need to tell
    // them apart. The DAG shape itself is UNCHANGED by the kind: an `active`
    // edge constrains ordering exactly as a `completed` edge does, so cycle
    // detection below stays exactly as strong.
    let mut edges: BTreeMap<NodeId, BTreeSet<NodeId>> = BTreeMap::new();
    let mut edge_kinds: BTreeMap<NodeId, BTreeMap<NodeId, BTreeSet<EdgeKind>>> = BTreeMap::new();
    for info in nodes.values() {
        let PrereqState::Valid(formula) = &info.prereq else {
            continue;
        };
        for atom in atoms(formula) {
            let target = NodeId::of_atom(&atom, &nodes);
            if nodes.contains_key(&target) {
                add_edge(
                    &mut edges,
                    &mut edge_kinds,
                    target,
                    &info.id,
                    atom_edge_kind(&atom),
                );
            }
        }
    }
    // Synthesized edges go in after every authored one — subquest, then
    // `start`, then `::accept` — and each only where it closes no cycle: a
    // source downstream of the quest it anchors (a scene whose `after` needs
    // the quest) cannot be the quest's way in, and an anchor is no `after`
    // clause for `E-CONN-CYCLE` to report. Such an anchor stays on the node;
    // reachability reads it `Unknown` (it is never ordered before the
    // quest). A child that declares its own `after` still hangs off its
    // parent (dsl 0.25.0 §4).
    for (child, parent) in &decls.parents {
        let (from, to) = (
            NodeId::Quest(parent.to_string()),
            NodeId::Quest(child.to_string()),
        );
        if nodes.contains_key(&from) && nodes.contains_key(&to) && !reaches(&edges, &to, &from) {
            add_edge(&mut edges, &mut edge_kinds, from, &to, EdgeKind::Subquest);
        }
    }
    for kind in [EdgeKind::Start, EdgeKind::Accept] {
        for info in nodes.values() {
            let PrereqState::Anchored(anchors) = &info.prereq else {
                continue;
            };
            for from in anchors
                .iter()
                .filter(|a| a.kind == kind)
                .flat_map(|a| &a.from)
            {
                if nodes.contains_key(from) && !reaches(&edges, &info.id, from) {
                    add_edge(&mut edges, &mut edge_kinds, from.clone(), &info.id, kind);
                }
            }
        }
    }

    let mut diags = Vec::new();
    detect_conn_cycles(&nodes, &edges, &mut diags);

    // Per-node cycle recovery (spec §4.1): build the order UNCONDITIONALLY.
    // Kahn's algorithm below never frees a cycle member (its in-degree never
    // reaches 0) nor anything transitively downstream of one, so those nodes
    // are simply omitted — every cycle-INDEPENDENT node keeps its slot and a
    // sound verdict. `detect_conn_cycles` above still emits `E-CONN-CYCLE`;
    // we only stop blanking the whole root's order.
    let topo_order = topo_sort(&nodes, &edges);

    (
        ConnGraph {
            nodes,
            edges,
            edge_kinds,
            topo_order,
        },
        diags,
    )
}

/// Record the `from -> to` edge with `kind` in both [`ConnGraph`] maps.
fn add_edge(
    edges: &mut BTreeMap<NodeId, BTreeSet<NodeId>>,
    edge_kinds: &mut BTreeMap<NodeId, BTreeMap<NodeId, BTreeSet<EdgeKind>>>,
    from: NodeId,
    to: &NodeId,
    kind: EdgeKind,
) {
    edges.entry(from.clone()).or_default().insert(to.clone());
    edge_kinds
        .entry(from)
        .or_default()
        .entry(to.clone())
        .or_default()
        .insert(kind);
}

/// Whether `to` is `from` or reachable from it along `edges`.
fn reaches(edges: &BTreeMap<NodeId, BTreeSet<NodeId>>, from: &NodeId, to: &NodeId) -> bool {
    let mut seen = BTreeSet::new();
    let mut stack = vec![from];
    while let Some(n) = stack.pop() {
        if n == to {
            return true;
        }
        if seen.insert(n) {
            stack.extend(edges.get(n).into_iter().flatten());
        }
    }
    false
}

/// The quests and entries of one resolved root as [`assemble_graph`]
/// anchors them (dsl 0.24.0 §2, 0.25.0 §4).
struct QuestDecls<'a> {
    /// Each quest id's first declaration, with its document.
    first: BTreeMap<&'a str, (&'a Path, &'a Quest)>,
    /// Every quest id some declaration gives an `after` — its declared
    /// route replaces the synthesized anchors.
    with_after: BTreeSet<&'a str>,
    /// Declared subquest child → the first declared parent whose
    /// `<objective quest=…>` names it. A self-reference is
    /// `E-QUEST-TREE-CYCLE`'s and makes no child here.
    parents: BTreeMap<&'a str, &'a str>,
    /// Each lore entry id's first declaration: its document and id span.
    entries: BTreeMap<&'a str, (&'a Path, Span)>,
}

impl<'a> QuestDecls<'a> {
    fn new(docs: &'a [(PathBuf, Document)]) -> Self {
        let mut first: BTreeMap<&str, (&Path, &Quest)> = BTreeMap::new();
        let mut with_after = BTreeSet::new();
        let mut entries = BTreeMap::new();
        for (path, doc) in docs {
            for quest in doc.quests.iter().filter(|q| !q.id.is_empty()) {
                first
                    .entry(quest.id.as_str())
                    .or_insert((path.as_path(), quest));
                if quest.after.is_some() {
                    with_after.insert(quest.id.as_str());
                }
            }
            for entry in doc.entries.iter().filter(|e| !e.id.is_empty()) {
                entries
                    .entry(entry.id.as_str())
                    .or_insert((path.as_path(), entry.id_span));
            }
        }
        let mut parents = BTreeMap::new();
        for (_, doc) in docs {
            for quest in doc.quests.iter().filter(|q| !q.id.is_empty()) {
                for node in &quest.body {
                    let Node::Objective(o) = node else { continue };
                    if let Some(child) = o.quest.as_deref() {
                        if let Some((&child, _)) = first.get_key_value(child) {
                            if child != quest.id {
                                parents.entry(child).or_insert(quest.id.as_str());
                            }
                        }
                    }
                }
            }
        }
        QuestDecls {
            first,
            with_after,
            parents,
            entries,
        }
    }
}

/// The [`PrereqState::Anchored`] anchors of every quest that declares no
/// `after` (dsl 0.24.0 §2, 0.25.0 §4), in the order subquest parent,
/// `start` conjuncts ([`start_anchors`]), `::accept`s ([`accept_sources`]).
/// A quest none of them anchors is absent. `nodes` holds the scene and
/// bundle beat nodes the sources resolve against.
fn quest_anchors(
    docs: &[(PathBuf, Document)],
    decls: &QuestDecls<'_>,
    nodes: &BTreeMap<NodeId, NodeInfo>,
) -> BTreeMap<String, Vec<Anchor>> {
    let mut accepts = accept_sources(docs, nodes);
    let mut out = BTreeMap::new();
    for (&id, &(_, quest)) in &decls.first {
        if decls.with_after.contains(id) {
            continue;
        }
        let mut anchors = Vec::new();
        if let Some(parent) = decls.parents.get(id) {
            anchors.push(Anchor {
                kind: EdgeKind::Subquest,
                from: vec![NodeId::Quest(parent.to_string())],
            });
        }
        anchors.extend(start_anchors(quest, decls, nodes));
        if let Some(from) = accepts.remove(id) {
            anchors.push(Anchor {
                kind: EdgeKind::Accept,
                from,
            });
        }
        if !anchors.is_empty() {
            out.insert(id.to_string(), anchors);
        }
    }
    out
}

/// dsl 0.25.0 §4: one [`EdgeKind::Start`] anchor per top-level `&&`
/// conjunct of `quest`'s `start` that reads a graph node — `visited('K')`
/// (a scene or bundle beat node), `entry.X.everRead` (bare or `== true`, a
/// declared entry) or `quest.Y.state == '<state>'` (a declared quest other
/// than `quest`, any state but `unset`) — or is an `||` of such reads (one
/// anchor, several sources). Every other conjunct gates without anchoring;
/// an unparseable `start` is the per-file check's and anchors nothing.
fn start_anchors(
    quest: &Quest,
    decls: &QuestDecls<'_>,
    nodes: &BTreeMap<NodeId, NodeInfo>,
) -> Vec<Anchor> {
    let Some(start) = &quest.start else {
        return Vec::new();
    };
    let mut arena = lute_cel::CelArena::default();
    let Some(root) = lute_cel::parse_slot_marked_refs(&mut arena, &start.raw)
        .and_then(|h| arena.get(h).cloned())
    else {
        return Vec::new();
    };
    let mut conjuncts = Vec::new();
    top_conjuncts(&root.expr, &mut conjuncts);
    conjuncts
        .into_iter()
        .filter_map(|c| start_sources(c, &quest.id, decls, nodes))
        .map(|from| Anchor {
            kind: EdgeKind::Start,
            from,
        })
        .collect()
}

/// The top-level `&&` conjuncts of `e`, in order.
fn top_conjuncts<'e>(e: &'e Expr, out: &mut Vec<&'e Expr>) {
    if let Expr::Call(c) = e {
        if c.func_name == op::LOGICAL_AND && c.target.is_none() && c.args.len() == 2 {
            top_conjuncts(&c.args[0].expr, out);
            top_conjuncts(&c.args[1].expr, out);
            return;
        }
    }
    out.push(e);
}

/// The sources one `start` conjunct anchors at ([`start_anchors`]): the
/// node it reads, or every source of an `||` whose every arm reads one.
fn start_sources(
    e: &Expr,
    own: &str,
    decls: &QuestDecls<'_>,
    nodes: &BTreeMap<NodeId, NodeInfo>,
) -> Option<Vec<NodeId>> {
    if let Expr::Call(c) = e {
        if c.func_name == op::LOGICAL_OR && c.target.is_none() && c.args.len() == 2 {
            let mut from = start_sources(&c.args[0].expr, own, decls, nodes)?;
            for n in start_sources(&c.args[1].expr, own, decls, nodes)? {
                if !from.contains(&n) {
                    from.push(n);
                }
            }
            return Some(from);
        }
    }
    start_source(e, own, decls, nodes).map(|n| vec![n])
}

/// The graph node one anchoring read names (see [`start_anchors`]).
fn start_source(
    e: &Expr,
    own: &str,
    decls: &QuestDecls<'_>,
    nodes: &BTreeMap<NodeId, NodeInfo>,
) -> Option<NodeId> {
    let entry = |e: &Expr| {
        let path = crate::cel_paths::select_path(e)?;
        let id = crate::cel_paths::reserved_entry_id(&path)?;
        (crate::cel_paths::is_entry_ever_read(&path) && decls.entries.contains_key(id))
            .then(|| NodeId::Entry(id.to_string()))
    };
    let quest_state = |path: &Expr, state: &Expr| {
        let Expr::Literal(Val::String(state)) = state else {
            return None;
        };
        let path = crate::cel_paths::select_path(path)?;
        if !crate::cel_paths::is_reserved_quest_state(&path) || state.as_str() == "unset" {
            return None;
        }
        let id = path.split('.').nth(1)?;
        (id != own && decls.first.contains_key(id)).then(|| NodeId::Quest(id.to_string()))
    };
    let Expr::Call(c) = e else {
        return entry(e);
    };
    if let Some(key) = crate::cel_resolve::visited_call_target(c) {
        let node = NodeId::visited(key, nodes);
        return nodes.contains_key(&node).then_some(node);
    }
    if c.func_name != op::EQUALS || c.target.is_some() || c.args.len() != 2 {
        return None;
    }
    let (a, b) = (&c.args[0].expr, &c.args[1].expr);
    if matches!(b, Expr::Literal(Val::Boolean(true))) {
        return entry(a);
    }
    quest_state(a, b).or_else(|| quest_state(b, a))
}

/// dsl 0.24.0 §2: per accept-driven quest
/// ([`crate::accept::accept_driven_quests`]), every graph node whose body
/// `::accept`s it, in document order — the accepting scene, the accepting
/// bundle beat, or the accepting quest (which must be active for its body
/// to run). A lore entry's `::accept` anchors nothing (the entry is no
/// source until a `start` reads it); a quest never anchors itself.
fn accept_sources<'a>(
    docs: &'a [(PathBuf, Document)],
    nodes: &BTreeMap<NodeId, NodeInfo>,
) -> BTreeMap<&'a str, Vec<NodeId>> {
    let driven = crate::accept::accept_driven_quests(docs);
    let mut sites: BTreeMap<&str, Vec<NodeId>> = BTreeMap::new();
    let mut record = |d: &lute_syntax::ast::Directive, source: &NodeId, own: Option<&str>| {
        let Some((id, _)) = d.accept_quest() else {
            return;
        };
        let Some(id) = driven.get(id).copied() else {
            return;
        };
        if own == Some(id) {
            return;
        }
        let from = sites.entry(id).or_default();
        if !from.contains(source) {
            from.push(source.clone());
        }
    };
    for (_, doc) in docs {
        if let Some(source) = scene_key(doc)
            .map(NodeId::Scene)
            .filter(|s| nodes.contains_key(s))
        {
            for shot in &doc.shots {
                crate::accept::walk(&shot.body, &mut |d| record(d, &source, None));
            }
        }
        if resolve_doc_kind(&doc.meta).0 == Some(DocKind::Lore) {
            if let Some(doc_id) = bundle_id(doc) {
                for beat in doc
                    .beats
                    .iter()
                    .filter(|b| crate::lore::is_entry_ident(&b.id))
                {
                    let source = NodeId::Beat(crate::bundles::bundle_beat_key(&doc_id, &beat.id));
                    if nodes.contains_key(&source) {
                        crate::accept::walk(&beat.body, &mut |d| record(d, &source, None));
                    }
                }
            }
        }
        for quest in doc.quests.iter().filter(|q| !q.id.is_empty()) {
            let source = NodeId::Quest(quest.id.clone());
            crate::accept::walk(&quest.body, &mut |d| {
                record(d, &source, Some(quest.id.as_str()))
            });
        }
    }
    sites
}

/// A prerequisite reference [`assemble_graph`] does not draw — what `lute
/// scenario` notes beside the graph instead of leaving a missing edge
/// unexplained (dsl 0.23.0 §1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OmittedRef {
    /// `completed(Q)` / `active(Q)` in node `from`'s `after`, where `Q` is a
    /// declared quest that is no graph node (no `after`, no anchor, no
    /// subquest tree).
    Lifecycle {
        from: NodeId,
        kind: EdgeKind,
        quest: String,
    },
    /// `visited('<scene>')` in a lifecycle condition of `quest`, which
    /// declares no `after`, that draws no edge — not a top-level `start`
    /// conjunct (dsl 0.25.0 §4). `slot` names where it is read: `start`,
    /// `fail`, or `objective <id> <done|when|by|until>`. A condition read
    /// gates the quest; it is no anchor, and an `after` copying it would
    /// replace the quest's real anchors (summer S1).
    Visited {
        quest: String,
        scene: String,
        slot: String,
    },
}

/// Every [`OmittedRef`] of one resolved root, graph-node order for
/// [`OmittedRef::Lifecycle`], then document order for
/// [`OmittedRef::Visited`]. `quest_ids` is [`quest_id_set`] over `docs`.
pub fn omitted_refs(
    docs: &[(PathBuf, Document)],
    graph: &ConnGraph,
    quest_ids: &BTreeSet<String>,
) -> Vec<OmittedRef> {
    let mut out = Vec::new();
    for info in graph.nodes.values() {
        let PrereqState::Valid(formula) = &info.prereq else {
            continue;
        };
        for atom in atoms(formula) {
            let kind = atom_edge_kind(&atom);
            let (Atom::Completed(quest) | Atom::Active(quest)) = atom else {
                continue;
            };
            if quest_ids.contains(&quest)
                && !graph.nodes.contains_key(&NodeId::Quest(quest.clone()))
            {
                out.push(OmittedRef::Lifecycle {
                    from: info.id.clone(),
                    kind,
                    quest,
                });
            }
        }
    }
    for (_, doc) in docs {
        for quest in &doc.quests {
            if quest.after.is_some() || quest.id.is_empty() {
                continue;
            }
            let mut slots: Vec<(String, &lute_syntax::ast::CelSlot)> = Vec::new();
            slots.extend(quest.start.iter().map(|s| ("start".to_string(), s)));
            slots.extend(quest.fail.iter().map(|s| ("fail".to_string(), s)));
            for node in &quest.body {
                if let Node::Objective(o) = node {
                    let named = |key: &str| format!("objective {} {key}", o.id);
                    slots.push((named("done"), &o.done));
                    slots.extend(o.when.iter().map(|s| (named("when"), s)));
                    slots.extend(o.by.iter().map(|s| (named("by"), s)));
                    slots.extend(o.until.iter().map(|s| (named("until"), s)));
                }
            }
            for (name, slot) in slots {
                if !slot.raw.contains(crate::cel_resolve::VISITED_FN) {
                    continue;
                }
                let mut arena = lute_cel::CelArena::default();
                let Some(root) = lute_cel::parse_slot_marked_refs(&mut arena, &slot.raw)
                    .and_then(|h| arena.get(h).cloned())
                else {
                    continue;
                };
                for scene in crate::cel_resolve::visited_targets(&root.expr) {
                    let from = NodeId::visited(&scene, &graph.nodes);
                    if graph
                        .edge_kinds_for(&from, &NodeId::Quest(quest.id.clone()))
                        .is_some()
                    {
                        continue;
                    }
                    out.push(OmittedRef::Visited {
                        quest: quest.id.clone(),
                        scene,
                        slot: name.clone(),
                    });
                }
            }
        }
    }
    out
}

/// Detect any directed cycle in `edges`, reporting each as [`E_CONN_CYCLE`]
/// into `diags`. Standard DFS 3-coloring, cloned from
/// `schema_import::detect_cycles` / `dfs_cycle` (`schema_import.rs:784-833`)
/// over [`NodeId`] rather than `PathBuf`. Nodes are visited in [`NodeId`]'s
/// own sorted (`BTreeMap`) order for a deterministic, order-independent
/// result; `edges`' `BTreeSet` targets are already sorted, so (unlike the
/// `Vec`-adjacency precedent) there is no separate neighbor sort step.
fn detect_conn_cycles(
    nodes: &BTreeMap<NodeId, NodeInfo>,
    edges: &BTreeMap<NodeId, BTreeSet<NodeId>>,
    diags: &mut Vec<(PathBuf, Diagnostic)>,
) {
    let mut on_stack: BTreeSet<NodeId> = BTreeSet::new();
    let mut done: BTreeSet<NodeId> = BTreeSet::new();
    let mut stack: Vec<NodeId> = Vec::new();
    for start in nodes.keys() {
        if !done.contains(start) && !on_stack.contains(start) {
            dfs_conn_cycle(
                start,
                nodes,
                edges,
                &mut on_stack,
                &mut done,
                &mut stack,
                diags,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn dfs_conn_cycle(
    node: &NodeId,
    nodes: &BTreeMap<NodeId, NodeInfo>,
    edges: &BTreeMap<NodeId, BTreeSet<NodeId>>,
    on_stack: &mut BTreeSet<NodeId>,
    done: &mut BTreeSet<NodeId>,
    stack: &mut Vec<NodeId>,
    diags: &mut Vec<(PathBuf, Diagnostic)>,
) {
    on_stack.insert(node.clone());
    stack.push(node.clone());
    if let Some(targets) = edges.get(node) {
        for nbr in targets {
            if on_stack.contains(nbr) {
                // Back edge -> cycle: report the chain from `nbr` around to `node`.
                let start_idx = stack.iter().position(|n| n == nbr).unwrap_or(0);
                let chain = stack[start_idx..]
                    .iter()
                    .chain(std::iter::once(nbr))
                    .map(NodeId::to_string)
                    .collect::<Vec<_>>()
                    .join(" -> ");
                // The chain `stack[start_idx..]` is EXACTLY the nodes on this
                // cycle; it is used only to render the diagnostic message. The
                // gate's on/downstream-of-cycle test is decided separately by
                // topological-order exclusion (`node_cycle_degraded`), which
                // is complete where a back-edge stack slice under-approximates.
                let info = nodes
                    .get(nbr)
                    .expect("cycle target must be a graph node -- edges only ever target nodes");
                diags.push((
                    info.path.clone(),
                    cycle_diag(
                        format!("prerequisite cycle: {chain} (dsl §2.4/§4.1 §A)"),
                        info.span,
                    ),
                ));
            } else if !done.contains(nbr) {
                dfs_conn_cycle(nbr, nodes, edges, on_stack, done, stack, diags);
            }
        }
    }
    stack.pop();
    on_stack.remove(node);
    done.insert(node.clone());
}

/// Deterministic Kahn's-algorithm topological sort over `edges`. Runs
/// UNCONDITIONALLY even on a cyclic graph (spec §4.1 per-node cycle
/// recovery): any node that never reaches in-degree 0 — every cycle member
/// and everything transitively downstream of one — is simply omitted from
/// the returned order, never panicked on; every cycle-independent node is
/// still emitted with its prerequisites before it. Ties (multiple
/// zero-in-degree nodes ready at once) break on [`NodeId`]'s own `Ord` via a
/// `BTreeSet` ready queue — independent of `nodes`/`edges`' own insertion
/// order.
fn topo_sort(
    nodes: &BTreeMap<NodeId, NodeInfo>,
    edges: &BTreeMap<NodeId, BTreeSet<NodeId>>,
) -> Vec<NodeId> {
    let mut in_degree: BTreeMap<NodeId, usize> = nodes.keys().map(|id| (id.clone(), 0)).collect();
    for targets in edges.values() {
        for target in targets {
            *in_degree.entry(target.clone()).or_insert(0) += 1;
        }
    }
    let mut ready: BTreeSet<NodeId> = in_degree
        .iter()
        .filter(|&(_, degree)| *degree == 0)
        .map(|(id, _)| id.clone())
        .collect();
    let mut order = Vec::with_capacity(nodes.len());
    while let Some(next) = ready.iter().next().cloned() {
        ready.remove(&next);
        if let Some(targets) = edges.get(&next) {
            for target in targets {
                if let Some(degree) = in_degree.get_mut(target) {
                    *degree -= 1;
                    if *degree == 0 {
                        ready.insert(target.clone());
                    }
                }
            }
        }
        order.push(next);
    }
    order
}

/// Task 6 (dsl §4.1): a graph node's PROVABLE reachability from the
/// project's entry set, computed by memoized structural recursion over
/// each `Valid` `after` formula in [`ConnGraph::topo_order`]. Tri-state,
/// never binary — `Unknown` covers every case this pass cannot PROVE
/// either way (a malformed formula, an atom this pass cannot resolve): the
/// "provable-only, never guess" discipline (design spec §2.4, mirrored
/// from `E-QUEST-UNREACHABLE`) means `Unknown` NEVER collapses to
/// `Reachable`/`Unreachable`, and only `Unreachable` ever earns
/// [`E_CONN_UNREACHABLE`] — a false positive here is strictly worse than a
/// missed one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reachability {
    Reachable,
    Unreachable,
    Unknown,
}

/// dsl §4.1: a graph node has no satisfiable route from the project's
/// entry set — every path to it is provably blocked. UNLIKE
/// §4.2/§4.3/§4.4's envelope diagnostics, this one carries NO "under your
/// declared routes" hedge (design spec §2.6's one named exception): it is
/// a pure fact about the AUTHORED graph's own self-consistency (no route
/// exists in what you declared), never a claim about runtime engine
/// behavior — so the hedge would misrepresent it, not merely soften it.
pub const E_CONN_UNREACHABLE: &str = "E-CONN-UNREACHABLE";

/// dsl §4.1: a defensive cap on one node's `after` formula atom count — a
/// pragmatic guard against a pathological/degenerate formula, not the
/// primary soundness mechanism (the structural recursion itself is
/// already linear in formula size, design spec §2.4). [`MAX_FORMULA_ATOMS`]
/// (256) is generous for any realistic hand-authored `after` clause;
/// crossing it is itself a strong signal something degenerate (e.g.
/// machine-generated) reached the parser.
pub const E_CONN_FORMULA_TOO_COMPLEX: &str = "E-CONN-FORMULA-TOO-COMPLEX";

/// See [`E_CONN_FORMULA_TOO_COMPLEX`].
const MAX_FORMULA_ATOMS: usize = 256;

fn unreachable_diag(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: E_CONN_UNREACHABLE.to_string(),
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

fn too_complex_diag(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: E_CONN_FORMULA_TOO_COMPLEX.to_string(),
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

/// Task 6 (dsl §4.1): PROVABLE per-node reachability from the project's
/// entry set, one memoized pass over [`ConnGraph::topo_order`] — linear in
/// total formula size, no route enumeration (design spec §2.4).
///
/// `quest_ids` is the FULL declared `<quest id>` set for this resolved
/// project root (T4 [`quest_id_set`]) — spec-required so `completed(Q)`
/// consults every declared quest, not merely the `after`-opted-in subset
/// [`ConnGraph::nodes`] admits (Task 6 review): a declared PLAIN (no-`after`)
/// quest that is alive still reads `Reachable`, never a false `Unknown`.
///
/// `ambiguous_quest_ids` is every quest id with MORE THAN ONE declaration in
/// this root (Task 6 review-2) — a duplicate id might carry one dead and one
/// alive declaration (locally via [`unreachable_quest_ids`] OR structurally
/// via this very graph's own node reachability), and neither source can
/// pick the "right" one. Provable-only discipline demands `Unknown` for it,
/// checked BEFORE both the lifecycle and graph-reach checks below — so an
/// ambiguous id's OWN graph node (if one of its declarations opted into
/// `after`) still gets its own real memoized reachability in the returned
/// map, but every OTHER formula's `completed(Q)` reference to it reads
/// `Unknown` regardless.
///
/// `unreachable_quests` is the caller-supplied set of quest ids that are
/// THEMSELVES provably unable to complete (dsl 0.4.0 §5.3's
/// `E-QUEST-UNREACHABLE`/`E-OBJECTIVE-UNSATISFIABLE` signal) — a
/// `completed(Q)` atom reads `Q`'s own quest-lifecycle reachability, a
/// DIFFERENT engine from this graph's, so it is threaded in rather than
/// recomputed here (see [`unreachable_quest_ids`] for the real
/// project-wide extraction `lute-cli` wires in).
///
/// Recursion over each `Valid` formula (design spec §4.1, widened to
/// three-valued per the provable-only discipline):
/// - [`PrereqState::Absent`] ⇒ [`Reachability::Reachable`] — an absent
///   `after` is a graph ENTRY point, trivially reachable.
/// - [`PrereqState::Invalid`] ⇒ [`Reachability::Unknown`] — a malformed
///   formula already earns `E-CONN-PROFILE` once (T1); this pass must
///   never additionally GUESS reachable or unreachable for it.
/// - [`PrereqState::Valid`]`(f)`, recursing over `f`:
///   - `visited(Y)`: `Y` a known [`NodeId::Scene`] (else bundle beat,
///     [`NodeId::visited`]) ⇒ its own (already
///     memoized, since it always precedes this node in `topo_order`)
///     reachability; not a known node ⇒ `Unknown` (that miss is
///     `E-CONN-UNKNOWN-NODE`'s problem, T4 — it must never CASCADE into a
///     false `E-CONN-UNREACHABLE`).
///   - `completed(Q)` AND `active(Q)` (lang 0.8.0 — IDENTICAL here; see
///     below for why the weaker atom earns no weaker verdict), by
///     precedence:
///     1. `Q ∉ quest_ids` (undeclared) ⇒ `Unknown`.
///     2. `Q ∈ ambiguous_quest_ids` (>1 declaration) ⇒ `Unknown`.
///     3. `Q ∈ unreachable_quests` ⇒ `Unreachable` (quest lifecycle,
///        tracked OUTSIDE this graph).
///     4. `NodeId::Quest(Q) ∈ nodes` (an `after`-declaring quest already
///        memoized above) ⇒ its memoized reachability (TRANSITIVE).
///     5. else (a declared PLAIN quest, not unreachable) ⇒ `Reachable`.
///
///     Case 3 is the only one where `active` could conceivably be weaker,
///     and it is not: `unreachable_quests` carries
///     [`crate::reachability::E_QUEST_UNREACHABLE`], whose BOTH roots
///     (dsl 0.4.0 §5.3) already preclude the `active` state — a `start`
///     that decides false never activates the quest at all, and a `fail`
///     that decides true fails it at the first evaluation instant (0.2
///     §6.3 precedence), so it is never observably `active` either.
///     `Unreachable` is therefore PROVEN for `active(Q)` too, not guessed.
///   - `And`: `Unreachable` iff EITHER arm is `Unreachable` (checked
///     first — it dominates); else `Reachable` iff BOTH `Reachable`; else
///     `Unknown`.
///   - `Or`: `Reachable` iff EITHER arm is `Reachable` (checked first —
///     it dominates, even against an `Unreachable` other arm); else
///     `Unreachable` iff BOTH `Unreachable`; else `Unknown`.
///
/// - [`PrereqState::Anchored`]`(anchors)` ⇒ the `And` over the anchors of
///   the `Or` over each anchor's sources (a quest source through the same
///   ambiguous / `unreachable_quests` precedence), except that
///   `Unreachable` reads `Unknown` (see [`PrereqState::Anchored`]).
///
/// A node whose formula's flattened atom count exceeds
/// [`MAX_FORMULA_ATOMS`] earns [`E_CONN_FORMULA_TOO_COMPLEX`] instead of
/// being evaluated at all (its own reachability is `Unknown`).
///
/// [`E_CONN_UNREACHABLE`] fires ONLY for a node this pass computes
/// `Unreachable` — never for `Unknown` (provable-only, never a false
/// positive). A node missing from `topo_order` (graph has a cycle,
/// [`E_CONN_CYCLE`] already reported by T5) gets no reachability entry
/// and no diagnostic here.
pub fn check_reachability(
    g: &ConnGraph,
    quest_ids: &BTreeSet<String>,
    ambiguous_quest_ids: &BTreeSet<String>,
    unreachable_quests: &BTreeSet<String>,
) -> (BTreeMap<NodeId, Reachability>, Vec<(PathBuf, Diagnostic)>) {
    let mut reach: BTreeMap<NodeId, Reachability> = BTreeMap::new();
    let mut diags = Vec::new();

    for id in &g.topo_order {
        let Some(info) = g.nodes.get(id) else {
            continue;
        };
        let r = match &info.prereq {
            PrereqState::Absent => Reachability::Reachable,
            PrereqState::Invalid => Reachability::Unknown,
            PrereqState::Valid(f) => {
                let count = atoms(f).len();
                if count > MAX_FORMULA_ATOMS {
                    diags.push((
                        info.path.clone(),
                        too_complex_diag(
                            format!(
                                "{id}'s `after` formula has {count} atoms, over the \
                                 {MAX_FORMULA_ATOMS}-atom complexity cap"
                            ),
                            info.span,
                        ),
                    ));
                    Reachability::Unknown
                } else {
                    eval_reach(
                        f,
                        g,
                        &reach,
                        quest_ids,
                        ambiguous_quest_ids,
                        unreachable_quests,
                    )
                }
            }
            // Synthesized anchors never prove a quest dead: the engine may
            // accept it outside any `::accept` (dsl 0.24.0 §2), and `after`
            // stays the declared route (dsl 0.25.0 §4). Every anchor must
            // hold; one source of each suffices.
            PrereqState::Anchored(anchors) => {
                let source = |n: &NodeId| match n {
                    NodeId::Quest(q) if ambiguous_quest_ids.contains(q) => Reachability::Unknown,
                    NodeId::Quest(q) if unreachable_quests.contains(q) => Reachability::Unreachable,
                    _ => reach.get(n).copied().unwrap_or(Reachability::Unknown),
                };
                let all = anchors.iter().fold(Reachability::Reachable, |all, a| {
                    let any = a
                        .from
                        .iter()
                        .fold(Reachability::Unreachable, |any, n| or_reach(any, source(n)));
                    and_reach(all, any)
                });
                match all {
                    Reachability::Unreachable => Reachability::Unknown,
                    r => r,
                }
            }
        };
        if r == Reachability::Unreachable {
            diags.push((
                info.path.clone(),
                unreachable_diag(
                    format!("{id} has no satisfiable route from the project's entry set"),
                    info.span,
                ),
            ));
        }
        reach.insert(id.clone(), r);
    }

    (reach, diags)
}

/// Recurse [`check_reachability`]'s tri-state lattice directly over `f`'s
/// AST shape (never route enumeration — see [`check_reachability`]'s doc
/// comment for the full per-case rules). `reach` holds every node already
/// memoized earlier in `topo_order`.
fn eval_reach(
    f: &PrereqFormula,
    g: &ConnGraph,
    reach: &BTreeMap<NodeId, Reachability>,
    quest_ids: &BTreeSet<String>,
    ambiguous_quest_ids: &BTreeSet<String>,
    unreachable_quests: &BTreeSet<String>,
) -> Reachability {
    match f {
        PrereqFormula::Visited(key) => {
            let target = NodeId::visited(key, &g.nodes);
            if g.nodes.contains_key(&target) {
                reach.get(&target).copied().unwrap_or(Reachability::Unknown)
            } else {
                Reachability::Unknown
            }
        }
        // Graph semantics are IDENTICAL for both quest-lifecycle atoms — see
        // `check_reachability`'s doc comment for why `unreachable_quests`
        // soundly covers `active(Q)` too (lang 0.8.0).
        PrereqFormula::Completed(id) | PrereqFormula::Active(id) => {
            // Unknown for two distinct reasons, same verdict: the id names no
            // declared quest at all, or it names more than one (ambiguous), so
            // no single node's reachability can answer for it.
            if !quest_ids.contains(id) || ambiguous_quest_ids.contains(id) {
                Reachability::Unknown
            } else if unreachable_quests.contains(id) {
                Reachability::Unreachable
            } else if g.nodes.contains_key(&NodeId::Quest(id.clone())) {
                reach
                    .get(&NodeId::Quest(id.clone()))
                    .copied()
                    .unwrap_or(Reachability::Unknown)
            } else {
                Reachability::Reachable
            }
        }
        PrereqFormula::And(l, r) => and_reach(
            eval_reach(
                l,
                g,
                reach,
                quest_ids,
                ambiguous_quest_ids,
                unreachable_quests,
            ),
            eval_reach(
                r,
                g,
                reach,
                quest_ids,
                ambiguous_quest_ids,
                unreachable_quests,
            ),
        ),
        PrereqFormula::Or(l, r) => or_reach(
            eval_reach(
                l,
                g,
                reach,
                quest_ids,
                ambiguous_quest_ids,
                unreachable_quests,
            ),
            eval_reach(
                r,
                g,
                reach,
                quest_ids,
                ambiguous_quest_ids,
                unreachable_quests,
            ),
        ),
    }
}

fn and_reach(a: Reachability, b: Reachability) -> Reachability {
    if a == Reachability::Unreachable || b == Reachability::Unreachable {
        Reachability::Unreachable
    } else if a == Reachability::Reachable && b == Reachability::Reachable {
        Reachability::Reachable
    } else {
        Reachability::Unknown
    }
}

fn or_reach(a: Reachability, b: Reachability) -> Reachability {
    if a == Reachability::Reachable || b == Reachability::Reachable {
        Reachability::Reachable
    } else if a == Reachability::Unreachable && b == Reachability::Unreachable {
        Reachability::Unreachable
    } else {
        Reachability::Unknown
    }
}

/// Every declared `<quest id>` occurring in MORE THAN ONE `<quest>`
/// declaration across `docs` (Task 6 review-2): a shared id already earns
/// its own `E-QUEST-ID-DUP` elsewhere, but [`check_reachability`]'s
/// `completed(Q)` needs this set SEPARATELY — an ambiguous id might carry
/// one dead declaration and one alive one (either locally, via
/// [`unreachable_quest_ids`], or structurally, via one declaration's own
/// opted-in graph-node reachability), and neither signal alone can pick
/// the "right" declaration. Provable-only discipline: `completed(Q)` for
/// an ambiguous `Q` is always `Unknown`, never guessed either way. An
/// empty id is skipped (that document's own `E-QUEST-ID-MISSING` problem).
pub fn ambiguous_quest_ids(docs: &[(PathBuf, Document)]) -> BTreeSet<String> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for (_, document) in docs {
        for quest in &document.quests {
            if quest.id.is_empty() {
                continue;
            }
            *counts.entry(quest.id.clone()).or_insert(0) += 1;
        }
    }
    counts
        .into_iter()
        .filter(|(_, count)| *count > 1)
        .map(|(id, _)| id)
        .collect()
}

/// T6/T7 wiring: every `<quest>` in `docs` whose declaration was flagged
/// [`crate::reachability::E_QUEST_UNREACHABLE`] (dsl 0.4.0 §5.3) by the
/// per-file `check()` pass on that SAME file — the exact set
/// [`check_reachability`] expects as its `unreachable_quests` parameter.
///
/// Matched by `Quest.span` (the diagnostic's own anchor —
/// `reachability.rs`'s `check_quest_reach` pushes `E-QUEST-UNREACHABLE` at
/// `quest.span` verbatim) rather than id text alone: two DIFFERENT quests
/// sharing an id (a separate `E-QUEST-ID-DUP` problem) must never be
/// conflated by this lookup. This span correspondence is exact (not a
/// heuristic) because `docs` and `file_results` are both derived from
/// parsing the SAME source text — a deterministic parse yields identical
/// byte spans every time.
///
/// A quest id with MORE THAN ONE declaration in `docs` ([`ambiguous_quest_ids`],
/// Task 6 review-2) is OMITTED here entirely, even when the SPECIFIC
/// matched declaration is itself flagged `E-QUEST-UNREACHABLE`: collapsing
/// per-declaration spans down to the shared string id would otherwise
/// wrongly mark a quest id "provably unreachable" when a DIFFERENT
/// declaration of that same id is alive — provable-only means an ambiguous
/// id's lifecycle is `Unknown`, never `Unreachable`, here.
///
/// `file_results` is the caller's own per-file `check()` output; a path in
/// `docs` this pass has no matching entry for (or a quest whose id is
/// empty — that quest's own `E-QUEST-ID-MISSING` problem) contributes
/// nothing, never a panic.
pub fn unreachable_quest_ids(
    docs: &[(PathBuf, Document)],
    file_results: &[(PathBuf, CheckResult)],
) -> BTreeSet<String> {
    let ambiguous = ambiguous_quest_ids(docs);
    let mut out = BTreeSet::new();
    for (path, document) in docs {
        let Some((_, result)) = file_results.iter().find(|(p, _)| p == path) else {
            continue;
        };
        for quest in &document.quests {
            if quest.id.is_empty() || ambiguous.contains(&quest.id) {
                continue;
            }
            let flagged = result.diagnostics.iter().any(|d| {
                d.code == crate::reachability::E_QUEST_UNREACHABLE && d.span == quest.span
            });
            if flagged {
                out.insert(quest.id.clone());
            }
        }
    }
    out
}

/// dsl 0.4.0 §4.2/§B: every relation name with at least one `::assert{R(…)}`
/// site inside a node this root's [`check_reachability`] pass did NOT prove
/// [`Reachability::Unreachable`] — the relation-level projection of
/// [`live_assert_sites`], read by `producible()`'s base case (c) (spec §4.2's
/// "a node that is `E-CONN-UNREACHABLE`-clean").
///
/// Callers MUST pre-scope `docs`/`reach`/`ambiguous_quest_ids`/
/// `unreachable_quests` to ONE resolved project root (`lute-cli`'s `by_root`
/// grouping) — an assert site in one root can never seed a relation in a
/// sibling root's `producible()` walk.
pub fn live_assert_relations(
    docs: &[(PathBuf, Document)],
    reach: &BTreeMap<NodeId, Reachability>,
    ambiguous_quest_ids: &BTreeSet<String>,
    unreachable_quests: &BTreeSet<String>,
) -> BTreeSet<String> {
    live_assert_sites(docs, reach, ambiguous_quest_ids, unreachable_quests)
        .into_iter()
        .filter(|(_, a)| !a.pattern.relation.is_empty())
        .map(|(_, a)| a.pattern.relation.clone())
        .collect()
}

/// Every `::assert` site, with its document, inside a node this root's
/// [`check_reachability`] pass did NOT prove [`Reachability::Unreachable`] —
/// the reachability gate both `producible()` (relation names,
/// [`live_assert_relations`]) and the dsl 0.20.0 may set
/// (`crate::fact_env::MaySet`, ground facts) seed from. `Reachable` AND
/// `Unknown` both count: provable-only discipline demands an impossibility
/// be a PROVEN fact before a guard over it is flagged dead, so a node this
/// pass cannot resolve either way (`Unknown`, OR one this graph has no entry
/// for at all — e.g. inside an `E-CONN-CYCLE`, or a scene whose identity
/// triad this pass could not even compute) must never be treated as dead by
/// omission; only a node PROVABLY `Unreachable` excludes its assert sites.
///
/// A scene assert site's hosting `NodeId::Scene` is its own
/// [`canonical_episode_key`] (every scene is always a `ConnGraph` node —
/// `Absent` `after` included, so `reach` always has an entry UNLESS the
/// graph itself had a cycle). A quest-body assert site's hosting
/// `NodeId::Quest` mirrors [`check_reachability`]'s own `completed(Q)`
/// precedence (T6): ambiguous (2+ declarations) reads `Unknown`; a
/// caller-supplied `E-QUEST-UNREACHABLE` id reads `Unreachable`; an
/// `after`-declaring quest already has a memoized `reach` entry; a plain
/// (no-`after`) quest not otherwise dead defaults `Reachable`. A lore
/// entry's assert site (dsl 0.19.0 §4) has no hosting node at all — lore
/// documents are not part of the scene/quest graph, and the engine presents
/// an entry whenever it chooses — so it is never proven `Unreachable` and
/// always counts as live.
///
/// Parse-failed asserts (D13's empty-relation sentinel) are included; each
/// consumer skips them. Same root-scoping contract as
/// [`live_assert_relations`].
pub fn live_assert_sites<'d>(
    docs: &'d [(PathBuf, Document)],
    reach: &BTreeMap<NodeId, Reachability>,
    ambiguous_quest_ids: &BTreeSet<String>,
    unreachable_quests: &BTreeSet<String>,
) -> Vec<(&'d Path, &'d Assert)> {
    let mut out = Vec::new();
    for (path, doc) in docs {
        let mut sites = Vec::new();
        if resolve_doc_kind(&doc.meta).0 == Some(DocKind::Scene) {
            let node_reach =
                scene_identity(doc).and_then(|ident| reach.get(&NodeId::Scene(ident.key)).copied());
            if assert_site_is_live(node_reach) {
                for shot in &doc.shots {
                    collect_asserts(&shot.body, &mut sites);
                }
            }
        }
        for quest in &doc.quests {
            let node_reach = if ambiguous_quest_ids.contains(&quest.id) {
                Some(Reachability::Unknown)
            } else if unreachable_quests.contains(&quest.id) {
                Some(Reachability::Unreachable)
            } else {
                Some(
                    reach
                        .get(&NodeId::Quest(quest.id.clone()))
                        .copied()
                        .unwrap_or(Reachability::Reachable),
                )
            };
            if assert_site_is_live(node_reach) {
                collect_asserts(&quest.body, &mut sites);
            }
        }
        for entry in &doc.entries {
            collect_asserts(&entry.body, &mut sites);
        }
        for beat in &doc.beats {
            collect_asserts(&beat.body, &mut sites);
        }
        out.extend(sites.into_iter().map(|a| (path.as_path(), a)));
    }
    out
}

/// Every `::assert{R(…)}` relation name, per document — the producer half of
/// the producer→consumer edge `lute scenario`'s facts section renders
/// (#15, T4.7).
///
/// Unlike [`live_assert_relations`] this applies NO reachability gate: it
/// answers "which file writes this relation", a question about the source
/// rather than about the route, so an assert on a currently-dead route is
/// still the answer to "where do I go to change this". Callers MUST pre-scope
/// `docs` to one resolved root.
pub fn assert_relations_per_doc(
    docs: &[(PathBuf, Document)],
) -> BTreeMap<PathBuf, BTreeSet<String>> {
    let mut out = BTreeMap::new();
    for (path, doc) in docs {
        let mut sites = Vec::new();
        for shot in &doc.shots {
            collect_asserts(&shot.body, &mut sites);
        }
        for quest in &doc.quests {
            collect_asserts(&quest.body, &mut sites);
        }
        for entry in &doc.entries {
            collect_asserts(&entry.body, &mut sites);
        }
        for beat in &doc.beats {
            collect_asserts(&beat.body, &mut sites);
        }
        let rels: BTreeSet<String> = sites
            .into_iter()
            .filter(|a| !a.pattern.relation.is_empty())
            .map(|a| a.pattern.relation.clone())
            .collect();
        if !rels.is_empty() {
            out.insert(path.clone(), rels);
        }
    }
    out
}

/// See [`live_assert_relations`]: `None` (identity/graph unresolvable — a
/// malformed scene triad, or a node absent from a cyclic graph's `reach`
/// map) counts as live, exactly like `Some(Reachability::Unknown)` — only a
/// PROVEN [`Reachability::Unreachable`] excludes.
fn assert_site_is_live(r: Option<Reachability>) -> bool {
    !matches!(r, Some(Reachability::Unreachable))
}

/// Recursively collect every `::assert` site of a node stream — mirrors
/// `reachability.rs`'s `walk_reach` recursion shape (match-arm /
/// branch-choice / hub-choice / on / objective bodies). Also the producer
/// half of `lute scenario knowledge` (dsl 0.23.0 §1).
pub fn collect_asserts<'d>(nodes: &'d [Node], out: &mut Vec<&'d Assert>) {
    for node in nodes {
        match node {
            Node::Assert(a) => out.push(a),
            Node::Match(m) => {
                for arm in &m.arms {
                    let body = match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => body,
                    };
                    collect_asserts(body, out);
                }
            }
            Node::Branch(b) => {
                for choice in &b.choices {
                    collect_asserts(&choice.body, out);
                }
            }
            Node::Hub(h) => {
                for choice in &h.choices {
                    collect_asserts(&choice.body, out);
                }
            }
            Node::On(o) => collect_asserts(&o.body, out),
            Node::Objective(o) => collect_asserts(&o.body, out),
            Node::Line(_)
            | Node::Directive(_)
            | Node::Set(_)
            | Node::Timeline(_)
            | Node::Retract(_) => {}
        }
    }
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
            entries: Vec::new(),
            beats: Vec::new(),
            span: span(0),
        }
    }

    #[test]
    fn identical_pair_in_same_root_is_dup() {
        let raw = "kind: scene\ncharacter: marina\nseason: 1\nepisode: 1\n";
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
                doc("kind: scene\ncharacter: marina\nseason: 1\nepisode: 1\n"),
            ),
            (
                PathBuf::from("b.lute"),
                doc("kind: scene\ncharacter: marina\nseason: 1\nepisode: 2\n"),
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
