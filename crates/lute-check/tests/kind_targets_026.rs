//! dsl 0.26.0 §5: a beat or entry targeting a whole kind (`target="kind:X"`)
//! and the member it was raised for, `occasion.target`.
use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_manifest::schema::{OccasionDecl, OccasionSelect, OccasionTarget};

fn input(text: &str) -> CheckInput {
    let mut snapshot = lute_manifest::core::load_core_snapshot();
    snapshot.occasions.insert(
        "caught".into(),
        OccasionDecl {
            name: "caught".into(),
            select: OccasionSelect::First,
            target: OccasionTarget::Domain {
                prefix: "mon".into(),
                entity: "species".into(),
                members: None,
            },
            description: None,
            ..Default::default()
        },
    );
    CheckInput {
        text: text.to_string(),
        uri: "kind_targets".into(),
        snapshot,
        providers: Default::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
        defaults: Default::default(),
    }
}

fn errors(text: &str) -> Vec<(String, String)> {
    check(&input(text))
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == lute_core_span::Severity::Error)
        .map(|d| (d.code, d.message))
        .collect()
}

const VOCAB: &str = "entities:\n  species: { members: [hornbeetle, bladebug, slumbear] }\n  \
                     bugMon: { subsetOf: species, members: [hornbeetle, bladebug] }\n  \
                     item: { members: [ball] }\n\
                     state:\n  run.best: { type: number, default: 0 }\n";

fn lore(body: &str) -> String {
    format!("---\nkind: lore\nid: lore.contest\ntitle: Contest\n{VOCAB}---\n{body}")
}

#[test]
fn a_kind_beat_reads_the_raised_member_typed_by_the_kind() {
    let errs = errors(&lore(
        "<beat id=\"catch\" on=\"caught\" target=\"kind:bugMon\" once=\"false\" when=\"occasion.target != 'bladebug' || run.best == 0\">\n  ::set{run.best = 3 when=\"occasion.target == 'hornbeetle'\"}\n  @narrator: A {{occasion.target}}!\n</beat>\n",
    ));
    assert!(errs.is_empty(), "{errs:?}");
}

#[test]
fn occasion_target_compared_with_a_non_member_is_literal_domain() {
    let errs = errors(&lore(
        "<beat id=\"catch\" on=\"caught\" target=\"kind:bugMon\" once=\"false\" when=\"occasion.target != 'slumbear'\">\n  ::set{run.best = 3 when=\"occasion.target == 'hornbetle'\"}\n  @narrator: A {{occasion.target}}!\n</beat>\n",
    ));
    let lit: Vec<&String> = errs
        .iter()
        .filter(|(c, _)| c == "E-WHEN-LITERAL-DOMAIN")
        .map(|(_, m)| m)
        .collect();
    assert_eq!(lit.len(), 2, "{errs:?}");
    assert!(
        lit.iter().any(|m| m.contains(
            "`'slumbear'` is not a member of `occasion.target`'s domain [bladebug, hornbeetle]"
        )),
        "{lit:?}"
    );
    assert!(
        lit.iter()
            .any(|m| m.contains("did you mean `'hornbeetle'`")),
        "{lit:?}"
    );
}

#[test]
fn a_kind_target_outside_the_domain_or_unknown_is_a_beat_attr_error() {
    let errs = errors(&lore(
        "<beat id=\"a\" on=\"caught\" target=\"kind:item\" once=\"false\">\n  @narrator: a\n</beat>\n\
         <beat id=\"b\" on=\"caught\" target=\"kind:bugMn\" once=\"false\">\n  @narrator: b\n</beat>\n",
    ));
    let attr: Vec<&String> = errs
        .iter()
        .filter(|(c, _)| c == "E-BEAT-ATTR")
        .map(|(_, m)| m)
        .collect();
    assert_eq!(attr.len(), 2, "{errs:?}");
    assert!(
        attr.iter().any(|m| m.contains("`ball` is outside")),
        "{attr:?}"
    );
    assert!(
        attr.iter()
            .any(|m| m.contains("did you mean `kind:bugMon`")),
        "{attr:?}"
    );
}

#[test]
fn occasion_target_outside_a_kind_beat_is_undeclared() {
    let errs = errors(&lore(
        "<beat id=\"catch\" on=\"caught\" target=\"kind:bugMon\" once=\"false\">\n  @narrator: ok\n</beat>\n\
         <beat id=\"one\" on=\"caught\" target=\"mon.slumbear\" once=\"false\">\n  @narrator: A {{occasion.target}}!\n</beat>\n",
    ));
    assert!(
        errs.iter()
            .any(|(c, m)| c == "E-UNDECLARED" && m.contains("targets a kind")),
        "{errs:?}"
    );
}
