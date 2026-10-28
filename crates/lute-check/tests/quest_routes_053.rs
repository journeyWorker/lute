//! 0.53.0 §7.1–§7.2: the routes into an accept-driven quest, and the one
//! rule for `heuristic` evidence ([`host_heuristic_nodes`]): a `Reachable`
//! verdict is `heuristic` exactly when every proof of it needs a host route
//! with `follows`.

use lute_check::connectivity::{
    assemble_graph, check_reachability, host_heuristic_nodes, quest_id_set, scene_key_set, Anchor,
    EdgeKind, FollowsDecl, HostGate, NodeId, PrereqState, Reachability,
};
use lute_check::{ProjectDocs, EMPTY_SNAPSHOT};
use lute_manifest::semantics::prereq::PrereqFormula;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

fn docs_for(texts: &[(&str, String)]) -> ProjectDocs<'static> {
    let docs = texts
        .iter()
        .map(|(path, text)| {
            let (doc, _) = lute_syntax::parse(text);
            (PathBuf::from(*path), doc)
        })
        .collect();
    ProjectDocs::parse(docs, &EMPTY_SNAPSHOT)
}

/// A quest document holding `quests`.
fn quests(quests: &str) -> String {
    format!("---\nkind: quest\n---\n{quests}")
}

fn quest(id: &str, attrs: &str, body: &str) -> String {
    format!("<quest id=\"{id}\"{attrs}>\n<objective id=\"o\" done=\"run.d\">\n{body}</objective>\n</quest>\n")
}

/// The graph, its verdicts and its heuristic nodes.
fn analyse(
    text: &str,
) -> (
    lute_check::connectivity::ConnGraph,
    BTreeMap<NodeId, Reachability>,
    BTreeSet<NodeId>,
) {
    let docs = docs_for(&[("q.lute", quests(text))]);
    let views = docs.views();
    let ids = quest_id_set(&views);
    let (g, diags) = assemble_graph(
        &views,
        &scene_key_set(&views),
        &ids,
        &lute_check::raises::RaiseAnchors::default(),
    );
    assert!(diags.is_empty(), "{diags:?}");
    let none = BTreeSet::new();
    let (reach, _) = check_reachability(&g, &ids, &none, &none);
    let heuristic = host_heuristic_nodes(&g, &reach, &ids, &none, &none);
    (g, reach, heuristic)
}

fn q(id: &str) -> NodeId {
    NodeId::Quest(id.to_string())
}

const ACCEPT_GIFT: &str = "::accept{quest=\"gift\"}\n";

/// In-script route only: `gift` is accepted from `giver`'s body. Proven.
#[test]
fn an_in_script_route_is_proven() {
    let (g, reach, heuristic) = analyse(&format!(
        "{}{}",
        quest("giver", " start=\"true\"", ACCEPT_GIFT),
        quest("gift", "", "")
    ));
    assert_eq!(reach[&q("gift")], Reachability::Reachable);
    assert!(heuristic.is_empty(), "{heuristic:?}");
    let PrereqState::Anchored(anchors) = &g.nodes[&q("gift")].prereq else {
        panic!("{g:?}");
    };
    assert_eq!(
        anchors,
        &[Anchor {
            kind: EdgeKind::Accept,
            from: vec![q("giver")],
            host: None,
        }]
    );
}

