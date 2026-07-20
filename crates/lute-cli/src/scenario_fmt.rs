//! `lute scenario --format json|dot` — machine-readable renderings of the
//! SAME per-root scenario analysis the text view reports. Never a second
//! analysis: every graph/reach/envelope datum comes from
//! `crate::collect_project_docs` + `crate::assemble_root_scenario` (the exact
//! per-root collection + connectivity/envelope passes `check-project` and the
//! text `lute scenario` view share), reusing `crate::reach_verdict_text`,
//! `crate::format_prereq`, `crate::topo_layers`, `crate::node_cycle_degraded`,
//! and the `reach`/`envelope` node-resolution seam verbatim — only the
//! presentation differs.
//!
//! Determinism: output is byte-identical across runs. `serde_json`'s default
//! `Map` is key-sorted and every array is built from a `BTreeMap`/`BTreeSet`
//! iteration or an explicitly sorted `topo_layers` order.

use std::path::Path;
use std::process::ExitCode;

use serde_json::{Map, Value};

use lute_check::connectivity::{NodeId, PrereqState};
use lute_check::envelope;

use crate::{
    assemble_root_scenario, collect_project_docs, format_prereq, node_cycle_degraded,
    node_ref_to_id, primary_node_ambiguity_note, reach_verdict_text, resolve_node_ref, topo_layers,
    NodeRef, RootScenario, ScenarioCommand,
};

/// Render the scenario report in a non-text format. See
/// [`crate::Command::Scenario`]. `json` and `dot` are the only valid
/// formats; anything else is a usage error (exit `2`) naming the valid set.
pub fn run(
    dir: &Path,
    providers: Option<&Path>,
    command: Option<ScenarioCommand>,
    format: &str,
) -> ExitCode {
    match format {
        "json" => run_json(dir, providers, command),
        "dot" => run_dot(dir, providers, command),
        other => {
            eprintln!(
                "lute scenario: unknown --format `{other}` (valid formats: text, json, dot)"
            );
            ExitCode::from(2)
        }
    }
}

// ===========================================================================
// Shared node → token mappings (presentation of the SAME analysis fields).
// ===========================================================================

/// The node kind token — matches the `NodeId` variant, never re-derived.
fn node_kind_str(node: &NodeId) -> &'static str {
    match node {
        NodeId::Scene(_) => "scene",
        NodeId::Quest(_) => "quest",
    }
}

/// The stable lowercase reach token, derived from the SAME verdict logic the
/// text view prints ([`crate::reach_verdict_text`]) plus the SAME per-node
/// cycle-degradation test ([`crate::node_cycle_degraded`]) — so a node's JSON
/// token and its text verdict can never disagree. A cycle-degraded node reads
/// `cycle-degraded` (its reach is genuinely unavailable, `E-CONN-CYCLE`),
/// never a fabricated reachable/unreachable claim.
fn reach_token(scenario: &RootScenario, node: &NodeId) -> &'static str {
    if node_cycle_degraded(scenario, node) {
        return "cycle-degraded";
    }
    let verdict = reach_verdict_text(scenario, node);
    if verdict.starts_with("Reachable") {
        "reachable"
    } else if verdict.starts_with("Unreachable") {
        "unreachable"
    } else {
        "unknown"
    }
}

/// A node's declared `after` prerequisite as JSON: `null` when absent (an
/// entry node), the malformed marker string when present-but-unparseable
/// (`E-CONN-PROFILE`, already reported by `check`), else the fully
/// parenthesized formula text ([`crate::format_prereq`] — `&&`/`||` intact,
/// never flattened into a misleading joint requirement).
fn prereq_json(scenario: &RootScenario, node: &NodeId) -> Value {
    match scenario.graph.nodes.get(node).map(|info| &info.prereq) {
        None | Some(PrereqState::Absent) => Value::Null,
        Some(PrereqState::Invalid) => Value::String("(malformed — E-CONN-PROFILE)".to_string()),
        Some(PrereqState::Valid(f)) => Value::String(format_prereq(f)),
    }
}

