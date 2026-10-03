use lute_core_span::Evidence;
use lute_model::graph::{NodeKey, NodeKind, SemanticGraph};
use lute_model::impact::{query_graph, ImpactTarget};

fn graph_with(edges: &[(NodeKey, NodeKey, &str, Evidence)]) -> SemanticGraph {
    let mut graph = SemanticGraph::default();
    for (source, target, kind, evidence) in edges {
        graph.edge(
            source.clone(),
            target.clone(),
            *kind,
            format!("{kind} dependency"),
            None,
            None,
            evidence.clone(),
        );
    }
    graph.outgoing.clear();
    for (index, edge) in graph.edges.iter().enumerate() {
        graph.outgoing.entry(edge.source.clone()).or_default().push(index);
    }
    graph
}

fn graph_with_reasons(
    edges: &[(NodeKey, NodeKey, &str, &str, Evidence)],
) -> SemanticGraph {
    let mut graph = SemanticGraph::default();
    for (source, target, kind, reason, evidence) in edges {
        graph.edge(
            source.clone(),
            target.clone(),
            *kind,
            *reason,
            None,
            None,
            evidence.clone(),
        );
    }
    graph.outgoing.clear();
    for (index, edge) in graph.edges.iter().enumerate() {
        graph.outgoing.entry(edge.source.clone()).or_default().push(index);
    }
    graph
}

fn state(name: &str) -> NodeKey {
    NodeKey::new(NodeKind::State, name)
}
fn fact(name: &str) -> NodeKey {
    NodeKey::new(NodeKind::Fact, name)
}
fn line(name: &str) -> NodeKey {
    NodeKey::new(NodeKind::Line, name)
}

fn item<'a>(report: &'a lute_model::ImpactReport, key: &str) -> &'a lute_model::ImpactItem {
    report
        .items
        .values()
        .flatten()
        .find(|candidate| candidate.key == key)
        .unwrap_or_else(|| panic!("missing {key}: {report:?}"))
}

#[test]
fn reverse_closure_terminates_on_cycles_and_excludes_a_self_read() {
    let a = state("a");
    let b = state("b");
    let graph = graph_with(&[
        (a.clone(), a.clone(), "reads", Evidence::Proven),
        (a.clone(), b.clone(), "reads", Evidence::Proven),
        (b, a.clone(), "reads", Evidence::Proven),
    ]);
    let report = query_graph(
        &graph,
        "fixture".into(),
        &ImpactTarget::parse("state:a").unwrap(),
    );
    assert_eq!(item(&report, "state:b").evidence, Evidence::Proven);
    assert!(report.items.values().flatten().all(|candidate| candidate.key != "state:a"));
}

#[test]
fn fact_overlap_accepts_wildcards_and_unbound_parameters_but_not_different_grounds() {
    let wildcard = fact("slew(_)");
    let parameter = fact("slew(@foe)");
    let different = fact("slew(eel)");
    let wildcard_line = line("wildcard");
    let parameter_line = line("parameter");
    let different_line = line("different");
    let graph = graph_with(&[
        (wildcard, wildcard_line.clone(), "queries", Evidence::Proven),
        (parameter, parameter_line.clone(), "queries", Evidence::Heuristic),
        (different, different_line.clone(), "queries", Evidence::Proven),
    ]);
    let report = query_graph(&graph, "fixture".into(), &ImpactTarget::parse("fact:slew(regent)").unwrap());
    assert_eq!(item(&report, "line:wildcard").evidence, Evidence::Proven);
    assert_eq!(item(&report, "line:parameter").evidence, Evidence::Heuristic);
    assert!(report.items.values().flatten().all(|candidate| candidate.key != "line:different"));
}

#[test]
fn negated_count_count_distinct_and_valid_at_queries_remain_dependencies() {
    let target = fact("knows(kai)");
    let negated = line("negated");
    let counted = line("count");
    let distinct = line("distinct");
    let valid_at = line("valid-at");
    let graph = graph_with_reasons(&[
        (target.clone(), negated.clone(), "queries", "!holds('knows', ['kai'])", Evidence::Proven),
        (target.clone(), counted.clone(), "queries", "count('knows', ['kai'])", Evidence::Proven),
        (target.clone(), distinct.clone(), "queries", "countDistinct('knows', ['kai'])", Evidence::Proven),
        (target, valid_at.clone(), "queries", "validAt('knows', ['kai'], clock.index)", Evidence::Proven),
    ]);
    let report = query_graph(&graph, "fixture".into(), &ImpactTarget::parse("fact:knows(kai)").unwrap());
    for (key, reason) in [
        ("line:negated", "!holds('knows', ['kai'])"),
        ("line:count", "count('knows', ['kai'])"),
        ("line:distinct", "countDistinct('knows', ['kai'])"),
        ("line:valid-at", "validAt('knows', ['kai'], clock.index)"),
    ] {
        assert_eq!(item(&report, key).reasons[0].reason, reason);
    }
    assert_eq!(item(&report, "line:negated").evidence, Evidence::Proven);
}

