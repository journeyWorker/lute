//! 0.50.0 §3.1–§3.3 (#163): `stampAttrs` entries scoped to named surfaces
//! with `on:` — the load faults, the per-package and cross-plugin overlap
//! rules, surface resolution, shadowing, and the `capabilitySnapshot` fold.
//! Every package here is written to disk and goes through the real loader
//! and assembler.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use lute_manifest::assemble::{assemble_snapshot, AssembleError};
use lute_manifest::loader::{load_plugins_dir, LoadError};
use lute_manifest::resolve::ActivePlugin;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::stamps::StampSurface;
use lute_manifest::types::Type;

/// One plugin package: its id, `depends:` ids, `stampAttrs` file and an
/// optional `directives` file.
struct Pkg<'a> {
    id: &'a str,
    depends: &'a [&'a str],
    stamps: &'a str,
    directives: Option<&'a str>,
}

fn write_pkg(root: &Path, pkg: &Pkg) {
    let dir = root.join(pkg.id);
    fs::create_dir_all(dir.join("stampattrs")).unwrap();
    let mut depends = String::from("[{ id: lute.core, range: \"^0.0.1\" }");
    for d in pkg.depends {
        depends.push_str(&format!(", {{ id: {d}, range: \"^0.1.0\" }}"));
    }
    depends.push(']');
    let mut exports = String::from("  stampAttrs: stampattrs/\n");
    if let Some(directives) = pkg.directives {
        fs::create_dir_all(dir.join("directives")).unwrap();
        fs::write(dir.join("directives/d.yaml"), directives).unwrap();
        exports.push_str("  directives: directives/\n");
    }
    fs::write(
        dir.join("plugin.yaml"),
        format!(
            "id: {}\nversion: 0.1.0\nkind: capability\ndepends: {depends}\nexports:\n{exports}",
            pkg.id
        ),
    )
    .unwrap();
    fs::write(dir.join("stampattrs/s.yaml"), pkg.stamps).unwrap();
}

/// Load `pkgs` from disk and assemble the ones in `active` (plus core).
fn assemble(
    pkgs: &[Pkg],
    active: &[&str],
) -> (CapabilitySnapshot, Vec<LoadError>, Vec<AssembleError>) {
    let scratch = lute_scratch::ScratchDir::new();
    let root = scratch.join("plugins");
    for pkg in pkgs {
        write_pkg(&root, pkg);
    }
    let (installed, load_errs) = load_plugins_dir(&root);
    let mut plugins = vec![ActivePlugin {
        id: "lute.core".into(),
        options: BTreeMap::new(),
    }];
    plugins.extend(active.iter().map(|id| ActivePlugin {
        id: (*id).to_string(),
        options: BTreeMap::new(),
    }));
    let (snap, errs) = assemble_snapshot(&plugins, &installed);
    (snap, load_errs, errs)
}

fn one(id: &str, stamps: &str) -> (CapabilitySnapshot, Vec<LoadError>, Vec<AssembleError>) {
    assemble(
        &[Pkg {
            id,
            depends: &[],
            stamps,
            directives: None,
        }],
        &[id],
    )
}

/// The single load error's code and message (without its `path:line:col`).
fn load_fault(errs: &[LoadError]) -> (&'static str, String) {
    assert_eq!(errs.len(), 1, "{errs:?}");
    let shown = errs[0].to_string();
    let message = shown
        .split_once(": ")
        .map_or(shown.as_str(), |(_, m)| m)
        .to_string();
    (errs[0].code(), message)
}

fn bg() -> StampSurface {
    StampSurface::Directive("bg".to_string())
}

#[test]
fn required_without_on_is_a_parse_error() {
    let (_, load, _) = one(
        "acme.stage",
        "stampAttrs:\n  - { name: voiceAssetId, type: string, required: true }\n",
    );
    assert_eq!(
        load_fault(&load),
        (
            "E-PLUGIN-PARSE",
            "stamp attribute `voiceAssetId` is `required: true` but has no `on:`; a stamp \
             required on every record is not a column — name its surfaces with `on: […]`"
                .to_string()
        )
    );
}

