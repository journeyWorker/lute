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

/// Core snapshot + a plugin directive `p` (own attr `label`) + the OSHiZ-shaped
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
    for (name, ty) in [("bonusId", Type::Str), ("bonusScore", Type::Number)] {
        snap.stamp_attrs.insert(
            name.to_string(),
            AttrDecl {
                name: name.to_string(),
                required: false,
                ty,
                default: None,
            },
        );
    }
    snap
}

fn codes_against(text: &str, snapshot: CapabilitySnapshot) -> Vec<String> {
    let input = CheckInput {
        text: text.to_string(),
        uri: "stamp_attrs".into(),
        snapshot,
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
    };
    check(&input)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn codes(text: &str) -> Vec<String> {
    codes_against(text, snap())
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
    let t = format!("{HDR}@sofia{{bonusId=\"b1\"}}: text\n");
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
    let line = format!("{HDR}@sofia{{nopeId=\"b1\"}}: text\n");
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
    let l = format!("{HDR}@sofia{{bonusId=\"b1\"}}: text\n");
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
    let line = format!("{HDR}@sofia{{bonusScore=\"nope\"}}: text\n");
    assert!(
        codes(&line).contains(&"E-ATTR-TYPE".to_string()),
        "a mistyped stampAttr on a content line must be E-ATTR-TYPE; got {:?}",
        codes(&line)
    );
}

#[test]
fn a_directives_own_attr_wins_over_a_same_named_stamp_attr() {
    // `label` is `::p`'s OWN attr (string) AND — here — also a declared stamp
    // attr typed `number`. The directive's own decl must win, so a string value
    // is clean rather than E-ATTR-TYPE against the number stamp decl.
    let mut s = snap();
    s.stamp_attrs.insert(
        "label".to_string(),
        AttrDecl {
            name: "label".to_string(),
            required: false,
            ty: Type::Number,
            default: None,
        },
    );
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
    s.stamp_attrs.insert(
        "variant".to_string(),
        AttrDecl {
            name: "variant".to_string(),
            required: false,
            ty: Type::Bool,
            default: None,
        },
    );
    let t = format!("{HDR}@sofia{{variant=\"2\"}}: text\n");
    let got = codes_against(&t, s);
    assert!(
        !got.contains(&"E-ATTR-TYPE".to_string()) && !got.contains(&"E-UNKNOWN-ATTR".to_string()),
        "the content line's own `variant` must win over a same-named stampAttr; got {got:?}"
    );
}
