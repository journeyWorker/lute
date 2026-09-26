//! `lute scenario` (connectivity T14, dsl §5:571-584) — project-wide,
//! read-only reporting surface over everything §4 computes. Evaluates no
//! CEL, runs no Datalog, takes no mocks: pure graph math over declared
//! structure, reusing [`collect_project_docs`]'s SAME per-root doc grouping
//! `check-project` builds (never a second file-walk/parse) plus the SAME
//! `lute_check::connectivity`/`envelope` analyses `check-project`'s own
//! per-root pass calls (never duplicated math — only the presentation, and
//! the omission of diagnostics, differ).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_check::{check_definite_assignment, defassign, envelope};
use lute_core_span::Span;

use crate::cli::ScenarioCommand;
use crate::cmd_scenario::graph::run_scenario_graph;
use crate::cmd_scenario::node_envelope::run_scenario_envelope;
use crate::cmd_scenario::reach::run_scenario_reach;
use crate::endings;
use crate::knowledge;
use crate::output::write_stdout;
use crate::project::reconcile::compute_conn_fixpoint;
use crate::project::{collect_project_docs, ByRoot, DocGroup};

pub(crate) mod graph;
pub(crate) mod node_envelope;
pub(crate) mod reach;

/// A bare scene-key, `quest:<id>`, `scene:<key>`, or `beat:<doc>.<beat>`
/// node reference, parsed from a `scenario reach`/`scenario envelope` CLI
/// argument (dsl §4.4's `envelope quest:<id>` syntax; `scene:<key>` and
/// `beat:<key>` are its symmetric counterparts -- see [`resolve_node_ref`]'s
/// doc comment for why explicit prefixes exist).
pub(crate) enum NodeRef {
    Scene(String),
    Quest(String),
    /// A bundle beat's canonical id (dsl 0.23.0 §4).
    Beat(String),
}

/// Parse an EXPLICIT `quest:<id>` / `scene:<key>` / `beat:<key>` prefix
/// only -- `None` for a bare (unprefixed) string, which [`resolve_node_ref`]
/// resolves against actual project candidates instead of guessing. An
/// explicit prefix is always authoritative: `quest:foo` is ALWAYS a quest
/// lookup and `scene:foo` is ALWAYS a scene lookup, never re-tried as
/// another kind (that would silently paper over a genuine "no such quest"
/// typo).
fn parse_node_ref_prefix(raw: &str) -> Option<NodeRef> {
    if let Some(id) = raw.strip_prefix("quest:") {
        return Some(NodeRef::Quest(id.to_string()));
    }
    if let Some(key) = raw.strip_prefix("scene:") {
        return Some(NodeRef::Scene(key.to_string()));
    }
    if let Some(key) = raw.strip_prefix("beat:") {
        return Some(NodeRef::Beat(key.to_string()));
    }
    None
}

pub(crate) fn node_ref_to_id(node: &NodeRef) -> lute_check::connectivity::NodeId {
    match node {
        NodeRef::Scene(key) => lute_check::connectivity::NodeId::Scene(key.clone()),
        NodeRef::Quest(id) => lute_check::connectivity::NodeId::Quest(id.clone()),
        NodeRef::Beat(key) => lute_check::connectivity::NodeId::Beat(key.clone()),
    }
}

