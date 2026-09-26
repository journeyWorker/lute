//! `lute scenario envelope <node>`: a node's Guaranteed/Possible state
//! envelope, its writers, and its fact envelope.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_check::envelope;
use lute_core_span::{Severity, Span};

use crate::cmd_scenario::{
    node_ref_to_id, primary_node_ambiguity_note, resolve_node_ref, NodeRef, RootScenario,
};
use crate::knowledge;
use crate::project::ByRoot;

/// dsl 0.20.0 §6: the fact envelope — each fact guaranteed on arrival with
/// where it is established (an assert site, an enclosing guard, a seed).
fn print_must_facts(out: &mut String, facts: &[lute_check::fact_env::MustFact]) {
    if facts.is_empty() {
        outln!(out, "    (none)");
    }
    for m in facts {
        outln!(out, "    - {} ({})", m.fact, m.provenance);
    }
}

/// Every node from which `node` is transitively reachable in the prerequisite
/// graph — the writers whose writes provably happen BEFORE control reaches
/// it, which is the only thing a PRE-ENTRY envelope may claim. `g.edges` is
/// keyed `prerequisite -> dependent`, so this is a reverse walk.
///
/// `node` itself is excluded unless a cycle puts it upstream of itself; that
/// is deliberate — the tables are explicitly "before its own writes".
fn ancestors_of(
    g: &lute_check::connectivity::ConnGraph,
    node: &lute_check::connectivity::NodeId,
) -> BTreeSet<lute_check::connectivity::NodeId> {
    let mut rev: BTreeMap<
        &lute_check::connectivity::NodeId,
        Vec<&lute_check::connectivity::NodeId>,
    > = BTreeMap::new();
    for (prereq, deps) in &g.edges {
        for dep in deps {
            rev.entry(dep).or_default().push(prereq);
        }
    }
    let mut seen: BTreeSet<lute_check::connectivity::NodeId> = BTreeSet::new();
    let mut stack: Vec<&lute_check::connectivity::NodeId> =
        rev.get(node).cloned().unwrap_or_default();
    while let Some(n) = stack.pop() {
        if !seen.insert(n.clone()) {
            continue;
        }
        if let Some(ps) = rev.get(n) {
            stack.extend(ps.iter().copied());
        }
    }
    seen
}

/// Invert `PerDocEffects` into `path -> (writers upstream of `node`, the
/// rest)`. A scene contributes its own `possible_writes`; a quest contributes
/// its `writesOnComplete`, which is what draws manifest-gap.lute's completion
/// handler as a writer of `run.vesnaTrust` — the edge nobody could see,
/// though what-vesna-carries activates on exactly that path (#15, T9.14).
///
/// The split is load-bearing, and the plan asked for the join UNSPLIT. A flat
/// project-wide list names `scene(haven.s01ep09)` as a writer in
/// `haven.s01ep01`'s PRE-ENTRY envelope, which is false — eight scenes
/// separate them — and, being project-wide, it renders identically at every
/// node, which is the very defect #15 opens with. Dropping the non-upstream
/// half instead loses `quest(manifestGap)` from `quest:whatVesnaCarries`,
/// #15's own last verify bullet, because a no-`after` quest is never a graph
/// node and manifestGap is not upstream of it in any case. Both halves are
/// reported, each labelled with what is actually known about it: a
/// non-upstream writer may be downstream, unordered, or ungraphed, so the
/// second label claims only that its write is NOT provably before this node.
type WriterSplit = BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)>;

fn writers_of(scenario: &RootScenario, node: &lute_check::connectivity::NodeId) -> WriterSplit {
    let upstream = ancestors_of(&scenario.graph, node);
    let mut out: WriterSplit = BTreeMap::new();
    let mut record = |path: &String, id: lute_check::connectivity::NodeId, label: String| {
        let entry = out.entry(path.clone()).or_default();
        if upstream.contains(&id) {
            entry.0.insert(label);
        } else {
            entry.1.insert(label);
        }
    };
    for (key, (_guaranteed, possible)) in &scenario.per_doc.scene {
        for path in possible {
            record(
                path,
                lute_check::connectivity::NodeId::Scene(key.clone()),
                format!("scene({key})"),
            );
        }
    }
    for (id, writes) in &scenario.per_doc.quest_writes_on_complete {
        for path in writes {
            record(
                path,
                lute_check::connectivity::NodeId::Quest(id.clone()),
                format!("quest({id}) on completion"),
            );
        }
    }
    out
}

pub(crate) fn join(set: &BTreeSet<String>) -> String {
    set.iter().cloned().collect::<Vec<_>>().join(", ")
}

