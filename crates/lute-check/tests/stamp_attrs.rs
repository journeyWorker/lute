//! plugin §14.1 `stampAttrs` — CROSS-CUTTING attr admission.
//!
//! A plugin-declared stamp attr is admissible on EVERY directive (core and
//! plugin) AND on content lines, in addition to that surface's own attrs.
//! Resolution order: the surface's own decls win, then `snapshot.stamp_attrs`,
//! then `E-UNKNOWN-ATTR`. Value typing rides the ordinary attr-type path, so a
//! mistyped stamp attr is a plain `E-ATTR-TYPE`.

use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;
use lute_manifest::schema::{AttrDecl, DirectiveDecl, Lowering};
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::Type;

const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n";

/// Core snapshot + a plugin directive `p` (own attr `label`) + the Stage-shaped
/// cross-cutting vocabulary `bonusId: string` / `bonusScore: number`.
fn snap() -> CapabilitySnapshot {
    let mut snap = lute_manifest::core::load_core_snapshot();
    snap.directives.insert(
        "p".to_string(),
        DirectiveDecl {
            name: "p".to_string(),
            layer: None,
            attrs: vec![AttrDecl {
                name: "label".to_string(),
                required: false,
                ty: Type::Str,
                default: None,
            }],
            semantics: Vec::new(),
            state: None,
            effects: None,
            bridge: None,
            lower: Lowering::Builtin {
                kind: "builtin".to_string(),
                name: "noop".to_string(),
            },
        },
    );
    for (name, ty) in [("bonusId", Type::Str), ("bonusScore", Type::Int)] {
        snap.stamp_attrs
            .insert(lute_manifest::stamps::StampAttrDecl::cross_cutting(
                "test",
                AttrDecl {
                    name: name.to_string(),
                    required: false,
                    ty,
                    default: None,
                },
            ))
            .unwrap();
    }
    snap
}

