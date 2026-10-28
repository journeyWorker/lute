//! dsl 0.51.0 §3.4/§3.8: the relational must set at a loop head is the
//! fixed point `meet(entry, back edges)`, iterated, and each probe pass
//! routes jumps inside the region exactly as the final pass does
//! (Review051b F1: a fresh probe-local pending map per pass, nested heads
//! included; nothing published from a probe). Run through the pipeline
//! whole-project `check` runs (per-file `check()` → reachability →
//! `live_assert_sites` → `MaySet` → `compute_must` → `check_fact_guards`).

use std::path::{Path, PathBuf};

use lute_check::connectivity::{
    ambiguous_quest_ids, assemble_graph, check_reachability, live_assert_sites, quest_id_set,
    scene_key_set, unreachable_quest_ids,
};
use lute_check::{
    check, check_fact_guards, compute_must, fold_env, loop_walk_stats, reset_loop_walk_stats,
    stable_seeds, CheckInput, FactEnv, FoldedEnv, MaySet, Mode, ProjectDocs, RootVocab, WalkKind,
};
use lute_core_span::Diagnostic;

fn input_for(text: &str) -> CheckInput {
    CheckInput::new(
        text.to_string(),
        "test".into(),
        Mode::Author,
        lute_check::InputSources::from_parts(
            lute_manifest::core::load_core_snapshot(),
            Default::default(),
            lute_check::SchemaImports::default(),
            Default::default(),
            Default::default(),
        ),
    )
}

/// A scene declaring `met` (run tier) over `mira`, with one shot of `body`.
fn scene(body: &str) -> String {
    format!(
        "---\nkind: scene\nid: loops\nentities:\n  person: {{ members: [mira, theo] }}\n\
         relations:\n  met: {{ args: [person], tier: run }}\n---\n## Shot 1.\n{body}\n"
    )
}

/// The project pass's diagnostics for the one document `text`, with the
/// per-file ones it builds on.
fn guards(text: &str) -> Vec<Diagnostic> {
    let path = PathBuf::from("a.lute");
    let input = input_for(text);
    let (doc, _) = lute_syntax::parse(&input.text);
    let (folded, _, _) = fold_env(&doc, &input);
    let result = check(&input);
    let per_file = result.diagnostics.clone();
    assert!(
        !per_file
            .iter()
            .any(|d| d.code.starts_with("E-RELATION") || d.code.starts_with("E-JUMP")),
        "fixture must resolve: {per_file:?}"
    );
    let results = vec![(path.clone(), result)];
    let docs = ProjectDocs::parse(
        vec![(path.clone(), doc)],
        lute_manifest::core::core_snapshot(),
    );
    let views = docs.views();
    let key_set = scene_key_set(&views);
    let quest_ids = quest_id_set(&views);
    let (graph, _) = assemble_graph(
        &views,
        &key_set,
        &quest_ids,
        &lute_check::raises::RaiseAnchors::default(),
    );
    let lifecycle = unreachable_quest_ids(&views, &results);
    let ambiguous = ambiguous_quest_ids(&views);
    let (reach, _) = check_reachability(&graph, &quest_ids, &ambiguous, &lifecycle);
    let mut vocab = RootVocab::default();
    vocab.add(&folded.env.rel_vocab, &folded.env.domains);
    vocab.note_unreadable_documents(&views);
    let facts = live_assert_sites(&views, &reach, &ambiguous, &lifecycle, &Default::default())
        .into_iter()
        .flat_map(|(_, a)| vocab.asserted_facts(&a));
    let may = MaySet::build(&vocab, facts, &stable_seeds(&views, &vocab));
    let folded_refs: Vec<&FoldedEnv> = vec![&folded];
    let must = compute_must(&views, &folded_refs, &graph, &vocab, &may);
    let env = FactEnv::new(may, must.slots);
    let (path, doc) = &docs.documents()[0];
    assert_eq!(path, Path::new("a.lute"));
    check_fact_guards(path, doc, &folded, &env, &per_file)
}

fn codes(ds: &[Diagnostic]) -> Vec<&str> {
    ds.iter().map(|d| d.code.as_str()).collect()
}

/// A `<match>` whose arm tests `test`.
fn guard(test: &str) -> String {
    format!("<match>\n<when test=\"{test}\">\n@mira: Yes.\n</when>\n<otherwise>\n@mira: No.\n</otherwise>\n</match>")
}

const HOLDS: &str = "holds('met', ['mira'])";
const NOT_HOLDS: &str = "!holds('met', ['mira'])";

/// `::assert{met(mira)}` before a loop whose body opens with `guard` and
/// retracts the fact late: the guard is possible, not guaranteed.
fn retract_late(test: &str) -> String {
    scene(&format!(
        "::assert{{met(mira)}}\n::label{{name=\"top\"}}\n{}\n@theo: Time passes.\n\
         ::retract{{met(mira)}}\n::jump{{to=\"top\" max=\"2\"}}",
        guard(test)
    ))
}

/// The fixed-point mutation test (spec §3.8): fails with the iteration
/// disabled, where the head keeps `met(mira)` from its entry.
#[test]
fn a_retract_late_in_the_body_keeps_the_head_guard_possible() {
    assert_eq!(codes(&guards(&retract_late(HOLDS))), Vec::<&str>::new());
    assert_eq!(codes(&guards(&retract_late(NOT_HOLDS))), Vec::<&str>::new());
    // The control: without the retract the fact is Must at the head, so the
    // guard is guaranteed and its negation dead.
    let kept = |test: &str| {
        scene(&format!(
            "::assert{{met(mira)}}\n::label{{name=\"top\"}}\n{}\n@theo: Time passes.\n\
             ::jump{{to=\"top\" max=\"2\"}}",
            guard(test)
        ))
    };
    assert_eq!(codes(&guards(&kept(HOLDS))), ["W-FACT-GUARANTEED"]);
    assert_eq!(codes(&guards(&kept(NOT_HOLDS))), ["E-ARM-DEAD"]);
}