/// Print a path set with each path's writers named beside it.
fn print_path_set_with_writers(out: &mut String, paths: &BTreeSet<String>, writers: &WriterSplit) {
    if paths.is_empty() {
        outln!(out, "    (none)");
        return;
    }
    for path in paths {
        let (upstream, other) = match writers.get(path) {
            Some(w) => (&w.0, &w.1),
            None => (&BTreeSet::new(), &BTreeSet::new()),
        };
        // A path in the envelope that nothing upstream writes got there from
        // the schema-default floor `D`. Saying so is the answer to "why is
        // this readable"; a blank column would read as a missing join.
        let head = if upstream.is_empty() {
            "(nothing on a declared route reaching here — schema default only)".to_string()
        } else {
            join(upstream)
        };
        let tail = if other.is_empty() {
            String::new()
        } else {
            format!(
                "; also written, but not provably before this node: {}",
                join(other)
            )
        };
        outln!(out, "    - {path}   written by: {head}{tail}");
    }
}

/// The relational layer the scalar envelope tables cannot show: every
/// declared relation, whether static analysis can produce it, and who makes
/// it true — in `lute scenario knowledge`'s words (T3-2: asserting
/// documents, seed facts, the engine for a `reserved` relation, the rules).
/// `producible` reads the SAME reachability-gated assert sites
/// (`live_assert_relations`) the `check-project` fact envelope seeds from;
/// this renders relation-level facts rather than deciding anything (#15,
/// T4.7).
fn print_facts_section(out: &mut String, scenario: &RootScenario, root: &Path) {
    let vocab = &scenario.rel_vocab;
    if vocab.relations.is_empty() {
        return;
    }
    let live = lute_check::connectivity::live_assert_relations(
        &scenario.docs,
        &scenario.reach,
        &scenario.ambiguous_quests,
        &scenario.unreachable_quests,
        &vocab.effect_directives,
    );
    let producible = lute_check::producible::producible(vocab, &live);
    let per_doc = lute_check::connectivity::assert_relations_per_doc(
        &scenario.docs,
        &vocab.effect_directives,
    );

    outln!(
        out,
        "  Facts (the relational layer — declared relations, how each becomes true):"
    );
    for (name, decl) in &vocab.relations {
        let prod = if producible.get(name).copied().unwrap_or(false) {
            "producible"
        } else {
            "NOT producible by any declared route"
        };
        let writers: Vec<String> = per_doc
            .iter()
            .filter(|(_, rels)| rels.contains(name))
            .map(|(p, _)| p.strip_prefix(root).unwrap_or(p).display().to_string())
            .collect();
        let sites: Vec<&str> = writers.iter().map(String::as_str).collect();
        let by = knowledge::relation_producers(name, vocab, &sites);
        outln!(out, "    - {name}/{} ({prod}) — {by}", decl.args.len());
    }
    if !vocab.rules.is_empty() {
        outln!(out, "  Rules:");
        for r in &vocab.rules {
            outln!(out, "    - {}", r.raw);
        }
    }
}

/// True when the project's prerequisite graph contains a cycle (`E-CONN-CYCLE`,
/// dsl §2.4/§4.1 §A). Kahn's algorithm in `assemble_graph` emits every node
/// EXCEPT the cycle members and everything transitively downstream of one, so
/// a graph is cyclic iff `topo_order` is shorter than the node set — a
/// self-contained signal that needs no diagnostic replay.
fn graph_has_cycle(scenario: &RootScenario) -> bool {
    scenario.graph.topo_order.len() < scenario.graph.nodes.len()
}

/// True when `node` is ON or DOWNSTREAM of a prerequisite cycle
/// (`E-CONN-CYCLE`, dsl §2.4/§4.1 §A) — per-node cycle degradation (spec
/// §4.1). `assemble_graph` excludes exactly those nodes from `topo_order`, so
/// [`lute_check::connectivity::check_reachability`] AND [`envelope::propagate`]
/// (each iterating `topo_order`) populate NEITHER `reach` NOR `envs` for them;
/// a cycle-INDEPENDENT node keeps its real verdict and is never degraded. The
/// test is a node absent from `reach` in a root that does contain a cycle —
/// the same absence [`reach_verdict_text`]'s cycle arm keys off, reused
/// verbatim so a node's reach verdict and its envelope note never disagree.
///
/// [`reach_verdict_text`]: crate::cmd_scenario::reach::reach_verdict_text
pub(crate) fn node_cycle_degraded(
    scenario: &RootScenario,
    node: &lute_check::connectivity::NodeId,
) -> bool {
    !scenario.reach.contains_key(node) && graph_has_cycle(scenario)
}

