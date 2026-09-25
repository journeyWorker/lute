//! dsl 0.24.0 §3 at evaluation time: an entity-kind atom in a rule body is a
//! membership test (it never derived anything before — no fact is asserted
//! under a kind's name), a sub-kind is such a kind, and a rule reading
//! `run.approval[P]` evaluates per member — through the checker's vocabulary
//! (`lute trace`) and through the compiled IR (`lute play` / `lute run`)
//! alike.

use std::collections::{BTreeMap, BTreeSet};

use lute_check::{CheckInput, Mode};
use lute_trace::datalog::{ir_kinds, Explanation, Fact, Premise, Program, Proof};
use lute_trace::{trace_document, EffectiveState, MockSet, TraceExit, Value};

const PARTY: &str = r#"---
kind: scene
character: x
season: 1
episode: 1
state:
  run.approval: { type: number, default: 0, per: companion }
entities:
  person: { members: [isolde, corvin, oda] }
  companion: { subsetOf: person, members: [isolde, corvin] }
relations:
  recruited: { args: [person], tier: run }
  inParty: { args: [companion], derive: true }
  loyal: { args: [companion], derive: true }
facts:
  - "recruited(isolde)"
  - "recruited(corvin)"
  - "recruited(oda)"
rules:
  - "inParty(P) :- companion(P), recruited(P)"
  - "loyal(P) :- inParty(P), cel(\"run.approval[P] >= 3\")"
---
## Shot 1.
::set{run.approval.isolde += 3}
<branch id="who">
<choice id="isolde" label="Isolde" when="holds(loyal(isolde))">
@narrator: isolde
</choice>
<choice id="corvin" label="Corvin" when="holds(loyal(corvin))">
@narrator: corvin
</choice>
<choice id="party" label="Party" when="holds(inParty(corvin))">
@narrator: party
</choice>
<choice id="leave" label="Leave">
@narrator: leave
</choice>
</branch>
"#;

fn input() -> CheckInput {
    let (doc, parse_diags) = lute_syntax::parse(PARTY);
    assert!(parse_diags.is_empty(), "{parse_diags:?}");
    let (meta0, _) = lute_check::parse_meta(&doc.meta, &lute_manifest::snapshot::CapabilitySnapshot::default());
    let (snapshot, _) =
        lute_manifest::project::resolve_document_snapshot(None, meta0.profile.as_deref(), &meta0.plugins);
    CheckInput {
        text: PARTY.to_string(),
        uri: "party.lute".to_string(),
        snapshot,
        providers: lute_manifest::provider::ProviderSet::default(),
        mode: Mode::Ci,
        imports: Default::default(),
        components: Default::default(),
        defaults: Default::default(),
    }
}

fn fact(rel: &str, arg: &str) -> Fact {
    (rel.to_string(), vec![arg.to_string()])
}

/// `lute trace`: the kind predicate binds each companion, and the indexed
/// guard reads the approval the scene itself set.
#[test]
fn trace_derives_through_a_kind_predicate_and_an_indexed_guard() {
    let (report, exit) = trace_document(&input(), MockSet::default());
    assert!(matches!(exit, TraceExit::Complete), "{exit:?} {:?}", report.unresolved);
    let who = report.decisions.iter().find(|d| d.id == "who").expect("the branch");
    assert!(who.eligible.contains(&"isolde".to_string()), "{who:?}");
    assert!(!who.eligible.contains(&"corvin".to_string()), "{who:?}");
    assert!(who.eligible.contains(&"party".to_string()), "{who:?}");
}

/// The IR carries the rule grounded per member and the kinds as `entities`;
/// the runner's evaluator derives from exactly that.
#[test]
fn the_compiled_rules_derive_with_the_ir_kinds() {
    let art = lute_compile::compile(&input()).expect("compiles");
    let json = serde_json::to_value(&art).unwrap();
    let rules = json.get("rules").and_then(|r| r.as_array()).unwrap();
    let raws: Vec<&str> = rules.iter().filter_map(|r| r.get("raw").and_then(|r| r.as_str())).collect();
    assert_eq!(rules.len(), 3, "{raws:?}");
    assert!(raws.iter().any(|r| r.ends_with("[P = isolde]")), "{raws:?}");
    for r in rules {
        for lit in r["body"].as_array().unwrap() {
            if let Some(cel) = lit.get("cel").and_then(|c| c.as_str()) {
                assert!(!cel.contains('['), "an IR guard is over ground terms: {cel}");
            }
        }
    }

    let program = Program::from_ir(json.get("rules")).with_kinds(ir_kinds(json.get("entities")));
    let schema = lute_check::StateSchema::default();
    let state = BTreeMap::from([
        ("run.approval.isolde".to_string(), Value::Num(3.0)),
        ("run.approval.corvin".to_string(), Value::Num(1.0)),
    ]);
    let eff = EffectiveState::new(&schema, state);
    let base: BTreeSet<Fact> = ["isolde", "corvin", "oda"].iter().map(|p| fact("recruited", p)).collect();
    let closure = program.fixpoint(&base, &eff);
    assert!(closure.facts.contains(&fact("inParty", "isolde")));
    assert!(closure.facts.contains(&fact("inParty", "corvin")));
    assert!(!closure.facts.contains(&fact("inParty", "oda")), "oda is a person, not a companion");
    assert!(closure.facts.contains(&fact("loyal", "isolde")));
    assert!(!closure.facts.contains(&fact("loyal", "corvin")));
    assert!(
        !closure.facts.iter().any(|(r, _)| r == "companion" || r == "person"),
        "membership is tested, never materialised as facts"
    );

    // `--explain`: the kind premise reads as membership, the guard as the
    // member's own path.
    let Explanation::Holds(Proof::Derived { rule, premises, .. }) =
        program.explain(&closure, &fact("inParty", "isolde"), &eff)
    else {
        panic!("inParty(isolde) holds");
    };
    assert!(rule.starts_with("inParty(P)"), "{rule}");
    assert_eq!(
        premises[0],
        Premise::Test {
            text: "companion(isolde) — entity kind `companion`".to_string(),
            holds: Some(true),
        }
    );
    let Explanation::Holds(Proof::Derived { rule, premises, .. }) =
        program.explain(&closure, &fact("loyal", "isolde"), &eff)
    else {
        panic!("loyal(isolde) holds");
    };
    assert!(rule.ends_with("[P = isolde]"), "{rule}");
    assert!(
        premises.iter().any(|p| matches!(p, Premise::Test { text, holds: Some(true) } if text.contains("run.approval.isolde >= 3"))),
        "{premises:?}"
    );
}

/// Without the kinds a kind atom matches nothing — the pre-0.24 runner bug
/// this closes: `companion(P)` never bound, so nothing derived.
#[test]
fn a_kind_atom_needs_the_kinds_to_bind() {
    let art = lute_compile::compile(&input()).expect("compiles");
    let json = serde_json::to_value(&art).unwrap();
    let schema = lute_check::StateSchema::default();
    let eff = EffectiveState::new(&schema, BTreeMap::new());
    let base: BTreeSet<Fact> = BTreeSet::from([fact("recruited", "isolde")]);
    let bare = Program::from_ir(json.get("rules")).fixpoint(&base, &eff);
    assert!(!bare.facts.contains(&fact("inParty", "isolde")));
}
