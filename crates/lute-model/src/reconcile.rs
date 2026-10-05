//! `check-project`'s per-root reconciliation over already-collected docs:
//! the connectivity fixpoint, project-wide passes, and the relocation /
//! roll-up of imported and component-body diagnostics.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use lute_check::{
    check_definite_assignment, check_project_quest_ids, check_project_quest_refs, defassign,
    envelope, ProjectDoc,
};
use lute_core_span::{Diagnostic, Severity, Span};
use lute_manifest::project::load_project;
use rayon::prelude::*;

use lute_load::normalize_span_from_text;
use crate::{ByRoot, DocGroup};

/// The converged result of [`compute_conn_fixpoint`]'s monotone iteration
/// (dsl 0.4.0 §4.2's relational-objective-liveness CLOSURE, connectivity
/// design spec §4.2 -- reviewer finding: a single round misses a
/// multi-hop chain, e.g. a dead required objective -> a scene gated on
/// `completed()` that scene becomes unreachable -> its own `::assert`
/// producer drops -> a relation elsewhere goes non-producible -> ANOTHER
/// quest's required objective dies -> repeat).
pub struct ConnFixpoint {
    pub reach:
        BTreeMap<lute_check::connectivity::NodeId, lute_check::connectivity::Reachability>,
    pub reach_diags: Vec<(PathBuf, Diagnostic)>,
    /// dsl 0.20.0 §3/§4: the root's fact envelope at the converged `reach` —
    /// the one the project-level relational guard pass decides with.
    pub fact_env: lute_check::FactEnv,
    /// dsl 0.20.0 §6: per scene key, the facts guaranteed on arrival —
    /// `lute scenario`'s fact envelope.
    pub scene_must: BTreeMap<String, Vec<lute_check::fact_env::MustFact>>,
    pub dead_required_objective_quests: BTreeSet<String>,
    pub unreachable_quests: BTreeSet<String>,
}

