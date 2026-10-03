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
                     state:\n  run.best: { type: int, default: 0 }\n";

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

/// The project beat passes (`W-BEAT-SHADOWED`, `W-BEAT-PRIORITY-TIE`) over
/// one lore document with `kinds` as its entity vocabulary.
fn project_codes(kinds: &str, body: &str) -> Vec<(String, String)> {
    let text = format!("---\nkind: lore\nid: dex\nentities:\n{kinds}---\n{body}");
    let input = input(&text);
    let (doc, _) = lute_syntax::parse(&input.text);
    let folded = lute_check::fold_env(&doc, &input).0;
    let docs = vec![(std::path::PathBuf::from("dex.lute"), doc)];
    lute_check::check_project_beats(
        &docs,
        &[&folded],
        &lute_check::cast::fact_producers(&docs, &Default::default()),
        None,
        &Default::default(),
    )
    .into_iter()
    .map(|(_, d)| (d.code, d.message))
    .collect()
}

fn kind_beat(id: &str, kind: &str) -> String {
    format!("<beat id=\"{id}\" on=\"caught\" target=\"kind:{kind}\" once=\"false\">\n  @narrator: {id}.\n</beat>\n")
}

/// dsl 0.27.0 (T3-10): member > sub-kind > kind. The parent kind's beat
/// comes first in the file, yet at equal priority the sub-kind's wins its
/// members: neither shadowed nor a file-order tie.
#[test]
fn a_sub_kind_beat_outranks_its_parent_kind_beat_at_equal_priority() {
    let kinds = "  species: { members: [hornbeetle, bladebug, slumbear] }\n  \
                 bugMon: { subsetOf: species, members: [hornbeetle, bladebug] }\n  \
                 hornMon: { subsetOf: bugMon, members: [hornbeetle] }\n";
    let body = format!(
        "{}{}{}",
        kind_beat("any", "species"),
        kind_beat("bug", "bugMon"),
        kind_beat("horn", "hornMon")
    );
    let out = project_codes(kinds, &body);
    assert!(out.is_empty(), "{out:?}");
    // The order is the specificity, not the file.
    let ms = |xs: &[&str]| xs.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let (species, bug, horn) = (
        ms(&["hornbeetle", "bladebug", "slumbear"]),
        ms(&["hornbeetle", "bladebug"]),
        ms(&["hornbeetle"]),
    );
    let order = lute_check::beats::selection_order(&[
        ("caught", 0, Some(&species)),
        ("caught", 0, Some(&bug)),
        ("caught", 0, None),
        ("caught", 0, Some(&horn)),
    ]);
    assert_eq!(order, [2, 3, 1, 0]);
    // A higher priority still wins over specificity.
    let order = lute_check::beats::selection_order(&[
        ("caught", 1, Some(&species)),
        ("caught", 0, Some(&bug)),
    ]);
    assert_eq!(order, [0, 1]);
}

/// Two kinds that overlap without one containing the other keep file order
/// — and so tie.
#[test]
fn overlapping_unrelated_kinds_keep_file_order_and_tie() {
    let kinds = "  species: { members: [hornbeetle, bladebug, slumbear] }\n  \
                 bugMon: { subsetOf: species, members: [hornbeetle, bladebug] }\n  \
                 heavy: { subsetOf: species, members: [bladebug, slumbear] }\n";
    let body = format!(
        "{}{}",
        kind_beat("heavy", "heavy"),
        kind_beat("bug", "bugMon")
    );
    let out = project_codes(kinds, &body);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].0, "W-BEAT-PRIORITY-TIE");
    assert!(out[0].1.contains("today beat `dex.heavy`"), "{}", out[0].1);
    let ms = |xs: &[&str]| xs.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let (heavy, bug) = (
        ms(&["bladebug", "slumbear"]),
        ms(&["hornbeetle", "bladebug"]),
    );
    let order = lute_check::beats::selection_order(&[
        ("caught", 0, Some(&heavy)),
        ("caught", 0, Some(&bug)),
    ]);
    assert_eq!(order, [0, 1]);
}