/// A sorted JSON array of a path set (`Guaranteed`/`Possible`/difference) —
/// the envelope tables are pure path sets (no per-path type is computed
/// anywhere in the analysis), so this emits exactly what the text view's
/// `print_path_set` lists, honestly, never a fabricated `path: type` map.
fn path_set_json(set: &std::collections::BTreeSet<String>) -> Value {
    Value::Array(set.iter().map(|p| Value::String(p.clone())).collect())
}

// ===========================================================================
// JSON
// ===========================================================================

fn run_json(dir: &Path, providers: Option<&Path>, command: Option<ScenarioCommand>) -> ExitCode {
    let (file_results, by_root) = match collect_project_docs(dir, providers, false) {
        Ok(v) => v,
        Err(code) => return code,
    };
    match command {
        None => {
            let roots: Vec<Value> = by_root
                .iter()
                .map(|(root, group_full)| {
                    let scenario = assemble_root_scenario(group_full, &file_results);
                    root_graph_json(root, &scenario)
                })
                .collect();
            let mut top = Map::new();
            top.insert("roots".to_string(), Value::Array(roots));
            print_json(&Value::Object(top))
        }
        Some(ScenarioCommand::Reach { node_id }) => {
            reach_json(dir, &by_root, &file_results, &node_id)
        }
        Some(ScenarioCommand::Envelope { node_id }) => {
            envelope_json(dir, &by_root, &file_results, &node_id)
        }
    }
}

/// One root's bare graph view as JSON: `root`, its `nodes` (id in `NodeId`
/// display form so edges resolve unambiguously, `kind`, `reach` token, and
/// declared `prereq` formula), the flattened `edges` (prerequisite ->
/// dependent, mirroring `print_graph_for_root`'s SAME `graph.edges` walk),
/// and the deterministic topological `layers` ([`crate::topo_layers`]).
fn root_graph_json(root: &Path, scenario: &RootScenario) -> Value {
    let nodes: Vec<Value> = scenario
        .graph
        .nodes
        .keys()
        .map(|node| {
            let mut obj = Map::new();
            obj.insert("id".to_string(), Value::String(node.to_string()));
            obj.insert("kind".to_string(), Value::String(node_kind_str(node).to_string()));
            obj.insert("reach".to_string(), Value::String(reach_token(scenario, node).to_string()));
            obj.insert("prereq".to_string(), prereq_json(scenario, node));
            Value::Object(obj)
        })
        .collect();

    let mut edges: Vec<Value> = Vec::new();
    for (from, targets) in &scenario.graph.edges {
        for to in targets {
            let mut obj = Map::new();
            obj.insert("from".to_string(), Value::String(from.to_string()));
            obj.insert("to".to_string(), Value::String(to.to_string()));
            edges.push(Value::Object(obj));
        }
    }

    let layers: Vec<Value> = topo_layers(&scenario.graph)
        .into_iter()
        .map(|layer| {
            Value::Array(layer.into_iter().map(|n| Value::String(n.to_string())).collect())
        })
        .collect();

    let mut obj = Map::new();
    obj.insert("root".to_string(), Value::String(root.display().to_string()));
    obj.insert("nodes".to_string(), Value::Array(nodes));
    obj.insert("edges".to_string(), Value::Array(edges));
    obj.insert("layers".to_string(), Value::Array(layers));
    Value::Object(obj)
}

