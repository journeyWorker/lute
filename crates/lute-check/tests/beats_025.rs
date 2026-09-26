//! dsl 0.25.0 §2 shared spends (`share`) and §3 bundle beat `after=`: the
//! shape rules (`E-BEAT-ATTR`, `E-CONN-PROFILE`), the project-wide `once`
//! agreement of a key, the presence ladder over a key, and the scenario
//! edge an `after=` draws (with the unanchored hint a `visited()` conjunct
//! of `when` gets instead).

use std::path::PathBuf;

use lute_check::connectivity::{
    assemble_graph, quest_id_set, resolve_nodes, scene_key_set, when_visited_unanchored, EdgeKind,
    NodeId, PrereqState,
};
use lute_check::{
    check, check_project_beats, fold_env, CheckInput, FoldedEnv, Mode, SchemaImports,
};
use lute_core_span::Diagnostic;
use lute_manifest::provider::ProviderSet;

fn input(text: &str) -> CheckInput {
    CheckInput {
        text: text.to_string(),
        uri: "doc".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    }
}

fn beat_attr(text: &str) -> Vec<Diagnostic> {
    check(&input(text))
        .diagnostics
        .into_iter()
        .filter(|d| d.code == "E-BEAT-ATTR")
        .collect()
}

fn lore(body: &str) -> String {
    format!("---\nkind: lore\nid: talks\n---\n{body}")
}

fn scene(id: &str, fm: &str) -> String {
    format!("---\nkind: scene\nid: {id}\n{fm}---\n## Shot 1.\n@n: hi\n")
}

fn project(files: &[(&str, &str)]) -> Vec<(PathBuf, lute_syntax::ast::Document)> {
    files
        .iter()
        .map(|(p, t)| (PathBuf::from(p), lute_syntax::parse(t).0))
        .collect()
}

fn project_beat_diags(files: &[(&str, &str)]) -> Vec<(PathBuf, Diagnostic)> {
    let docs = project(files);
    let foldeds: Vec<FoldedEnv> = files
        .iter()
        .zip(&docs)
        .map(|((_, t), (_, d))| fold_env(d, &input(t)).0)
        .collect();
    let refs: Vec<&FoldedEnv> = foldeds.iter().collect();
    check_project_beats(&docs, &refs, &lute_check::cast::fact_producers(&docs), None)
}

// ── §2 shape ────────────────────────────────────────────────────────────

#[test]
fn share_needs_a_written_spending_once() {
    // Clean: a key beside a written `once` on every beat kind.
    let ok = lore(
        "<beat id=\"radio\" on=\"visit\" once=\"user\" share=\"solWarm\">\n@n: a\n</beat>\n\
         <entry id=\"note\" on=\"visit\" once=\"user\" share=\"solWarm\">\n@n: b\n</entry>\n",
    );
    assert!(beat_attr(&ok).is_empty(), "{:#?}", beat_attr(&ok));
    assert!(beat_attr(&scene("a.one", "on: visit\nonce: user\nshare: solWarm\n")).is_empty());

    // `share` without `once`: a scene's defaulted `run` is not written, and
    // an entry without `once` is repeatable.
    for text in [
        scene("a.one", "on: visit\nshare: solWarm\n"),
        scene("a.one", "on: visit\nonce: false\nshare: solWarm\n"),
        lore("<beat id=\"radio\" on=\"visit\" share=\"solWarm\">\n@n: a\n</beat>\n"),
        lore("<beat id=\"radio\" on=\"visit\" once=\"false\" share=\"solWarm\">\n@n: a\n</beat>\n"),
        lore("<entry id=\"note\" on=\"visit\" share=\"solWarm\">\n@n: b\n</entry>\n"),
    ] {
        let ds = beat_attr(&text);
        assert!(
            ds.iter()
                .any(|d| d.message.contains("`share` `solWarm` without `once`")),
            "{text}\n{ds:#?}"
        );
    }
    // A key is an identifier.
    let bad =
        lore("<beat id=\"radio\" on=\"visit\" once=\"user\" share=\"sol warm\">\n@n: a\n</beat>\n");
    assert!(
        beat_attr(&bad)
            .iter()
            .any(|d| d.message.contains("must be a key")),
        "{:#?}",
        beat_attr(&bad)
    );
    // Scene-only frontmatter: a beat key, so `share:` needs `on:` too.
    let no_on = beat_attr(&scene("a.one", "once: user\nshare: solWarm\n"));
    assert!(
        no_on
            .iter()
            .any(|d| d.message.contains("`share:` without `on:`")),
        "{no_on:#?}"
    );
}

