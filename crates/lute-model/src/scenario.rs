use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use lute_check::{check_definite_assignment, defassign, envelope, ProjectDoc};
use lute_core_span::Span;

use crate::{compute_conn_fixpoint, ByRoot, DocGroup};

/// The project views of `group`'s documents: borrowed, built per call, never
/// stored (a [`RootScenario`] lives inside the model that owns the documents).
pub fn project_docs(group: &DocGroup) -> Vec<ProjectDoc<'_>> {
    group
        .iter()
        .map(|(p, d, f)| ProjectDoc::new(p.as_path(), d, &f.typed))
        .collect()
}

#[derive(Debug)]
pub struct RootScenario {
    pub graph: lute_check::connectivity::ConnGraph,
    pub reach: BTreeMap<lute_check::connectivity::NodeId, lute_check::connectivity::Reachability>,
    /// Each node's ROUTE envelope ([`envelope::propagate`]); its whole
    /// envelope also holds [`Self::envelope_d`] — read it with
    /// [`Self::env`].
    pub route_envs: BTreeMap<lute_check::connectivity::NodeId, envelope::Env>,
    pub tainted: BTreeSet<lute_check::connectivity::NodeId>,
    pub reads_per_scene: BTreeMap<String, Vec<(String, Span)>>,
    pub key_set: BTreeMap<String, Vec<(PathBuf, Span)>>,
    pub beat_keys: BTreeMap<String, Vec<(PathBuf, Span)>>,
    pub quest_ids: BTreeSet<String>,
    pub ambiguous_quests: BTreeSet<String>,
    pub unreachable_quests: BTreeSet<String>,
    pub dead_required_objective_quests: BTreeSet<String>,
    pub envelope_d: BTreeSet<String>,
    /// The root's `per:` family paths: an envelope message's guard spelling
    /// ([`envelope::check_envelope`]).
    pub families: BTreeSet<String>,
    pub per_doc: envelope::PerDocEffects,
    /// The relational vocabulary of the root's last document declaring a
    /// relation, shared with that document.
    pub rel_vocab: std::sync::Arc<lute_check::RelVocab>,
    pub scene_must: BTreeMap<String, Vec<lute_check::fact_env::MustFact>>,
}

impl RootScenario {
    /// `node`'s whole envelope: its route part with the entry floor `D`;
    /// `None` when [`envelope::propagate`] gave it none (a cycle).
    pub fn env(&self, node: &lute_check::connectivity::NodeId) -> Option<envelope::Env> {
        self.route_envs
            .get(node)
            .map(|env| env.clone().with_floor(&self.envelope_d))
    }
}