/// Everything `lute scenario` needs for ONE resolved project root, built
/// from the SAME `lute_check::connectivity`/`envelope` analyses
/// `run_check_project`'s own per-root pass calls (T5/T6/T8/T9/T10) — never
/// re-derived independently. Unlike `check-project`, this never scans for
/// project diagnostics (`E-CONN-*`/`E-STATE-MAYBE-UNAVAILABLE`): a
/// read-only reporting surface, not a pass/fail gate (dsl §5:571-584).
pub(crate) struct RootScenario {
    pub(crate) graph: lute_check::connectivity::ConnGraph,
    pub(crate) reach:
        BTreeMap<lute_check::connectivity::NodeId, lute_check::connectivity::Reachability>,
    pub(crate) envs: BTreeMap<lute_check::connectivity::NodeId, envelope::Env>,
    pub(crate) tainted: BTreeSet<lute_check::connectivity::NodeId>,
    pub(crate) reads_per_scene: BTreeMap<String, Vec<(String, Span)>>,
    pub(crate) key_set: BTreeMap<String, Vec<(PathBuf, Span)>>,
    /// Every bundle beat's canonical id (dsl 0.23.0 §4) with its
    /// declarations — each is a [`lute_check::connectivity::NodeId::Beat`]
    /// graph node (lamplight N8, ashen N9).
    pub(crate) beat_keys: BTreeMap<String, Vec<(PathBuf, Span)>>,
    pub(crate) quest_ids: BTreeSet<String>,
    pub(crate) ambiguous_quests: BTreeSet<String>,
    pub(crate) unreachable_quests: BTreeSet<String>,
    /// The subset of `unreachable_quests` that is unreachable via a
    /// PROVABLY dead REQUIRED objective (dsl 0.4.0 §8.2 rule C4 -- the
    /// cause C4 deliberately does NOT surface as a standalone
    /// `E-QUEST-UNREACHABLE`) -- kept SEPARATE from the lifecycle cause
    /// (`start=false`/`fail=true`) so [`reach_verdict_text`] can name the
    /// correct diagnostic code for each cause, never misattributing a C4
    /// note to the suppressed standalone code.
    ///
    /// [`reach_verdict_text`]: crate::cmd_scenario::reach::reach_verdict_text
    pub(crate) dead_required_objective_quests: BTreeSet<String>,
    /// `D` (dsl §4.3 spec lines 442-448): the project-resolved `run.*`/
    /// `user.*` schema-defaulted set, unioned across every doc's own
    /// resolved schema in this root — [`envelope::quest_envelope`]'s own
    /// defaults-only floor.
    pub(crate) envelope_d: BTreeSet<String>,
    /// This root's plain (doc-stripped-of-`FoldedEnv`) docs — quest
    /// envelope printing needs the `&Quest` struct itself
    /// ([`envelope::quest_envelope`]'s signature), never re-parsed here.
    pub(crate) docs: Vec<(PathBuf, lute_syntax::ast::Document)>,
    /// T8/T9's per-document write sets, KEPT rather than consumed. Inverting
    /// `per_doc.scene` names the WRITERS of a path (#15, T9.14); the envelope
    /// already computed it and dropped it on the floor.
    pub(crate) per_doc: envelope::PerDocEffects,
    /// The root's relational vocabulary — declared relations, their arity and
    /// `derive` flag, the `facts:` seeds and the rules. The envelope tables
    /// are scalar-only, so at the scene whose every line is gated on who is
    /// awake the tool that exists to say what is true on arrival did not
    /// mention the subject (#15, T4.7).
    pub(crate) rel_vocab: lute_check::RelVocab,
    /// dsl 0.20.0 §6: per scene key, the facts guaranteed on arrival (the
    /// fact envelope beside the scalar one), each with where it is
    /// established.
    pub(crate) scene_must: BTreeMap<String, Vec<lute_check::fact_env::MustFact>>,
}

