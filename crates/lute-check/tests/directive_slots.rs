//! Directive-slot expansion: an active directive's state.declares[] opens
//! concrete StateSchema slots at each use site (plugin §8/§9).
use lute_check::{check, CheckInput, Mode};
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
    };
    check(&input)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

const SCENE: &str = "---\ncharacter: bianca\nseason: 1\nepisode: 5\n---\n## Shot 1.\n\
::minigame{kind=\"rhythm\" id=\"x\" resultKey=\"service01\" wait=\"true\"}\n\
<match on=\"scene.minigame.service01.rank\">\n\
<when test=\"$ == 'gold'\">:line[bianca]: a\n</when>\n\
<otherwise>:line[bianca]: b\n</otherwise>\n\
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
