use lute_manifest::assemble::assemble_snapshot;
use lute_manifest::loader::LoadedPlugin;
use lute_manifest::resolve::{ActivePlugin, InstalledPlugin, InstalledPlugins};
use lute_manifest::schema::{
    AssetKindDecl, AssetResolve, AttrDecl, DirectiveDecl, Lowering, PluginManifest,
};
use lute_manifest::types::Type;
use std::collections::BTreeMap;

fn plugin_with_directive(id: &str, dname: &str) -> LoadedPlugin {
    LoadedPlugin {
        manifest: PluginManifest {
            id: id.into(),
            version: "0.1.0".into(),
            kind: "capability".into(),
            depends: vec![],
            exports: BTreeMap::new(),
            options: vec![],
        },
        directives: vec![DirectiveDecl {
            name: dname.into(),
            layer: Some("bridge".into()),
            attrs: vec![AttrDecl {
                name: "x".into(),
                required: false,
                ty: Type::Bool,
                default: None,
            }],
            semantics: vec![],
            state: None,
            effects: None,
            bridge: None,
            lower: Lowering::Builtin {
                kind: "builtin".into(),
                name: "n".into(),
            },
        }],
        enums: BTreeMap::new(),
        state_shapes: vec![],
        state_templates: vec![],
        providers: vec![],
        bridge: vec![],
        defs: vec![],
        frontmatter: BTreeMap::new(),
        asset_kinds: vec![],
    }
}

#[test]
fn active_plugin_directive_lands_in_snapshot() {
    let reg = InstalledPlugins {
        by_id: BTreeMap::from([(
            "idola.minigame".to_string(),
            InstalledPlugin {
                loaded: plugin_with_directive("idola.minigame", "minigame"),
            },
        )]),
    };
    let active = vec![
        ActivePlugin {
            id: "lute.core".into(),
            options: BTreeMap::new(),
        },
        ActivePlugin {
            id: "idola.minigame".into(),
            options: BTreeMap::new(),
        },
    ];
    let (snap, errs) = assemble_snapshot(&active, &reg);
    assert!(errs.is_empty(), "{errs:?}");
    assert!(
        snap.directive("minigame").is_some(),
        "plugin directive merged"
    );
    assert!(snap.directive("bg").is_some(), "core directive retained");
    assert!(!snap.version.is_empty());
}

#[test]
fn inactive_plugin_is_indexed_not_merged() {
    let reg = InstalledPlugins {
        by_id: BTreeMap::from([(
            "idola.minigame".to_string(),
            InstalledPlugin {
                loaded: plugin_with_directive("idola.minigame", "minigame"),
            },
        )]),
    };
    // only core active
    let active = vec![ActivePlugin {
        id: "lute.core".into(),
        options: BTreeMap::new(),
    }];
    let (snap, errs) = assemble_snapshot(&active, &reg);
    assert!(errs.is_empty(), "{errs:?}");
    assert!(
        snap.directive("minigame").is_none(),
        "inactive directive must NOT merge"
    );
    assert_eq!(
        snap.inactive.get("minigame"),
        Some(&"idola.minigame".to_string())
    );
}

#[test]
fn plugin_directive_with_bad_semantics_flag_is_rejected() {
    let mut pkg = plugin_with_directive("a.bad", "boom");
    pkg.directives[0].semantics = vec!["totallyMadeUp".into()];
    let reg = InstalledPlugins {
        by_id: BTreeMap::from([("a.bad".to_string(), InstalledPlugin { loaded: pkg })]),
    };
    let active = vec![
        ActivePlugin {
            id: "lute.core".into(),
            options: BTreeMap::new(),
        },
        ActivePlugin {
            id: "a.bad".into(),
            options: BTreeMap::new(),
        },
    ];
    let (_snap, errs) = assemble_snapshot(&active, &reg);
    assert!(
        errs.iter().any(|e| matches!(
            e,
            lute_manifest::assemble::AssembleError::InvalidDirective { .. }
        )),
        "{errs:?}"
    );
}