/// Print the explicit per-node `E-CONN-CYCLE` degradation note (C-honesty,
/// persona review), mirroring [`reach_verdict_text`]'s cycle wording so a
/// cyclic-degraded node's envelope is never silently indistinguishable from a
/// genuinely-empty one. Printed ONLY for a node on or downstream of the cycle
/// (see [`node_cycle_degraded`]); a cycle-independent node prints its real
/// tables with no note. Prepended before the tables (which fall back to the
/// schema-default D/D floor when this node's `envs` entry is absent).
///
/// [`reach_verdict_text`]: crate::cmd_scenario::reach::reach_verdict_text
fn print_cycle_envelope_note(out: &mut String) {
    outln!(
        out,
        "  note: envelope unavailable — this node is on or downstream of a prerequisite cycle \
         (E-CONN-CYCLE); the Guaranteed/Possible tables below cannot be computed under your \
         declared routes and fall back to the schema-default floor."
    );
}

/// Print a scene node's Guaranteed/Possible envelope tables (T10) plus its
/// `Possible \ Guaranteed` warning-grade READS (contract #2): T11's
/// [`envelope::check_envelope`] already computes BOTH grades together and
/// returns them — `check-project` filters to `Severity::Error` only and
/// drops the warning grade; this RE-derives the SAME call, singleton-scoped
/// to `key` so every returned diagnostic necessarily belongs to this node,
/// and keeps the warning grade instead. Never a second classification pass
/// — `check_envelope` is reused verbatim, never re-implemented.
///
/// A bundle beat (`node_id` a `NodeId::Beat`) prints through here too: an
/// edgeless entry node, so its tables are the entry floor.
fn print_scene_envelope(
    out: &mut String,
    scenario: &RootScenario,
    node_id: &lute_check::connectivity::NodeId,
    key: &str,
    root: &Path,
) {
    outln!(
        out,
        "envelope for {node_id} (pre-entry — state available when control REACHES this node, \
         before its own writes):"
    );
    if node_cycle_degraded(scenario, node_id) {
        print_cycle_envelope_note(out);
    }
    if scenario.tainted.contains(node_id) {
        outln!(
            out,
            "  note: this node's envelope is a defaults-only placeholder -- its `after` \
             formula is malformed or references an unresolved node (E-CONN-PROFILE/\
             E-CONN-UNKNOWN-NODE)."
        );
    }
    let env = scenario
        .envs
        .get(node_id)
        .cloned()
        .unwrap_or_else(|| envelope::Env {
            guaranteed: scenario.envelope_d.clone(),
            possible: scenario.envelope_d.clone(),
        });
    let writers = writers_of(scenario, node_id);
    outln!(
        out,
        "  Guaranteed (safe to read under your declared routes):"
    );
    print_path_set_with_writers(out, &env.guaranteed, &writers);
    // T3-15: Possible ⊇ Guaranteed; print only what is new beside the table
    // above instead of every guaranteed path a second time.
    let possible_only: BTreeSet<String> =
        env.possible.difference(&env.guaranteed).cloned().collect();
    outln!(
        out,
        "  Possible (set on SOME but not every declared route reaching this node; the \
         Guaranteed paths above are not repeated):"
    );
    print_path_set_with_writers(out, &possible_only, &writers);
    outln!(
        out,
        "  Guaranteed facts (hold on every declared route reaching this node):"
    );
    print_must_facts(out, scenario.scene_must.get(key).map_or(&[], Vec::as_slice));

    let mut single: BTreeMap<String, Vec<(String, Span)>> = BTreeMap::new();
    if let Some(reads) = scenario.reads_per_scene.get(key) {
        single.insert(key.to_string(), reads.clone());
    }
    let diags =
        envelope::check_envelope(&scenario.graph, &scenario.envs, &scenario.tainted, &single);
    outln!(
        out,
        "  Possible \\ Guaranteed -- warning-grade reads (set on SOME but not every declared \
         route; suppressed by default in `check-project`, surfaced here):"
    );
    let mut any = false;
    for (path, d) in &diags {
        if d.severity != Severity::Warning {
            continue;
        }
        any = true;
        outln!(
            out,
            "    - {}:{}:{}: {}",
            path.display(),
            d.span.line,
            d.span.column,
            d.text()
        );
    }
    if !any {
        outln!(out, "    (none)");
    }
    print_facts_section(out, scenario, root);
}

