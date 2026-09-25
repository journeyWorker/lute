//! `lute trace --beat` (dsl 0.23.0 §4): presenting ONE bundle `<beat>` of a
//! lore document — its body walked like a scene shot body (a `<branch>`
//! honours `choose:`, every effect applies), its `when` shown on the head
//! under the canonical `<document id>.<beat id>`, never enforced.
//!
//! Harness mirrors `tests/lore.rs`'s `input_for` idiom.

use std::collections::BTreeMap;
use std::path::Path;

use lute_check::{CheckInput, Mode};
use lute_core_span::Span;
use lute_trace::{
    trace_beat, trace_beat_with_check, trace_entry, MockSet, Step, TraceExit, E_TRACE_BEAT,
};

fn input_for(text: &str) -> CheckInput {
    let (doc, parse_diags) = lute_syntax::parse(text);
    assert!(
        parse_diags.is_empty(),
        "fixture must parse clean: {parse_diags:?}"
    );
    let (meta0, _) = lute_check::parse_meta(
        &doc.meta,
        &lute_manifest::snapshot::CapabilitySnapshot::default(),
    );
    let (snapshot, _) = lute_manifest::project::resolve_document_snapshot(
        None,
        meta0.profile.as_deref(),
        &meta0.plugins,
    );
    let base = Path::new(".");
    CheckInput {
        text: text.to_string(),
        uri: "interviews.lute".to_string(),
        snapshot,
        providers: lute_manifest::provider::ProviderSet::default(),
        mode: Mode::Ci,
        imports: lute_check::resolve_imports(base, &meta0.uses, &meta0.extends, doc.meta.span),
        components: lute_check::resolve_components(base, &meta0.components, doc.meta.span),
        defaults: Default::default(),
    }
}

const BUNDLE: &str = r#"---
kind: lore
id: interviews
state:
  run.porterTrust: { type: number, default: 0 }
---

<entry id="porterNote" target="item.porter_note">
  @narrator: A note about the porter.
</entry>

<beat id="porter" on="talk" target="npc.porter" when="run.porterTrust >= 0">
  @porter: You again.
  <branch id="porterTalk">
    <choice id="ask" label="Ask about the night">
      @porter: I saw nothing.
      ::set{run.porterTrust += 1}
    </choice>
    <choice id="leave" label="Leave">
      @porter: Good.
    </choice>
  </branch>
</beat>
"#;

fn span0() -> Span {
    Span {
        byte_start: 0,
        byte_end: 0,
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    }
}

fn choose(branch: &str, choice: &str) -> MockSet {
    MockSet {
        choose: BTreeMap::from([(branch.to_string(), vec![choice.to_string()])]),
        ..Default::default()
    }
}