#[test]
fn malformed_on_lists_are_parse_errors() {
    for (on, message) in [
        (
            "[]",
            "stamp attribute `x` has an empty `on:` — list the surfaces it applies to, or omit \
             `on:` for every directive and content line",
        ),
        (
            "",
            "stamp attribute `x` has an empty `on:` — list the surfaces it applies to, or omit \
             `on:` for every directive and content line",
        ),
        ("[bg, bg]", "stamp attribute `x` lists `bg` twice in `on:`"),
        (
            "bg",
            "stamp attribute `x` has `on: bg`, which is not a list — write `on: [bg]`",
        ),
        (
            "[1]",
            "stamp attribute `x` has an `on:` that is not a list of surface names — write \
             `on: [line, bg]`, naming `line`, `branch`, `hub`, `choice` or a directive",
        ),
    ] {
        let (_, load, _) = one(
            "acme.stage",
            &format!("stampAttrs:\n  - {{ name: x, type: string, on: {on} }}\n"),
        );
        assert_eq!(
            load_fault(&load),
            ("E-PLUGIN-PARSE", message.to_string()),
            "on: {on}"
        );
    }
}

#[test]
fn one_name_on_disjoint_surfaces_keeps_each_type() {
    let (snap, load, errs) = one(
        "acme.stage",
        "stampAttrs:\n  - { name: assetName, type: string, on: [bg] }\n  - { name: assetName, \
         type: int, on: [actor] }\n",
    );
    assert!(load.is_empty() && errs.is_empty(), "{load:?} {errs:?}");
    let actor = StampSurface::Directive("actor".to_string());
    assert_eq!(
        snap.stamp_attrs.get(&bg(), "assetName").unwrap().ty,
        Type::Str
    );
    assert_eq!(
        snap.stamp_attrs.get(&actor, "assetName").unwrap().ty,
        Type::Int
    );
    assert!(snap
        .stamp_attrs
        .get(&StampSurface::Line, "assetName")
        .is_none());
}

#[test]
fn overlap_within_a_package_is_a_duplicate_naming_the_surface() {
    let (snap, load, _) = one(
        "acme.stage",
        "stampAttrs:\n  - { name: assetName, type: string, on: [bg, actor] }\n  - { name: \
         assetName, type: int, on: [bg] }\n",
    );
    assert_eq!(
        load_fault(&load),
        (
            "E-PLUGIN-DUP-ID",
            "stampAttr `assetName` is declared twice on the surface `bg` (first at \
             stampattrs/s.yaml:2) — give each entry disjoint `on:` surfaces"
                .to_string()
        )
    );
    assert_eq!(
        snap.stamp_attrs.get(&bg(), "assetName").unwrap().ty,
        Type::Str
    );
    // An entry without `on:` overlaps every entry.
    let (_, load, _) = one(
        "acme.stage",
        "stampAttrs:\n  - { name: cue, type: string }\n  - { name: cue, type: string, on: [hub] \
         }\n",
    );
    assert_eq!(load_fault(&load).0, "E-PLUGIN-DUP-ID");
    // Two cross-cutting entries keep the 0.49 wording.
    let (_, load, _) = one(
        "acme.stage",
        "stampAttrs:\n  - { name: cue, type: string }\n  - { name: cue, type: int }\n",
    );
    assert_eq!(
        load_fault(&load).1,
        "stampAttr `cue` is already declared at stampattrs/s.yaml:2; this declaration is \
         ignored — rename or remove one"
    );
}

