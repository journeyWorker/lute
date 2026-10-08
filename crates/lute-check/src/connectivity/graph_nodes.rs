use super::*;
use crate::ProjectDoc;

use std::fmt;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use lute_core_span::{Diagnostic, Layer, Severity, Span};
use lute_syntax::ast::{CelKind, Document};
use crate::meta::{meta_key_span, resolve_doc_kind, DocKind};
use crate::prereq::parse_prereq;
use lute_manifest::semantics::prereq::{atoms, Atom, PrereqFormula};

/// silently paper over a real typo.
pub const E_CONN_UNKNOWN_NODE: &str = "E-CONN-UNKNOWN-NODE";

pub(super) fn unknown_node_diag(message: String, span: Span) -> Diagnostic {
    Diagnostic {
        code: E_CONN_UNKNOWN_NODE.to_string(),
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

/// The raw `after:` frontmatter shape retained in a scene's typed metadata.
/// Distinguishes an ABSENT key from a PRESENT-but-non-string one. The retained
/// YAML is the parse result captured before duplicate-key sanitization.
pub(super) enum SceneAfter {
    /// No `after:` key at all (or the frontmatter itself failed to parse /
    /// wasn't a mapping) — a valid entry node.
    Absent,
    /// `after:` present and its YAML value IS a string (possibly empty).
    String(String),
    /// `after:` present but its YAML value is NOT a string (int/bool/seq/
    /// map/null) — malformed, must classify as `PrereqState::Invalid`.
    NonString,
}

pub(super) fn scene_after(meta: &crate::meta::TypedMeta) -> SceneAfter {
    let Some(value) = meta.yaml() else {
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
pub fn quest_id_set(docs: &[ProjectDoc<'_>]) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for item in docs {
        for quest in &item.doc.quests {
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
pub(super) fn nearest_match<'a>(
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
/// `follows_span`), never a synthetic per-atom location (`PrereqFormula`
/// carries none). `quest_after`: the formula is a `<quest follows=…>` — its
/// miss says how to drop `follows=` instead (dsl 0.24.0 §2).
pub(super) fn check_formula_atoms(
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
                check_scene_key(key, false, span, path, key_set, hint, out);
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

/// The tail of an unresolvable `<quest follows=…>` miss (dsl 0.24.0 §2): a
/// quest has no `when`, and an accept-driven quest needs no `follows=` to be
/// drawn — it is anchored at every `::accept` of it.
pub(super) const QUEST_AFTER_HINT: &str = "; if the quest is taken on by `::accept`, drop `follows=` — an \
     accept-driven quest is anchored at every document that accepts it (dsl 0.24.0 §2)";

/// One `visited(K)` target against the project's scene and bundle beat keys
/// (a bundle beat is a graph node, dsl 0.24.0 §2): a miss is
/// [`E_CONN_UNKNOWN_NODE`] at `span`, with a "did you mean" when a key is
/// close and `hint` appended when given. `cel` is whether the call is a CEL
/// `visited()` (else a prerequisite formula atom) — it picks the cited
/// surface, and the quest read a quest id is pointed at.
///
/// A miss is NOT reported when the key set is incomplete
/// ([`SceneKeys::complete`]): some document in the root has a frontmatter that
/// does not parse, so its id is unreadable and `K` may well be it. That
/// document already fails with `E-META-PARSE`; reporting every reference to
/// it as unknown would cascade one YAML slip into errors in files that are
/// fine (0.21.1 T3-8, seven F3).
pub(super) fn check_scene_key(
    key: &str,
    cel: bool,
    span: Span,
    path: &Path,
    key_set: &SceneKeys<'_>,
    hint: Option<&str>,
    out: &mut Vec<(PathBuf, Diagnostic)>,
) {
    if key_set.keys.contains_key(key) || key_set.bundles.contains_key(key) || !key_set.complete {
        return;
    }
    let cite = if cel {
        "dsl 0.21.0 §7a.1"
    } else {
        "dsl §2.3/§4.1"
    };
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
    } else if let Some(canonical) = local_bundle_id(key, path, key_set) {
        // A bundle beat's own `id=` is local to its document.
        message.push_str(&format!(
            " — did you mean `{canonical}`? A bundle beat's key is `<document id>.<beat id>`"
        ));
    }
    if key_set.quests.contains(key) {
        // A quest is no `visited()` node: its state is the read.
        let read = if cel {
            format!("quest.{key}.state == 'complete'")
        } else {
            format!("completed('{key}')")
        };
        message.push_str(&format!("; `{key}` is a quest — write `{read}`"));
    }
    if let Some(hint) = hint {
        message.push_str(hint);
    }
    out.push((path.to_path_buf(), unknown_node_diag(message, span)));
}

/// dsl 0.23.0 §4: the canonical key of the bundle beat whose local `id=` is
/// `key` — the one in `path`'s own document, else the only one in the
/// project.
pub(super) fn local_bundle_id(key: &str, path: &Path, key_set: &SceneKeys<'_>) -> Option<String> {
    let local: Vec<(&String, &Vec<(PathBuf, Span)>)> = key_set
        .bundles
        .iter()
        .filter(|(k, _)| k.rsplit_once('.').is_some_and(|(_, id)| id == key))
        .collect();
    local
        .iter()
        .find(|(_, at)| at.iter().any(|(p, _)| p == path))
        .or_else(|| (local.len() == 1).then(|| &local[0]))
        .map(|(k, _)| (*k).clone())
}

/// The project's scene keys ([`scene_key_set`]) plus whether they are all of
/// them: `complete` is false when some document's frontmatter does not parse
/// ([`crate::meta::frontmatter_parses`]) — a scene whose id cannot be read.
pub(super) struct SceneKeys<'a> {
    keys: &'a BTreeMap<String, Vec<(PathBuf, Span)>>,
    /// dsl 0.23.0 §4: the bundle beat canonical ids ([`bundle_beat_key_set`]).
    bundles: BTreeMap<String, Vec<(PathBuf, Span)>>,
    complete: bool,
    /// Every declared quest id ([`quest_id_set`]): `visited()` on one is
    /// pointed at the quest read.
    quests: &'a BTreeSet<String>,
}

/// dsl 0.21.0 §7a.1: every `visited('<scene id>')` call in a condition slot
/// of `doc` — the body's CEL slots (quest `start` / `fail`, objective
/// `done`, entry `when`, line / branch `when=`, `<when test>`, …) and a
/// scene beat's frontmatter `when:` — resolved against the scene keys
/// exactly as an `after:` `visited` atom is. Each slot is re-parsed here
/// (the project walk holds no CEL arena); an unparseable slot is the
/// per-file check's `E-CEL-PARSE`, never re-reported.
pub(super) fn check_visited_calls(
    doc: &Document,
    meta: &crate::meta::TypedMeta,
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
            check_scene_key(&key, true, span, path, key_set, None, out);
        }
    };
    lute_syntax::walk::for_each_cel_slot(doc, &mut |slot| {
        if slot.kind == CelKind::Condition && slot.raw.contains(crate::cel_resolve::VISITED_FN) {
            check_raw(&slot.raw, slot.span, out);
        }
    });
    if resolve_doc_kind(&doc.meta).0 == Some(DocKind::Scene) {
        if let Some(when) = scene_frontmatter_str(meta, "when") {
            if when.contains(crate::cel_resolve::VISITED_FN) {
                check_raw(&when, crate::beats::top_value_span(&doc.meta, "when"), out);
            }
        }
    }
}

/// A top-level string value of a scene's frontmatter (the [`scene_after`]
/// lookup, for keys whose non-string shape another pass owns).
pub(super) fn scene_frontmatter_str(meta: &crate::meta::TypedMeta, key: &str) -> Option<String> {
    meta.yaml()?.get(key)?.as_str().map(str::to_string)
}

/// Resolve every prerequisite formula in `docs` — BOTH surfaces
/// (dsl §2.1): a scene document's frontmatter `after:` key, AND every
/// `<quest follows="…">` attribute (a quest pack declares its prerequisite
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
    docs: &[ProjectDoc<'_>],
    key_set: &BTreeMap<String, Vec<(PathBuf, Span)>>,
    quest_ids: &BTreeSet<String>,
) -> Vec<(PathBuf, Diagnostic)> {
    let bundles = bundle_beat_key_set(docs);
    let key_set = &SceneKeys {
        keys: key_set,
        complete: docs
            .iter()
            .all(|item| crate::meta::frontmatter_parses(&item.doc.meta)),
        quests: quest_ids,
        bundles,
    };
    let mut out = Vec::new();
    for item in docs {
        let path = item.path;
        let doc = item.doc;
        // dsl 0.28.0 §4: an `after:` a `chapters:` chain derived names the
        // entry before this one; a bad entry is `E-CHAPTERS`'s, at the
        // manifest, never this scene's.
        if resolve_doc_kind(&doc.meta).0 == Some(DocKind::Scene)
            && !crate::chapters::derived(&doc.meta, "after")
        {
            if let SceneAfter::String(after) = scene_after(item.meta) {
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
            if let Some(after) = &quest.follows {
                let (formula, _) = parse_prereq(after, quest.follows_span);
                if let Some(formula) = formula {
                    check_formula_atoms(
                        &formula,
                        quest.follows_span,
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
        check_visited_calls(doc, item.meta, path, key_set, &mut out);
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
pub(super) fn atom_edge_kind(atom: &Atom) -> EdgeKind {
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
        evidence: Some(lute_core_span::Evidence::Proven),
    }
}