/// The probe routing regression (Review051b F1): the retract is reached only
/// through an unguarded forward jump inside the loop. A probe that drops
/// pending-label routing (no fall-through after the jump, nothing arriving
/// at `skip`) loses the retract and the back edge and keeps `met(mira)` at
/// the head; so does one that publishes probe states into the final pass's
/// pending map (the final pass then meets them at `skip`, where they only
/// remove, but the head is never re-derived from them).
fn routed(test: &str) -> String {
    format!(
        "::label{{name=\"top\"}}\n{}\n::jump{{to=\"skip\"}}\n@theo: Skipped.\n\
         ::label{{name=\"skip\"}}\n::retract{{met(mira)}}\n::jump{{to=\"top\" max=\"2\"}}",
        guard(test)
    )
}

#[test]
fn a_probe_pass_routes_a_forward_jump_inside_the_loop() {
    for test in [HOLDS, NOT_HOLDS] {
        let text = scene(&format!("::assert{{met(mira)}}\n{}", routed(test)));
        assert_eq!(codes(&guards(&text)), Vec::<&str>::new(), "{test}");
    }
}

/// The same shape inside an inner loop of an outer loop: the inner head's
/// probes run inside each probe pass of the outer head, each with its own
/// map.
#[test]
fn a_nested_probe_pass_routes_a_forward_jump_inside_the_inner_loop() {
    for test in [HOLDS, NOT_HOLDS] {
        let text = scene(&format!(
            "::assert{{met(mira)}}\n::label{{name=\"outer\"}}\n@theo: Again.\n{}\n\
             @theo: Outer.\n::jump{{to=\"outer\" max=\"2\"}}",
            routed(test)
        ));
        assert_eq!(codes(&guards(&text)), Vec::<&str>::new(), "{test}");
    }
}

/// An exit jump's set reaches its label only from the final pass: a fact
/// asserted only on the way out of the loop (between the exit jump and the
/// back edge) is not Must after the loop, and a fact asserted before the
/// loop and never retracted still is.
#[test]
fn an_exit_jump_carries_the_final_pass_set_out_of_the_loop() {
    let text = scene(&format!(
        "::assert{{met(theo)}}\n::label{{name=\"top\"}}\n::jump{{to=\"out\" when=\"true\"}}\n\
         ::assert{{met(mira)}}\n::jump{{to=\"top\" max=\"2\"}}\n::label{{name=\"out\"}}\n{}\n{}",
        guard(HOLDS),
        guard("holds('met', ['theo'])")
    ));
    assert_eq!(codes(&guards(&text)), ["W-FACT-GUARANTEED"]);
}

/// A head is iterated at most `|entry| + 1` times, and a document without a
/// bounded jump iterates nothing.
#[test]
fn the_fact_head_stays_within_its_bound() {
    reset_loop_walk_stats();
    let _ = guards(&retract_late(HOLDS));
    let stats = loop_walk_stats(WalkKind::Facts);
    assert_eq!(
        (stats.heads, stats.probe_passes, stats.widened),
        (1, 2, 0),
        "{stats:?}"
    );
    reset_loop_walk_stats();
    let _ = guards(&scene(&format!(
        "::assert{{met(mira)}}\n::jump{{to=\"l\"}}\n::label{{name=\"l\"}}\n{}",
        guard(HOLDS)
    )));
    assert_eq!(loop_walk_stats(WalkKind::Facts), Default::default());
}

/// dsl 0.53.0 §9.3 (#231): the retract and the back edge in a later section
/// reach the head, so its guard is possible, not guaranteed (failing before:
/// the region stopped at the head's section and `met(mira)` stayed Must).
#[test]
fn a_retract_in_a_later_section_reaches_the_head() {
    let split = |test: &str, retract: &str| {
        scene(&format!(
            "::assert{{met(mira)}}\n::label{{name=\"top\"}}\n{}\n\n## Shot 2.\n\n\
             @theo: Time passes.\n{retract}::jump{{to=\"top\" max=\"2\"}}",
            guard(test)
        ))
    };
    let retract = "::retract{met(mira)}\n";
    assert_eq!(codes(&guards(&split(HOLDS, retract))), Vec::<&str>::new());
    assert_eq!(
        codes(&guards(&split(NOT_HOLDS, retract))),
        Vec::<&str>::new()
    );
    assert_eq!(codes(&guards(&split(HOLDS, ""))), ["W-FACT-GUARANTEED"]);
}

/// Two back edges to one head, the later after a retract in a later
/// section: the head sees the retract (failing with a region that stops at
/// the first back edge).
#[test]
fn a_retract_before_a_second_back_edge_reaches_the_head() {
    let text = scene(&format!(
        "::assert{{met(mira)}}\n::label{{name=\"top\"}}\n{}\n::jump{{to=\"top\" max=\"2\"}}\n\n\
         ## Shot 2.\n\n::retract{{met(mira)}}\n::jump{{to=\"top\" max=\"2\"}}",
        guard(HOLDS)
    ));
    assert_eq!(codes(&guards(&text)), Vec::<&str>::new());
}
