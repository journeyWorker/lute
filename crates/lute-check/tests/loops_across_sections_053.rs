//! dsl 0.53.0 §9 (#231): a document's top-level `##` sections are one loop
//! scope. A bounded loop may close in a later section; every flow walk
//! iterates the region through its last back edge, across sections; the
//! other loop rules apply unchanged; `E-JUMP-INTO-LOOP` names a block one
//! option alone reaches.

use lute_check::{check, CheckInput, Mode};
use lute_core_span::Diagnostic;
use lute_manifest::schema::{CastMember, OccasionDecl, OccasionSelect};

fn input_for(text: &str, snapshot: lute_manifest::snapshot::CapabilitySnapshot) -> CheckInput {
    CheckInput::new(
        text.to_string(),
        "loops".into(),
        Mode::Author,
        lute_check::InputSources::from_parts(
            snapshot,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        ),
    )
}

const FRONT: &str = "state:\n  run.x: { type: int }\n  run.f: { type: bool, default: false }\n\
                     enums:\n  action:\n    members: [show, hide]\n    exits: [hide]\n  \
                     anchor:\n    members: [left, center, right]\n    default: center\n";

fn scene(body: &str) -> String {
    format!("---\nkind: scene\nid: loops\n{FRONT}---\n## One\n\n{body}\n")
}

fn diags(text: &str) -> Vec<Diagnostic> {
    check(&input_for(text, lute_manifest::core::load_core_snapshot())).diagnostics
}

/// `(code, line)` of every diagnostic.
fn at(ds: &[Diagnostic]) -> Vec<(&str, u32)> {
    ds.iter().map(|d| (d.code.as_str(), d.span.line)).collect()
}

fn line_of(text: &str, needle: &str) -> u32 {
    let at = text.find(needle).expect("needle in fixture");
    u32::try_from(text[..at].matches('\n').count() + 1).expect("a short fixture")
}

// --- the label pass ------------------------------------------------------------

/// The #231 repro on the core snapshot (the plugin `::challenge` is a line
/// here): failing before with `E-JUMP-LOOP-SCOPE` "outside the section
/// `## Retry`".
const RETRY: &str = "@mira: Want to try the ring toss?\n<branch id=\"offer\">\n  \
    <choice id=\"play\" text=\"Play\">\n    ::jump{to=\"play\"}\n  </choice>\n  \
    <choice id=\"skip\" text=\"Not now\">\n    ::jump{to=\"out\"}\n  </choice>\n</branch>\n\n\
    ## Play {#play}\n\n::label{name=\"play\"}\n@mira: Toss!\n\n## Result {#result}\n\n\
    @mira: So close!\n\n## Retry {#retry}\n\n<branch id=\"again\">\n  \
    <choice id=\"retry\" text=\"Try again\">\n    ::jump{to=\"play\" max=\"99\"}\n  </choice>\n  \
    <choice id=\"stop\" text=\"Stop\">\n    ::jump{to=\"out\"}\n  </choice>\n</branch>\n\n\
    ## After {#after}\n\n::label{name=\"out\"}\n@mira: Let's head back.";

#[test]
fn the_retry_over_sections_checks_clean() {
    let text = scene(RETRY);
    assert_eq!(at(&diags(&text)), Vec::<(&str, u32)>::new());
}

#[test]
fn a_loop_across_sections_entered_in_the_middle_is_into_loop() {
    let text = scene(
        "::jump{to=\"mid\" when=\"run.f\"}\n::label{name=\"top\"}\n@mira: Top.\n\n## Two\n\n\
         ::label{name=\"mid\"}\n@mira: Mid.\n::jump{to=\"top\" max=\"2\"}",
    );
    let ds = diags(&text);
    assert_eq!(
        at(&ds),
        [("E-JUMP-INTO-LOOP", line_of(&text, "to=\"mid\""))]
    );
    // Not the last node of a `<choice>` body: no one-option hint.
    assert!(!ds[0].message.contains("reached only"), "{}", ds[0].message);
}

#[test]
fn a_loop_leaving_a_hub_option_is_still_loop_scope() {
    let text = scene(
        "::label{name=\"top\"}\n@mira: Top.\n\n## Two\n\n<hub id=\"h\">\n  \
         <choice id=\"go\" text=\"Go\">\n    ::jump{to=\"top\" max=\"2\"}\n  </choice>\n  \
         <choice id=\"out\" text=\"Out\" exit>\n    @mira: Bye.\n  </choice>\n</hub>",
    );
    let ds = diags(&text);
    assert_eq!(
        at(&ds),
        [("E-JUMP-LOOP-SCOPE", line_of(&text, "to=\"top\""))]
    );
    assert!(
        ds[0].message.contains("the `<hub>` option"),
        "{}",
        ds[0].message
    );
    assert!(!ds[0].message.contains("section"), "{}", ds[0].message);
}

/// A loop back into a nested body the jump is not in names no section.
#[test]
fn a_loop_back_into_a_choice_body_names_no_section() {
    let text = scene(
        "<branch id=\"b\">\n  <choice id=\"c\" text=\"C\">\n    ::label{name=\"top\"}\n    \
         @mira: In.\n  </choice>\n</branch>\n\n## Two\n\n::jump{to=\"top\" max=\"2\"}",
    );
    let ds = diags(&text);
    assert_eq!(
        at(&ds),
        [("E-JUMP-LOOP-SCOPE", line_of(&text, "to=\"top\" max"))]
    );
    assert!(
        ds[0].message.contains("sits in a body this jump is not in"),
        "{}",
        ds[0].message
    );
}

