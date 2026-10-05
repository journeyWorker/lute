    use super::*;

    fn parse_meta_str(yaml: &str) -> (TypedMeta, Vec<Diagnostic>) {
        let meta = Meta {
            raw_yaml: yaml.to_string(),
            span: lute_core_span::Span {
                byte_start: 0,
                byte_end: yaml.len(),
                line: 1,
                column: 1,
                utf16_range: (0, 0),
            },
        };
        parse_meta(&meta, &CapabilitySnapshot::default())
    }

    // ── 0.15.0 §2/§3/§4: authored scene `id:`, `extra:` block, legacy demotion.

    #[test]
    fn authored_id_relieves_required_triad() {
        let (m, diags) = parse_meta_str("id: haven.s01ep01\n");
        assert!(
            !diags.iter().any(|d| d.code == "E-META-MISSING"),
            "authored `id:` must satisfy the required-key rule (§4): {diags:?}"
        );
        assert_eq!(m.id.as_deref(), Some("haven.s01ep01"));
        assert_eq!(canonical_scene_key(&m).as_deref(), Some("haven.s01ep01"));
    }

    #[test]
    fn no_id_keeps_required_triad() {
        let (_m, diags) = parse_meta_str("season: 1\nepisode: 2\n");
        assert!(
            diags.iter().any(|d| d.code == "E-META-MISSING"),
            "without `id:` the legacy required-key rule still fires (§4): {diags:?}"
        );
    }

    /// FS-F14: a scene with no identity at all gets one error teaching
    /// `id:`, not the legacy `character`/`season`/`episode` triple.
    #[test]
    fn no_identity_asks_for_id_once() {
        let (_m, diags) = parse_meta_str("title: A first scene\n");
        let missing: Vec<_> = diags
            .iter()
            .filter(|d| d.code == "E-META-MISSING")
            .collect();
        assert_eq!(missing.len(), 1, "{diags:?}");
        assert!(missing[0].message.contains("needs an `id:`"), "{diags:?}");
    }

    #[test]
    fn malformed_id_is_e_meta_id() {
        let (_m, diags) = parse_meta_str("id: \"has space\"\n");
        assert!(
            diags.iter().any(|d| d.code == "E-META-ID"),
            "an `id:` outside `[A-Za-z0-9_.-]+` must draw E-META-ID (§2): {diags:?}"
        );
    }

    #[test]
    fn empty_id_is_e_meta_id() {
        // §2: non-empty is part of the charset gate.
        let (m, diags) = parse_meta_str("id: \"\"\n");
        assert!(
            diags.iter().any(|d| d.code == "E-META-ID"),
            "an empty `id:` must draw E-META-ID (§2): {diags:?}"
        );
        assert!(m.id.is_none(), "a rejected `id:` value is not lifted (§2)");
    }

    #[test]
    fn extra_block_lifts_scalars_and_lists() {
        let (m, diags) =
            parse_meta_str("id: x\nextra:\n  arc: main\n  season: 1\n  tags: [harbor, night]\n");
        assert!(
            diags.is_empty(),
            "clean scalars/lists must not diagnose: {diags:?}"
        );
        assert_eq!(m.extra_block.get("arc"), Some(&serde_json::json!("main")));
        assert_eq!(m.extra_block.get("season"), Some(&serde_json::json!(1)));
        assert_eq!(
            m.extra_block.get("tags"),
            Some(&serde_json::json!(["harbor", "night"]))
        );
    }

    #[test]
    fn nested_meta_value_is_e_meta_value() {
        let (_m, diags) = parse_meta_str("id: x\nextra:\n  nested: { a: 1 }\n");
        assert!(
            diags.iter().any(|d| d.code == "E-META-VALUE"),
            "a nested mapping under meta: must draw E-META-VALUE (§3): {diags:?}"
        );
    }

    #[test]
    fn non_mapping_extra_is_e_meta_value() {
        let (m, diags) = parse_meta_str("id: x\nextra: not-a-mapping\n");
        assert!(
            diags.iter().any(|d| d.code == "E-META-VALUE"),
            "a non-mapping `meta:` must draw one E-META-VALUE (§3): {diags:?}"
        );
        assert!(m.extra_block.is_empty());
    }

    #[test]
    fn authored_legacy_beside_id_warns_per_key() {
        let (_m, diags) = parse_meta_str("id: x\ncharacter: marina\nseason: 1\nepisode: 1\n");
        let n = diags.iter().filter(|d| d.code == "W-META-LEGACY").count();
        assert_eq!(
            n, 3,
            "one W-META-LEGACY per authored legacy key (§4): {diags:?}"
        );
        assert!(diags
            .iter()
            .all(|d| d.code != "W-META-LEGACY" || d.severity == Severity::Warning));
    }

    #[test]
    fn authored_episode_id_beside_id_warns_too() {
        let (_m, diags) = parse_meta_str("id: x\nepisodeId: ep03\n");
        let n = diags.iter().filter(|d| d.code == "W-META-LEGACY").count();
        assert_eq!(
            n, 1,
            "authored `episodeId:` alongside `id:` warns (§2/§4): {diags:?}"
        );
    }

    #[test]
    fn defaults_inherited_legacy_beside_id_is_silent() {
        // §4/D-C: the pass reads the AUTHORED map. A manifest-inherited
        // `character:`/`season:`/`episode:` on an `id:`-carrying document
        // does not draw W-META-LEGACY (the document author wrote nothing to
        // move); required-key/unknown-key enforcement still runs over the
        // merged view (unchanged from 0.10.0 §6.2).
        let defaults: lute_manifest::project::MetaDefaults = [
            (
                "character".to_string(),
                serde_yaml::Value::String("marina".into()),
            ),
            ("season".to_string(), serde_yaml::Value::Number(1.into())),
            ("episode".to_string(), serde_yaml::Value::Number(2.into())),
        ]
        .into_iter()
        .collect();
        let yaml = "id: x\n";
        let meta = Meta {
            raw_yaml: yaml.to_string(),
            span: lute_core_span::Span {
                byte_start: 0,
                byte_end: yaml.len(),
                line: 1,
                column: 1,
                utf16_range: (0, 0),
            },
        };
        let (_m, diags) = parse_meta_kind_with_defaults(
            &meta,
            &CapabilitySnapshot::default(),
            MetaKind::Scene,
            &defaults,
        );
        assert!(
            !diags.iter().any(|d| d.code == "W-META-LEGACY"),
            "defaults-inherited legacy keys must be silent (§4 D-C): {diags:?}"
        );
        assert!(
            !diags.iter().any(|d| d.code == "E-META-MISSING"),
            "authored `id:` satisfies the required-key rule via the merged map: {diags:?}"
        );
    }

    #[test]
    fn derived_fallback_unchanged() {
        let (m, diags) = parse_meta_str("character: x\nseason: 1\nepisode: 2\n");
        assert!(
            diags.is_empty(),
            "clean legacy scene must not diagnose: {diags:?}"
        );
        assert_eq!(canonical_scene_key(&m).as_deref(), Some("x.s01ep02"));
    }

    #[test]
    fn meta_key_is_legal_on_a_quest() {
        let (m, diags) = parse_kind_str("kind: quest\nextra:\n  arc: main\n", MetaKind::Quest);
        assert!(
            !diags.iter().any(|d| d.code == "E-META-UNKNOWN-KEY"),
            "`meta:` is legal on quest roots too (§3): {diags:?}"
        );
        assert_eq!(m.extra_block.get("arc"), Some(&serde_json::json!("main")));
    }

    /// 0.10.0 §12.4: `{ type: X }` is a synonym for `X`, for every spelling the
    /// shared `Type` deserializer admits — the same long form `state:`, `defs:`
    /// and a domain declaration already use.
    #[test]
    fn component_params_accept_the_long_form() {
        let (meta, _d) = parse_meta_str(
            "component: greet\nparams:\n  a: { type: string }\n  \
             b: { type: { enum: [steady, rising] } }\n  \
             c: { type: { providerRef: cast } }\n  d: int\n",
        );
        assert!(!meta.params_malformed, "the long form is legal (§12.4)");
        assert_eq!(
            meta.params
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b", "c", "d"],
            "source order is preserved and no param is silently dropped"
        );
        assert_eq!(meta.params[0].ty, Type::Str);
        assert_eq!(
            meta.params[1].ty,
            Type::Enum(vec!["steady".to_string(), "rising".to_string()])
        );
        assert_eq!(meta.params[2].ty, Type::ProviderRef("cast".to_string()));
        assert_eq!(meta.params[3].ty, Type::Int);
    }

    /// dsl 0.26.0 §3.3: the long form takes `default:` — a literal, or a
    /// `"@def"` the host resolves — and no other key.
    #[test]
    fn component_params_take_a_default_and_no_other_key() {
        let (meta, _d) = parse_meta_str(
            "component: greet\nparams:\n  a: { type: string, default: \"steady\" }\n  \
             b: { type: bool, default: \"@wonFight\" }\n  c: { type: int, default: 3 }\n",
        );
        assert!(!meta.params_malformed);
        assert!(matches!(
            meta.param_defaults.get("a"),
            Some(lute_syntax::ast::AttrValue::Str(s)) if s == "steady"
        ));
        assert!(matches!(
            meta.param_defaults.get("b"),
            Some(lute_syntax::ast::AttrValue::Ref(slot)) if slot.raw == "@wonFight"
        ));
        assert!(matches!(
            meta.param_defaults.get("c"),
            Some(lute_syntax::ast::AttrValue::Str(s)) if s == "3"
        ));
        let (meta, _d) = parse_meta_str(
            "component: greet\nparams:\n  a: { type: string, default: x, extra: 1 }\n",
        );
        assert!(
            meta.params_malformed,
            "a key beside type/default is malformed"
        );
        let (meta, _d) =
            parse_meta_str("component: greet\nparams:\n  a: { type: string, default: [x] }\n");
        assert!(meta.params_malformed, "a non-scalar default is malformed");
    }

    /// §12.4 relaxes `type`, not "any wrapper".
    #[test]
    fn component_params_reject_an_unknown_wrapper_key() {
        let (meta, _d) = parse_meta_str("component: greet\nparams:\n  a: { kind: string }\n");
        assert!(
            meta.params_malformed,
            "`{{ kind: X }}` is not a Type spelling"
        );
    }

    #[test]
    fn canonical_episode_id_uses_authored_value_verbatim() {
        assert_eq!(canonical_episode_id(1, 2, Some("custom-ep")), "custom-ep");
    }

    #[test]
    fn canonical_episode_id_empty_string_falls_back_to_default() {
        assert_eq!(canonical_episode_id(1, 2, Some("")), "s01ep02");
    }

    #[test]
    fn canonical_episode_id_absent_falls_back_to_default() {
        assert_eq!(canonical_episode_id(1, 2, None), "s01ep02");
    }

    #[test]
    fn parses_state_decls_with_namespace() {
        let yaml = "character: marina\nseason: 1\nepisode: 2\npov: fixer\nstate:\n  scene.affect.marina: { type: int, default: 0 }\n";
        let (meta, diags) = parse_meta_str(yaml);
        assert!(diags.is_empty(), "{diags:?}");
        let d = meta.state.decls.get("scene.affect.marina").unwrap();
        assert_eq!(d.namespace, Namespace::Scene);
    }

    #[test]
    fn missing_required_meta_key_errors() {
        let (_m, diags) = parse_meta_str("season: 1\nepisode: 2\n"); // no character
        assert!(diags.iter().any(|d| d.code == "E-META-MISSING"));
    }

    #[test]
    fn codes_locked_is_a_known_universal_key() {
        // dsl §12 publish guard (`lute tag --force`): `codesLocked:` is core
        // frontmatter on every document kind — never E-META-UNKNOWN-KEY.
        let (_m, diags) =
            parse_meta_str("character: x\nseason: 1\nepisode: 1\ncodesLocked: true\n");
        assert!(
            !diags.iter().any(|d| d.code == "E-META-UNKNOWN-KEY"),
            "{diags:?}"
        );
    }

    #[test]
    fn app_write_is_flagged_readonly_at_schema_level() {
        // scene may declare; app.* declared read-only downstream (checked in Task 4.5)
        let (meta, _d) = parse_meta_str(
            "character: x\nseason: 1\nepisode: 2\nstate:\n  app.lang: { type: string }\n",
        );
        assert_eq!(
            meta.state.decls.get("app.lang").unwrap().namespace,
            Namespace::App
        );
    }

    #[test]
    fn parses_extends_scalar() {
        let (meta, diags) =
            parse_meta_str("character: x\nseason: 1\nepisode: 1\nextends: base.lute\n");
        assert!(
            !diags.iter().any(|d| d.code == "E-META-UNKNOWN-KEY"),
            "`extends` must be a known core key; got {diags:?}"
        );
        assert_eq!(meta.extends, vec!["base.lute".to_string()]);
    }

    #[test]
    fn parses_extends_list() {
        let (meta, diags) =
            parse_meta_str("character: x\nseason: 1\nepisode: 1\nextends: [a.lute, b.lute]\n");
        assert!(
            !diags.iter().any(|d| d.code == "E-META-UNKNOWN-KEY"),
            "`extends` list must parse; got {diags:?}"
        );
        assert_eq!(
            meta.extends,
            vec!["a.lute".to_string(), "b.lute".to_string()]
        );
    }

    fn parse_kind_str(yaml: &str, kind: MetaKind) -> (TypedMeta, Vec<Diagnostic>) {
        let meta = Meta {
            raw_yaml: yaml.to_string(),
            span: lute_core_span::Span {
                byte_start: 0,
                byte_end: yaml.len(),
                line: 1,
                column: 1,
                utf16_range: (0, 0),
            },
        };
        parse_meta_kind(&meta, &CapabilitySnapshot::default(), kind)
    }

    #[test]
    fn scene_components_list_parses_as_known_key() {
        let (meta, diags) =
            parse_meta_str("character: x\nseason: 1\nepisode: 1\ncomponents: [greet.lute]\n");
        assert!(
            !diags.iter().any(|d| d.code == "E-META-UNKNOWN-KEY"),
            "`components` must be a known core key; got {diags:?}"
        );
        assert_eq!(meta.components, vec!["greet.lute".to_string()]);
    }

    #[test]
    fn component_mode_lifts_component_and_params() {
        let (meta, diags) = parse_kind_str(
            "component: greet\nparams:\n  who: { providerRef: cast }\n",
            MetaKind::Component,
        );
        assert!(
            diags.is_empty(),
            "component frontmatter must be clean; got {diags:?}"
        );
        assert_eq!(meta.component.as_deref(), Some("greet"));
        assert_eq!(meta.params.len(), 1);
        assert_eq!(meta.params[0].name, "who");
        assert_eq!(meta.params[0].ty, Type::ProviderRef("cast".to_string()));
    }

    #[test]
    fn component_mode_skips_scene_required_keys() {
        // A component file carries no character/season/episode; Component mode
        // must not flag E-META-MISSING (like Schema).
        let (_m, diags) = parse_kind_str("component: greet\n", MetaKind::Component);
        assert!(
            !diags.iter().any(|d| d.code == "E-META-MISSING"),
            "Component mode must skip scene-required keys; got {diags:?}"
        );
    }

    #[test]
    fn component_params_preserve_source_order() {
        let (meta, _d) = parse_kind_str(
            "component: c\nparams:\n  first: string\n  second: int\n  third: bool\n",
            MetaKind::Component,
        );
        let names: Vec<&str> = meta.params.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["first", "second", "third"]);
    }

    #[test]
    fn scene_component_and_params_keys_are_unknown() {
        // dsl §13: `component:`/`params:` are COMPONENT-FILE-ONLY frontmatter keys.
        // A SCENE (MetaKind::Scene) declaring them must hit the unknown-key
        // diagnostic, not be silently accepted.
        let (_m, diags) = parse_meta_str(
            "character: x\nseason: 1\nepisode: 1\ncomponent: greet\nparams:\n  who: string\n",
        );
        let unknown: Vec<&str> = diags
            .iter()
            .filter(|d| d.code == "E-META-UNKNOWN-KEY")
            .map(|d| d.message.as_str())
            .collect();
        assert!(
            unknown.iter().any(|m| m.contains("`component`")),
            "scene `component:` must be an unknown top-level key; got {diags:?}"
        );
        assert!(
            unknown.iter().any(|m| m.contains("`params`")),
            "scene `params:` must be an unknown top-level key; got {diags:?}"
        );

        // No regression: Component mode still accepts both cleanly.
        let (cm, cdiags) = parse_kind_str(
            "component: greet\nparams:\n  who: string\n",
            MetaKind::Component,
        );
        assert!(
            !cdiags.iter().any(|d| d.code == "E-META-UNKNOWN-KEY"),
            "Component mode must accept component/params; got {cdiags:?}"
        );
        assert_eq!(cm.component.as_deref(), Some("greet"));

        // Schema mode (imported via `uses:`) likewise rejects them.
        let (_sm, sdiags) = parse_kind_str("component: greet\n", MetaKind::Schema);
        assert!(
            sdiags
                .iter()
                .any(|d| d.code == "E-META-UNKNOWN-KEY" && d.message.contains("`component`")),
            "Schema mode must reject `component:`; got {sdiags:?}"
        );
    }

    /// A snapshot whose active plugins declare three typed frontmatter keys
    /// (plugin Appendix C2).
    fn snap_with_frontmatter() -> CapabilitySnapshot {
        let mut snap = CapabilitySnapshot::default();
        snap.frontmatter
            .insert("questTier".into(), Type::Enum(vec!["a".into(), "b".into()]));
        snap.frontmatter.insert("difficulty".into(), Type::Int);
        snap.frontmatter
            .insert("tags".into(), Type::List(Box::new(crate::meta::Type::Str)));
        snap
    }

    fn parse_with_snapshot(yaml: &str) -> Vec<Diagnostic> {
        let meta = Meta {
            raw_yaml: yaml.to_string(),
            span: lute_core_span::Span {
                byte_start: 0,
                byte_end: yaml.len(),
                line: 1,
                column: 1,
                utf16_range: (0, 0),
            },
        };
        parse_meta(&meta, &snap_with_frontmatter()).1
    }

    const SCENE_HEAD: &str = "character: marina\nseason: 1\nepisode: 2\n";

    #[test]
    fn plugin_frontmatter_key_with_wrong_typed_value_is_an_error() {
        let diags = parse_with_snapshot(&format!("{SCENE_HEAD}difficulty: hard\n"));
        let hits: Vec<_> = diags
            .iter()
            .filter(|d| d.code == "E-FRONTMATTER-SCHEMA")
            .collect();
        assert_eq!(hits.len(), 1, "{diags:?}");
        assert_eq!(
            hits[0].message,
            "frontmatter key `difficulty` expects int (declared by an active plugin), got \"hard\""
        );
        assert_eq!(hits[0].severity, Severity::Error);
        // The span points at the offending KEY, not the whole frontmatter.
        // `parse_with_snapshot` wraps the YAML in a synthetic whole-file
        // `Meta` with no `---` envelope, so the offset is direct: until #21
        // T10.2 `meta_key_span` added the four bytes of an opener that is not
        // there, and this assertion subtracted them back out.
        assert_eq!(
            &format!("{SCENE_HEAD}difficulty: hard\n")
                [hits[0].span.byte_start..hits[0].span.byte_end],
            "difficulty"
        );
        // A schema violation is NOT also an unknown key.
        assert!(!diags.iter().any(|d| d.code == "E-META-UNKNOWN-KEY"));
    }

    #[test]
    fn plugin_frontmatter_key_with_a_conforming_value_passes() {
        let diags = parse_with_snapshot(&format!(
            "{SCENE_HEAD}difficulty: 3\nquestTier: b\ntags: [rhythm, timing]\n"
        ));
        assert!(
            !diags
                .iter()
                .any(|d| d.code == "E-FRONTMATTER-SCHEMA" || d.code == "E-META-UNKNOWN-KEY"),
            "{diags:?}"
        );
    }

    #[test]
    fn enum_and_list_frontmatter_violations_report_the_declared_type() {
        let diags =
            parse_with_snapshot(&format!("{SCENE_HEAD}questTier: zzz\ntags: [rhythm, 4]\n"));
        let msgs: Vec<_> = diags
            .iter()
            .filter(|d| d.code == "E-FRONTMATTER-SCHEMA")
            .map(|d| d.message.as_str())
            .collect();
        assert_eq!(
            msgs,
            vec![
                "frontmatter key `questTier` expects enum(a|b) (declared by an active plugin), got \"zzz\"",
                "frontmatter key `tags` expects list<string> (declared by an active plugin), got [\"rhythm\", 4]",
            ],
            "{diags:?}"
        );
    }

    #[test]
    fn unrepresentable_frontmatter_value_is_the_same_error_not_a_panic() {
        // `null` has no `Literal` form at all — a schema violation, never a panic.
        let diags = parse_with_snapshot(&format!("{SCENE_HEAD}difficulty:\n"));
        let hits: Vec<_> = diags
            .iter()
            .filter(|d| d.code == "E-FRONTMATTER-SCHEMA")
            .collect();
        assert_eq!(hits.len(), 1, "{diags:?}");
        assert_eq!(
            hits[0].message,
            "frontmatter key `difficulty` expects int (declared by an active plugin), got null"
        );
    }

    #[test]
    fn core_keys_are_not_routed_through_the_plugin_schema_check() {
        // `title`/`season` are core keys with bespoke handling; a plugin schema
        // check must never fire on them, and a genuinely unknown key still hits
        // `E-META-UNKNOWN-KEY` rather than the schema code.
        let diags = parse_with_snapshot(&format!("{SCENE_HEAD}title: A Night Out\nnope: 1\n"));
        assert!(
            !diags.iter().any(|d| d.code == "E-FRONTMATTER-SCHEMA"),
            "{diags:?}"
        );
        assert!(
            diags
                .iter()
                .any(|d| d.code == "E-META-UNKNOWN-KEY" && d.message.contains("`nope`")),
            "{diags:?}"
        );
    }

    #[test]
    fn duplicate_relation_child_keeps_raw_yaml_snapshot_unreadable() {
        let (typed, _diags) = parse_meta_str(
            "kind: component\nrelations:\n  links:\n    subject: crew\n    object: topic\n  links:\n    subject: crew\n    object: topic\n",
        );
        assert!(typed.yaml().is_none());
    }