fn codes_against(text: &str, snapshot: CapabilitySnapshot) -> Vec<String> {
    let input = CheckInput::new(
        text.to_string(),
        "stamp_attrs".into(),
        Mode::Author,
        lute_check::InputSources::from_parts(
            snapshot,
            ProviderSet::default(),
            SchemaImports::default(),
            Default::default(),
            Default::default(),
        ),
    );
    check(&input)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn codes(text: &str) -> Vec<String> {
    codes_against(text, snap())
}

/// The one `E-ATTR-TYPE` message this snapshot produces, or a panic naming
/// every code that fired.
fn attr_type_message(text: &str) -> String {
    let input = CheckInput::new(
        text.to_string(),
        "stamp_attrs".into(),
        Mode::Author,
        lute_check::InputSources::from_parts(
            snap(),
            ProviderSet::default(),
            SchemaImports::default(),
            Default::default(),
            Default::default(),
        ),
    );
    let all = check(&input).diagnostics;
    all.iter().find(|d| d.code == "E-ATTR-TYPE").map_or_else(
        || {
            panic!(
                "no E-ATTR-TYPE in {:?}",
                all.iter().map(|d| &d.code).collect::<Vec<_>>()
            )
        },
        |d| d.message.clone(),
    )
}

#[test]
fn stamp_attr_is_accepted_on_a_core_directive() {
    let t = format!("{HDR}::bg{{location=\"cafe\" bonusId=\"b1\"}}\n@narrator: hi\n");
    assert!(
        !codes(&t).contains(&"E-UNKNOWN-ATTR".to_string()),
        "a declared stampAttr must be admissible on `::bg`; got {:?}",
        codes(&t)
    );
}

#[test]
fn stamp_attr_is_accepted_on_a_content_line() {
    let t = format!("{HDR}@elena{{bonusId=\"b1\"}}: text\n");
    assert!(
        !codes(&t).contains(&"E-UNKNOWN-ATTR".to_string()),
        "a declared stampAttr must be admissible on a content line; got {:?}",
        codes(&t)
    );
}

#[test]
fn stamp_attr_is_accepted_on_a_plugin_directive() {
    let t = format!("{HDR}::p{{label=\"x\" bonusScore=\"7\"}}\n@narrator: hi\n");
    assert!(
        !codes(&t).contains(&"E-UNKNOWN-ATTR".to_string()),
        "a declared stampAttr must be admissible on a plugin directive; got {:?}",
        codes(&t)
    );
}

#[test]
fn undeclared_attr_is_still_unknown_on_both_surfaces() {
    let dir = format!("{HDR}::bg{{location=\"cafe\" nopeId=\"b1\"}}\n@narrator: hi\n");
    assert!(
        codes(&dir).contains(&"E-UNKNOWN-ATTR".to_string()),
        "an UNdeclared attr on a directive must still be E-UNKNOWN-ATTR; got {:?}",
        codes(&dir)
    );
    let line = format!("{HDR}@elena{{nopeId=\"b1\"}}: text\n");
    assert!(
        codes(&line).contains(&"E-UNKNOWN-ATTR".to_string()),
        "an UNdeclared attr on a content line must still be E-UNKNOWN-ATTR; got {:?}",
        codes(&line)
    );
}

#[test]
fn stamp_attr_without_the_declaration_is_unknown() {
    // The SAME documents, against a snapshot with no cross-cutting vocabulary:
    // admission comes from the declaration, not from a hardcoded allowlist.
    let core = lute_manifest::core::load_core_snapshot();
    let t = format!("{HDR}::bg{{location=\"cafe\" bonusId=\"b1\"}}\n@narrator: hi\n");
    assert!(
        codes_against(&t, core.clone()).contains(&"E-UNKNOWN-ATTR".to_string()),
        "without a `stampAttrs` declaration `bonusId` must be E-UNKNOWN-ATTR"
    );
    let l = format!("{HDR}@elena{{bonusId=\"b1\"}}: text\n");
    assert!(
        codes_against(&l, core).contains(&"E-UNKNOWN-ATTR".to_string()),
        "without a `stampAttrs` declaration `bonusId` must be E-UNKNOWN-ATTR on a content line"
    );
}

#[test]
fn mistyped_stamp_attr_is_an_ordinary_attr_type_error() {
    // `bonusScore` is declared `number`; a non-numeric value rides the existing
    // attr-type path (E-ATTR-TYPE), NOT a bespoke stamp-attr diagnostic.
    let dir = format!("{HDR}::bg{{location=\"cafe\" bonusScore=\"nope\"}}\n@narrator: hi\n");
    assert!(
        codes(&dir).contains(&"E-ATTR-TYPE".to_string()),
        "a mistyped stampAttr must be E-ATTR-TYPE; got {:?}",
        codes(&dir)
    );
    let line = format!("{HDR}@elena{{bonusScore=\"nope\"}}: text\n");
    assert!(
        codes(&line).contains(&"E-ATTR-TYPE".to_string()),
        "a mistyped stampAttr on a content line must be E-ATTR-TYPE; got {:?}",
        codes(&line)
    );
}

#[test]
fn a_directives_own_attr_wins_over_a_same_named_stamp_attr() {
    // `label` is `::p`'s OWN attr (string) AND — here — also a declared stamp
    // attr typed `int`. The directive's own decl must win, so a string value
    // is clean rather than E-ATTR-TYPE against the int stamp decl.
    let mut s = snap();
    s.stamp_attrs
        .insert(lute_manifest::stamps::StampAttrDecl::cross_cutting(
            "test",
            AttrDecl {
                name: "label".to_string(),
                required: false,
                ty: Type::Int,
                default: None,
            },
        ))
        .unwrap();
    let t = format!("{HDR}::p{{label=\"x\"}}\n@narrator: hi\n");
    assert!(
        !codes_against(&t, s).contains(&"E-ATTR-TYPE".to_string()),
        "the directive's own `label` decl must win over a same-named stampAttr"
    );
}

#[test]
fn a_content_lines_own_attr_wins_over_a_same_named_stamp_attr() {
    // `variant` is a built-in content-line key; declaring it as a stamp attr
    // typed `bool` must not retype it — the content-line surface owns it.
    let mut s = snap();
    s.stamp_attrs
        .insert(lute_manifest::stamps::StampAttrDecl::cross_cutting(
            "test",
            AttrDecl {
                name: "variant".to_string(),
                required: false,
                ty: Type::Bool,
                default: None,
            },
        ))
        .unwrap();
    let t = format!("{HDR}@elena{{variant=\"2\"}}: text\n");
    let got = codes_against(&t, s);
    assert!(
        !got.contains(&"E-ATTR-TYPE".to_string()) && !got.contains(&"E-UNKNOWN-ATTR".to_string()),
        "the content line's own `variant` must win over a same-named stampAttr; got {got:?}"
    );
}

/// #33 / T1.4: `E-ATTR-TYPE` reaches a content line through the SAME
/// `check_attr_value` a directive uses, so it inherited the same defect —
/// the hardcoded `::` sigil named a `::narrator` that exists in no document,
/// no grammar and no `lute context` listing. This is the only content-line
/// surface that reaches `E-ATTR-TYPE` (the built-in content attrs are not
/// value-typed), which is why the probe is a mistyped stamp attr.
#[test]
fn attr_type_names_the_owning_construct_with_its_real_sigil() {
    let line = attr_type_message(&format!("{HDR}@narrator{{bonusScore=\"nope\"}}: hi\n"));
    assert!(
        line.contains("of `@narrator`"),
        "a content line's owner renders `@speaker`, never `::speaker`: {line}"
    );
    assert!(!line.contains("::narrator"), "{line}");

    let dir = attr_type_message(&format!(
        "{HDR}::bg{{location=\"cafe\" bonusScore=\"nope\"}}\n@narrator: hi\n"
    ));
    assert!(
        dir.contains("of `::bg`"),
        "a directive's owner keeps the `::` sigil: {dir}"
    );
}

/// Core snapshot + `acme.stage`'s scoped stamps (0.50.0 §3.1): `assetName`
/// required on `::bg`, `voiceAssetId` on content lines, `menuType` on
/// `<branch>`, and the cross-cutting `bonusId` of [`snap`].
fn scoped_snap() -> CapabilitySnapshot {
    use lute_manifest::stamps::{StampAttrDecl, StampSurface};
    let mut s = snap();
    let decl = |name: &str, required: bool| AttrDecl {
        name: name.to_string(),
        required,
        ty: Type::Str,
        default: Some(lute_manifest::types::Literal::Str("x".into())),
    };
    for (d, on) in [
        (
            decl("assetName", true),
            StampSurface::Directive("bg".into()),
        ),
        (decl("voiceAssetId", false), StampSurface::Line),
        (decl("menuType", false), StampSurface::Branch),
    ] {
        s.stamp_attrs
            .insert(StampAttrDecl::scoped("acme.stage", d, [on]))
            .unwrap();
    }
    s
}

fn messages_against(text: &str, snapshot: CapabilitySnapshot) -> Vec<(String, String)> {
    let input = CheckInput::new(
        text.to_string(),
        "stamp_attrs".into(),
        Mode::Author,
        lute_check::InputSources::from_parts(
            snapshot,
            ProviderSet::default(),
            SchemaImports::default(),
            Default::default(),
            Default::default(),
        ),
    );
    check(&input)
        .diagnostics
        .into_iter()
        .map(|d| (d.code, d.message))
        .collect()
}

/// 0.50.0 §3.4: a stamp required on a directive must be authored there; a
/// declared `default:` does not count, since a stamp is never injected.
#[test]
fn a_required_directive_stamp_must_be_authored() {
    let missing = messages_against(
        &format!("{HDR}::bg{{location=\"cafe\"}}\n@narrator{{voiceAssetId=\"v\"}}: hi\n"),
        scoped_snap(),
    );
    assert_eq!(
        missing,
        [(
            "E-MISSING-ATTR".to_string(),
            "`::bg` requires attribute `assetName` (a stamp of `acme.stage`)".to_string()
        )]
    );
    let authored = messages_against(
        &format!("{HDR}::bg{{location=\"cafe\" assetName=\"room\"}}\n@narrator: hi\n"),
        scoped_snap(),
    );
    assert!(authored.is_empty(), "{authored:?}");
}

/// 0.50.0 §3.4: a stamp scoped elsewhere names where it applies — on a
/// directive and on a content line; a cross-cutting stamp keeps its 0.49
/// surfaces.
#[test]
fn a_stamp_scoped_elsewhere_names_its_surfaces() {
    let got = messages_against(
        &format!(
            "{HDR}::sfx{{assetId=\"s\" voiceAssetId=\"v\" bonusId=\"b\"}}\n@narrator{{assetName=\"a\" \
             menuType=\"m\" bonusId=\"b\"}}: hi\n"
        ),
        scoped_snap(),
    );
    assert_eq!(
        got,
        [
            (
                "E-UNKNOWN-ATTR".to_string(),
                "`::sfx` has no attribute `voiceAssetId`: it is a stamp of `acme.stage` for \
                 content lines only"
                    .to_string()
            ),
            (
                "E-UNKNOWN-ATTR".to_string(),
                "a content line has no attribute `assetName`: it is a stamp of `acme.stage` for \
                 `::bg` only"
                    .to_string()
            ),
            (
                "E-UNKNOWN-ATTR".to_string(),
                "a content line has no attribute `menuType`: it is a stamp of `acme.stage` for \
                 `<branch>` only"
                    .to_string()
            ),
        ]
    );
}
