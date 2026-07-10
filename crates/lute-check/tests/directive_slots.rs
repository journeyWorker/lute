//! Directive-slot expansion: an active directive's state.declares[] opens
//! concrete StateSchema slots at each use site (plugin §8/§9).
use lute_check::{check, CheckInput, Mode, SchemaImports};
use lute_manifest::provider::ProviderSet;
use lute_manifest::schema::*;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::{Field, FromAttr, Literal, PathSegment, Type};

/// Core snapshot + a synthetic `::minigame` directive declaring
/// scene.minigame.<resultKey>.* via the `minigameResult` shape.
fn snapshot_with_minigame() -> CapabilitySnapshot {
    let mut snap = lute_manifest::core::load_core_snapshot();
    snap.state_shapes.insert(
        "minigameResult".into(),
        StateShape {
            name: "minigameResult".into(),
            fields: vec![
                Field {
                    name: "score".into(),
                    ty: Type::Number,
                    default: Some(Literal::Num(0.0)),
                    required: false,
                    shape: None,
                },
                Field {
                    name: "rank".into(),
                    ty: Type::Enum(vec![
                        "fail".into(),
                        "bronze".into(),
                        "silver".into(),
                        "gold".into(),
                    ]),
                    default: Some(Literal::Str("fail".into())),
                    required: false,
                    shape: None,
                },
                Field {
                    name: "cleared".into(),
                    ty: Type::Bool,
                    default: Some(Literal::Bool(false)),
                    required: false,
                    shape: None,
                },
                Field {
                    name: "attempts".into(),
                    ty: Type::Number,
                    default: Some(Literal::Num(0.0)),
                    required: false,
                    shape: None,
                },
            ],
        },
    );
    snap.directives.insert(
        "minigame".into(),
        DirectiveDecl {
            name: "minigame".into(),
            layer: Some("bridge".into()),
            attrs: vec![
                AttrDecl {
                    name: "kind".into(),
                    required: true,
                    ty: Type::Str,
                    default: None,
                },
                AttrDecl {
                    name: "id".into(),
                    required: true,
                    ty: Type::Str,
                    default: None,
                },
                AttrDecl {
                    name: "resultKey".into(),
                    required: true,
                    ty: Type::SlotId {
                        namespace: "scene.minigame".into(),
                    },
                    default: None,
                },
                AttrDecl {
                    name: "wait".into(),
                    required: false,
                    ty: Type::Bool,
                    default: Some(Literal::Bool(true)),
                },
            ],
            semantics: vec![],
            state: Some(DirectiveState {
                declares: vec![SlotDecl {
                    scope: "scene".into(),
                    path: vec![
                        PathSegment::Literal("minigame".into()),
                        PathSegment::FromAttr {
                            from_attr: FromAttr {
                                name: "resultKey".into(),
                                slot_type: Some("localId".into()),
                            },
                        },
                    ],
                    shape: "minigameResult".into(),
                }],
            }),
            effects: None,
            bridge: None,
            lower: Lowering::Builtin {
                kind: "builtin".into(),
                name: "bridgeMinigame".into(),
            },
        },
    );
    snap
}

fn check_codes(text: &str, snap: CapabilitySnapshot) -> Vec<String> {
    let input = CheckInput {
        text: text.into(),
        uri: "t".into(),
        snapshot: snap,
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

const SCENE: &str = "---\nkind: scene\ncharacter: bianca\nseason: 1\nepisode: 5\n---\n## Shot 1.\n\
::minigame{kind=\"rhythm\" id=\"x\" resultKey=\"service01\" wait=\"true\"}\n\
<match on=\"scene.minigame.service01.rank\">\n\
<when test=\"$ == 'gold'\">:bianca: a\n</when>\n\
<otherwise>:bianca: b\n</otherwise>\n\
</match>\n";

#[test]
fn directive_slot_opens_scene_path() {
    let codes = check_codes(SCENE, snapshot_with_minigame());
    assert!(
        !codes.contains(&"E-UNDECLARED".to_string()),
        "slot path must be declared; got {codes:?}"
    );
}

#[test]
fn without_directive_the_path_is_undeclared() {
    // core-only: no ::minigame directive => tag unknown AND path undeclared.
    let codes = check_codes(SCENE, lute_manifest::core::load_core_snapshot());
    assert!(
        codes.contains(&"E-UNDECLARED".to_string()),
        "core-only must flag undeclared path; got {codes:?}"
    );
}

#[test]
fn cyclic_state_shapes_do_not_overflow() {
    let mut snap = lute_manifest::core::load_core_snapshot();
    // shape A { child: shape B }, shape B { parent: shape A } -> cycle
    snap.state_shapes.insert(
        "A".into(),
        StateShape {
            name: "A".into(),
            fields: vec![Field {
                name: "child".into(),
                ty: Type::Bool,
                default: None,
                required: false,
                shape: Some("B".into()),
            }],
        },
    );
    snap.state_shapes.insert(
        "B".into(),
        StateShape {
            name: "B".into(),
            fields: vec![Field {
                name: "parent".into(),
                ty: Type::Bool,
                default: None,
                required: false,
                shape: Some("A".into()),
            }],
        },
    );
    snap.directives.insert(
        "cyc".into(),
        DirectiveDecl {
            name: "cyc".into(),
            layer: None,
            attrs: vec![AttrDecl {
                name: "k".into(),
                required: true,
                ty: Type::Str,
                default: None,
            }],
            semantics: vec![],
            state: Some(DirectiveState {
                declares: vec![SlotDecl {
                    scope: "scene".into(),
                    path: vec![
                        PathSegment::Literal("cyc".into()),
                        PathSegment::FromAttr {
                            from_attr: FromAttr {
                                name: "k".into(),
                                slot_type: None,
                            },
                        },
                    ],
                    shape: "A".into(),
                }],
            }),
            effects: None,
            bridge: None,
            lower: Lowering::Builtin {
                kind: "builtin".into(),
                name: "n".into(),
            },
        },
    );
    let text = "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n::cyc{k=\"slot\"}\n";
    let input = CheckInput {
        text: text.into(),
        uri: "cyc".into(),
        snapshot: snap,
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
    };
    // Must return without stack-overflow (no-panic contract).
    let _ = check(&input);
}

#[test]
fn unknown_tag_from_inactive_plugin_gets_fixit() {
    let mut snap = lute_manifest::core::load_core_snapshot();
    snap.inactive
        .insert("minigame".into(), "idola.minigame".into());
    let text =
        "---\nkind: scene\ncharacter: x\nseason: 1\nepisode: 1\n---\n## Shot 1.\n::minigame{kind=\"rhythm\"}\n";
    let input = CheckInput {
        text: text.into(),
        uri: "t".into(),
        snapshot: snap,
        providers: ProviderSet::default(),
        mode: Mode::Author,
        imports: SchemaImports::default(),
        components: Default::default(),
    };
    let res = check(&input);
    let d = res
        .diagnostics
        .iter()
        .find(|d| d.code == "E-UNKNOWN-DIRECTIVE")
        .expect("unknown directive");
    assert!(
        !d.fixits.is_empty(),
        "inactive-plugin unknown tag must carry a fix-it"
    );
    assert!(
        d.fixits.iter().any(|f| f.title.contains("idola.minigame")),
        "fix-it names the plugin: {:?}",
        d.fixits
    );
}
