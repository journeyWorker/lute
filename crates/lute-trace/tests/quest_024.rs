//! dsl 0.24.0 §2 quest structure through `lute trace`: `activate="accept"`
//! children, `complete="any"` parents (the untaken alternatives fail
//! `superseded`, the synthesized fail waits for every alternative), the
//! reserved `quest.<id>.failedBy` / `quest.<id>.objectives.<o>.failed`
//! reads, `<on event target>`, and `::accept{… at="nextRun"}`.

use std::path::Path;

use lute_check::{CheckInput, Mode};
use lute_trace::{trace_document, Decision, MockSet, Step, TraceExit, TraceReport};

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
    let (mut snapshot, _) = lute_manifest::project::resolve_document_snapshot(
        None,
        meta0.profile.as_deref(),
        &meta0.plugins,
    );
    snapshot.events.insert(
        "talk".to_string(),
        lute_manifest::schema::EventDecl {
            name: "talk".to_string(),
        },
    );
    let base = Path::new(".");
    CheckInput {
        text: text.to_string(),
        uri: "quest_024.lute".to_string(),
        snapshot,
        providers: lute_manifest::provider::ProviderSet::default(),
        mode: Mode::Ci,
        imports: lute_check::resolve_imports(base, &meta0.uses, &meta0.extends, doc.meta.span),
        components: lute_check::resolve_components(base, &meta0.components, doc.meta.span),
        defaults: Default::default(),
    }
}

fn trace(text: &str, mocks: MockSet) -> TraceReport {
    let (report, exit) = trace_document(&input_for(text), mocks);
    assert!(
        matches!(exit, TraceExit::Complete),
        "expected a complete walk, got {exit:?}: {:?}",
        report.decisions
    );
    report
}

fn accepts(ids: &[&str]) -> MockSet {
    MockSet {
        accepts: ids.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }
}

fn quest_outcomes<'a>(decisions: &'a [Decision], id: &str) -> Vec<&'a str> {
    decisions
        .iter()
        .filter(|d| d.construct == "quest" && d.id == id)
        .map(|d| d.outcome.as_str())
        .collect()
}

fn quest_failed<'a>(decisions: &'a [Decision], id: &str) -> &'a Decision {
    decisions
        .iter()
        .find(|d| d.construct == "quest" && d.id == id && d.outcome == "failed")
        .unwrap_or_else(|| panic!("quest {id} never failed: {decisions:?}"))
}

fn final_state<'a>(report: &'a TraceReport, path: &str) -> Option<&'a str> {
    report.final_state.get(path).map(String::as_str)
}

fn said(report: &TraceReport, needle: &str) -> bool {
    report
        .steps
        .iter()
        .any(|s| matches!(s, Step::Line { text, .. } if text.contains(needle)))
}

/// The ember-road shape: a road opened by ANY of two ways, each taken up
/// from dialogue.
const ROAD: &str = r#"---
kind: quest
title: Road
state:
  run.talked: { type: bool, default: false }
  run.paid: { type: bool, default: false }
  run.refused: { type: bool, default: false }
---

<quest id="road" title="Road" start="true" complete="any">
  <objective id="words" title="Words" quest="parley"/>
  <objective id="silver" title="Silver" quest="toll"/>
</quest>

<quest id="parley" title="Parley" activate="accept" fail="run.refused">
  <objective id="talk" title="Talk" done="run.talked"/>
  <on event="questFailed">
    @narrator: parley moot
  </on>
</quest>

<quest id="toll" title="Toll" activate="accept">
  <objective id="pay" title="Pay" done="run.paid"/>
  <on event="questFailed">
    @narrator: toll moot
  </on>
</quest>
"#;

fn road(seed: &[(&str, &str)], accepted: &[&str]) -> TraceReport {
    let mut text = ROAD.to_string();
    for (path, value) in seed {
        text = text.replace(
            &format!("{path}: {{ type: bool, default: false }}"),
            &format!("{path}: {{ type: bool, default: {value} }}"),
        );
    }
    trace(&text, accepts(accepted))
}

