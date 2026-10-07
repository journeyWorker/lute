use lute_check::{CheckInput, Mode};
use lute_compile::{compile, ExecutionIr};

fn artifact(meta: &str, body: &str) -> ExecutionIr {
    let input = CheckInput {
        text: format!("---\nkind: scene\nid: semanticTest\n{meta}---\n## Test\n{body}\n"),
        uri: "semantic-trigger".into(),
        snapshot: lute_test_vocab::vocab_snapshot(),
        providers: Default::default(), mode: Mode::Ci,
        imports: Default::default(), components: Default::default(), defaults: Default::default(),
    };
    compile(&input).unwrap_or_else(|d| panic!("minimal semantic source failed: {d:?}"))
}

fn requires(ir: ExecutionIr, ids: &[&str]) {
    let mut expected = vec!["lute.core/1"];
    expected.extend_from_slice(ids);
    expected.sort_unstable();
    expected.dedup();
    assert_eq!(ir.required_semantics, expected);
}

#[test]
fn core_source() { requires(artifact("", "@narrator: baseline"), &[]); }

#[test]
fn staging_source() { requires(artifact("", "::camera{focus=\"hero\" duration=\"0.5\"}"), &["lute.staging/1"]); }

#[test]
fn temporal_source() {
    requires(artifact("", "@narrator{when=\"now() == now()\"}: temporal"), &["lute.knowledge.temporal/1"]);
}

#[test]
fn quest_lifecycle_source() {
    let mut input = CheckInput {
        text: "---\nkind: quest\n---\n<quest id=\"q\" start=\"true\">\n<objective id=\"o\" done=\"true\"/>\n</quest>\n".into(),
        uri: "quest-trigger".into(), snapshot: lute_test_vocab::vocab_snapshot(), providers: Default::default(),
        mode: Mode::Ci, imports: Default::default(), components: Default::default(), defaults: Default::default(),
    };
    requires(compile(&input).expect("minimal quest compiles"), &["lute.quest.lifecycle/1"]);
    // Rewards are declarations owned by a quest, so lifecycle is unavoidable.
    input.text = "---\nkind: quest\n---\n<quest id=\"q\" start=\"true\">\n<objective id=\"o\" done=\"true\"/>\n<reward kind=\"xp\" amount=\"1\"/>\n</quest>\n".into();
    requires(compile(&input).expect("minimal reward quest compiles"), &["lute.quest.lifecycle/1", "lute.quest.rewards/1"]);
}

#[test]
fn timeline_source() {
    requires(artifact("state:\n  scene.n: {type: int, default: 0}\n", "<timeline duration=\"1\">\n<track subject=\"state\">\n::set{scene.n = 1}\n</track>\n</timeline>"), &["lute.timeline/1"]);
}

#[test]
fn facts_source() {
    requires(artifact("entities:\n  person: {members: [a]}\nrelations:\n  seen: {args: [person]}\n", "::assert{seen(a)}"), &["lute.knowledge.facts/1"]);
}

#[test]
fn rules_source() {
    // Datalog rules depend on their relation vocabulary's facts semantics.
    requires(artifact("entities:\n  person: {members: [a]}\nrelations:\n  seen: {args: [person]}\n  known: {args: [person], derive: true}\nrules:\n  - 'known(X) :- seen(X)'\n", "@narrator: rules"), &["lute.knowledge.facts/1", "lute.knowledge.rules/1"]);
}

#[test]
fn plain_enum_staging_has_no_facts() {
    requires(artifact("enums:\n  mode: [quiet, loud]\nstate:\n  scene.mode: {type: {enum: [quiet, loud]}, default: quiet}\n", "::camera{focus=\"hero\"}"), &["lute.staging/1"]);
}

#[test]
fn selection_source() {
    requires(artifact("after: 'visited(\"other\")'\n", "@narrator: after"), &["lute.occasions.selection/1"]);
}
