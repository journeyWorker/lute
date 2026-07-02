use lute_manifest::assemble::assemble_snapshot;
use lute_manifest::loader::LoadedPlugin;
use lute_manifest::resolve::{ActivePlugin, InstalledPlugin, InstalledPlugins};
use lute_manifest::schema::{AttrDecl, DirectiveDecl, Lowering, PluginManifest};
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