#[test]
fn overlap_across_plugins_names_both_plugins_and_the_shared_surfaces() {
    let (snap, load, errs) = assemble(
        &[
            Pkg {
                id: "a.one",
                depends: &[],
                stamps: "stampAttrs:\n  - { name: tag, type: string, on: [bg, line] }\n",
                directives: None,
            },
            Pkg {
                id: "b.two",
                depends: &[],
                stamps: "stampAttrs:\n  - { name: tag, type: int, on: [line, bg, sfx] }\n  - \
                         { name: mood, type: string, on: [choice] }\n",
                directives: None,
            },
        ],
        &["a.one", "b.two"],
    );
    assert!(load.is_empty(), "{load:?}");
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert_eq!(errs[0].code(), "E-PLUGIN-DUP-ACROSS");
    let shown = errs[0].to_string();
    assert!(
        shown.contains("stampAttr `tag` is declared by both `a.one` (at "),
        "{shown}"
    );
    assert!(
        shown.ends_with(
            " and `b.two` on content lines and `::bg`; two active plugins cannot own one stamp \
             on the same surface — `a.one`'s is used"
        ),
        "{shown}"
    );
    // The refused entry is dropped; the sibling lands.
    assert_eq!(snap.stamp_attrs.get(&bg(), "tag").unwrap().ty, Type::Str);
    assert!(snap
        .stamp_attrs
        .get(&StampSurface::Directive("sfx".into()), "tag")
        .is_none());
    assert!(snap
        .stamp_attrs
        .get(&StampSurface::Choice, "mood")
        .is_some());
    // Disjoint entries from two plugins are legal.
    let (snap, load, errs) = assemble(
        &[
            Pkg {
                id: "a.one",
                depends: &[],
                stamps: "stampAttrs:\n  - { name: tag, type: string, on: [bg] }\n",
                directives: None,
            },
            Pkg {
                id: "b.two",
                depends: &[],
                stamps: "stampAttrs:\n  - { name: tag, type: int, on: [sfx] }\n",
                directives: None,
            },
        ],
        &["a.one", "b.two"],
    );
    assert!(load.is_empty() && errs.is_empty(), "{load:?} {errs:?}");
    assert_eq!(snap.stamp_attrs.iter().count(), 2);
}

#[test]
fn unknown_and_unstamped_surfaces_are_refused() {
    let (snap, load, errs) = one(
        "acme.stage",
        "stampAttrs:\n  - { name: assetName, type: string, on: [bgg] }\n  - { name: cue, type: \
         string, on: [line] }\n",
    );
    assert!(load.is_empty(), "{load:?}");
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert_eq!(errs[0].code(), "E-PLUGIN-STAMP-SURFACE");
    assert!(
        errs[0].to_string().ends_with(
            "stamp attribute `assetName` (`acme.stage`) names `bgg` in `on:`, which is not \
             `line`, `branch`, `hub`, `choice` or a directive of the resolved profile — did you \
             mean `bg`?"
        ),
        "{}",
        errs[0]
    );
    assert!(!snap.stamp_attrs.contains("assetName"));
    assert!(snap.stamp_attrs.contains("cue"), "the sibling still lands");
    for tag in ["jump", "label", "clear"] {
        let (snap, _, errs) = one(
            "acme.stage",
            &format!("stampAttrs:\n  - {{ name: x, type: string, on: [{tag}] }}\n"),
        );
        assert_eq!(errs.len(), 1, "{errs:?}");
        assert_eq!(errs[0].code(), "E-PLUGIN-STAMP-SURFACE");
        assert!(
            errs[0].to_string().ends_with(&format!(
                "stamp attribute `x` (`acme.stage`) names `{tag}` in `on:`, but `::{tag}` emits \
                 no stamped record (0.49.0 §4.1)"
            )),
            "{}",
            errs[0]
        );
        assert!(snap.stamp_attrs.is_empty());
    }
}

const FX_DIRECTIVE: &str = "directives:\n  - name: fx\n    attrs:\n      - { name: power, \
                            type: int }\n";

#[test]
fn a_surface_of_an_inactive_plugin_names_the_depends_fix() {
    let (_, _, errs) = assemble(
        &[
            Pkg {
                id: "a.one",
                depends: &[],
                stamps: "stampAttrs:\n  - { name: tone, type: string, on: [fx] }\n",
                directives: None,
            },
            Pkg {
                id: "b.fx",
                depends: &[],
                stamps: "stampAttrs: []\n",
                directives: Some(FX_DIRECTIVE),
            },
        ],
        &["a.one"],
    );
    assert_eq!(errs.len(), 1, "{errs:?}");
    assert!(
        errs[0].to_string().ends_with(
            "stamp attribute `tone` (`a.one`) names `fx` in `on:`, which is a directive of \
             `b.fx`, installed but not active in this profile — declare `b.fx` in the \
             `depends:` of `a.one`, so that the directive is in every profile that activates \
             `a.one`"
        ),
        "{}",
        errs[0]
    );
}