/// `scenario reach <node> --format json`: the single-node reachability view
/// (verdict text + token + declared `after` structure + each directly
/// referenced node's own verdict) — the JSON mirror of `run_scenario_reach`.
fn reach_json(
    dir: &Path,
    by_root: &crate::ByRoot,
    file_results: &[(std::path::PathBuf, lute_check::CheckResult)],
    node_id_raw: &str,
) -> ExitCode {
    let (node_ref, root, scenario) = match resolve_node_ref(dir, by_root, file_results, node_id_raw)
    {
        Ok(v) => v,
        Err(code) => return code,
    };
    let node_id = node_ref_to_id(&node_ref);
    let mut obj = Map::new();
    obj.insert("root".to_string(), Value::String(root.display().to_string()));
    obj.insert("node".to_string(), Value::String(node_id.to_string()));
    obj.insert("kind".to_string(), Value::String(node_kind_str(&node_id).to_string()));

    if let Some(note) = primary_node_ambiguity_note(&scenario, &node_ref) {
        obj.insert("unavailable".to_string(), Value::String(note));
        return print_json(&Value::Object(obj));
    }

    obj.insert("reach".to_string(), Value::String(reach_token(&scenario, &node_id).to_string()));
    obj.insert("verdict".to_string(), Value::String(reach_verdict_text(&scenario, &node_id)));
    obj.insert("prereq".to_string(), prereq_json(&scenario, &node_id));

    // Directly referenced nodes (same set `print_prereq_structure` lists) —
    // each with its own verdict, so a disjunction's alternatives are visible
    // without pretending the `after` formula is a flat requirement list (the
    // `prereq` string above carries the real && / || structure).
    if let Some(PrereqState::Valid(f)) = scenario.graph.nodes.get(&node_id).map(|i| &i.prereq) {
        let mut targets: std::collections::BTreeSet<NodeId> = std::collections::BTreeSet::new();
        for atom in lute_check::atoms(f) {
            targets.insert(match atom {
                lute_check::Atom::Visited(key) => NodeId::Scene(key),
                lute_check::Atom::Completed(id) => NodeId::Quest(id),
            });
        }
        let referenced: Vec<Value> = targets
            .iter()
            .map(|t| {
                let mut r = Map::new();
                r.insert("node".to_string(), Value::String(t.to_string()));
                r.insert("kind".to_string(), Value::String(node_kind_str(t).to_string()));
                r.insert("reach".to_string(), Value::String(reach_token(&scenario, t).to_string()));
                r.insert("verdict".to_string(), Value::String(reach_verdict_text(&scenario, t)));
                Value::Object(r)
            })
            .collect();
        obj.insert("referenced".to_string(), Value::Array(referenced));
    }

    print_json(&Value::Object(obj))
}

/// `scenario envelope <node> --format json`: the single-node envelope view —
/// the SAME `Guaranteed`/`Possible` tables + `Possible \ Guaranteed`
/// difference the text `print_scene_envelope`/`print_quest_envelope` report,
/// reusing `envelope::quest_envelope` verbatim for quests and the SAME
/// `envs`/`D` fallback for scenes.
fn envelope_json(
    dir: &Path,
    by_root: &crate::ByRoot,
    file_results: &[(std::path::PathBuf, lute_check::CheckResult)],
    node_id_raw: &str,
) -> ExitCode {
    let (node_ref, root, scenario) = match resolve_node_ref(dir, by_root, file_results, node_id_raw)
    {
        Ok(v) => v,
        Err(code) => return code,
    };
    let node_id = node_ref_to_id(&node_ref);
    let mut obj = Map::new();
    obj.insert("root".to_string(), Value::String(root.display().to_string()));
    obj.insert("node".to_string(), Value::String(node_id.to_string()));
    obj.insert("kind".to_string(), Value::String(node_kind_str(&node_id).to_string()));

    if let Some(note) = primary_node_ambiguity_note(&scenario, &node_ref) {
        obj.insert("unavailable".to_string(), Value::String(note));
        return print_json(&Value::Object(obj));
    }

    obj.insert("reach".to_string(), Value::String(reach_token(&scenario, &node_id).to_string()));
    obj.insert("prereq".to_string(), prereq_json(&scenario, &node_id));
    obj.insert(
        "cycleDegraded".to_string(),
        Value::Bool(node_cycle_degraded(&scenario, &node_id)),
    );

    let (env, enrichment_note) = match &node_ref {
        NodeRef::Scene(_) => {
            obj.insert("tainted".to_string(), Value::Bool(scenario.tainted.contains(&node_id)));
            let env = scenario.envs.get(&node_id).cloned().unwrap_or_else(|| envelope::Env {
                guaranteed: scenario.envelope_d.clone(),
                possible: scenario.envelope_d.clone(),
            });
            (env, None)
        }
        NodeRef::Quest(id) => {
            let Some(quest) =
                scenario.docs.iter().flat_map(|(_, d)| d.quests.iter()).find(|q| &q.id == id)
            else {
                eprintln!("lute: internal error: quest `{id}` resolved but no declaration found");
                return ExitCode::from(2);
            };
            let qe = envelope::quest_envelope(
                quest,
                &scenario.graph,
                &scenario.envs,
                &scenario.envelope_d,
            );
            (qe.env, Some(qe.enrichment_note))
        }
    };

    let diff: std::collections::BTreeSet<String> =
        env.possible.difference(&env.guaranteed).cloned().collect();
    let mut envelope_obj = Map::new();
    envelope_obj.insert("guaranteed".to_string(), path_set_json(&env.guaranteed));
    envelope_obj.insert("possible".to_string(), path_set_json(&env.possible));
    envelope_obj.insert("possibleNotGuaranteed".to_string(), path_set_json(&diff));
    obj.insert("envelope".to_string(), Value::Object(envelope_obj));
    if let Some(note) = enrichment_note {
        obj.insert("enrichmentNote".to_string(), Value::Bool(note));
    }

    print_json(&Value::Object(obj))
}

