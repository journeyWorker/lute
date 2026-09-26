//! `lute trace --entry` (dsl 0.19.0 §8, `docs/runtime/lore-entries.md`):
//! presenting ONE `<entry>` of a lore document against mocked state —
//! first-read effects apply only while `entry.<id>.read` is false, a re-read
//! reports them skipped, and `<match>` picks its arm from live state.
//!
//! Harness mirrors `tests/quest.rs`'s `input_for` idiom.

use std::path::Path;

use lute_check::{CheckInput, Mode};
use lute_core_span::Span;
use lute_trace::{trace_document, trace_entry, MockSet, Step, TraceExit, E_TRACE_ENTRY};

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
        uri: "lore.lute".to_string(),
        snapshot,
        providers: lute_manifest::provider::ProviderSet::default(),
        mode: Mode::Ci,
        imports: lute_check::resolve_imports(base, &meta0.uses, &meta0.extends, doc.meta.span),
        components: lute_check::resolve_components(base, &meta0.components, doc.meta.span),
        defaults: Default::default(),
    }
}

const LORE: &str = r#"---
kind: lore
entities:
  crew: { members: [vesna, scientist] }
  topic: { members: [project_lumen] }
relations:
  knows: { args: [crew, topic], tier: run }
state:
  run.labBurned: { type: bool, default: false }
  run.logsRead: { type: number, default: 0 }
---

<entry id="scientistLog1" target="item.torn_note_1" category="note">
  @scientist: Day three. Subject E does not respond to light.
  ::assert{knows(vesna, project_lumen)}
  ::set{run.logsRead += 1}
</entry>

<entry id="rustyKey" target="item.rusty_key" category="item" when="entry.scientistLog1.read">
  <match on="run.labBurned">
    <when is="true">
      @narrator: A scorched key.
    </when>
    <otherwise>
      @narrator: A rusty key.
    </otherwise>
  </match>
</entry>
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

fn state(pairs: &[(&str, &str)]) -> MockSet {
    MockSet {
        state: pairs
            .iter()
            .map(|(p, v)| (p.to_string(), v.to_string(), span0()))
            .collect(),
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
fn first_read_applies_assert_and_set() {
    let (report, exit) = trace_entry(&input_for(LORE), MockSet::default(), "scientistLog1");
    assert_eq!(exit, TraceExit::Complete);
    assert_eq!(
        report.steps,
        vec![
            Step::Entry {
                id: "scientistLog1".into(),
                first_read: true,
                eligible: Some(true),
                spent: None,
            },
            Step::Line {
                speaker: "scientist".into(),
                text: "Day three. Subject E does not respond to light.".into(),
            },
            Step::Assert {
                text: "knows(vesna, project_lumen)".into(),
            },
            Step::Set {
                path: "run.logsRead".into(),
                value: "1".into(),
                sugar: false,
            },
        ]
    );
}

#[test]
fn re_read_shows_the_text_and_skips_every_effect() {
    let (report, exit) = trace_entry(
        &input_for(LORE),
        state(&[("entry.scientistLog1.read", "true")]),
        "scientistLog1",
    );
    assert_eq!(exit, TraceExit::Complete);
    assert_eq!(
        report.steps,
        vec![
            Step::Entry {
                id: "scientistLog1".into(),
                first_read: false,
                eligible: Some(true),
                spent: None,
            },
            Step::Line {
                speaker: "scientist".into(),
                text: "Day three. Subject E does not respond to light.".into(),
            },
            Step::Skipped {
                effect: "assert".into(),
                text: "knows(vesna, project_lumen)".into(),
            },
            Step::Skipped {
                effect: "set".into(),
                text: "run.logsRead += 1".into(),
            },
        ]
    );
}

#[test]
fn match_arm_follows_the_mocked_state_and_when_gates_eligibility() {
    let input = input_for(LORE);
    // Before the fire, and before the log is read: the `otherwise` arm, and
    // the entry is shown as NOT eligible (`when` false) — shown, not enforced.
    let (report, exit) = trace_entry(&input, MockSet::default(), "rustyKey");
    assert_eq!(exit, TraceExit::Complete);
    assert!(matches!(
        report.steps[0],
        Step::Entry {
            first_read: true,
            eligible: Some(false),
            ..
        }
    ));
    assert_eq!(lines(&report.steps), vec!["A rusty key."]);
    assert_eq!(report.decisions[0].outcome, "otherwise");

    // After the fire, once the log was read: arm 1, eligible.
    let (report, _) = trace_entry(
        &input,
        state(&[
            ("run.labBurned", "true"),
            ("entry.scientistLog1.read", "true"),
        ]),
        "rustyKey",
    );
    assert!(matches!(
        report.steps[0],
        Step::Entry {
            eligible: Some(true),
            ..
        }
    ));
    assert_eq!(lines(&report.steps), vec!["A scorched key."]);
    assert_eq!(report.decisions[0].outcome, "arm 1");
}

#[test]
fn entry_read_mock_outside_its_bool_domain_is_refused() {
    let (_, exit) = trace_entry(
        &input_for(LORE),
        state(&[("entry.scientistLog1.read", "yes")]),
        "scientistLog1",
    );
    let TraceExit::Refused(diags) = exit else {
        panic!("expected a refusal, got {exit:?}");
    };
    assert_eq!(diags[0].code, "E-TRACE-MOCK-TYPE", "{diags:?}");
}

#[test]
fn unknown_entry_id_or_non_lore_document_is_e_trace_entry() {
    let (_, exit) = trace_entry(&input_for(LORE), MockSet::default(), "nope");
    let TraceExit::Refused(diags) = exit else {
        panic!("expected a refusal, got {exit:?}");
    };
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].code, E_TRACE_ENTRY);
    assert!(
        diags[0].message.contains("scientistLog1, rustyKey"),
        "{diags:?}"
    );

    let scene = "---\nkind: scene\nid: a.b\n---\n\n## One\n@narrator: Hi.\n";
    let (_, exit) = trace_entry(&input_for(scene), MockSet::default(), "scientistLog1");
    let TraceExit::Refused(diags) = exit else {
        panic!("expected a refusal, got {exit:?}");
    };
    assert_eq!(diags[0].code, E_TRACE_ENTRY);

    // The same scene without `--entry` still walks normally.
    let (_, exit) = trace_document(&input_for(scene), MockSet::default());
    assert_eq!(exit, TraceExit::Complete);
}
