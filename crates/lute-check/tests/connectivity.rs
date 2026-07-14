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
fn scene_after_exact_empty_raises_no_profile_error() {
    // Spec §4.1: an EXACT empty `after` is a graph entry point, same as an
    // absent key -- `check()` must never feed it to `parse_prereq` (which
    // would previously panic on blank CEL text) and must never raise
    // E-CONN-PROFILE for it.
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nafter: ''\n---\n## Shot 1.\n@a: hi\n";
    let res = check(&input_for(text));
    assert!(
        !res.diagnostics.iter().any(|d| d.code == "E-CONN-PROFILE"),
        "exact-empty `after` must be accepted as an entry node: {:?}",
        res.diagnostics
    );
}

#[test]
fn scene_after_whitespace_only_raises_profile_error_without_panic() {
    // A whitespace-only `after` is PRESENT and non-empty -- it reaches
    // `parse_prereq`, which must reject it (not panic) as E-CONN-PROFILE.
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nafter: '   '\n---\n## Shot 1.\n@a: hi\n";
    let res = check(&input_for(text));
    assert!(res.diagnostics.iter().any(|d| d.code == "E-CONN-PROFILE"));
}

#[test]
fn quest_after_exact_empty_raises_no_profile_error() {
    let text = "---\nkind: quest\n---\n<quest id=\"q\" start=\"true\" after=\"\">\n<objective id=\"o\" done=\"true\"/>\n</quest>\n";
    let res = check(&input_for(text));
    assert!(
        !res.diagnostics.iter().any(|d| d.code == "E-CONN-PROFILE"),
        "exact-empty quest `after` must be accepted as an entry node: {:?}",
        res.diagnostics
    );
}