/// Iterate `reach -> live assert sites -> May (fact envelope) ->
/// dead_required_objective_quests / dead_lifecycle_quests -> grow
/// unreachable_quests` to a FINITE FIXPOINT (dsl 0.4.0 §8.2 rule C4 + design
/// spec §4.2's closure, dsl 0.20.0 §3), shared by [`run_check_project`] and
/// [`assemble_root_scenario`].
///
/// **Fix 1 (reviewer, soundness/false-positive):** `live_assert_sites`'s
/// per-quest host-liveness check is seeded ONLY from
/// `lifecycle_unreachable_quests` (`start=false`/`fail=true` -- 0.4 §5.3:
/// `fail` "precedes completion... fails at the first evaluation instant",
/// i.e. the body genuinely NEVER executes), never from the GROWING combined
/// set. A quest with a dead REQUIRED objective can still ACTIVATE and run
/// its OTHER body nodes (an optional objective's own `::assert`, a
/// top-level assert, …) -- "can never COMPLETE" is not "never ACTIVATES".
/// Conflating the two would wrongly drop a still-live producer and cascade
/// a FALSE `E-OBJECTIVE-UNSATISFIABLE` onto an unrelated, genuinely-alive
/// objective. `reach` (scene-node liveness) is NOT similarly restricted --
/// a scene whose ONLY declared route runs through a now-unreachable
/// `completed(Q)` gate really is never entered, so ITS assert sites really
/// do drop; that is the intended closure, not a false positive.
///
/// **Fix 2 (advisory, completeness): finite fixpoint, not one round.** Each
/// iteration recomputes `reach` from the CURRENT `unreachable_quests`, then
/// the live assert sites / the may set / the dead-quest sets from that
/// `reach`, then grows `unreachable_quests` by the union. The composition is
/// MONOTONE over the finite quest-id domain:
/// - `eval_reach`'s `And`/`Or` lattice is monotone in `unreachable_quests`
///   (more unreachable input never turns a node MORE reachable) -> `reach`
///   only ever loses `Reachable`/`Unknown` entries to `Unreachable` as the
///   set grows, never the reverse.
/// - `live_assert_sites`'s scene branch reads `reach` directly -> the live
///   site set can only SHRINK (or stay the same) as `reach` tightens.
/// - `MaySet::build` is a monotone least fixpoint over its seeds -> fewer
///   live asserts can only shrink `May`, never grow it.
/// - a smaller `May` only turns *possible* `holds`/`count` verdicts into
///   decided ones (Kleene composition, R1-R5, never un-decides a value) --
///   so both dead-quest sets only grow.
///
/// So `unreachable_quests` is monotone NON-DECREASING, bounded above by the
/// full finite `quest_ids` set (every id either set can ever contain is
/// itself one of this root's declared quests) -- the loop terminates in AT
/// MOST `quest_ids.len() + 1` rounds (it either adds >=1 new id, or
/// stabilizes and returns). Every id ever added is PROVABLY dead/unreachable
/// at the round it was added -- monotone growth of a provable-only set can
/// never introduce a false positive.
///
/// [`assemble_root_scenario`]: crate::cmd_scenario::assemble_root_scenario
/// [`run_check_project`]: crate::cmd_check_project::run_check_project
pub fn compute_conn_fixpoint(
    group: &[ProjectDoc<'_>],
    group_full: &DocGroup,
    file_results: &[(PathBuf, lute_check::CheckResult)],
    conn_graph: &lute_check::connectivity::ConnGraph,
    quest_ids: &BTreeSet<String>,
    ambiguous_quests: &BTreeSet<String>,
) -> ConnFixpoint {
    let lifecycle_unreachable_quests =
        lute_check::connectivity::unreachable_quest_ids(group, file_results);
    let mut root_vocab = lute_check::RootVocab::default();
    for (_path, _doc, folded) in group_full {
        root_vocab.add(&folded.env.rel_vocab, &folded.env.domains);
    }
    // seven F3: a document whose frontmatter does not parse may produce facts
    // nothing here can see — no guard is dead for want of them.
    root_vocab.note_unreadable_documents(group);
    // dsl 0.23.0 §9: seeds nothing can remove — a negated rule atom over one
    // of them never holds.
    let stable = lute_check::stable_seeds(group, &root_vocab);
    let mut unreachable_quests = lifecycle_unreachable_quests.clone();
    // dsl 0.10.0 §5.1 (D-N), dsl 0.20.0 §5: grows inside the loop like
    // `newly_dead`, and is tracked separately so the two derived causes keep
    // distinct verdict text.
    let mut dead_lifecycle_quests: BTreeSet<String> = BTreeSet::new();
    // dsl 0.20.0 §4: the must sets, computed ONCE from the first round's may
    // set. `May` only shrinks across rounds and `Must` reads it solely to
    // prove negated rule atoms, so the first (largest) `May` keeps `Must`
    // sound for every later round — and a fixed `Must` keeps the dead-quest
    // sets monotone (the termination argument above).
    let mut must: Option<lute_check::FactMust> = None;
    let foldeds: Vec<&lute_check::FoldedEnv> = group_full.iter().map(|(_, _, f)| f).collect();
    loop {
        let (reach, reach_diags) = lute_check::connectivity::check_reachability(
            conn_graph,
            quest_ids,
            ambiguous_quests,
            &unreachable_quests,
        );
        let live_facts = lute_check::connectivity::live_assert_sites(
            group,
            &reach,
            ambiguous_quests,
            &lifecycle_unreachable_quests,
            root_vocab.effect_directives(),
        )
        .into_iter()
        .flat_map(|(_path, p)| root_vocab.asserted_facts(&p));
        let may = lute_check::MaySet::build(&root_vocab, live_facts, &stable);
        let must = must.get_or_insert_with(|| {
            lute_check::compute_must(group, &foldeds, conn_graph, &root_vocab, &may)
        });
        let fact_env = lute_check::FactEnv::new(may, must.slots.clone());
        let mut newly_dead: BTreeSet<String> = BTreeSet::new();
        // A quest whose `start=` decides false (or `fail=` true) only under
        // the fact envelope can never complete. The project guard pass says
        // so as a diagnostic, but that diagnostic lands in `project_diags`
        // and NEVER in `file_results`, so `unreachable_quest_ids` — which
        // scans the per-file `check()` output — cannot see it. Connectivity
        // reads the FACT from `dead_lifecycle_quests` instead, exactly as it
        // reads `dead_required_objective_quests` rather than the diagnostic
        // that cause emits.
        let mut newly_dead_lifecycle: BTreeSet<String> = BTreeSet::new();
        for (path, doc, folded) in group_full {
            newly_dead.extend(lute_check::fact_check::dead_required_objective_quests(
                path,
                doc,
                folded,
                &fact_env,
                ambiguous_quests,
            ));
            newly_dead_lifecycle.extend(lute_check::fact_check::dead_lifecycle_quests(
                path,
                doc,
                folded,
                &fact_env,
                ambiguous_quests,
            ));
        }
        // Both derived sets only ever GROW, so the fixpoint argument in this
        // function's doc holds.
        dead_lifecycle_quests.extend(newly_dead_lifecycle);
        let grown: BTreeSet<String> = lifecycle_unreachable_quests
            .iter()
            .cloned()
            .chain(dead_lifecycle_quests.iter().cloned())
            .chain(newly_dead)
            .collect();
        if grown == unreachable_quests {
            // A dead-lifecycle quest is a LIFECYCLE cause, not a
            // dead-objective one: it must reach `reach_verdict_text`'s
            // `E-QUEST-UNREACHABLE` branch, not the `E-OBJECTIVE-UNSATISFIABLE`
            // one that is checked first. Subtract it here so the two causes
            // keep their own text.
            let dead_required_objective_quests: BTreeSet<String> = unreachable_quests
                .difference(&lifecycle_unreachable_quests)
                .filter(|id| !dead_lifecycle_quests.contains(*id))
                .cloned()
                .collect();
            return ConnFixpoint {
                reach,
                reach_diags,
                fact_env,
                scene_must: must.scene_entry.clone(),
                dead_required_objective_quests,
                unreachable_quests,
            };
        }
        unreachable_quests = grown;
    }
}

/// Reconcile the per-root project analysis over ALREADY-COLLECTED docs
/// ([`collect_project_docs`]): run each resolved root's `<quest id>`
/// uniqueness pass (dsl 0.2.0 §6.3, [`lute_check::check_project_quest_ids`]),
/// quest-ref pass (dsl 0.5.1 §1.4), and the T5–T11 connectivity graph /
/// reachability / envelope analyses, then RECONCILE the per-file diagnostics
/// against the project-wide proof: suppress a per-file `E-QUEST-ID-DUP` the
/// project pass already covers (only within its OWN resolved root — never one
/// reaching outside the walk root or a sibling root, [`lute_check::colliding_occurrences`]),
/// and reclassify every entry-dependent, in-scope, non-tainted `E-MAYBE-UNSET`
/// against the connectivity envelope (dropped when `Guaranteed`,
/// dropped-and-suppressed when `Possible\Guaranteed`, replaced by error-grade
/// `E-STATE-MAYBE-UNAVAILABLE` when `∉ Possible`). Returns the reconciled
/// per-file results (each `ok` recomputed) plus the project-wide diagnostics
/// (spans normalized against each file's own text — [`normalize_span_from_text`]).
///
/// The connectivity fixpoint ([`compute_conn_fixpoint`]) and this
/// reconciliation live HERE, in `lute-cli`, never in `lute-check` (which stays
/// FS-free and format-free). Shared by [`run_check_project`] (grouping +
/// human/JSON output) and [`reconciled_project_results`] (the compile/trace
/// project-aware §5 gate).
/// `wip` grades the fact-envelope dead-guard verdicts for `check-project
/// --wip` (dsl 0.23.0 §10). The last element is each root's converged fact
/// envelope, for the reports that judge a condition under it (`lute beats`'
/// gate marks, dsl 0.27.0 §4).
///
/// [`collect_project_docs`]: crate::project::collect_project_docs
/// [`reconciled_project_results`]: crate::project::gate::reconciled_project_results
/// [`run_check_project`]: crate::cmd_check_project::run_check_project
#[allow(clippy::type_complexity)]
pub fn reconcile_collected(
    mut file_results: Vec<(PathBuf, lute_check::CheckResult)>,
    by_root: &ByRoot,
    wip: bool,
) -> (
    Vec<(PathBuf, lute_check::CheckResult)>,
    Vec<(PathBuf, Diagnostic)>,
    BTreeMap<PathBuf, Vec<(lute_check::connectivity::NodeId, Span)>>,
    BTreeMap<PathBuf, lute_check::FactEnv>,
) {
    let mut project_diags = Vec::new();
    // T11: every ENTRY-DEPENDENT, RUN/USER-TIER read at a NON-TAINTED scene
    // node that per-file `check()` already flagged `E-MAYBE-UNSET` gets
    // RECLASSIFIED against the project envelope below (mirrors the
    // `E-QUEST-ID-DUP` retain-pass precedent, §5). Matched by (path, span,
    // exact message) -- NOT (path, span) alone: `check_reads`/
    // `apply_condition` give every path in ONE CEL slot the SAME `Span`
    // (defassign.rs has no per-path span), so a mixed expression like
    // `run.upstream && scene.local` has BOTH reads at an IDENTICAL span --
    // only the message (which embeds the exact path text verbatim,
    // uniquely) tells them apart. `envelope::in_envelope_scope` is applied
    // BEFORE a site ever enters this list, so an out-of-scope `scene.*`/
    // `quest.*`/`app.*` `E-MAYBE-UNSET` is NEVER reconciled (T11 only ever
    // classifies `run.*`/`user.*`). Every reconciled site's per-file
    // `E-MAYBE-UNSET` is dropped in the retain pass further down,
    // REGARDLESS of the reclassification's outcome (`Guaranteed` → dropped
    // with no replacement; `Possible\Guaranteed` → dropped, warning-grade
    // `E-STATE-MAYBE-UNAVAILABLE` computed-and-discarded, default-
    // suppressed per dsl §4.3/§5 until T14's `lute scenario envelope`
    // exists; `∉ Possible` → dropped, replaced by an error-grade
    // `E-STATE-MAYBE-UNAVAILABLE` in `project_diags`). A TAINTED node's
    // reads are never added here -- its `Env` is untrustworthy, so its
    // per-file `E-MAYBE-UNSET` stays exactly as `check()` reported it.
    let mut reconciled_reads: Vec<(PathBuf, Span, String)> = Vec::new();
    // Every occurrence within its own resolved root already covers (see the
    // fn doc comment above) — used below to suppress ONLY the per-file
    // `E-QUEST-ID-DUP`s that pass demonstrably re-reports, never the ones it
    // structurally cannot see (an import-graph collision reaching outside
    // `dir`, or a same-id declare in a SIBLING project root).
    let mut covered = Vec::new();
    // The lore mirror (dsl 0.19.0 §3): every `<entry id>` / `(series, order)`
    // occurrence the project entry pass already reports, used below to
    // suppress the per-file `E-ENTRY-ID-DUP` / `E-ENTRY-SERIES-ORDER` twins.
    let mut entry_covered = Vec::new();
    // P28S-03/HW28-06: per root, the `quest.<id>.` / `entry.<id>.` heads of
    // every quest or entry id with a `.`. Its declaration is refused
    // (`E-PATH-IDENT` / `E-ENTRY-ATTR`), the one report: a per-file
    // `E-UNDECLARED` read of it in the same root is dropped below. A `-` is
    // a name (dsl 0.30.0): its declaration is clean, and a read that writes
    // it after a `.` keeps its own `E-PATH-IDENT`.
    let mut dotted_heads: Vec<Vec<String>> = Vec::new();
    let mut dotted_root_of: std::collections::HashMap<PathBuf, usize> =
        std::collections::HashMap::new();
    // Spec §5 project gate side channel (additive; NEVER affects
    // `project_diags`, so `check-project`'s output is byte-identical).
    // Accumulated across every resolved root so the single-root gate
    // (`reconciled_project_results`) can map its target document to the
    // NodeId(s) it hosts; the on/downstream-of-cycle test itself is decided
    // by topological-order exclusion in `reconciled_project_results`.
    let mut nodes_by_path: BTreeMap<PathBuf, Vec<(lute_check::connectivity::NodeId, Span)>> =
        BTreeMap::new();
    let mut fact_envs: BTreeMap<PathBuf, lute_check::FactEnv> = BTreeMap::new();
    // First result index per path — the answer `iter().find(p == path)`
    // gave, without an O(files) scan per document.
    let mut result_ix: std::collections::HashMap<PathBuf, usize> =
        std::collections::HashMap::with_capacity(file_results.len());
    for (i, (p, _)) in file_results.iter().enumerate() {
        result_ix.entry(p.clone()).or_insert(i);
    }
    for (root, group_full) in by_root {
        let group: Vec<ProjectDoc<'_>> = group_full
            .iter()
            .map(|(p, d, f)| ProjectDoc::new(p.as_path(), d, &f.typed))
            .collect();
        let mut group_ix: std::collections::HashMap<&Path, usize> =
            std::collections::HashMap::with_capacity(group_full.len());
        for (i, (p, _, _)) in group_full.iter().enumerate() {
            group_ix.entry(p.as_path()).or_insert(i);
        }
        let beat_foldeds: Vec<&lute_check::FoldedEnv> =
            group_full.iter().map(|(_, _, f)| f).collect();
        let mocked = mocked_accepts_under(root);
        // The standalone project passes read only this root's documents —
        // never one another's output or the connectivity chain's — so they
        // run in parallel with it; every result is appended below in the
        // fixed order the passes always ran in.
        type Pass<'a> = Box<dyn Fn() -> Vec<(PathBuf, Diagnostic)> + Send + Sync + 'a>;
        let standalone: Vec<Pass<'_>> = vec![
            Box::new(|| check_project_quest_ids(&group)),
            // A root with a manifest is a whole project (a root never ascends
            // above the walk), so an unknown quest read is an error there.
            Box::new(|| check_project_quest_refs(&group, root.join("lute.project.yaml").is_file())),
            // dsl 0.21.0 §7a.3: every `::accept` names an accept-driven quest.
            Box::new(|| lute_check::check_project_accepts(&group)),
            // dsl 0.24.0 §2: an accept-driven quest no `::accept`, mock, or test accepts.
            Box::new(|| lute_check::check_project_never_accepted(&group, &mocked)),
            // dsl 0.19.0 §3/§5: project-wide entry id / series-order uniqueness
            // and `entry.<id>.read` references (the quest passes' lore mirror).
            Box::new(|| lute_check::check_project_entry_ids(&group)),
            Box::new(|| lute_check::check_project_entry_refs(&group)),
            // dsl 2026-08-31 §4 (subquest design): structural checks over the
            // parent→child tree implied by every `<objective quest="c">`. Sits
            // next to the existing quest-ref pass because the two ask the same
            // question at two different depths -- ref pass on the READ side
            // (`quest.<id>.state` from anywhere), tree pass on the STRUCTURAL
            // side (parent quest naming the child). Both are project-wide
            // because a `quest=` reference can name a quest in a sibling file.
            Box::new(|| lute_check::check_project_quest_tree(&group)),
            // dsl 0.22.0 §7: `<on event="questFailed">` on a quest that cannot
            // fail (project-wide: a parent in another file can cascade-fail it).
            Box::new(|| lute_check::check_project_quest_handlers(&group)),
            Box::new(|| lute_check::connectivity::check_conn_episode_dup(&group)),
            // Two documents' menus answered by one `choose:` key.
            Box::new(|| lute_check::check_project_branch_ids(root, &group)),
            // dsl 0.28.0 §4: the manifest's `chapters:` names this root's scenes.
            Box::new(|| lute_check::chapters::check_project_chapters(root, &group, &beat_foldeds)),
            // dsl 0.26.0 §2.1: every declaration of one state path agrees.
            Box::new(|| lute_check::state_decls::check_project_state_decls(&group, &beat_foldeds)),
            // An objective whose `done` can only hold once its deadline does.
            Box::new(|| {
                lute_check::clock_positions::check_project_deadline_windows(&group, &beat_foldeds)
            }),
            // A beat whose `when` holds only where the clock does not raise
            // its occasion (or only at a last `dayEnd` the game's end closes).
            Box::new(|| lute_check::clock_positions::check_project_unraised(&group, &beat_foldeds)),
            // A repeatable beat must not answer a clock raise and advance
            // itself into the next raised position.
            Box::new(|| {
                lute_check::clock_positions::check_project_advance_cascades(
                    &group,
                    &beat_foldeds,
                )
            }),
            // dsl 0.31.0: required objective windows and same-slot
            // advancement contention.
            Box::new(|| {
                lute_check::check_project_objective_clock_windows(&group, &beat_foldeds)
            }),
            // A `spentBy` another document's quest spends at the start, or
            // one whose condition can turn false again after it has held.
            Box::new(|| lute_check::spent_by::check_project_spent_by(root, &group, &beat_foldeds)),
            // dsl 0.26.0 §2.8: advisory — two speakers sharing a display name.
            Box::new(|| {
                let casts: Vec<_> = beat_foldeds.iter().map(|f| &f.cast).collect();
                let use_lines: Vec<_> = beat_foldeds.iter().map(|f| &f.use_lines).collect();
                let origins: Vec<_> = beat_foldeds
                    .iter()
                    .map(|f| &f.env.rel_vocab.origins.cast)
                    .collect();
                // A name nobody speaks is reported at the cast entry.
                let home = |id: &str| {
                    let project = load_project(root).ok().flatten();
                    let plugins = project.as_ref().map(|p| p.plugins_dir.as_path());
                    cast_home(root, plugins, &origins, id)
                };
                lute_check::display_names::check_display_names(&group, &casts, &use_lines, &home)
            }),
        ];
        let (chain, (standalone_diags, (ladder, producers))) = rayon::join(
            || {
                let key_set = lute_check::connectivity::scene_key_set(&group);
                let quest_ids = lute_check::connectivity::quest_id_set(&group);
                let node_diags =
                    lute_check::connectivity::resolve_nodes(&group, &key_set, &quest_ids);
                let (conn_graph, cycle_diags) =
                    lute_check::connectivity::assemble_graph(&group, &key_set, &quest_ids);
                // T7/T14/Fix2 wiring: `compute_conn_fixpoint` iterates the
                // reach/live-assert/may-set/dead-quest composition to a finite
                // fixpoint (see its own doc comment for the termination + soundness
                // argument) -- `ambiguous_quests` is shared with the envelope wiring
                // below.
                let ambiguous_quests = lute_check::connectivity::ambiguous_quest_ids(&group);
                let fp = compute_conn_fixpoint(
                    &group,
                    group_full,
                    &file_results,
                    &conn_graph,
                    &quest_ids,
                    &ambiguous_quests,
                );
                (
                    key_set,
                    node_diags,
                    conn_graph,
                    cycle_diags,
                    ambiguous_quests,
                    fp,
                )
            },
            || {
                rayon::join(
                    || standalone.par_iter().map(|pass| pass()).collect::<Vec<_>>(),
                    || {
                        (
                            lute_check::beats::presence_ladder(&group, &beat_foldeds),
                            lute_check::cast::fact_producers(
                                &group,
                                &lute_check::directive_facts::root_table(
                                    group_full.iter().map(|(_, _, f)| f),
                                ),
                            ),
                        )
                    },
                )
            },
        );
        let (key_set, node_diags, conn_graph, cycle_diags, ambiguous_quests, fp) = chain;
        // `standalone_diags` keeps the fixed order the passes always ran in.
        for diags in standalone_diags {
            project_diags.extend(diags);
        }
        project_diags.extend(node_diags);
        project_diags.extend(cycle_diags);
        // Spec §5 gate side channel (additive, no diagnostic effect): record
        // every node's (id, span) keyed by its declaring file, so the gate can
        // anchor a TARGET-owned `E-CONN-CYCLE` even when the emitted cycle
        // diagnostic landed on a different node's file.
        for info in conn_graph.nodes.values() {
            nodes_by_path
                .entry(info.path.clone())
                .or_default()
                .push((info.id.clone(), info.span));
        }
        project_diags.extend(fp.reach_diags);
        // dsl 2026-08-31 §4 extension: `E-QUEST-UNREACHABLE` propagates one
        // edge UP a subquest tree — a required `<objective quest="c">` on a
        // dead child can never complete (§2.1's synthesized predicate
        // `quest.c.state == 'complete'` never fires). Sits AFTER the
        // fixpoint because it consumes `fp.unreachable_quests` — the union
        // of lifecycle-dead, dead-`start`, and dead-required-objective
        // consequences the fixpoint has just settled. `optional` is
        // filtered inside the helper, matching §2.1's own carve-out.
        //
        // dsl 0.20.0 §5: every guard slot below is re-decided under the
        // root's fact envelope (built once, by the fixpoint above). Under
        // `--wip` (dsl 0.23.0 §10, 0.26.0 §2.6) the envelope carries its
        // work-in-progress twin: relations nothing produces yet, or only a
        // component `::assert` with an unbound `@param` writes, may hold
        // anything there.
        let wip_env = wip.then(|| {
            let mut vocab = lute_check::RootVocab::default();
            for (_, _, folded) in group_full {
                vocab.add(&folded.env.rel_vocab, &folded.env.domains);
            }
            let unproduced = lute_check::unproduced_relations(&group, &vocab);
            fp.fact_env.clone().with_wip(&vocab, &unproduced)
        });
        match &wip_env {
            None => project_diags.extend(lute_check::check_project_subquest_unsatisfiable(
                &group,
                &fp.unreachable_quests,
            )),
            // dsl 0.26.0 §2.6: a child unreachable only for want of
            // producers not written yet grades its parent's objective a
            // warning. `firm` re-derives the fixpoint's causes ignoring
            // verdicts the work-in-progress twin does not share.
            Some(env) => {
                let mut firm =
                    lute_check::connectivity::unreachable_quest_ids(&group, &file_results);
                for (path, doc, folded) in group_full {
                    firm.extend(lute_check::fact_check::dead_required_objective_quests(
                        path,
                        doc,
                        folded,
                        env,
                        &ambiguous_quests,
                    ));
                    firm.extend(lute_check::fact_check::dead_lifecycle_quests(
                        path,
                        doc,
                        folded,
                        env,
                        &ambiguous_quests,
                    ));
                }
                firm.retain(|q| fp.unreachable_quests.contains(q));
                let pending: BTreeSet<String> =
                    fp.unreachable_quests.difference(&firm).cloned().collect();
                project_diags.extend(lute_check::check_project_subquest_unsatisfiable(
                    &group, &firm,
                ));
                project_diags.extend(
                    lute_check::check_project_subquest_unsatisfiable(&group, &pending)
                        .into_iter()
                        .map(|(path, d)| {
                            let d = lute_check::fact_check::wip_warning(
                                d,
                                "the child is unreachable only for want of producers not \
                                 written yet (dsl 0.26.0 §2.6)",
                            );
                            (path, d)
                        }),
                );
            }
        }
        let fact_env = wip_env.as_ref().unwrap_or(&fp.fact_env);
        // Only verdicts the facts newly make decidable are added; one the
        // per-file `check()` already reported for the same slot is not
        // repeated.
        // Per document, independent: in parallel, appended in walk order.
        // Beside it, dsl 0.21.0 §5 / 0.22.0 §13: `W-BEAT-SHADOWED` — a
        // `select: first` beat an earlier-ordered, always-eligible,
        // never-spent beat on the same occasion always beats (project order
        // is the selection tiebreak) — and `W-BEAT-PRIORITY-TIE`, which (dsl
        // 0.26.0 §8) reads the fact envelope's must sets and the root's
        // assert sites; appended after the cast pass, where it always was.
        let (guard_diags, beat_diags): (Vec<Vec<Diagnostic>>, _) = rayon::join(
            || {
                group_full
                    .par_iter()
                    .map(|(path, doc, folded)| {
                        let reported = result_ix
                            .get(path)
                            .map_or(&[][..], |&i| file_results[i].1.diagnostics.as_slice());
                        lute_check::check_fact_guards(path, doc, folded, fact_env, reported)
                    })
                    .collect()
            },
            || {
                // T3-5: a beat the per-file check rejected is left out of the
                // tie and shadow passes — its error is the one report. An
                // error folded into an earlier one (`(+N more: …)`) rejects
                // the beat it sits in just the same.
                let errors: lute_check::beats::ReportedErrors = group_full
                    .iter()
                    .filter_map(|(path, _, _)| {
                        let &i = result_ix.get(path)?;
                        let spans: Vec<_> = file_results[i]
                            .1
                            .diagnostics
                            .iter()
                            .filter(|d| d.severity == Severity::Error)
                            .flat_map(|d| std::iter::once(&d.span).chain(&d.covered))
                            .map(|s| s.byte_start..s.byte_end)
                            .collect();
                        (!spans.is_empty()).then(|| (path.clone(), spans))
                    })
                    .collect();
                lute_check::check_project_beats(
                    &group,
                    &beat_foldeds,
                    &producers,
                    Some(fact_env),
                    &errors,
                )
            },
        );
        for ((path, _, _), diags) in group_full.iter().zip(guard_diags) {
            for d in diags {
                project_diags.push((path.clone(), d));
            }
        }
        // dsl 0.24.0 §4: `W-CAST-ABSENT` re-decided under the fact envelope,
        // the beat ladders and the root's assert sites — a line the Must set,
        // the beats a ladder must have spent first, or a fact only its own
        // unit produces shows its speaker present at is dropped. dsl 0.25.0
        // §6: a line that follows a `changedOn` occasion in the scenario
        // graph is decided without `assume: true` for that relation — added.
        // (`ladder` and `producers` were computed alongside the fixpoint.)
        let after = lute_check::cast::occasions_before(&group, &beat_foldeds, &conn_graph);
        let no_ladder = BTreeMap::new();
        let no_after = BTreeMap::new();
        for (path, doc, folded) in group_full {
            if let Some(r) = result_ix.get(path).map(|&i| &mut file_results[i].1) {
                let project = lute_check::cast::PresenceProject {
                    env: fact_env,
                    ladder: ladder.get(path).unwrap_or(&no_ladder),
                    producers: &producers,
                    after: after.get(path).unwrap_or(&no_after),
                };
                let added = lute_check::cast::reconcile_presence(
                    &mut r.diagnostics,
                    path,
                    doc,
                    folded,
                    &project,
                );
                // dsl 0.28.0: a kind beat's `<match on="occasion.target">`
                // needs no arm for a member its `when` never holds for with
                // the facts in scope.
                lute_check::fact_check::reconcile_member_matches(
                    &mut r.diagnostics,
                    path,
                    doc,
                    folded,
                    fact_env,
                );
                if !added.is_empty() {
                    let text = std::fs::read_to_string(path).unwrap_or_default();
                    for mut d in added {
                        d.span = normalize_span_from_text(&text, d.span);
                        let at = r.diagnostics.partition_point(|x| {
                            (x.span.byte_start, &x.code) <= (d.span.byte_start, &d.code)
                        });
                        r.diagnostics.insert(at, d);
                    }
                }
            }
        }
        project_diags.extend(beat_diags);
        fact_envs.insert(root.clone(), fp.fact_env);
        // T10/T11: connectivity envelope (dsl §4.3). `PerDocEffects`
        // populated from T8 (per-scene `guaranteed`/`possible_writes`,
        // recomputed here from this root's own docs+resolved schema, keyed
        // by the SAME canonical key as `NodeId::Scene` -- the key's FIRST
        // `key_set` occurrence, mirroring `assemble_graph`'s own node
        // anchor) and T9 (`writes_on_complete` per quest id, EVERY resolved
        // quest present as a key incl. empty-write; an empty or AMBIGUOUS
        // id is omitted -- absence is `propagate`'s resolvability signal).
        // `d` = project-resolved `run.*`/`user.*` schema-default set (dsl
        // §4.3 spec lines 442-448), unioned across every doc's own resolved
        // schema in this root.
        let mut per_doc = envelope::PerDocEffects::default();
        let mut envelope_d: BTreeSet<String> = BTreeSet::new();
        let mut reads_per_scene: BTreeMap<String, Vec<(String, Span)>> = BTreeMap::new();
        // Per non-tainted, in-scope, entry-dependent read site: the exact
        // per-file `E-MAYBE-UNSET` diagnostic it would earn, keyed by
        // canonical scene key. Built HERE (not after `propagate`) because
        // it needs `local_diags`, discarded everywhere else -- `reads[i]`
        // and the i-th `E-MAYBE-UNSET` in `local_diags` are pushed
        // TOGETHER, unconditionally, at the SAME `check_read` call site
        // (defassign.rs), so zipping them by position is exact, not a
        // heuristic.
        let mut sites_per_scene: BTreeMap<String, Vec<(Span, String)>> = BTreeMap::new();
        // `owner: engine` paths: an unavailable read of one is advised a
        // guard or a default, never an `after:` (no scene sets it).
        let mut engine_owned: BTreeSet<String> = BTreeSet::new();
        for (_path, doc, folded) in group_full {
            envelope_d.extend(envelope::schema_defaults(&folded.env.state));
            engine_owned.extend(
                folded
                    .env
                    .state
                    .decls
                    .iter()
                    .filter(|(_, d)| d.owner == Some(lute_manifest::types::Owner::Engine))
                    .map(|(p, _)| p.clone()),
            );
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
        for (key, occurrences) in &key_set {
            let Some((scene_path, _)) = occurrences.first() else {
                continue;
            };
            let Some((_, doc, folded)) =
                group_ix.get(scene_path.as_path()).map(|&i| &group_full[i])
            else {
                continue;
            };
            let all_nodes: Vec<lute_syntax::ast::Node> = doc
                .shots
                .iter()
                .flat_map(|s| s.body.iter().cloned())
                .collect();
            // dsl 0.24.0: the same scope and beat-`when` assumption `check()`
            // walks with, so both passes see the same entry-dependent reads.
            let scope = defassign::Scope::of(folded);
            let beat_when = folded.typed.beat.as_ref().and_then(|b| b.when.as_ref());
            let (local_diags, assigned, reads) =
                check_definite_assignment(&all_nodes, &scope, beat_when);
            // T4.4/T4.6 carry-forward parity (dsl §7 soundness invariant): the
            // real `check()` pipeline (`check.rs::suppress_exhaustive_subject_reads`)
            // drops any `E-MAYBE-UNSET` whose span is a domain-exhaustive
            // `<match>` subject BEFORE `file_results` is ever populated -- a
            // read like that never earns a per-file `E-MAYBE-UNSET` standalone,
            // so it must never be treated as "entry-dependent" here either, or
            // this project-level recomputation (which calls
            // `check_definite_assignment` raw, unaware of that later
            // suppression) would newly error a file `check()` reports clean.
            let exhaustive_spans = defassign::exhaustive_match_subject_spans(&all_nodes, &scope);
            let is_exhaustive_subject = |span: &Span| {
                exhaustive_spans
                    .iter()
                    .any(|s| s.byte_start == span.byte_start && s.byte_end == span.byte_end)
            };
            per_doc.scene.insert(
                key.clone(),
                (
                    envelope::guaranteed(&assigned),
                    envelope::possible_writes(&all_nodes),
                ),
            );
            let maybe_unset_messages: Vec<&str> = local_diags
                .iter()
                .filter(|d| d.code == "E-MAYBE-UNSET")
                .map(|d| d.message.as_str())
                .collect();
            debug_assert_eq!(
                reads.len(),
                maybe_unset_messages.len(),
                "check_definite_assignment must push exactly one E-MAYBE-UNSET per \
                 entry-dependent read, in the same order"
            );
            let paired: Vec<((String, Span), &str)> =
                reads.into_iter().zip(maybe_unset_messages).collect();
            let sites: Vec<(Span, String)> = paired
                .iter()
                .filter(|((path, span), _)| {
                    envelope::in_envelope_scope(path) && !is_exhaustive_subject(span)
                })
                .map(|((_, span), msg)| (*span, (*msg).to_string()))
                .collect();
            let reads: Vec<(String, Span)> = paired
                .into_iter()
                .filter(|((_, span), _)| !is_exhaustive_subject(span))
                .map(|(r, _)| r)
                .collect();
            sites_per_scene.insert(key.clone(), sites);
            reads_per_scene.insert(key.clone(), reads);
        }
        let (envs, tainted) = envelope::propagate(&conn_graph, &per_doc, &envelope_d);
        // `check_envelope` returns BOTH grades together (see its own doc
        // comment); only the error grade joins `project_diags` -- the
        // warning grade is intentionally computed-and-discarded here (dsl
        // §4.3/§5: default-suppressed until T14's `lute scenario envelope`
        // exists to surface it). EVERY entry-dependent, in-scope,
        // non-tainted read is reconciled below regardless of its own
        // classification outcome.
        for (path, d) in envelope::check_envelope(
            &conn_graph,
            &envs,
            &tainted,
            &reads_per_scene,
            &engine_owned,
        ) {
            if d.severity == Severity::Error {
                project_diags.push((path, d));
            }
        }
        for (key, occurrences) in &key_set {
            let node_id = lute_check::connectivity::NodeId::Scene(key.clone());
            // Only reconcile (drop) a read's per-file `E-MAYBE-UNSET` when
            // its node has a REAL envelope to reclassify against: present
            // in `envs` AND not `tainted`. Per-node cycle recovery (spec
            // §4.1): a node ON or DOWNSTREAM of an `E-CONN-CYCLE` is the
            // ONLY kind `propagate` omits from `envs` (a cycle-independent
            // node keeps a real entry and IS reconciled here) — such a node
            // is exactly as untrustworthy as a tainted one: `check_envelope`
            // above already skips it (no replacement diagnostic emitted for
            // it either), so dropping its per-file diagnostic here would
            // silently lose a genuine local maybe-unset error with nothing
            // to replace it.
            if tainted.contains(&node_id) || !envs.contains_key(&node_id) {
                continue;
            }
            let Some((scene_path, _)) = occurrences.first() else {
                continue;
            };
            let Some(sites) = sites_per_scene.get(key) else {
                continue;
            };
            for (span, message) in sites {
                reconciled_reads.push((scene_path.clone(), *span, message.clone()));
            }
        }
        covered.extend(lute_check::colliding_occurrences(&group));
        entry_covered.extend(lute_check::colliding_entry_occurrences(&group));
        let heads: Vec<String> = group
            .iter()
            .flat_map(|item| {
                let quests = item.doc.quests.iter().map(|q| ("quest", &q.id));
                quests.chain(item.doc.entries.iter().map(|e| ("entry", &e.id)))
            })
            .filter(|(_, id)| id.contains('.'))
            .map(|(root, id)| format!("{root}.{id}."))
            .collect();
        if !heads.is_empty() {
            for item in &group {
                dotted_root_of.insert(item.path.to_path_buf(), dotted_heads.len());
            }
            dotted_heads.push(heads);
        }
    }
    for (path, result) in &mut file_results {
        result.diagnostics.retain(|d| {
            let quest_dup_covered = d.code == "E-QUEST-ID-DUP"
                && covered.iter().any(|(p, s)| p == path && *s == d.span);
            let entry_dup_covered = (d.code == lute_check::E_ENTRY_ID_DUP
                || d.code == lute_check::E_ENTRY_SERIES_ORDER)
                && entry_covered.iter().any(|(p, s)| p == path && *s == d.span);
            let envelope_reconciled = d.code == "E-MAYBE-UNSET"
                && reconciled_reads
                    .iter()
                    .any(|(p, s, m)| p == path && *s == d.span && *m == d.message);
            // The path an `E-UNDECLARED` names first, read of a dotted id this
            // root declares.
            let dotted_read = d.code == "E-UNDECLARED"
                && dotted_root_of.get(path).is_some_and(|&i| {
                    d.message
                        .strip_prefix('`')
                        .and_then(|m| m.split_once('`'))
                        .is_some_and(|(read, _)| {
                            dotted_heads[i].iter().any(|h| read.starts_with(h.as_str()))
                        })
                });
            !quest_dup_covered && !entry_dup_covered && !envelope_reconciled && !dotted_read
        });
        result.ok = !result
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error);
    }
    // Defect fix (persona review, connectivity T-final): every project-wide
    // diagnostic anchored via `lute_check::meta::meta_key_span` (the
    // `E-CONN-EPISODE-ID-DUP`/`E-CONN-UNKNOWN-NODE`/`E-CONN-CYCLE`/
    // `E-CONN-UNREACHABLE` scene anchors) carries a CORRECT byte range but
    // a ZEROED `line`/`column` -- that helper's own documented contract:
    // "`crate::check`'s `normalize_spans` recomputes them from the byte
    // offsets." Per-file diagnostics get that treatment inside `check()`
    // itself; these are assembled here, project-wide, and never pass
    // through it, so they printed `0:0` verbatim. Mirror the SAME
    // normalization here, per diagnostic's own file text -- a `Span` that
    // already carries a real line/col (a quest's parser-produced
    // `id_span`/`after_span`, or `E-STATE-MAYBE-UNAVAILABLE`'s read-site
    // span) recomputes identically from the SAME byte offsets against the
    // SAME source text, so this is a no-op for those, never a regression.
    let mut project_diag_text_cache: BTreeMap<PathBuf, String> = BTreeMap::new();
    for (path, d) in &mut project_diags {
        let text = project_diag_text_cache
            .entry(path.clone())
            .or_insert_with(|| std::fs::read_to_string(path.as_path()).unwrap_or_default());
        d.span = normalize_span_from_text(text, d.span);
    }
    (file_results, project_diags, nodes_by_path, fact_envs)
}