#[test]
fn an_accept_activated_child_waits_for_its_accept() {
    // Nothing accepted: the parent runs, neither alternative activates.
    let report = road(&[("run.talked", "true")], &[]);
    assert_eq!(quest_outcomes(&report.decisions, "road"), ["active"]);
    assert_eq!(
        quest_outcomes(&report.decisions, "parley"),
        ["awaiting accept"]
    );
    assert_eq!(
        quest_outcomes(&report.decisions, "toll"),
        ["awaiting accept"]
    );
    assert_ne!(final_state(&report, "quest.parley.state"), Some("active"));

    // `--accept` on an `activate="accept"` child is legal and activates it
    // under its active parent.
    let report = road(&[], &["toll"]);
    assert_eq!(quest_outcomes(&report.decisions, "toll"), ["active"]);
    assert_eq!(final_state(&report, "quest.toll.state"), Some("active"));
    assert_eq!(
        quest_outcomes(&report.decisions, "parley"),
        ["awaiting accept"]
    );
}

#[test]
fn complete_any_completes_on_one_alternative_and_supersedes_the_others() {
    let report = road(&[("run.talked", "true")], &["parley", "toll"]);
    assert_eq!(
        quest_outcomes(&report.decisions, "parley"),
        ["active", "complete"]
    );
    assert_eq!(
        quest_outcomes(&report.decisions, "road"),
        ["active", "complete"]
    );
    let toll = quest_failed(&report.decisions, "toll");
    assert_eq!(toll.guard.as_deref(), Some("superseded from quest.road"));
    assert_eq!(
        final_state(&report, "quest.toll.failedBy"),
        Some("superseded")
    );
    assert_eq!(final_state(&report, "quest.toll.state"), Some("failed"));
    assert!(
        said(&report, "toll moot"),
        "a superseded child fires questFailed: {:?}",
        report.steps
    );
    assert_ne!(
        final_state(&report, "quest.parley.failedBy"),
        Some("superseded")
    );

    // A never-accepted alternative is not superseded: it stays unset.
    let report = road(&[("run.talked", "true")], &["parley"]);
    assert_eq!(
        quest_outcomes(&report.decisions, "road"),
        ["active", "complete"]
    );
    assert_eq!(
        quest_outcomes(&report.decisions, "toll"),
        ["awaiting accept"]
    );
    assert!(!said(&report, "toll moot"));
    assert_ne!(final_state(&report, "quest.toll.state"), Some("failed"));
}

#[test]
fn one_failed_alternative_leaves_an_any_parent_open_and_all_of_them_fail_it() {
    // `parley` fails; `toll` is still open, so `road` stays active.
    let report = road(&[("run.refused", "true")], &["parley", "toll"]);
    assert_eq!(
        quest_outcomes(&report.decisions, "parley"),
        ["active", "failed"]
    );
    assert_eq!(final_state(&report, "quest.parley.failedBy"), Some("fail"));
    assert_eq!(quest_outcomes(&report.decisions, "road"), ["active"]);
    assert_eq!(quest_outcomes(&report.decisions, "toll"), ["active"]);

    // A plain objective's missed deadline counts as a failed alternative:
    // the synthesized fail needs every required objective failed.
    let text = r#"---
kind: quest
title: Any
state:
  run.late: { type: bool, default: true }
  run.refused: { type: bool, default: true }
  run.never: { type: bool, default: false }
---

<quest id="road" title="Road" start="true" complete="any">
  <objective id="storm" title="Storm" done="run.never" by="run.late"/>
  <objective id="words" title="Words" quest="parley"/>
  <on event="questFailed">
    @narrator: road lost
  </on>
</quest>

<quest id="parley" title="Parley" activate="accept" fail="run.refused">
  <objective id="talk" title="Talk" done="run.never"/>
</quest>
"#;
    // Only the deadline missed: `parley` was never accepted, so it is open.
    let report = trace(text, MockSet::default());
    assert_eq!(
        final_state(&report, "quest.road.objectives.storm.failed"),
        Some("true")
    );
    assert_eq!(quest_outcomes(&report.decisions, "road"), ["active"]);
    assert!(!said(&report, "road lost"));
    // Both alternatives failed: the parent fails.
    let report = trace(text, accepts(&["parley"]));
    assert_eq!(final_state(&report, "quest.parley.failedBy"), Some("fail"));
    assert_eq!(
        quest_outcomes(&report.decisions, "road"),
        ["active", "failed"]
    );
    assert_eq!(final_state(&report, "quest.road.failedBy"), Some("fail"));
    assert!(said(&report, "road lost"));
}

