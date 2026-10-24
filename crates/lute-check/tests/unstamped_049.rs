//! 0.49.0 §4.1–§4.3: timing keys and plugin stamps on the directives with no
//! stamp carrier (#157), `wait` on a content line (#174), and the
//! reference-typed plugin frontmatter keys (#178).

use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_core_span::Diagnostic;
use lute_manifest::provider::{ProviderSet, ProviderSnapshot};
use lute_manifest::schema::AttrDecl;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::{Literal, Type};

/// Core + the shared test vocabulary, a plugin stamp `cue: string`, and the
/// plugin frontmatter keys `lead` (entity `person`), `party` (list of
/// entity `person`), `prize` (providerRef `items`), `loot` (list of
/// providerRef `items`) and `tone` (domain `mood`).
fn snapshot() -> CapabilitySnapshot {
    let mut snap = lute_test_vocab::vocab_snapshot();
    snap.stamp_attrs.insert(
        "cue".into(),
        AttrDecl {
            name: "cue".into(),
            required: false,
            ty: Type::Str,
            default: None,
        },
    );
    for (key, ty) in [
        ("lead", Type::Entity("person".into())),
        ("party", Type::List(Box::new(Type::Entity("person".into())))),
        ("prize", Type::ProviderRef("items".into())),
        (
            "loot",
            Type::List(Box::new(Type::ProviderRef("items".into()))),
        ),
        ("tone", Type::Domain("mood".into())),
        ("weather", Type::EnumFromOption("weathers".into())),
        (
            "skies",
            Type::List(Box::new(Type::EnumFromOption("weathers".into()))),
        ),
        ("backdrop", Type::AssetKind("BG".into())),
        (
            "backdrops",
            Type::List(Box::new(Type::AssetKind("BG".into()))),
        ),
    ] {
        snap.frontmatter.insert(key.into(), ty);
    }
    // `enumFromOption: weathers` resolves through an active plugin option.
    snap.plugins.insert(
        "t.weather".into(),
        lute_manifest::snapshot::ResolvedPlugin {
            version: "0.1.0".into(),
            options: [(
                "weathers".to_string(),
                Literal::List(vec![
                    Literal::Str("sunny".into()),
                    Literal::Str("rain".into()),
                ]),
            )]
            .into_iter()
            .collect(),
            option_defaults: Default::default(),
        },
    );
    // A segment-less query asset kind: provider existence only.
    snap.asset_kinds.insert(
        "BG".into(),
        lute_manifest::schema::AssetKindDecl {
            kind: "BG".into(),
            sep: ".".into(),
            resolve: lute_manifest::schema::AssetResolve::Query,
            segments: vec![],
            provider: Some("backgrounds".into()),
            match_: vec![],
            aliases: Default::default(),
            fallback: vec![],
            persistence: None,
        },
    );
    snap
}