/// Assemble [`RootScenario`] for one resolved root's docs — mirrors
/// `run_check_project`'s own per-root block (T5 `assemble_graph`, T6
/// `check_reachability`, T8/T9 `PerDocEffects`, T10 `propagate`) verbatim,
/// minus the diagnostic emission (`lute scenario` reports, never gates).
pub(crate) fn assemble_root_scenario(
    group_full: &DocGroup,
    file_results: &[(PathBuf, lute_check::CheckResult)],
) -> RootScenario {
    let docs: Vec<(PathBuf, lute_syntax::ast::Document)> = group_full
        .iter()
        .map(|(p, d, _)| (p.clone(), d.clone()))
        .collect();
    let key_set = lute_check::connectivity::scene_key_set(&docs);
    let quest_ids = lute_check::connectivity::quest_id_set(&docs);
    let beat_keys = lute_check::connectivity::bundle_beat_key_set(&docs);
    let (graph, _cycle_diags) =
        lute_check::connectivity::assemble_graph(&docs, &key_set, &quest_ids);
    // T7/T14/Fix2 wiring: shares `compute_conn_fixpoint`'s finite-fixpoint
    // iteration with `run_check_project` (see that fn's own doc comment
    // for the termination + soundness argument) -- never re-derived
    // independently.
    let ambiguous_quests = lute_check::connectivity::ambiguous_quest_ids(&docs);
    let fp = compute_conn_fixpoint(
        &docs,
        group_full,
        file_results,
        &graph,
        &quest_ids,
        &ambiguous_quests,
    );
    let reach = fp.reach;
    let unreachable_quests = fp.unreachable_quests;
    let dead_required_objective_quests = fp.dead_required_objective_quests;
    let scene_must = fp.scene_must;

    let mut per_doc = envelope::PerDocEffects::default();
    let mut envelope_d: BTreeSet<String> = BTreeSet::new();
    let mut reads_per_scene: BTreeMap<String, Vec<(String, Span)>> = BTreeMap::new();
    let mut rel_vocab = lute_check::RelVocab::default();
    for (_path, doc, folded) in group_full {
        envelope_d.extend(envelope::schema_defaults(&folded.env.state));
        // Every doc in one resolved root folds the SAME imported vocabulary;
        // taking the last non-empty one matches how `check-project`'s own
        // project-wide relational passes read it.
        if !folded.env.rel_vocab.relations.is_empty() {
            rel_vocab = (*folded.env.rel_vocab).clone();
        }
        for quest in &doc.quests {
            if quest.id.is_empty() || ambiguous_quests.contains(&quest.id) {
                continue;
            }
            per_doc.quest_writes_on_complete.insert(
                quest.id.clone(),
                envelope::writes_on_complete(quest, &folded.env.state),
            );
        }
    }
    let mut group_ix: std::collections::HashMap<&Path, usize> =
        std::collections::HashMap::with_capacity(group_full.len());
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
        let all_nodes: Vec<lute_syntax::ast::Node> = doc
            .shots
            .iter()
            .flat_map(|s| s.body.iter().cloned())
            .collect();
        let scope = defassign::Scope::of(folded);
        let beat_when = folded.typed.beat.as_ref().and_then(|b| b.when.as_ref());
        let (_local_diags, assigned, reads) =
            check_definite_assignment(&all_nodes, &scope, beat_when);
        // Same T4.4/T4.6 carry-forward parity fix as `run_check_project`'s
        // T11 wiring above (dsl §7 soundness invariant) -- `lute scenario
        // envelope`/`reach` must not classify a domain-exhaustive `<match>`
        // subject read as entry-dependent either.
        let exhaustive_spans = defassign::exhaustive_match_subject_spans(&all_nodes, &scope);
        let reads: Vec<(String, Span)> = reads
            .into_iter()
            .filter(|(_, span)| {
                !exhaustive_spans
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
    let (envs, tainted) = envelope::propagate(&graph, &per_doc, &envelope_d);

    RootScenario {
        graph,
        reach,
        envs,
        tainted,
        reads_per_scene,
        key_set,
        beat_keys,
        quest_ids,
        ambiguous_quests,
        unreachable_quests,
        dead_required_objective_quests,
        envelope_d,
        docs,
        per_doc,
        rel_vocab,
        scene_must,
    }
}

/// Find EVERY resolved root (sorted, deterministic — [`ByRoot`] is a
/// `BTreeMap`) whose docs declare `node` (a scene key in `scene_key_set` or
/// a declared `<quest id>`), returning each match's root path alongside its
/// assembled [`RootScenario`]. A scene/quest id is only unique WITHIN one
/// resolved project root (dsl §2.3/§6.3) — the SAME id may legitimately
/// exist in two independently resolved sibling roots (the bare `lute
/// scenario` graph view already shows both). Callers MUST treat 2+ matches
/// as an ambiguous lookup (Main review: never silently pick the
/// lexicographically-first root), never collapse to one.
fn find_matching_roots<'a>(
    by_root: &'a ByRoot,
    file_results: &[(PathBuf, lute_check::CheckResult)],
    node: &NodeRef,
) -> Vec<(&'a PathBuf, RootScenario)> {
    let mut out = Vec::new();
    for (root, group_full) in by_root {
        let scenario = assemble_root_scenario(group_full, file_results);
        let present = match node {
            NodeRef::Scene(key) => scenario.key_set.contains_key(key),
            NodeRef::Quest(id) => scenario.quest_ids.contains(id),
            NodeRef::Beat(key) => scenario
                .graph
                .nodes
                .contains_key(&lute_check::connectivity::NodeId::Beat(key.clone())),
        };
        if present {
            out.push((root, scenario));
        }
    }
    out
}

/// Reduce an already-computed [`find_matching_roots`] result to exactly
/// ONE matching root, or `Err(ExitCode::from(2))` with a clear stderr
/// message when it is declared in ZERO roots (unknown node) or 2+ roots
/// (ambiguous — Main review: a scene/quest id is only unique WITHIN one
/// resolved root, dsl §2.3/§6.3; the SAME id may legitimately exist in
/// independent sibling roots, so this NEVER silently picks the
/// lexicographically-first one).
fn pick_unique_root<'a>(
    mut matches: Vec<(&'a PathBuf, RootScenario)>,
    dir: &Path,
    node_id_raw: &str,
) -> Result<(&'a PathBuf, RootScenario), ExitCode> {
    match matches.len() {
        0 => {
            eprintln!("lute: unknown node `{node_id_raw}` under {}", dir.display());
            Err(ExitCode::from(2))
        }
        1 => Ok(matches.pop().expect("len == 1")),
        n => {
            let roots: Vec<String> = matches
                .iter()
                .map(|(r, _)| r.display().to_string())
                .collect();
            eprintln!(
                "lute: node `{node_id_raw}` is declared in {n} different project roots under \
                 {} -- ambiguous (a scene/quest id is only unique WITHIN one resolved project \
                 root); narrow the directory argument to a single project root: \
                 {}",
                dir.display(),
                roots.join(", ")
            );
            Err(ExitCode::from(2))
        }
    }
}

/// Resolve `node_ref` to exactly ONE matching root's [`RootScenario`] --
/// thin wrapper: [`find_matching_roots`] then [`pick_unique_root`].
fn resolve_unique_root<'a>(
    dir: &Path,
    by_root: &'a ByRoot,
    file_results: &[(PathBuf, lute_check::CheckResult)],
    node_ref: &NodeRef,
    node_id_raw: &str,
) -> Result<(&'a PathBuf, RootScenario), ExitCode> {
    let matches = find_matching_roots(by_root, file_results, node_ref);
    pick_unique_root(matches, dir, node_id_raw)
}