#[test]
fn every_beat_of_a_key_declares_the_same_once() {
    let talks = lore(
        "<beat id=\"radio\" on=\"visit\" once=\"user\" share=\"solWarm\">\n@n: a\n</beat>\n\
         <beat id=\"roof\" on=\"visit\" once=\"run\" share=\"solWarm\">\n@n: b\n</beat>\n",
    );
    let out = project_beat_diags(&[("talks.lute", &talks)]);
    let ds: Vec<&Diagnostic> = out
        .iter()
        .map(|(_, d)| d)
        .filter(|d| d.code == "E-BEAT-ATTR")
        .collect();
    assert_eq!(ds.len(), 1, "{out:#?}");
    assert!(
        ds[0]
            .message
            .contains("beat `talks.roof` shares `solWarm` with beat `talks.radio`"),
        "{}",
        ds[0].message
    );
    // Across documents and kinds, one `once` is one spend.
    let talks =
        lore("<beat id=\"radio\" on=\"visit\" once=\"user\" share=\"solWarm\">\n@n: a\n</beat>\n");
    let roof = scene("sol.roof", "on: visit\nonce: user\nshare: solWarm\n");
    let out = project_beat_diags(&[("talks.lute", &talks), ("roof.lute", &roof)]);
    assert!(out.iter().all(|(_, d)| d.code != "E-BEAT-ATTR"), "{out:#?}");
}

/// A `share` without `once` is reported once, per file — its defaulted
/// `once: run` is not a second, project-wide "differs from the key" error.
#[test]
fn share_without_once_is_not_also_a_once_mismatch() {
    for (two, scene_fm) in [
        (
            "<beat id=\"two\" on=\"visit\" share=\"warm\">",
            "on: visit\nshare: warm\n",
        ),
        (
            "<beat id=\"two\" on=\"visit\" once=\"false\" share=\"warm\">",
            "on: visit\nonce: false\nshare: warm\n",
        ),
    ] {
        let talks = lore(&format!(
            "<beat id=\"one\" on=\"visit\" once=\"user\" share=\"warm\">\n@n: a\n</beat>\n{two}\n@n: b\n</beat>\n\
             <entry id=\"note\" on=\"visit\" share=\"warm\">\n@n: c\n</entry>\n"
        ));
        let roof = scene("sol.roof", scene_fm);
        let out = project_beat_diags(&[("talks.lute", &talks), ("roof.lute", &roof)]);
        assert!(
            out.iter().all(|(_, d)| d.code != "E-BEAT-ATTR"),
            "{two}: {out:#?}"
        );
        assert_eq!(beat_attr(&talks).len(), 2, "{:#?}", beat_attr(&talks));
    }
}

// ── §2 presence ladder ──────────────────────────────────────────────────

/// A beat below an always-eligible shared beat may assume the beat above is
/// spent — by ANY beat of its key, so the ladder is the key's disjunction.
#[test]
fn the_presence_ladder_reads_a_shared_spend_as_the_keys_disjunction() {
    let talks = lore(
        "<beat id=\"radio\" on=\"visit\" priority=\"5\" once=\"user\" share=\"solWarm\">\n@n: a\n</beat>\n\
         <beat id=\"roof\" on=\"dusk\" once=\"user\" share=\"solWarm\">\n@n: b\n</beat>\n\
         <beat id=\"after\" on=\"visit\">\n@n: c\n</beat>\n",
    );
    let docs = project(&[("talks.lute", &talks)]);
    let folded = fold_env(&docs[0].1, &input(&talks)).0;
    let ladder = lute_check::beats::presence_ladder(&docs, &[&folded]);
    let rungs: Vec<&String> = ladder[&PathBuf::from("talks.lute")]
        .values()
        .flatten()
        .collect();
    assert_eq!(
        rungs,
        ["(visited('talks.radio') || visited('talks.roof'))"],
        "{ladder:#?}"
    );
}

