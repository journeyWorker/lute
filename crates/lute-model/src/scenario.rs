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
    let (graph, _) = lute_check::connectivity::assemble_graph(&project_docs, &key_set, &quest_ids);
    let ambiguous_quests = lute_check::connectivity::ambiguous_quest_ids(&project_docs);
    let fp = compute_conn_fixpoint(
        &project_docs,
        group_full,
        file_results,
        &graph,
        &quest_ids,
        &ambiguous_quests,
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
        per_doc.scene.insert(
            key.clone(),
            (
                envelope::guaranteed(&assigned),
                envelope::possible_writes(&all_nodes),
            ),
        );
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