#[test]
fn cyclic_state_shapes_are_rejected() {
    use lute_manifest::schema::StateShape;
    use lute_manifest::types::Field;
    // A.child -> shape B, B.parent -> shape A: a mutual cycle.
    let mut pkg = plugin_with_directive("a.cyc", "cyc");
    pkg.state_shapes.push(StateShape {
        name: "A".into(),
        fields: vec![Field {
            name: "child".into(),
            ty: Type::Bool,
            default: None,
            required: false,
            shape: Some("B".into()),
        }],
    });
    pkg.state_shapes.push(StateShape {
        name: "B".into(),
        fields: vec![Field {
            name: "parent".into(),
            ty: Type::Bool,
            default: None,
            required: false,
            shape: Some("A".into()),
        }],
    });
    let reg = InstalledPlugins {
        by_id: BTreeMap::from([("a.cyc".to_string(), InstalledPlugin { loaded: pkg })]),
    };
    let active = vec![
        ActivePlugin {
            id: "lute.core".into(),
            options: BTreeMap::new(),
        },
        ActivePlugin {
            id: "a.cyc".into(),
            options: BTreeMap::new(),
        },
    ];
    let (snap, errs) = assemble_snapshot(&active, &reg);
    assert!(
        snap.state_shapes.contains_key("A") && snap.state_shapes.contains_key("B"),
        "shapes must be merged into the snapshot"
    );
    let cyc: std::collections::BTreeSet<String> = errs
        .iter()
        .filter_map(|e| match e {
            lute_manifest::assemble::AssembleError::CyclicStateShape { shape } => {
                Some(shape.clone())
            }
            _ => None,
        })
        .collect();
    assert!(
        cyc.contains("A") && cyc.contains("B"),
        "both cycle members reported, got {cyc:?}"
    );
}

/// A shape whose field references itself (S -> S) is a self-cycle: it must be
/// reported exactly once, as just "S".
#[test]
fn self_cycle_state_shape_reports_single_member() {
    use lute_manifest::schema::StateShape;
    use lute_manifest::types::Field;
    let mut pkg = plugin_with_directive("a.self", "sel");
    pkg.state_shapes.push(StateShape {
        name: "S".into(),
        fields: vec![Field {
            name: "me".into(),
            ty: Type::Bool,
            default: None,
            required: false,
            shape: Some("S".into()),
        }],
    });
    let reg = InstalledPlugins {
        by_id: BTreeMap::from([("a.self".to_string(), InstalledPlugin { loaded: pkg })]),
    };
    let active = vec![
        ActivePlugin {
            id: "lute.core".into(),
            options: BTreeMap::new(),
        },
        ActivePlugin {
            id: "a.self".into(),
            options: BTreeMap::new(),
        },
    ];
    let (_snap, errs) = assemble_snapshot(&active, &reg);
    let cyc: Vec<String> = errs
        .iter()
        .filter_map(|e| match e {
            lute_manifest::assemble::AssembleError::CyclicStateShape { shape } => {
                Some(shape.clone())
            }
            _ => None,
        })
        .collect();
    assert_eq!(cyc, vec!["S".to_string()], "self-cycle reports exactly [S]");
}

/// A diamond (A -> B, A -> C, B -> D, C -> D) is acyclic: it must report ZERO
/// cyclic state shapes despite the shared descendant D.
#[test]
fn acyclic_diamond_state_shapes_report_no_cycle() {
    use lute_manifest::schema::StateShape;
    use lute_manifest::types::Field;
    fn shape(name: &str, refs: &[&str]) -> StateShape {
        StateShape {
            name: name.into(),
            fields: refs
                .iter()
                .enumerate()
                .map(|(i, r)| Field {
                    name: format!("f{i}"),
                    ty: Type::Bool,
                    default: None,
                    required: false,
                    shape: Some((*r).into()),
                })
                .collect(),
        }
    }
    let mut pkg = plugin_with_directive("a.dia", "dia");
    pkg.state_shapes.push(shape("A", &["B", "C"]));
    pkg.state_shapes.push(shape("B", &["D"]));
    pkg.state_shapes.push(shape("C", &["D"]));
    pkg.state_shapes.push(shape("D", &[]));
    let reg = InstalledPlugins {
        by_id: BTreeMap::from([("a.dia".to_string(), InstalledPlugin { loaded: pkg })]),
    };
    let active = vec![
        ActivePlugin {
            id: "lute.core".into(),
            options: BTreeMap::new(),
        },
        ActivePlugin {
            id: "a.dia".into(),
            options: BTreeMap::new(),
        },
    ];
    let (_snap, errs) = assemble_snapshot(&active, &reg);
    assert!(
        !errs.iter().any(|e| matches!(
            e,
            lute_manifest::assemble::AssembleError::CyclicStateShape { .. }
        )),
        "acyclic diamond must report no cycle, got {errs:?}"
    );
}

fn asset_kind(kind: &str) -> AssetKindDecl {
    AssetKindDecl {
        kind: kind.into(),
        sep: ".".into(),
        resolve: AssetResolve::Compose,
        segments: vec![],
        provider: None,
        match_: vec![],
        aliases: BTreeMap::new(),
        fallback: vec![],
        persistence: None,
    }
}

