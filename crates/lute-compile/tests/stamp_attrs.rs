//! plugin §14.1 `stampAttrs` at lowering: a plugin-declared CROSS-CUTTING attr
//! rides the record's STAMP (`Stamp.stamps`, serialized as the nested
//! `stamps` object, 0.49.0 §3.1), typed by its `AttrDecl`, instead of the
//! record's own fields — on core directives, plugin directives, and content
//! lines alike. A document that authors none is byte-identical to one
//! compiled against a snapshot that never declared the vocabulary.

use std::collections::BTreeMap;

use lute_check::{check, CheckInput, Mode};
use lute_compile::compile;
use lute_core_span::Severity;
use lute_manifest::assemble::assemble_snapshot;
use lute_manifest::loader::LoadedPlugin;
use lute_manifest::resolve::{ActivePlugin, InstalledPlugin, InstalledPlugins};
use lute_manifest::schema::{AttrDecl, DirectiveDecl, Lowering, PluginManifest};
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::Type;

/// Core snapshot + a plugin directive `p` (own attr `label`) + the Stage-shaped
/// cross-cutting vocabulary `bonusId: string` / `bonusScore: number`.
fn snap_with_stamp_attrs() -> CapabilitySnapshot {
    let mut snap = lute_test_vocab::vocab_snapshot();
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
    for (name, ty) in [("bonusId", Type::Str), ("bonusScore", Type::Double)] {
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

fn input(text: &str, snapshot: CapabilitySnapshot) -> CheckInput {
    CheckInput::new(
        text.to_string(),
        "stamp_attrs".into(),
        Mode::Ci,
        lute_check::InputSources::from_parts(
            snapshot,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        ),
    )
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
    let text =
        format!("{HDR}::sfx{{assetId=\"x\" bonusId=\"b1\" bonusScore=\"7\"}}\n@narrator: hi\n");
    let cmds = commands(&text, snap_with_stamp_attrs());
    let sfx = find(&cmds, "sfx");
    assert_eq!(sfx["assetId"], serde_json::json!("x"));
    // Nested under `stamps` (0.49.0 §3.1), NOT a top-level key and NOT a field.
    assert_eq!(sfx["stamps"]["bonusId"], serde_json::json!("b1"));
    assert!(sfx.get("bonusId").is_none(), "{sfx:#?}");
    // Typed by the decl: `number` is a JSON number, not the authored string.
    assert_eq!(sfx["stamps"]["bonusScore"], serde_json::json!(7.0));
    assert!(
        sfx["stamps"]["bonusScore"].is_number(),
        "`bonusScore` must be typed by its AttrDecl, got {}",
        sfx["stamps"]["bonusScore"]
    );
}

#[test]
fn stamp_attrs_land_on_a_content_line_record() {
    let text = format!("{HDR}@elena{{bonusId=\"b1\" bonusScore=\"7\"}}: text\n");
    let cmds = commands(&text, snap_with_stamp_attrs());
    let line = find(&cmds, "line");
    assert_eq!(line["stamps"]["bonusId"], serde_json::json!("b1"));
    assert_eq!(line["stamps"]["bonusScore"], serde_json::json!(7.0));
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
    assert_eq!(p["stamps"]["bonusId"], serde_json::json!("b1"));
}

#[test]
fn unauthored_stamp_attrs_are_never_injected() {
    // A decl with a `default` must NOT materialize on a record that never
    // authored the attr — absent means absent (byte-stability).
    let mut snap = snap_with_stamp_attrs();
    let surface = lute_manifest::stamps::StampSurface::Line;
    let mut bonus = snap.stamp_attrs.get(&surface, "bonusId").unwrap().clone();
    bonus.default = Some(lute_manifest::types::Literal::Str("fallback".into()));
    snap.stamp_attrs = [lute_manifest::stamps::StampAttrDecl::cross_cutting(
        "test", bonus,
    )]
    .into_iter()
    .collect();
    let text = format!("{HDR}::sfx{{assetId=\"x\"}}\n@narrator: hi\n");
    let cmds = commands(&text, snap);
    let sfx = find(&cmds, "sfx");
    assert!(
        sfx.get("bonusId").is_none() && sfx.get("stamps").is_none(),
        "an unauthored stamp attr must never be default-injected: {sfx:#?}"
    );
}

#[test]
fn a_document_authoring_none_is_byte_identical() {
    // The SAME document, compiled against a snapshot that declares the
    // cross-cutting vocabulary and one that does not, must produce identical
    // command bytes. (`capabilitySnapshot` legitimately differs — a changed
    // capability surface — so the comparison is scoped to `commands`.)
    let text = format!(
        "{HDR}::bg{{location=\"cafe\" time=\"night\"}}\n\
         ::sfx{{assetId=\"x\"}}\n\
         @elena{{emotion=\"content\"}}: hello\n\
         @narrator: done\n"
    );
    let plain = commands(&text, lute_test_vocab::vocab_snapshot());
    let declared = commands(&text, snap_with_stamp_attrs());
    assert_eq!(
        serde_json::to_string(&plain).unwrap(),
        serde_json::to_string(&declared).unwrap(),
        "declaring a `stampAttrs` vocabulary must not perturb a document that authors none"
    );
}

/// A REAL third-party plugin package whose `stampAttrs` export declares
/// `pose: string`, merged through [`assemble_snapshot`] so this test PROVES
/// assembly admits the name rather than assuming it — `pose` is absent from
/// `lute-manifest`'s `RESERVED_STAMP_ATTR_NAMES`, and that is the only guard
/// on the export. The shared test vocabulary is folded in afterwards exactly
/// as `lute_test_vocab::vocab_snapshot` does, because the fixture's `::actor`
/// needs the `action`/`anchor` domains and the core snapshot ships no members
/// (dsl 0.9.0 D-A).
fn snap_with_plugin_declared_pose() -> CapabilitySnapshot {
    assembled_snapshot(Vec::new(), &["pose"])
}

/// A third-party plugin `third.party` exporting `directives` and one `string`
/// `stampAttrs` entry per name in `stamps`, assembled onto `lute.core`
/// through [`assemble_snapshot`] (which must admit it), plus the shared test
/// vocabulary.
fn assembled_snapshot(directives: Vec<DirectiveDecl>, stamps: &[&str]) -> CapabilitySnapshot {
    let pkg = LoadedPlugin {
        manifest: PluginManifest {
            id: "third.party".into(),
            version: "0.1.0".into(),
            kind: "capability".into(),
            depends: Vec::new(),
            exports: BTreeMap::new(),
            options: Vec::new(),
        },
        directives,
        enums: BTreeMap::new(),
        state_shapes: Vec::new(),
        state_templates: Vec::new(),
        providers: Vec::new(),
        bridge: Vec::new(),
        defs: Vec::new(),
        frontmatter: BTreeMap::new(),
        asset_kinds: Vec::new(),
        events: Vec::new(),
        // `stampAttrs:\n  - name: <name>\n    type: string`, per name
        stamp_attrs: stamps
            .iter()
            .map(|name| lute_manifest::stamps::DeclaredStampAttr {
                decl: AttrDecl {
                    name: (*name).to_string(),
                    required: false,
                    ty: Type::Str,
                    default: None,
                },
                on: None,
                site: None,
            })
            .collect(),
        reward_kinds: Vec::new(),
        occasions: Vec::new(),
        lints: Vec::new(),
        cast: Vec::new(),
        sites: Default::default(),
        invalid: Vec::new(),
    };
    let installed = InstalledPlugins {
        by_id: BTreeMap::from([("third.party".to_string(), InstalledPlugin { loaded: pkg })]),
        ..Default::default()
    };
    let active = vec![
        ActivePlugin {
            id: "lute.core".into(),
            options: BTreeMap::new(),
        },
        ActivePlugin {
            id: "third.party".into(),
            options: BTreeMap::new(),
        },
    ];
    let (mut snap, errs) = assemble_snapshot(&active, &installed);
    assert!(
        errs.is_empty(),
        "a third-party plugin must be free to own {stamps:?} — nothing reserves them: {errs:?}"
    );
    for name in stamps {
        assert!(
            snap.stamp_attrs.contains(name),
            "the `{name}` stampAttr must merge into the snapshot, got {:?}",
            snap.stamp_attrs.names().collect::<Vec<_>>()
        );
    }
    for (name, dom) in lute_test_vocab::test_domains() {
        snap.enums.insert(name.clone(), dom.members.clone());
        snap.domains.insert(name, dom);
    }
    snap.version = lute_manifest::snapshot::capability_version(&snap);
    snap
}

/// dsl 0.9.0 / plugin §14.1 — `pose` is DELIBERATELY left unreserved, and the
/// guarantee that makes leaving it unreserved safe is **stamp-only lowering**.
///
/// `pose` is absent from `lute_manifest::core::LINE_ATTRS`, so `@x{pose="…"}`
/// is normally `E-UNKNOWN-ATTR`. That arm has exactly ONE escape hatch: a
/// plugin-declared cross-cutting `stampAttrs` entry. And `pose` is likewise
/// absent from `assemble.rs`'s `RESERVED_STAMP_ATTR_NAMES`, so a third-party
/// plugin MAY own the name — and *should* be allowed to, because plugin
/// metadata is the plugin's own business.
///
/// That is only correct because a `stampAttr` lands in `Stamp.stamps` and
/// carries NO sprite semantics. Task 7 deleted `lute-check/src/inject.rs`'s
/// two `pose` reads (`line_is_stateful`, and the `stage_bookkeeping_line`
/// pose write) as unreachable; a plugin declaring `pose` would have made them
/// reachable, and the reducer would then have silently reinterpreted that
/// plugin's metadata as sprite state — dirtying the speaker and injecting a
/// bogus `posReset`. This test pins the property the deletion depends on,
/// asserted against the compiled ARTIFACT rather than reducer internals: the
/// value is stamp metadata, and nothing else.
#[test]
fn a_plugin_declared_pose_lowers_to_the_stamp_and_never_to_sprite_state() {
    let snap = snap_with_plugin_declared_pose();
    let text = format!(
        "{HDR}::actor{{character=\"elena\" anchor=\"center\" action=\"fadeInUp\"}}\n\
         @elena{{pose=\"crouch\"}}: A!\n\
         @elena: B.\n"
    );

    // (1) A SUPPORTED authoring path, not an error: the `stampAttrs` escape
    // hatch in the unknown-attr arm admits `pose` on a content line.
    let diags = check(&input(&text, snap.clone())).diagnostics;
    assert!(
        diags.iter().all(|d| d.severity != Severity::Error),
        "a plugin-declared `pose` must check clean on a content line: {diags:#?}"
    );

    let cmds = commands(&text, snap.clone());
    let lines: Vec<&serde_json::Value> = cmds.iter().filter(|c| c["kind"] == "line").collect();
    assert_eq!(lines.len(), 2, "{cmds:#?}");

    // (2) The value rides the record's STAMP, nested like any other
    // cross-cutting attr — and only on the record that authored it.
    assert_eq!(lines[0]["stamps"]["pose"], serde_json::json!("crouch"));
    assert!(
        lines[1].get("stamps").is_none(),
        "the stamp attr must not leak onto the next record: {:#?}",
        lines[1]
    );

    // (3) It is NOT sprite state. It must not be read as the record's own
    // sprite slot (`action`)…
    assert!(
        lines[0].get("action").is_none(),
        "a stamp attr must never become the record's sprite `action` field: {:#?}",
        lines[0]
    );
    // …and it must not dirty the speaker's sprite, so the following PLAIN
    // line receives no injected `posReset`, and no sprite record carries the
    // authored value.
    let sprites: Vec<&serde_json::Value> = cmds.iter().filter(|c| c["kind"] == "actor").collect();
    assert!(
        !sprites
            .iter()
            .any(|s| s["posReset"] == serde_json::json!(true)),
        "a stamp attr must not dirty the sprite — no `posReset` may be injected: {cmds:#?}"
    );
    assert!(
        sprites
            .iter()
            .all(|s| s.get("pose").is_none() && s["action"] != serde_json::json!("crouch")),
        "no sprite record may carry the stamp attr's value: {sprites:#?}"
    );

    // Control, so the assertion above cannot pass vacuously: the reducer's
    // `auto-pose-reset` path IS live for this exact document shape. Swap
    // `pose=` for the real sprite slot `action=` and the plain line DOES get a
    // `posReset` — so (3)'s absence is a property of stamp-only lowering, not
    // an accident of the fixture.
    let sprite_slot = text.replace("pose=\"crouch\"", "action=\"poseLean\"");
    let control = commands(&sprite_slot, snap);
    assert!(
        control
            .iter()
            .any(|c| c["kind"] == "actor" && c["posReset"] == serde_json::json!(true)),
        "control: the real sprite slot must still inject a `posReset`: {control:#?}"
    );
}

/// A JSON value walk that FAILS on a repeated object key — Python's
/// `json.loads(…, object_pairs_hook=…)` check of 0.49.0 §3.1. `serde_json`'s
/// own `Value` keeps the last of two equal keys silently, which is exactly
/// how #155/#156 went unseen.
struct NoDuplicateKeys;

impl<'de> serde::Deserialize<'de> for NoDuplicateKeys {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(NoDuplicateKeysVisitor)
    }
}

struct NoDuplicateKeysVisitor;

impl<'de> serde::de::Visitor<'de> for NoDuplicateKeysVisitor {
    type Value = NoDuplicateKeys;
    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("any JSON value")
    }
    fn visit_bool<E>(self, _: bool) -> Result<Self::Value, E> {
        Ok(NoDuplicateKeys)
    }
    fn visit_i64<E>(self, _: i64) -> Result<Self::Value, E> {
        Ok(NoDuplicateKeys)
    }
    fn visit_u64<E>(self, _: u64) -> Result<Self::Value, E> {
        Ok(NoDuplicateKeys)
    }
    fn visit_f64<E>(self, _: f64) -> Result<Self::Value, E> {
        Ok(NoDuplicateKeys)
    }
    fn visit_str<E>(self, _: &str) -> Result<Self::Value, E> {
        Ok(NoDuplicateKeys)
    }
    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(NoDuplicateKeys)
    }
    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        while seq.next_element::<NoDuplicateKeys>()?.is_some() {}
        Ok(NoDuplicateKeys)
    }
    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut seen = std::collections::BTreeSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !seen.insert(key.clone()) {
                return Err(serde::de::Error::custom(format!("duplicate key `{key}`")));
            }
            map.next_value::<NoDuplicateKeys>()?;
        }
        Ok(NoDuplicateKeys)
    }
}