/// Round-5 T3-4: a diagnostic a document carries only because it imports a
/// broken file is reported where the fault IS, not at the importer.
///
/// - Its cause lies in a `.lute` file this walk checks itself (a component)
///   and that file's own check already reports every cause at the same
///   position: every importer's copy is dropped. The component file carries
///   the failure; its callers are judged on their own content.
/// - Its cause lies in a schema (a non-`.lute` file, never a walked document):
///   each cause becomes ONE project-wide diagnostic at the schema's own line,
///   counting the importers, and every importer's copy is dropped — so no
///   importer is `failed` while another is `ok` for the same fault.
///
/// Anything else (a caller-specific body fault the component's own check does
/// not report) is left for [`rollup_component_body_diags`].
pub fn relocate_imported_diags(
    file_results: &mut [(PathBuf, lute_check::CheckResult)],
    project_diags: &mut Vec<(PathBuf, Diagnostic)>,
    dir: &Path,
) {
    use std::collections::{BTreeMap, BTreeSet};

    let canon_of = |p: &Path| {
        std::fs::canonicalize(p)
            .unwrap_or_else(|_| p.to_path_buf())
            .display()
            .to_string()
    };
    let canon: Vec<String> = file_results.iter().map(|(p, _)| canon_of(p)).collect();
    let by_canon: BTreeMap<&str, usize> = canon
        .iter()
        .enumerate()
        .map(|(i, c)| (c.as_str(), i))
        .collect();
    // Whether `file`'s own check reports `inner` (or, for an already-folded
    // body copy, every cause it wraps) at the same position, as a LOCAL
    // diagnostic — never another cross-file copy, so two files naming each
    // other cannot both drop theirs.
    fn reported_at(
        file: &str,
        inner: &Diagnostic,
        results: &[(PathBuf, lute_check::CheckResult)],
        by_canon: &BTreeMap<&str, usize>,
    ) -> bool {
        let Some(&i) = by_canon.get(file) else {
            return false;
        };
        let own = results[i].1.diagnostics.iter().any(|h| {
            h.code == inner.code
                && h.span.byte_start == inner.span.byte_start
                && h.message == inner.message
                && h.related.iter().all(|r| r.file == file)
        });
        own || (!inner.related.is_empty()
            && inner
                .related
                .iter()
                .all(|r| reported_at(&r.file, &r.diagnostic, results, by_canon)))
    }
    let is_schema =
        |file: &str| Path::new(file).extension().and_then(|e| e.to_str()) != Some("lute");

    let mut drop: Vec<BTreeSet<usize>> = vec![BTreeSet::new(); file_results.len()];
    // (schema file, byte_start, code, message) -> (diagnostic, importers):
    // a schema's reports print in its own line order.
    let mut moved: BTreeMap<(String, usize, String, String), (Diagnostic, usize)> = BTreeMap::new();
    for (fi, (_, result)) in file_results.iter().enumerate() {
        let here = &canon[fi];
        for (di, d) in result.diagnostics.iter().enumerate() {
            let foreign: Vec<_> = d.related.iter().filter(|r| &r.file != here).collect();
            if foreign.is_empty() || foreign.len() != d.related.len() {
                continue;
            }
            if foreign
                .iter()
                .all(|r| reported_at(&r.file, &r.diagnostic, file_results, &by_canon))
            {
                drop[fi].insert(di);
            } else if foreign.iter().all(|r| is_schema(&r.file)) {
                drop[fi].insert(di);
                for r in foreign {
                    let key = (
                        r.file.clone(),
                        r.diagnostic.span.byte_start,
                        r.diagnostic.code.clone(),
                        r.diagnostic.message.clone(),
                    );
                    moved
                        .entry(key)
                        .or_insert_with(|| (r.diagnostic.clone(), 0))
                        .1 += 1;
                }
            }
        }
    }
    for ((_, result), drop) in file_results.iter_mut().zip(drop) {
        if drop.is_empty() {
            continue;
        }
        let mut i = 0;
        result.diagnostics.retain(|_| {
            i += 1;
            !drop.contains(&(i - 1))
        });
        result.ok = !result
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error);
    }
    let canon_dir = std::fs::canonicalize(dir).ok();
    for ((file, ..), (mut d, n)) in moved {
        let path = PathBuf::from(&file);
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        d.span = normalize_span_from_text(&text, d.span);
        // A default the manifest supplies is used by the documents, not imported.
        let verb = if path.file_name().is_some_and(|n| n == "lute.project.yaml") {
            "applies to"
        } else {
            "imported by"
        };
        d.message = format!("{} ({verb} {n} document{})", d.message, plural(n));
        let shown = canon_dir
            .as_deref()
            .and_then(|c| path.strip_prefix(c).ok())
            .map_or_else(|| path.clone(), |rel| dir.join(rel));
        project_diags.push((shown, d));
    }
}

