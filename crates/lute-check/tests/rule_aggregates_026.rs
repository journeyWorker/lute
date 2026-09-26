//! dsl 0.26.0 §6 (T3-9): a rule body's `count(…) <op> n` /
//! `countDistinct(…) <op> n` over a relation the head does not depend on is
//! stratified and checks clean; a count inside the head's own recursion is
//! `E-RULE-AGGREGATE-CYCLE`; a `countDistinct` variable the counted atom
//! does not name, or another literal binds, is `E-DATALOG-UNSAFE`.
use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_core_span::Diagnostic;
use lute_manifest::provider::ProviderSet;

const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
entities:\n  person: { members: [ann, bob] }\n  town: { members: [t1, t2] }\n\
relations:\n  toured: { args: [person, town] }\n  listed: { args: [person] }\n\
\x20 traveled: { args: [person], derive: true }\n  busy: { args: [person], derive: true }\n\
facts:\n  - \"listed(ann)\"\n  - \"toured(ann, t1)\"\n";

/// A scene whose `rules:` are `rules`, one line guarded on `traveled(ann)`.
fn scene(rules: &[&str]) -> String {
    let rules: String = rules.iter().map(|r| format!("  - \"{r}\"\n")).collect();
    format!("{HDR}rules:\n{rules}---\n## Shot 1.\n@narrator{{when=\"holds(traveled(ann))\"}}: a\n")
}

fn diags(text: &str) -> Vec<Diagnostic> {
    check(&CheckInput {
        text: text.to_string(),
        uri: "rule_aggregates_026".into(),
        snapshot: lute_manifest::core::load_core_snapshot(),
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    })
    .diagnostics
}

fn with_code<'a>(ds: &'a [Diagnostic], code: &str) -> Vec<&'a Diagnostic> {
    ds.iter().filter(|d| d.code == code).collect()
}

#[test]
fn a_count_over_a_lower_stratum_checks_clean() {
    // `busy` is itself derived: the count reads a finished lower stratum.
    let busy = "busy(P) :- toured(P, _)";
    for rules in [
        &[
            busy,
            "traveled(P) :- listed(P), countDistinct(toured(P, T), T) >= 2",
        ][..],
        &[busy, "traveled(P) :- listed(P), count(toured(P, _)) > 0"],
        &[busy, "traveled(P) :- listed(P), count(busy(_)) <= 1"],
    ] {
        let ds = diags(&scene(rules));
        assert!(ds.is_empty(), "{rules:?}: {ds:#?}");
    }
}

#[test]
fn a_count_inside_the_heads_recursion_is_e_rule_aggregate_cycle() {
    // Directly, and through a second relation the head feeds.
    for rules in [
        &["traveled(P) :- listed(P), count(traveled(_)) >= 1"][..],
        &[
            "busy(P) :- traveled(P)",
            "traveled(P) :- listed(P), count(busy(_)) >= 1",
        ],
    ] {
        let ds = diags(&scene(rules));
        let cyc = with_code(&ds, "E-RULE-AGGREGATE-CYCLE");
        assert_eq!(cyc.len(), 1, "{rules:?}: {ds:#?}");
        assert!(
            cyc[0].message.contains("a rule deriving `traveled` counts"),
            "{}",
            cyc[0].message
        );
        // A negation-free cycle through a count is not also unstratified.
        assert!(
            with_code(&ds, "E-DATALOG-UNSTRATIFIED").is_empty(),
            "{ds:#?}"
        );
    }
}

#[test]
fn a_count_distinct_variable_must_be_the_counted_atoms_own() {
    let ds = diags(&scene(&[
        "traveled(P) :- listed(P), countDistinct(toured(P, _), T) >= 1",
    ]));
    let unsafe_ = with_code(&ds, "E-DATALOG-UNSAFE");
    assert_eq!(unsafe_.len(), 1, "{ds:#?}");
    assert!(
        unsafe_[0]
            .message
            .contains("`T` is not an argument of `toured`"),
        "{}",
        unsafe_[0].message
    );

    // `P` is bound by `listed(P)`: it has one value, nothing to count.
    let ds = diags(&scene(&[
        "traveled(P) :- listed(P), countDistinct(toured(P, T), P) >= 1",
    ]));
    let unsafe_ = with_code(&ds, "E-DATALOG-UNSAFE");
    assert_eq!(unsafe_.len(), 1, "{ds:#?}");
    assert!(
        unsafe_[0].message.contains("bound by another literal"),
        "{}",
        unsafe_[0].message
    );
}

#[test]
fn a_count_binds_no_head_variable() {
    // `P` appears only inside the count: the head is unsafe.
    let ds = diags(&scene(&["traveled(P) :- count(toured(P, _)) >= 1"]));
    assert_eq!(with_code(&ds, "E-DATALOG-UNSAFE").len(), 1, "{ds:#?}");
}