/// Resolve a RAW `scenario reach`/`scenario envelope` CLI argument to its
/// [`NodeRef`] plus the single matching root's [`RootScenario`].
///
/// ## Why bare strings are never guessed
/// A scene's canonical key (`{character}.{episodeId}`,
/// `meta::canonical_episode_key`) is an UNVALIDATED, author-controlled
/// string — `character`/`episodeId` accept arbitrary YAML scalars, no
/// charset restriction — so a scene key CAN literally begin with
/// `quest:` (e.g. `character: "quest:foo"`). Unconditionally reserving
/// that prefix for quest lookups (the original design) would make such a
/// scene permanently unselectable. The fix:
/// - An EXPLICIT `quest:<id>` / `scene:<key>` / `beat:<key>` prefix
///   ([`parse_node_ref_prefix`]) is always authoritative — never re-tried as
///   another kind.
/// - A BARE (unprefixed) string is resolved against ACTUAL project
///   candidates: a declared scene key, a declared quest id, or a bundle
///   beat's canonical id (dsl 0.23.0 §4) in some root. Exactly one kind
///   matching → use it (the overwhelmingly common case — no prefix needed
///   at all). Two or more kinds matching is genuinely ambiguous — none is
///   silently preferred; the user is told to disambiguate with an explicit
///   prefix (mirrors [`primary_node_ambiguity_note`]'s honesty pattern:
///   never silently pick one candidate over another equally-valid one).
pub(crate) fn resolve_node_ref<'a>(
    dir: &Path,
    by_root: &'a ByRoot,
    file_results: &[(PathBuf, lute_check::CheckResult)],
    node_id_raw: &str,
) -> Result<(NodeRef, &'a PathBuf, RootScenario), ExitCode> {
    if let Some(explicit) = parse_node_ref_prefix(node_id_raw) {
        return resolve_unique_root(dir, by_root, file_results, &explicit, node_id_raw)
            .map(|(root, scenario)| (explicit, root, scenario));
    }
    let raw = node_id_raw.to_string();
    let mut found: Vec<(NodeRef, Vec<(&'a PathBuf, RootScenario)>)> = [
        NodeRef::Scene(raw.clone()),
        NodeRef::Quest(raw.clone()),
        NodeRef::Beat(raw),
    ]
    .into_iter()
    .map(|r| {
        let matches = find_matching_roots(by_root, file_results, &r);
        (r, matches)
    })
    .filter(|(_, matches)| !matches.is_empty())
    .collect();
    match found.len() {
        0 => {
            eprintln!("lute: unknown node `{node_id_raw}` under {}", dir.display());
            Err(ExitCode::from(2))
        }
        1 => {
            let (node_ref, matches) = found.pop().expect("len == 1");
            pick_unique_root(matches, dir, node_id_raw)
                .map(|(root, scenario)| (node_ref, root, scenario))
        }
        _ => {
            let kinds: Vec<&str> = found
                .iter()
                .map(|(r, _)| match r {
                    NodeRef::Scene(_) => "a scene key",
                    NodeRef::Quest(_) => "a quest id",
                    NodeRef::Beat(_) => "a bundle beat id",
                })
                .collect();
            let prefixes: Vec<String> = found
                .iter()
                .map(|(r, _)| match r {
                    NodeRef::Scene(_) => format!("`scene:{node_id_raw}`"),
                    NodeRef::Quest(_) => format!("`quest:{node_id_raw}`"),
                    NodeRef::Beat(_) => format!("`beat:{node_id_raw}`"),
                })
                .collect();
            eprintln!(
                "lute: node `{node_id_raw}` matches {} in this project -- ambiguous (none is \
                 silently preferred); disambiguate with an explicit {} prefix",
                kinds.join(" and "),
                prefixes.join(" or "),
            );
            Err(ExitCode::from(2))
        }
    }
}

/// `Some(message)` when the PRIMARY requested node itself is ambiguous
/// WITHIN its resolved root — a duplicated scene key
/// (`E-CONN-EPISODE-ID-DUP`, T3: 2+ scene documents computing the same
/// canonical key) or a duplicated quest id (`E-QUEST-ID-DUP`) — in which
/// case neither `reach` nor `envelope` has a single well-defined
/// declaration to report on. Callers MUST check this BEFORE any deeper
/// analysis so neither command ever silently displays one
/// arbitrarily-chosen declaration's data as if it were authoritative
/// (Main review: symmetric honesty treatment for scenes and quests —
/// `assemble_root_scenario`'s own `key_set[key].first()` / graph
/// admission both already pick an arbitrary declaration internally,
/// mirroring `assemble_graph`'s own "anchored at first occurrence"
/// precedent, which is fine for the underlying graph math but must never
/// be surfaced to the user as if it were an unambiguous answer).
pub(crate) fn primary_node_ambiguity_note(
    scenario: &RootScenario,
    node_ref: &NodeRef,
) -> Option<String> {
    match node_ref {
        NodeRef::Scene(key) => {
            let occurrences = scenario.key_set.get(key)?;
            (occurrences.len() > 1).then(|| {
                format!(
                    "ambiguous scene key (E-CONN-EPISODE-ID-DUP): `{key}` is computed by {} \
                     different scene documents in this project root, so a single \
                     reach/envelope report cannot be given.",
                    occurrences.len()
                )
            })
        }
        NodeRef::Quest(id) => scenario.ambiguous_quests.contains(id).then(|| {
            format!(
                "ambiguous quest id (E-QUEST-ID-DUP): `{id}` has more than one declaration in \
                 this project root, so a single reach/envelope report cannot be given."
            )
        }),
        NodeRef::Beat(key) => {
            let occurrences = scenario.beat_keys.get(key)?;
            (occurrences.len() > 1).then(|| {
                format!(
                    "ambiguous bundle beat id (E-CONN-EPISODE-ID-DUP): `{key}` is declared by {} \
                     different lore documents in this project root, so a single reach/envelope \
                     report cannot be given.",
                    occurrences.len()
                )
            })
        }
    }
}

/// `lute scenario` dispatch (dsl §5:571-584): reuses [`collect_project_docs`]
/// — the SAME per-root doc collection `check-project` builds — then routes
/// to the bare graph view, `reach`, or `envelope`.
pub(crate) fn run_scenario(
    dir: &Path,
    providers: Option<&Path>,
    command: Option<ScenarioCommand>,
    facts: bool,
) -> ExitCode {
    let (file_results, by_root) = match collect_project_docs(dir, providers, false) {
        Ok(v) => v,
        Err(code) => return code,
    };
    // T3-15: the report is built into one buffer and written once through
    // [`write_stdout`] — `lute scenario … | head` used to panic on EPIPE
    // from a bare `println!`.
    let mut out = String::new();
    let code = match command {
        None => run_scenario_graph(&mut out, &by_root, facts),
        Some(ScenarioCommand::Reach {
            endings: Some(occasion),
            ..
        }) => endings::run_text(
            &mut out,
            &by_root,
            &file_results,
            (!occasion.is_empty()).then_some(occasion.as_str()),
        ),
        Some(ScenarioCommand::Reach { node_id, .. }) => run_scenario_reach(
            &mut out,
            dir,
            &by_root,
            &file_results,
            node_id.as_deref().unwrap_or_default(),
        ),
        Some(ScenarioCommand::Envelope { node_id }) => {
            run_scenario_envelope(&mut out, dir, &by_root, &file_results, &node_id)
        }
        Some(ScenarioCommand::Knowledge { for_node }) => {
            knowledge::run_text(&mut out, &by_root, &file_results, for_node.as_deref())
        }
    };
    if write_stdout(&out).is_err() {
        return ExitCode::from(2);
    }
    code
}
