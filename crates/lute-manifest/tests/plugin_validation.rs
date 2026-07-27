//! Assembly-time enforcement of the declarative-lowering declaration
//! (`lower: { record, fields }`) and activation-time enforcement of plugin
//! options (plugin Appendix C1).
//!
//! `validate.rs` owns the RULES (unit-tested there against `ManifestError`);
//! this file pins the SEAM — that the rule reaches `assemble_snapshot`'s error
//! list carrying the specific `E-LOWER-RECORD-*` code, not the generic
//! `E-PLUGIN-INVALID-DIRECTIVE`, and that a `lower: { kind: builtin, … }`
//! plugin is unaffected.

use lute_manifest::assemble::{assemble_snapshot, AssembleError};
use lute_manifest::loader::LoadedPlugin;
use lute_manifest::resolve::{
    validate_activation_options, ActivePlugin, InstalledPlugin, InstalledPlugins,
};
use lute_manifest::schema::{AttrDecl, DirectiveDecl, Lowering, OptionDecl, PluginManifest};
use lute_manifest::types::{Literal, Type};
use std::collections::BTreeMap;

/// A one-directive plugin: `::backdrop{ img }`, lowering as `lower`.
fn backdrop_plugin(lower: Lowering) -> LoadedPlugin {
    LoadedPlugin {
        manifest: PluginManifest {
            id: "idola.vn".into(),
            version: "0.1.0".into(),
            kind: "capability".into(),
            depends: vec![],
            exports: BTreeMap::new(),
            options: vec![OptionDecl {
                name: "resultScope".into(),
                ty: Type::Enum(vec!["scene".into(), "run".into()]),
                default: None,
            }],
        },
        directives: vec![DirectiveDecl {
            name: "backdrop".into(),
            layer: Some("staging".into()),
            attrs: vec![AttrDecl {
                name: "img".into(),
                required: true,
                ty: Type::Str,
                default: None,
            }],
            semantics: vec![],
            state: None,
            effects: None,
            bridge: None,
            lower,
        }],
        enums: BTreeMap::new(),
        state_shapes: vec![],
        state_templates: vec![],
        providers: vec![],
        bridge: vec![],
        defs: vec![],
        frontmatter: BTreeMap::new(),
        asset_kinds: vec![],
        events: vec![],
        stamp_attrs: vec![],
    }
}

fn record_lowering(record: &str, fields_yaml: &str) -> Lowering {
    Lowering::Record {
        record: record.into(),
        fields: serde_yaml::from_str(fields_yaml).expect("fixture yaml"),
    }
}

fn assemble_with(lower: Lowering) -> Vec<AssembleError> {
    let reg = InstalledPlugins {
        by_id: BTreeMap::from([(
            "idola.vn".to_string(),
            InstalledPlugin {
                loaded: backdrop_plugin(lower),
            },
        )]),
    };
    let active = vec![
        ActivePlugin {
            id: "lute.core".into(),
            options: BTreeMap::new(),
        },
        ActivePlugin {
            id: "idola.vn".into(),
            options: BTreeMap::new(),
        },
    ];
    assemble_snapshot(&active, &reg).1
}

#[test]
fn valid_record_lowering_assembles_clean() {
    let errs = assemble_with(record_lowering(
        "background",
        "{ assetId: { fromAttr: img } }",
    ));
    assert!(errs.is_empty(), "{errs:?}");
}

#[test]
fn builtin_lowering_assembles_clean() {
    let errs = assemble_with(Lowering::Builtin {
        kind: "builtin".into(),
        name: "backdrop".into(),
    });
    assert!(errs.is_empty(), "{errs:?}");
}

#[test]
fn bad_record_name_reaches_assembly_with_its_own_code() {
    let errs = assemble_with(record_lowering("line", "{ speaker: { fromAttr: img } }"));
    let codes: Vec<_> = errs.iter().map(|e| e.code()).collect();
    assert_eq!(codes, vec!["E-LOWER-RECORD-UNKNOWN"], "{errs:?}");
    let AssembleError::InvalidDirective {
        plugin,
        directive,
        msg,
        ..
    } = &errs[0]
    else {
        panic!("{errs:?}")
    };
    assert_eq!(plugin, "idola.vn");
    assert_eq!(directive, "backdrop");
    assert_eq!(
        msg,
        "directive `::backdrop` lowers to unknown record `line`; declarative lowering \
         targets the staging kinds (background, music, sfx, vfx, sprite, camera, cut, video)"
    );
}

#[test]
fn bad_field_name_reaches_assembly_with_its_own_code() {
    let errs = assemble_with(record_lowering(
        "background",
        "{ backdropId: { fromAttr: img } }",
    ));
    let codes: Vec<_> = errs.iter().map(|e| e.code()).collect();
    assert_eq!(codes, vec!["E-LOWER-RECORD-FIELD"], "{errs:?}");
}

