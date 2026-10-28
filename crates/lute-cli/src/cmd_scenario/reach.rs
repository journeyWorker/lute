//! `lute scenario reach --node <node>`: a node's declared prerequisite structure
//! and reach verdict.

use std::collections::BTreeSet;
use std::path::Path;
use std::process::ExitCode;

use crate::cmd_scenario::{
    node_ref_to_id, primary_node_ambiguity_note, resolve_node_ref, RootScenario,
};
use crate::project::{ByRoot, RootModels};

/// Render a [`lute_manifest::semantics::prereq::PrereqFormula`] back to CEL-like text, fully
/// parenthesized so the `&&`/`||` nesting is always visible — a
/// `visited(A) || visited(B)` node is reachable via A OR B, never rendered
/// as a flat list that could blur that into "requires A and B" (Main
/// review: routes must never be flattened away).
pub(crate) fn format_prereq(f: &lute_manifest::semantics::prereq::PrereqFormula) -> String {
    match f {
        lute_manifest::semantics::prereq::PrereqFormula::Visited(key) => {
            format!("visited({})", quote_cel_string(key))
        }
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

/// The evidence token of `node`'s reachability claim
/// ([`lute_model::reach_claim`]): `proven` or `unknown`.
pub(crate) fn reach_evidence(
    scenario: &RootScenario,
    node: &lute_check::connectivity::NodeId,
) -> &'static str {
    match lute_model::reach_claim(scenario, node).evidence {
        lute_core_span::Evidence::Unknown => "unknown",
        _ => "proven",
    }
}

/// `node`'s reachability claim as text ([`lute_model::reach_claim`]), with
/// `[unknown]` appended when its evidence is unknown.
pub(crate) fn reach_verdict_text(
    scenario: &RootScenario,
    node: &lute_check::connectivity::NodeId,
) -> String {
    let claim = lute_model::reach_claim(scenario, node);
    if claim.evidence == lute_core_span::Evidence::Unknown {
        format!("{} [unknown]", claim.reason)
    } else {
        claim.reason
    }
}

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
                let from: Vec<String> = a
                    .from
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect();
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
    models: &RootModels,
    node_id_raw: &str,
) -> ExitCode {
    let (node_ref, root, scenario) = match resolve_node_ref(dir, models, node_id_raw) {
        Ok(v) => v,
        Err(code) => return code,
    };
    let node_id = node_ref_to_id(&node_ref);
    let docs = lute_model::project_docs(
        by_root
            .get(root)
            .expect("resolved scenario root must have documents"),
    );
    outln!(out, "project root: {}", root.display());
    if let Some(note) = primary_node_ambiguity_note(scenario, &node_ref) {
        outln!(out, "reach {node_id}: unavailable -- {note}");
        return ExitCode::SUCCESS;
    }
    outln!(out, "reach {node_id}:");
    outln!(out, "  verdict: {}", reach_verdict_text(scenario, &node_id));
    print_prereq_structure(out, scenario, &docs, &node_id);
    ExitCode::SUCCESS
}