fn providers() -> ProviderSet {
    ProviderSet::from_one(ProviderSnapshot {
        manifest_version: "cap".into(),
        provider_version: "1".into(),
        entries: [
            ("items".to_string(), vec!["sword".to_string()]),
            ("backgrounds".to_string(), vec!["bg_home".to_string()]),
        ]
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

fn coded<'a>(ds: &'a [Diagnostic], code: &str) -> Vec<&'a str> {
    ds.iter()
        .filter(|d| d.code == code)
        .map(|d| d.message.as_str())
        .collect()
}

fn errors(ds: &[Diagnostic]) -> Vec<(&str, &str)> {
    ds.iter()
        .filter(|d| d.severity == lute_core_span::Severity::Error)
        .map(|d| (d.code.as_str(), d.message.as_str()))
        .collect()
}

/// A scene whose frontmatter adds `front` after the triad and an
/// `entities: person` kind, with `body` as its only shot.
fn scene(front: &str, body: &str) -> String {
    format!(
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n\
         entities:\n  person: {{ members: [mira, oda] }}\n{front}---\n## Shot 1.\n{body}"
    )
}

/// #157: on `::jump` and `::label` a plugin stamp and every core timing key
/// are `E-UNKNOWN-ATTR` at the attribute, with the spec's message. Fails on
/// 0.48.1, which checked them clean and dropped them at lowering.
#[test]
fn jump_and_label_refuse_stamps_and_timing_keys() {
    for key in [
        "cue=\"drums\"",
        "wait=\"true\"",
        "duration=\"1\"",
        "delay=\"1\"",
    ] {
        let name = key.split('=').next().unwrap();
        let text = scene(
            "",
            &format!(
                "@narrator: a\n::jump{{to=\"end\" {key}}}\n@narrator: b\n\
                 ::label{{name=\"end\" {key}}}\n@narrator: c\n"
            ),
        );
        let ds = diagnostics(&text);
        assert_eq!(
            coded(&ds, "E-UNKNOWN-ATTR"),
            vec![
                format!(
                    "`::jump` does not carry `{name}`: it compiles to a `jump` record that has \
                     no stamp — put the attribute on the line or directive before it"
                ),
                format!(
                    "`::label` does not carry `{name}`: it compiles to no record — put the \
                     attribute on the line or directive after it"
                ),
            ],
            "{key}: {ds:#?}"
        );
        // The attribute's own span, not the directive's.
        let at = ds.iter().find(|d| d.code == "E-UNKNOWN-ATTR").unwrap();
        assert_eq!(&text[at.span.byte_start..at.span.byte_end], key, "{at:?}");
    }
}

/// #157: `::clear` refuses a plugin stamp with its existing message (0.48.1
/// admitted it through the `stampAttrs` fallback and dropped it).
#[test]
fn clear_refuses_a_plugin_stamp() {
    let text = scene(
        "",
        "::actor{character=\"elena\" anchor=\"center\" action=\"fadeInUp\"}\n\
         @elena: hi\n::clear{cue=\"drums\"}\n@narrator: done\n",
    );
    let ds = diagnostics(&text);
    assert_eq!(
        coded(&ds, "E-UNKNOWN-ATTR"),
        vec![
            "`::clear` has no attribute `cue` — it takes no attributes; every character on stage \
             exits with its own exit (dsl 0.24.0 §4)"
        ],
        "{ds:#?}"
    );
}

/// A too-fine `duration` on an unstamped directive is reported once, as the
/// refused attribute, not also as a time-resolution error.
#[test]
fn a_refused_timing_key_is_not_also_a_resolution_error() {
    let text = scene("", "@narrator: a\n::jump{to=\"end\" duration=\"1.0005s\"}\n@narrator: b\n::label{name=\"end\"}\n");
    let ds = diagnostics(&text);
    assert_eq!(errors(&ds).len(), 1, "{ds:#?}");
    assert_eq!(coded(&ds, "E-UNKNOWN-ATTR").len(), 1, "{ds:#?}");
}

/// The stamp-carrying directives keep both: a stamp and a timing key on
/// `::sfx` and `::end` check clean.
#[test]
fn stamped_directives_still_take_stamps_and_timing_keys() {
    let text = scene(
        "",
        "::sfx{assetId=\"x\" cue=\"drums\" wait=\"true\"}\n@narrator: a\n::end{cue=\"drums\"}\n",
    );
    let ds = diagnostics(&text);
    assert!(errors(&ds).is_empty(), "{ds:#?}");
}

/// #174: a content line takes `wait` (bool); a non-bool is `E-ATTR-TYPE`;
/// `delay`/`duration` stay `E-UNKNOWN-ATTR`. Fails on 0.48.1, where `wait`
/// on a line was `E-UNKNOWN-ATTR`.
#[test]
fn a_content_line_takes_wait_only() {
    let ok = diagnostics(&scene(
        "",
        "@elena{wait=\"false\"}: a\n::sfx{assetId=\"x\"}\n@elena{wait=\"true\"}: b\n",
    ));
    assert!(errors(&ok).is_empty(), "{ok:#?}");

    let bad = diagnostics(&scene("", "@elena{wait=\"soon\"}: a\n"));
    assert_eq!(
        coded(&bad, "E-ATTR-TYPE"),
        vec!["attribute `wait` of `@elena` expects a boolean"],
        "{bad:#?}"
    );

    for key in ["delay", "duration"] {
        let ds = diagnostics(&scene("", &format!("@elena{{{key}=\"1\"}}: a\n")));
        let unknown = coded(&ds, "E-UNKNOWN-ATTR");
        assert_eq!(unknown.len(), 1, "{key}: {ds:#?}");
        assert!(
            unknown[0].starts_with(&format!("unknown content-line attribute `{key}`"))
                && unknown[0].contains("`wait`"),
            "{key}: {unknown:?}"
        );
    }
}

/// #178: an entity-typed frontmatter value outside its kind is `E-BAD-ENUM`
/// with the attribute's did-you-mean, owner `frontmatter`, at the key. Fails
/// on 0.48.1, which accepted any string.
#[test]
fn an_entity_frontmatter_value_is_a_member_of_its_kind() {
    let text = scene("lead: mirra\n", "@narrator: hi\n");
    let ds = diagnostics(&text);
    assert_eq!(
        coded(&ds, "E-BAD-ENUM"),
        vec![
            "`mirra` is not a member of entity kind `person` (attribute `lead` of `frontmatter`) \
             — did you mean `mira`?"
        ],
        "{ds:#?}"
    );
    let at = ds.iter().find(|d| d.code == "E-BAD-ENUM").unwrap();
    assert_eq!(
        &text[at.span.byte_start..at.span.byte_end],
        "lead",
        "{at:?}"
    );

    let ok = diagnostics(&scene("lead: mira\n", "@narrator: hi\n"));
    assert!(errors(&ok).is_empty(), "{ok:#?}");
}

/// #178: a `providerRef` value resolves against the provider catalog: a
/// known id is clean (0.48.1 refused every string as `E-FRONTMATTER-SCHEMA`),
/// an unknown one is `E-UNKNOWN-ID`.
#[test]
fn a_provider_frontmatter_value_resolves_against_the_catalog() {
    let ok = diagnostics(&scene("prize: sword\n", "@narrator: hi\n"));
    assert!(errors(&ok).is_empty(), "{ok:#?}");

    let bad = diagnostics(&scene("prize: swrod\n", "@narrator: hi\n"));
    assert_eq!(
        errors(&bad),
        vec![("E-UNKNOWN-ID", "`swrod` is not a known `items` id")],
        "{bad:#?}"
    );
}

/// #178: a list of a reference type checks each element; a non-string value
/// (or element) stays `E-FRONTMATTER-SCHEMA`.
#[test]
fn a_reference_list_checks_each_element_and_shape_stays_a_schema_error() {
    let ds = diagnostics(&scene(
        "party: [mira, odda, zed]\nloot: [sword, shield]\n",
        "@narrator: hi\n",
    ));
    assert_eq!(
        errors(&ds),
        vec![
            (
                "E-BAD-ENUM",
                "`odda` is not a member of entity kind `person` (attribute `party` of \
                 `frontmatter`) — did you mean `oda`?"
            ),
            (
                "E-BAD-ENUM",
                "`zed` is not a member of entity kind `person` (attribute `party` of \
                 `frontmatter`)"
            ),
            ("E-UNKNOWN-ID", "`shield` is not a known `items` id"),
        ],
        "{ds:#?}"
    );

    for front in [
        "lead: 3\n",
        "party: [mira, 3]\n",
        "prize: [sword]\n",
        "tone: { a: b }\n",
    ] {
        let ds = diagnostics(&scene(front, "@narrator: hi\n"));
        assert_eq!(
            errors(&ds).iter().map(|(c, _)| *c).collect::<Vec<_>>(),
            vec!["E-FRONTMATTER-SCHEMA"],
            "{front}: {ds:#?}"
        );
    }
}

/// #178: a domain-typed value is checked against the merged domain.
#[test]
fn a_domain_frontmatter_value_is_a_member_of_its_domain() {
    let ds = diagnostics(&scene("tone: nowhere\n", "@narrator: hi\n"));
    assert_eq!(
        errors(&ds).iter().map(|(c, _)| *c).collect::<Vec<_>>(),
        vec!["E-BAD-ENUM"],
        "{ds:#?}"
    );
}

/// #178: `enumFromOption` values resolve against the active option's
/// members, `assetKind` values against the kind's provider; lists check each
/// element; null and non-string values stay `E-FRONTMATTER-SCHEMA`.
#[test]
fn option_and_asset_frontmatter_values_are_checked() {
    let ok = diagnostics(&scene(
        "weather: rain\nskies: [sunny, rain]\nbackdrop: bg_home\nbackdrops: [bg_home]\n",
        "@narrator: hi\n",
    ));
    assert!(errors(&ok).is_empty(), "{ok:#?}");

    let bad = diagnostics(&scene(
        "weather: rainy\nskies: [sunny, snow]\nbackdrop: bg_nowhere\nbackdrops: [bg_home, bg_gone]\n",
        "@narrator: hi\n",
    ));
    let codes: Vec<&str> = errors(&bad).iter().map(|(c, _)| *c).collect();
    assert_eq!(
        codes.iter().filter(|c| **c == "E-BAD-ENUM").count(),
        2,
        "{bad:#?}"
    );
    assert!(
        coded(&bad, "E-BAD-ENUM")[0].contains("`rainy`")
            && coded(&bad, "E-BAD-ENUM")[0].contains("did you mean `rain`"),
        "{bad:#?}"
    );
    assert_eq!(codes.len(), 4, "two asset ids also fail: {bad:#?}");
    assert!(
        codes
            .iter()
            .all(|c| *c == "E-BAD-ENUM" || c.starts_with("E-ASSET")),
        "{bad:#?}"
    );

    for front in [
        "weather: ~\n",
        "weather: 3\n",
        "skies: [sunny, ~]\n",
        "skies: sunny\n",
        "backdrop: ~\n",
        "backdrops: [bg_home, 7]\n",
        "lead: ~\n",
        "party: [mira, ~]\n",
    ] {
        let ds = diagnostics(&scene(front, "@narrator: hi\n"));
        assert_eq!(
            errors(&ds).iter().map(|(c, _)| *c).collect::<Vec<_>>(),
            vec!["E-FRONTMATTER-SCHEMA"],
            "{front}: {ds:#?}"
        );
    }
}

const DEFS: &str = "defs:\n  n: { type: int, cel: \"1\" }\n  s: { type: string, cel: \"'x'\" }\n  off: { type: bool, cel: \"false\" }\n";

/// F2-R1: a `@ref` line `wait` is held to `bool`: an int or string constant
/// is reported (before, it compiled to `"1"`/`"x"` and silently vanished
/// from the record); a bool constant checks clean.
#[test]
fn a_line_wait_reference_must_be_a_bool() {
    for def in ["n", "s"] {
        let ds = diagnostics(&scene(DEFS, &format!("@narrator{{wait=@{def}}}: hello\n")));
        assert!(
            !errors(&ds).is_empty()
                && errors(&ds)
                    .iter()
                    .all(|(c, m)| m.contains("wait") || c == &"E-REF-TYPE"),
            "@{def}: {ds:#?}"
        );
    }
    let ok = diagnostics(&scene(DEFS, "@narrator{wait=@off}: hello\n"));
    assert!(errors(&ok).is_empty(), "{ok:#?}");
}

/// F2-R1: inside a component, a line `wait` bound to an int param is
/// reported, standalone and through a `::use`.
#[test]
fn a_component_line_wait_param_must_be_a_bool() {
    let dir = lute_scratch::ScratchDir::new();
    std::fs::write(
        dir.join("c.lute"),
        "---\ncomponent: c\nparams:\n  hold: int\n  go: bool\n---\n## Scene 1.\n\
         @narrator{wait=@hold}: a\n@narrator{wait=@go}: b\n",
    )
    .unwrap();
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\ncomponents: [c.lute]\n---\n\
                ## Shot 1.\n::use{component=\"c\" hold=\"1\" go=\"false\"}\n"
        .to_string();
    let (doc, _) = lute_syntax::parse(&text);
    let (meta0, _) = lute_check::parse_meta(&doc.meta, &CapabilitySnapshot::default());
    let components = lute_check::resolve_components(&dir, &meta0.components, doc.meta.span);
    let input = CheckInput::new(
        text,
        "scene".into(),
        Mode::Ci,
        lute_check::InputSources::from_parts(
            snapshot(),
            providers(),
            SchemaImports::default(),
            components,
            Default::default(),
        ),
    );
    let ds = check(&input).diagnostics;
    let refs: Vec<&str> = errors(&ds)
        .into_iter()
        .filter(|(c, _)| *c == "E-REF-TYPE")
        .map(|(_, m)| m)
        .collect();
    assert_eq!(refs.len(), 1, "only `@hold` is no bool: {ds:#?}");
    assert!(refs[0].contains("hold"), "{refs:?}");
}
