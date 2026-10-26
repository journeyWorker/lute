//! dsl 0.51.0 §3.1–§3.2, §3.8 (#159): bounded backward jumps through the
//! full `check()` pipeline — the rule items: the #159 retry repro, each
//! structural rule positive and negative, each `E-JUMP-LOOP-SCOPE` boundary,
//! the `E-JUMP-MAX` cases, `max="two"` as `E-ATTR-TYPE` only, and no
//! `W-CODE-AFTER-JUMP` after a bounded jump.
//!
//! On 0.50.0 the retry repro is `E-JUMP-BACKWARD` plus `E-UNKNOWN-ATTR`
//! ("`::jump` has no attribute `max`"), checked on the 0.50.0 build.

use lute_check::{check, CheckInput, CheckResult, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;

fn run(text: &str) -> CheckResult {
    let input = CheckInput::new(
        text.to_string(),
        "bounded_jump_051".into(),
        Mode::Author,
        lute_check::InputSources::from_parts(
            lute_manifest::core::load_core_snapshot(),
            ProviderSet::default(),
            SchemaImports::default(),
            Default::default(),
            Default::default(),
        ),
    );
    check(&input)
}

fn codes(text: &str) -> Vec<String> {
    run(text).diagnostics.into_iter().map(|d| d.code).collect()
}

/// The one diagnostic `text` has, which must carry `code`; its message.
fn only(text: &str, code: &str) -> String {
    let diags = run(text).diagnostics;
    let [d] = diags.as_slice() else {
        panic!("one diagnostic for {text}: {diags:?}")
    };
    assert_eq!(d.code, code, "{text}");
    d.message.clone()
}

const SCENE: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n";

fn scene(body: &str) -> String {
    format!("{SCENE}{body}")
}

fn quest(body: &str) -> String {
    format!(
        "---\nkind: quest\nstate:\n  run.d: {{ type: bool, default: false }}\n---\n\
         <quest id=\"q\" tier=\"run\">\n{body}</quest>\n"
    )
}

/// The #159 reproduction: a retry menu that loops back to its prompt.
#[test]
fn the_retry_menu_with_max_checks_clean() {
    let retry = |jump: &str| {
        scene(&format!(
            "::label{{name=\"again\"}}\n@mira: Ready to try?\n<branch id=\"retry\" \
             prompt=\"Again?\">\n  <choice id=\"yes\" text=\"Yes\">\n    @theo: Here we go.\n    \
             {jump}\n  </choice>\n  <choice id=\"no\" text=\"No\">\n    @theo: Fine.\n  \
             </choice>\n</branch>\n::end\n"
        ))
    };
    assert_eq!(
        codes(&retry("::jump{to=\"again\" max=\"3\"}")),
        Vec::<String>::new()
    );
    let backward = only(&retry("::jump{to=\"again\"}"), "E-JUMP-BACKWARD");
    assert!(
        backward.contains(
            "`::jump` only jumps forward unless it carries `max`: `::jump{to=\"again\" \
             max=\"3\"}` repeats at most 3 times; to offer choices again, use a `<hub>`"
        ),
        "{backward}"
    );
}

#[test]
fn max_out_of_range_or_a_ref_is_e_jump_max_only() {
    let loop_with = |max: &str| {
        scene(&format!(
            "::label{{name=\"a\"}}\n@mira: hi\n::jump{{to=\"a\" max={max}}}\n@theo: after\n"
        ))
    };
    for bad in ["\"0\"", "\"100\"", "\"-1\""] {
        only(&loop_with(bad), "E-JUMP-MAX");
    }
    // A constant def folds to a literal in every other attribute; a loop bound
    // is still refused as an `@ref`.
    let with_def =
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\ndefs:\n  tries: { type: \
                    int, cel: \"3\" }\n---\n## Shot 1.\n::label{name=\"a\"}\n@mira: hi\n\
                    ::jump{to=\"a\" max=@tries}\n@theo: after\n";
    only(with_def, "E-JUMP-MAX");
    // Not an integer: the declared `int`'s `E-ATTR-TYPE`, and nothing else.
    only(&loop_with("\"two\""), "E-ATTR-TYPE");
    // The bounds themselves are legal.
    assert!(codes(&loop_with("\"1\"")).is_empty());
    assert!(codes(&loop_with("\"99\"")).is_empty());
    // `max` on a forward jump means nothing.
    let forward = only(
        &scene("::jump{to=\"b\" max=\"2\"}\n@mira: hi\n::label{name=\"b\"}\n@theo: after\n"),
        "E-JUMP-MAX",
    );
    assert!(forward.contains("already goes forward"), "{forward}");
}

/// `W-CODE-AFTER-JUMP`: none after a bounded jump (its exhaustion falls
/// through), unchanged after an unguarded forward jump.
#[test]
fn no_code_after_jump_warning_after_a_bounded_jump() {
    assert!(codes(&scene(
        "::label{name=\"a\"}\n@mira: hi\n::jump{to=\"a\" max=\"2\"}\n@theo: after\n"
    ))
    .is_empty());
    only(
        &scene("::jump{to=\"b\"}\n@mira: skipped\n::label{name=\"b\"}\n@theo: after\n"),
        "W-CODE-AFTER-JUMP",
    );
}

#[test]
fn a_loop_may_not_leave_a_hub_option_or_return_body() {
    let option = only(
        &scene(
            "::label{name=\"attempt\"}\n<hub id=\"h\">\n  <choice id=\"go\" text=\"Go\">\n    \
             ::jump{to=\"attempt\" max=\"2\"}\n  </choice>\n  <choice id=\"out\" text=\"Out\" \
             exit>\n    @a: bye\n  </choice>\n</hub>\n",
        ),
        "E-JUMP-LOOP-SCOPE",
    );
    assert!(
        option.contains("outside the `<hub>` option it is in"),
        "{option}"
    );
    let ret = only(
        &scene(
            "::label{name=\"attempt\"}\n<hub id=\"h\">\n  <choice id=\"out\" text=\"Out\" exit>\n    \
             @a: bye\n  </choice>\n  <return>\n    ::jump{to=\"attempt\" max=\"2\"}\n  \
             </return>\n</hub>\n",
        ),
        "E-JUMP-LOOP-SCOPE",
    );
    assert!(ret.contains("outside the `<hub>` `<return>` body"), "{ret}");
    // Inside one option body: legal.
    assert!(codes(&scene(
        "<hub id=\"h\">\n  <choice id=\"go\" text=\"Go\">\n    ::label{name=\"attempt\"}\n    \
         @a: try\n    ::jump{to=\"attempt\" max=\"2\"}\n  </choice>\n  <choice id=\"out\" \
         text=\"Out\" exit>\n    @a: bye\n  </choice>\n</hub>\n"
    ))
    .is_empty());
}

#[test]
fn a_loop_may_not_leave_its_section() {
    let m = only(
        &scene("::label{name=\"a\"}\n@a: hi\n\n## Two.\n\n@a: there\n::jump{to=\"a\" max=\"2\"}\n"),
        "E-JUMP-LOOP-SCOPE",
    );
    assert!(m.contains("outside the section `## Two.`"), "{m}");
}

#[test]
fn a_quest_loop_stays_out_of_and_off_its_on_and_objective_bodies() {
    // Spanning an `<objective>` at the quest's top level (R4).
    let over = only(
        &quest(
            "::label{name=\"top\"}\n<objective id=\"o\" done=\"run.d\">\n  @a: go\n\
             </objective>\n::jump{to=\"top\" max=\"2\"}\n",
        ),
        "E-JUMP-LOOP-SCOPE",
    );
    assert_eq!(
        over,
        "`::jump{to=\"top\" max=\"2\"}` loops back over `<objective id=\"o\">`, which runs on its \
         own; put the loop inside the objective's body or before it"
    );
    // Spanning an `<on>`.
    let on = only(
        &quest(
            "::label{name=\"top\"}\n<on event=\"questComplete\">\n  @a: done\n</on>\n\
             ::jump{to=\"top\" max=\"2\"}\n",
        ),
        "E-JUMP-LOOP-SCOPE",
    );
    assert!(
        on.contains("loops back over `<on event=\"questComplete\">`"),
        "{on}"
    );
    assert!(on.contains("inside the handler's body"), "{on}");
    // Leaving an objective body for a label at the quest's top level.
    let leave = only(
        &quest(
            "::label{name=\"top\"}\n<objective id=\"o\" done=\"run.d\">\n  @a: go\n  \
             ::jump{to=\"top\" max=\"2\"}\n</objective>\n",
        ),
        "E-JUMP-LOOP-SCOPE",
    );
    assert!(
        leave.contains("outside `<objective id=\"o\">` it is in"),
        "{leave}"
    );
    // Leaving an `<on>` body.
    let handler = only(
        &quest(
            "::label{name=\"top\"}\n<on event=\"questComplete\">\n  @a: done\n  \
             ::jump{to=\"top\" max=\"2\"}\n</on>\n",
        ),
        "E-JUMP-LOOP-SCOPE",
    );
    assert!(
        handler.contains("outside the `<on event=\"questComplete\">` handler"),
        "{handler}"
    );
    // A loop inside an objective's body, or inside a handler's: legal.
    for body in [
        "<objective id=\"o\" done=\"run.d\">\n  ::label{name=\"top\"}\n  @a: go\n  \
         ::jump{to=\"top\" max=\"2\"}\n</objective>\n",
        "<on event=\"questComplete\">\n  ::label{name=\"top\"}\n  @a: go\n  \
         ::jump{to=\"top\" max=\"2\"}\n</on>\n",
    ] {
        assert!(codes(&quest(body)).is_empty(), "{body}");
    }
}

/// An entry body admits no `::label`/`::jump` at all (it is looked up, not
/// played); a bundle beat does, and is its own range.
#[test]
fn a_loop_may_not_leave_its_beat() {
    let lore = |body: &str| format!("---\nkind: lore\nid: records\n---\n{body}");
    let beat = only(
        &lore(
            "<beat id=\"first\" on=\"talk\">\n::label{name=\"a\"}\n@n: one\n</beat>\n\
             <beat id=\"second\" on=\"talk\">\n@n: two\n::jump{to=\"a\" max=\"2\"}\n</beat>\n",
        ),
        "E-JUMP-LOOP-SCOPE",
    );
    assert!(
        beat.contains("outside `<beat id=\"second\">` it is in"),
        "{beat}"
    );
}

#[test]
fn loops_nest_and_are_entered_only_at_their_head() {
    // Nested and same-head loops, an exit jump and a forward jump to a head.
    let nested: Vec<String> = codes(&scene(
        "::jump{to=\"outer\"}\n@a: skipped\n::label{name=\"outer\"}\n@a: one\n\
         ::label{name=\"inner\"}\n<branch id=\"b\">\n  <choice id=\"leave\" text=\"Leave\">\n    \
         ::jump{to=\"out\"}\n  </choice>\n  <choice id=\"stay\" text=\"Stay\">\n    @a: ok\n  \
         </choice>\n</branch>\n::jump{to=\"inner\" max=\"2\"}\n\
         ::jump{to=\"inner\" max=\"3\"}\n::jump{to=\"outer\" max=\"2\"}\n\
         ::label{name=\"out\"}\n@a: done\n",
    ))
    .into_iter()
    .filter(|c| c.starts_with("E-"))
    .collect();
    assert!(nested.is_empty(), "{nested:?}");
    only(
        &scene(
            "::label{name=\"a\"}\n@x: one\n::label{name=\"b\"}\n@x: two\n\
             ::jump{to=\"a\" max=\"2\"}\n@x: three\n::jump{to=\"b\" max=\"2\"}\n",
        ),
        "E-JUMP-LOOP-CROSS",
    );
    only(
        &scene(
            "::jump{to=\"mid\"}\n::label{name=\"attempt\"}\n@x: one\n::label{name=\"mid\"}\n\
             @x: two\n::jump{to=\"attempt\" max=\"2\"}\n",
        ),
        "E-JUMP-INTO-LOOP",
    );
}