fn lines(steps: &[Step]) -> Vec<String> {
    steps
        .iter()
        .filter_map(|s| match s {
            Step::Line { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn bundle_beat_walks_the_chosen_arm_and_applies_its_effects() {
    let input = input_for(BUNDLE);
    let (report, exit) = trace_beat_with_check(
        &input,
        lute_check::check(&input),
        choose("porterTalk", "ask"),
        "porter",
        None,
    );
    assert_eq!(exit, TraceExit::Complete, "{}", report.render_human());
    assert_eq!(
        report.steps[0],
        Step::Beat {
            id: "interviews.porter".into(),
            eligible: Some(true),
            after_unmet: false,
        }
    );
    assert_eq!(lines(&report.steps), vec!["You again.", "I saw nothing."]);
    assert!(
        report.steps.contains(&Step::Set {
            path: "run.porterTrust".into(),
            value: "1".into(),
            sugar: false,
        }),
        "{:?}",
        report.steps
    );
    let decision = &report.decisions[0];
    assert_eq!((decision.id.as_str(), decision.outcome.as_str()), ("porterTalk", "ask"));
    // Coverage counts the beat's choices like a scene's.
    let cov = &report.coverage.choices["porterTalk"];
    assert_eq!((cov.visited, cov.total), (1, 2));
    assert!(
        report.render_human().contains("<beat interviews.porter>"),
        "{}",
        report.render_human()
    );
}

#[test]
fn bundle_beat_accepts_the_canonical_id_and_follows_another_choice() {
    let (report, exit) = trace_beat(
        &input_for(BUNDLE),
        choose("porterTalk", "leave"),
        "interviews.porter",
    );
    assert_eq!(exit, TraceExit::Complete, "{}", report.render_human());
    assert_eq!(lines(&report.steps), vec!["You again.", "Good."]);
    assert!(!report
        .steps
        .iter()
        .any(|s| matches!(s, Step::Set { .. })));
}

#[test]
fn bundle_beat_when_is_shown_not_enforced() {
    let mocks = MockSet {
        state: vec![("run.porterTrust".into(), "-1".into(), span0())],
        ..choose("porterTalk", "leave")
    };
    let (report, exit) = trace_beat(&input_for(BUNDLE), mocks, "porter");
    assert_eq!(exit, TraceExit::Complete, "{}", report.render_human());
    assert_eq!(
        report.steps[0],
        Step::Beat {
            id: "interviews.porter".into(),
            eligible: Some(false),
            after_unmet: false,
        }
    );
    assert_eq!(lines(&report.steps), vec!["You again.", "Good."]);
    assert!(
        report.render_human().contains("not eligible"),
        "{}",
        report.render_human()
    );
}

/// dsl 0.25.0 §3: a bundle beat's `after=` joins its `when` on the head —
/// unmet over the mocked `visited:` it reads as not eligible, and says why.
#[test]
fn bundle_beat_after_is_an_eligibility_conjunct() {
    let src = BUNDLE.replacen(
        "when=\"run.porterTrust >= 0\"",
        "when=\"run.porterTrust >= 0\" after=\"visited('interviews.first')\"",
        1,
    );
    let (report, _) = trace_beat(&input_for(&src), choose("porterTalk", "leave"), "porter");
    assert_eq!(
        report.steps[0],
        Step::Beat {
            id: "interviews.porter".into(),
            eligible: Some(false),
            after_unmet: true,
        }
    );
    assert!(
        report
            .render_human()
            .contains("not eligible: `after` prerequisite not satisfied"),
        "{}",
        report.render_human()
    );
    let visited = MockSet {
        visited: vec!["interviews.first".into()],
        ..choose("porterTalk", "leave")
    };
    let (report, _) = trace_beat(&input_for(&src), visited, "porter");
    assert!(
        matches!(&report.steps[0], Step::Beat { eligible: Some(true), after_unmet: false, .. }),
        "{:?}",
        report.steps[0]
    );
}

#[test]
fn unknown_beat_id_or_non_lore_document_is_e_trace_beat() {
    let (_, exit) = trace_beat(&input_for(BUNDLE), MockSet::default(), "nope");
    let TraceExit::Refused(diags) = exit else {
        panic!("expected a refusal, got {exit:?}");
    };
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code, E_TRACE_BEAT);
    assert!(diags[0].message.contains("interviews.porter"), "{diags:?}");

    // An entry id is not a beat id.
    let (_, exit) = trace_beat(&input_for(BUNDLE), MockSet::default(), "porterNote");
    assert!(
        matches!(&exit, TraceExit::Refused(d) if d[0].code == E_TRACE_BEAT),
        "{exit:?}"
    );

    let scene = "---\nkind: scene\nid: a.b\n---\n\n## One\n@narrator: Hi.\n";
    let (_, exit) = trace_beat(&input_for(scene), MockSet::default(), "porter");
    let TraceExit::Refused(diags) = exit else {
        panic!("expected a refusal, got {exit:?}");
    };
    assert_eq!(diags[0].code, E_TRACE_BEAT);
}

#[test]
fn entry_beside_a_bundle_beat_presents_only_its_own_body() {
    let (report, exit) = trace_entry(&input_for(BUNDLE), MockSet::default(), "porterNote");
    assert_eq!(exit, TraceExit::Complete, "{}", report.render_human());
    assert_eq!(lines(&report.steps), vec!["A note about the porter."]);
}
