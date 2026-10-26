use lute_manifest::loader::{load_plugin_dir, LoadError};
use std::fs;

/// Build a minimal on-disk plugin package under a temp dir; return its path.
fn write_pkg(root: &std::path::Path, dup: bool) {
    fs::create_dir_all(root.join("directives")).unwrap();
    fs::write(
        root.join("plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  directives: directives/\n",
    )
    .unwrap();
    let d = if dup {
        "directives:\n  - { name: foo, attrs: [] }\n  - { name: foo, attrs: [] }\n"
    } else {
        "directives:\n  - { name: foo, attrs: [ { name: x, type: bool } ] }\n"
    };
    fs::write(root.join("directives/a.yaml"), d).unwrap();
}

#[test]
fn loads_a_valid_package() {
    let tmp_scratch = lute_scratch::ScratchDir::new();
    let tmp = tmp_scratch.join("lute_pkg_ok");
    write_pkg(&tmp, false);
    let p = load_plugin_dir(&tmp).expect("valid package loads");
    assert_eq!(p.manifest.id, "t.plug");
    assert_eq!(p.directives.len(), 1);
    assert_eq!(p.directives[0].name, "foo");
}

#[test]
fn rejects_duplicate_directive_id() {
    let tmp_scratch = lute_scratch::ScratchDir::new();
    let tmp = tmp_scratch.join("lute_pkg_dup");
    write_pkg(&tmp, true);
    let errs = load_plugin_dir(&tmp).unwrap_err();
    // Located at the SECOND entry of the one file, naming the first.
    assert!(
        errs.iter().any(|e| matches!(
            e,
            LoadError::DuplicateId { kind, id, at: Some((3, 13)), first: Some(first), .. }
                if kind == "directive" && id == "foo" && first == "directives/a.yaml:2"
        )),
        "{errs:?}"
    );
}

#[test]
fn rejects_missing_export_dir() {
    let tmp_scratch = lute_scratch::ScratchDir::new();
    let tmp = tmp_scratch.join("lute_pkg_miss");
    fs::create_dir_all(&tmp).unwrap();
    fs::write(
        tmp.join("plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  directives: directives/\n",
    )
    .unwrap();
    let errs = load_plugin_dir(&tmp).unwrap_err();
    assert!(errs
        .iter()
        .any(|e| matches!(e, LoadError::MissingExportDir { .. })));
}

#[test]
fn unreadable_declaration_file_is_error() {
    use std::io::Write;
    let tmp_scratch = lute_scratch::ScratchDir::new();
    let tmp = tmp_scratch.join("lute_pkg_badenc");
    fs::create_dir_all(tmp.join("directives")).unwrap();
    fs::write(
        tmp.join("plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  directives: directives/\n",
    )
    .unwrap();
    // invalid UTF-8 -> read_to_string fails
    let mut f = fs::File::create(tmp.join("directives/a.yaml")).unwrap();
    f.write_all(&[0xff, 0xfe, 0x00, 0x9f]).unwrap();
    drop(f);
    let errs = load_plugin_dir(&tmp).unwrap_err();
    assert!(
        errs.iter().any(|e| matches!(e, LoadError::Io { .. })),
        "unreadable file must surface LoadError::Io, got {errs:?}"
    );
}

#[test]
fn scans_a_plugins_directory() {
    let root_scratch = lute_scratch::ScratchDir::new();
    let root = root_scratch.join("lute_plugins");
    write_pkg(&root.join("t.plug"), false); // reuse the helper; nested dir = plugin id
    let (reg, errs) = lute_manifest::loader::load_plugins_dir(&root);
    assert!(errs.is_empty(), "{errs:?}");
    assert!(reg.get("t.plug").is_some());
}

#[test]
fn missing_plugins_dir_is_empty() {
    let (reg, errs) =
        lute_manifest::loader::load_plugins_dir(std::path::Path::new("/no/such/dir/xyz"));
    assert!(reg.by_id.is_empty());
    assert!(errs.is_empty());
}

#[test]
fn rejects_unknown_export_key() {
    let tmp_scratch = lute_scratch::ScratchDir::new();
    let tmp = tmp_scratch.join("lute_pkg_badexport");
    fs::create_dir_all(tmp.join("directivez")).unwrap();
    fs::write(
        tmp.join("plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  directivez: directivez/\n",
    )
    .unwrap();
    fs::write(tmp.join("directivez/a.yaml"), "directives: []\n").unwrap();
    let errs = load_plugin_dir(&tmp).unwrap_err();
    assert!(
        errs.iter()
            .any(|e| matches!(e, LoadError::UnknownExport { .. })),
        "unknown export key must be a LoadError, got {errs:?}"
    );
}

/// Build a plugin package that exports `assetkinds/`; when `dup`, the file
/// declares two `kind: CH` entries (a per-package duplicate).
fn write_asset_pkg(root: &std::path::Path, dup: bool) {
    fs::create_dir_all(root.join("assetkinds")).unwrap();
    fs::write(
        root.join("plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  assetKinds: assetkinds/\n",
    )
    .unwrap();
    let content = if dup {
        "assetKinds:\n  - kind: CH\n  - kind: CH\n"
    } else {
        "assetKinds:\n  - kind: CH\n    segments:\n      - { name: prefix, const: CH }\n      - { name: characterId, type: { providerRef: character } }\n"
    };
    fs::write(root.join("assetkinds/ch.yaml"), content).unwrap();
}

#[test]
fn loads_asset_kinds() {
    let tmp_scratch = lute_scratch::ScratchDir::new();
    let tmp = tmp_scratch.join("lute_pkg_ak");
    write_asset_pkg(&tmp, false);
    let p = load_plugin_dir(&tmp).expect("asset-kind package loads");
    assert_eq!(p.asset_kinds.len(), 1);
    assert_eq!(p.asset_kinds[0].kind, "CH");
}

#[test]
fn loads_asset_kinds_rejects_dup() {
    let tmp_scratch = lute_scratch::ScratchDir::new();
    let tmp = tmp_scratch.join("lute_pkg_akdup");
    write_asset_pkg(&tmp, true);
    let errs = load_plugin_dir(&tmp).unwrap_err();
    assert!(
        errs.iter().any(|e| matches!(
            e,
            LoadError::DuplicateId { kind, id, .. } if kind == "assetKind" && id == "CH"
        )),
        "dup asset kind must be DuplicateId{{kind:\"assetKind\"}}, got {errs:?}"
    );
}

/// Build a plugin package that exports `events/`.
fn write_events_pkg(root: &std::path::Path) {
    fs::create_dir_all(root.join("events")).unwrap();
    fs::write(
        root.join("plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  events: events/\n",
    )
    .unwrap();
    fs::write(root.join("events/a.yaml"), "events:\n  - name: combatEnd\n").unwrap();
}

#[test]
fn loads_events_export() {
    let tmp_scratch = lute_scratch::ScratchDir::new();
    let tmp = tmp_scratch.join("lute_pkg_ev");
    write_events_pkg(&tmp);
    let loaded = load_plugin_dir(&tmp).expect("loads");
    assert_eq!(loaded.events.len(), 1);
    assert_eq!(loaded.events[0].name, "combatEnd");
}

/// plugin §14.1: the `stampattrs` export loads off disk exactly like its
/// sibling export kinds — `stampAttrs:` entries deserialize as ordinary
/// `AttrDecl`s (name + `type:` tagged map), and a per-package duplicate name
/// is a `DuplicateId` from the shared `merge_named` path.
fn write_stamp_attrs_pkg(root: &std::path::Path, dup: bool) {
    fs::create_dir_all(root.join("stampattrs")).unwrap();
    fs::write(
        root.join("plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  stampAttrs: stampattrs/\n",
    )
    .unwrap();
    let second = if dup { "bonusId" } else { "bonusScore" };
    fs::write(
        root.join("stampattrs/a.yaml"),
        format!(
            "stampAttrs:\n  - name: bonusId\n    type: string\n  - name: {second}\n    type: int\n"
        ),
    )
    .unwrap();
}

#[test]
fn loads_stamp_attrs_export() {
    let tmp_scratch = lute_scratch::ScratchDir::new();
    let tmp = tmp_scratch.join("lute_pkg_sa");
    write_stamp_attrs_pkg(&tmp, false);
    let loaded = load_plugin_dir(&tmp).expect("loads");
    assert_eq!(loaded.stamp_attrs.len(), 2);
    assert_eq!(loaded.stamp_attrs[0].decl.name, "bonusId");
    assert!(matches!(
        loaded.stamp_attrs[0].decl.ty,
        lute_manifest::types::Type::Str
    ));
    assert!(matches!(
        loaded.stamp_attrs[1].decl.ty,
        lute_manifest::types::Type::Int
    ));
}

#[test]
fn loads_stamp_attrs_rejects_dup() {
    let tmp_scratch = lute_scratch::ScratchDir::new();
    let tmp = tmp_scratch.join("lute_pkg_sadup");
    write_stamp_attrs_pkg(&tmp, true);
    let errs = load_plugin_dir(&tmp).expect_err("dup stampAttr name");
    assert!(
        errs.iter().any(|e| matches!(
            e,
            lute_manifest::loader::LoadError::DuplicateId { kind, id, .. }
                if kind == "stampAttr" && id == "bonusId"
        )),
        "{errs:?}"
    );
}

/// Build a plugin package exporting `assetkinds/` whose single kind `CH` has
/// a `const` prefix segment plus one `variant` segment typed by `type_yaml`
/// (a raw `type:` value fragment, e.g. `"bool"` or `"{ domain: slotDomain }"`).
fn write_asset_pkg_with_segment_type(root: &std::path::Path, type_yaml: &str) {
    fs::create_dir_all(root.join("assetkinds")).unwrap();
    fs::write(
        root.join("plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  assetKinds: assetkinds/\n",
    )
    .unwrap();
    let content = format!(
        "assetKinds:\n  - kind: CH\n    segments:\n      - {{ name: prefix, const: CH }}\n      - {{ name: variant, type: {type_yaml} }}\n"
    );
    fs::write(root.join("assetkinds/ch.yaml"), content).unwrap();
}

/// Assert that declaring an assetKind segment typed `type_yaml` is rejected
/// with `LoadError::AssetSegmentType { kind: "CH", segment: "variant", .. }`,
/// whose `found` contains `found_fragment` (the `type_str` rendering).
fn assert_segment_type_rejected(dir_suffix: &str, type_yaml: &str, found_fragment: &str) {
    let tmp = lute_scratch::ScratchDir::named(dir_suffix);
    write_asset_pkg_with_segment_type(&tmp, type_yaml);
    let errs = load_plugin_dir(&tmp).expect_err("inadmissible segment type must fail to load");
    assert!(
        errs.iter().any(|e| matches!(
            e,
            LoadError::AssetSegmentType { kind, segment, found, .. }
                if kind == "CH" && segment == "variant" && found.contains(found_fragment)
        )),
        "type {type_yaml} must reject as AssetSegmentType(found~{found_fragment}), got {errs:?}"
    );
}

/// The reported defect: `{ domain: slotDomain }` parses as a segment type
/// (shared `Type` enum) and previously validated nothing. Now rejected at
/// load — the failing test written first, per this repo's TDD convention.
#[test]
fn rejects_domain_segment() {
    assert_segment_type_rejected("domain", "{ domain: slotDomain }", "domain:slotDomain");
}

#[test]
fn rejects_bool_segment() {
    // Structurally a single token like `number`, but nothing admits it for a
    // segment (§6.9's own example and the four `validate_segments` enforces
    // are exhaustive) — admitting it would reopen the same "enforces
    // nothing" hole as `domain`, just for a different variant.
    assert_segment_type_rejected("bool", "bool", "bool");
}

#[test]
fn rejects_list_segment() {
    // A segment is one delimited string token (§6.9 decompose/compose);
    // `list` has no serialization into a single token.
    assert_segment_type_rejected("list", "{ list: string }", "list<string>");
}

#[test]
fn rejects_record_segment() {
    assert_segment_type_rejected(
        "record",
        "{ record: [{ name: a, type: string }] }",
        "record{a}",
    );
}

#[test]
fn rejects_map_segment() {
    assert_segment_type_rejected(
        "map",
        "{ map: { key: string, value: string } }",
        "map<string,string>",
    );
}

#[test]
fn rejects_enum_from_option_segment() {
    // plugin-system 0.0.1 §7: "attribute types only".
    assert_segment_type_rejected(
        "enumfromoption",
        "{ enumFromOption: allowedKinds }",
        "enumFromOption:allowedKinds",
    );
}

#[test]
fn rejects_slot_id_segment() {
    // plugin-system 0.0.1 §7: "attribute types only".
    assert_segment_type_rejected("slotid", "{ slotId: { namespace: run } }", "slotId:run");
}

#[test]
fn rejects_asset_kind_segment() {
    // §7's own elaboration: `assetKind` validates a value AS a complete
    // authored id decomposed against ANOTHER kind's segment grammar — the
    // inverse of being one token WITHIN an id; nesting has no defined
    // meaning.
    assert_segment_type_rejected("assetkind", "{ assetKind: BG }", "assetKind:BG");
}

#[test]
fn rejects_narrative_time_segment() {
    // Absent from the closed `Type ::=` production entirely; opaque,
    // "NEVER author-declarable" (`types.rs`).
    assert_segment_type_rejected("narrativetime", "narrativeTime", "narrativeTime");
}

/// The four types a segment position actually admits — §7's own segment
/// callout for `providerRef`, plus the unrestricted primitives `enum`,
/// `int`, `string` (the exact set `validate_segments` enforces) — must
/// still load cleanly side by side, proving the fix narrowed the declaration
/// surface without disturbing legitimate segment declarations.
#[test]
fn admits_enum_int_string_provider_ref_segments() {
    let tmp_scratch = lute_scratch::ScratchDir::new();
    let tmp = tmp_scratch.join("lute_pkg_segty_admit");
    fs::create_dir_all(tmp.join("assetkinds")).unwrap();
    fs::write(
        tmp.join("plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  assetKinds: assetkinds/\n",
    )
    .unwrap();
    fs::write(
        tmp.join("assetkinds/ch.yaml"),
        "assetKinds:\n  \
         - kind: CH\n    \
           segments:\n      \
           - { name: prefix, const: CH }\n      \
           - { name: characterId, type: { providerRef: character } }\n      \
           - { name: costume, type: string }\n      \
           - { name: emotion, type: { enum: [neutral, sad] } }\n      \
           - { name: variant, type: int }\n",
    )
    .unwrap();
    let p = load_plugin_dir(&tmp).expect("providerRef/enum/int/string segments must load");
    assert_eq!(p.asset_kinds[0].segments.len(), 5);
}

/// Second symptom, same cause (`crates/lute-check/src/project_check.rs:400-447`,
/// `W-DOMAIN-UNREAD`'s read-set): before this fix, a domain used only as an
/// asset segment drew a spurious `W-DOMAIN-UNREAD`, because that read-set
/// enumerates directive attrs, stamp attrs, content-line slots, and relation
/// arguments — never asset-kind segments — while the segment machinery
/// silently accepted the `{ domain: … }` declaration anyway. That absence is
/// CORRECT once the declaration is rejected: `snap.asset_kinds` is populated
/// EXCLUSIVELY from `InstalledPlugins` (`assemble.rs`'s `merge_map` over
/// `pkg.asset_kinds`, `crates/lute-manifest/src/assemble.rs:338-344`), and
/// `InstalledPlugins` is populated EXCLUSIVELY from a package whose
/// `load_plugin_dir` returned `Ok` (`load_plugins_dir`'s `Ok(loaded) => ...
/// reg.by_id...insert`, `Err(mut e) => errs.append(...)`, no partial
/// registration). A package with a domain-typed segment now always resolves
/// `Err`, so it is NEVER installed and NEVER reaches
/// `CapabilitySnapshot.asset_kinds` — there is no longer a legitimate
/// "domain read via a segment" case for `domain_reading_set` to be missing,
/// so it needs no change (`project_check.rs` is untouched by this fix).
///
/// Mirrors the positive control's shape: one plugin, one assetKind, two
/// segments (`mood` enum-typed, `variant` domain-typed) — the same package
/// that used to load fine and enforce nothing for `variant` now fails to
/// load AT ALL, and is absent from the registry entirely.
#[test]
fn domain_segment_plugin_never_reaches_installed_registry() {
    let root_scratch = lute_scratch::ScratchDir::new();
    let root = root_scratch.join("lute_plugins_domseg");
    let pkg = root.join("t.plug");
    fs::create_dir_all(pkg.join("assetkinds")).unwrap();
    fs::write(
        pkg.join("plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  assetKinds: assetkinds/\n",
    )
    .unwrap();
    fs::write(
        pkg.join("assetkinds/ch.yaml"),
        "assetKinds:\n  \
         - kind: CH\n    \
           segments:\n      \
           - { name: prefix, const: CH }\n      \
           - { name: mood, type: { enum: [alpha, beta] } }\n      \
           - { name: variant, type: { domain: slotDomain } }\n",
    )
    .unwrap();

    let (reg, errs) = lute_manifest::loader::load_plugins_dir(&root);
    assert!(
        reg.get("t.plug").is_none(),
        "a package with a rejected segment type must not be installed at all"
    );
    assert!(
        errs.iter().any(|e| matches!(
            e,
            LoadError::AssetSegmentType { kind, segment, .. }
                if kind == "CH" && segment == "variant"
        )),
        "{errs:?}"
    );
}

/// Defect 1: `AssetSegmentType`'s rendered message must be prose — the kind,
/// the segment, the declared type, and the closed admitted set — never the
/// `Debug` struct dump (`AssetSegmentType { file: …, kind: …, .. }`) a raw
/// `{e:?}` format produces. Written first, failing, per this repo's TDD
/// convention.
#[test]
fn asset_segment_type_message_is_prose() {
    let tmp_scratch = lute_scratch::ScratchDir::new();
    let tmp = tmp_scratch.join("lute_pkg_segty_msg");
    write_asset_pkg_with_segment_type(&tmp, "{ domain: slotDomain }");
    let errs = load_plugin_dir(&tmp).expect_err("inadmissible segment type must fail to load");
    let err = errs
        .iter()
        .find(|e| matches!(e, LoadError::AssetSegmentType { .. }))
        .expect("AssetSegmentType error present");
    let msg = err.to_string();
    assert!(
        !msg.contains("AssetSegmentType {"),
        "must not be a Debug dump: {msg}"
    );
    assert!(msg.contains("CH"), "must name the kind: {msg}");
    assert!(msg.contains("variant"), "must name the segment: {msg}");
    assert!(
        msg.contains("domain:slotDomain"),
        "must name the declared type: {msg}"
    );
    for admitted in ["enum", "int", "string", "providerRef"] {
        assert!(
            msg.contains(admitted),
            "must list admitted type `{admitted}`: {msg}"
        );
    }
}

/// Defect 2: `AssetSegmentType.file` must name the `.yaml` that declared the
/// bad segment, not the `assetkinds/` export directory it was read from
/// (which is what `check_asset_segment_types` was anchored at before). The
/// suffix assertion keeps this host-independent (no absolute-path compare).
#[test]
fn asset_segment_type_file_names_the_yaml() {
    let tmp_scratch = lute_scratch::ScratchDir::new();
    let tmp = tmp_scratch.join("lute_pkg_segty_file");
    write_asset_pkg_with_segment_type(&tmp, "{ domain: slotDomain }");
    let errs = load_plugin_dir(&tmp).expect_err("inadmissible segment type must fail to load");
    let file = errs
        .iter()
        .find_map(|e| match e {
            LoadError::AssetSegmentType { file, .. } => Some(file.clone()),
            _ => None,
        })
        .expect("AssetSegmentType error present");
    assert!(
        file.ends_with("ch.yaml"),
        "file must name the source .yaml, not its directory, got {file:?}"
    );
}

/// Pins the `Display` rendering of a PRE-EXISTING variant (`Parse`), so the
/// new `impl Display for LoadError` is exercised for more than just the one
/// variant this defect report cares about. Malformed YAML syntax (not just a
/// shape mismatch) so `yaml_serde::from_str` fails before `merge` ever runs.
#[test]
fn parse_error_message_is_prose() {
    let tmp_scratch = lute_scratch::ScratchDir::new();
    let tmp = tmp_scratch.join("lute_pkg_parsemsg");
    fs::create_dir_all(tmp.join("directives")).unwrap();
    fs::write(
        tmp.join("plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  directives: directives/\n",
    )
    .unwrap();
    fs::write(
        tmp.join("directives/a.yaml"),
        "directives: [ not valid yaml\n",
    )
    .unwrap();
    let errs = load_plugin_dir(&tmp).unwrap_err();
    let err = errs
        .iter()
        .find(|e| matches!(e, LoadError::Parse { .. }))
        .expect("Parse error present");
    let msg = err.to_string();
    assert!(!msg.contains("Parse {"), "must not be a Debug dump: {msg}");
    assert!(
        msg.contains("a.yaml"),
        "must name the offending file: {msg}"
    );
}

/// lint-system design §6: the `lints` export loads off disk like every
/// other export kind. `lints:` entries deserialize as `LintRuleDecl`s
/// (id/target/when/level/message/options) with lowercase-serialized enums
/// on `target` and `level`.
fn write_lints_pkg(root: &std::path::Path, dup: bool) {
    fs::create_dir_all(root.join("lints")).unwrap();
    fs::write(
        root.join("plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  lints: lints/\n",
    )
    .unwrap();
    let second = if dup {
        "too-many-choices"
    } else {
        "another-rule"
    };
    fs::write(
        root.join("lints/a.yaml"),
        format!(
            "lints:\n  - id: too-many-choices\n    target: scene\n    when: \"scene.choices > options.max\"\n    level: warn\n    message: \"scene has {{scene.choices}} choices\"\n    options: {{ max: 6 }}\n  - id: {second}\n    target: line\n    when: \"line.words > 40\"\n    message: \"too long\"\n"
        ),
    )
    .unwrap();
}

#[test]
fn loads_lints_export_round_trip() {
    use lute_manifest::lint::{LintLevel, LintRuleBody, LintTarget};
    let tmp_scratch = lute_scratch::ScratchDir::new();
    let tmp = tmp_scratch.join("lute_pkg_lints");
    write_lints_pkg(&tmp, false);
    let loaded = load_plugin_dir(&tmp).expect("lints package loads");
    assert_eq!(loaded.lints.len(), 2);
    let r0 = &loaded.lints[0];
    assert_eq!(r0.id, "too-many-choices");
    assert_eq!(r0.target, LintTarget::Scene);
    assert_eq!(
        r0.body,
        LintRuleBody::Predicate {
            when: "scene.choices > options.max".to_string()
        }
    );
    assert_eq!(r0.level, Some(LintLevel::Warn));
    assert_eq!(r0.message, "scene has {scene.choices} choices");
    // options are the raw yaml_serde::Mapping — { max: 6 } is present.
    assert_eq!(
        r0.options
            .get(yaml_serde::Value::String("max".into()))
            .and_then(yaml_serde::Value::as_i64),
        Some(6)
    );
    // level absent -> None (plugin declines to declare a default)
    assert_eq!(loaded.lints[1].level, None);
    assert_eq!(loaded.lints[1].target, LintTarget::Line);
}

#[test]
fn loads_lints_rejects_dup_id() {
    let tmp_scratch = lute_scratch::ScratchDir::new();
    let tmp = tmp_scratch.join("lute_pkg_lintsdup");
    write_lints_pkg(&tmp, true);
    let errs = load_plugin_dir(&tmp).expect_err("dup lint id");
    assert!(
        errs.iter().any(|e| matches!(
            e,
            LoadError::DuplicateId { kind, id, .. }
                if kind == "lint" && id == "too-many-choices"
        )),
        "duplicate rule id must surface DuplicateId{{kind:\"lint\"}}, got {errs:?}"
    );
}

/// A malformed `lints/*.yaml` file (bad YAML structure) surfaces through
/// the loader's shared per-file `LoadError::Parse` channel — the load-time
/// flavor of the design's `E-LINT-RULE`, using the SAME anchor every
/// other export uses (see `parse_error_message_is_prose` above).
#[test]
fn malformed_lint_file_is_parse_error() {
    let tmp_scratch = lute_scratch::ScratchDir::new();
    let tmp = tmp_scratch.join("lute_pkg_lintsbad");
    fs::create_dir_all(tmp.join("lints")).unwrap();
    fs::write(
        tmp.join("plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  lints: lints/\n",
    )
    .unwrap();
    // wrong shape: `target: nope` isn't a LintTarget variant.
    fs::write(
        tmp.join("lints/a.yaml"),
        "lints:\n  - id: r\n    target: nope\n    when: \"true\"\n    message: m\n",
    )
    .unwrap();
    let errs = load_plugin_dir(&tmp).expect_err("malformed rule must fail");
    assert!(
        errs.iter().any(|e| matches!(
            e,
            LoadError::Parse { file, .. } if file.ends_with("a.yaml")
        )),
        "malformed rule must surface as LoadError::Parse anchored at the file, got {errs:?}"
    );
}

/// dsl 0.51.0 §6.1: a `pair` rule loads with its predicates and scope, and
/// each shape fault — `when` on `pair`, a missing `open`/`close`, a pair
/// key on another target, a missing `when` — is the file's
/// `LoadError::Parse` (`E-PLUGIN-PARSE`), naming the rule.
#[test]
fn pair_lint_rules_load_and_their_shape_is_enforced() {
    use lute_manifest::lint::{LintRuleBody, LintTarget, PairScope, PairSpec};
    let pkg = |rule: &str| {
        let tmp_scratch = lute_scratch::ScratchDir::new();
        let tmp = tmp_scratch.join("lute_pkg_pair");
        fs::create_dir_all(tmp.join("lints")).unwrap();
        fs::write(
            tmp.join("plugin.yaml"),
            "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  lints: lints/\n",
        )
        .unwrap();
        fs::write(
            tmp.join("lints/a.yaml"),
            format!("lints:\n  - id: g\n    message: m\n{rule}"),
        )
        .unwrap();
        let loaded = load_plugin_dir(&tmp);
        drop(tmp_scratch);
        loaded
    };
    let loaded = pkg(
        "    target: pair\n    open: 'node.tag == \"a\"'\n    close: 'node.tag == \"b\"'\n    \
         inside: 'node.kind == \"line\"'\n    scope: section\n",
    )
    .expect("a pair rule loads");
    assert_eq!(loaded.lints[0].target, LintTarget::Pair);
    assert_eq!(
        loaded.lints[0].body,
        LintRuleBody::Pair(PairSpec {
            open: "node.tag == \"a\"".to_string(),
            close: "node.tag == \"b\"".to_string(),
            inside: Some("node.kind == \"line\"".to_string()),
            scope: PairScope::Section,
        })
    );
    for (rule, want) in [
        (
            "    target: pair\n    when: 'true'\n    open: 'true'\n    close: 'true'\n",
            "rule `g`: `when` is refused on `target: pair`",
        ),
        (
            "    target: pair\n    close: 'true'\n",
            "rule `g`: a `target: pair` rule needs `open`",
        ),
        (
            "    target: pair\n    open: 'true'\n",
            "rule `g`: a `target: pair` rule needs `close`",
        ),
        (
            "    target: node\n    when: 'true'\n    inside: 'true'\n",
            "rule `g`: `inside` belongs to `target: pair`, not `target: node`",
        ),
        (
            "    target: scene\n    when: 'true'\n    scope: section\n",
            "rule `g`: `scope` belongs to `target: pair`, not `target: scene`",
        ),
        ("    target: line\n", "rule `g`: missing field `when`"),
    ] {
        let errs = pkg(rule).expect_err(rule);
        assert!(
            errs.iter().any(|e| matches!(
                e,
                LoadError::Parse { file, msg, .. } if file.ends_with("a.yaml") && msg.contains(want)
            )),
            "{rule}: {errs:?}"
        );
        assert!(
            errs.iter().all(|e| e.code() == "E-PLUGIN-PARSE"),
            "{errs:?}"
        );
    }
}

/// Lints must NEVER perturb `CapabilitySnapshot` / `capabilitySnapshot`
/// (design §1 non-goals). Two packages that differ ONLY by the presence
/// of a `lints/` export produce assembled snapshots whose `version` hash
/// is byte-identical — the concrete evidence that lints are not folded
/// anywhere on the capability-surface hash path.
#[test]
fn lints_do_not_participate_in_capability_version() {
    use lute_manifest::assemble::assemble_snapshot;
    use lute_manifest::loader::load_plugins_dir;
    use lute_manifest::resolve::ActivePlugin;

    // Package A: no lints export.
    let root_a_scratch = lute_scratch::ScratchDir::new();
    let root_a = root_a_scratch.join("lute_lints_hashA");
    fs::create_dir_all(root_a.join("t.plug/directives")).unwrap();
    fs::write(
        root_a.join("t.plug/plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  directives: directives/\n",
    )
    .unwrap();
    fs::write(
        root_a.join("t.plug/directives/a.yaml"),
        "directives:\n  - { name: foo, attrs: [ { name: x, type: bool } ] }\n",
    )
    .unwrap();

    // Package B: same, PLUS a lints export.
    let root_b_scratch = lute_scratch::ScratchDir::new();
    let root_b = root_b_scratch.join("lute_lints_hashB");
    fs::create_dir_all(root_b.join("t.plug/directives")).unwrap();
    fs::create_dir_all(root_b.join("t.plug/lints")).unwrap();
    fs::write(
        root_b.join("t.plug/plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  directives: directives/\n  lints: lints/\n",
    )
    .unwrap();
    fs::write(
        root_b.join("t.plug/directives/a.yaml"),
        "directives:\n  - { name: foo, attrs: [ { name: x, type: bool } ] }\n",
    )
    .unwrap();
    fs::write(
        root_b.join("t.plug/lints/a.yaml"),
        "lints:\n  - id: r\n    target: scene\n    when: \"scene.words > 100\"\n    level: warn\n    message: m\n",
    )
    .unwrap();

    let (reg_a, errs_a) = load_plugins_dir(&root_a);
    assert!(errs_a.is_empty(), "{errs_a:?}");
    let (reg_b, errs_b) = load_plugins_dir(&root_b);
    assert!(errs_b.is_empty(), "{errs_b:?}");

    // Confirm B actually carries the rule (guards against a silent regression
    // where a future refactor drops the lints load path and the hash equality
    // below trivially holds because both packages are empty of lints).
    assert_eq!(reg_b.get("t.plug").unwrap().loaded.lints.len(), 1);
    assert_eq!(reg_a.get("t.plug").unwrap().loaded.lints.len(), 0);

    let active = vec![ActivePlugin {
        id: "t.plug".into(),
        options: Default::default(),
    }];
    let (snap_a, ea) = assemble_snapshot(&active, &reg_a);
    let (snap_b, eb) = assemble_snapshot(&active, &reg_b);
    assert!(ea.is_empty(), "{ea:?}");
    assert!(eb.is_empty(), "{eb:?}");
    assert_eq!(
        snap_a.version, snap_b.version,
        "capabilitySnapshot must be byte-identical whether a plugin ships lints or not"
    );
}

/// [`lute_manifest::lint::namespace_active_lints`] pairs an activation
/// order with the installed registry, emitting each rule keyed by its
/// `<plugin-id>/<id>` (design §3 / §6) in activation order. A rule from
/// an installed-but-inactive plugin is not emitted; a rule from an
/// active-but-not-installed plugin is silently skipped (assembly owns
/// the `MissingActivePlugin` diagnostic for that).
#[test]
fn namespace_active_lints_pairs_activation_with_registry() {
    use lute_manifest::lint::namespace_active_lints;
    use lute_manifest::loader::load_plugins_dir;
    use lute_manifest::resolve::ActivePlugin;

    let root_scratch = lute_scratch::ScratchDir::new();
    let root = root_scratch.join("lute_lints_ns");

    for id in ["a.plug", "b.plug", "inactive.plug"] {
        let pkg = root.join(id);
        fs::create_dir_all(pkg.join("lints")).unwrap();
        fs::write(
            pkg.join("plugin.yaml"),
            format!("id: {id}\nversion: 0.1.0\nkind: capability\nexports:\n  lints: lints/\n"),
        )
        .unwrap();
        fs::write(
            pkg.join("lints/a.yaml"),
            "lints:\n  - id: r1\n    target: scene\n    when: \"true\"\n    message: m\n",
        )
        .unwrap();
    }
    let (reg, errs) = load_plugins_dir(&root);
    assert!(errs.is_empty(), "{errs:?}");

    let active = vec![
        ActivePlugin {
            id: "b.plug".into(),
            options: Default::default(),
        },
        ActivePlugin {
            id: "a.plug".into(),
            options: Default::default(),
        },
        // active but NOT installed — silently skipped.
        ActivePlugin {
            id: "ghost.plug".into(),
            options: Default::default(),
        },
    ];
    let ns = namespace_active_lints(&active, &reg);
    let keys: Vec<&str> = ns.iter().map(|(k, _)| k.as_str()).collect();
    // Activation order preserved; inactive.plug is not emitted; ghost.plug is skipped.
    assert_eq!(keys, vec!["b.plug/r1", "a.plug/r1"]);
}

/// dsl 0.16.0 §4: the `rewardkinds` export loads off disk exactly like its
/// sibling export kinds. A bare `{}` value declares a shape-only kind; a
/// `{ target: { provider: <name> } }` value carries the target contract
/// verbatim onto the [`lute_manifest::schema::RewardKindDecl`], and the
/// map key materializes on `name` (the id is the key, not a body field).
fn write_reward_kinds_pkg(root: &std::path::Path, dup_across_files: bool) {
    fs::create_dir_all(root.join("rewardkinds")).unwrap();
    fs::write(
        root.join("plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  rewardKinds: rewardkinds/\n",
    )
    .unwrap();
    fs::write(
        root.join("rewardkinds/a.yaml"),
        "rewardKinds:\n  SHARD: {}\n  ITEM:\n    target: { provider: item }\n",
    )
    .unwrap();
    if dup_across_files {
        // A second file redeclaring `SHARD` — per-package duplicate caught by
        // the shared `merge_named` path, exactly like the `stampAttrs` sibling.
        fs::write(
            root.join("rewardkinds/b.yaml"),
            "rewardKinds:\n  SHARD: {}\n",
        )
        .unwrap();
    }
}

#[test]
fn loads_reward_kinds_export() {
    let tmp_scratch = lute_scratch::ScratchDir::new();
    let tmp = tmp_scratch.join("lute_pkg_rk");
    write_reward_kinds_pkg(&tmp, false);
    let loaded = load_plugin_dir(&tmp).expect("valid rewardKinds package loads");
    assert_eq!(loaded.reward_kinds.len(), 2);
    let by_name: std::collections::BTreeMap<_, _> = loaded
        .reward_kinds
        .iter()
        .map(|r| (r.name.clone(), r))
        .collect();
    let item = by_name.get("ITEM").expect("ITEM present");
    assert_eq!(
        item.target
            .as_ref()
            .expect("ITEM target")
            .provider
            .as_deref(),
        Some("item"),
        "ITEM target must resolve to `{{ provider: item }}`"
    );
    let shard = by_name.get("SHARD").expect("SHARD present");
    assert!(
        shard.target.is_none(),
        "SHARD is shape-only ({{}}), must have no target"
    );
    assert!(
        shard.attrs.is_empty(),
        "bare {{}} kind must have no game-specific attrs"
    );
}

#[test]
fn loads_reward_kinds_rejects_dup() {
    let tmp_scratch = lute_scratch::ScratchDir::new();
    let tmp = tmp_scratch.join("lute_pkg_rkdup");
    write_reward_kinds_pkg(&tmp, true);
    let errs = load_plugin_dir(&tmp).expect_err("per-package dup rewardKind must fail load");
    assert!(
        errs.iter().any(|e| matches!(
            e,
            lute_manifest::loader::LoadError::DuplicateId { kind, id, .. }
                if kind == "rewardKind" && id == "SHARD"
        )),
        "per-package duplicate must surface as DuplicateId {{ kind: \"rewardKind\" }}, got {errs:?}"
    );
}

/// dsl 0.26.0 §2.5: `target:` also takes `{ entity: <kind> }` and
/// `required: true`; naming both `provider:` and `entity:` fails the load.
#[test]
fn reward_kind_target_takes_entity_and_required_but_not_both_sources() {
    let pkg = |tag: &str, body: &str| {
        let tmp = lute_scratch::ScratchDir::named(tag);
        fs::create_dir_all(tmp.join("rewardkinds")).unwrap();
        fs::write(
            tmp.join("plugin.yaml"),
            "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  rewardKinds: rewardkinds/\n",
        )
        .unwrap();
        fs::write(tmp.join("rewardkinds/a.yaml"), body).unwrap();
        tmp
    };
    let ok = pkg(
        "ok",
        "rewardKinds:\n  ITEM: { target: { entity: bagItem, required: true } }\n  TM: { target: { required: true } }\n",
    );
    let loaded = load_plugin_dir(&ok).expect("entity/required contracts load");
    let item = loaded
        .reward_kinds
        .iter()
        .find(|r| r.name == "ITEM")
        .unwrap();
    let t = item.target.as_ref().unwrap();
    assert_eq!(
        (t.provider.as_deref(), t.entity.as_deref(), t.required),
        (None, Some("bagItem"), true)
    );
    let tm = loaded.reward_kinds.iter().find(|r| r.name == "TM").unwrap();
    assert!(tm.target.as_ref().unwrap().required);

    let both = pkg(
        "both",
        "rewardKinds:\n  ITEM: { target: { provider: items, entity: bagItem } }\n",
    );
    let errs = load_plugin_dir(&both).expect_err("provider + entity must fail the load");
    let msg = format!("{errs:?}");
    assert!(msg.contains("not both"), "{msg}");
}

/// 0.49.0 §3.3 (#168): `target: { forbidden: true }` loads alone and is
/// refused beside `provider:`, `entity:` or `required: true`.
#[test]
fn reward_kind_target_forbidden_is_exclusive() {
    let pkg = |tag: &str, target: &str| {
        let tmp = lute_scratch::ScratchDir::named(tag);
        fs::create_dir_all(tmp.join("rewardkinds")).unwrap();
        fs::write(
            tmp.join("plugin.yaml"),
            "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  rewardKinds: rewardkinds/\n",
        )
        .unwrap();
        fs::write(
            tmp.join("rewardkinds/a.yaml"),
            format!("rewardKinds:\n  COINS: {{ target: {target} }}\n"),
        )
        .unwrap();
        tmp
    };
    let ok = pkg("forbidden-ok", "{ forbidden: true }");
    let loaded = load_plugin_dir(&ok).expect("`forbidden: true` alone loads");
    let t = loaded.reward_kinds[0].target.as_ref().unwrap();
    assert!(t.forbidden);
    assert_eq!(
        (t.provider.as_deref(), t.entity.as_deref(), t.required),
        (None, None, false)
    );

    for (tag, target) in [
        ("forbidden-provider", "{ forbidden: true, provider: items }"),
        ("forbidden-entity", "{ forbidden: true, entity: bagItem }"),
        ("forbidden-required", "{ forbidden: true, required: true }"),
        (
            "forbidden-not-required",
            "{ forbidden: true, required: false }",
        ),
    ] {
        let errs = load_plugin_dir(&pkg(tag, target)).expect_err(target);
        let msg = format!("{errs:?}");
        assert!(
            msg.contains(
                "a reward kind's `target:` takes `forbidden: true` alone — a kind that \
                 takes no target cannot also name an id space or require one"
            ),
            "{target}: {msg}"
        );
    }
    // A present `required:` must be a bool: `null` is not read as omitted,
    // with or without `forbidden: true` (0.48.1 refused it as not a bool).
    for (tag, target) in [
        ("required-null", "{ required: null }"),
        (
            "forbidden-required-null",
            "{ forbidden: true, required: null }",
        ),
    ] {
        let errs = load_plugin_dir(&pkg(tag, target)).expect_err(target);
        let msg = format!("{errs:?}");
        assert!(
            msg.contains(
                "`rewardKinds.COINS.target.required` is empty, where `true` or `false` is expected"
            ),
            "{target}: {msg}"
        );
    }
}

/// 0.49.0 §3.3: `capabilitySnapshot` hashes `RewardTarget`'s `Debug`, which
/// prints `forbidden` only when set — a contract without it keeps the
/// 0.48.1 text and hash (the pinned digest was computed by 0.48.1).
#[test]
fn reward_target_debug_without_forbidden_keeps_the_048_hash() {
    use lute_manifest::schema::{RewardKindDecl, RewardTarget};
    let target = RewardTarget {
        entity: Some("bagItem".into()),
        required: true,
        ..Default::default()
    };
    assert_eq!(
        format!("{target:?}"),
        r#"RewardTarget { entity: "bagItem", required: true }"#
    );
    let mut snap = lute_manifest::snapshot::CapabilitySnapshot::default();
    for (name, target) in [
        (
            "ITEM",
            RewardTarget {
                provider: Some("items".into()),
                ..Default::default()
            },
        ),
        ("TM", target),
    ] {
        snap.reward_kinds.insert(
            name.into(),
            RewardKindDecl {
                name: name.into(),
                target: Some(target),
                ..Default::default()
            },
        );
    }
    assert_eq!(
        lute_manifest::snapshot::capability_version(&snap),
        "d5e01c4e21f36c3c49ac738c09eab4b2a19be251634ec5b5fee326d12a1362ec"
    );

    let forbidden = RewardTarget {
        forbidden: true,
        ..Default::default()
    };
    assert_eq!(format!("{forbidden:?}"), "RewardTarget { forbidden: true }");
}

/// dsl 0.28.0 §6: the export keys are camelCase (`rewardKinds`, `assetKinds`,
/// `stampAttrs`, like the files' own top-level keys); the old lowercase
/// spelling is refused, naming the new one at its line in `plugin.yaml`, and
/// any other near miss gets a did-you-mean that ignores case.
#[test]
fn old_export_spelling_names_the_new_one() {
    for (key, says) in [
        (
            "rewardkinds",
            "plugin.yaml:5:3: export `rewardkinds` is now spelled `rewardKinds` — write \
             `rewardKinds:` in `exports:`",
        ),
        ("assetkinds", "is now spelled `assetKinds`"),
        ("stampattrs", "is now spelled `stampAttrs`"),
        (
            "Occasions",
            "export `Occasions` is not an export kind — did you mean `occasions`?",
        ),
    ] {
        let tmp_scratch = lute_scratch::ScratchDir::new();
        let tmp = tmp_scratch.join(format!("lute_pkg_oldexp_{key}_{}", std::process::id()));
        fs::create_dir_all(tmp.join("x")).unwrap();
        fs::write(
            tmp.join("plugin.yaml"),
            format!("id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  {key}: x/\n"),
        )
        .unwrap();
        let errs = load_plugin_dir(&tmp).unwrap_err();
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].code(), "E-PLUGIN-UNKNOWN-EXPORT");
        let msg = errs[0].to_string();
        assert!(msg.contains(says), "{key}: {msg}");
        let _ = fs::remove_dir_all(&tmp); // best-effort cleanup
    }
}

/// dsl 0.21.0 §2: the `occasions` export loads like `rewardkinds` — the map
/// key is the name, `select` defaults to `first`, `target` to `false`; a
/// `select` outside `first`/`all` fails the load; a per-package duplicate is
/// `DuplicateId { kind: "occasion" }`.
#[test]
fn loads_occasions_export() {
    use lute_manifest::schema::{OccasionSelect, OccasionTarget};
    let tmp_scratch = lute_scratch::ScratchDir::new();
    let tmp = tmp_scratch.join("lute_pkg_oc");
    fs::create_dir_all(tmp.join("occasions")).unwrap();
    fs::write(
        tmp.join("plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  occasions: occasions/\n",
    )
    .unwrap();
    fs::write(
        tmp.join("occasions/a.yaml"),
        "occasions:\n  hubVisit: {}\n  talk: { select: first, target: true }\n  \
         inbox: { select: all, description: Messages }\n  \
         visit: { target: { prefix: place, entity: location } }\n",
    )
    .unwrap();
    let loaded = load_plugin_dir(&tmp).expect("valid occasions package loads");
    let by_name: std::collections::BTreeMap<_, _> = loaded
        .occasions
        .iter()
        .map(|o| (o.name.as_str(), o))
        .collect();
    assert_eq!(by_name.len(), 4);
    let hub = by_name["hubVisit"];
    assert_eq!(
        (hub.select, hub.target.clone()),
        (OccasionSelect::First, OccasionTarget::Shape(false)),
        "defaults"
    );
    assert!(!hub.target.takes_target());
    assert_eq!(by_name["talk"].target, OccasionTarget::Shape(true));
    // dsl 0.22.0 §8: a target domain names a prefix and an entity kind.
    assert_eq!(
        by_name["visit"].target,
        OccasionTarget::Domain {
            prefix: "place".into(),
            entity: "location".into(),
            members: None,
        }
    );
    assert!(by_name["visit"].target.takes_target());
    assert_eq!(by_name["inbox"].select, OccasionSelect::All);
    assert_eq!(by_name["inbox"].description.as_deref(), Some("Messages"));

    fs::write(tmp.join("occasions/b.yaml"), "occasions:\n  talk: {}\n").unwrap();
    let errs = load_plugin_dir(&tmp).expect_err("per-package dup occasion must fail load");
    assert!(
        errs.iter().any(|e| matches!(
            e,
            lute_manifest::loader::LoadError::DuplicateId { kind, id, .. }
                if kind == "occasion" && id == "talk"
        )),
        "{errs:?}"
    );

    fs::write(
        tmp.join("occasions/b.yaml"),
        "occasions:\n  map: { select: random }\n",
    )
    .unwrap();
    let errs = load_plugin_dir(&tmp).expect_err("`select` is a closed enum");
    assert!(
        errs.iter()
            .any(|e| matches!(e, lute_manifest::loader::LoadError::Parse { .. })),
        "{errs:?}"
    );

    // A domain missing `entity` is neither a bool nor a domain.
    fs::write(
        tmp.join("occasions/b.yaml"),
        "occasions:\n  map: { target: { prefix: place } }\n",
    )
    .unwrap();
    let errs = load_plugin_dir(&tmp).expect_err("a half domain fails the load");
    assert!(
        errs.iter()
            .any(|e| matches!(e, lute_manifest::loader::LoadError::Parse { .. })),
        "{errs:?}"
    );

    // A member subset loads onto the domain.
    fs::write(
        tmp.join("occasions/b.yaml"),
        "occasions:\n  bossDefeated: { target: { prefix: boss, entity: foe, members: [gatekeeper, warden] } }\n",
    )
    .unwrap();
    let loaded = load_plugin_dir(&tmp).expect("a member subset loads");
    let boss = loaded
        .occasions
        .iter()
        .find(|o| o.name == "bossDefeated")
        .unwrap();
    assert_eq!(
        boss.target,
        OccasionTarget::Domain {
            prefix: "boss".into(),
            entity: "foe".into(),
            members: Some(vec!["gatekeeper".into(), "warden".into()]),
        }
    );

    // An empty or repeating member list fails the load, naming the occasion.
    for (list, why) in [
        ("[]", "is empty"),
        ("[warden, warden]", "`warden` more than once"),
    ] {
        fs::write(
            tmp.join("occasions/b.yaml"),
            format!("occasions:\n  bossDefeated: {{ target: {{ prefix: boss, entity: foe, members: {list} }} }}\n"),
        )
        .unwrap();
        let errs = load_plugin_dir(&tmp).expect_err("a bad member list fails the load");
        assert!(
            errs.iter().any(|e| {
                let msg = e.to_string();
                matches!(e, lute_manifest::loader::LoadError::Parse { file, .. } if file.ends_with("b.yaml"))
                    && msg.contains("occasion `bossDefeated`")
                    && msg.contains(why)
            }),
            "{list}: {errs:?}"
        );
    }
}

/// A one-export package at a fresh temp dir: `plugin.yaml` exporting
/// `<export>: <export>/` plus `<export>/a.yaml` = `body`.
fn one_export_pkg(tag: &str, export: &str, body: &str) -> lute_scratch::ScratchDir {
    let tmp = lute_scratch::ScratchDir::named(tag);
    fs::create_dir_all(tmp.join(export)).unwrap();
    fs::write(
        tmp.join("plugin.yaml"),
        format!("id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  {export}: {export}/\n"),
    )
    .unwrap();
    fs::write(tmp.join(export).join("a.yaml"), body).unwrap();
    tmp
}

fn only_parse_msg(errs: &[LoadError]) -> &str {
    match errs {
        [LoadError::Parse { msg, .. }] => msg,
        other => panic!("expected one E-PLUGIN-PARSE, got {other:?}"),
    }
}

fn only_key_msg(errs: &[LoadError]) -> String {
    match errs {
        [e @ LoadError::Key { .. }] => e.to_string(),
        other => panic!("expected one E-PLUGIN-KEY, got {other:?}"),
    }
}

/// dsl 0.24.0 T1-3: `selct: all` used to load silently as `select: first`.
#[test]
fn unknown_occasion_key_is_rejected_with_did_you_mean() {
    let tmp = one_export_pkg(
        "oc_typo",
        "occasions",
        "occasions:\n  report: { selct: all, description: Pick one }\n",
    );
    let errs = load_plugin_dir(&tmp).unwrap_err();
    assert_eq!(errs[0].code(), "E-PLUGIN-KEY");
    let msg = only_key_msg(&errs);
    assert!(
        msg.contains("a.yaml:2:13: `occasions.report` has no key `selct` — did you mean `select`?"),
        "{msg}"
    );
}

#[test]
fn unknown_key_in_other_export_bodies_is_rejected() {
    for (tag, export, body, key, suggestion) in [
        (
            "rk_typo",
            "rewardKinds",
            "rewardKinds:\n  gold: { credit: run.gold }\n",
            "credit",
            "credits",
        ),
        (
            "dir_typo",
            "directives",
            "directives:\n  - { name: x, attrs: [], efects: { writes: [] } }\n",
            "efects",
            "effects",
        ),
        (
            "br_typo",
            "bridge",
            "bridgeCapabilities:\n  - { service: dice, operation: roll, replays: live }\n",
            "replays",
            "replay",
        ),
    ] {
        let tmp = one_export_pkg(tag, export, body);
        let errs = load_plugin_dir(&tmp).unwrap_err();
        let msg = only_key_msg(&errs);
        assert!(
            msg.contains(&format!("has no key `{key}`")),
            "{export}: {msg}"
        );
        assert!(
            msg.contains(&format!("— did you mean `{suggestion}`?")),
            "{export}: {msg}"
        );
        let _ = fs::remove_dir_all(&tmp); // best-effort cleanup
    }
}

/// An unquoted flow-map description split at its comma leaves a null-valued
/// key; the error says to quote the description, spelled out in full.
#[test]
fn null_valued_key_hints_to_quote_the_description() {
    let tmp = one_export_pkg(
        "oc_comma",
        "occasions",
        "occasions:\n  report: { select: all, description: Pick one, the player picks one }\n",
    );
    let errs = load_plugin_dir(&tmp).unwrap_err();
    let msg = only_key_msg(&errs);
    assert!(msg.contains("has no key `the player picks one`"), "{msg}");
    assert!(msg.contains("`the player picks one` has no value"), "{msg}");
    assert!(
        msg.contains(r#"quote the description: `description: "Pick one, the player picks one"`"#),
        "{msg}"
    );
    assert!(!msg.contains("did you mean"), "{msg}");
}

/// dsl 0.24.0 T3-7: `lower:` is optional; absent means the passthrough.
#[test]
fn directive_without_lower_loads_as_passthrough() {
    let tmp = one_export_pkg(
        "dir_nolower",
        "directives",
        "directives:\n  - { name: encounter, attrs: [ { name: id, required: true, type: string } ] }\n",
    );
    let loaded = load_plugin_dir(&tmp).expect("a directive without `lower:` loads");
    assert!(loaded.directives[0].lower.is_passthrough());
}

#[test]
fn unregistered_builtin_hook_is_a_parse_error() {
    let tmp = one_export_pkg(
        "dir_hook",
        "directives",
        "directives:\n  - { name: encounter, attrs: [], lower: { kind: builtin, name: encounter } }\n",
    );
    let errs = load_plugin_dir(&tmp).unwrap_err();
    let msg = only_parse_msg(&errs);
    assert!(
        msg.contains("`encounter` is not a builtin lowering hook"),
        "{msg}"
    );
    assert!(
        msg.contains(&lute_manifest::schema::BUILTIN_LOWERING_HOOKS.join(", ")),
        "{msg}"
    );
}

/// A package whose manifest parsed but whose export failed is reported by
/// assembly as failed to load (naming the load error's code), not as a
/// plugin that is not installed.
#[test]
fn failed_export_plugin_is_reported_as_failed_not_missing() {
    use lute_manifest::assemble::assemble_snapshot;
    use lute_manifest::resolve::ActivePlugin;
    let root_scratch = lute_scratch::ScratchDir::new();
    let root = root_scratch.join("lute_plugins_failed");
    fs::create_dir_all(&root).unwrap();
    let pkg = one_export_pkg(
        "failed_member",
        "occasions",
        "occasions:\n  report: { selct: all }\n",
    );
    fs::rename(&pkg, root.join("t.plug")).unwrap();
    let (reg, errs) = lute_manifest::loader::load_plugins_dir(&root);
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(reg.get("t.plug").is_none());
    let active = [ActivePlugin {
        id: "t.plug".into(),
        options: Default::default(),
    }];
    let (_snap, aerrs) = assemble_snapshot(&active, &reg);
    assert_eq!(aerrs.len(), 1, "{aerrs:?}");
    assert_eq!(aerrs[0].code(), "E-PLUGIN-MISSING-ACTIVE");
    let msg = aerrs[0].to_string();
    assert!(
        msg.contains("failed to load (see E-PLUGIN-KEY above)"),
        "{msg}"
    );
    assert!(!msg.contains("not installed"), "{msg}");
    // A plugin no package declares still reads as not installed.
    let (_snap, aerrs) = assemble_snapshot(
        &[ActivePlugin {
            id: "t.absent".into(),
            options: Default::default(),
        }],
        &reg,
    );
    assert!(
        aerrs[0].to_string().contains("is not installed"),
        "{aerrs:?}"
    );
}

/// A cast id two files of one package declare is reported at the later
/// declaration, naming the first by its package-relative `file:line`; the
/// package still installs with the first declaration and the rest of the
/// later file, so assembly sees it instead of a plugin that failed to load.
#[test]
fn duplicate_cast_id_is_located_and_the_package_still_loads() {
    use lute_manifest::assemble::assemble_snapshot;
    use lute_manifest::resolve::ActivePlugin;
    let root_scratch = lute_scratch::ScratchDir::new();
    let root = root_scratch.join("lute_plugins_dupcast");
    fs::create_dir_all(&root).unwrap();
    let pkg = one_export_pkg(
        "dup_cast",
        "cast",
        "cast:\n  maud: { name: Maud }\n  orla: { name: Captain Orla }\n",
    );
    fs::write(
        pkg.join("cast/b.yaml"),
        "cast:\n  wren: { name: Wren }\n  orla: { name: Orla Again }\n",
    )
    .unwrap();
    fs::rename(&pkg, root.join("t.plug")).unwrap();
    let (reg, errs) = lute_manifest::loader::load_plugins_dir(&root);
    let [err] = errs.as_slice() else {
        panic!("expected one duplicate, got {errs:?}");
    };
    let shown = err.to_string();
    assert_eq!(err.code(), "E-PLUGIN-DUP-ID");
    assert!(
        shown.ends_with(
            "t.plug/cast/b.yaml:3:3: cast `orla` is already declared at cast/a.yaml:3; \
             this declaration is ignored — rename or remove one"
        ),
        "{shown}"
    );
    let loaded = &reg
        .get("t.plug")
        .expect("installed despite the duplicate")
        .loaded;
    let names: Vec<(&str, Option<&str>)> = loaded
        .cast
        .iter()
        .map(|c| (c.id.as_str(), c.name.as_deref()))
        .collect();
    assert_eq!(
        names,
        [
            ("maud", Some("Maud")),
            ("orla", Some("Captain Orla")),
            ("wren", Some("Wren"))
        ]
    );
    let active = [ActivePlugin {
        id: "t.plug".into(),
        options: Default::default(),
    }];
    let (snap, aerrs) = assemble_snapshot(&active, &reg);
    assert!(aerrs.is_empty(), "{aerrs:?}");
    assert!(snap.cast.contains_key("wren"));
}

/// dsl 0.24.0 §4: a plugin cast entry carries `present:` and `emotions:`
/// through to the loaded member; any other key is `E-PLUGIN-KEY`.
#[test]
fn cast_export_carries_present_and_emotions_and_rejects_unknown_keys() {
    let tmp = one_export_pkg(
        "cast_party",
        "cast",
        "cast:\n  isolde:\n    name: Isolde\n    present: \"holds(inParty(isolde))\"\n    \
         emotions: [calm, fierce]\n  maud: { name: Maud }\n",
    );
    let p = load_plugin_dir(&tmp).expect("valid cast export loads");
    let isolde = p.cast.iter().find(|c| c.id == "isolde").expect("isolde");
    assert_eq!(isolde.name.as_deref(), Some("Isolde"));
    assert_eq!(isolde.present.as_deref(), Some("holds(inParty(isolde))"));
    assert_eq!(
        isolde.emotions.as_deref(),
        Some(&["calm".to_string(), "fierce".to_string()][..])
    );
    let maud = p.cast.iter().find(|c| c.id == "maud").expect("maud");
    assert_eq!(
        (maud.present.as_deref(), maud.emotions.as_deref()),
        (None, None)
    );
    let _ = fs::remove_dir_all(&tmp); // best-effort cleanup

    let tmp = one_export_pkg(
        "cast_typo",
        "cast",
        "cast:\n  isolde: { name: Isolde, presnt: \"holds(inParty(isolde))\" }\n",
    );
    let errs = load_plugin_dir(&tmp).unwrap_err();
    let msg = only_key_msg(&errs);
    assert!(
        msg.contains("has no key `presnt` — did you mean `present`?"),
        "{msg}"
    );
}

/// A package whose `plugin.yaml` is `manifest` and exports nothing but an
/// empty `occasions/`.
fn manifest_pkg(tag: &str, manifest: &str) -> lute_scratch::ScratchDir {
    let tmp = lute_scratch::ScratchDir::named(tag);
    fs::create_dir_all(tmp.join("occasions")).unwrap();
    fs::write(tmp.join("plugin.yaml"), manifest).unwrap();
    fs::write(tmp.join("occasions/a.yaml"), "occasions:\n  talk: {}\n").unwrap();
    tmp
}

/// dsl 0.28.0 §1 (T1-1): `plugin.yaml`'s unknown keys used to be dropped
/// silently (`dependencies:` for `depends:`, `kind: plugin`). Each is an
/// `E-PLUGIN-KEY` at its line, and the failure still carries the plugin id
/// (assembly says it failed to load, not that it is not installed).
#[test]
fn plugin_manifest_rejects_unknown_keys_and_a_non_capability_kind() {
    for (tag, manifest, says) in [
        (
            "deps",
            "id: t.plug\nversion: 0.1.0\nkind: capability\n\
             dependencies: [ { id: lute.core, range: \"^0.0.1\" } ]\nexports: { occasions: occasions/ }\n",
            "plugin.yaml:4:1: `plugin.yaml` has no key `dependencies` — did you mean `depends`?",
        ),
        (
            "kind",
            "id: t.plug\nversion: 0.1.0\nkind: plugin\nexports: { occasions: occasions/ }\n",
            "plugin.yaml:3:1: `kind: plugin` is not a plugin kind — a plugin package declares \
             `kind: capability`",
        ),
        (
            "dep_entry",
            "id: t.plug\nversion: 0.1.0\nkind: capability\n\
             depends:\n  - { id: lute.core, rnage: \"^0.0.1\" }\nexports: { occasions: occasions/ }\n",
            "a `depends:` entry has no key `rnage` — did you mean `range`?",
        ),
    ] {
        let tmp = manifest_pkg(tag, manifest);
        let root = tmp.parent().unwrap().join(format!("lute_pkgs_man_{tag}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        fs::rename(&tmp, root.join("t.plug")).unwrap();
        let (reg, errs) = lute_manifest::loader::load_plugins_dir(&root);
        assert_eq!(errs.len(), 1, "{tag}: {errs:?}");
        assert_eq!(errs[0].code(), "E-PLUGIN-KEY", "{tag}");
        let msg = errs[0].to_string();
        assert!(msg.contains(says), "{tag}: {msg}");
        assert!(reg.failed.contains_key("t.plug"), "{tag}: the id is known");
        let _ = fs::remove_dir_all(&root); // best-effort cleanup
    }
    let tmp = manifest_pkg("noid", "version: 0.1.0\nkind: capability\nexports: {}\n");
    let errs = load_plugin_dir(&tmp).unwrap_err();
    assert_eq!(errs[0].code(), "E-PLUGIN-MANIFEST");
    assert!(
        errs[0]
            .to_string()
            .contains("`plugin.yaml` names no `id:` — add `id: <plugin id>`"),
        "{errs:?}"
    );
}

/// dsl 0.28.0 T3-20: an occasion `target:` typo named serde's "did not match
/// any variant of untagged enum OccasionTarget". It names the bad key, the
/// key meant, and the legal shape, at its line.
#[test]
fn occasion_target_typos_name_the_key_meant() {
    for (tag, target, says) in [
        (
            "kind",
            "{ prefix: npc, kind: crew }",
            "`target:` has no key `kind` — did you mean `entity`?",
        ),
        (
            "domain",
            "{ prefix: isle, domain: island }",
            "has no key `domain` — did you mean `entity`?",
        ),
        (
            "member",
            "{ prefix: npc, entity: crew, member: [mira] }",
            "has no key `member` — did you mean `members`?",
        ),
        (
            "noentity",
            "{ prefix: npc }",
            "`target:` names no `entity:`",
        ),
        ("string", "npc", "`target: npc` is not a target"),
    ] {
        let tmp = one_export_pkg(
            &format!("oct_{tag}"),
            "occasions",
            &format!(
                "occasions:\n  hubVisit: {{}}\n  talk: {{ select: first, target: {target} }}\n"
            ),
        );
        let errs = load_plugin_dir(&tmp).unwrap_err();
        let msg = only_parse_msg(&errs).to_string();
        let shown = errs[0].to_string();
        assert!(msg.contains(says), "{tag}: {msg}");
        assert!(
            msg.contains("`{ prefix: <id prefix>, entity: <entity kind>"),
            "{tag}: {msg}"
        );
        assert!(
            !shown.contains("untagged") && !shown.contains("OccasionTarget"),
            "{shown}"
        );
        assert!(shown.contains("a.yaml:3:"), "{tag}: located: {shown}");
        let _ = fs::remove_dir_all(&tmp); // best-effort cleanup
    }
}

/// dsl 0.28.0 T3-20 (LO-26): a directive without attributes may leave out
/// `attrs:` — it used to fail the whole plugin with `missing field attrs`.
#[test]
fn a_directive_without_attrs_loads() {
    let tmp = one_export_pkg("noattrs", "directives", "directives:\n  - name: tip\n");
    let loaded = load_plugin_dir(&tmp).expect("a directive may omit `attrs:`");
    assert!(loaded.directives[0].attrs.is_empty());
    assert!(
        loaded
            .site("directive", "tip")
            .is_some_and(|s| s.ends_with("a.yaml:2:11")),
        "{:?}",
        loaded.sites
    );
}

/// dsl 0.28.0 T3-20 (audit#22): a top-level key of the wrong shape says what
/// the key holds, in YAML's words; a YAML syntax slip is the plain sentence
/// at its line; neither shows serde's or Rust's own wording.
#[test]
fn export_shape_and_syntax_errors_are_plain() {
    for (tag, export, body, says) in [
        (
            "oclist",
            "occasions",
            "occasions:\n  - talk\n",
            "`occasions` is a list, where a mapping is expected — `occasions:` maps each occasion \
             name to its declaration",
        ),
        (
            "evmap",
            "events",
            "events:\n  combatEnd: {}\n",
            "`events` is a mapping, where a list is expected — `events:` is a list",
        ),
        (
            "enumtypo",
            "enums",
            "enums:\n  mood: { membres: [calm] }\n",
            "has no key `membres` — did you mean `members`?",
        ),
        (
            "select",
            "occasions",
            "occasions:\n  talk: { select: firts }\n",
            "is `firts`, which is not one of first, all, sequence — did you mean `first`?",
        ),
        ("syntax", "events", "events: [ { name: a }\n", "a.yaml:"),
    ] {
        let tmp = one_export_pkg(&format!("shape_{tag}"), export, body);
        let errs = load_plugin_dir(&tmp).unwrap_err();
        assert_eq!(errs.len(), 1, "{tag}: {errs:?}");
        let msg = errs[0].to_string();
        assert!(msg.contains(says), "{tag}: {msg}");
        for leak in [
            "invalid type",
            "untagged",
            "struct ",
            "at line",
            "expected one of",
        ] {
            assert!(!msg.contains(leak), "{tag}: `{leak}` leaked: {msg}");
        }
        let _ = fs::remove_dir_all(&tmp); // best-effort cleanup
    }
}

/// dsl 0.28.0: an occasion's payload field types parse explicitly. An
/// incomplete (`list`, `enum`) or misspelled type used to fail with serde's
/// "unit variant, where newtype variant is expected"; it names the field,
/// what is wrong, and the forms a payload field takes, at its line. The
/// forms a raise can give load as their `Type`.
#[test]
fn occasion_payload_types_are_named_in_plain_words() {
    for (tag, ty, says) in [
        (
            "list",
            "list",
            "payload field `copies` cannot be a `list`; a payload field's type is",
        ),
        (
            "listof",
            "{ list: int }",
            "payload field `copies` cannot be a `list`",
        ),
        (
            "typo",
            "nit",
            "payload field `copies` has the type `nit`, which is not a payload type — did you \
             mean `int`?",
        ),
        (
            "bareenum",
            "enum",
            "payload field `copies` is typed `enum` without its argument — write `copies: { enum: \
             [<member>, …] }`",
        ),
        (
            "nokind",
            "{ domain: 3 }",
            "`{ domain: 3 }` names no kind — write `{ domain: <kind> }`",
        ),
        (
            "formtypo",
            "{ entiy: hero }",
            "has the type `entiy`, which is not a payload type — did you mean `entity`?",
        ),
        ("empty", "", "payload field `copies` names no type"),
    ] {
        let tmp = one_export_pkg(
            &format!("payload_{tag}"),
            "occasions",
            &format!(
                "occasions:\n  hubVisit: {{}}\n  summon:\n    select: first\n    payload:\n      \
                 seen: bool\n      copies: {ty}\n"
            ),
        );
        let errs = load_plugin_dir(&tmp).unwrap_err();
        let msg = only_parse_msg(&errs).to_string();
        let shown = errs[0].to_string();
        assert!(msg.contains(says), "{tag}: {msg}");
        assert!(
            msg.contains("`{ enum: [<member>, …] }`") || tag == "bareenum" || tag == "nokind",
            "{tag}: names the forms: {msg}"
        );
        for leak in ["variant", "Type", "invalid type"] {
            assert!(!shown.contains(leak), "{tag}: `{leak}` leaked: {shown}");
        }
        // At the field's own line, not the occasion's.
        assert!(shown.contains("a.yaml:7:"), "{tag}: located: {shown}");
        let _ = fs::remove_dir_all(&tmp); // best-effort cleanup
    }

    let tmp = one_export_pkg(
        "payload_ok",
        "occasions",
        "occasions:\n  summon: { payload: { seen: bool, copies: int, note: string, \
         mood: { enum: [calm, storm] }, to: { domain: route }, who: { entity: hero } } }\n",
    );
    let loaded = load_plugin_dir(&tmp).expect("every payload form loads");
    let payload = &loaded.occasions[0].payload;
    use lute_manifest::types::Type;
    assert_eq!(payload["seen"], Type::Bool);
    assert_eq!(payload["copies"], Type::Int);
    assert_eq!(payload["note"], Type::Str);
    assert_eq!(
        payload["mood"],
        Type::Enum(vec!["calm".into(), "storm".into()])
    );
    assert_eq!(payload["to"], Type::Domain("route".into()));
    assert_eq!(payload["who"], Type::Entity("hero".into()));
}