/// A plugin may scope a stamp to the directive of a plugin activated after
/// it (a `depends:` is activated after its dependant).
#[test]
fn a_surface_of_a_dependency_resolves() {
    let (snap, load, errs) = assemble(
        &[
            Pkg {
                id: "a.one",
                depends: &["b.fx"],
                stamps: "stampAttrs:\n  - { name: tone, type: string, on: [fx] }\n",
                directives: None,
            },
            Pkg {
                id: "b.fx",
                depends: &[],
                stamps: "stampAttrs: []\n",
                directives: Some(FX_DIRECTIVE),
            },
        ],
        &["a.one", "b.fx"],
    );
    assert!(load.is_empty() && errs.is_empty(), "{load:?} {errs:?}");
    let fx = StampSurface::Directive("fx".to_string());
    assert!(snap.stamp_attrs.get(&fx, "tone").is_some());
}

#[test]
fn a_scoped_stamp_shadowed_by_its_surface_is_refused() {
    for (name, on, owner) in [
        ("prompt", "branch", "`<branch>`"),
        ("text", "choice", "`<choice>`"),
        ("label", "choice", "`<choice>`"),
        ("code", "line", "a content line"),
        ("when", "line", "a content line"),
        ("assetId", "bg", "`::bg`"),
        ("power", "fx", "`::fx`"),
    ] {
        let (snap, load, errs) = assemble(
            &[Pkg {
                id: "acme.stage",
                depends: &[],
                stamps: &format!(
                    "stampAttrs:\n  - {{ name: {name}, type: string, on: [{on}] }}\n  - {{ name: \
                     keep, type: string, on: [{on}] }}\n"
                ),
                directives: Some(FX_DIRECTIVE),
            }],
            &["acme.stage"],
        );
        assert!(load.is_empty(), "{load:?}");
        assert_eq!(errs.len(), 1, "{name} on {on}: {errs:?}");
        assert_eq!(errs[0].code(), "E-PLUGIN-RESERVED-STAMP-ATTR");
        assert_eq!(
            errs[0].to_string(),
            format!(
                "stamp attribute `{name}` (`acme.stage`) is scoped to `{on}`, but `{name}` is an \
                 attribute of {owner} itself, so the stamp could never be read"
            )
        );
        assert!(!snap.stamp_attrs.contains(name));
        assert!(snap.stamp_attrs.contains("keep"));
    }
    // A cross-cutting entry keeps the 0.49 rule: the surface's own
    // declaration wins, and the entry is not refused.
    let (snap, _, errs) = one(
        "acme.stage",
        "stampAttrs:\n  - { name: assetId, type: string }\n",
    );
    assert!(errs.is_empty(), "{errs:?}");
    assert!(snap.stamp_attrs.contains("assetId"));
}

/// 0.50.0 §3.3: a manifest without `on:` hashes exactly as in 0.49 (the
/// 0.49 fold, spelled out), and `on:` moves the hash.
#[test]
fn capability_snapshot_folds_on_only_when_written() {
    let stamps = "stampAttrs:\n  - { name: cue, type: string }\n  - { name: tempo, type: int }\n";
    let (snap, load, errs) = one("arena", stamps);
    assert!(load.is_empty() && errs.is_empty(), "{load:?} {errs:?}");
    // The 0.49 fold over the same declarations, from a snapshot whose
    // `stampAttrs` section is reproduced byte for byte.
    let mut without = snap.clone();
    without.stamp_attrs = lute_manifest::stamps::StampAttrs::default();
    let base = lute_manifest::snapshot::capability_version(&without);
    assert_ne!(base, snap.version);
    let legacy: String = snap
        .stamp_attrs
        .iter()
        .map(|s| format!("{}={:?};", s.decl.name, s.decl))
        .collect();
    assert_eq!(
        legacy,
        "cue=AttrDecl { name: \"cue\", required: false, ty: Str, default: None };\
         tempo=AttrDecl { name: \"tempo\", required: false, ty: Int, default: None };"
    );
    let (scoped, _, errs) = one(
        "arena",
        "stampAttrs:\n  - { name: cue, type: string, on: [line] }\n  - { name: tempo, type: int \
         }\n",
    );
    assert!(errs.is_empty(), "{errs:?}");
    assert_ne!(scoped.version, snap.version, "`on:` is a changed surface");
}