/// dsl 0.53.0 §9.4: a block after the loop's head reached from one option
/// only; the suggested move checks clean.
#[test]
fn into_loop_from_one_option_hints_the_move() {
    let entered = scene(
        "<branch id=\"pick\">\n  <choice id=\"a\" text=\"A\">\n    ::jump{to=\"blockA\"}\n  \
         </choice>\n  <choice id=\"b\" text=\"B\">\n    @mira: B.\n  </choice>\n</branch>\n\
         ::label{name=\"top\"}\n@mira: Top.\n\n## Two\n\n::label{name=\"blockA\"}\n@mira: A.\n\
         ::jump{to=\"top\" max=\"2\"}",
    );
    let ds = diags(&entered);
    assert_eq!(
        at(&ds),
        [("E-JUMP-INTO-LOOP", line_of(&entered, "to=\"blockA\""))]
    );
    assert!(
        ds[0].message.ends_with(
            " — `blockA` is reached only from option `a` of `<branch id=\"pick\">`: move the \
             block into that option's body"
        ),
        "{}",
        ds[0].message
    );
    let moved = scene(
        "<branch id=\"pick\">\n  <choice id=\"a\" text=\"A\">\n    @mira: A.\n  </choice>\n  \
         <choice id=\"b\" text=\"B\">\n    @mira: B.\n  </choice>\n</branch>\n\
         ::label{name=\"top\"}\n@mira: Top.\n\n## Two\n\n::jump{to=\"top\" max=\"2\"}",
    );
    assert_eq!(at(&diags(&moved)), Vec::<(&str, u32)>::new());
    // A second jump to the block: no hint.
    let twice = entered.replace("@mira: B.\n", "::jump{to=\"blockA\"}\n");
    let ds = diags(&twice);
    assert!(
        ds.iter().all(|d| !d.message.contains("reached only")),
        "{ds:?}"
    );
}

// --- the walks: state written in a later section reaches the head ------------

/// Definite assignment: a write in the later section does not prove the
/// head (the first round has none).
#[test]
fn an_assignment_in_a_later_section_reaches_the_head() {
    let text = scene(
        "::label{name=\"top\"}\n@mira: x is {{run.x}}.\n\n## Two\n\n::set{run.x = 1}\n\
         ::jump{to=\"top\" max=\"2\"}\n@mira: now {{run.x}}.",
    );
    assert_eq!(
        at(&diags(&text)),
        [("E-MAYBE-UNSET", line_of(&text, "@mira: x is"))]
    );
}

/// The stage fold: an exit in a later section makes `mira` maybe absent at
/// the head (failing before: the head kept her on stage).
#[test]
fn a_stage_change_in_a_later_section_reaches_the_head() {
    let text = scene(
        "::actor{character=\"mira\"}\n::label{name=\"top\"}\n@mira: Again.\n\n## Two\n\n\
         ::actor{character=\"mira\" action=\"hide\"}\n::jump{to=\"top\" max=\"2\"}",
    );
    assert_eq!(
        at(&diags(&text)),
        [
            ("W-STAGE-ABSENT", line_of(&text, "@mira: Again.")),
            ("W-STAGE-ABSENT", line_of(&text, "action=\"hide\"")),
        ]
    );
}

/// Two back edges to one head, the later one in a later section after an
/// exit: the head sees the exit (failing with a region that stops at the
/// first back edge).
#[test]
fn two_back_edges_reach_the_head_through_the_later_one() {
    let text = scene(
        "::actor{character=\"mira\"}\n::label{name=\"top\"}\n@mira: Again.\n\
         ::jump{to=\"top\" max=\"2\"}\n\n## Two\n\n::set{run.x = 1}\n\
         ::actor{character=\"mira\" action=\"hide\"}\n::jump{to=\"top\" max=\"3\"}",
    );
    let ds = diags(&text);
    assert!(
        at(&ds).contains(&("W-STAGE-ABSENT", line_of(&text, "@mira: Again."))),
        "{ds:?}"
    );
}

/// Presence: a write in a later section kills the guard read at the head.
fn presence_input(text: &str) -> CheckInput {
    let mut snapshot = lute_manifest::core::load_core_snapshot();
    snapshot.cast.insert(
        "corvin".into(),
        CastMember {
            id: "corvin".into(),
            name: None,
            present: Some("run.withUs == true".into()),
            emotions: None,
            assume: None,
            shared_name: None,
        },
    );
    snapshot.occasions.insert(
        "hubVisit".into(),
        OccasionDecl {
            name: "hubVisit".into(),
            select: OccasionSelect::First,
            ..Default::default()
        },
    );
    input_for(text, snapshot)
}

fn presence_scene(write: &str) -> String {
    format!(
        "---\nkind: scene\nid: walk\nstate:\n  run.withUs: {{ type: bool, default: false }}\n  \
         run.x: {{ type: int, default: 0 }}\non: hubVisit\nonce: false\n\
         when: \"run.withUs == true\"\n---\n## One\n\n::label{{name=\"top\"}}\n@corvin: Onward.\n\n\
         ## Two\n\n{write}\n::jump{{to=\"top\" max=\"2\"}}\n"
    )
}

#[test]
fn a_presence_write_in_a_later_section_reaches_the_head() {
    let absent = |text: &str| -> Vec<u32> {
        check(&presence_input(text))
            .diagnostics
            .into_iter()
            .filter(|d| d.code == "W-CAST-ABSENT")
            .map(|d| d.span.line)
            .collect()
    };
    let text = presence_scene("::set{run.withUs = false}");
    assert_eq!(absent(&text), [line_of(&text, "@corvin")]);
    assert_eq!(
        absent(&presence_scene("::set{run.x = 1}")),
        Vec::<u32>::new()
    );
}
