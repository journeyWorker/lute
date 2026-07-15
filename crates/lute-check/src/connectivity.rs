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

use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{Arm, Document, Node};

use crate::check::CheckResult;
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
            if let SceneAfter::String(after) = scene_after(doc) {
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
    /// `completed(Q)` target: `Q` is an `after`-declaring `<quest id>` (a
    /// plain, no-`after` quest is never a [`ConnGraph`] node — see
    /// [`assemble_graph`]).
    Quest(String),
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NodeId::Scene(key) => write!(f, "scene({key})"),
            NodeId::Quest(id) => write!(f, "quest({id})"),
        }
    }
}

/// One [`ConnGraph`] node: its identity, the file it was declared in, its
/// parsed `after` prerequisite state ([`PrereqState`] — `Absent` for a
/// scene/quest with no `after:` key at all, `Valid` for one whose CEL text
/// parsed, `Invalid` for one present-but-malformed —
/// [`crate::prereq::E_CONN_PROFILE`] already reports the malformed case once,
/// from T2's per-file `check()`; only `Absent`/`Invalid` nodes here
/// contribute no outgoing edges), and the span this node is anchored at for
/// diagnostics (a scene's `character:` key span — the SAME span
/// [`scene_key_set`] stores; a quest's `id_span`).
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
    pub topo_order: Vec<NodeId>,
}

/// dsl §2.4 (graph 1) / §4.1 (§A cycle): the topological-precedence DAG over
/// scenes + `after`-declaring quests contains a directed cycle — no
/// evaluation order can satisfy every `after` clause simultaneously.
pub const E_CONN_CYCLE: &str = "E-CONN-CYCLE";