/// Print a quest node's envelope (T12 [`envelope::quest_envelope`]) — full
/// tables for an `after`-opted-in quest, defaults-only `D` plus the
/// enrichment note for a bare quest (dsl §4.4) — plus its `Possible \
/// Guaranteed` SET as plain inventory. [`envelope::check_envelope`] is
/// SCENE-ONLY by design (its own doc comment: quest reads stay
/// `check_quest_guard_defassign`'s territory), so this is NEVER labeled
/// as the T11 warning-grade read-site class (Main review) — there is no
/// read-SITE list for a quest at all, only the plain set difference.
fn print_quest_envelope(
    out: &mut String,
    scenario: &RootScenario,
    id: &str,
    quest: &lute_syntax::ast::Quest,
    root: &Path,
) {
    let node_id = lute_check::connectivity::NodeId::Quest(id.to_string());
    outln!(
        out,
        "envelope for {node_id} (pre-entry — state available when control REACHES this node, \
         before its own writes):"
    );
    // The E-CONN-CYCLE degradation note applies ONLY to a graph-positioned
    // quest (`after.is_some()`) that is itself cyclic/downstream:
    // `quest_envelope` returns the defaults-only D/D floor for an after-less
    // quest REGARDLESS of graph topology, so such a quest's tables did NOT
    // degrade due to the cycle -- and `node_cycle_degraded` would misfire on
    // it (a no-`after` quest is never a graph node, so it is trivially absent
    // from `reach`), so the `after.is_some()` guard is REQUIRED. A cycle-
    // independent `after` quest keeps its real tables with no note (per-node
    // recovery, spec §4.1); only a cyclic/downstream one prints the note.
    if quest.after.is_some() && node_cycle_degraded(scenario, &node_id) {
        print_cycle_envelope_note(out);
    }
    let qe = envelope::quest_envelope(quest, &scenario.graph, &scenario.envs, &scenario.envelope_d);
    let writers = writers_of(scenario, &node_id);
    outln!(
        out,
        "  Guaranteed (safe to read under your declared routes):"
    );
    print_path_set_with_writers(out, &qe.env.guaranteed, &writers);
    // T3-15: Possible ⊇ Guaranteed, so the full set printed every guaranteed
    // path twice; only the difference is new information.
    let possible_only: BTreeSet<String> = qe
        .env
        .possible
        .difference(&qe.env.guaranteed)
        .cloned()
        .collect();
    // #33 / T4.10: this sentence named `T11` (an internal task label) and
    // `check_quest_guard_defassign` (a Rust function) at an AUTHOR. Neither
    // appears anywhere on the website, so neither is lookupable. The
    // distinction the sentence exists to draw is real and is kept; only the
    // vocabulary changes. The doc comment above keeps both terms — that reader
    // has the source open, which is exactly the audience this message was
    // wrongly addressed to.
    outln!(
        out,
        "  Possible (set on SOME but not every declared route reaching this quest; \
         the Guaranteed paths above are not repeated) -- inventory only: unlike a scene's, this \
         list is not a set of warned read sites; a quest's guard reads are checked where they \
         are written, not against this table:"
    );
    print_path_set_with_writers(out, &possible_only, &writers);
    if qe.enrichment_note {
        outln!(
            out,
            "  note: this quest declares no `after` attribute, so this is the defaults-only \
             `D` table; declaring `after` on quest:{id} would enrich this table \
             with the full project-resolved envelope."
        );
    }
    print_facts_section(out, scenario, root);
}

pub(crate) fn run_scenario_envelope(
    out: &mut String,
    dir: &Path,
    by_root: &ByRoot,
    file_results: &[(PathBuf, lute_check::CheckResult)],
    node_id_raw: &str,
) -> ExitCode {
    let (node_ref, root, scenario) = match resolve_node_ref(dir, by_root, file_results, node_id_raw)
    {
        Ok(v) => v,
        Err(code) => return code,
    };
    outln!(out, "project root: {}", root.display());
    if let Some(note) = primary_node_ambiguity_note(&scenario, &node_ref) {
        let node_id = node_ref_to_id(&node_ref);
        outln!(out, "envelope for {node_id}: unavailable -- {note}");
        return ExitCode::SUCCESS;
    }
    match &node_ref {
        NodeRef::Scene(key) | NodeRef::Beat(key) => {
            print_scene_envelope(out, &scenario, &node_ref_to_id(&node_ref), key, root)
        }
        NodeRef::Quest(id) => {
            let Some(quest) = scenario
                .docs
                .iter()
                .flat_map(|(_, d)| d.quests.iter())
                .find(|q| &q.id == id)
            else {
                eprintln!("lute: internal error: quest `{id}` resolved but no declaration found");
                return ExitCode::from(2);
            };
            print_quest_envelope(out, &scenario, id, quest, root);
        }
    }
    ExitCode::SUCCESS
}
