use super::*;
use crate::types::Type;


    #[test]
    fn number_plugin_attr_type_names_int_and_double() {
        let error = serde_yaml::from_str::<DirectivesFile>(
            "directives:\n  - name: depth\n    attrs:\n      - { name: value, type: number }\n"
        ).unwrap_err().to_string();
        assert!(error.contains("type: int") && error.contains("type: double"), "{error}");
    }

    #[test]
    fn number_payload_type_names_int_and_double() {
        let error = serde_yaml::from_str::<OccasionsFile>(
            "occasions:\n  dive: { select: first, payload: { depth: number } }\n"
        ).unwrap_err().to_string();
        assert!(error.contains("type: int") && error.contains("type: double"), "{error}");
    }

    #[test]
    fn number_def_param_type_names_int_and_double() {
        for params in ["{ depth: number }", "[{ name: depth, type: number }]"] {
            let source = format!(
                "defs:\n  - name: deep\n    type: bool\n    cel: 'depth > 0'\n    params: {params}\n"
            );
            let error = serde_yaml::from_str::<DefsFile>(&source).unwrap_err().to_string();
            assert!(error.contains("type: int") && error.contains("type: double"), "{error}");
        }
    }

    const MINIGAME_DIR: &str = r#"
directives:
  - name: minigame
    layer: bridge
    attrs:
      - { name: kind, required: true, type: { enumFromOption: allowedKinds } }
      - { name: id, required: true, type: { providerRef: minigameId } }
      - { name: wait, type: bool, default: true }
    semantics: [ "writes.sceneState", "bridgeCall" ]
    bridge: { service: minigame, operation: play }
    lower: { kind: builtin, name: actorStage }