pub fn assemble_root_scenario(
    group_full: &DocGroup,
    file_results: &[(PathBuf, lute_check::CheckResult)],
) -> RootScenario {
    let project_docs = project_docs(group_full);
    let key_set = lute_check::connectivity::scene_key_set(&project_docs);
    let quest_ids = lute_check::connectivity::quest_id_set(&project_docs);
    let beat_keys = lute_check::connectivity::bundle_beat_key_set(&project_docs);
    let (mut graph, _) =
        lute_check::connectivity::assemble_graph(&project_docs, &key_set, &quest_ids);
    let ambiguous_quests = lute_check::connectivity::ambiguous_quest_ids(&project_docs);
    let fp = compute_conn_fixpoint(
        &project_docs,
        group_full,
        file_results,
        &graph,
        &quest_ids,
        &ambiguous_quests,
    );
    // dsl 0.49.0 §6.1: start cycles, against the final verdicts.
    let _ = lute_check::connectivity::resolve_start_blocks(
        &mut graph,
        &project_docs,
        &fp.reach,
        &fp.unreachable_quests,
    );
    let mut per_doc = envelope::PerDocEffects::default();
    let mut reads_per_scene = BTreeMap::new();
    let mut rel_vocab = std::sync::Arc::default();
    let mut state_union = lute_check::StateUnion::default();
    for (_, doc, folded) in group_full {
        state_union.add(&folded.env.state);
        if !folded.env.rel_vocab.relations.is_empty() {
            rel_vocab = std::sync::Arc::clone(&folded.env.rel_vocab);
        }
        for quest in &doc.quests {
            if !quest.id.is_empty() && !ambiguous_quests.contains(&quest.id) {
                per_doc.quest_writes_on_complete.insert(
                    quest.id.clone(),
                    envelope::writes_on_complete(quest, &folded.env.state),
                );
            }
        }
    }
    let envelope_d = state_union.defaults();
    let families = state_union.families();
    let mut group_ix = std::collections::HashMap::with_capacity(group_full.len());
    for (i, (p, _, _)) in group_full.iter().enumerate() {
        group_ix.entry(p.as_path()).or_insert(i);
    }
    for (key, occurrences) in &key_set {
        let Some((scene_path, _)) = occurrences.first() else {
            continue;
        };
        let Some((_, doc, folded)) = group_ix.get(scene_path.as_path()).map(|&i| &group_full[i])
        else {
            continue;
        };
        let all_nodes: Vec<_> = doc
            .sections
            .iter()
            .flat_map(|shot| shot.body.iter().cloned())
            .collect();
        let scope = defassign::Scope::of(folded);
        let beat_when = folded
            .typed
            .beat
            .as_ref()
            .and_then(|beat| beat.when.as_ref());
        let (_, assigned, reads) = check_definite_assignment(&all_nodes, &scope, beat_when);
        let exhaustive = defassign::exhaustive_match_subject_spans(&all_nodes, &scope);
        let reads = reads
            .into_iter()
            .filter(|(_, span)| {
                !exhaustive
                    .iter()
                    .any(|s| s.byte_start == span.byte_start && s.byte_end == span.byte_end)
            })
            .collect();
        per_doc
            .scene
            .insert(key.clone(), envelope::scene_effects(&assigned, &all_nodes));
        reads_per_scene.insert(key.clone(), reads);
    }
    let (route_envs, tainted) = envelope::propagate(&graph, &per_doc);
    RootScenario {
        graph,
        reach: fp.reach,
        route_envs,
        tainted,
        reads_per_scene,
        key_set,
        beat_keys,
        quest_ids,
        ambiguous_quests,
        unreachable_quests: fp.unreachable_quests,
        dead_required_objective_quests: fp.dead_required_objective_quests,
        envelope_d,
        families,
        per_doc,
        rel_vocab,
        scene_must: fp.scene_must,
    }
}

/// A reachability verdict ([`reach_claim`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReachVerdict {
    Reachable,
    Unreachable,
    /// A declared quest on no graph edge: available from the start of play.
    Unanchored,
    Unknown,
}

impl ReachVerdict {
    /// The JSON/DOT token.
    pub fn as_str(self) -> &'static str {
        match self {
            ReachVerdict::Reachable => "reachable",
            ReachVerdict::Unreachable => "unreachable",
            ReachVerdict::Unanchored => "unanchored",
            ReachVerdict::Unknown => "unknown",
        }
    }
}

/// One node's reachability claim: the verdict, its evidence and the reason
/// an author reads (dsl 0.49.0 §6.3). `scenario reach` and `constraints`
/// both report it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReachClaim {
    pub verdict: ReachVerdict,
    pub evidence: lute_core_span::Evidence,
    pub reason: String,
}

/// The reach verdict of a declared quest without `follows=` (dsl 0.21.0
/// §7a.5).
const UNANCHORED_REASON: &str = "Unanchored — a quest with no declared `follows` edge: \
     available from the start of play; the connectivity layer holds no prerequisites for it, so \
     only its quest lifecycle (`start`, or an accept) decides when it activates.";

