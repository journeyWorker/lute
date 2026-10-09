//! `lute scenario reach <node>`: a node's declared prerequisite structure
//! and reach verdict.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::cmd_scenario::{
    node_ref_to_id, primary_node_ambiguity_note, resolve_node_ref, RootScenario,
};
use crate::project::ByRoot;

/// Render a [`lute_manifest::semantics::prereq::PrereqFormula`] back to CEL-like text, fully
/// parenthesized so the `&&`/`||` nesting is always visible — a
/// `visited(A) || visited(B)` node is reachable via A OR B, never rendered
/// as a flat list that could blur that into "requires A and B" (Main
/// review: routes must never be flattened away).
pub(crate) fn format_prereq(f: &lute_manifest::semantics::prereq::PrereqFormula) -> String {
    match f {
        lute_manifest::semantics::prereq::PrereqFormula::Visited(key) => format!("visited({})", quote_cel_string(key)),
        lute_manifest::semantics::prereq::PrereqFormula::Completed(id) => {
            format!("completed({})", quote_cel_string(id))
        }
        lute_manifest::semantics::prereq::PrereqFormula::Active(id) => {
            format!("active({})", quote_cel_string(id))
        }
        lute_manifest::semantics::prereq::PrereqFormula::And(l, r) => {
            format!("({} && {})", format_prereq(l), format_prereq(r))
        }
        lute_manifest::semantics::prereq::PrereqFormula::Or(l, r) => {
            format!("({} || {})", format_prereq(l), format_prereq(r))
        }
    }
}

/// Quote+escape a `visited`/`completed`/`active` atom id for CEL-like
/// rendering.
/// JSON string-literal escaping (`serde_json::to_string`) is a safe,
/// well-tested superset of what a CEL string literal needs
/// (backslash/quote/control-char escaping) — a raw `format!("\"{id}\"")`
/// interpolation (Main review) would render an id containing an embedded
/// `"`, `\`, or control character verbatim, breaking the printed
/// structure's own quoting. `String` -> JSON serialization is infallible
/// (a Rust `String` is always valid UTF-8, which `serde_json` always
/// accepts), so the `Result` is unwrapped unconditionally.
fn quote_cel_string(s: &str) -> String {
    serde_json::to_string(s).expect("String -> JSON serialization is infallible")
}

/// The evidence level for a reachability claim. Structural fallback cases
/// mirror the text verdict and preserve cycle-degraded uncertainty.
pub(crate) fn reach_evidence(
    scenario: &RootScenario,
    node: &lute_check::connectivity::NodeId,
) -> &'static str {
    use lute_check::connectivity::{NodeId, Reachability};
    if let Some(r) = scenario.reach.get(node) {
        return match r {
            Reachability::Reachable | Reachability::Unreachable => "proven",
            Reachability::Unknown => "unknown",
        };
    }
    match node {
        NodeId::Quest(id) if scenario.ambiguous_quests.contains(id) => "unknown",
        NodeId::Quest(id) if scenario.dead_required_objective_quests.contains(id) => "proven",
        NodeId::Quest(id) if scenario.unreachable_quests.contains(id) => "proven",
        NodeId::Quest(id) if !scenario.quest_ids.contains(id) => "unknown",
        NodeId::Quest(id)
            if !scenario
                .graph
                .nodes
                .contains_key(&NodeId::Quest(id.clone())) =>
        {
            "proven"
        }
        NodeId::Scene(key) if !scenario.key_set.contains_key(key) => "unknown",
        _ => "unknown",
    }
}