"#;

    #[test]
    fn parses_directive_with_attrs_and_lower() {
        let file: DirectivesFile = serde_yaml::from_str(MINIGAME_DIR).unwrap();
        let d = &file.directives[0];
        assert_eq!(d.name, "minigame");
        assert_eq!(d.attrs.len(), 3);
        assert!(d.attrs[0].required);
        assert!(matches!(d.lower, Lowering::Builtin { .. }));
    }

    #[test]
    fn state_shape_field_defaults_are_typed() {
        let y = r#"
stateShapes:
  - name: minigameResult
    fields:
      - { name: rank, type: { enum: [fail, gold] }, default: fail }
"#;
        let f: StateFile = serde_yaml::from_str(y).unwrap();
        assert_eq!(f.state_shapes.unwrap()[0].fields[0].name, "rank");
    }
    #[test]
    fn write_value_untagged_variants_bind() {
        let y = r#"
writes:
  - { scope: scene, path: [minigame, rank], value: { fromBridgeResult: rank } }
  - { scope: scene, path: [minigame, attempts], value: { op: increment, by: 1 } }
  - { scope: scene, path: [flags, done], value: true }
"#;
        let e: DirectiveEffects = serde_yaml::from_str(y).unwrap();
        assert!(matches!(
            e.writes[0].value,
            WriteValue::FromBridgeResult { .. }
        ));
        assert!(matches!(e.writes[1].value, WriteValue::Op { .. }));
        assert!(matches!(e.writes[2].value, WriteValue::Literal(_)));
    }

    #[test]
    fn fact_effects_parse_and_keep_the_debug_of_a_writes_only_block() {
        // dsl 0.27.0 §4: `writes` is optional, facts are `rel(arg, …)`.
        let e: DirectiveEffects =
            serde_yaml::from_str("asserts: [\"holding(@item)\", \"seen(key, true)\"]").unwrap();
        assert!(e.writes.is_empty());
        assert_eq!(e.asserts[0].relation, "holding");
        assert_eq!(e.asserts[0].args, vec![FactEffectArg::Attr("item".into())]);
        assert_eq!(e.asserts[1].to_string(), "seen(key, true)");
        // capabilityVersion hashes `Debug`: a block without facts prints as
        // it did before `asserts`/`retracts` existed.
        let w: DirectiveEffects = serde_yaml::from_str(
            "writes: [ { scope: run, path: [sanity], value: { op: increment, by: -1 } } ]",
        )
        .unwrap();
        let dbg = format!("{w:?}");
        assert!(dbg.starts_with("DirectiveEffects { writes: ["), "{dbg}");
        assert!(
            !dbg.contains("asserts") && !dbg.contains("retracts"),
            "{dbg}"
        );
        assert!(format!("{e:?}").contains("asserts: [\"holding(@item)\""));
        // …and serializes without the empty lists.
        let back = serde_yaml::to_string(&w).unwrap();
        assert!(!back.contains("asserts"), "{back}");
    }

    #[test]
    fn malformed_fact_effects_are_refused_naming_the_shape() {
        for bad in [
            "holding",
            "holding(@)",
            "holding(a b)",
            "(item)",
            "holding(\"x\")",
        ] {
            let err = serde_yaml::from_str::<DirectiveEffects>(&format!("asserts: ['{bad}']"))
                .unwrap_err()
                .to_string();
            assert!(
                err.contains("a fact effect is `relation(arg, …)`"),
                "{bad}: {err}"
            );
        }
        let err = serde_yaml::from_str::<DirectiveEffects>("asserts: [ { holding: item } ]")
            .unwrap_err()
            .to_string();
        assert!(err.contains("expected a string"), "{err}");
    }

    #[test]
    fn lowering_record_form_binds() {
        let y = "record: setBackground\nfields: {}";
        let l: Lowering = serde_yaml::from_str(y).unwrap();
        assert!(matches!(l, Lowering::Record { .. }));
    }

    #[test]
    fn absent_lower_is_the_generic_passthrough() {
        let y = "directives:\n  - { name: encounter, attrs: [ { name: id, type: string } ] }\n";
        let file: DirectivesFile = serde_yaml::from_str(y).unwrap();
        assert!(file.directives[0].lower.is_passthrough());
        // …and it serializes as no `lower:` at all.
        let back = serde_yaml::to_string(&file.directives[0]).unwrap();
        assert!(!back.contains("lower"), "{back}");
    }

    #[test]
    fn unregistered_builtin_hook_is_rejected_naming_the_registry() {
        let err = serde_yaml::from_str::<Lowering>("{ kind: builtin, name: encounter }")
            .unwrap_err()
            .to_string();
        assert!(
            err.contains("`encounter` is not a builtin lowering hook"),
            "{err}"
        );
        assert!(err.contains(&BUILTIN_LOWERING_HOOKS.join(", ")), "{err}");
        assert!(err.contains("omit `lower:`"), "{err}");
        let err = serde_yaml::from_str::<Lowering>("{ kind: builtin, name: actorStag }")
            .unwrap_err()
            .to_string();
        assert!(err.contains("did you mean `actorStage`?"), "{err}");
    }

    #[test]
    fn malformed_lowering_shapes_are_named() {
        for (y, want) in [
            (
                "{ kind: record, name: background }",
                "the only kind is `builtin`",
            ),
            ("{ kind: builtin }", "needs `name:`"),
            ("{ record: background }", "needs `fields:`"),
            (
                "{ record: background, fields: {}, kind: builtin, name: end }",
                "not a mix",
            ),
            ("{}", "`lower:` is empty"),
            (
                "{ record: background, feilds: {} }",
                "unknown field `feilds`",
            ),
        ] {
            let err = serde_yaml::from_str::<Lowering>(y).unwrap_err().to_string();
            assert!(err.contains(want), "{y}: {err}");
        }
    }

    #[test]
    fn builtin_hook_registry_is_exactly_the_core_hooks() {
        // The registry is the set of hooks the embedded `lute.core` manifest
        // names; a hook added to one without the other fails here.
        let core: DirectivesFile =
            serde_yaml::from_str(include_str!("../../assets/lute.core/directives/staging.yaml"))
                .unwrap();
        let mut named: Vec<String> = core
            .directives
            .iter()
            .filter_map(|d| match &d.lower {
                Lowering::Builtin { name, .. } => Some(name.clone()),
                _ => None,
            })
            .collect();
        named.sort();
        assert_eq!(named, BUILTIN_LOWERING_HOOKS);
    }

    #[test]
    fn export_bodies_reject_unknown_keys() {
        let occ = serde_yaml::from_str::<OccasionsFile>("occasions:\n  report: { selct: all }\n")
            .unwrap_err()
            .to_string();
        assert!(occ.contains("unknown field `selct`"), "{occ}");
        let rk =
            serde_yaml::from_str::<RewardKindsFile>("rewardKinds:\n  gold: { credit: run.gold }\n")
                .unwrap_err()
                .to_string();
        assert!(rk.contains("unknown field `credit`"), "{rk}");
        let dir = serde_yaml::from_str::<DirectivesFile>(
            "directives:\n  - { name: x, attrs: [ { name: a, type: string, requird: true } ] }\n",
        )
        .unwrap_err()
        .to_string();
        assert!(dir.contains("unknown field `requird`"), "{dir}");
        let target = serde_yaml::from_str::<OccasionsFile>(
            "occasions:\n  talk: { target: { prefix: npc, entity: npc, member: [a] } }\n",
        );
        assert!(target.is_err(), "a typo'd target domain key must not parse");
    }

    #[test]
    fn plugin_manifest_parses_spec_entry() {
        let y = r#"
id: arcia.minigame
version: 0.1.0
kind: capability
depends: [ { id: lute.core, range: "^0.0.1" } ]
exports: { directives: directives/, state: state/ }
options:
  - { name: resultScope, type: { enum: [scene, run] }, default: scene }
"#;
        let m: PluginManifest = serde_yaml::from_str(y).unwrap();
        assert_eq!(m.id, "arcia.minigame");
        assert_eq!(m.kind, "capability");
        assert_eq!(m.depends.len(), 1);
        assert_eq!(m.options[0].name, "resultScope");
    }

    #[test]
    fn asset_kind_decl_parses_ch() {
        let y = r#"
assetKinds:
  - kind: CH
    sep: "."
    segments:
      - { name: prefix,      const: CH }
      - { name: characterId, type: { providerRef: character } }
      - { name: costume,     type: string }
      - { name: emotion,     type: { enum: [delighted, content, neutral] } }
      - { name: variant,     type: int }
    fallback: [emotionGroup, neutral, variant0]
    persistence: scene
"#;
        let file: AssetKindsFile = serde_yaml::from_str(y).unwrap();
        let d = &file.asset_kinds[0];
        assert_eq!(d.kind, "CH");
        assert_eq!(d.sep, ".");
        assert_eq!(d.resolve, AssetResolve::Compose);
        assert_eq!(d.segments.len(), 5);
        assert_eq!(
            d.segments
                .iter()
                .map(|s| s.name.as_str())
                .collect::<Vec<_>>(),
            ["prefix", "characterId", "costume", "emotion", "variant"]
        );
        assert_eq!(d.segments[0].r#const.as_deref(), Some("CH"));
        assert_eq!(d.segments[0].ty, None);
        assert_eq!(
            d.segments[1].ty,
            Some(Type::ProviderRef("character".into()))
        );
        assert_eq!(d.segments[2].ty, Some(Type::Str));
        assert_eq!(
            d.segments[3].ty,
            Some(Type::Enum(vec![
                "delighted".into(),
                "content".into(),
                "neutral".into(),
            ]))
        );
        assert_eq!(d.segments[4].ty, Some(Type::Int));
        assert_eq!(d.fallback, ["emotionGroup", "neutral", "variant0"]);
        assert_eq!(d.persistence.as_deref(), Some("scene"));
    }

    #[test]
    fn asset_kind_decl_parses_bg_query() {
        let y = r#"
assetKinds:
  - kind: BG
    resolve: query
    provider: backgrounds
    aliases: { location: locationAlias }
    match: [ { attr: location, field: spaceId, via: locationAlias },
             { attr: time, field: timeOfDay }, { attr: view, field: view },
             { attr: variation, field: variation } ]
    fallback: [ dropVariation, areaKind, preferAfternoon, anyView ]
"#;
        let file: AssetKindsFile = serde_yaml::from_str(y).unwrap();
        let d = &file.asset_kinds[0];
        assert_eq!(d.kind, "BG");
        assert_eq!(d.resolve, AssetResolve::Query);
        assert_eq!(d.provider.as_deref(), Some("backgrounds"));
        assert_eq!(d.match_.len(), 4);
        assert_eq!(d.match_[0].attr, "location");
        assert_eq!(d.match_[0].field, "spaceId");
        assert_eq!(d.match_[0].via.as_deref(), Some("locationAlias"));
        assert_eq!(d.match_[1].via, None);
        assert_eq!(d.aliases["location"], "locationAlias");
        assert_eq!(
            d.fallback,
            ["dropVariation", "areaKind", "preferAfternoon", "anyView"]
        );
    }

    #[test]
    fn def_params_mapping_deserializes_in_source_order() {
        // §8.1 `params:` MAPPING spelling — order MUST be preserved for positional
        // arg binding (serde_yaml::Mapping is insertion-ordered).
        let src = "defs:\n  - name: pair\n    type: bool\n    cel: \"true\"\n    params: { a: int, b: bool }\n";
        let file: DefsFile = serde_yaml::from_str(src).unwrap();
        let d = &file.defs[0];
        assert_eq!(
            d.params,
            vec![
                DefParam {
                    name: "a".into(),
                    ty: Type::Int
                },
                DefParam {
                    name: "b".into(),
                    ty: Type::Bool
                },
            ]
        );
    }

    #[test]
    fn def_params_sequence_spelling_deserializes() {
        // The plugin `defs.yaml` list spelling `[{ name, type }]` also works.
        let src = "defs:\n  - name: pair\n    type: bool\n    cel: \"true\"\n    params:\n      - { name: a, type: int }\n      - { name: b, type: bool }\n";
        let file: DefsFile = serde_yaml::from_str(src).unwrap();
        assert_eq!(
            file.defs[0].params,
            vec![
                DefParam {
                    name: "a".into(),
                    ty: Type::Int
                },
                DefParam {
                    name: "b".into(),
                    ty: Type::Bool
                },
            ]
        );
    }

    #[test]
    fn def_params_sequence_skips_malformed_entry() {
        // §8.1 SEQUENCE spelling MUST be fail-soft: one malformed entry (here
        // missing `type`) is skipped, not fatal — the file still keeps its good
        // params rather than being rejected wholesale (mirrors the MAPPING path).
        let src = "defs:\n  - name: pair\n    type: bool\n    cel: \"true\"\n    params:\n      - { name: a, type: int }\n      - { name: b }\n";
        let file: DefsFile = serde_yaml::from_str(src).unwrap();
        assert_eq!(
            file.defs[0].params,
            vec![DefParam {
                name: "a".into(),
                ty: Type::Int
            }]
        );
    }

    #[test]
    fn def_params_absent_yields_empty() {
        let src = "defs:\n  - name: bare\n    type: bool\n    cel: \"true\"\n";
        let file: DefsFile = serde_yaml::from_str(src).unwrap();
        assert!(file.defs[0].params.is_empty());
    }
