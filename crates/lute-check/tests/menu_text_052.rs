//! dsl 0.52.0 §6.1 (#218): a `<branch prompt>` / `<hub prompt>` takes the
//! label grammar — every pass that scans a choice label's `{{…}}` scans the
//! prompt's, with the same codes, at the prompt's value. §6.7 (#220): an
//! `<objective title>` takes no `{{…}}` (`E-OBJECTIVE-TITLE-INTERP`). On
//! v0.51.0 `prompt="You have {{run.gould}} gold."` and `title="Pay
//! {{run.gold}} gold"` both check clean.

use lute_check::{check, CheckInput, CheckResult, Mode, SchemaImports};
use lute_core_span::{Diagnostic, Severity};
use lute_manifest::provider::ProviderSet;

fn run(text: &str) -> CheckResult {
    let input = CheckInput::new(
        text.to_string(),
        "menu_text_052".into(),
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

/// `run.gold` has a default; `run.maybe` has none (read before any write it
/// is `E-MAYBE-UNSET`); `@cyc` is a def that expands into itself.
const HEAD: &str = "---\nkind: scene\nid: gold\nstate:\n  run.gold: { type: int, default: 5 }\n  \
                    run.maybe: { type: int }\ndefs:\n  cyc: { type: int, cel: \"@cyc + 1\" }\n\
                    ---\n## Shot 1.\n";

fn branch(prompt: &str, label: &str) -> String {
    format!(
        "{HEAD}<branch id=\"pay\" prompt=\"{prompt}\">\n  <choice id=\"yes\" text=\"{label}\">\n    \
         @narrator: Paid.\n  </choice>\n</branch>\n"
    )
}

fn hub(prompt: &str, label: &str) -> String {
    format!(
        "{HEAD}<hub id=\"purse\" prompt=\"{prompt}\">\n  <choice id=\"look\" text=\"{label}\">\n    \
         @narrator: Looked.\n  </choice>\n  <choice id=\"leave\" text=\"Leave\" exit>\n    \
         @narrator: Bye.\n  </choice>\n</hub>\n"
    )
}

fn errors(r: &CheckResult) -> Vec<&Diagnostic> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect()
}

fn codes(r: &CheckResult) -> Vec<String> {
    errors(r).iter().map(|d| d.code.clone()).collect()
}

/// The source text a diagnostic is anchored at.
fn anchored<'a>(text: &'a str, d: &Diagnostic) -> &'a str {
    &text[d.span.byte_start..d.span.byte_end]
}

/// Each faulty interpolation is refused with the same codes in a prompt as
/// in a label, on a `<branch>` and on a `<hub>` — the referent check, definite
/// assignment and def inlining — anchored at the prompt's value.
#[test]
fn a_prompt_interpolation_is_judged_as_a_label_is() {
    for interp in ["{{run.gould}}", "{{run.maybe}}", "{{@cyc}}"] {
        for (shape, build) in [("branch", branch as fn(&str, &str) -> String), ("hub", hub)] {
            let as_label = run(&build("Pay up.", &format!("Pay {interp}")));
            let as_prompt_text = build(&format!("You have {interp}."), "Pay");
            let as_prompt = run(&as_prompt_text);
            assert!(
                !codes(&as_label).is_empty(),
                "{shape} {interp}: {as_label:#?}"
            );
            assert_eq!(
                codes(&as_prompt),
                codes(&as_label),
                "{shape} {interp}: {:#?}",
                as_prompt.diagnostics
            );
            for d in errors(&as_prompt) {
                assert_eq!(
                    anchored(&as_prompt_text, d),
                    format!("You have {interp}."),
                    "{shape} {interp}: {d:?}"
                );
            }
        }
    }
}