/// The reachability claim for `node`, under the declared routes. A plain
/// quest with no `after` is unanchored; omitted graph nodes are cycle-degraded.
pub(crate) fn reach_verdict_text(
    scenario: &RootScenario,
    node: &lute_check::connectivity::NodeId,
) -> String {
    use lute_check::connectivity::{NodeId, Reachability};
    let text = if let Some(r) = scenario.reach.get(node) {
        match r {
            Reachability::Reachable => {
                "Reachable — a satisfiable route exists under your declared routes.".to_string()
            }
            Reachability::Unreachable => "Unreachable — no satisfiable route exists under your \
                 declared routes (E-CONN-UNREACHABLE)."
                .to_string(),
            Reachability::Unknown => "Unknown — this analysis cannot prove reachability either \
                 way under your declared routes."
                .to_string(),
        }
    } else {
        match node {
            NodeId::Quest(id) if scenario.ambiguous_quests.contains(id) => {
                "Unknown — ambiguous quest id (more than one declaration) under your declared \
                 routes."
                    .to_string()
            }
            NodeId::Quest(id) if scenario.dead_required_objective_quests.contains(id) => {
                "Unreachable — this quest has a provably dead REQUIRED objective, so it can never \
                 complete (E-OBJECTIVE-UNSATISFIABLE), under your \
                 declared routes."
                    .to_string()
            }
            NodeId::Quest(id) if scenario.unreachable_quests.contains(id) => {
                "Unreachable — quest lifecycle proves this quest can never complete \
                 (E-QUEST-UNREACHABLE), under your declared routes."
                    .to_string()
            }
            // Main review fix: an id referenced by a formula but never declared
            // anywhere in this root (E-CONN-UNKNOWN-NODE's own concern) must
            // read Unknown -- checked BEFORE the "plain quest, no `follows`"
            // fallback below, since an undeclared id is trivially also absent
            // from `graph.nodes` and would otherwise be misreported Reachable.
            NodeId::Quest(id) if !scenario.quest_ids.contains(id) => {
                "Unknown — this quest id is not declared anywhere in this project root \
                 (E-CONN-UNKNOWN-NODE), under your declared routes."
                    .to_string()
            }
            // dsl 0.21.0 §7a.5: a declared quest without `follows=` is not a graph
            // node at all — UNANCHORED, available from the start of play.
            NodeId::Quest(id)
                if !scenario
                    .graph
                    .nodes
                    .contains_key(&NodeId::Quest(id.clone())) =>
            {
                UNANCHORED_VERDICT.to_string()
            }
            // Same fix for a `visited(Y)` atom targeting an undeclared scene
            // key -- every DECLARED scene is unconditionally a graph node.
            NodeId::Scene(key) if !scenario.key_set.contains_key(key) => {
                "Unknown — this scene key is not declared anywhere in this project root \
                 (E-CONN-UNKNOWN-NODE), under your declared routes."
                    .to_string()
            }
            _ => "Unknown — this node is on or downstream of a prerequisite cycle \
                  (E-CONN-CYCLE); its reachability is unavailable under your declared routes."
                .to_string(),
        }
    };
    if reach_evidence(scenario, node) == "unknown" {
        format!("{text} [unknown]")
    } else {
        text
    }
}

/// dsl 0.21.0 §7a.5: the reach verdict of a declared quest without `follows=`.
/// Its leading word is the JSON/DOT `unanchored` token's source
/// ([`scenario_fmt`]'s `reach_token` keys off it, like every other verdict).
///
/// [`scenario_fmt`]: crate::scenario_fmt
const UNANCHORED_VERDICT: &str = "Unanchored — a quest with no declared `follows` edge: \
     available from the start of play; the connectivity layer holds no prerequisites for it, so \
     only its quest lifecycle (`start`, or an accept) decides when it activates.";

/// dsl 0.21.0 §7a.5: the declared quests the prerequisite graph does not
/// hold (no `follows=`, no `::accept` anchor — dsl 0.24.0 §2 — and no
/// subquest tree or `start` anchor — dsl 0.25.0 §4), in id order — the
/// `unanchored` list every `lute scenario` graph view prints beside the
/// layers.
pub(crate) fn unanchored_quests(
    quest_ids: &BTreeSet<String>,
    graph: &lute_check::connectivity::ConnGraph,
) -> Vec<lute_check::connectivity::NodeId> {
    quest_ids
        .iter()
        .map(|id| lute_check::connectivity::NodeId::Quest(id.clone()))
        .filter(|node| !graph.nodes.contains_key(node))
        .collect()
}