/// `node`'s reachability claim under the declared routes (dsl 0.49.0 §6.3).
/// A quest that can never start ([`lute_check::connectivity::StartBlock`])
/// is `Unknown` naming the start cycle or self-read; a plain quest with no
/// `after` is unanchored; omitted graph nodes are cycle-degraded.
pub fn reach_claim(scenario: &RootScenario, node: &lute_check::connectivity::NodeId) -> ReachClaim {
    use lute_check::connectivity::{NodeId, Reachability, StartBlock};
    use lute_core_span::Evidence;
    let claim = |verdict, evidence, reason: &str| ReachClaim {
        verdict,
        evidence,
        reason: reason.to_string(),
    };
    let unknown = |reason: &str| claim(ReachVerdict::Unknown, Evidence::Unknown, reason);
    if let Some((at, block)) = scenario.graph.start_block_root(node) {
        let reason = match block {
            StartBlock::Cycle(path) => format!(
                "Unknown — this quest's `start` waits on a start cycle (E-QUEST-START-CYCLE): \
                 {}; it can never start under your declared routes.",
                path.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(" -> ")
            ),
            StartBlock::SelfRead if at == node => {
                "Unknown — this quest's `start` reads its own state (E-QUEST-START-SELF); it can \
                 never start under your declared routes."
                    .to_string()
            }
            StartBlock::SelfRead | StartBlock::Behind(_) => format!(
                "Unknown — this quest's `start` waits on {at}, whose `start` reads its own state \
                 (E-QUEST-START-SELF); it can never start under your declared routes."
            ),
        };
        return unknown(&reason);
    }
    if let Some(r) = scenario.reach.get(node) {
        return match r {
            Reachability::Reachable => claim(
                ReachVerdict::Reachable,
                Evidence::Proven,
                "Reachable — a satisfiable route exists under your declared routes.",
            ),
            Reachability::Unreachable => claim(
                ReachVerdict::Unreachable,
                Evidence::Proven,
                "Unreachable — no satisfiable route exists under your declared routes \
                 (E-CONN-UNREACHABLE).",
            ),
            Reachability::Unknown => unknown(
                "Unknown — this analysis cannot prove reachability either way under your \
                 declared routes.",
            ),
        };
    }
    match node {
        NodeId::Quest(id) if scenario.ambiguous_quests.contains(id) => unknown(
            "Unknown — ambiguous quest id (more than one declaration) under your declared routes.",
        ),
        NodeId::Quest(id) if scenario.dead_required_objective_quests.contains(id) => claim(
            ReachVerdict::Unreachable,
            Evidence::Proven,
            "Unreachable — this quest has a provably dead REQUIRED objective, so it can never \
             complete (E-OBJECTIVE-UNSATISFIABLE), under your declared routes.",
        ),
        NodeId::Quest(id) if scenario.unreachable_quests.contains(id) => claim(
            ReachVerdict::Unreachable,
            Evidence::Proven,
            "Unreachable — quest lifecycle proves this quest can never complete \
             (E-QUEST-UNREACHABLE), under your declared routes.",
        ),
        // An id referenced by a formula but never declared anywhere in this
        // root (E-CONN-UNKNOWN-NODE's own concern) reads Unknown — checked
        // before the unanchored fallback, since an undeclared id is also
        // absent from `graph.nodes`.
        NodeId::Quest(id) if !scenario.quest_ids.contains(id) => unknown(
            "Unknown — this quest id is not declared anywhere in this project root \
             (E-CONN-UNKNOWN-NODE), under your declared routes.",
        ),
        NodeId::Quest(_) if !scenario.graph.nodes.contains_key(node) => claim(
            ReachVerdict::Unanchored,
            Evidence::Proven,
            UNANCHORED_REASON,
        ),
        NodeId::Scene(key) if !scenario.key_set.contains_key(key) => unknown(
            "Unknown — this scene key is not declared anywhere in this project root \
             (E-CONN-UNKNOWN-NODE), under your declared routes.",
        ),
        _ => unknown(
            "Unknown — this node is on or downstream of a prerequisite cycle (E-CONN-CYCLE); \
             its reachability is unavailable under your declared routes.",
        ),
    }
}

pub fn node_cycle_degraded(
    scenario: &RootScenario,
    node: &lute_check::connectivity::NodeId,
) -> bool {
    !scenario.reach.contains_key(node)
        && scenario.graph.topo_order.len() < scenario.graph.nodes.len()
}

pub fn find_matching_roots<'a>(
    by_root: &'a ByRoot,
    file_results: &[(PathBuf, lute_check::CheckResult)],
    node: &lute_check::connectivity::NodeId,
) -> Vec<(&'a PathBuf, RootScenario)> {
    by_root
        .iter()
        .filter_map(|(root, group)| {
            let project_docs = project_docs(group);
            let found = match node {
                lute_check::connectivity::NodeId::Scene(key) => {
                    lute_check::connectivity::scene_key_set(&project_docs).contains_key(key)
                }
                lute_check::connectivity::NodeId::Quest(id) => {
                    lute_check::connectivity::quest_id_set(&project_docs).contains(id)
                }
                lute_check::connectivity::NodeId::Beat(key) => {
                    lute_check::connectivity::bundle_beat_key_set(&project_docs).contains_key(key)
                }
                lute_check::connectivity::NodeId::Entry(_) => false,
            };
            found.then(|| (root, assemble_root_scenario(group, file_results)))
        })
        .collect()
}
