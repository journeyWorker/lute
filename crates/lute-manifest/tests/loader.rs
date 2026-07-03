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
