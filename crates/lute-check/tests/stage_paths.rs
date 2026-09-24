//! dsl 0.22.0 §12 (T1-15, seven F7): `W-STAGE-ABSENT` follows paths through
//! `check()`. Each choice/match arm folds from the stage state at the fork and
//! the arms join at convergence, so a sibling arm's exit never warns in the
//! other arm; a character absent on ANY path into the convergence is not
//! present after it (the fact must-set discipline). A `::bg` auto-hide records
//! the hidden character as exited.

use lute_check::{check, CheckInput, Mode};
use lute_core_span::Diagnostic;
use lute_manifest::core::load_core_snapshot;
use lute_manifest::provider::ProviderSet;

const ENUMS: &str = "enums:\n  action:\n    members: [show, hide]\n    exits: [hide]\n  \
                     anchor:\n    members: [left, center, right]\n    default: center\n";

fn absent(body: &str) -> Vec<Diagnostic> {
    let text = format!(
        "---\nkind: scene\ncharacter: vesna\nseason: 1\nepisode: 1\nstate:\n  \
         run.mood: {{ type: enum, values: [calm, tense], default: calm }}\n{ENUMS}---\n\
         ## Shot 1.\n::auto{{character=\"vesna\" action=\"show\"}}\n{body}\n"
    );
    let input = CheckInput {
        text,
        uri: "t".into(),
        snapshot: load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: Default::default(),
        components: Default::default(),
        defaults: Default::default(),
    };
    check(&input)
        .diagnostics
        .into_iter()
        .filter(|d| d.code == "W-STAGE-ABSENT")
        .collect()
}

const BRANCH_EXIT_ON_A: &str = "<branch id=\"fork\">\n\
     <choice id=\"a\" label=\"A\">\n::auto{character=\"vesna\" action=\"hide\"}\n</choice>\n\
     <choice id=\"b\" label=\"B\">\n@vesna: Still here.\n</choice>\n</branch>";

#[test]
fn an_exit_in_one_choice_arm_does_not_warn_in_its_sibling() {
    let ds = absent(BRANCH_EXIT_ON_A);
    assert!(ds.is_empty(), "{ds:#?}");
}

#[test]
fn an_exit_in_one_match_arm_does_not_warn_in_its_sibling() {
    let ds = absent(
        "<match on=\"run.mood\">\n\
         <when is=\"calm\">\n::auto{character=\"vesna\" action=\"hide\"}\n</when>\n\
         <when is=\"tense\">\n@vesna: Still here.\n</when>\n</match>",
    );
    assert!(ds.is_empty(), "{ds:#?}");
}

/// Present on arm `b`, gone on arm `a`: after the convergence she is not
/// provably on stage, so her line warns (may-absent → warn).
#[test]
fn a_line_after_convergence_warns_when_any_arm_took_the_character_off() {
    let ds = absent(&format!("{BRANCH_EXIT_ON_A}\n@vesna: After."));
    assert_eq!(ds.len(), 1, "{ds:#?}");
    assert_eq!(ds[0].span.line, 26, "the `@vesna: After.` line: {ds:#?}");
}

/// Both arms re-show or keep her: present on every path, so silent.
#[test]
fn a_line_after_convergence_is_silent_when_every_arm_keeps_the_character() {
    let ds = absent(
        "<branch id=\"fork\">\n\
         <choice id=\"a\" label=\"A\">\n::auto{character=\"vesna\" action=\"hide\"}\n\
         ::auto{character=\"vesna\" action=\"show\"}\n</choice>\n\
         <choice id=\"b\" label=\"B\">\n@vesna: Still here.\n</choice>\n</branch>\n\
         @vesna: After.",
    );
    assert!(ds.is_empty(), "{ds:#?}");
}

#[test]
fn a_line_after_a_bg_auto_hide_warns_until_the_character_is_shown_again() {
    let ds = absent("::bg{location=\"cafe\"}\n@vesna: Hello?");
    assert_eq!(ds.len(), 1, "{ds:#?}");
    assert!(
        ds[0].message.contains("auto-hidden by an earlier `::bg`"),
        "{}",
        ds[0].message
    );

    let ds = absent(
        "::bg{location=\"cafe\"}\n::auto{character=\"vesna\" action=\"show\"}\n@vesna: Hello.",
    );
    assert!(ds.is_empty(), "{ds:#?}");
}