fn cycle_diag(message: String, span: Span) -> Diagnostic {
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
///   (`NodeId::Quest`) — a quest with no `after` is NEVER a node.
/// - **Edges**: flattened, over-approximating (ignoring `&&`/`||` position)
///   — for each atom `p` in node `n`'s formula, add `p -> n` IFF `p` is
///   itself a node in this graph. `visited(K)` targets `NodeId::Scene(K)`;
///   `completed(Q)` targets `NodeId::Quest(Q)` ONLY when `Q` is itself an
///   `after`-declaring quest node — `completed` on a plain (no-`after`)
///   quest is a LEAF dependency (Task 6's quest-lifecycle signal, never a
///   DAG edge here).
///
/// `key_set` (T3 [`scene_key_set`]) is supplied by the caller — computed
/// once per resolved project root (`lute-cli`'s `by_root` grouping), same
/// convention as [`resolve_nodes`]. `quest_ids` (T4 [`quest_id_set`]) is
/// accepted for call-site symmetry with [`resolve_nodes`] but is NOT
/// consulted here (Task 5 review fix): quest-node ADMISSION is decided
/// solely by "this quest declares a nonempty `after`" — gating it on the
/// (potentially stale/filtered) `quest_ids` set could silently drop a
/// quest, and its edges/cycles, from the graph. A `completed(Q)` EDGE
/// target resolves via plain `nodes` membership (only an `after`-declaring
/// quest is ever a [`NodeId::Quest`] node — see the edge model above), never
/// `quest_ids` either.
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
        let prereq = by_path.get(path.as_path()).map_or(PrereqState::Absent, |doc| {
            match scene_after(doc) {
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

    // Edges: flattened union of atoms per formula -- `atom_target -> n` iff
    // `atom_target` is itself a node above (never a bare-string cross-check
    // against key_set/quest_ids -- membership in `nodes`, typed, is the only
    // question here).
    let mut edges: BTreeMap<NodeId, BTreeSet<NodeId>> = BTreeMap::new();
    for info in nodes.values() {
        let formula = match &info.prereq {
            PrereqState::Valid(f) => f,
            PrereqState::Absent | PrereqState::Invalid => continue,
        };
        for atom in atoms(formula) {
            let target = match atom {
                Atom::Visited(key) => NodeId::Scene(key),
                Atom::Completed(id) => NodeId::Quest(id),
            };
            if nodes.contains_key(&target) {
                edges.entry(target).or_default().insert(info.id.clone());
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
            topo_order,
        },
        diags,
    )
}

/// Detect any directed cycle in `edges` and report it as [`E_CONN_CYCLE`].
/// Standard DFS 3-coloring, cloned from `schema_import::detect_cycles` /
/// `dfs_cycle` (`schema_import.rs:784-833`) over [`NodeId`] rather than
/// `PathBuf`. Nodes are visited in [`NodeId`]'s own sorted (`BTreeMap`) order
/// for a deterministic, order-independent result; `edges`' `BTreeSet`
/// targets are already sorted, so (unlike the `Vec`-adjacency precedent)
/// there is no separate neighbor sort step.
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
            dfs_conn_cycle(start, nodes, edges, &mut on_stack, &mut done, &mut stack, diags);
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
fn topo_sort(nodes: &BTreeMap<NodeId, NodeInfo>, edges: &BTreeMap<NodeId, BTreeSet<NodeId>>) -> Vec<NodeId> {
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
///   - `visited(Y)`: `Y` a known [`NodeId::Scene`] ⇒ its own (already
///     memoized, since it always precedes this node in `topo_order`)
///     reachability; not a known node ⇒ `Unknown` (that miss is
///     `E-CONN-UNKNOWN-NODE`'s problem, T4 — it must never CASCADE into a
///     false `E-CONN-UNREACHABLE`).
///   - `completed(Q)`, by precedence:
///     1. `Q ∉ quest_ids` (undeclared) ⇒ `Unknown`.
///     2. `Q ∈ ambiguous_quest_ids` (>1 declaration) ⇒ `Unknown`.
///     3. `Q ∈ unreachable_quests` ⇒ `Unreachable` (quest lifecycle,
///        tracked OUTSIDE this graph).
///     4. `NodeId::Quest(Q) ∈ nodes` (an `after`-declaring quest already
///        memoized above) ⇒ its memoized reachability (TRANSITIVE).
///     5. else (a declared PLAIN quest, not unreachable) ⇒ `Reachable`.
///   - `And`: `Unreachable` iff EITHER arm is `Unreachable` (checked
///     first — it dominates); else `Reachable` iff BOTH `Reachable`; else
///     `Unknown`.
///   - `Or`: `Reachable` iff EITHER arm is `Reachable` (checked first —
///     it dominates, even against an `Unreachable` other arm); else
///     `Unreachable` iff BOTH `Unreachable`; else `Unknown`.
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
        let Some(info) = g.nodes.get(id) else { continue };
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
                    eval_reach(f, g, &reach, quest_ids, ambiguous_quest_ids, unreachable_quests)
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
            let target = NodeId::Scene(key.clone());
            if g.nodes.contains_key(&target) {
                reach.get(&target).copied().unwrap_or(Reachability::Unknown)
            } else {
                Reachability::Unknown
            }
        }
        PrereqFormula::Completed(id) => {
            if !quest_ids.contains(id) {
                Reachability::Unknown
            } else if ambiguous_quest_ids.contains(id) {
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
            eval_reach(l, g, reach, quest_ids, ambiguous_quest_ids, unreachable_quests),
            eval_reach(r, g, reach, quest_ids, ambiguous_quest_ids, unreachable_quests),
        ),
        PrereqFormula::Or(l, r) => or_reach(
            eval_reach(l, g, reach, quest_ids, ambiguous_quest_ids, unreachable_quests),
            eval_reach(r, g, reach, quest_ids, ambiguous_quest_ids, unreachable_quests),
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
    counts.into_iter().filter(|(_, count)| *count > 1).map(|(id, _)| id).collect()
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
            let flagged = result
                .diagnostics
                .iter()
                .any(|d| d.code == crate::reachability::E_QUEST_UNREACHABLE && d.span == quest.span);
            if flagged {
                out.insert(quest.id.clone());
            }
        }
    }
    out
}

/// dsl 0.4.0 §4.2/§B: every relation name with at least one `::assert{R(…)}`
/// site inside a node this root's [`check_reachability`] pass did NOT prove
/// [`Reachability::Unreachable`] — the reachability-GATED refinement of
/// `producible()`'s base case (c) (spec §4.2's "a node that is
/// `E-CONN-UNREACHABLE`-clean"). `Reachable` AND `Unknown` both seed
/// producibility: provable-only discipline demands `producible(R) == false`
/// be a PROVEN fact before an objective gated on `R` is flagged dead, so a
/// node this pass cannot resolve either way (`Unknown`, OR one this graph
/// has no entry for at all — e.g. inside an `E-CONN-CYCLE`, or a scene whose
/// identity triad this pass could not even compute) must never be treated
/// as dead by omission; only a node PROVABLY `Unreachable` excludes its
/// assert sites.
///
/// A scene assert site's hosting `NodeId::Scene` is its own
/// [`canonical_episode_key`] (every scene is always a `ConnGraph` node —
/// `Absent` `after` included, so `reach` always has an entry UNLESS the
/// graph itself had a cycle). A quest-body assert site's hosting
/// `NodeId::Quest` mirrors [`check_reachability`]'s own `completed(Q)`
/// precedence (T6): ambiguous (2+ declarations) reads `Unknown`; a
/// caller-supplied `E-QUEST-UNREACHABLE` id reads `Unreachable`; an
/// `after`-declaring quest already has a memoized `reach` entry; a plain
/// (no-`after`) quest not otherwise dead defaults `Reachable`.
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
    let mut out = BTreeSet::new();
    for (_, doc) in docs {
        if resolve_doc_kind(&doc.meta).0 == Some(DocKind::Scene) {
            let node_reach = scene_identity(doc).and_then(|(character, season, episode, episode_id)| {
                let key = canonical_episode_key(&character, season, episode, episode_id.as_deref());
                reach.get(&NodeId::Scene(key)).copied()
            });
            if assert_site_is_live(node_reach) {
                for shot in &doc.shots {
                    collect_assert_relations(&shot.body, &mut out);
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
                collect_assert_relations(&quest.body, &mut out);
            }
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

/// Recursively collect every `::assert{R(…)}` site's relation name from a
/// node stream — mirrors `reachability.rs`'s `walk_reach` recursion shape
/// (match-arm / branch-choice / hub-choice / on / objective bodies). A
/// parse-failed assert (`pattern.relation.is_empty()`, D13) contributes
/// nothing — never fabricates a relation name out of malformed input.
fn collect_assert_relations(nodes: &[Node], out: &mut BTreeSet<String>) {
    for node in nodes {
        match node {
            Node::Assert(a) => {
                if !a.pattern.relation.is_empty() {
                    out.insert(a.pattern.relation.clone());
                }
            }
            Node::Match(m) => {
                for arm in &m.arms {
                    let body = match arm {
                        Arm::When { body, .. } | Arm::Otherwise { body, .. } => body,
                    };
                    collect_assert_relations(body, out);
                }
            }
            Node::Branch(b) => {
                for choice in &b.choices {
                    collect_assert_relations(&choice.body, out);
                }
            }
            Node::Hub(h) => {
                for choice in &h.choices {
                    collect_assert_relations(&choice.body, out);
                }
            }
            Node::On(o) => collect_assert_relations(&o.body, out),
            Node::Objective(o) => collect_assert_relations(&o.body, out),
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