// ── §3 bundle beat `after=` ─────────────────────────────────────────────

const WITNESSES: &str = "---\nkind: lore\nid: witnesses\n---\n\
    <beat id=\"maren\" on=\"interview\">\n@n: first\n</beat>\n\
    <beat id=\"marenConfronted\" on=\"interview\" after=\"visited('witnesses.maren')\">\n@n: again\n</beat>\n";

#[test]
fn bundle_after_draws_a_scenario_edge() {
    assert!(
        check(&input(WITNESSES)).ok,
        "{:#?}",
        check(&input(WITNESSES)).diagnostics
    );
    let docs = project(&[("witnesses.lute", WITNESSES)]);
    let (graph, _) = assemble_graph(&docs, &scene_key_set(&docs), &quest_id_set(&docs));
    let from = NodeId::Beat("witnesses.maren".into());
    let to = NodeId::Beat("witnesses.marenConfronted".into());
    assert!(
        graph.edges.get(&from).is_some_and(|t| t.contains(&to)),
        "{:#?}",
        graph.edges
    );
    assert_eq!(
        graph
            .edge_kinds_for(&from, &to)
            .map(|k| k.iter().copied().collect::<Vec<_>>()),
        Some(vec![EdgeKind::Visited])
    );
    assert!(matches!(graph.nodes[&to].prereq, PrereqState::Valid(_)));
    // An `after=` never lists the beat as unanchored.
    assert!(when_visited_unanchored(&docs, &graph).is_empty());
}

#[test]
fn bundle_after_is_checked_like_a_scene_after() {
    let bad = WITNESSES.replace(
        "after=\"visited('witnesses.maren')\"",
        "after=\"!visited('witnesses.maren')\"",
    );
    let r = check(&input(&bad));
    assert!(
        r.diagnostics.iter().any(|d| d.code == "E-CONN-PROFILE"),
        "{:#?}",
        r.diagnostics
    );
    let typo = WITNESSES.replace("witnesses.maren')", "witnesses.marn')");
    let docs = project(&[("witnesses.lute", &typo)]);
    let out = resolve_nodes(&docs, &scene_key_set(&docs), &quest_id_set(&docs));
    assert!(
        out.iter().any(|(_, d)| d.code == "E-CONN-UNKNOWN-NODE"
            && d.message.contains("did you mean `witnesses.maren`")),
        "{out:#?}"
    );
}

/// A `visited()` conjunct of `when` still gates but draws no edge — the
/// beat is listed with the `after` that would draw it.
#[test]
fn a_visited_when_conjunct_is_an_unanchored_hint() {
    let gated = WITNESSES.replace(
        "after=\"visited('witnesses.maren')\"",
        "when=\"visited('witnesses.maren') && true\"",
    );
    let docs = project(&[("witnesses.lute", &gated)]);
    let (graph, _) = assemble_graph(&docs, &scene_key_set(&docs), &quest_id_set(&docs));
    assert!(graph.edges.is_empty(), "{:#?}", graph.edges);
    assert_eq!(
        when_visited_unanchored(&docs, &graph),
        [(
            NodeId::Beat("witnesses.marenConfronted".into()),
            vec!["witnesses.maren".to_string()]
        )]
    );
    // Under `||` a `visited()` is no prerequisite: no hint.
    let either = WITNESSES.replace(
        "after=\"visited('witnesses.maren')\"",
        "when=\"visited('witnesses.maren') || true\"",
    );
    let docs = project(&[("witnesses.lute", &either)]);
    let (graph, _) = assemble_graph(&docs, &scene_key_set(&docs), &quest_id_set(&docs));
    assert!(when_visited_unanchored(&docs, &graph).is_empty());
}