#[test]
fn failed_by_names_the_reason_and_objective_failed_is_readable() {
    let text = r#"---
kind: quest
title: Reasons
state:
  run.late: { type: bool, default: true }
  run.never: { type: bool, default: false }
  run.boom: { type: bool, default: false }
---

<quest id="errand" title="Errand" start="true">
  <objective id="letter" title="Letter" done="run.never" by="run.late"/>
  <objective id="sub" title="Sub" quest="child" optional/>
  <on event="questFailed">
    ::set{run.boom = true}
    @narrator: letter failed {{quest.errand.objectives.letter.failed}} by {{quest.errand.failedBy}}
  </on>
</quest>

<quest id="child" title="Child">
  <objective id="wait" title="Wait" done="run.never"/>
</quest>

<quest id="watch" title="Watch" start="true" fail="run.boom">
  <objective id="look" title="Look" done="run.never"/>
</quest>
"#;
    let report = trace(text, MockSet::default());
    assert_eq!(final_state(&report, "quest.errand.failedBy"), Some("by"));
    assert_eq!(
        final_state(&report, "quest.errand.objectives.letter.failed"),
        Some("true")
    );
    assert!(
        said(&report, "letter failed true by by"),
        "{:?}",
        report.steps
    );
    assert_eq!(
        final_state(&report, "quest.child.failedBy"),
        Some("cascade")
    );
    assert_eq!(
        quest_failed(&report.decisions, "child").guard.as_deref(),
        Some("cascade from quest.errand")
    );
    assert_eq!(final_state(&report, "quest.watch.failedBy"), Some("fail"));
}

#[test]
fn until_is_the_failed_by_reason_of_a_raise_bound_deadline() {
    let text = r#"---
kind: quest
title: Until
state:
  run.late: { type: bool, default: true }
  run.never: { type: bool, default: false }
---

<quest id="fest" title="Fest" start="true">
  <objective id="go" title="Go" on="talk" done="run.never" until="run.late"/>
</quest>
"#;
    let report = trace(
        text,
        MockSet {
            occasions: vec!["talk".to_string()],
            ..Default::default()
        },
    );
    assert_eq!(final_state(&report, "quest.fest.failedBy"), Some("until"));
    assert_eq!(
        final_state(&report, "quest.fest.objectives.go.failed"),
        Some("true")
    );
}

/// dsl 0.24.0 §2.1 (lighthouse N2): an `on=` objective whose `by` holds
/// wherever its `done` does. The walk that raises its occasion judges `done`
/// first; a walk that never raises it fails the objective at the settle.
#[test]
fn a_raise_judges_done_before_the_by_it_defers() {
    let text = r#"---
kind: quest
title: Verdict
state:
  run.filed: { type: bool, default: true }
---

<quest id="verdict" title="Verdict" start="true">
  <objective id="fate" title="Fate" on="talk" done="run.filed" by="run.filed"/>
</quest>
"#;
    let raised = MockSet {
        occasions: vec!["talk".to_string()],
        ..Default::default()
    };
    let report = trace(text, raised);
    assert_eq!(
        quest_outcomes(&report.decisions, "verdict"),
        ["active", "complete"]
    );
    assert_ne!(
        final_state(&report, "quest.verdict.objectives.fate.failed"),
        Some("true")
    );

    let report = trace(text, MockSet::default());
    assert_eq!(
        quest_outcomes(&report.decisions, "verdict"),
        ["active", "failed"]
    );
    assert_eq!(final_state(&report, "quest.verdict.failedBy"), Some("by"));
}