#[test]
fn after_visited_and_lifecycle_gates_are_proven_edges() {
    let visited = NodeKey::new(NodeKind::Scene, "hub.regentFell");
    let after_scene = NodeKey::new(NodeKind::Scene, "hub.after");
    let beat = NodeKey::new(NodeKind::Beat, "lore.regentTold");
    let after_beat = NodeKey::new(NodeKind::Beat, "lore.after");
    let objective_done = state("quest.q.objectives.o.done");
    let quest_state = state("quest.q.state");
    let reader = line("quest-reader");
    let graph = graph_with(&[
        (visited.clone(), beat.clone(), "gates", Evidence::Proven),
        (after_scene, after_beat.clone(), "gates", Evidence::Proven),
        (objective_done, quest_state.clone(), "writes", Evidence::Proven),
        (quest_state, reader.clone(), "reads", Evidence::Proven),
    ]);
    let visited_report = query_graph(&graph, "fixture".into(), &ImpactTarget::parse("scene:hub.regentFell").unwrap());
    assert_eq!(item(&visited_report, "beat:lore.regentTold").evidence, Evidence::Proven);
    let after_report = query_graph(&graph, "fixture".into(), &ImpactTarget::parse("scene:hub.after").unwrap());
    assert_eq!(item(&after_report, "beat:lore.after").evidence, Evidence::Proven);
    let lifecycle_report = query_graph(&graph, "fixture".into(), &ImpactTarget::parse("state:quest.q.objectives.o.done").unwrap());
    assert_eq!(item(&lifecycle_report, "line:quest-reader").evidence, Evidence::Proven);
}

#[test]
fn entry_read_and_ever_read_are_disclosure_dependencies() {
    let entry = NodeKey::new(NodeKind::Entry, "codexThrone");
    let read_line = line("read-line");
    let ever_read_line = line("ever-read-line");
    let graph = graph_with(&[
        (entry.clone(), read_line.clone(), "discloses", Evidence::Proven),
        (entry, ever_read_line.clone(), "discloses", Evidence::Proven),
    ]);
    let report = query_graph(&graph, "fixture".into(), &ImpactTarget::parse("entry:codexThrone").unwrap());
    assert_eq!(item(&report, "line:read-line").evidence, Evidence::Proven);
    assert_eq!(item(&report, "line:ever-read-line").evidence, Evidence::Proven);
}

#[test]
fn definitions_and_components_expand_to_their_consumers() {
    let def = NodeKey::new(NodeKind::Def, "readyForCrown");
    let expanded_def = NodeKey::new(NodeKind::Expanded, "readyForCrown@use");
    let component = NodeKey::new(NodeKind::Component, "greeting");
    let expanded_component = NodeKey::new(NodeKind::Expanded, "greeting@use");
    let def_line = line("def-line");
    let component_line = line("component-line");
    let graph = graph_with(&[
        (def.clone(), expanded_def.clone(), "expands", Evidence::Proven),
        (expanded_def, def_line.clone(), "reads", Evidence::Proven),
        (component.clone(), expanded_component.clone(), "expands", Evidence::Proven),
        (expanded_component, component_line.clone(), "reads", Evidence::Proven),
    ]);
    let def_report = query_graph(&graph, "fixture".into(), &ImpactTarget::parse("def:readyForCrown").unwrap());
    assert_eq!(item(&def_report, "line:def-line").evidence, Evidence::Proven);
    let component_report = query_graph(&graph, "fixture".into(), &ImpactTarget::parse("component:greeting").unwrap());
    assert_eq!(item(&component_report, "line:component-line").evidence, Evidence::Proven);
}

#[test]
fn engine_owned_paths_are_listed_as_unknown() {
    let engine_path = state("quest.q.state");
    let reader = line("quest-state-reader");
    let graph = graph_with(&[(engine_path, reader.clone(), "reads", Evidence::Unknown)]);
    let report = query_graph(&graph, "fixture".into(), &ImpactTarget::parse("state:quest.q.state").unwrap());
    assert_eq!(item(&report, "line:quest-state-reader").evidence, Evidence::Unknown);
}