/// The compiled artifact as `lute compile` writes it, checked for duplicate
/// keys, then parsed.
fn strict_commands(text: &str, snapshot: CapabilitySnapshot) -> Vec<serde_json::Value> {
    let artifact = compile(&input(text, snapshot)).expect("compiles clean");
    let json = serde_json::to_string(&artifact).expect("artifact serializes");
    if let Err(e) = serde_json::from_str::<NoDuplicateKeys>(&json) {
        panic!("the compiled JSON repeats a key: {e}\n{json}");
    }
    let value: serde_json::Value = serde_json::from_str(&json).expect("artifact parses");
    value["commands"]
        .as_array()
        .expect("commands array")
        .clone()
}

/// #155: a plugin stamp named like its carrier's own key (`tag` on a
/// `plugin` record, `kind`/`position` on any record) no longer writes that
/// key twice: the record keeps its key and the stamp sits under `stamps`
/// (0.49.0 §3.1).
#[test]
fn a_stamp_named_like_a_record_key_nests_instead_of_repeating_it() {
    let signal = DirectiveDecl {
        name: "signal".to_string(),
        layer: None,
        attrs: vec![AttrDecl {
            name: "value".to_string(),
            required: false,
            ty: Type::Str,
            default: None,
        }],
        semantics: Vec::new(),
        state: None,
        effects: None,
        bridge: None,
        lower: Lowering::Passthrough,
    };
    let snap = assembled_snapshot(vec![signal], &["tag", "kind", "position"]);
    let text = format!(
        "{HDR}::signal{{value=\"puzzle\" tag=\"warmup\"}}\n\
         ::sfx{{assetId=\"x\" kind=\"loud\" position=\"left\"}}\n\
         @narrator: hi\n"
    );
    let cmds = strict_commands(&text, snap);
    let plugin = find(&cmds, "plugin");
    assert_eq!(plugin["tag"], serde_json::json!("signal"));
    assert_eq!(plugin["fields"], serde_json::json!({ "value": "puzzle" }));
    assert_eq!(plugin["stamps"], serde_json::json!({ "tag": "warmup" }));
    let sfx = find(&cmds, "sfx");
    assert_eq!(sfx["kind"], serde_json::json!("sfx"));
    assert!(
        sfx["position"].as_str().is_some_and(|p| p != "left"),
        "{sfx:#?}"
    );
    assert_eq!(
        sfx["stamps"],
        serde_json::json!({ "kind": "loud", "position": "left" })
    );
}

/// #156: content-line stamps named `speaker`/`text`/`kind` keep the line's
/// own keys and sit under `stamps`.
#[test]
fn line_stamps_named_speaker_text_kind_keep_the_line_keys() {
    let snap = assembled_snapshot(Vec::new(), &["speaker", "text", "kind"]);
    let text =
        format!("{HDR}@elena{{speaker=\"ghost\" text=\"alt\" kind=\"aside\"}}: real words\n");
    let cmds = strict_commands(&text, snap);
    let line = find(&cmds, "line");
    assert_eq!(line["kind"], serde_json::json!("line"));
    assert_eq!(line["speaker"], serde_json::json!("elena"));
    assert_eq!(line["text"], serde_json::json!("real words"));
    assert_eq!(
        line["stamps"],
        serde_json::json!({ "kind": "aside", "speaker": "ghost", "text": "alt" })
    );
}
