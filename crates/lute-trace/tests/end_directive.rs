//! dsl 0.8.0 `::end` in the mock evaluator's walk: an unconditional
//! terminator. The walk stops at the record, nothing after it in the traced
//! path is stepped, and the run is COMPLETE (exit 0) — not the exit-3
//! "halted on an unknown" outcome an `Incomplete` guard produces.
//!
//! Fixture assembly mirrors `tests/walk.rs`'s `input_for` idiom.

use std::collections::BTreeMap;
use std::path::Path;

use lute_check::{CheckInput, Mode};
use lute_trace::{trace_document, MockSet, Step, TraceExit};

fn input_for(text: &str) -> CheckInput {
    let (doc, parse_diags) = lute_syntax::parse(text);
    assert!(parse_diags.is_empty(), "fixture must parse clean: {parse_diags:?}");
    let (meta0, _) =
        lute_check::parse_meta(&doc.meta, &lute_manifest::snapshot::CapabilitySnapshot::default());
    let (snapshot, _) = lute_manifest::project::resolve_document_snapshot(
        None,
        meta0.profile.as_deref(),
        &meta0.plugins,
    );
    let base = Path::new(".");
    CheckInput {
        text: text.to_string(),
        uri: "end_directive".into(),
        snapshot,
        providers: lute_manifest::provider::ProviderSet::default(),
        mode: Mode::Ci,
        imports: lute_check::resolve_imports(base, &meta0.uses, &meta0.extends, doc.meta.span),
        components: lute_check::resolve_components(base, &meta0.components, doc.meta.span),
        defaults: Default::default(),
    }
}

fn choose(id: &str, cids: &[&str]) -> MockSet {
    let mut choose = BTreeMap::new();
    choose.insert(id.to_string(), cids.iter().map(|s| s.to_string()).collect());
    MockSet { choose, ..Default::default() }
}

/// Every spoken line in the transcript, in walk order.
fn lines(steps: &[Step]) -> Vec<&str> {
    steps
        .iter()
        .filter_map(|s| match s {
            Step::Line { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect()
}

fn directives(steps: &[Step]) -> Vec<&str> {
    steps
        .iter()
        .filter_map(|s| match s {
            Step::Directive { tag, .. } => Some(tag.as_str()),
            _ => None,
        })
        .collect()
}

const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n\n## Shot 1.\n\n";

#[test]
fn end_terminates_the_walk_and_the_run_is_complete() {
    let text = format!("{HDR}@narrator: seen\n::end{{reason=\"done\"}}\n@narrator: unreachable\n");
    let (report, exit) = trace_document(&input_for(&text), MockSet::default());
    assert!(matches!(exit, TraceExit::Complete), "an `::end` walk is COMPLETE: {exit:?}");
    assert_eq!(lines(&report.steps), ["seen"]);
    assert_eq!(directives(&report.steps), ["end"], "the terminator IS recorded");
}

#[test]
fn a_later_shot_is_never_entered() {
    let text = format!("{HDR}::end\n\n## Shot 2.\n\n@narrator: unreachable\n");
    let (report, exit) = trace_document(&input_for(&text), MockSet::default());
    assert!(matches!(exit, TraceExit::Complete), "{exit:?}");
    assert!(lines(&report.steps).is_empty(), "{:?}", report.steps);
    // Shot 2's header step is never pushed either — the walk left `walk_document`.
    let shots = report.steps.iter().filter(|s| matches!(s, Step::Shot { .. })).count();
    assert_eq!(shots, 1);
}

#[test]
fn end_inside_a_chosen_branch_arm_terminates_the_whole_walk() {
    let text = format!(
        "{HDR}<branch id=\"exit\">\n\
         <choice id=\"stay\" label=\"Stay\">\n@narrator: staying\n</choice>\n\
         <choice id=\"leave\" label=\"Leave\">\n::end{{reason=\"left\"}}\n</choice>\n\
         </branch>\n\
         @narrator: after the converge\n"
    );
    let (report, exit) = trace_document(&input_for(&text), choose("exit", &["leave"]));
    assert!(matches!(exit, TraceExit::Complete), "{exit:?}");
    assert_eq!(
        lines(&report.steps),
        Vec::<&str>::new(),
        "the converge is downstream of the terminator: {:?}",
        report.steps
    );
    assert_eq!(directives(&report.steps), ["end"]);
}

#[test]
fn the_unterminated_arm_still_reaches_the_converge() {
    let text = format!(
        "{HDR}<branch id=\"exit\">\n\
         <choice id=\"stay\" label=\"Stay\">\n@narrator: staying\n</choice>\n\
         <choice id=\"leave\" label=\"Leave\">\n::end{{reason=\"left\"}}\n</choice>\n\
         </branch>\n\
         @narrator: after the converge\n"
    );
    let (report, exit) = trace_document(&input_for(&text), choose("exit", &["stay"]));
    assert!(matches!(exit, TraceExit::Complete), "{exit:?}");
    assert_eq!(lines(&report.steps), ["staying", "after the converge"]);
    assert!(directives(&report.steps).is_empty(), "the other arm's `::end` never ran");
}

/// A terminator does NOT launder an unresolved atom recorded EARLIER in the
/// walk: `Ended` maps to exit 0 only when nothing went unresolved, exactly
/// like a walk that ran off the last node. Here `slow`'s `start` reads
/// narrative time (no mock surface — unknown), which records unresolved
/// WITHOUT halting; `fast` then terminates the walk.
#[test]
fn a_terminator_does_not_launder_an_unresolved_atom() {
    let text = "---\nkind: quest\n---\n\
        <quest id=\"slow\" title=\"Slow\" start=\"now() > now()\">\n\
        <objective id=\"o\" title=\"O\" done=\"true\"/>\n\
        </quest>\n\
        <quest id=\"fast\" title=\"Fast\" start=\"true\">\n\
        <objective id=\"p\" title=\"P\" done=\"true\"/>\n\
        <on event=\"questActive\">\n::end{reason=\"cut\"}\n</on>\n\
        </quest>\n";
    let (report, exit) = trace_document(&input_for(text), MockSet::default());
    assert!(
        !report.unresolved.is_empty(),
        "fixture must record an unresolved atom: {:?}",
        report.unresolved
    );
    assert_eq!(directives(&report.steps), ["end"], "the terminator ran: {:?}", report.steps);
    assert!(
        matches!(exit, TraceExit::Incomplete),
        "an unresolved atom outranks the terminator's clean exit: {exit:?}"
    );
}
