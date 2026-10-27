//! dsl 0.52.0 §5.4 (#217) through `check()`: the stage at a hub's options is
//! the hub head — the fixed point of the hub's re-presentation edge — not
//! the hub's entry state. Each test that names v0.51.0 was run on its build
//! (`$SNAP/lute-0.51.0 check`), where it reports nothing.

use lute_check::{check, loop_walk_stats, reset_loop_walk_stats, CheckInput, Mode, WalkKind};
use lute_core_span::Diagnostic;

fn scene(body: &str) -> String {
    format!(
        "---\nkind: scene\nid: desk\nstate:\n  run.f: {{ type: bool, default: false }}\n\
         enums:\n  action:\n    members: [show, hide]\n    exits: [hide]\n  \
         anchor:\n    members: [left, center, right]\n    default: center\n---\n## Shot 1.\n\
         {body}\n"
    )
}

fn diags(text: &str) -> Vec<Diagnostic> {
    let input = CheckInput::new(
        text.to_string(),
        "hub.lute".into(),
        Mode::Author,
        lute_check::InputSources::from_parts(
            lute_manifest::core::load_core_snapshot(),
            Default::default(),
            lute_check::SchemaImports::default(),
            Default::default(),
            Default::default(),
        ),
    );
    let result = check(&input);
    assert!(result.ok, "{:?}", result.diagnostics);
    result.diagnostics
}

/// `(code, line)` of every diagnostic.
fn at(ds: &[Diagnostic]) -> Vec<(&str, u32)> {
    ds.iter().map(|d| (d.code.as_str(), d.span.line)).collect()
}

/// The 1-based line `needle` is on.
fn line_of(text: &str, needle: &str) -> u32 {
    let index = text.lines().position(|l| l.contains(needle)).unwrap();
    u32::try_from(index + 1).unwrap()
}

/// The issue's hub (`send` hides `mira` on a declared exit).
const ISSUE: &str = "@mira: Hi.\n<hub id=\"desk\">\n\
                     <choice id=\"send\" text=\"Send her away\">\n\
                     ::actor{character=\"mira\" action=\"hide\"}\n</choice>\n\
                     <choice id=\"talk\" text=\"Talk\">\n@mira: Still here.\n</choice>\n\
                     <choice id=\"leave\" text=\"Leave\" exit>\n@narrator: Bye.\n</choice>\n</hub>";

/// #217: choosing `send` then `talk` speaks after `mira` left (v0.51.0:
/// clean). On a second `send` her exit does nothing either.
#[test]
fn a_line_after_an_exit_in_another_option_warns() {
    reset_loop_walk_stats();
    let text = scene(ISSUE);
    let ds = diags(&text);
    assert_eq!(
        at(&ds),
        [
            ("W-STAGE-ABSENT", line_of(&text, "action=\"hide\"")),
            ("W-STAGE-ABSENT", line_of(&text, "@mira: Still here.")),
        ]
    );
    // The hub head is a head of the stage fold: it changes once.
    let stats = loop_walk_stats(WalkKind::Stage);
    assert_eq!((stats.heads, stats.probe_passes, stats.widened), (1, 2, 0));
    assert!(stats.probe_nodes > 0);
}

/// A `<return>` body that shows her again keeps `talk` clean: every route
/// back to the options passes it.
#[test]
fn a_return_that_shows_the_character_again_keeps_the_options_clean() {
    let text = scene(&ISSUE.replace(
        "<hub id=\"desk\">\n",
        "<hub id=\"desk\">\n<return>\n::actor{character=\"mira\"}\n</return>\n",
    ));
    assert_eq!(at(&diags(&text)), Vec::<(&str, u32)>::new());
}

/// An exit option's completion leaves the hub: it does not feed the head.
#[test]
fn an_exit_option_does_not_feed_the_head() {
    let text = scene(
        "::actor{character=\"mira\"}\n<hub id=\"desk\">\n\
         <choice id=\"talk\" text=\"Talk\">\n@mira: Still here.\n</choice>\n\
         <choice id=\"leave\" text=\"Leave\" exit>\n\
         ::actor{character=\"mira\" action=\"hide\"}\n</choice>\n</hub>",
    );
    assert_eq!(at(&diags(&text)), Vec::<(&str, u32)>::new());
}

