//! dsl 0.22.0 §6: `lute trace` applies the project's seed facts and Datalog
//! rules (stratified negation) over the mocked/asserted facts by default —
//! the SAME fixpoint `lute run`/`lute play` use. `derive: false` restores the
//! 0.21 lookup model, in which an unmocked derived atom is unknown.

use std::path::Path;

use lute_check::{CheckInput, Mode};
use lute_trace::{trace_document, MockSet, TraceExit, TraceReport};

fn input_for(text: &str, uri: &str, base: &Path) -> CheckInput {
    let (doc, parse_diags) = lute_syntax::parse(text);
    assert!(parse_diags.is_empty(), "fixture must parse clean: {parse_diags:?}");
    let (meta0, _) = lute_check::parse_meta(
        &doc.meta,
        &lute_manifest::snapshot::CapabilitySnapshot::default(),
    );
    let (snapshot, _) = lute_manifest::project::resolve_document_snapshot(
        None,
        meta0.profile.as_deref(),
        &meta0.plugins,
    );
    let imports = lute_check::resolve_imports(base, &meta0.uses, &meta0.extends, doc.meta.span);
    let components = lute_check::resolve_components(base, &meta0.components, doc.meta.span);
    CheckInput {
        text: text.to_string(),
        uri: uri.to_string(),
        snapshot,
        providers: lute_manifest::provider::ProviderSet::default(),
        mode: Mode::Ci,
        imports,
        components,
        defaults: Default::default(),
    }
}

fn halsin() -> CheckInput {
    let path = "../../docs/examples/quest-rescue-halsin.lute";
    let text = std::fs::read_to_string(path).unwrap();
    input_for(&text, path, Path::new("../../docs/examples"))
}

fn span0() -> lute_core_span::Span {
    lute_core_span::Span {
        byte_start: 0,
        byte_end: 0,
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    }
}

fn decided(report: &TraceReport, construct: &str, id: &str, outcome: &str) -> bool {
    report
        .decisions
        .iter()
        .any(|d| d.construct == construct && d.id == id && d.outcome == outcome)
}

/// The seeds put Shadowheart in the party (quest starts), `questActive`
/// asserts `heardLocation` (so `believesLocation` derives) and the seeded
/// topology derives `canReach(player, grove)` recursively — the quest
/// completes with no derived conclusion mocked.
#[test]
fn rule_derived_facts_complete_objectives_without_mocking_the_conclusion() {
    let (report, exit) = trace_document(&halsin(), MockSet::default());
    assert!(matches!(exit, TraceExit::Complete), "{exit:?} {:?}", report.unresolved);
    assert!(decided(&report, "objective", "reach", "done"), "{:?}", report.decisions);
    assert!(decided(&report, "objective", "learn", "done"), "{:?}", report.decisions);
    assert!(decided(&report, "quest", "rescueHalsin", "complete"), "{:?}", report.decisions);
    assert!(report.notes.is_empty(), "seeds loaded, rules applied: {:?}", report.notes);
}

/// `derive: false` is the 0.21 model: the seeds are not loaded, derived
/// objectives read unknown (exit 3), and each derived relation read is
/// named in a note.
#[test]
fn derive_false_leaves_derived_atoms_unknown_and_notes_each_derived_read() {
    let mocks = MockSet {
        facts: vec!["inParty(shadowheart)".to_string()],
        derive: Some(false),
        ..Default::default()
    };
    let (report, exit) = trace_document(&halsin(), mocks);
    assert!(matches!(exit, TraceExit::Incomplete), "{exit:?}");
    for rel in ["believesLocation", "canReach"] {
        assert!(
            report
                .notes
                .iter()
                .any(|n| n.contains(&format!("derived relation `{rel}`")) && n.contains("derive: false")),
            "expected a derived-read note for {rel}: {:?}",
            report.notes
        );
    }
}

/// `culprit(P) :- suspect(P), not alibi(P)` with `suspect(ann)` seeded.
const NEGATION: &str = r#"---
kind: scene
character: x
season: 1
episode: 1
entities:
  person: { members: [ann] }
relations:
  suspect: { args: [person] }
  alibi: { args: [person] }
  culprit: { args: [person], derive: true }
facts:
  - "suspect(ann)"
rules:
  - "culprit(P) :- suspect(P), not alibi(P)"
---
## Shot 1.
<branch id="verdict">
<choice id="accuse" label="Accuse" when="holds(culprit(ann))">
@narrator: accused
</choice>
<choice id="wait" label="Wait">
::assert{ alibi(ann) }
@narrator: waited
</choice>
</branch>
"#;

fn verdict(mocks: MockSet) -> (TraceReport, TraceExit) {
    let input = input_for(NEGATION, "negation.lute", Path::new("."));
    trace_document(&input, mocks)
}