#[test]
fn quest_after_whitespace_only_raises_profile_error_without_panic() {
    let text = "---\nkind: quest\n---\n<quest id=\"q\" start=\"true\" after=\"   \">\n<objective id=\"o\" done=\"true\"/>\n</quest>\n";
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

use lute_check::connectivity::{assemble_graph, NodeId, PrereqState};

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

#[test]
fn absent_after_scene_prereq_is_absent() {
    // No `after:` key at all -- a valid entry node, `PrereqState::Absent`.
    let text = "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@a: hi\n";
    let docs = docs_for(&[("a.lute", text)]);
    let key_set = scene_key_set(&docs);
    let quest_ids = quest_id_set(&docs);
    let (g, diags) = assemble_graph(&docs, &key_set, &quest_ids);
    assert!(diags.is_empty(), "unexpected diags: {diags:?}");

    let node = NodeId::Scene("a.s01ep01".to_string());
    let info = g.nodes.get(&node).expect("scene node must exist");
    assert!(
        matches!(info.prereq, PrereqState::Absent),
        "an absent `after` must resolve to PrereqState::Absent, got {:?}",
        info.prereq
    );
}

#[test]
fn malformed_after_scene_prereq_is_invalid_not_absent() {
    // `after:` present but out-of-profile (negation is unsupported) --
    // `parse_prereq` returns `None`. This MUST resolve to
    // `PrereqState::Invalid`, distinct from an absent `after` -- T6/T10
    // reachability/envelope passes rely on telling the two apart, never
    // treating a malformed doc as a clean entry node.
    let text = "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\nafter: '!visited(\"x\")'\n---\n## Shot 1.\n@a: hi\n";
    let docs = docs_for(&[("a.lute", text)]);
    let key_set = scene_key_set(&docs);
    let quest_ids = quest_id_set(&docs);
    let (g, diags) = assemble_graph(&docs, &key_set, &quest_ids);
    assert!(diags.is_empty(), "unexpected diags: {diags:?}");

    let node = NodeId::Scene("a.s01ep01".to_string());
    let info = g.nodes.get(&node).expect("malformed-after scene must still be a node");
    assert!(
        matches!(info.prereq, PrereqState::Invalid),
        "a malformed `after` must resolve to PrereqState::Invalid, got {:?}",
        info.prereq
    );
    assert!(
        g.edges.values().all(|targets| !targets.contains(&node)),
        "an Invalid prereq must contribute no outgoing edge: {:?}",
        g.edges
    );
}

#[test]
fn nonstring_after_scene_prereq_is_invalid_not_absent() {
    // `after:` present but its YAML value is NOT a string (`42`, not
    // `'42'`) -- `.as_str()`-based extraction used to collapse this with an
    // absent key into `None`, wrongly classifying a malformed doc as a
    // clean entry node (review-2). MUST resolve to `PrereqState::Invalid`
    // and contribute no outgoing edge.
    let text = "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\nafter: 42\n---\n## Shot 1.\n@a: hi\n";
    let docs = docs_for(&[("a.lute", text)]);
    let key_set = scene_key_set(&docs);
    let quest_ids = quest_id_set(&docs);
    let (g, diags) = assemble_graph(&docs, &key_set, &quest_ids);
    assert!(diags.is_empty(), "unexpected diags: {diags:?}");

    let node = NodeId::Scene("a.s01ep01".to_string());
    let info = g.nodes.get(&node).expect("non-string-after scene must still be a node");
    assert!(
        matches!(info.prereq, PrereqState::Invalid),
        "a present non-string `after` must resolve to PrereqState::Invalid, got {:?}",
        info.prereq
    );
    assert!(
        g.edges.values().all(|targets| !targets.contains(&node)),
        "an Invalid prereq must contribute no outgoing edge: {:?}",
        g.edges
    );
}

#[test]
fn empty_string_after_scene_prereq_is_absent() {
    // `after: ''` -- present, a string, but empty. Spec §4.1: absent OR
    // empty `after` is a graph entry point, so this is `Absent`, NOT
    // `Invalid` (empty CEL text fails to parse but is not malformed here).
    let text = "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\nafter: ''\n---\n## Shot 1.\n@a: hi\n";
    let docs = docs_for(&[("a.lute", text)]);
    let key_set = scene_key_set(&docs);
    let quest_ids = quest_id_set(&docs);
    let (g, diags) = assemble_graph(&docs, &key_set, &quest_ids);
    assert!(diags.is_empty(), "unexpected diags: {diags:?}");

    let node = NodeId::Scene("a.s01ep01".to_string());
    let info = g.nodes.get(&node).expect("empty-after scene must still be a node");
    assert!(
        matches!(info.prereq, PrereqState::Absent),
        "an empty-string `after` must resolve to PrereqState::Absent, got {:?}",
        info.prereq
    );
}

#[test]
fn whitespace_only_after_scene_prereq_is_invalid_not_absent() {
    // `after: '   '` -- present, a string, non-empty (whitespace only).
    // NOT the Absent entry case (that is reserved for the EXACT empty
    // string); a whitespace-only value is a present, malformed CEL string
    // that `parse_prereq` rejects, so it MUST resolve to
    // `PrereqState::Invalid`.
    let text = "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\nafter: '   '\n---\n## Shot 1.\n@a: hi\n";
    let docs = docs_for(&[("a.lute", text)]);
    let key_set = scene_key_set(&docs);
    let quest_ids = quest_id_set(&docs);
    let (g, diags) = assemble_graph(&docs, &key_set, &quest_ids);
    assert!(diags.is_empty(), "unexpected diags: {diags:?}");

    let node = NodeId::Scene("a.s01ep01".to_string());
    let info = g.nodes.get(&node).expect("whitespace-only-after scene must still be a node");
    assert!(
        matches!(info.prereq, PrereqState::Invalid),
        "a whitespace-only `after` must resolve to PrereqState::Invalid, got {:?}",
        info.prereq
    );
}

#[test]
fn quest_admitted_regardless_of_stale_quest_ids_set() {
    // Review fix (RevT5): a nonempty-`after`-declaring quest MUST become a
    // graph node even when the caller-supplied `quest_ids` set does not
    // contain it (stale/filtered relative to `docs`) -- admission depends
    // ONLY on the quest itself declaring a nonempty `after`.
    let text = "---\nkind: quest\n---\n<quest id=\"q\" start=\"true\" after=\"completed('other')\">\n<objective id=\"o1\" done=\"true\"/>\n</quest>\n<quest id=\"other\" start=\"true\">\n<objective id=\"o2\" done=\"true\"/>\n</quest>\n";
    let docs = docs_for(&[("quests.lute", text)]);
    let key_set = scene_key_set(&docs);
    let stale_quest_ids: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let (g, diags) = assemble_graph(&docs, &key_set, &stale_quest_ids);
    assert!(diags.is_empty(), "unexpected diags: {diags:?}");

    let q_node = NodeId::Quest("q".to_string());
    assert!(
        g.nodes.contains_key(&q_node),
        "an after-declaring quest must be admitted even when absent from `quest_ids`: {:?}",
        g.nodes.keys().collect::<Vec<_>>()
    );
}

// --- Task 6: `E-CONN-UNREACHABLE` structural reachability + `E-CONN-FORMULA-TOO-COMPLEX` ---

use lute_check::connectivity::{ambiguous_quest_ids, check_reachability, unreachable_quest_ids, Reachability};
use std::collections::BTreeSet;

#[test]
fn entry_scene_is_reachable() {
    let text_a = "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@a: hi\n";
    let docs = docs_for(&[("a.lute", text_a)]);
    let key_set = scene_key_set(&docs);
    let quest_ids = quest_id_set(&docs);
    let (g, diags) = assemble_graph(&docs, &key_set, &quest_ids);
    assert!(diags.is_empty(), "unexpected diags: {diags:?}");

    let (reach, r_diags) = check_reachability(&g, &quest_ids, &BTreeSet::new(), &BTreeSet::new());
    let node = NodeId::Scene("a.s01ep01".to_string());
    assert_eq!(reach.get(&node), Some(&Reachability::Reachable));
    assert!(r_diags.is_empty(), "an entry node must never earn a reachability diagnostic: {r_diags:?}");
}

#[test]
fn node_behind_unreachable_completed_quest_is_unreachable() {
    // `a` gates on `completed("deadQ")`; `deadQ` is injected into the
    // synthetic `unreachable_quests` set (T6's own reviewable core -- the
    // real project-wide extraction is `unreachable_quest_ids` below).
    // `deadQ` is NOT declared anywhere here -- since it is still consulted
    // via `unreachable_quests` directly, wire it into `quest_ids` too (T6
    // review's step-1 "undeclared" precedence would otherwise shadow it).
    let text_a =
        "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\nafter: 'completed(\"deadQ\")'\n---\n## Shot 1.\n@a: hi\n";
    let docs = docs_for(&[("a.lute", text_a)]);
    let key_set = scene_key_set(&docs);
    let quest_ids: BTreeSet<String> = ["deadQ".to_string()].into_iter().collect();
    let (g, diags) = assemble_graph(&docs, &key_set, &quest_ids);
    assert!(diags.is_empty(), "unexpected diags: {diags:?}");

    let unreachable_quests: BTreeSet<String> = ["deadQ".to_string()].into_iter().collect();
    let (reach, r_diags) = check_reachability(&g, &quest_ids, &BTreeSet::new(), &unreachable_quests);
    let node = NodeId::Scene("a.s01ep01".to_string());
    assert_eq!(reach.get(&node), Some(&Reachability::Unreachable));
    let hit = r_diags
        .iter()
        .find(|(_, d)| d.code == "E-CONN-UNREACHABLE")
        .unwrap_or_else(|| panic!("expected E-CONN-UNREACHABLE, got {r_diags:?}"));
    assert!(
        !hit.1.message.contains("declared routes"),
        "E-CONN-UNREACHABLE is exempt from the declared-routes hedge (design spec §2.6): {}",
        hit.1.message
    );
}

#[test]
fn transitive_unreachability_propagates() {
    // `a` gates on the dead quest; `b` gates on `visited(a)`. Both must
    // resolve Unreachable -- the taint propagates one memoized hop.
    let text_a =
        "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\nafter: 'completed(\"deadQ\")'\n---\n## Shot 1.\n@a: hi\n";
    let text_b =
        "---\nkind: scene\ncharacter: b\nseason: 1\nepisode: 1\nafter: 'visited(\"a.s01ep01\")'\n---\n## Shot 1.\n@b: hi\n";
    let docs = docs_for(&[("a.lute", text_a), ("b.lute", text_b)]);
    let key_set = scene_key_set(&docs);
    let quest_ids: BTreeSet<String> = ["deadQ".to_string()].into_iter().collect();
    let (g, diags) = assemble_graph(&docs, &key_set, &quest_ids);
    assert!(diags.is_empty(), "unexpected diags: {diags:?}");

    let unreachable_quests: BTreeSet<String> = ["deadQ".to_string()].into_iter().collect();
    let (reach, r_diags) = check_reachability(&g, &quest_ids, &BTreeSet::new(), &unreachable_quests);
    assert_eq!(reach.get(&NodeId::Scene("a.s01ep01".to_string())), Some(&Reachability::Unreachable));
    assert_eq!(reach.get(&NodeId::Scene("b.s01ep01".to_string())), Some(&Reachability::Unreachable));
    assert_eq!(
        r_diags.iter().filter(|(_, d)| d.code == "E-CONN-UNREACHABLE").count(),
        2,
        "both a and b must each earn their own E-CONN-UNREACHABLE: {r_diags:?}"
    );
}

#[test]
fn or_arm_still_reachable_keeps_node_reachable() {
    // `gated` gates on `completed(deadQ) || visited(alive)` -- one live arm
    // is enough (Or's dominance rule): the dead quest must NOT poison it.
    let text_alive = "---\nkind: scene\ncharacter: alive\nseason: 1\nepisode: 1\n---\n## Shot 1.\n@alive: hi\n";
    let text_gated = "---\nkind: scene\ncharacter: gated\nseason: 1\nepisode: 1\nafter: 'completed(\"deadQ\") || visited(\"alive.s01ep01\")'\n---\n## Shot 1.\n@gated: hi\n";
    let docs = docs_for(&[("alive.lute", text_alive), ("gated.lute", text_gated)]);
    let key_set = scene_key_set(&docs);
    let quest_ids: BTreeSet<String> = ["deadQ".to_string()].into_iter().collect();
    let (g, diags) = assemble_graph(&docs, &key_set, &quest_ids);
    assert!(diags.is_empty(), "unexpected diags: {diags:?}");

    let unreachable_quests: BTreeSet<String> = ["deadQ".to_string()].into_iter().collect();
    let (reach, r_diags) = check_reachability(&g, &quest_ids, &BTreeSet::new(), &unreachable_quests);
    let node = NodeId::Scene("gated.s01ep01".to_string());
    assert_eq!(reach.get(&node), Some(&Reachability::Reachable));
    assert!(
        !r_diags.iter().any(|(_, d)| d.code == "E-CONN-UNREACHABLE"),
        "a node with one live Or arm must never be flagged: {r_diags:?}"
    );
}

#[test]
fn invalid_formula_is_unknown_not_flagged() {
    // Out-of-profile `after` (negation) resolves to PrereqState::Invalid --
    // must be Unknown, never guessed reachable/unreachable.
    let text_a =
        "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\nafter: '!visited(\"x\")'\n---\n## Shot 1.\n@a: hi\n";
    let docs = docs_for(&[("a.lute", text_a)]);
    let key_set = scene_key_set(&docs);
    let quest_ids = quest_id_set(&docs);
    let (g, diags) = assemble_graph(&docs, &key_set, &quest_ids);
    assert!(diags.is_empty(), "unexpected diags: {diags:?}");

    let (reach, r_diags) = check_reachability(&g, &quest_ids, &BTreeSet::new(), &BTreeSet::new());
    let node = NodeId::Scene("a.s01ep01".to_string());
    assert_eq!(reach.get(&node), Some(&Reachability::Unknown));
    assert!(
        !r_diags.iter().any(|(_, d)| d.code == "E-CONN-UNREACHABLE"),
        "an Invalid formula must never earn E-CONN-UNREACHABLE (provable-only): {r_diags:?}"
    );
}

#[test]
fn unknown_visited_ref_does_not_cascade_to_false() {
    // `a` references a nonexistent scene key -- E-CONN-UNKNOWN-NODE (T4)
    // fires separately, but reachability must stay Unknown, never cascade
    // into a false E-CONN-UNREACHABLE.
    let text_a = "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\nafter: 'visited(\"nonexistent.s99ep99\")'\n---\n## Shot 1.\n@a: hi\n";
    let docs = docs_for(&[("a.lute", text_a)]);
    let key_set = scene_key_set(&docs);
    let quest_ids = quest_id_set(&docs);

    let unknown_diags = resolve_nodes(&docs, &key_set, &quest_ids);
    assert!(
        unknown_diags.iter().any(|(_, d)| d.code == "E-CONN-UNKNOWN-NODE"),
        "fixture must trigger E-CONN-UNKNOWN-NODE (T4): {unknown_diags:?}"
    );

    let (g, diags) = assemble_graph(&docs, &key_set, &quest_ids);
    assert!(diags.is_empty(), "unexpected diags: {diags:?}");

    let (reach, r_diags) = check_reachability(&g, &quest_ids, &BTreeSet::new(), &BTreeSet::new());
    let node = NodeId::Scene("a.s01ep01".to_string());
    assert_eq!(reach.get(&node), Some(&Reachability::Unknown));
    assert!(
        !r_diags.iter().any(|(_, d)| d.code == "E-CONN-UNREACHABLE"),
        "an unresolved atom must never cascade to a false E-CONN-UNREACHABLE: {r_diags:?}"
    );
}

#[test]
fn completed_on_known_quest_node_defaults_reachable() {
    // `q1` declares a present-but-empty `after` (admitted as a node,
    // PrereqState::Absent); `q2` gates on `completed('q1')`. `q1` is a
    // known graph node and NOT in `unreachable_quests` -> Reachable.
    let text = "---\nkind: quest\n---\n<quest id=\"q1\" start=\"true\" after=\"\">\n<objective id=\"o1\" done=\"true\"/>\n</quest>\n<quest id=\"q2\" start=\"true\" after=\"completed('q1')\">\n<objective id=\"o2\" done=\"true\"/>\n</quest>\n";
    let docs = docs_for(&[("quests.lute", text)]);
    let key_set = scene_key_set(&docs);
    let quest_ids = quest_id_set(&docs);
    let (g, diags) = assemble_graph(&docs, &key_set, &quest_ids);
    assert!(diags.is_empty(), "unexpected diags: {diags:?}");

    let (reach, r_diags) = check_reachability(&g, &quest_ids, &BTreeSet::new(), &BTreeSet::new());
    assert_eq!(reach.get(&NodeId::Quest("q1".to_string())), Some(&Reachability::Reachable));
    assert_eq!(reach.get(&NodeId::Quest("q2".to_string())), Some(&Reachability::Reachable));
    assert!(r_diags.is_empty(), "unexpected diags: {r_diags:?}");
}

#[test]
fn oversized_formula_is_capped() {
    // 300 Or-chained atoms, over the 256-atom cap.
    let clauses: Vec<String> = (0..300).map(|i| format!("visited(\"k{i}.s01ep01\")")).collect();
    let formula = clauses.join(" || ");
    let text_a = format!(
        "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\nafter: '{formula}'\n---\n## Shot 1.\n@a: hi\n"
    );
    let docs = docs_for(&[("a.lute", &text_a)]);
    let key_set = scene_key_set(&docs);
    let quest_ids = quest_id_set(&docs);
    let (g, diags) = assemble_graph(&docs, &key_set, &quest_ids);
    assert!(diags.is_empty(), "unexpected diags: {diags:?}");

    let (reach, r_diags) = check_reachability(&g, &quest_ids, &BTreeSet::new(), &BTreeSet::new());
    assert!(
        r_diags.iter().any(|(_, d)| d.code == "E-CONN-FORMULA-TOO-COMPLEX"),
        "expected E-CONN-FORMULA-TOO-COMPLEX for a 300-atom formula: {r_diags:?}"
    );
    let node = NodeId::Scene("a.s01ep01".to_string());
    assert_eq!(
        reach.get(&node),
        Some(&Reachability::Unknown),
        "an over-cap formula must not be evaluated"
    );
}

#[test]
fn declared_plain_alive_quest_via_completed_is_reachable() {
    // `plainQ` declares no `after` at all -- never a graph node -- but IS a
    // declared quest. `completed("plainQ")` from a scene must read
    // Reachable, NOT Unknown (T6 review: `completed(Q)` must consult the
    // FULL declared quest-id set, independent of `after`-opt-in).
    let quest_text = "---\nkind: quest\n---\n<quest id=\"plainQ\" start=\"true\">\n<objective id=\"o1\" done=\"true\"/>\n</quest>\n";
    let scene_text = "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\nafter: 'completed(\"plainQ\")'\n---\n## Shot 1.\n@a: hi\n";
    let docs = docs_for(&[("quests.lute", quest_text), ("scene.lute", scene_text)]);
    let key_set = scene_key_set(&docs);
    let quest_ids = quest_id_set(&docs);
    let (g, diags) = assemble_graph(&docs, &key_set, &quest_ids);
    assert!(diags.is_empty(), "unexpected diags: {diags:?}");

    let (reach, r_diags) = check_reachability(&g, &quest_ids, &BTreeSet::new(), &BTreeSet::new());
    let node = NodeId::Scene("a.s01ep01".to_string());
    assert_eq!(
        reach.get(&node),
        Some(&Reachability::Reachable),
        "a declared plain alive quest must be Reachable, not Unknown"
    );
    assert!(r_diags.is_empty(), "unexpected diags: {r_diags:?}");
}

#[test]
fn or_and_over_plain_and_dead_completed_quests() {
    // `plainAliveQ` is a declared, plain (no-`after`), alive quest;
    // `deadQ` is synthetically injected into `unreachable_quests`. Or
    // dominance keeps the Or-scene Reachable; And requires both, so the
    // And-scene is Unreachable.
    let quest_text = "---\nkind: quest\n---\n<quest id=\"plainAliveQ\" start=\"true\">\n<objective id=\"o1\" done=\"true\"/>\n</quest>\n<quest id=\"deadQ\" start=\"true\">\n<objective id=\"o2\" done=\"true\"/>\n</quest>\n";
    let or_text = "---\nkind: scene\ncharacter: orScene\nseason: 1\nepisode: 1\nafter: 'completed(\"plainAliveQ\") || completed(\"deadQ\")'\n---\n## Shot 1.\n@orScene: hi\n";
    let and_text = "---\nkind: scene\ncharacter: andScene\nseason: 1\nepisode: 1\nafter: 'completed(\"plainAliveQ\") && completed(\"deadQ\")'\n---\n## Shot 1.\n@andScene: hi\n";
    let docs = docs_for(&[("quests.lute", quest_text), ("or.lute", or_text), ("and.lute", and_text)]);
    let key_set = scene_key_set(&docs);
    let quest_ids = quest_id_set(&docs);
    let (g, diags) = assemble_graph(&docs, &key_set, &quest_ids);
    assert!(diags.is_empty(), "unexpected diags: {diags:?}");

    let unreachable_quests: BTreeSet<String> = ["deadQ".to_string()].into_iter().collect();
    let (reach, r_diags) = check_reachability(&g, &quest_ids, &BTreeSet::new(), &unreachable_quests);
    let or_node = NodeId::Scene("orScene.s01ep01".to_string());
    let and_node = NodeId::Scene("andScene.s01ep01".to_string());
    assert_eq!(reach.get(&or_node), Some(&Reachability::Reachable));
    assert_eq!(reach.get(&and_node), Some(&Reachability::Unreachable));
    assert_eq!(
        r_diags.iter().filter(|(_, d)| d.code == "E-CONN-UNREACHABLE").count(),
        1,
        "only the And scene may earn E-CONN-UNREACHABLE, never the Or scene: {r_diags:?}"
    );
}

#[test]
fn transitive_unreachable_through_opted_in_quest_completed() {
    // `deadQ` (plain, synthetically dead) taints `qOpt`, an `after`-opted-in
    // quest gating on `completed('deadQ')`. `s` gates on `completed("qOpt")`
    // -- both `qOpt`'s own node and `s` must resolve Unreachable (T6
    // review's step-3/4 transitivity through a memoized graph node).
    let quest_text = "---\nkind: quest\n---\n<quest id=\"deadQ\" start=\"true\">\n<objective id=\"o1\" done=\"true\"/>\n</quest>\n<quest id=\"qOpt\" start=\"true\" after=\"completed('deadQ')\">\n<objective id=\"o2\" done=\"true\"/>\n</quest>\n";
    let scene_text = "---\nkind: scene\ncharacter: s\nseason: 1\nepisode: 1\nafter: 'completed(\"qOpt\")'\n---\n## Shot 1.\n@s: hi\n";
    let docs = docs_for(&[("quests.lute", quest_text), ("scene.lute", scene_text)]);
    let key_set = scene_key_set(&docs);
    let quest_ids = quest_id_set(&docs);
    let (g, diags) = assemble_graph(&docs, &key_set, &quest_ids);
    assert!(diags.is_empty(), "unexpected diags: {diags:?}");

    let unreachable_quests: BTreeSet<String> = ["deadQ".to_string()].into_iter().collect();
    let (reach, r_diags) = check_reachability(&g, &quest_ids, &BTreeSet::new(), &unreachable_quests);
    let scene_node = NodeId::Scene("s.s01ep01".to_string());
    let qopt_node = NodeId::Quest("qOpt".to_string());
    assert_eq!(reach.get(&qopt_node), Some(&Reachability::Unreachable));
    assert_eq!(reach.get(&scene_node), Some(&Reachability::Unreachable));
    assert_eq!(
        r_diags.iter().filter(|(_, d)| d.code == "E-CONN-UNREACHABLE").count(),
        2,
        "both qOpt and s must each earn their own E-CONN-UNREACHABLE: {r_diags:?}"
    );
}

#[test]
fn unreachable_quest_ids_extracts_by_span_match() {
    // `deadQ`'s dead `start` earns E-QUEST-UNREACHABLE from the per-file
    // `check()` pass; `aliveQ` is clean. The helper must extract exactly
    // `deadQ`, matched by `Quest.span`, never by id text alone.
    let text = "---\nkind: quest\n---\n<quest id=\"deadQ\" start=\"1 > 2\">\n\
         <objective id=\"o\" done=\"true\"/>\n</quest>\n\
         <quest id=\"aliveQ\" start=\"true\">\n\
         <objective id=\"o2\" done=\"true\"/>\n</quest>\n";
    let docs = docs_for(&[("quests.lute", text)]);
    let result = check(&input_for(text));
    assert!(
        result.diagnostics.iter().any(|d| d.code == "E-QUEST-UNREACHABLE"),
        "fixture must trigger E-QUEST-UNREACHABLE: {:?}",
        result.diagnostics
    );
    let file_results = vec![(PathBuf::from("quests.lute"), result)];
    let ids = unreachable_quest_ids(&docs, &file_results);
    assert_eq!(ids, ["deadQ".to_string()].into_iter().collect::<BTreeSet<_>>());
}

#[test]
fn unreachable_quest_ids_ignores_paths_with_no_matching_result() {
    // A doc with no corresponding entry in `file_results` contributes
    // nothing -- never a panic.
    let text = "---\nkind: quest\n---\n<quest id=\"deadQ\" start=\"1 > 2\">\n\
         <objective id=\"o\" done=\"true\"/>\n</quest>\n";
    let docs = docs_for(&[("quests.lute", text)]);
    let ids = unreachable_quest_ids(&docs, &[]);
    assert!(ids.is_empty());
}

#[test]
fn duplicate_quest_id_is_omitted_from_unreachable_set() {
    // `dupQ` is declared TWICE: one dead (`start="1 > 2"`), one alive. The
    // shared id must be OMITTED entirely from `unreachable_quest_ids` --
    // never provably Unreachable when a DIFFERENT declaration is alive
    // (T6 review: provable-only, ambiguous -> Unknown, never a guess).
    let text = "---\nkind: quest\n---\n<quest id=\"dupQ\" start=\"1 > 2\">\n\
         <objective id=\"o1\" done=\"true\"/>\n</quest>\n\
         <quest id=\"dupQ\" start=\"true\">\n\
         <objective id=\"o2\" done=\"true\"/>\n</quest>\n";
    let docs = docs_for(&[("quests.lute", text)]);
    let result = check(&input_for(text));
    assert!(
        result.diagnostics.iter().any(|d| d.code == "E-QUEST-UNREACHABLE"),
        "fixture must trigger E-QUEST-UNREACHABLE on the dead declaration: {:?}",
        result.diagnostics
    );
    let file_results = vec![(PathBuf::from("quests.lute"), result)];

    assert_eq!(
        ambiguous_quest_ids(&docs),
        ["dupQ".to_string()].into_iter().collect::<BTreeSet<_>>()
    );

    let unreachable = unreachable_quest_ids(&docs, &file_results);
    assert!(
        !unreachable.contains("dupQ"),
        "a duplicate quest id must never be provably Unreachable: {unreachable:?}"
    );
}

#[test]
fn duplicate_quest_id_local_dead_completed_is_unknown() {
    // Both `dupQ` declarations are plain (no `after`); one is locally dead
    // (E-QUEST-UNREACHABLE), the other alive. A scene's `completed("dupQ")`
    // must resolve Unknown -- WITHOUT the ambiguous-id precedence check
    // (T6 review-2), it would wrongly fall through to the plain-quest
    // default and read Reachable, since `unreachable_quest_ids` already
    // omits the ambiguous id from its own output.
    let quest_text = "---\nkind: quest\n---\n<quest id=\"dupQ\" start=\"1 > 2\">\n\
         <objective id=\"o1\" done=\"true\"/>\n</quest>\n\
         <quest id=\"dupQ\" start=\"true\">\n\
         <objective id=\"o2\" done=\"true\"/>\n</quest>\n";
    let scene_text = "---\nkind: scene\ncharacter: s\nseason: 1\nepisode: 1\nafter: 'completed(\"dupQ\")'\n---\n## Shot 1.\n@s: hi\n";
    let docs = docs_for(&[("quests.lute", quest_text), ("scene.lute", scene_text)]);
    let result = check(&input_for(quest_text));
    let file_results = vec![(PathBuf::from("quests.lute"), result)];
    let unreachable_quests = unreachable_quest_ids(&docs, &file_results);
    assert!(!unreachable_quests.contains("dupQ"));

    let key_set = scene_key_set(&docs);
    let quest_ids = quest_id_set(&docs);
    let ambiguous = ambiguous_quest_ids(&docs);
    assert!(ambiguous.contains("dupQ"));
    let (g, diags) = assemble_graph(&docs, &key_set, &quest_ids);
    assert!(diags.is_empty(), "unexpected diags: {diags:?}");

    let (reach, r_diags) = check_reachability(&g, &quest_ids, &ambiguous, &unreachable_quests);
    let scene_node = NodeId::Scene("s.s01ep01".to_string());
    assert_eq!(
        reach.get(&scene_node),
        Some(&Reachability::Unknown),
        "completed() on a duplicate id must be Unknown, never guessed Reachable/Unreachable"
    );
    assert!(
        !r_diags.iter().any(|(_, d)| d.code == "E-CONN-UNREACHABLE"),
        "unexpected diags: {r_diags:?}"
    );
}

#[test]
fn duplicate_quest_id_graph_dead_completed_is_unknown() {
    // `dupQ` is declared twice: once `after`-opted-in gating on the
    // synthetically dead `deadQ` (so its OWN graph-node reachability is
    // structurally Unreachable), once plain and alive. A scene's
    // `completed("dupQ")` must still resolve Unknown -- the ambiguous-id
    // precedence (checked BEFORE the graph-node lookup) must dominate even
    // a genuinely Unreachable memoized node for one of the id's own
    // declarations.
    let quest_text = "---\nkind: quest\n---\n<quest id=\"deadQ\" start=\"true\">\n\
         <objective id=\"o0\" done=\"true\"/>\n</quest>\n\
         <quest id=\"dupQ\" start=\"true\" after=\"completed('deadQ')\">\n\
         <objective id=\"o1\" done=\"true\"/>\n</quest>\n\
         <quest id=\"dupQ\" start=\"true\">\n\
         <objective id=\"o2\" done=\"true\"/>\n</quest>\n";
    let scene_text = "---\nkind: scene\ncharacter: s\nseason: 1\nepisode: 1\nafter: 'completed(\"dupQ\")'\n---\n## Shot 1.\n@s: hi\n";
    let docs = docs_for(&[("quests.lute", quest_text), ("scene.lute", scene_text)]);
    let key_set = scene_key_set(&docs);
    let quest_ids = quest_id_set(&docs);
    let ambiguous = ambiguous_quest_ids(&docs);
    assert_eq!(ambiguous, ["dupQ".to_string()].into_iter().collect::<BTreeSet<_>>());
    let (g, diags) = assemble_graph(&docs, &key_set, &quest_ids);
    assert!(diags.is_empty(), "unexpected diags: {diags:?}");

    let unreachable_quests: BTreeSet<String> = ["deadQ".to_string()].into_iter().collect();
    let (reach, r_diags) = check_reachability(&g, &quest_ids, &ambiguous, &unreachable_quests);

    // `dupQ`'s own opted-in graph node IS genuinely Unreachable structurally.
    assert_eq!(reach.get(&NodeId::Quest("dupQ".to_string())), Some(&Reachability::Unreachable));
    // But a DIFFERENT formula's `completed("dupQ")` reference must still
    // read Unknown -- the ambiguity dominates over the graph-node lookup.
    let scene_node = NodeId::Scene("s.s01ep01".to_string());
    assert_eq!(
        reach.get(&scene_node),
        Some(&Reachability::Unknown),
        "an ambiguous id's completed() reference must stay Unknown even when its OWN node is graph-dead"
    );
    assert_eq!(
        r_diags.iter().filter(|(_, d)| d.code == "E-CONN-UNREACHABLE").count(),
        1,
        "only dupQ's own opted-in node may earn E-CONN-UNREACHABLE, never the scene: {r_diags:?}"
    );
}

// ---------------------------------------------------------------------
// T7: `producible()` rule-dependency walk + relational-objective-liveness
// (dsl 0.4.0 §4.2/§B) -- wired into the SAME by_root pipeline
// `lute-cli::run_check_project` runs (reachability -> `live_assert_relations`
// -> per-doc `producible()` -> `scan_objective_liveness`), reproduced here
// so the connectivity layer's own test suite covers the full project-wide
// wiring without depending on the `lute-cli` binary.
// ---------------------------------------------------------------------

use lute_check::producible::{producible, scan_objective_liveness};
use lute_check::{fold_env, resolve_components, resolve_imports, CheckResult};
use lute_core_span::Diagnostic;
use lute_manifest::project::{load_project, project_providers, resolve_document_snapshot};
use lute_manifest::snapshot::CapabilitySnapshot;
use std::collections::BTreeMap;

/// Build a `CheckInput` for a REAL file on disk, resolving `uses:`/
/// `extends:`/`components:` against its own directory and (optionally) a
/// project root -- mirrors `lute-cli`'s `build_input` exactly (project +
/// provider + import resolution), the ONLY way to exercise a genuine
/// schema-imported `RelVocab` (a plain in-memory `docs_for` never resolves
/// `uses:`).
fn build_project_input(file: &PathBuf, project_dir: Option<&std::path::Path>) -> CheckInput {
    let text = std::fs::read_to_string(file)
        .unwrap_or_else(|e| panic!("read {}: {e}", file.display()));
    let project = project_dir.and_then(|d| load_project(d).ok().flatten());
    let providers = project_providers(project.as_ref());
    let (doc, _) = lute_syntax::parse(&text);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let (snapshot, _rdiags) =
        resolve_document_snapshot(project.as_ref(), meta0.profile.as_deref(), &meta0.plugins);
    let base = file.parent().unwrap_or_else(|| std::path::Path::new("."));
    let imports = resolve_imports(base, &meta0.uses, &meta0.extends, doc.meta.span);
    let components = resolve_components(base, &meta0.components, doc.meta.span);
    CheckInput {
        text,
        uri: file.display().to_string(),
        snapshot,
        providers,
        mode: Mode::Ci,
        imports,
        components,
    }
}

fn parse_meta(
    meta: &lute_syntax::ast::Meta,
    snapshot: &CapabilitySnapshot,
) -> (lute_check::TypedMeta, Vec<Diagnostic>) {
    lute_check::parse_meta(meta, snapshot)
}

/// Reproduce `run_check_project`'s by_root T3-T7 pipeline over an explicit
/// `(path, CheckInput)` list (all ONE resolved root) and return the
/// project-wide diagnostics `producible()`/`scan_objective_liveness`
/// contribute.
fn run_producible_pipeline(files: Vec<(PathBuf, CheckInput)>) -> Vec<(PathBuf, Diagnostic)> {
    let mut docs: Vec<(PathBuf, lute_syntax::ast::Document)> = Vec::new();
    let mut foldeds: Vec<lute_check::FoldedEnv> = Vec::new();
    let mut file_results: Vec<(PathBuf, CheckResult)> = Vec::new();
    for (path, input) in &files {
        let (doc, _) = lute_syntax::parse(&input.text);
        let (folded, _, _) = fold_env(&doc, input);
        foldeds.push(folded);
        let result = check(input);
        file_results.push((path.clone(), result));
        docs.push((path.clone(), doc));
    }
    let key_set = scene_key_set(&docs);
    let quest_ids = quest_id_set(&docs);
    let (conn_graph, _cycle_diags) = assemble_graph(&docs, &key_set, &quest_ids);
    let unreachable_quests = unreachable_quest_ids(&docs, &file_results);
    let ambiguous_quests = ambiguous_quest_ids(&docs);
    let (reach, _reach_diags) =
        check_reachability(&conn_graph, &quest_ids, &ambiguous_quests, &unreachable_quests);
    let live_asserts = lute_check::connectivity::live_assert_relations(
        &docs,
        &reach,
        &ambiguous_quests,
        &unreachable_quests,
    );
    let no_params: BTreeMap<String, lute_check::DomainInfo> = BTreeMap::new();
    let mut out = Vec::new();
    for (idx, (path, doc)) in docs.iter().enumerate() {
        let folded = &foldeds[idx];
        let prod = producible(&folded.env.rel_vocab, &live_asserts);
        let defs = lute_check::DefTable {
            bodies: &folded.def_bodies,
            params: &folded.env.def_params,
        };
        let ctx = lute_check::DecideCtx {
            schema: &folded.env.state,
            dollar: None,
            params: &no_params,
        };
        for d in scan_objective_liveness(doc, &prod, &defs, &ctx) {
            out.push((path.clone(), d));
        }
    }
    out
}

/// Real files on disk under `project_dir` (real `uses:` resolution).
fn check_project_on_corpus(paths: &[&str], project_dir: &str) -> Vec<(PathBuf, Diagnostic)> {
    let project_dir = std::path::Path::new(project_dir);
    let files = paths
        .iter()
        .map(|p| {
            let path = PathBuf::from(*p);
            let input = build_project_input(&path, Some(project_dir));
            (path, input)
        })
        .collect();
    run_producible_pipeline(files)
}

/// In-memory fixture text, no project/`uses:` resolution (inline
/// `entities:`/`relations:`/`facts:`/`rules:` only) -- mirrors this file's
/// top-of-file `input_for` helper.
fn check_project_fixture(texts: &[(&str, &str)]) -> Vec<(PathBuf, Diagnostic)> {
    let files = texts
        .iter()
        .map(|(path, text)| (PathBuf::from(*path), input_for(text)))
        .collect();
    run_producible_pipeline(files)
}

// Canonical false-positive guard (spec §4.2's own worked counterexample):
// `docs/examples/quest-rescue-halsin.lute:31` gates
// `done="holds(canReach(player, grove))"`; `canReach` is `derive: true`
// (`act1.schema.yaml:14`), derived from `atLocation`/`connected`, BOTH
// unconditionally `facts:`-seeded -- producible from load, independent of
// any episode. A naive `::assert`-site-only search would falsely kill this
// shipped, correct example (spec §4.2's own stated failure mode); the
// rule-dependency walk must not.
#[test]
fn derived_relation_seeded_via_facts_is_producible_no_false_positive() {
    let diags = check_project_on_corpus(
        &["../../docs/examples/quest-rescue-halsin.lute"],
        "../../docs/examples",
    );
    assert!(
        !diags.iter().any(|(_, d)| d.code == "E-OBJECTIVE-UNSATISFIABLE"),
        "canReach is structurally producible (facts-seeded atLocation/connected feed its rule \
         closure) -- the halsin corpus objective must NEVER be flagged dead: {diags:?}"
    );
}

// The synthetic positive: a derived relation whose ONLY rule body needs a
// base relation with no `facts:` seed, not `reserved`, and no `::assert`
// site anywhere in the project -- structurally never producible, so an
// objective gated on it IS flagged.
#[test]
fn objective_on_never_producible_relation_is_dead() {
    let text = "---\nkind: quest\nentities:\n  c: { members: [ana] }\nrelations:\n  \
                neverSeeded: { args: [c], tier: run }\n  dead: { args: [c], derive: true }\n\
                rules:\n  - \"dead(X) :- neverSeeded(X)\"\n---\n\
                <quest id=\"q\" start=\"true\">\n\
                <objective id=\"o\" done=\"holds(dead(ana))\"/>\n</quest>\n";
    let diags = check_project_fixture(&[("dead.lute", text)]);
    let hit = diags.iter().find(|(_, d)| d.code == "E-OBJECTIVE-UNSATISFIABLE");
    assert!(
        hit.is_some(),
        "an objective gated on a relation with no facts seed/reserved tier/assert site \
         anywhere in the project must be flagged provably dead: {diags:?}"
    );
    assert!(
        hit.unwrap().1.message.contains("under your declared routes"),
        "a §4.2 diagnostic message MUST carry the verbatim declared-routes hedge (§2.6): {}",
        hit.unwrap().1.message
    );
}

// A relation with a `facts:` seed IS producible even with zero rules using
// it (base case (a), unconditional) -- an objective gated on it must NOT be
// flagged.
#[test]
fn objective_on_facts_seeded_base_relation_is_not_dead() {
    let text = "---\nkind: quest\nentities:\n  c: { members: [ana] }\nrelations:\n  \
                seeded: { args: [c], tier: run }\n\
                facts:\n  - \"seeded(ana)\"\n---\n\
                <quest id=\"q\" start=\"true\">\n\
                <objective id=\"o\" done=\"holds(seeded(ana))\"/>\n</quest>\n";
    let diags = check_project_fixture(&[("seeded.lute", text)]);
    assert!(
        !diags.iter().any(|(_, d)| d.code == "E-OBJECTIVE-UNSATISFIABLE"),
        "a facts:-seeded base relation is unconditionally producible: {diags:?}"
    );
}

// Reachability-gated assert-site base case (c): an `::assert{R(…)}` inside a
// node this root's own T6 pass PROVES `Unreachable` must NOT seed
// producibility -- that assert can never fire.
#[test]
fn assert_in_provably_unreachable_node_does_not_seed_producibility() {
    let text = "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\n---\n\
                ## Shot 1.\n::assert{ seen(a) }\n@a: hi\n";
    let docs = docs_for(&[("a.lute", text)]);
    let mut reach = BTreeMap::new();
    reach.insert(NodeId::Scene("a.s01ep01".to_string()), Reachability::Unreachable);
    let live = lute_check::connectivity::live_assert_relations(
        &docs,
        &reach,
        &BTreeSet::new(),
        &BTreeSet::new(),
    );
    assert!(
        !live.contains("seen"),
        "an assert inside a PROVABLY Unreachable node must never seed producibility: {live:?}"
    );
}

// The critical corollary: an `Unknown` node (this pass cannot prove EITHER
// way) MUST still seed -- provable-only means only a PROVEN `Unreachable`
// excludes; excluding `Unknown` would risk a false-dead claim on a node
// that may well be reachable at runtime.
#[test]
fn assert_in_unknown_node_still_seeds_producibility() {
    let text = "---\nkind: scene\ncharacter: a\nseason: 1\nepisode: 1\n---\n\
                ## Shot 1.\n::assert{ seen(a) }\n@a: hi\n";
    let docs = docs_for(&[("a.lute", text)]);
    let mut reach = BTreeMap::new();
    reach.insert(NodeId::Scene("a.s01ep01".to_string()), Reachability::Unknown);
    let live = lute_check::connectivity::live_assert_relations(
        &docs,
        &reach,
        &BTreeSet::new(),
        &BTreeSet::new(),
    );
    assert!(
        live.contains("seen"),
        "an assert inside an Unknown node must still seed producibility (provable-only, never a \
         false-dead claim): {live:?}"
    );
}

// A quest-body `::assert` mirrors the same reachability gate through the
// quest-lifecycle `unreachable_quests` set, not the graph `reach` map.
#[test]
fn assert_in_lifecycle_unreachable_quest_does_not_seed_producibility() {
    let text = "---\nkind: quest\n---\n<quest id=\"q\" start=\"true\">\n\
                <on event=\"questActive\">\n::assert{ seen(a) }\n</on>\n\
                <objective id=\"o\" done=\"true\"/>\n</quest>\n";
    let docs = docs_for(&[("q.lute", text)]);
    let unreachable: BTreeSet<String> = ["q".to_string()].into_iter().collect();
    let live = lute_check::connectivity::live_assert_relations(
        &docs,
        &BTreeMap::new(),
        &BTreeSet::new(),
        &unreachable,
    );
    assert!(
        !live.contains("seen"),
        "an assert inside a quest-lifecycle-dead (E-QUEST-UNREACHABLE) quest must never seed \
         producibility: {live:?}"
    );
}

// ---------------------------------------------------------------------
// Sound partial evaluator (substitute dead fact-query -> false/0, then run
// the EXISTING decide() R1-R5) -- NOT a top-level-only or naive nested-scan
// match. Each case below is a worked example from the algorithm's own
// contract.
// ---------------------------------------------------------------------

fn dead_relation_fixture(done: &str) -> String {
    format!(
        "---\nkind: quest\nentities:\n  c: {{ members: [ana] }}\nrelations:\n  \
         neverSeeded: {{ args: [c], tier: run }}\n  live: {{ args: [c], tier: run }}\n\
         facts:\n  - \"live(ana)\"\n---\n\
         <quest id=\"q\" start=\"true\">\n\
         <objective id=\"o\" done=\"{done}\"/>\n</quest>\n"
    )
}

// `count(deadR) > 0` -- substituted to `0 > 0` -- decides false -- DEAD.
// A top-level-only match would MISS this (the top-level node is `_>_`, not
// a bare fact-query call).
#[test]
fn count_comparison_greater_than_zero_over_dead_relation_is_dead() {
    let text = dead_relation_fixture("count(neverSeeded(ana)) > 0");
    let diags = check_project_fixture(&[("count_gt.lute", text.as_str())]);
    assert!(
        diags.iter().any(|(_, d)| d.code == "E-OBJECTIVE-UNSATISFIABLE"),
        "count(deadR) > 0 substitutes to 0 > 0 -- provably false -- must be flagged: {diags:?}"
    );
}

// `count(deadR) >= 0` -- substituted to `0 >= 0` -- decides TRUE -- the
// SAME dead relation, a DIFFERENT comparison, is fine: never flagged. This
// is exactly why constant substitution (not a nested "any dead call" scan)
// is required for soundness.
#[test]
fn count_comparison_greater_equal_zero_over_dead_relation_is_not_dead() {
    let text = dead_relation_fixture("count(neverSeeded(ana)) >= 0");
    let diags = check_project_fixture(&[("count_gte.lute", text.as_str())]);
    assert!(
        !diags.iter().any(|(_, d)| d.code == "E-OBJECTIVE-UNSATISFIABLE"),
        "count(deadR) >= 0 substitutes to 0 >= 0 -- provably TRUE, not dead: {diags:?}"
    );
}

// `holds(deadR) && x` -- AND short-circuits to false regardless of `x` --
// DEAD.
#[test]
fn and_with_dead_relation_short_circuits_dead() {
    let text = dead_relation_fixture("holds(neverSeeded(ana)) && holds(live(ana))");
    let diags = check_project_fixture(&[("and_dead.lute", text.as_str())]);
    assert!(
        diags.iter().any(|(_, d)| d.code == "E-OBJECTIVE-UNSATISFIABLE"),
        "holds(deadR) && holds(liveR) substitutes to false && Undecided -- AND short-circuits \
         to false -- must be flagged: {diags:?}"
    );
}

// `holds(deadR) || holds(liveR)` -- OR never proves false from one dead
// arm -- NOT dead. The naive "any nested dead call" scan would wrongly
// flag this.
#[test]
fn or_with_one_live_relation_is_not_dead() {
    let text = dead_relation_fixture("holds(neverSeeded(ana)) || holds(live(ana))");
    let diags = check_project_fixture(&[("or_live.lute", text.as_str())]);
    assert!(
        !diags.iter().any(|(_, d)| d.code == "E-OBJECTIVE-UNSATISFIABLE"),
        "holds(deadR) || holds(liveR) substitutes to false || Undecided -- OR never proves \
         false from one dead arm -- must NOT be flagged: {diags:?}"
    );
}

// `holds(deadR) || holds(unknownR)` -- `unknownR` is UNDECLARED (never
// substituted, stays Undecided per R5) -- OR still can't prove false -- NOT
// dead.
#[test]
fn or_with_one_undeclared_relation_is_not_dead() {
    let text = dead_relation_fixture("holds(neverSeeded(ana)) || holds(unknownR(ana))");
    let diags = check_project_fixture(&[("or_unknown.lute", text.as_str())]);
    assert!(
        !diags.iter().any(|(_, d)| d.code == "E-OBJECTIVE-UNSATISFIABLE"),
        "holds(deadR) || holds(unknownR) -- unknownR stays Undecided -- OR can't prove false: \
         {diags:?}"
    );
}