/// dsl 0.10.0 §9 rule 2: fold identical component-body diagnostics across
/// callers into one, keeping the first in byte-sorted path order and
/// summarising the rest.
///
/// `validate_components` runs once per importing document, so N callers of one
/// broken component produce N separate `check()` runs and N identical
/// diagnostics — eleven modules, eleven identical messages, at line 1 column 1
/// of eleven files that are all correct. The roll-up belongs here because this
/// is the first place those runs meet.
///
/// The key is `(code, message)`. After §9 rule 1 a component-body message reads
/// ``component `{name}` ({src}): {original}``, which is byte-identical across
/// callers exactly when the problem is caller-INDEPENDENT and different when it
/// is not — `E-BAD-ENUM` enumerates the resolved domain, so two callers with
/// different vocabularies differ in the message itself. That is rule 2's
/// boundary precisely, and it needs no marker field: a caller-specific fault
/// stays with its own caller, where the caller is visible.
pub fn rollup_component_body_diags(file_results: &mut [(PathBuf, lute_check::CheckResult)]) {
    use std::collections::BTreeMap;

    // Pass 1: count, in byte-sorted path order, which is `collect_project_docs`'
    // own order — so "the first" is deterministic without re-sorting.
    let mut counts: BTreeMap<(String, String), usize> = BTreeMap::new();
    for (path, result) in file_results.iter() {
        for d in &result.diagnostics {
            if is_component_body_diag(d, path) {
                *counts
                    .entry((d.code.clone(), d.message.clone()))
                    .or_insert(0) += 1;
            }
        }
    }

    // Pass 2: keep the first of each group, annotate it, drop the rest.
    let mut seen: std::collections::BTreeSet<(String, String)> = Default::default();
    for (path, result) in file_results.iter_mut() {
        let mut kept = Vec::with_capacity(result.diagnostics.len());
        for mut d in std::mem::take(&mut result.diagnostics) {
            if !is_component_body_diag(&d, path) {
                kept.push(d);
                continue;
            }
            let key = (d.code.clone(), d.message.clone());
            if !seen.insert(key.clone()) {
                continue; // a later caller reporting the same problem
            }
            let others = counts.get(&key).copied().unwrap_or(1).saturating_sub(1);
            if others > 0 {
                d.message = format!("{} (+{others} more caller{})", d.message, plural(others));
            }
            kept.push(d);
        }
        result.diagnostics = kept;
        result.ok = !result
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error);
    }
}