fn choose_accuse(facts: &[&str]) -> MockSet {
    MockSet {
        facts: facts.iter().map(|f| f.to_string()).collect(),
        choose: [("verdict".to_string(), vec!["accuse".to_string()])].into(),
        ..Default::default()
    }
}

#[test]
fn negated_premise_absent_derives_the_conclusion() {
    let (report, exit) = verdict(choose_accuse(&[]));
    assert!(matches!(exit, TraceExit::Complete), "{exit:?}");
    let d = report
        .decisions
        .iter()
        .find(|d| d.id == "verdict")
        .expect("verdict decision");
    assert_eq!(d.outcome, "accuse");
    assert!(!d.forced, "the guard decided true, nothing was forced: {d:?}");
    assert!(d.eligible.contains(&"accuse".to_string()), "{d:?}");
}

/// With the negated premise present the conclusion is definitely false:
/// forcing the guarded choice is refused, not forced past an unknown.
#[test]
fn negated_premise_present_makes_the_conclusion_definitely_false() {
    let (_report, exit) = verdict(choose_accuse(&["alibi(ann)"]));
    let TraceExit::Refused(diags) = exit else {
        panic!("expected E-TRACE-CHOICE refusal, got {exit:?}");
    };
    assert!(diags.iter().any(|d| d.code == lute_trace::E_TRACE_CHOICE), "{diags:?}");
}

/// A rule guard over undecided state decides nothing: the conclusion is
/// unknown and the report names the state path that would decide it.
#[test]
fn rule_guard_over_undecided_state_is_unknown_naming_the_path() {
    let text = r#"---
kind: scene
character: x
season: 1
episode: 1
state:
  run.day: { type: number }
entities:
  person: { members: [ann] }
relations:
  suspect: { args: [person] }
  ready: { args: [person], derive: true }
facts:
  - "suspect(ann)"
rules:
  - "ready(P) :- suspect(P), cel(\"run.day >= 3\")"
---
## Shot 1.
<branch id="go">
<choice id="now" label="Now" when="holds(ready(ann))">
@narrator: now
</choice>
<choice id="later" label="Later">
@narrator: later
</choice>
</branch>
"#;
    let input = input_for(text, "guard.lute", Path::new("."));
    let forced = MockSet {
        choose: [("go".to_string(), vec!["now".to_string()])].into(),
        ..Default::default()
    };
    let (report, exit) = trace_document(&input, forced.clone());
    assert!(!matches!(exit, TraceExit::Refused(_)), "{exit:?}");
    let entry = report
        .forced_unknown
        .iter()
        .find(|u| u.id.starts_with("go"))
        .expect("the guard stayed undecided");
    assert!(
        entry.atoms.iter().any(|a| a.contains("run.day")),
        "the undecided rule guard's path is named: {entry:?}"
    );

    let decided_day = MockSet {
        state: vec![("run.day".to_string(), "3".to_string(), span0())],
        ..forced
    };
    let (report, exit) = trace_document(&input, decided_day);
    assert!(matches!(exit, TraceExit::Complete), "{exit:?}");
    assert!(report.forced_unknown.is_empty(), "{:?}", report.forced_unknown);
}

/// dsl 0.24 T1-1/T3-9: a rule guard's `@def` is expanded (it used to stay
/// undecided and silently drop the rule), `_` in a rule body is an
/// anonymous variable (existential under `not`), and `countDistinct` counts
/// witnesses rather than tuples — `ann` saw two things, `bob` one: three
/// tuples, two witnesses.
const RULES_024: &str = r#"---
kind: scene
character: x
season: 1
episode: 1
state:
  run.day: { type: number, default: 1 }
entities:
  person: { members: [ann, bob, cy] }
  place: { members: [dock, pier] }
relations:
  listed: { args: [person] }
  sawAt: { args: [person, place] }
  testified: { args: [person], derive: true }
  quiet: { args: [person], derive: true }
  early: { args: [person], derive: true }
facts:
  - "listed(ann)"
  - "listed(bob)"
  - "listed(cy)"
  - "sawAt(ann, dock)"
  - "sawAt(ann, pier)"
  - "sawAt(bob, dock)"
rules:
  - "testified(W) :- sawAt(W, _)"
  - "quiet(W) :- listed(W), not sawAt(W, _)"
  - "early(W) :- listed(W), cel(\"@firstDay\")"
defs:
  firstDay: "run.day == 1"
---
## Shot 1.
<branch id="count">
<choice id="yes" label="Yes" when="holds(testified(bob)) && holds(quiet(cy)) && !holds(quiet(ann)) && holds(early(ann)) && countDistinct(sawAt(W, _), W) == 2">
@narrator: yes
</choice>
<choice id="no" label="No" when="countDistinct(sawAt(W, _), W) >= 3">
@narrator: no
</choice>
<choice id="other" label="Other">
@narrator: other
</choice>
</branch>
"#;