/// Review052 F2: a hub of exit options only never runs its `<return>`, so
/// the `<return>`'s exit reaches neither an option nor the hub's converge
/// (v0.51.0: `W-STAGE-ABSENT` at `After.`, the `<return>` walked from the
/// entry and joined into the converge).
#[test]
fn an_unreachable_return_of_an_exit_only_hub_hides_nobody() {
    reset_loop_walk_stats();
    let text = scene(
        "::actor{character=\"mira\"}\n<hub id=\"desk\">\n\
         <return>\n::actor{character=\"mira\" action=\"hide\"}\n</return>\n\
         <choice id=\"leave\" text=\"Leave\" exit>\n@mira: Bye.\n</choice>\n</hub>\n\
         @mira: After.",
    );
    assert_eq!(at(&diags(&text)), Vec::<(&str, u32)>::new());
    // No back edge: the first pass is stable.
    let stats = loop_walk_stats(WalkKind::Stage);
    assert_eq!((stats.heads, stats.probe_passes), (1, 1));
}

/// Review052 F2: non-exit options that all leave the hub (a leaving jump)
/// or `::end` never complete, so the `<return>` never runs and there is no
/// back edge (v0.51.0: `W-STAGE-ABSENT` at `Bye.`'s converge line `After.`).
#[test]
fn options_that_all_leave_or_end_feed_no_back_edge() {
    reset_loop_walk_stats();
    let text = scene(
        "::actor{character=\"mira\"}\n<hub id=\"desk\">\n\
         <return>\n::actor{character=\"mira\" action=\"hide\"}\n</return>\n\
         <choice id=\"stop\" text=\"Stop\">\n::end{reason=\"done\"}\n</choice>\n\
         <choice id=\"go\" text=\"Go\">\n::jump{to=\"out\"}\n</choice>\n\
         <choice id=\"leave\" text=\"Leave\" exit>\n@mira: Bye.\n</choice>\n</hub>\n\
         @mira: After.\n::label{name=\"out\"}\n@mira: Out.",
    );
    assert_eq!(at(&diags(&text)), Vec::<(&str, u32)>::new());
    let stats = loop_walk_stats(WalkKind::Stage);
    assert_eq!((stats.heads, stats.probe_passes), (1, 1));
}

/// Nested hubs: an exit inside an inner hub's non-exit option reaches the
/// inner hub's options again and, once the inner hub is left, the outer
/// hub's other options (v0.51.0: clean).
#[test]
fn an_exit_in_a_nested_hub_reaches_the_outer_options() {
    let text = scene(
        "::actor{character=\"mira\"}\n<hub id=\"outer\">\n\
         <choice id=\"enter\" text=\"In\">\n<hub id=\"inner\">\n\
         <choice id=\"send\" text=\"Send\">\n::actor{character=\"mira\" action=\"hide\"}\n\
         </choice>\n\
         <choice id=\"back\" text=\"Back\" exit>\n@narrator: Back.\n</choice>\n</hub>\n\
         </choice>\n\
         <choice id=\"talk\" text=\"Talk\">\n@mira: Still here.\n</choice>\n\
         <choice id=\"leave\" text=\"Leave\" exit>\n@narrator: Bye.\n</choice>\n</hub>",
    );
    assert_eq!(
        at(&diags(&text)),
        [
            ("W-STAGE-ABSENT", line_of(&text, "action=\"hide\"")),
            ("W-STAGE-ABSENT", line_of(&text, "@mira: Still here.")),
        ]
    );
}

/// A hub inside a 0.51 loop region: the loop's back edge carries the hub's
/// converge, and each probe pass of the loop iterates the hub head.
#[test]
fn a_hub_inside_a_loop_region() {
    reset_loop_walk_stats();
    let text = scene(
        "::actor{character=\"mira\"}\n::label{name=\"top\"}\n@mira: Again.\n\
         <hub id=\"desk\">\n<choice id=\"talk\" text=\"Talk\">\n@narrator: Talk.\n</choice>\n\
         <choice id=\"leave\" text=\"Leave\" exit>\n\
         ::actor{character=\"mira\" action=\"hide\"}\n</choice>\n</hub>\n\
         ::jump{to=\"top\" max=\"2\"}",
    );
    assert_eq!(
        at(&diags(&text)),
        [
            ("W-STAGE-ABSENT", line_of(&text, "@mira: Again.")),
            ("W-STAGE-ABSENT", line_of(&text, "action=\"hide\"")),
        ]
    );
    let stats = loop_walk_stats(WalkKind::Stage);
    // The loop head (2 passes) and the hub head once per loop probe pass
    // and once in the final pass.
    assert_eq!((stats.heads, stats.widened), (4, 0));
}