#[test]
fn unbound_rule_variable_is_heuristic() {
    let target = fact("told(regent)");
    let derived = fact("heard(@N,regent)");
    let reader = line("derived-reader");
    let graph = graph_with(&[
        (target, derived.clone(), "derives", Evidence::Heuristic),
        (derived, reader.clone(), "queries", Evidence::Heuristic),
    ]);
    let report = query_graph(&graph, "fixture".into(), &ImpactTarget::parse("fact:told(regent)").unwrap());
    assert_eq!(item(&report, "line:derived-reader").evidence, Evidence::Heuristic);
}


#[test]
fn def_raised_when_reaches_occasion_answering_scene() {
    let target = fact("felled(regent)");
    let def = NodeKey::new(NodeKind::Def, "readyForCrown");
    let occasion = NodeKey::new(NodeKind::Occasion, "lastDive");
    let scene = NodeKey::new(NodeKind::Scene, "last.rail");
    let scene_line = line("last.rail.brann");
    let graph = graph_with_reasons(&[
        (target, def, "queries", "holds('felled', ['regent'])", Evidence::Proven),
        (NodeKey::new(NodeKind::Def, "readyForCrown"), occasion.clone(), "gates", "@readyForCrown", Evidence::Proven),
        (occasion, scene, "raises", "occasion lastDive raises answering scene", Evidence::Proven),
        (NodeKey::new(NodeKind::Scene, "last.rail"), scene_line.clone(), "contains", "containing node contains line", Evidence::Proven),
    ]);
    let report = query_graph(&graph, "fixture".into(), &ImpactTarget::parse("fact:felled(regent)").unwrap());
    let item = item(&report, "line:last.rail.brann");
    assert_eq!(item.evidence, Evidence::Proven);
    assert_eq!(item.reasons.iter().map(|reason| reason.edge.as_str()).collect::<Vec<_>>(), ["queries", "gates", "raises", "contains"]);
}

#[test]
fn objective_reward_is_reached_through_objective_containment() {
    let target = fact("done(q)");
    let objective = NodeKey::new(NodeKind::Objective, "q.o");
    let reward = NodeKey::new(NodeKind::Reward, "q.o#0");
    let graph = graph_with(&[
        (target, objective.clone(), "queries", Evidence::Proven),
        (objective, reward.clone(), "contains", Evidence::Proven),
    ]);
    let report = query_graph(&graph, "fixture".into(), &ImpactTarget::parse("fact:done(q)").unwrap());
    assert_eq!(item(&report, "reward:q.o#0").evidence, Evidence::Proven);
}

#[test]
fn objective_reward_when_attaches_to_reward_not_quest() {
    let target = state("run.active");
    let reward = NodeKey::new(NodeKind::Reward, "q.o#0");
    let quest = NodeKey::new(NodeKind::Quest, "q");
    let graph = graph_with(&[
        (target, reward.clone(), "reads", Evidence::Proven),
        (quest.clone(), reward, "contains", Evidence::Proven),
    ]);
    let report = query_graph(&graph, "fixture".into(), &ImpactTarget::parse("state:run.active").unwrap());
    assert_eq!(item(&report, "reward:q.o#0").evidence, Evidence::Proven);
    assert!(report.items.values().flatten().all(|candidate| candidate.key != "quest:q"));
}

#[test]
fn directive_effects_propagate_bound_fact_and_state_writes() {
    let target = fact("gifted(brann,lantern)");
    let owner = line("directive-call");
    let state_path = state("user.bond.brann");
    let reader = line("directive-consumer");
    let graph = graph_with(&[
        (owner.clone(), target.clone(), "asserts", Evidence::Proven),
        (owner, state_path, "writes", Evidence::Proven),
        (target, reader.clone(), "queries", Evidence::Proven),
    ]);
    let report = query_graph(&graph, "fixture".into(), &ImpactTarget::parse("fact:gifted(brann,lantern)").unwrap());
    assert_eq!(item(&report, "line:directive-consumer").evidence, Evidence::Proven);
}

#[test]
fn rule_guard_fact_string_is_not_a_state_path() {
    let span = lute_core_span::Span { byte_start: 0, byte_end: 32, line: 1, column: 1, utf16_range: (0, 32) };
    let feed = lute_check::deps::slot_dependencies("holds('seen', ['run.enemy'])", span);
    assert!(feed.reads.is_empty());
    assert_eq!(feed.queries.len(), 1);
    assert_eq!(feed.queries[0].pattern.relation, "seen");
}