#[test]
fn anonymous_rule_variables_defs_in_rule_guards_and_count_distinct_evaluate() {
    let input = input_for(RULES_024, "rules_024.lute", Path::new("."));
    let pick = |id: &str| MockSet {
        choose: [("count".to_string(), vec![id.to_string()])].into(),
        ..Default::default()
    };
    let (report, exit) = trace_document(&input, pick("yes"));
    assert!(matches!(exit, TraceExit::Complete), "{exit:?} {:?}", report.unresolved);
    let d = report.decisions.iter().find(|d| d.id == "count").expect("decision");
    assert!(!d.forced && d.eligible.contains(&"yes".to_string()), "{d:?}");
    assert!(!d.eligible.contains(&"no".to_string()), "{d:?}");
    let (_, exit) = trace_document(&input, pick("no"));
    assert!(matches!(exit, TraceExit::Refused(_)), "three tuples are two witnesses: {exit:?}");
}

// ---------------------------------------------------------------------
// `derive:` mock key and precedence.
// ---------------------------------------------------------------------

#[test]
fn derive_key_parses_and_the_flag_wins_over_the_file() {
    let file = lute_trace::parse_mock_yaml("derive: false\n").expect("parses");
    assert!(!file.derives());
    assert!(MockSet::default().derives(), "derivation is the default");
    let bad = lute_trace::parse_mock_yaml("derive: maybe\n").unwrap_err();
    assert_eq!(bad.code, lute_trace::E_TRACE_MOCK_PARSE);

    let on = lute_trace::parse_mock_yaml("derive: true\n").unwrap();
    let flag = MockSet {
        derive: Some(false),
        ..Default::default()
    };
    assert!(!lute_trace::merge(on.clone(), flag).derives());
    assert!(lute_trace::merge(on, MockSet::default()).derives());
}

// ---------------------------------------------------------------------
// `Program::explain` (`lute play --explain`): proof or failing premises.
// ---------------------------------------------------------------------

mod explain {
    use std::collections::{BTreeMap, BTreeSet};

    use lute_trace::datalog::{Explanation, Fact, Premise, Program, Proof};
    use lute_trace::EffectiveState;

    fn fact(rel: &str, args: &[&str]) -> Fact {
        (rel.to_string(), args.iter().map(|a| a.to_string()).collect())
    }

    /// The NEGATION fixture's rules, through the compiler's IR — the
    /// surface `lute play` hands the evaluator.
    fn program() -> Program {
        let input = super::input_for(super::NEGATION, "negation.lute", std::path::Path::new("."));
        let art = lute_compile::compile(&input).expect("compiles");
        let json = serde_json::to_value(&art).unwrap();
        Program::from_ir(json.get("rules"))
    }

    fn explain(base: &[Fact]) -> Explanation {
        let p = program();
        let schema = lute_check::StateSchema::default();
        let state = EffectiveState::new(&schema, BTreeMap::new());
        let base: BTreeSet<Fact> = base.iter().cloned().collect();
        let closure = p.fixpoint(&base, &state);
        p.explain(&closure, &fact("culprit", &["ann"]), &state)
    }

    #[test]
    fn a_derived_atom_is_proved_with_its_negated_premise_absent() {
        let Explanation::Holds(Proof::Derived { rule, premises, .. }) =
            explain(&[fact("suspect", &["ann"])])
        else {
            panic!("culprit(ann) holds by its rule");
        };
        assert!(rule.starts_with("culprit(P)"), "{rule}");
        assert_eq!(
            premises,
            vec![
                Premise::Holds(Box::new(Proof::Base(fact("suspect", &["ann"])))),
                Premise::Absent(fact("alibi", &["ann"])),
            ]
        );
    }

    #[test]
    fn a_present_negated_premise_is_the_failing_premise() {
        let Explanation::Fails { derived, attempts } =
            explain(&[fact("suspect", &["ann"]), fact("alibi", &["ann"])])
        else {
            panic!("culprit(ann) does not hold with an alibi");
        };
        assert!(derived);
        assert_eq!(attempts.len(), 1);
        assert_eq!(
            attempts[0].premises,
            vec![
                Premise::Holds(Box::new(Proof::Base(fact("suspect", &["ann"])))),
                Premise::Present(Box::new(Proof::Base(fact("alibi", &["ann"])))),
            ]
        );
    }

    #[test]
    fn a_missing_positive_premise_stops_the_attempt() {
        let Explanation::Fails { attempts, .. } = explain(&[]) else {
            panic!("nothing to derive from");
        };
        assert_eq!(
            attempts[0].premises,
            vec![
                Premise::Missing {
                    atom: "suspect(ann)".to_string(),
                    why: Vec::new(),
                },
                Premise::Unreached("not alibi(ann)".to_string()),
            ]
        );
    }
}
