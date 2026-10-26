//! dsl 0.51.0 §3.4/§3.8 through `check()`: definite assignment and the
//! stage fold join the states of the jumps that reach each `::label`, and
//! iterate each loop head. The two forward-jump reproductions (decision
//! note item 4) report on 0.50.0 nothing; both are checked on the 0.50.0
//! build (`$SNAP/lute-0.50.0`).

use lute_check::defassign::{check_definite_assignment, Scope};
use lute_check::{
    check, envelope, fold_env, loop_walk_stats, reset_loop_walk_stats, CheckInput, Mode, WalkKind,
};
use lute_core_span::Diagnostic;

fn input_for(text: &str) -> CheckInput {
    CheckInput::new(
        text.to_string(),
        "flow".into(),
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

const FRONT: &str = "state:\n  run.x: { type: int }\n  run.f: { type: bool, default: false }\n\
                     enums:\n  action:\n    members: [show, hide]\n    exits: [hide]\n  \
                     anchor:\n    members: [left, center, right]\n    default: center\n";

fn scene(body: &str) -> String {
    format!("---\nkind: scene\nid: flow\n{FRONT}---\n## Shot 1.\n{body}\n")
}

fn diags(text: &str) -> Vec<Diagnostic> {
    check(&input_for(text)).diagnostics
}

/// `(code, line)` of every diagnostic.
fn at(ds: &[Diagnostic]) -> Vec<(&str, u32)> {
    ds.iter().map(|d| (d.code.as_str(), d.span.line)).collect()
}

/// The line `needle` is on.
fn line_of(text: &str, needle: &str) -> u32 {
    let at = text.find(needle).expect("needle in fixture");
    u32::try_from(text[..at].matches('\n').count() + 1).expect("a short fixture")
}

/// The guaranteed-write set `G` of `text`'s one shot, as the project
/// envelope computes it.
fn scene_g(text: &str) -> Vec<String> {
    let input = input_for(text);
    let (doc, _) = lute_syntax::parse(&input.text);
    let (folded, _, _) = fold_env(&doc, &input);
    let nodes = &doc.sections[0].body;
    let (_, writes, _) = check_definite_assignment(nodes, &Scope::of(&folded), None);
    let (g, _) = envelope::scene_effects(&writes, nodes);
    g.into_iter().collect()
}

// --- definite assignment: the forward-jump reproduction ----------------------

const SKIP_SET: &str = "::jump{to=\"done\" when=\"run.f\"}\n::set{run.x = 1}\n\
                        ::label{name=\"done\"}\n@narrator: x is {{run.x}}.";

#[test]
fn a_guarded_jump_past_a_set_leaves_the_read_maybe_unset() {
    let text = scene(SKIP_SET);
    assert_eq!(
        at(&diags(&text)),
        [("E-MAYBE-UNSET", line_of(&text, "@narrator"))]
    );
    // The skipped path is no guaranteed write: the envelope's G drops it.
    assert_eq!(scene_g(&text), Vec::<String>::new());
    // Without the jump the write is guaranteed.
    assert_eq!(
        scene_g(&scene("::set{run.x = 1}\n@narrator: x is {{run.x}}.")),
        ["run.x"]
    );
}

/// The completion envelope of a quest whose `questComplete` handler has the
/// same shape (`writesOnComplete`).
#[test]
fn a_guarded_jump_past_a_set_leaves_the_completion_envelope_without_it() {
    let quest = |body: &str| {
        format!(
            "---\nkind: quest\n{FRONT}---\n<quest id=\"q\" tier=\"run\" title=\"Q\" start=\"true\">\n  \
             <objective id=\"o\" title=\"O\" done=\"run.f\"/>\n  \
             <on event=\"questComplete\">\n{body}\n  </on>\n</quest>\n"
        )
    };
    let on_complete = |text: &str| {
        let input = input_for(text);
        let (doc, _) = lute_syntax::parse(&input.text);
        let (folded, _, _) = fold_env(&doc, &input);
        envelope::writes_on_complete(&doc.quests[0], &folded.env.state)
            .into_iter()
            .collect::<Vec<_>>()
    };
    let skipped = quest(
        "::jump{to=\"done\" when=\"run.f\"}\n::set{run.x = 1}\n::label{name=\"done\"}\n\
         @narrator: Done.",
    );
    assert!(check(&input_for(&skipped)).ok, "{:?}", diags(&skipped));
    assert_eq!(on_complete(&skipped), Vec::<String>::new());
    assert_eq!(on_complete(&quest("::set{run.x = 1}")), ["run.x"]);
}

/// An unguarded jump leaves no fall-through: what it skips is unset at its
/// label, and nothing between is reported (no route reaches it).
#[test]
fn an_unguarded_jump_skips_its_writes_and_reports_nothing_it_skips() {
    let text = scene(
        "::jump{to=\"done\"}\n@narrator: {{run.x}}.\n::set{run.x = 1}\n\
         ::label{name=\"done\"}\n@narrator: x is {{run.x}}.",
    );
    let ds = diags(&text);
    let maybe: Vec<u32> = ds
        .iter()
        .filter(|d| d.code == "E-MAYBE-UNSET")
        .map(|d| d.span.line)
        .collect();
    assert_eq!(maybe, [line_of(&text, "@narrator: x is")], "{ds:?}");
}

/// A quest of `body` (its objective `o` done on `run.f`).
fn quest(body: &str) -> String {
    format!(
        "---\nkind: quest\n{FRONT}---\n<quest id=\"q\" tier=\"run\" title=\"Q\" start=\"true\">\n\
         {body}\n</quest>\n"
    )
}

/// S2 review (S2Review051 P1): the engine runs an `<on>` handler and an
/// objective body on their own, so one placed after an unguarded jump
/// still has its reads checked (from the body's entry).
#[test]
fn a_handler_after_an_unguarded_jump_still_checks_its_reads() {
    let text = quest(
        "::jump{to=\"later\"}\n<on event=\"questActive\">\n@narrator: {{run.x}}.\n</on>\n\
         <objective id=\"o\" title=\"O\" done=\"run.f\">\n@narrator: Done {{run.x}}.\n\
         </objective>\n::label{name=\"later\"}",
    );
    let maybe: Vec<u32> = diags(&text)
        .iter()
        .filter(|d| d.code == "E-MAYBE-UNSET")
        .map(|d| d.span.line)
        .collect();
    assert_eq!(
        maybe,
        [
            line_of(&text, "@narrator: {{"),
            line_of(&text, "@narrator: Done")
        ]
    );
}

/// S2 review (S2Review051 P2): a handler that writes and then jumps to a
/// quest-level label leaves its body on that route: the write is in its
/// completion envelope.
#[test]
fn a_handler_jumping_out_keeps_its_writes_in_the_completion_envelope() {
    let text = quest(
        "<objective id=\"o\" title=\"O\" done=\"run.f\"/>\n<on event=\"questComplete\">\n\
         ::set{run.x = 1}\n::jump{to=\"after\"}\n</on>\n::label{name=\"after\"}",
    );
    assert!(check(&input_for(&text)).ok, "{:?}", diags(&text));
    let input = input_for(&text);
    let (doc, _) = lute_syntax::parse(&input.text);
    let (folded, _, _) = fold_env(&doc, &input);
    let on_complete: Vec<String> = envelope::writes_on_complete(&doc.quests[0], &folded.env.state)
        .into_iter()
        .collect();
    assert_eq!(on_complete, ["run.x"]);
}

/// S2 review (S2Review051, stage): a branch whose choices are all guarded
/// (`E-BRANCH-ALL-GUARDED`) may end without a pick; when every choice jumps
/// away, the fold keeps staging past it on that route, so it still reports
/// past the error: `mira` left before the branch, so her line after it is
/// absent on the no-pick route.
#[test]
fn the_no_pick_route_past_choices_that_all_jump_away_keeps_staging() {
    let text = scene(
        "@mira: Hi.\n::actor{character=\"mira\" action=\"hide\"}\n<branch id=\"g\">\n\
         <choice id=\"c\" text=\"Go\" when=\"run.f\">\n::jump{to=\"out\"}\n</choice>\n\
         </branch>\n@mira: Still.\n::label{name=\"out\"}\n@narrator: Out.",
    );
    assert_eq!(
        at(&diags(&text)),
        [
            ("E-BRANCH-ALL-GUARDED", line_of(&text, "<branch")),
            ("W-STAGE-ABSENT", line_of(&text, "@mira: Still.")),
        ]
    );
}

/// S2 re-review (S2FixReview051): a match without `<otherwise>` whose arms
/// cover its subject's domain (`true` and `false` of a defaulted bool) is
/// exhaustive — it has no no-match route. When both arms jump away, nothing
/// reaches the line after it, so no stage warning is reported there.
#[test]
fn an_exhaustive_match_whose_arms_all_jump_away_has_no_no_match_route() {
    let text = scene(
        "@mira: Hi.\n::actor{character=\"mira\" action=\"hide\"}\n<match subject=\"run.f\">\n\
         <when is=\"true\">\n::jump{to=\"a\"}\n</when>\n<when is=\"false\">\n::jump{to=\"b\"}\n\
         </when>\n</match>\n@mira: Never.\n::label{name=\"a\"}\n::label{name=\"b\"}\n\
         @narrator: Out.",
    );
    let ds = diags(&text);
    assert!(
        !ds.iter().any(|d| d.code == "W-STAGE-ABSENT"),
        "{:?}",
        at(&ds)
    );
}

// --- definite assignment: one-pass stability and soundness at a head ------

/// A loop with an exit jump at its head (so the head's own flow reaches the
/// label after the loop), whose body sets `run.x` before the back edge.
fn loop_setting_inside(before: &str) -> String {
    scene(&format!(
        "{before}::label{{name=\"top\"}}\n@narrator: top {{{{run.x}}}}.\n\
         ::jump{{to=\"out\" when=\"run.f\"}}\n::set{{run.x = 1}}\n\
         ::jump{{to=\"top\" max=\"2\"}}\n::label{{name=\"out\"}}\n@narrator: out {{{{run.x}}}}."
    ))
}

#[test]
fn a_set_inside_the_loop_proves_nothing_at_the_head() {
    reset_loop_walk_stats();
    let text = loop_setting_inside("");
    let maybe: Vec<u32> = diags(&text)
        .iter()
        .filter(|d| d.code == "E-MAYBE-UNSET")
        .map(|d| d.span.line)
        .collect();
    // Neither the read at the head nor the one after the loop (reached from
    // the head through the exit jump) is proven: the head's `available` and
    // `writes` hold no `run.x`.
    assert_eq!(
        maybe,
        [
            line_of(&text, "@narrator: top"),
            line_of(&text, "@narrator: out")
        ]
    );
    assert_eq!(scene_g(&text), Vec::<String>::new());
    // One-pass stability: the head state after the first probe pass equals
    // the state it started from (a back edge's flow contains the head's).
    let stats = loop_walk_stats(WalkKind::Assign);
    assert_eq!(
        (stats.heads, stats.probe_passes, stats.max_head_passes),
        (2, 2, 1)
    );
}

#[test]
fn a_set_before_the_loop_proves_the_head_and_the_exit() {
    let text = loop_setting_inside("::set{run.x = 0}\n");
    assert!(
        !diags(&text).iter().any(|d| d.code == "E-MAYBE-UNSET"),
        "{:?}",
        diags(&text)
    );
    assert_eq!(scene_g(&text), ["run.x"]);
}

// --- stage: the forward-jump reproduction -------------------------------------

#[test]
fn a_guarded_jump_past_a_reentrance_warns_at_the_spoken_line() {
    let text = scene(
        "@mira: Hi.\n::actor{character=\"mira\" action=\"hide\"}\n\
         ::jump{to=\"l\" when=\"run.f\"}\n::actor{character=\"mira\"}\n::label{name=\"l\"}\n\
         @mira: Again.",
    );
    let ds = diags(&text);
    assert_eq!(
        at(&ds),
        [("W-STAGE-ABSENT", line_of(&text, "@mira: Again."))]
    );
    // 0.50's hint: the `lower:` form of a re-show.
    assert!(
        ds[0].message.contains("`lower: { record: actor"),
        "{}",
        ds[0].message
    );
}

// --- stage: the loop head ---------------------------------------------------------

#[test]
fn an_exit_late_in_the_loop_warns_at_the_top_of_the_body() {
    reset_loop_walk_stats();
    let text = scene(
        "::actor{character=\"mira\"}\n::label{name=\"top\"}\n@mira: Again.\n\
         ::actor{character=\"mira\" action=\"hide\"}\n::jump{to=\"top\" max=\"2\"}",
    );
    let ds = diags(&text);
    // On a later round she is already off stage at her line and at her exit.
    assert_eq!(
        at(&ds),
        [
            ("W-STAGE-ABSENT", line_of(&text, "@mira: Again.")),
            ("W-STAGE-ABSENT", line_of(&text, "action=\"hide\"")),
        ]
    );
    let stats = loop_walk_stats(WalkKind::Stage);
    assert_eq!((stats.heads, stats.probe_passes, stats.widened), (1, 2, 0));
    // A re-show at the top of the body keeps her on stage on every path.
    let reshown = scene(
        "::actor{character=\"mira\"}\n::label{name=\"top\"}\n::actor{character=\"mira\"}\n\
         @mira: Again.\n::actor{character=\"mira\" action=\"hide\"}\n::jump{to=\"top\" max=\"2\"}",
    );
    assert_eq!(at(&diags(&reshown)), Vec::<(&str, u32)>::new());
}

/// A document without a bounded jump iterates nothing in any walk.
#[test]
fn a_document_without_a_loop_records_no_probe_work() {
    reset_loop_walk_stats();
    let text = scene(SKIP_SET);
    let _ = diags(&text);
    for kind in WalkKind::ALL {
        assert_eq!(loop_walk_stats(kind), Default::default(), "{kind:?}");
    }
}