/// The issue's prompt: an undeclared path is refused (clean on v0.51.0).
#[test]
fn issue_218_an_undeclared_prompt_path_is_refused() {
    let r = run(&branch(
        "You have {{run.gould}} gold.",
        "Pay {{run.gold}} gold",
    ));
    assert!(!r.ok, "{:#?}", r.diagnostics);
    assert!(
        errors(&r).iter().any(|d| d.message.contains("run.gould")),
        "{:#?}",
        r.diagnostics
    );
    let ok = run(&branch(
        "You have {{run.gold}} gold.",
        "Pay {{run.gold}} gold",
    ));
    assert!(ok.ok, "{:#?}", ok.diagnostics);
}

/// The prompt shows with the menu, before any option's guard: a guard that
/// proves a path does not prove it for the prompt.
#[test]
fn a_prompt_is_read_before_the_option_guards() {
    let text = format!(
        "{HEAD}<branch id=\"pay\" prompt=\"You owe {{{{run.maybe}}}}.\">\n  <choice id=\"yes\" \
         text=\"Pay {{{{run.maybe}}}}\" when=\"isSet(run.maybe)\">\n    @narrator: Paid.\n  \
         </choice>\n  <choice id=\"no\" text=\"No\">\n    @narrator: No.\n  </choice>\n</branch>\n"
    );
    let r = run(&text);
    let unset: Vec<&Diagnostic> = errors(&r)
        .into_iter()
        .filter(|d| d.code == "E-MAYBE-UNSET")
        .collect();
    assert_eq!(unset.len(), 1, "{:#?}", r.diagnostics);
    assert_eq!(anchored(&text, unset[0]), "You owe {{run.maybe}}.");
}

const QUEST: &str = "---\nkind: quest\nstate:\n  run.gold: { type: int, default: 5 }\n  \
                     run.d: { type: bool, default: false }\n---\n";

fn objective(title: &str) -> String {
    format!(
        "{QUEST}<quest id=\"toll\" title=\"Toll {{{{run.gold}}}}\" start=\"true\">\n  <objective \
         id=\"o\" title=\"{title}\" done=\"run.d\"/>\n</quest>\n"
    )
}

/// #220: a marker in an objective title is refused at the title's value,
/// once, naming the marker (clean on v0.51.0).
#[test]
fn issue_220_a_marker_in_an_objective_title_is_refused() {
    let text = objective("Pay {{run.gold}} gold, then {{run.gold}} more");
    let r = run(&text);
    let hits: Vec<&Diagnostic> = errors(&r)
        .into_iter()
        .filter(|d| d.code == "E-OBJECTIVE-TITLE-INTERP")
        .collect();
    assert_eq!(hits.len(), 1, "{:#?}", r.diagnostics);
    assert_eq!(
        anchored(&text, hits[0]),
        "Pay {{run.gold}} gold, then {{run.gold}} more"
    );
    assert_eq!(
        hits[0].message,
        "`<objective id=\"o\">`'s `title` is shown as written — a quest UI does not render \
         `{{run.gold}}` — so it takes no `{{…}}`: write the words as plain text \
         (`title=\"Pay the toll\"`), and show a value that changes in a line"
    );
}

/// An escaped `\{{` and an unterminated `{{` are refused under the same code
/// (not `E-STRING-ESCAPE`); a plain title is clean, and a quest `title` is
/// not the objective's rule.
#[test]
fn every_opener_in_an_objective_title_is_refused_and_plain_titles_are_clean() {
    for (title, marker) in [("Pay \\\\{{x}}", "`{{x}}`"), ("Pay {{x", "`{{x`")] {
        let r = run(&objective(title));
        assert_eq!(
            codes(&r),
            ["E-OBJECTIVE-TITLE-INTERP"],
            "{title}: {:#?}",
            r.diagnostics
        );
        assert!(
            errors(&r)[0].message.contains(marker),
            "{:?}",
            errors(&r)[0]
        );
    }
    let r = run(&objective("Pay the toll"));
    assert!(r.ok, "{:#?}", r.diagnostics);
}

/// A choice label keeps its markers: the title rule does not reach it.
#[test]
fn a_choice_label_marker_is_unaffected_by_the_title_rule() {
    let r = run(&branch("Pay up.", "Pay {{run.gold}} gold"));
    assert!(r.ok, "{:#?}", r.diagnostics);
}