/// A diagnostic surfaced from ANOTHER file: it carries a `related` entry whose
/// `file` is not the document it is reported on (§9 rule 1's cross-file
/// attribution). That is what distinguishes a component-body report from an
/// ordinary local diagnostic, without a new marker field.
fn is_component_body_diag(d: &Diagnostic, path: &Path) -> bool {
    let here = path.display().to_string();
    d.related.iter().any(|r| r.file != here)
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

/// Collect acceptance declarations from mocks and scenario test metadata.
fn mocked_accepts_under(root: &Path) -> BTreeMap<String, Vec<PathBuf>> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                if !path.join("lute.project.yaml").is_file() { stack.push(path); }
                continue;
            }
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
            let in_mocks = path.parent().and_then(|p| p.file_name()).and_then(|n| n.to_str()) == Some("mocks");
            if !(name.ends_with(".test.yaml") || (in_mocks && name.ends_with(".yaml"))) { continue; }
            let Ok(text) = std::fs::read_to_string(&path) else { continue };
            let Ok(serde_yaml::Value::Mapping(top)) = serde_yaml::from_str(&text) else { continue };
            let rel = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
            if let Some(serde_yaml::Value::Sequence(items)) = top.get("accepts") {
                for id in items.iter().filter_map(|item| item.as_str()) {
                    let files = out.entry(id.to_string()).or_insert_with(Vec::new);
                    if !files.iter().any(|file| file == &rel) { files.push(rel.clone()); }
                }
            }
        }
    }
    for files in out.values_mut() { files.sort(); }
    out
}

