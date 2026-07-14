//! Integration tests for the connectivity layer's `after` surface (Task 2):
//! the scene `after:` frontmatter key and the `<quest after="…">` attribute,
//! each validated LOCALLY (grammar only, via `prereq::parse_prereq`) by the
//! single-file `check()` entrypoint. Node existence (whether the referenced
//! `visited`/`completed` id actually exists in the project) is NOT resolved
//! here — that is `check-project`'s job (Task 3+).

use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;

fn permissive_providers() -> ProviderSet {
    ProviderSet::default()
}

fn input_for(text: &str) -> CheckInput {
    CheckInput {
        text: text.to_string(),
        uri: "test".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: permissive_providers(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
    }
}

#[test]
fn scene_after_key_is_parsed_and_validated() {
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nafter: 'visited(\"y.s01ep01\")'\n---\n## Shot 1.\n@a: hi\n";
    let res = check(&input_for(text));
    // A well-formed after against an (unknown, single-file) node must NOT raise E-CONN-PROFILE.
    assert!(!res.diagnostics.iter().any(|d| d.code == "E-CONN-PROFILE"));
}

#[test]
fn scene_after_malformed_raises_profile_error() {
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nafter: '!visited(\"y\")'\n---\n## Shot 1.\n@a: hi\n";
    let res = check(&input_for(text));
    assert!(res.diagnostics.iter().any(|d| d.code == "E-CONN-PROFILE"));
}

#[test]
fn quest_after_attribute_is_parsed_and_validated() {
    let text = "---\nkind: quest\n---\n<quest id=\"q\" start=\"true\" after=\"visited('y.s01ep01')\">\n<objective id=\"o\" done=\"true\"/>\n</quest>\n";
    let res = check(&input_for(text));
    assert!(!res.diagnostics.iter().any(|d| d.code == "E-CONN-PROFILE"));
}

#[test]
fn quest_after_malformed_raises_profile_error() {
    let text = "---\nkind: quest\n---\n<quest id=\"q\" start=\"true\" after=\"!visited('x')\">\n<objective id=\"o\" done=\"true\"/>\n</quest>\n";
    let res = check(&input_for(text));
    assert!(res.diagnostics.iter().any(|d| d.code == "E-CONN-PROFILE"));
}

#[test]
fn quest_frontmatter_after_key_is_not_a_prereq_surface() {
    // §2.1: frontmatter `after:` is a SCENE-ONLY prerequisite surface. A
    // quest pack declares its prerequisite via the per-`<quest>` `after`
    // attribute instead — a malformed `after:` in a quest doc's FRONTMATTER
    // must NOT be fed to `prereq::parse_prereq`; it must still raise the
    // ordinary unknown-meta-key diagnostic because frontmatter `after:` is a
    // scene-only placement key.
    let text = "---\nkind: quest\nafter: '!visited(\"x\")'\n---\n<quest id=\"q\" start=\"true\">\n<objective id=\"o\" done=\"true\"/>\n</quest>\n";
    let res = check(&input_for(text));
    assert!(!res.diagnostics.iter().any(|d| d.code == "E-CONN-PROFILE"));
    assert!(
        res.diagnostics
            .iter()
            .any(|d| d.code == "E-META-UNKNOWN-KEY" && d.message.contains("after")),
        "quest-pack frontmatter `after:` must be flagged as an unknown meta key (scene-only placement); got {:?}",
        res.diagnostics
    );
}

// --- Task 4: `E-CONN-UNKNOWN-NODE` node resolution (both `after` surfaces) ---

use lute_check::connectivity::{quest_id_set, resolve_nodes, scene_key_set};
use std::path::PathBuf;

fn docs_for(texts: &[(&str, &str)]) -> Vec<(PathBuf, lute_syntax::ast::Document)> {
    texts
        .iter()
        .map(|(path, text)| {
            let (doc, _) = lute_syntax::parse(text);
            (PathBuf::from(*path), doc)
        })
        .collect()
}

#[test]
fn unknown_visited_key_is_flagged() {
    // Scene `a` declares `after: visited("nope.s99ep99")`; no such episode
    // key exists anywhere in the project.
    let text = "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\nafter: 'visited(\"nope.s99ep99\")'\n---\n## Shot 1.\n@a: hi\n";
    let docs = docs_for(&[("a.lute", text)]);
    let key_set = scene_key_set(&docs);
    let quest_ids = quest_id_set(&docs);
    let res = resolve_nodes(&docs, &key_set, &quest_ids);
    assert!(
        res.iter().any(|(_, d)| d.code == "E-CONN-UNKNOWN-NODE"),
        "expected E-CONN-UNKNOWN-NODE, got {res:?}"
    );
}

#[test]
fn known_visited_key_resolves_clean() {
    // Scene `a` references scene `b`'s real canonical key `b.s01ep01`; both
    // exist in the same project walk.
    let text_a = "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\nafter: 'visited(\"b.s01ep01\")'\n---\n## Shot 1.\n@a: hi\n";
    let text_b = "---\nkind: scene\ncharacter: b\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@b: hi\n";
    let docs = docs_for(&[("a.lute", text_a), ("b.lute", text_b)]);
    let key_set = scene_key_set(&docs);
    let quest_ids = quest_id_set(&docs);
    let res = resolve_nodes(&docs, &key_set, &quest_ids);
    assert!(
        !res.iter().any(|(_, d)| d.code == "E-CONN-UNKNOWN-NODE"),
        "unexpected E-CONN-UNKNOWN-NODE for a known key, got {res:?}"
    );
}

#[test]
fn known_completed_quest_attribute_resolves_clean() {
    // `q2`'s `after="completed('q1')"` references `q1`, a real declared
    // quest id in the same doc.
    let text = "---\nkind: quest\n---\n<quest id=\"q1\" start=\"true\">\n<objective id=\"o1\" done=\"true\"/>\n</quest>\n<quest id=\"q2\" start=\"true\" after=\"completed('q1')\">\n<objective id=\"o2\" done=\"true\"/>\n</quest>\n";
    let docs = docs_for(&[("quests.lute", text)]);
    let key_set = scene_key_set(&docs);
    let quest_ids = quest_id_set(&docs);
    let res = resolve_nodes(&docs, &key_set, &quest_ids);
    assert!(
        !res.iter().any(|(_, d)| d.code == "E-CONN-UNKNOWN-NODE"),
        "unexpected E-CONN-UNKNOWN-NODE for a known quest id, got {res:?}"
    );
}

#[test]
fn unknown_completed_quest_attribute_is_flagged_and_anchored_on_quest_after() {
    // `q2`'s `after="completed('ghost')"` references a quest id that is
    // declared nowhere in the project — must be flagged, and anchored on
    // `q2`'s OWN `after` attribute span (not any scene's `after:` span; this
    // doc has no scene at all).
    let text = "---\nkind: quest\n---\n<quest id=\"q1\" start=\"true\">\n<objective id=\"o1\" done=\"true\"/>\n</quest>\n<quest id=\"q2\" start=\"true\" after=\"completed('ghost')\">\n<objective id=\"o2\" done=\"true\"/>\n</quest>\n";
    let docs = docs_for(&[("quests.lute", text)]);
    let key_set = scene_key_set(&docs);
    let quest_ids = quest_id_set(&docs);
    let res = resolve_nodes(&docs, &key_set, &quest_ids);
    let hit = res
        .iter()
        .find(|(_, d)| d.code == "E-CONN-UNKNOWN-NODE")
        .unwrap_or_else(|| panic!("expected E-CONN-UNKNOWN-NODE, got {res:?}"));
    let q2 = &docs[0].1.quests[1];
    assert_eq!(q2.id, "q2");
    assert_eq!(
        hit.1.span, q2.after_span,
        "unknown-node diagnostic for a quest-sourced formula must anchor on that quest's after_span"
    );
}

// --- Task 5: topological-precedence DAG + `E-CONN-CYCLE` (typed `NodeId`) ---

use lute_check::connectivity::{assemble_graph, NodeId};

#[test]
fn two_node_cycle_is_flagged() {
    // A after visited(B's key); B after visited(A's key).
    let text_a = "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\nafter: 'visited(\"b.s01ep01\")'\n---\n## Shot 1.\n@a: hi\n";
    let text_b = "---\nkind: scene\ncharacter: b\nseason: 1\nepisode: 1\nafter: 'visited(\"a.s01ep01\")'\n---\n## Shot 1.\n@b: hi\n";
    let docs = docs_for(&[("a.lute", text_a), ("b.lute", text_b)]);
    let key_set = scene_key_set(&docs);
    let quest_ids = quest_id_set(&docs);
    let (_g, diags) = assemble_graph(&docs, &key_set, &quest_ids);
    assert!(
        diags.iter().any(|(_p, d)| d.code == "E-CONN-CYCLE"),
        "expected E-CONN-CYCLE, got {diags:?}"
    );
}

#[test]
fn three_node_cycle_is_flagged() {
    // X after visited(Y); Y after visited(Z); Z after visited(X).
    let text_x = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nafter: 'visited(\"y.s01ep01\")'\n---\n## Shot 1.\n@x: hi\n";
    let text_y = "---\nkind: scene\ncharacter: y\nseason: 1\nepisode: 1\nafter: 'visited(\"z.s01ep01\")'\n---\n## Shot 1.\n@y: hi\n";
    let text_z = "---\nkind: scene\ncharacter: z\nseason: 1\nepisode: 1\nafter: 'visited(\"x.s01ep01\")'\n---\n## Shot 1.\n@z: hi\n";
    let docs = docs_for(&[("x.lute", text_x), ("y.lute", text_y), ("z.lute", text_z)]);
    let key_set = scene_key_set(&docs);
    let quest_ids = quest_id_set(&docs);
    let (_g, diags) = assemble_graph(&docs, &key_set, &quest_ids);
    assert!(
        diags.iter().any(|(_p, d)| d.code == "E-CONN-CYCLE"),
        "expected E-CONN-CYCLE for a 3-node cycle, got {diags:?}"
    );
}

#[test]
fn acyclic_graph_has_no_cycle_and_topo_order() {
    // A is an entry (no `after`); B after A; C after B.
    let text_a = "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@a: hi\n";
    let text_b = "---\nkind: scene\ncharacter: b\nseason: 1\nepisode: 1\nafter: 'visited(\"a.s01ep01\")'\n---\n## Shot 1.\n@b: hi\n";
    let text_c = "---\nkind: scene\ncharacter: c\nseason: 1\nepisode: 1\nafter: 'visited(\"b.s01ep01\")'\n---\n## Shot 1.\n@c: hi\n";
    let docs = docs_for(&[("a.lute", text_a), ("b.lute", text_b), ("c.lute", text_c)]);
    let key_set = scene_key_set(&docs);
    let quest_ids = quest_id_set(&docs);
    let (g, diags) = assemble_graph(&docs, &key_set, &quest_ids);
    assert!(diags.is_empty(), "unexpected diags: {diags:?}");
    assert_eq!(g.topo_order.len(), 3, "topo_order: {:?}", g.topo_order);
    assert_eq!(
        g.topo_order,
        vec![
            NodeId::Scene("a.s01ep01".to_string()),
            NodeId::Scene("b.s01ep01".to_string()),
            NodeId::Scene("c.s01ep01".to_string()),
        ],
        "prerequisite must precede dependent in topo_order"
    );
}

#[test]
fn scene_and_quest_sharing_a_string_are_distinct_nodes() {
    // Scene canonical key "shared.key" (character=shared, episodeId=key) and
    // quest id "shared.key" are the SAME string but SEPARATE namespaces --
    // both must exist as distinct graph nodes, and each incoming reference
    // must resolve to the correctly-typed node.
    let text_shared = "---\nkind: scene\ncharacter: shared\nseason: 1\nepisode: 1\nepisodeId: key\n---\n## Shot 1.\n@a: hi\n";
    let text_referencer = "---\nkind: scene\ncharacter: referencer\nseason: 1\nepisode: 1\nafter: 'visited(\"shared.key\")'\n---\n## Shot 1.\n@a: hi\n";
    let text_quests = "---\nkind: quest\n---\n<quest id=\"shared.key\" start=\"true\" after=\"completed('placeholder')\">\n<objective id=\"o1\" done=\"true\"/>\n</quest>\n<quest id=\"consumer\" start=\"true\" after=\"completed('shared.key')\">\n<objective id=\"o2\" done=\"true\"/>\n</quest>\n";
    let docs = docs_for(&[
        ("shared.lute", text_shared),
        ("referencer.lute", text_referencer),
        ("quests.lute", text_quests),
    ]);
    let key_set = scene_key_set(&docs);
    let quest_ids = quest_id_set(&docs);
    let (g, diags) = assemble_graph(&docs, &key_set, &quest_ids);
    assert!(diags.is_empty(), "unexpected diags: {diags:?}");

    let scene_node = NodeId::Scene("shared.key".to_string());
    let quest_node = NodeId::Quest("shared.key".to_string());
    assert!(g.nodes.contains_key(&scene_node), "scene node missing: {:?}", g.nodes.keys().collect::<Vec<_>>());
    assert!(g.nodes.contains_key(&quest_node), "quest node missing: {:?}", g.nodes.keys().collect::<Vec<_>>());

    let referencer_node = NodeId::Scene("referencer.s01ep01".to_string());
    let consumer_node = NodeId::Quest("consumer".to_string());

    let scene_edges = g.edges.get(&scene_node).cloned().unwrap_or_default();
    assert!(
        scene_edges.contains(&referencer_node),
        "visited(\"shared.key\") must edge to the SCENE node: {scene_edges:?}"
    );
    assert!(
        !scene_edges.contains(&consumer_node),
        "the scene node must never edge to a quest-namespace node: {scene_edges:?}"
    );

    let quest_edges = g.edges.get(&quest_node).cloned().unwrap_or_default();
    assert!(
        quest_edges.contains(&consumer_node),
        "completed(\"shared.key\") must edge to the QUEST node: {quest_edges:?}"
    );
    assert!(
        !quest_edges.contains(&referencer_node),
        "the quest node must never edge to a scene-namespace node: {quest_edges:?}"
    );
}

#[test]
fn completed_on_a_plain_after_less_quest_is_a_leaf_not_an_edge() {
    // `plain` declares no `after` -- it is NEVER a graph node. `dependent`'s
    // `completed('plain')` must therefore contribute NO edge at all (a leaf
    // dependency, resolved by Task 6's quest-lifecycle signal, not the DAG).
    let text = "---\nkind: quest\n---\n<quest id=\"plain\" start=\"true\">\n<objective id=\"o1\" done=\"true\"/>\n</quest>\n<quest id=\"dependent\" start=\"true\" after=\"completed('plain')\">\n<objective id=\"o2\" done=\"true\"/>\n</quest>\n";
    let docs = docs_for(&[("quests.lute", text)]);
    let key_set = scene_key_set(&docs);
    let quest_ids = quest_id_set(&docs);
    let (g, diags) = assemble_graph(&docs, &key_set, &quest_ids);
    assert!(diags.is_empty(), "unexpected diags: {diags:?}");

    let plain_node = NodeId::Quest("plain".to_string());
    let dependent_node = NodeId::Quest("dependent".to_string());
    assert!(
        !g.nodes.contains_key(&plain_node),
        "a no-`after` quest must never be a graph node: {:?}",
        g.nodes.keys().collect::<Vec<_>>()
    );
    assert!(g.nodes.contains_key(&dependent_node), "the after-declaring quest must be a node");
    assert!(
        !g.edges.contains_key(&plain_node),
        "no edge may originate from a non-node target: {:?}",
        g.edges
    );
    assert!(
        g.edges.values().all(|targets| !targets.contains(&dependent_node)),
        "dependent must have no incoming edge -- its only atom target isn't a graph node"
    );
    assert_eq!(g.topo_order, vec![dependent_node], "only the node-worthy quest appears in topo_order");
}