/// Ember N15: an `accepts:` of an `activate="accept"` child whose parent never
/// activated is spent — and the report says so.
#[test]
fn an_accept_spent_by_an_inactive_parent_is_noted() {
    let text = ROAD.replace(
        r#"<quest id="road" title="Road" start="true""#,
        r#"<quest id="road" title="Road" start="run.refused""#,
    );
    let report = trace(&text, accepts(&["toll"]));
    assert_eq!(quest_outcomes(&report.decisions, "road"), ["never"]);
    assert!(quest_outcomes(&report.decisions, "toll").is_empty());
    assert!(
        report
            .notes
            .iter()
            .any(|n| n
                .starts_with("accept of `toll` spent: its parent quest `road` is never active")),
        "{:?}",
        report.notes
    );
    // Accepted under an active parent: no note.
    let report = road(&[], &["toll"]);
    assert!(
        !report.notes.iter().any(|n| n.starts_with("accept of")),
        "{:?}",
        report.notes
    );
}

#[test]
fn a_targeted_on_fires_only_for_a_raise_for_its_target() {
    let text = r#"---
kind: quest
title: Targets
state:
  run.never: { type: bool, default: false }
---

<quest id="gossip" title="Gossip" start="true">
  <objective id="hear" title="Hear" done="run.never"/>
  <on event="talk" target="npc.maud">
    @narrator: maud handler
  </on>
  <on event="talk">
    @narrator: any handler
  </on>
</quest>
"#;
    let raise = |occasion: &str| MockSet {
        occasions: vec![occasion.to_string()],
        ..Default::default()
    };
    let report = trace(text, raise("talk@npc.maud"));
    assert!(
        said(&report, "maud handler") && said(&report, "any handler"),
        "{:?}",
        report.steps
    );
    let report = trace(text, raise("talk@npc.oskar"));
    assert!(!said(&report, "maud handler") && said(&report, "any handler"));
    let report = trace(text, raise("talk"));
    assert!(!said(&report, "maud handler") && said(&report, "any handler"));
    // A plain world event is never a raise for a target.
    let report = trace(
        text,
        MockSet {
            events: vec!["talk".to_string()],
            ..Default::default()
        },
    );
    assert!(!said(&report, "maud handler") && said(&report, "any handler"));
}

#[test]
fn an_accept_at_next_run_is_queued_not_applied() {
    let text = r#"---
kind: quest
title: Hub
state:
  run.never: { type: bool, default: false }
---

<quest id="delve" title="Delve" start="true">
  <objective id="enter" title="Enter" done="true"/>
  <on event="questComplete">
    ::accept{quest="bounty" at="nextRun"}
    ::accept{quest="errand"}
  </on>
</quest>

<quest id="bounty" title="Bounty" tier="run">
  <objective id="hunt" title="Hunt" done="run.never"/>
</quest>

<quest id="errand" title="Errand">
  <objective id="run" title="Run" done="run.never"/>
</quest>
"#;
    let report = trace(text, MockSet::default());
    let accepts: Vec<(&str, bool)> = report
        .steps
        .iter()
        .filter_map(|s| match s {
            Step::Accept { quest, next_run } => Some((quest.as_str(), *next_run)),
            _ => None,
        })
        .collect();
    assert_eq!(accepts, [("bounty", true), ("errand", false)]);
    let json = serde_json::to_value(&report.steps).unwrap();
    let queued: Vec<&serde_json::Value> = json
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s.get("nextRun").is_some())
        .collect();
    assert_eq!(
        queued.len(),
        1,
        "only the queued accept carries nextRun: {json}"
    );
    assert_ne!(final_state(&report, "quest.bounty.state"), Some("active"));
}
