//! 0.53.0 §6 (#226): references inside structured plugin frontmatter values
//! — `record` fields, `list` items at any depth, `map` keys and values — are
//! checked like a top-level value, named by their path.
//!
//! On v0.52.0 only a top-level value or a top-level list of a reference type
//! was checked: every reference below a `record` or `map` was silent, and a
//! `providerRef` field or an entity-keyed `map` was `E-FRONTMATTER-SCHEMA`.

use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_core_span::Diagnostic;
use lute_manifest::provider::{ProviderSet, ProviderSnapshot};
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::{Field, Type};

fn field(name: &str, ty: Type) -> Field {
    Field {
        name: name.into(),
        ty,
        default: None,
        required: false,
        shape: None,
    }
}

fn entity() -> Type {
    Type::Entity("person".into())
}

fn list(ty: Type) -> Type {
    Type::List(Box::new(ty))
}

/// Core + the shared test vocabulary and the plugin frontmatter keys:
/// `talk` (record: `partner` entity, `turns` int, `guests` list of entity,
/// `prize` providerRef `items`), `scenes` (list of record: `who` entity),
/// `moods` (map entity → domain `mood`), `lead` (entity).
fn snapshot() -> CapabilitySnapshot {
    let mut snap = lute_test_vocab::vocab_snapshot();
    for (key, ty) in [
        (
            "talk",
            Type::Record(vec![
                field("partner", entity()),
                field("turns", Type::Int),
                field("guests", list(entity())),
                field("prize", Type::ProviderRef("items".into())),
            ]),
        ),
        ("scenes", list(Type::Record(vec![field("who", entity())]))),
        (
            "moods",
            Type::Map {
                key: Box::new(entity()),
                value: Box::new(Type::Domain("mood".into())),
            },
        ),
        ("lead", entity()),
    ] {
        snap.frontmatter.insert(key.into(), ty);
    }
    snap
}

fn providers() -> ProviderSet {
    ProviderSet::from_one(ProviderSnapshot {
        manifest_version: "cap".into(),
        provider_version: "1".into(),
        entries: [("items".to_string(), vec!["sword".to_string()])]
            .into_iter()
            .collect(),
        stale: false,
    })
}

fn diagnostics(text: &str) -> Vec<Diagnostic> {
    let input = CheckInput::new(
        text.into(),
        "t".into(),
        Mode::Ci,
        lute_check::InputSources::from_parts(
            snapshot(),
            providers(),
            SchemaImports::default(),
            Default::default(),
            Default::default(),
        ),
    );
    check(&input).diagnostics
}

fn errors(ds: &[Diagnostic]) -> Vec<(&str, &str)> {
    ds.iter()
        .filter(|d| d.severity == lute_core_span::Severity::Error)
        .map(|d| (d.code.as_str(), d.message.as_str()))
        .collect()
}

/// A scene whose frontmatter adds `front` after the triad and an
/// `entities: person` kind.
fn scene(front: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
         entities:\n  person: {{ members: [mira, oda] }}\n{front}---\n## Shot 1.\n@narrator: hi\n"
    )
}

/// #226: a misspelled entity in a record field is `E-BAD-ENUM` with the
/// did-you-mean and the field path, at the key.
#[test]
fn a_record_field_reference_is_checked_and_named_by_its_path() {
    let text = scene("talk: { partner: mirra, turns: 3 }\n");
    let ds = diagnostics(&text);
    assert_eq!(
        errors(&ds),
        vec![(
            "E-BAD-ENUM",
            "`mirra` is not a member of entity kind `person` (field `partner` of `talk` of \
             `frontmatter`) — did you mean `mira`?"
        )],
        "{ds:#?}"
    );
    let at = ds.iter().find(|d| d.code == "E-BAD-ENUM").unwrap();
    assert_eq!(&text[at.span.byte_start..at.span.byte_end], "talk");

    let ok = diagnostics(&scene("talk: { partner: mira, turns: 3 }\n"));
    assert!(errors(&ok).is_empty(), "{ok:#?}");
}