fn cast_home(
    root: &Path,
    plugins_dir: Option<&Path>,
    origins: &[&BTreeMap<String, lute_check::rel_schema::DeclOrigin>],
    id: &str,
) -> Option<(PathBuf, Span)> {
    if let Some(found) = plugins_dir.and_then(|dir| plugin_cast_home(dir, id)) { return Some(found); }
    let origin = origins.iter().find_map(|origin| origin.get(id))?;
    let canon_root = std::fs::canonicalize(root).ok();
    let shown = canon_root.as_deref().and_then(|canon| origin.file.strip_prefix(canon).ok())
        .map_or_else(|| origin.file.clone(), |relative| root.join(relative));
    Some((shown, origin.span))
}

fn plugin_cast_home(plugins_dir: &Path, id: &str) -> Option<(PathBuf, Span)> {
    let mut packages: Vec<PathBuf> = std::fs::read_dir(plugins_dir).ok()?.flatten()
        .map(|entry| entry.path()).filter(|path| path.join("plugin.yaml").is_file()).collect();
    packages.sort();
    for package in packages {
        let Ok(text) = std::fs::read_to_string(package.join("plugin.yaml")) else { continue };
        let Ok(manifest) = serde_yaml::from_str::<serde_yaml::Value>(&text) else { continue };
        let Some(rel) = manifest.get("exports").and_then(|v| v.get("cast")).and_then(|v| v.as_str()) else { continue };
        let export = package.join(rel);
        let mut files: Vec<PathBuf> = if export.is_dir() {
            let Ok(entries) = std::fs::read_dir(export) else { continue };
            entries.flatten().map(|entry| entry.path())
                .filter(|path| matches!(path.extension().and_then(|e| e.to_str()), Some("yaml" | "yml"))).collect()
        } else { vec![export] };
        files.sort();
        for file in files {
            let Ok(text) = std::fs::read_to_string(&file) else { continue };
            let Some(offset) = lute_check::rel_schema::cast_entry_offset(&text, id) else { continue };
            return Some((file, Span::from_bytes(&lute_core_span::TextIndex::new(&text), offset, offset + id.len())));
        }
    }
    None
}
