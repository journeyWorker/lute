//! plugin §14.1 `stampAttrs` at lowering: a plugin-declared CROSS-CUTTING attr
//! rides the record's STAMP (`Stamp.extra`), typed by its `AttrDecl`, instead
//! of the record's own fields — on core directives, plugin directives, and
//! content lines alike. A document that authors none is byte-identical to one
//! compiled against a snapshot that never declared the vocabulary.

use lute_check::{CheckInput, Mode};
use lute_compile::compile;
use lute_manifest::schema::{AttrDecl, DirectiveDecl, Lowering};
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::Type;

/// Core snapshot + a plugin directive `p` (own attr `label`) + the OSHiZ-shaped
/// cross-cutting vocabulary `bonusId: string` / `bonusScore: number`.
fn snap_with_stamp_attrs() -> CapabilitySnapshot {
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

fn input(text: &str, snapshot: CapabilitySnapshot) -> CheckInput {
    CheckInput {
        text: text.to_string(),
        uri: "stamp_attrs".into(),
        snapshot,
        providers: Default::default(),
        mode: Mode::Ci,
        imports: Default::default(),
        components: Default::default(),
    }
}

fn commands(text: &str, snapshot: CapabilitySnapshot) -> Vec<serde_json::Value> {
    let artifact = compile(&input(text, snapshot)).expect("compiles clean");
    let json = serde_json::to_value(&artifact).expect("artifact serializes");
    json["commands"].as_array().expect("commands array").clone()
}

fn find<'a>(cmds: &'a [serde_json::Value], kind: &str) -> &'a serde_json::Value {
    cmds.iter()
        .find(|c| c["kind"] == kind)
        .unwrap_or_else(|| panic!("no `{kind}` command in {cmds:#?}"))
}

const HDR: &str = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n\n## Shot 1.\n\n";

#[test]
fn stamp_attrs_land_in_the_stamp_typed_by_their_decl() {
    let text = format!("{HDR}::sfx{{assetId=\"x\" bonusId=\"b1\" bonusScore=\"7\"}}\n@narrator: hi\n");
    let cmds = commands(&text, snap_with_stamp_attrs());
    let sfx = find(&cmds, "sfx");
    assert_eq!(sfx["assetId"], serde_json::json!("x"));
    // Flattened alongside the reserved stamp keys, NOT nested and NOT a field.
    assert_eq!(sfx["bonusId"], serde_json::json!("b1"));
    // Typed by the decl: `number` is a JSON number, not the authored string.
    assert_eq!(sfx["bonusScore"], serde_json::json!(7.0));
    assert!(
        sfx["bonusScore"].is_number(),
        "`bonusScore` must be typed by its AttrDecl, got {}",
        sfx["bonusScore"]
    );
}

#[test]
fn stamp_attrs_land_on_a_content_line_record() {
    let text = format!("{HDR}@sofia{{bonusId=\"b1\" bonusScore=\"7\"}}: text\n");
    let cmds = commands(&text, snap_with_stamp_attrs());
    let line = find(&cmds, "line");
    assert_eq!(line["bonusId"], serde_json::json!("b1"));
    assert_eq!(line["bonusScore"], serde_json::json!(7.0));
}

#[test]
fn stamp_attrs_bypass_the_plugin_passthrough_fields_map() {
    // On a plugin directive the record's OWN attr stays in `fields`; the
    // cross-cutting ones move to the stamp and must NOT be duplicated.
    let text = format!("{HDR}::p{{label=\"x\" bonusId=\"b1\"}}\n@narrator: hi\n");
    let cmds = commands(&text, snap_with_stamp_attrs());
    let p = find(&cmds, "plugin");
    assert_eq!(p["fields"]["label"], serde_json::json!("x"));
    assert!(
        p["fields"].get("bonusId").is_none(),
        "a cross-cutting stamp attr must not also appear in `fields`: {p:#?}"
    );
    assert_eq!(p["bonusId"], serde_json::json!("b1"));
}

#[test]
fn unauthored_stamp_attrs_are_never_injected() {
    // A decl with a `default` must NOT materialize on a record that never
    // authored the attr — absent means absent (byte-stability).
    let mut snap = snap_with_stamp_attrs();
    snap.stamp_attrs.get_mut("bonusId").unwrap().default =
        Some(lute_manifest::types::Literal::Str("fallback".into()));
    let text = format!("{HDR}::sfx{{assetId=\"x\"}}\n@narrator: hi\n");
    let cmds = commands(&text, snap);
    let sfx = find(&cmds, "sfx");
    assert!(
        sfx.get("bonusId").is_none(),
        "an unauthored stamp attr must never be default-injected: {sfx:#?}"
    );
}

#[test]
fn a_document_authoring_none_is_byte_identical() {
    // The SAME document, compiled against a snapshot that declares the
    // cross-cutting vocabulary and one that does not, must produce identical
    // command bytes. (`capabilityVersion` legitimately differs — a changed
    // capability surface — so the comparison is scoped to `commands`.)
    let text = format!(
        "{HDR}::bg{{location=\"cafe\" time=\"night\"}}\n\
         ::sfx{{assetId=\"x\"}}\n\
         @sofia{{emotion=\"content\"}}: hello\n\
         @narrator: done\n"
    );
    let plain = commands(&text, lute_manifest::core::load_core_snapshot());
    let declared = commands(&text, snap_with_stamp_attrs());
    assert_eq!(
        serde_json::to_string(&plain).unwrap(),
        serde_json::to_string(&declared).unwrap(),
        "declaring a `stampAttrs` vocabulary must not perturb a document that authors none"
    );
}
