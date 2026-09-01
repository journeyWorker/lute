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
        "directives:\n  - { name: foo, attrs: [], lower: { kind: builtin, name: n } }\n  - { name: foo, attrs: [], lower: { kind: builtin, name: n } }\n"
    } else {
        "directives:\n  - { name: foo, attrs: [ { name: x, type: bool } ], lower: { kind: builtin, name: n } }\n"
    };
    fs::write(root.join("directives/a.yaml"), d).unwrap();
}

#[test]
fn loads_a_valid_package() {
    let tmp = std::env::temp_dir().join(format!("lute_pkg_ok_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    write_pkg(&tmp, false);
    let p = load_plugin_dir(&tmp).expect("valid package loads");
    assert_eq!(p.manifest.id, "t.plug");
    assert_eq!(p.directives.len(), 1);
    assert_eq!(p.directives[0].name, "foo");
    fs::remove_dir_all(&tmp).ok();
}

#[test]
fn rejects_duplicate_directive_id() {
    let tmp = std::env::temp_dir().join(format!("lute_pkg_dup_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    write_pkg(&tmp, true);
    let errs = load_plugin_dir(&tmp).unwrap_err();
    assert!(errs.iter().any(
        |e| matches!(e, LoadError::DuplicateId { kind, id } if kind == "directive" && id == "foo")
    ));
    fs::remove_dir_all(&tmp).ok();
}

#[test]
fn rejects_missing_export_dir() {
    let tmp = std::env::temp_dir().join(format!("lute_pkg_miss_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
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
    fs::remove_dir_all(&tmp).ok();
}

#[test]
fn unreadable_declaration_file_is_error() {
    use std::io::Write;
    let tmp = std::env::temp_dir().join(format!("lute_pkg_badenc_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
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
    fs::remove_dir_all(&tmp).ok();
}

#[test]
fn scans_a_plugins_directory() {
    let root = std::env::temp_dir().join(format!("lute_plugins_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    write_pkg(&root.join("t.plug"), false); // reuse the helper; nested dir = plugin id
    let (reg, errs) = lute_manifest::loader::load_plugins_dir(&root);
    assert!(errs.is_empty(), "{errs:?}");
    assert!(reg.get("t.plug").is_some());
    std::fs::remove_dir_all(&root).ok();
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
    let tmp = std::env::temp_dir().join(format!("lute_pkg_badexport_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
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
    fs::remove_dir_all(&tmp).ok();
}

/// Build a plugin package that exports `assetkinds/`; when `dup`, the file
/// declares two `kind: CH` entries (a per-package duplicate).
fn write_asset_pkg(root: &std::path::Path, dup: bool) {
    fs::create_dir_all(root.join("assetkinds")).unwrap();
    fs::write(
        root.join("plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  assetkinds: assetkinds/\n",
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
    let tmp = std::env::temp_dir().join(format!("lute_pkg_ak_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    write_asset_pkg(&tmp, false);
    let p = load_plugin_dir(&tmp).expect("asset-kind package loads");
    assert_eq!(p.asset_kinds.len(), 1);
    assert_eq!(p.asset_kinds[0].kind, "CH");
    fs::remove_dir_all(&tmp).ok();
}

#[test]
fn loads_asset_kinds_rejects_dup() {
    let tmp = std::env::temp_dir().join(format!("lute_pkg_akdup_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    write_asset_pkg(&tmp, true);
    let errs = load_plugin_dir(&tmp).unwrap_err();
    assert!(
        errs.iter().any(|e| matches!(
            e,
            LoadError::DuplicateId { kind, id } if kind == "assetKind" && id == "CH"
        )),
        "dup asset kind must be DuplicateId{{kind:\"assetKind\"}}, got {errs:?}"
    );
    fs::remove_dir_all(&tmp).ok();
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
    let tmp = std::env::temp_dir().join(format!("lute_pkg_ev_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    write_events_pkg(&tmp);
    let loaded = load_plugin_dir(&tmp).expect("loads");
    assert_eq!(loaded.events.len(), 1);
    assert_eq!(loaded.events[0].name, "combatEnd");
    fs::remove_dir_all(&tmp).ok();
}

/// plugin §14.1: the `stampattrs` export loads off disk exactly like its
/// sibling export kinds — `stampAttrs:` entries deserialize as ordinary
/// `AttrDecl`s (name + `type:` tagged map), and a per-package duplicate name
/// is a `DuplicateId` from the shared `merge_named` path.
fn write_stamp_attrs_pkg(root: &std::path::Path, dup: bool) {
    fs::create_dir_all(root.join("stampattrs")).unwrap();
    fs::write(
        root.join("plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  stampattrs: stampattrs/\n",
    )
    .unwrap();
    let second = if dup { "bonusId" } else { "bonusScore" };
    fs::write(
        root.join("stampattrs/a.yaml"),
        format!(
            "stampAttrs:\n  - name: bonusId\n    type: string\n  - name: {second}\n    type: number\n"
        ),
    )
    .unwrap();
}

#[test]
fn loads_stamp_attrs_export() {
    let tmp = std::env::temp_dir().join(format!("lute_pkg_sa_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    write_stamp_attrs_pkg(&tmp, false);
    let loaded = load_plugin_dir(&tmp).expect("loads");
    assert_eq!(loaded.stamp_attrs.len(), 2);
    assert_eq!(loaded.stamp_attrs[0].name, "bonusId");
    assert!(matches!(
        loaded.stamp_attrs[0].ty,
        lute_manifest::types::Type::Str
    ));
    assert!(matches!(
        loaded.stamp_attrs[1].ty,
        lute_manifest::types::Type::Number
    ));
    fs::remove_dir_all(&tmp).ok();
}

#[test]
fn loads_stamp_attrs_rejects_dup() {
    let tmp = std::env::temp_dir().join(format!("lute_pkg_sadup_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    write_stamp_attrs_pkg(&tmp, true);
    let errs = load_plugin_dir(&tmp).expect_err("dup stampAttr name");
    assert!(
        errs.iter().any(|e| matches!(
            e,
            lute_manifest::loader::LoadError::DuplicateId { kind, id }
                if kind == "stampAttr" && id == "bonusId"
        )),
        "{errs:?}"
    );
    fs::remove_dir_all(&tmp).ok();
}

/// Build a plugin package exporting `assetkinds/` whose single kind `CH` has
/// a `const` prefix segment plus one `variant` segment typed by `type_yaml`
/// (a raw `type:` value fragment, e.g. `"bool"` or `"{ domain: slotDomain }"`).
fn write_asset_pkg_with_segment_type(root: &std::path::Path, type_yaml: &str) {
    fs::create_dir_all(root.join("assetkinds")).unwrap();
    fs::write(
        root.join("plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  assetkinds: assetkinds/\n",
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
    let tmp = std::env::temp_dir().join(format!(
        "lute_pkg_segty_{dir_suffix}_{}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&tmp);
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
    fs::remove_dir_all(&tmp).ok();
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
/// `number`, `string` (the exact set `validate_segments` enforces) — must
/// still load cleanly side by side, proving the fix narrowed the declaration
/// surface without disturbing legitimate segment declarations.
#[test]
fn admits_enum_number_string_provider_ref_segments() {
    let tmp = std::env::temp_dir().join(format!("lute_pkg_segty_admit_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(tmp.join("assetkinds")).unwrap();
    fs::write(
        tmp.join("plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  assetkinds: assetkinds/\n",
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
           - { name: variant, type: number }\n",
    )
    .unwrap();
    let p = load_plugin_dir(&tmp).expect("providerRef/enum/number/string segments must load");
    assert_eq!(p.asset_kinds[0].segments.len(), 5);
    fs::remove_dir_all(&tmp).ok();
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
    let root = std::env::temp_dir().join(format!("lute_plugins_domseg_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let pkg = root.join("t.plug");
    fs::create_dir_all(pkg.join("assetkinds")).unwrap();
    fs::write(
        pkg.join("plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  assetkinds: assetkinds/\n",
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
    fs::remove_dir_all(&root).ok();
}

/// Defect 1: `AssetSegmentType`'s rendered message must be prose — the kind,
/// the segment, the declared type, and the closed admitted set — never the
/// `Debug` struct dump (`AssetSegmentType { file: …, kind: …, .. }`) a raw
/// `{e:?}` format produces. Written first, failing, per this repo's TDD
/// convention.
#[test]
fn asset_segment_type_message_is_prose() {
    let tmp = std::env::temp_dir().join(format!("lute_pkg_segty_msg_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
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
    for admitted in ["enum", "number", "string", "providerRef"] {
        assert!(
            msg.contains(admitted),
            "must list admitted type `{admitted}`: {msg}"
        );
    }
    fs::remove_dir_all(&tmp).ok();
}

/// Defect 2: `AssetSegmentType.file` must name the `.yaml` that declared the
/// bad segment, not the `assetkinds/` export directory it was read from
/// (which is what `check_asset_segment_types` was anchored at before). The
/// suffix assertion keeps this host-independent (no absolute-path compare).
#[test]
fn asset_segment_type_file_names_the_yaml() {
    let tmp = std::env::temp_dir().join(format!("lute_pkg_segty_file_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
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
    fs::remove_dir_all(&tmp).ok();
}

/// Pins the `Display` rendering of a PRE-EXISTING variant (`Parse`), so the
/// new `impl Display for LoadError` is exercised for more than just the one
/// variant this defect report cares about. Malformed YAML syntax (not just a
/// shape mismatch) so `serde_yaml::from_str` fails before `merge` ever runs.
#[test]
fn parse_error_message_is_prose() {
    let tmp = std::env::temp_dir().join(format!("lute_pkg_parsemsg_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
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
    fs::remove_dir_all(&tmp).ok();
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
    use lute_manifest::lint::{LintLevel, LintTarget};
    let tmp = std::env::temp_dir().join(format!("lute_pkg_lints_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    write_lints_pkg(&tmp, false);
    let loaded = load_plugin_dir(&tmp).expect("lints package loads");
    assert_eq!(loaded.lints.len(), 2);
    let r0 = &loaded.lints[0];
    assert_eq!(r0.id, "too-many-choices");
    assert_eq!(r0.target, LintTarget::Scene);
    assert_eq!(r0.when, "scene.choices > options.max");
    assert_eq!(r0.level, Some(LintLevel::Warn));
    assert_eq!(r0.message, "scene has {scene.choices} choices");
    // options are the raw serde_yaml::Mapping — { max: 6 } is present.
    assert_eq!(
        r0.options
            .get(serde_yaml::Value::String("max".into()))
            .and_then(|v| v.as_i64()),
        Some(6)
    );
    // level absent -> None (plugin declines to declare a default)
    assert_eq!(loaded.lints[1].level, None);
    assert_eq!(loaded.lints[1].target, LintTarget::Line);
    fs::remove_dir_all(&tmp).ok();
}

#[test]
fn loads_lints_rejects_dup_id() {
    let tmp = std::env::temp_dir().join(format!("lute_pkg_lintsdup_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    write_lints_pkg(&tmp, true);
    let errs = load_plugin_dir(&tmp).expect_err("dup lint id");
    assert!(
        errs.iter().any(|e| matches!(
            e,
            LoadError::DuplicateId { kind, id }
                if kind == "lint" && id == "too-many-choices"
        )),
        "duplicate rule id must surface DuplicateId{{kind:\"lint\"}}, got {errs:?}"
    );
    fs::remove_dir_all(&tmp).ok();
}

/// A malformed `lints/*.yaml` file (bad YAML structure) surfaces through
/// the loader's shared per-file `LoadError::Parse` channel — the load-time
/// flavor of the design's `E-LINT-RULE`, using the SAME anchor every
/// other export uses (see `parse_error_message_is_prose` above).
#[test]
fn malformed_lint_file_is_parse_error() {
    let tmp = std::env::temp_dir().join(format!("lute_pkg_lintsbad_{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
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
    fs::remove_dir_all(&tmp).ok();
}

/// Lints must NEVER perturb `CapabilitySnapshot` / `capabilityVersion`
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
    let root_a = std::env::temp_dir().join(format!("lute_lints_hashA_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root_a);
    fs::create_dir_all(root_a.join("t.plug/directives")).unwrap();
    fs::write(
        root_a.join("t.plug/plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  directives: directives/\n",
    )
    .unwrap();
    fs::write(
        root_a.join("t.plug/directives/a.yaml"),
        "directives:\n  - { name: foo, attrs: [ { name: x, type: bool } ], lower: { kind: builtin, name: n } }\n",
    )
    .unwrap();

    // Package B: same, PLUS a lints export.
    let root_b = std::env::temp_dir().join(format!("lute_lints_hashB_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root_b);
    fs::create_dir_all(root_b.join("t.plug/directives")).unwrap();
    fs::create_dir_all(root_b.join("t.plug/lints")).unwrap();
    fs::write(
        root_b.join("t.plug/plugin.yaml"),
        "id: t.plug\nversion: 0.1.0\nkind: capability\nexports:\n  directives: directives/\n  lints: lints/\n",
    )
    .unwrap();
    fs::write(
        root_b.join("t.plug/directives/a.yaml"),
        "directives:\n  - { name: foo, attrs: [ { name: x, type: bool } ], lower: { kind: builtin, name: n } }\n",
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
        "capabilityVersion must be byte-identical whether a plugin ships lints or not"
    );

    fs::remove_dir_all(&root_a).ok();
    fs::remove_dir_all(&root_b).ok();
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

    let root = std::env::temp_dir().join(format!("lute_lints_ns_{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);

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

    fs::remove_dir_all(&root).ok();
}