/// Serialize `value` to stdout as pretty JSON + a trailing newline. Exit `0`
/// on success, `2` on a serialization/write failure (matching the CLI's
/// I/O-error tier).
fn print_json(value: &Value) -> ExitCode {
    match serde_json::to_string_pretty(value) {
        Ok(s) => {
            println!("{s}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("lute scenario: failed to serialize JSON: {e}");
            ExitCode::from(2)
        }
    }
}

// ===========================================================================
// DOT (Graphviz)
// ===========================================================================

fn run_dot(dir: &Path, providers: Option<&Path>, command: Option<ScenarioCommand>) -> ExitCode {
    // `dot` renders the graph structure only; a single-node reach/envelope
    // report has no graph to draw, so it is a usage error (exit 2) rather
    // than a misleading empty digraph.
    if command.is_some() {
        eprintln!(
            "lute scenario: --format dot applies to the graph view only; \
             `reach`/`envelope` have no graph to render (use --format json or text)"
        );
        return ExitCode::from(2);
    }
    let (file_results, by_root) = match collect_project_docs(dir, providers, false) {
        Ok(v) => v,
        Err(code) => return code,
    };
    if by_root.is_empty() {
        println!("// lute scenario: no .lute files found");
        return ExitCode::SUCCESS;
    }
    let mut out = String::new();
    for (root, group_full) in &by_root {
        let scenario = assemble_root_scenario(group_full, &file_results);
        out.push_str(&root_dot(root, &scenario));
    }
    print!("{out}");
    ExitCode::SUCCESS
}

/// One `digraph` for one root: a node line per `graph.nodes` (shape by kind —
/// box scene / ellipse quest; color by reach verdict — green reachable / red
/// unreachable / gray unknown / orange cycle-degraded; label = id) and an
/// edge line per `graph.edges` entry (the SAME prerequisite -> dependent walk
/// the JSON/text views use). Every id is JSON-escaped+quoted so an id
/// containing a `"`/`\`/control char stays valid Graphviz.
fn root_dot(root: &Path, scenario: &RootScenario) -> String {
    let mut s = String::new();
    s.push_str(&format!("digraph {} {{\n", dot_quote(&root.display().to_string())));
    for node in scenario.graph.nodes.keys() {
        let shape = match node {
            NodeId::Scene(_) => "box",
            NodeId::Quest(_) => "ellipse",
        };
        let color = match reach_token(scenario, node) {
            "reachable" => "green",
            "unreachable" => "red",
            "cycle-degraded" => "orange",
            _ => "gray",
        };
        let label = node.to_string();
        s.push_str(&format!(
            "  {} [shape={}, color={}, label={}];\n",
            dot_quote(&label),
            shape,
            color,
            dot_quote(&label),
        ));
    }
    for (from, targets) in &scenario.graph.edges {
        for to in targets {
            s.push_str(&format!(
                "  {} -> {};\n",
                dot_quote(&from.to_string()),
                dot_quote(&to.to_string()),
            ));
        }
    }
    s.push_str("}\n");
    s
}

/// Quote a string as a Graphviz double-quoted ID. JSON string-literal
/// escaping (`serde_json::to_string`) is a safe superset of DOT's
/// double-quoted-ID escaping (both escape `"` and `\`), so it never emits an
/// unescaped delimiter — mirroring `crate::quote_cel_string`'s reasoning.
fn dot_quote(s: &str) -> String {
    serde_json::to_string(s).expect("String -> JSON serialization is infallible")
}