#[test]
fn bad_from_attr_reaches_assembly_with_its_own_code() {
    let errs = assemble_with(record_lowering(
        "background",
        "{ assetId: { fromAttr: missing } }",
    ));
    let codes: Vec<_> = errs.iter().map(|e| e.code()).collect();
    assert_eq!(codes, vec!["E-LOWER-RECORD-FIELD"], "{errs:?}");
}

#[test]
fn semantics_error_keeps_the_generic_code() {
    // The `code` field must not have relabelled the pre-existing family.
    let mut pkg = backdrop_plugin(Lowering::Builtin {
        kind: "builtin".into(),
        name: "backdrop".into(),
    });
    pkg.directives[0].semantics = vec!["totallyMadeUp".into()];
    let reg = InstalledPlugins {
        by_id: BTreeMap::from([("idola.vn".to_string(), InstalledPlugin { loaded: pkg })]),
    };
    let active = vec![
        ActivePlugin {
            id: "lute.core".into(),
            options: BTreeMap::new(),
        },
        ActivePlugin {
            id: "idola.vn".into(),
            options: BTreeMap::new(),
        },
    ];
    let errs = assemble_snapshot(&active, &reg).1;
    let codes: Vec<_> = errs.iter().map(|e| e.code()).collect();
    assert_eq!(codes, vec!["E-PLUGIN-INVALID-DIRECTIVE"], "{errs:?}");
}

#[test]
fn option_validation_reads_the_manifest_of_the_active_plugin() {
    let reg = InstalledPlugins {
        by_id: BTreeMap::from([(
            "idola.vn".to_string(),
            InstalledPlugin {
                loaded: backdrop_plugin(Lowering::Builtin {
                    kind: "builtin".into(),
                    name: "backdrop".into(),
                }),
            },
        )]),
    };
    let active = vec![ActivePlugin {
        id: "idola.vn".into(),
        options: BTreeMap::from([("resultScope".to_string(), Literal::Str("galaxy".into()))]),
    }];
    let errs = validate_activation_options(&active, &reg);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert_eq!(errs[0].code(), "E-PLUGIN-OPTION-TYPE");
    assert_eq!(
        errs[0].message(),
        "option `idola.vn.resultScope` expects enum(scene|run), got \"galaxy\""
    );
}

/// End-to-end through the ONE resolver both the CLI and the LSP call: a real
/// on-disk project whose plugin mistypes an option AND declares a bad
/// declarative lowering. Both must surface as [`ResolveDiag`]s with their own
/// codes and their spec'd message text, or the two surfaces would diverge on
/// which projects are valid (plugin §11).
#[test]
fn resolver_surfaces_option_and_lowering_diagnostics_end_to_end() {
    use lute_manifest::project::{load_project, resolve_document_snapshot};
    use std::fs;

    let root = std::env::temp_dir().join(format!("lute_c1c2_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let pdir = root.join("plugins/idola.vn/directives");
    fs::create_dir_all(&pdir).unwrap();
    fs::write(
        root.join("plugins/idola.vn/plugin.yaml"),
        "id: idola.vn\nversion: 0.1.0\nkind: capability\n\
         exports:\n  directives: directives/\n\
         options:\n  - { name: rounds, type: number }\n",
    )
    .unwrap();
    fs::write(
        pdir.join("d.yaml"),
        "directives:\n\
         \x20 - { name: backdrop, attrs: [ { name: img, type: string } ], \
         lower: { record: background, fields: { assetId: { fromAttr: img } } } }\n\
         \x20 - { name: narrate, attrs: [ { name: who, type: string } ], \
         lower: { record: line, fields: { speaker: { fromAttr: who } } } }\n",
    )
    .unwrap();
    fs::write(
        root.join("lute.project.yaml"),
        "pluginsDir: plugins/\ndefaultProfile: story\n\
         profiles:\n  story:\n    plugins: { idola.vn: { rounds: many } }\n",
    )
    .unwrap();

    let proj = load_project(&root).expect("valid project").expect("present");
    let (snap, diags) = resolve_document_snapshot(Some(&proj), None, &BTreeMap::new());

    let got: Vec<(&str, &str)> = diags
        .iter()
        .map(|d| (d.code.as_str(), d.message.as_str()))
        .collect();
    assert_eq!(
        got,
        vec![
            (
                "E-PLUGIN-OPTION-TYPE",
                "option `idola.vn.rounds` expects number, got \"many\""
            ),
            (
                "E-LOWER-RECORD-UNKNOWN",
                "directive `::narrate` lowers to unknown record `line`; declarative lowering \
                 targets the staging kinds (background, music, sfx, vfx, sprite, camera, cut, video)"
            ),
        ],
        "{diags:?}"
    );
    // The CONFORMING directive still resolves — one bad declaration does not
    // take the whole capability surface down.
    assert!(snap.directive("backdrop").is_some(), "conforming decl merged");
    fs::remove_dir_all(&root).ok();
}