/// Host route with `follows` only: heuristic, and so is a quest that can
/// start only through it.
#[test]
fn a_host_route_with_follows_alone_is_heuristic() {
    let (g, reach, heuristic) = analyse(&format!(
        "{}{}{}",
        quest("intro", " start=\"true\" follows=\"\"", ""),
        quest(
            "gift",
            " accept=\"external\" follows=\"completed('intro')\"",
            ""
        ),
        quest("after", " start=\"quest.gift.state == 'active'\"", ""),
    ));
    assert_eq!(reach[&q("gift")], Reachability::Reachable);
    assert_eq!(reach[&q("after")], Reachability::Reachable);
    assert_eq!(
        heuristic,
        BTreeSet::from([q("after"), q("gift")]),
        "{reach:?}"
    );
    let info = &g.nodes[&q("gift")];
    let formula = PrereqFormula::Completed("intro".into());
    assert_eq!(info.follows, FollowsDecl::Valid(formula.clone()));
    let PrereqState::Anchored(anchors) = &info.prereq else {
        panic!("{g:?}");
    };
    assert_eq!(
        anchors,
        &[Anchor {
            kind: EdgeKind::Accept,
            from: vec![],
            host: Some(HostGate::Follows(formula)),
        }]
    );
    // The declared `follows` edge is drawn.
    assert_eq!(
        g.edge_kinds_for(&q("intro"), &q("gift"))
            .map(|k| k.iter().copied().collect::<Vec<_>>()),
        Some(vec![EdgeKind::Completed])
    );
}

/// Both: an in-script route beside the host route with `follows` proves the
/// quest without host policy.
#[test]
fn an_in_script_route_beside_the_host_route_is_proven() {
    let (_, reach, heuristic) = analyse(&format!(
        "{}{}{}",
        quest("intro", " start=\"true\" follows=\"\"", ""),
        quest("giver", " start=\"true\"", ACCEPT_GIFT),
        quest(
            "gift",
            " accept=\"external\" follows=\"completed('intro')\"",
            ""
        ),
    ));
    assert_eq!(reach[&q("gift")], Reachability::Reachable);
    assert!(heuristic.is_empty(), "{heuristic:?}");
}

/// An external quest with no in-script source and no `follows` stays
/// unanchored (no graph node), as in 0.52.
#[test]
fn a_bare_external_quest_stays_unanchored() {
    let (g, _, _) = analyse(&quest("board", " accept=\"external\"", ""));
    assert!(!g.nodes.contains_key(&q("board")), "{g:?}");
}

/// A reward-free `::accept` of a non-external quest with `follows`: the
/// accept anchor is its only route; `follows` is no route (§7.1).
#[test]
fn follows_is_no_route_beside_an_in_script_accept() {
    let (g, reach, heuristic) = analyse(&format!(
        "{}{}",
        quest("giver", " start=\"true\"", ACCEPT_GIFT),
        quest("gift", " follows=\"completed('nowhere')\"", ""),
    ));
    // `completed('nowhere')` alone would read `Unknown`; the accept proves it.
    assert_eq!(reach[&q("gift")], Reachability::Reachable);
    assert!(heuristic.is_empty());
    assert!(matches!(g.nodes[&q("gift")].follows, FollowsDecl::Valid(_)));
}

/// An accept-driven quest (not external) with `follows` and no in-script
/// source: `follows` is no route (§7.1), so its accept anchor is empty and
/// its reach `Unknown`; the declaration and its edge stay. QReview053 Q1:
/// failing before (no anchor, `follows` read as the activation route,
/// `Reachable`).
#[test]
fn a_source_less_accept_driven_quest_has_no_follows_route() {
    let (g, reach, heuristic) = analyse(&format!(
        "{}{}",
        quest("intro", " start=\"true\" follows=\"\"", ""),
        quest("gift", " follows=\"completed('intro')\"", ""),
    ));
    assert_eq!(reach[&q("gift")], Reachability::Unknown, "{reach:?}");
    assert!(heuristic.is_empty(), "{heuristic:?}");
    let info = &g.nodes[&q("gift")];
    assert_eq!(
        info.follows,
        FollowsDecl::Valid(PrereqFormula::Completed("intro".into()))
    );
    let PrereqState::Anchored(anchors) = &info.prereq else {
        panic!("{g:?}");
    };
    assert_eq!(
        anchors,
        &[Anchor {
            kind: EdgeKind::Accept,
            from: vec![],
            host: None,
        }]
    );
    assert_eq!(
        g.edge_kinds_for(&q("intro"), &q("gift"))
            .map(|k| k.iter().copied().collect::<Vec<_>>()),
        Some(vec![EdgeKind::Completed])
    );
}
