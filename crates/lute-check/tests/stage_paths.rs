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

/// seven F7 (dsl 0.23.1): shown in only SOME arms of a `<match>`, a
/// character is hidden at the next `::bg` on the paths where she is on stage,
/// so her later line without a re-show warns.
#[test]
fn a_character_shown_in_some_match_arms_is_hidden_at_the_next_bg() {
    let body = "::auto{character=\"vesna\" action=\"hide\"}\n\
                <match on=\"run.mood\">\n\
                <when is=\"calm\">\n::auto{character=\"pell\" action=\"show\"}\n@pell: Here.\n</when>\n\
                <when is=\"tense\">\n@narrator: Nobody.\n</when>\n</match>\n\
                ::bg{location=\"slipway\"}\n@pell: Still here?";
    let ds = absent(body);
    assert_eq!(ds.len(), 1, "{ds:#?}");
    assert!(
        ds[0]
            .message
            .starts_with("`pell` was auto-hidden by an earlier `::bg`"),
        "{}",
        ds[0].message
    );
    // Re-shown after the cut: silent.
    let ds = absent(&format!(
        "{}\n::auto{{character=\"pell\" action=\"show\"}}\n@pell: Back.",
        body.trim_end_matches("\n@pell: Still here?")
    ));
    assert!(ds.is_empty(), "{ds:#?}");
}

/// lamplight N15: an exit of a character a `::bg` already hid does nothing,
/// and the warning says so and names the fix — not "stages someone".
#[test]
fn an_exit_after_a_bg_auto_hide_is_named_redundant() {
    let ds = absent("::bg{location=\"cafe\"}\n::auto{character=\"vesna\" action=\"hide\"}");
    assert_eq!(ds.len(), 1, "{ds:#?}");
    let m = &ds[0].message;
    assert!(
        m.starts_with("`vesna` is already off stage (hidden by the `::bg` at line 18)"),
        "{m}"
    );
    assert!(
        m.contains("this exit does nothing. Move it before the `::bg`, or delete it"),
        "{m}"
    );
    assert!(!m.contains("stages"), "{m}");
}

/// dsl 0.24.0 §4: `::clear` takes every character on stage off it, so a later
/// line by any of them without a re-show warns and names the `::clear`.
#[test]
fn a_line_after_a_clear_warns_naming_the_clear() {
    let ds = absent(
        "::auto{character=\"pell\" action=\"show\"}\n@pell: Both here.\n::clear\n\
         @vesna: Hello?\n@pell: Anyone?",
    );
    assert_eq!(ds.len(), 2, "{ds:#?}");
    for (d, who) in ds.iter().zip(["vesna", "pell"]) {
        assert!(
            d.message.starts_with(&format!(
                "`{who}` was taken off stage by an earlier `::clear` (line 20)"
            )),
            "{}",
            d.message
        );
        assert!(d.message.contains("after the `::clear`"), "{}", d.message);
    }
}

/// A character shown again after the `::clear` is on stage: silent. The
/// other one, not re-shown, still warns.
#[test]
fn a_reshow_after_a_clear_is_clean() {
    let ds = absent("::clear\n::auto{character=\"vesna\" action=\"show\"}\n@vesna: Back.");
    assert!(ds.is_empty(), "{ds:#?}");
}

/// Shown in only some match arms, a character still leaves at the `::clear`,
/// as at a `::bg`.
#[test]
fn a_clear_takes_off_a_character_on_stage_on_some_paths() {
    let body = "::auto{character=\"vesna\" action=\"hide\"}\n\
                <match on=\"run.mood\">\n\
                <when is=\"calm\">\n::auto{character=\"pell\" action=\"show\"}\n@pell: Here.\n</when>\n\
                <when is=\"tense\">\n@narrator: Nobody.\n</when>\n</match>\n\
                ::clear\n@pell: Still here?";
    let ds = absent(body);
    assert_eq!(ds.len(), 1, "{ds:#?}");
    assert!(
        ds[0]
            .message
            .starts_with("`pell` was taken off stage by an earlier `::clear`"),
        "{}",
        ds[0].message
    );
}

/// An exit after a `::clear` does nothing, and the warning says so.
#[test]
fn an_exit_after_a_clear_is_named_redundant() {
    let ds = absent("::clear\n::auto{character=\"vesna\" action=\"hide\"}");
    assert_eq!(ds.len(), 1, "{ds:#?}");
    let m = &ds[0].message;
    assert!(
        m.starts_with("`vesna` is already off stage (taken off by the `::clear` at line 18)"),
        "{m}"
    );
    assert!(
        m.contains("Move it before the `::clear`, or delete it"),
        "{m}"
    );
}