#[test]
fn assemble_merges_asset_kinds() {
    let mut pkg = plugin_with_directive("idola.minigame", "minigame");
    pkg.asset_kinds.push(asset_kind("CH"));
    let reg = InstalledPlugins {
        by_id: BTreeMap::from([(
            "idola.minigame".to_string(),
            InstalledPlugin { loaded: pkg },
        )]),
    };
    let active = vec![
        ActivePlugin {
            id: "lute.core".into(),
            options: BTreeMap::new(),
        },
        ActivePlugin {
            id: "idola.minigame".into(),
            options: BTreeMap::new(),
        },
    ];
    let (snap, errs) = assemble_snapshot(&active, &reg);
    assert!(errs.is_empty(), "{errs:?}");
    assert!(
        snap.asset_kinds.contains_key("CH"),
        "plugin asset kind merged into snapshot"
    );
}

#[test]
fn assemble_rejects_cross_plugin_asset_kind_dup() {
    let mut a = plugin_with_directive("plug.a", "da");
    a.asset_kinds.push(asset_kind("CH"));
    let mut b = plugin_with_directive("plug.b", "db");
    b.asset_kinds.push(asset_kind("CH"));
    let reg = InstalledPlugins {
        by_id: BTreeMap::from([
            ("plug.a".to_string(), InstalledPlugin { loaded: a }),
            ("plug.b".to_string(), InstalledPlugin { loaded: b }),
        ]),
    };
    let active = vec![
        ActivePlugin {
            id: "lute.core".into(),
            options: BTreeMap::new(),
        },
        ActivePlugin {
            id: "plug.a".into(),
            options: BTreeMap::new(),
        },
        ActivePlugin {
            id: "plug.b".into(),
            options: BTreeMap::new(),
        },
    ];
    let (_snap, errs) = assemble_snapshot(&active, &reg);
    assert!(
        errs.iter().any(|e| matches!(
            e,
            lute_manifest::assemble::AssembleError::DuplicateAcrossPlugins { kind, id, .. }
                if kind == "assetKind" && id == "CH"
        )),
        "cross-plugin dup asset kind must be DuplicateAcrossPlugins{{kind:\"assetKind\"}}, got {errs:?}"
    );
}

/// A directive whose attr is typed `assetKind(kind)`, with `assetId` name.
fn directive_with_asset_ref(dname: &str, kind: &str) -> DirectiveDecl {
    DirectiveDecl {
        name: dname.into(),
        layer: None,
        attrs: vec![AttrDecl {
            name: "assetId".into(),
            required: true,
            ty: Type::AssetKind(kind.into()),
            default: None,
        }],
        semantics: vec![],
        state: None,
        effects: None,
        bridge: None,
        lower: Lowering::Builtin {
            kind: "builtin".into(),
            name: "n".into(),
        },
    }
}

#[test]
fn assemble_rejects_unknown_asset_kind() {
    use lute_manifest::assemble::AssembleError;
    let mut pkg = plugin_with_directive("idola.minigame", "minigame");
    // `CH` is declared, so a directive referencing it must NOT error.
    pkg.asset_kinds.push(asset_kind("CH"));
    pkg.directives
        .push(directive_with_asset_ref("present", "CH"));
    // `NOPE` is never declared/assembled: a dangling assetKind ref.
    pkg.directives
        .push(directive_with_asset_ref("dangling", "NOPE"));
    let reg = InstalledPlugins {
        by_id: BTreeMap::from([(
            "idola.minigame".to_string(),
            InstalledPlugin { loaded: pkg },
        )]),
    };
    let active = vec![
        ActivePlugin {
            id: "lute.core".into(),
            options: BTreeMap::new(),
        },
        ActivePlugin {
            id: "idola.minigame".into(),
            options: BTreeMap::new(),
        },
    ];
    let (_snap, errs) = assemble_snapshot(&active, &reg);
    assert!(
        errs.iter().any(|e| matches!(
            e,
            AssembleError::UnknownAssetKind { directive, attr, kind }
                if directive == "dangling" && attr == "assetId" && kind == "NOPE"
        )),
        "dangling assetKind ref must be UnknownAssetKind{{kind:\"NOPE\"}}, got {errs:?}"
    );
    assert!(
        !errs.iter().any(|e| matches!(
            e,
            AssembleError::UnknownAssetKind { kind, .. } if kind == "CH"
        )),
        "a present assetKind ref must not error, got {errs:?}"
    );
    assert_eq!(
        AssembleError::UnknownAssetKind {
            directive: "d".into(),
            attr: "a".into(),
            kind: "k".into(),
        }
        .code(),
        "E-PLUGIN-UNKNOWN-ASSETKIND"
    );
}