/// Print `node`'s declared `after` (a quest's `follows`) STRUCTURE (dsl §5:575) — the raw formula
/// shape, `&&`/`||` intact (Main review: never flattened into a
/// predecessor list that could misrepresent a disjunction as a joint
/// requirement), plus each directly-referenced node's own reachability as
/// supplementary context (explicitly labeled "referenced", never "route" —
/// the formula above IS the route structure).
fn print_prereq_structure(
    out: &mut String,
    scenario: &RootScenario,
    docs: &[lute_check::ProjectDoc<'_>],
    node: &lute_check::connectivity::NodeId,
) {
    use lute_check::connectivity::{NodeId, PrereqState};
    // A quest declares its graph edge with `follows=`; scenes and beats with `after`.
    let key = if matches!(node, NodeId::Quest(_)) {
        "follows"
    } else {
        "after"
    };
    match scenario.graph.nodes.get(node).map(|info| &info.prereq) {
        None if matches!(node, NodeId::Quest(id) if scenario.quest_ids.contains(id)) => {
            outln!(
                out,
                "  {key}: (none declared) — unanchored: this quest is in no prerequisite graph \
                 layer and on no edge; it is available from the start of play."
            );
        }
        _ if matches!(node, NodeId::Beat(_)) => {
            print_bundle_beat_selection(out, scenario, docs, node);
        }
        None | Some(PrereqState::Absent) => {
            outln!(
                out,
                "  {key}: (none declared) — this node is an entry point."
            );
        }
        Some(PrereqState::Invalid) => {
            outln!(
                out,
                "  {key}: (malformed — E-CONN-PROFILE; structure unavailable)"
            );
        }
        Some(prereq @ PrereqState::Valid(f)) => {
            outln!(out, "  {key}: {}", format_prereq(f));
            print_referenced(
                out,
                scenario,
                prereq,
                &format!("`{key}` above for the && / || structure"),
            );
        }
        Some(prereq @ PrereqState::Anchored(anchors)) => {
            outln!(
                out,
                "  {key}: (none declared) — anchored; each anchor \
                 holds before it activates, through any one of its sources:"
            );
            for a in anchors {
                let from: Vec<String> = a.from.iter().map(|n| n.to_string()).collect();
                outln!(out, "    [{}] {}", a.kind.as_str(), from.join(" || "));
            }
            print_referenced(out, scenario, prereq, "the anchors above");
        }
    }
}

/// Each node `prereq` names, with its own reach verdict — context for the
/// structure printed above it (`see`), never a route list.
fn print_referenced(
    out: &mut String,
    scenario: &RootScenario,
    prereq: &lute_check::connectivity::PrereqState,
    see: &str,
) {
    let targets = prereq.referenced(&scenario.graph.nodes);
    if targets.is_empty() {
        return;
    }
    outln!(
        out,
        "  referenced node(s) (see {see} — this is NOT a flat requirement list):"
    );
    for target in &targets {
        outln!(
            out,
            "    - {target}: {}",
            reach_verdict_text(scenario, target)
        );
    }
}

/// A bundle beat's reach report (dsl 0.23.0 §4): its `after=` (dsl 0.25.0
/// §3) as a scene's `after:` is printed — the formula and the nodes it
/// references — else it is an entry node; then what selects it: its
/// occasion, target and `when`, printed as authored so the reader sees why
/// it is on the graph.
fn print_bundle_beat_selection(
    out: &mut String,
    scenario: &RootScenario,
    docs: &[lute_check::ProjectDoc<'_>],
    node: &lute_check::connectivity::NodeId,
) {
    use lute_check::connectivity::PrereqState;
    let Some(info) = scenario.graph.nodes.get(node) else {
        return;
    };
    match &info.prereq {
        prereq @ PrereqState::Valid(f) => {
            outln!(out, "  after: {}", format_prereq(f));
            print_referenced(
                out,
                scenario,
                prereq,
                "`after` above for the && / || structure",
            );
        }
        PrereqState::Invalid => {
            outln!(
                out,
                "  after: (malformed — E-CONN-PROFILE; structure unavailable)"
            );
        }
        _ => outln!(
            out,
            "  after: (none declared) — an entry node: it plays when its occasion is raised and \
             its `when` holds."
        ),
    }
    let lute_check::connectivity::NodeId::Beat(key) = node else {
        return;
    };
    let beat = docs
        .iter()
        .filter(|item| item.path == info.path)
        .flat_map(|item| {
            let doc_id = lute_check::connectivity::bundle_id(item.meta);
            item.doc.beats.iter().map(move |b| (doc_id.clone(), b))
        })
        .find(|(doc_id, b)| {
            doc_id
                .as_deref()
                .is_some_and(|d| lute_check::bundles::bundle_beat_key(d, &b.id) == *key)
        })
        .map(|(_, b)| b);
    outln!(out, "  declared in: {}", info.path.display());
    if let Some(beat) = beat {
        if let Some((on, _)) = &beat.on {
            outln!(out, "  on: {on}");
        }
        if let Some((target, _)) = &beat.target {
            outln!(out, "  target: {target}");
        }
        if let Some(when) = &beat.when {
            outln!(out, "  when: {}", when.raw);
        }
    }
}

pub(crate) fn run_scenario_reach(
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
    let node_id = node_ref_to_id(&node_ref);
    let docs = lute_model::project_docs(
        by_root.get(root).expect("resolved scenario root must have documents"),
    );
    outln!(out, "project root: {}", root.display());
    if let Some(note) = primary_node_ambiguity_note(&scenario, &node_ref) {
        outln!(out, "reach {node_id}: unavailable -- {note}");
        return ExitCode::SUCCESS;
    }
    outln!(out, "reach {node_id}:");
    outln!(
        out,
        "  verdict: {}",
        reach_verdict_text(&scenario, &node_id)
    );
    print_prereq_structure(out, &scenario, &docs, &node_id);
    ExitCode::SUCCESS
}