/// A record field of a non-reference type holds no reference: a well-typed
/// value is clean, an ill-typed one is the whole value's
/// `E-FRONTMATTER-SCHEMA` (unchanged), and a reference beside a shape error
/// is not checked.
#[test]
fn a_non_reference_field_holds_no_reference() {
    let ok = diagnostics(&scene("talk: { turns: 3 }\n"));
    assert!(errors(&ok).is_empty(), "{ok:#?}");

    for front in [
        "talk: { partner: mira, turns: three }\n",
        "talk: { partner: mirra, turns: three }\n",
        "talk: { partner: mirra, color: red }\n",
        "talk: { partner: 3 }\n",
        "talk: [mira]\n",
    ] {
        let ds = diagnostics(&scene(front));
        assert_eq!(
            errors(&ds).iter().map(|(c, _)| *c).collect::<Vec<_>>(),
            vec!["E-FRONTMATTER-SCHEMA"],
            "{front}: {ds:#?}"
        );
    }
    let ds = diagnostics(&scene("talk: { partner: mira, turns: three }\n"));
    assert_eq!(
        errors(&ds)[0].1,
        "frontmatter key `talk` expects record{partner,turns,guests,prize} (declared by an active \
         plugin), got {partner: \"mira\", turns: \"three\"}",
    );
}

/// A `providerRef` field resolves against the catalog (v0.52.0 refused any
/// record holding one as `E-FRONTMATTER-SCHEMA`).
#[test]
fn a_provider_ref_field_resolves_against_the_catalog() {
    let ok = diagnostics(&scene("talk: { prize: sword }\n"));
    assert!(errors(&ok).is_empty(), "{ok:#?}");

    let bad = diagnostics(&scene("talk: { prize: swrod }\n"));
    assert_eq!(
        errors(&bad),
        vec![("E-UNKNOWN-ID", "`swrod` is not a known `items` id")],
        "{bad:#?}"
    );
}

/// A list field of a record checks each item; a list of records checks each
/// record's fields. Both are named innermost first.
#[test]
fn list_items_inside_records_and_records_inside_lists_are_checked() {
    let ds = diagnostics(&scene(
        "talk: { guests: [mira, odda] }\nscenes: [{ who: oda }, { who: mirra }]\n",
    ));
    assert_eq!(
        errors(&ds),
        vec![
            (
                "E-BAD-ENUM",
                "`odda` is not a member of entity kind `person` (item 2 of field `guests` of \
                 `talk` of `frontmatter`) — did you mean `oda`?"
            ),
            (
                "E-BAD-ENUM",
                "`mirra` is not a member of entity kind `person` (field `who` of item 2 of \
                 `scenes` of `frontmatter`) — did you mean `mira`?"
            ),
        ],
        "{ds:#?}"
    );
}

/// A `map` keyed by an entity kind checks each key against the kind and each
/// value against the value type (v0.52.0: `E-FRONTMATTER-SCHEMA` for any
/// such map).
#[test]
fn a_map_checks_its_keys_and_its_values() {
    let ok = diagnostics(&scene("moods: { mira: peaceful, oda: tense }\n"));
    assert!(errors(&ok).is_empty(), "{ok:#?}");

    let ds = diagnostics(&scene("moods: { mirra: peaceful, oda: grumpy }\n"));
    let errs = errors(&ds);
    assert_eq!(errs.len(), 2, "{ds:#?}");
    assert_eq!(
        errs[0],
        (
            "E-BAD-ENUM",
            "`mirra` is not a member of entity kind `person` (key `mirra` of `moods` of \
             `frontmatter`) — did you mean `mira`?"
        ),
    );
    assert_eq!(errs[1].0, "E-BAD-ENUM", "{ds:#?}");
    assert!(
        errs[1].1.starts_with(
            "`grumpy` is not a valid value for entry `oda` of `moods` of `frontmatter`"
        ),
        "{ds:#?}"
    );

    let ds = diagnostics(&scene("moods: { mira: [peaceful] }\n"));
    assert_eq!(
        errors(&ds).iter().map(|(c, _)| *c).collect::<Vec<_>>(),
        vec!["E-FRONTMATTER-SCHEMA"],
        "{ds:#?}"
    );
}

/// A top-level value keeps the attribute's text.
#[test]
fn a_top_level_value_keeps_the_attribute_text() {
    let ds = diagnostics(&scene("lead: mirra\n"));
    assert_eq!(
        errors(&ds),
        vec![(
            "E-BAD-ENUM",
            "`mirra` is not a member of entity kind `person` (attribute `lead` of \
             `frontmatter`) — did you mean `mira`?"
        )],
        "{ds:#?}"
    );
}
