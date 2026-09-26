//! dsl 0.8.0 `::end` — the walk terminator, from the checker's side:
//! `W-CODE-AFTER-END` (unreachable content in the SAME straight-line body),
//! its per-body scoping, and the `<track>`-clip rejection. Mirrors
//! `tests/reachability.rs`'s `run()`/`codes()` harness.

use lute_check::{check, CheckInput, CheckResult, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;

fn run(text: &str) -> CheckResult {
    let input = CheckInput {
        text: text.to_string(),
        uri: "end_directive".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    };
    check(&input)
}

fn codes(text: &str) -> Vec<String> {
    run(text).diagnostics.into_iter().map(|d| d.code).collect()
}

fn count(text: &str, code: &str) -> usize {
    codes(text).iter().filter(|c| c.as_str() == code).count()
}

const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n";

const W: &str = "W-CODE-AFTER-END";

// --- the directive itself is admitted -------------------------------------

#[test]
fn end_is_a_known_core_directive() {
    let out = codes(&format!(
        "{HDR}@narrator: bye\n::end{{reason=\"completed\"}}\n"
    ));
    assert!(out.is_empty(), "a terminating shot must be clean: {out:?}");
}

#[test]
fn end_takes_no_reason() {
    let out = codes(&format!("{HDR}@narrator: bye\n::end\n"));
    assert!(out.is_empty(), "`reason` is optional: {out:?}");
}

#[test]
fn end_rejects_an_undeclared_attr() {
    let out = codes(&format!("{HDR}::end{{because=\"x\"}}\n"));
    assert!(
        out.contains(&"E-UNKNOWN-ATTR".to_string()),
        "`::end` declares only `reason`: {out:?}"
    );
}

// --- W-CODE-AFTER-END: the straight-line rule ------------------------------

#[test]
fn content_after_end_in_the_same_shot_warns_once() {
    let text = format!("{HDR}::end{{reason=\"done\"}}\n@narrator: never\n");
    assert_eq!(count(&text, W), 1, "{:?}", codes(&text));
}

#[test]
fn the_warning_is_a_warning_with_the_spec_message() {
    let text = format!("{HDR}::end\n@narrator: never\n");
    let result = run(&text);
    let d = result
        .diagnostics
        .iter()
        .find(|d| d.code == W)
        .unwrap_or_else(|| panic!("expected {W}: {:?}", result.diagnostics));
    assert_eq!(d.severity, lute_core_span::Severity::Warning);
    assert_eq!(
        d.message,
        "unreachable content after `::end` (the walk terminates here)"
    );
    // Anchored at the FIRST unreachable node, not at the `::end`.
    assert_eq!(
        &text[d.span.byte_start..d.span.byte_end],
        "@narrator: never"
    );
}

#[test]
fn only_the_first_unreachable_node_warns() {
    // Three dead nodes, one mistake -> one diagnostic.
    let text = format!("{HDR}::end\n@narrator: a\n@narrator: b\n::sfx{{sound=\"s\"}}\n");
    assert_eq!(count(&text, W), 1, "{:?}", codes(&text));
}

#[test]
fn end_as_the_last_node_is_clean() {
    let text = format!("{HDR}@narrator: a\n::end\n");
    assert_eq!(count(&text, W), 0, "{:?}", codes(&text));
}

// --- dsl 0.27.0 (round-5 T3-7): a `::next` target is an entry point ------------

/// Spans of every `W-CODE-AFTER-END` in `text`, as the source they cover.
fn dead_starts(text: &str) -> Vec<String> {
    run(text)
        .diagnostics
        .iter()
        .filter(|d| d.code == W)
        .map(|d| text[d.span.byte_start..d.span.byte_end].to_string())
        .collect()
}

#[test]
fn a_jump_target_after_end_is_live_and_what_precedes_it_is_not() {
    // `::next` jumps past the `::end` into `flooded`: from the mark on the
    // body is live again. The line between `::end` and the mark stays dead,
    // and so does the stretch after the second `::end` — an untargeted mark
    // opens nothing.
    let text = format!(
        "{HDR}::next{{to=\"flooded\" when=\"true\"}}\n@narrator: out\n::end\n\
         @narrator: dead one\n::mark{{id=\"flooded\"}}\n@narrator: in\n::end\n\
         ::mark{{id=\"orphan\"}}\n@narrator: dead two\n"
    );
    assert_eq!(
        dead_starts(&text),
        ["@narrator: dead one", "::mark{id=\"orphan\"}"],
        "{:?}",
        codes(&text)
    );
}

#[test]
fn a_targeted_line_id_or_nested_mark_is_an_entry_point() {
    // A line's `id=` is a label like `::mark` (dsl 0.12.0).
    let line =
        format!("{HDR}::next{{to=\"l\" when=\"true\"}}\n::end\n@narrator{{id=\"l\"}}: jumped in\n");
    assert!(dead_starts(&line).is_empty(), "{:?}", codes(&line));
    // A mark inside a later choice body: the jump lands inside that node.
    let nested = format!(
        "{HDR}::next{{to=\"m\" when=\"true\"}}\n::end\n<branch id=\"b\">\n\
         <choice id=\"go\" label=\"Go\">\n::mark{{id=\"m\"}}\n@narrator: jumped in\n</choice>\n\
         </branch>\n"
    );
    assert!(dead_starts(&nested).is_empty(), "{:?}", codes(&nested));
}

// --- scoping: only the IMMEDIATELY enclosing body -------------------------

#[test]
fn a_sibling_choice_body_is_unaffected() {
    let text = format!(
        "{HDR}<branch id=\"b\">\n\
         <choice id=\"stop\" label=\"Stop\">\n::end{{reason=\"quit\"}}\n</choice>\n\
         <choice id=\"go\" label=\"Go\">\n@narrator: still reachable\n</choice>\n\
         </branch>\n"
    );
    assert_eq!(
        count(&text, W),
        0,
        "an `::end` in one choice says nothing about its sibling: {:?}",
        codes(&text)
    );
}

#[test]
fn content_after_the_enclosing_branch_is_unaffected() {
    let text = format!(
        "{HDR}<branch id=\"b\">\n\
         <choice id=\"stop\" label=\"Stop\">\n::end\n</choice>\n\
         </branch>\n\
         @narrator: reachable via the other route\n"
    );
    assert_eq!(
        count(&text, W),
        0,
        "the converge point is a DIFFERENT body: {:?}",
        codes(&text)
    );
}

#[test]
fn content_after_end_inside_one_choice_warns_for_that_choice_only() {
    let text = format!(
        "{HDR}<branch id=\"b\">\n\
         <choice id=\"stop\" label=\"Stop\">\n::end\n@narrator: dead\n</choice>\n\
         <choice id=\"go\" label=\"Go\">\n@narrator: alive\n</choice>\n\
         </branch>\n"
    );
    assert_eq!(count(&text, W), 1, "{:?}", codes(&text));
}

#[test]
fn content_after_end_in_a_when_arm_warns_for_that_arm_only() {
    let hdr = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\nstate:\n  \
        run.rank: { type: { enum: [bronze, gold] }, default: bronze }\n---\n## Shot 1.\n";
    let text = format!(
        "{hdr}<match on=\"run.rank\">\n\
         <when is=\"gold\">\n::end{{reason=\"win\"}}\n@narrator: dead\n</when>\n\
         <when is=\"bronze\">\n@narrator: alive\n</when>\n\
         </match>\n"
    );
    assert_eq!(count(&text, W), 1, "{:?}", codes(&text));
}

#[test]
fn content_after_end_in_an_on_body_warns() {
    let text = "---\nkind: quest\n---\n\
        <quest id=\"q\" title=\"T\" start=\"true\">\n\
        <objective id=\"o\" done=\"true\"/>\n\
        <on event=\"questComplete\">\n::end{reason=\"finale\"}\n@narrator: dead\n</on>\n\
        </quest>\n"
        .to_string();
    assert_eq!(count(&text, W), 1, "{:?}", codes(&text));
}

#[test]
fn a_shot_with_no_end_never_warns() {
    let text = format!("{HDR}@narrator: a\n::sfx{{sound=\"s\"}}\n@narrator: b\n");
    assert_eq!(count(&text, W), 0, "{:?}", codes(&text));
}

// --- `::end` is not a staging leaf ----------------------------------------

#[test]
fn end_inside_a_track_clip_is_rejected() {
    let text = format!("{HDR}<timeline>\n<track subject=\"cam\">\n::end\n</track>\n</timeline>\n");
    let out = codes(&text);
    assert!(
        out.contains(&"E-TIMELINE-CONTENT".to_string()),
        "a <track> clip may not be the walk terminator: {out:?}"
    );
}

#[test]
fn an_ordinary_staging_clip_stays_admitted() {
    let text = format!(
        "{HDR}<timeline>\n<track subject=\"cam\">\n::sfx{{sound=\"s\"}}\n</track>\n</timeline>\n"
    );
    let out = codes(&text);
    assert!(
        !out.contains(&"E-TIMELINE-CONTENT".to_string()),
        "the new guard must not catch ordinary staging: {out:?}"
    );
}
