//! Built-in `lute.core` capability snapshot (plugin §5, dsl Appendix A).
//!
//! The three YAML assets under `assets/lute.core/` are embedded at compile time
//! via `include_str!` so the language's baseline directives and enums ship with
//! the binary — no filesystem lookup, no network. `load_core_snapshot` is the
//! deterministic baseline every checker/LSP consumer resolves on top of.

use std::collections::BTreeMap;

use crate::schema::{DirectivesFile, EnumsFile, PluginManifest};
use crate::snapshot::{capability_version, CapabilitySnapshot, Domain, ResolvedPlugin};

const MANIFEST: &str = include_str!("../assets/lute.core/plugin.yaml");
const STAGING: &str = include_str!("../assets/lute.core/directives/staging.yaml");
const ENUMS: &str = include_str!("../assets/lute.core/enums.yaml");

/// Build the built-in `lute.core` capability snapshot: all dsl Appendix A
/// baseline directives (bg/music/sfx/auto/vfx/cut/video/camera) plus the core
/// enums, stamped with a deterministic `capabilityVersion` (plugin §13).
pub fn load_core_snapshot() -> CapabilitySnapshot {
    let manifest: PluginManifest =
        serde_yaml::from_str(MANIFEST).expect("core plugin.yaml must parse");
    let staging: DirectivesFile =
        serde_yaml::from_str(STAGING).expect("core staging.yaml must parse");
    let enums: EnumsFile = serde_yaml::from_str(ENUMS).expect("core enums.yaml must parse");

    let mut directives = BTreeMap::new();
    for d in staging.directives {
        directives.insert(d.name.clone(), d);
    }

    let mut plugins = BTreeMap::new();
    plugins.insert(
        manifest.id.clone(),
        ResolvedPlugin {
            version: manifest.version.clone(),
            options: BTreeMap::new(),
        },
    );

    // Seed `domains` from the same core enum map that seeds `enums` (mirrors
    // the plugin-loop fold in `assemble.rs`, which does the identical
    // `name -> Domain { members }` mapping for each active plugin's `enums`
    // export): built here, at the SAME seed site as `enums: enums.enums`
    // below, so the two stay in sync by construction rather than via a
    // separate mechanism. Without this, `lute.core`'s baseline enums
    // (emotion/mood/volume/anchor/vfxType/musicAction) would land in
    // `snap.enums` but never in `snap.domains`, leaving `domains` an
    // incomplete view of the merged vocabulary.
    let domains: BTreeMap<String, Domain> = enums
        .enums
        .iter()
        .map(|(k, v)| (k.clone(), Domain { members: v.clone() }))
        .collect();

    let mut snap = CapabilitySnapshot {
        plugins,
        directives,
        domains,
        enums: enums.enums,
        ..Default::default()
    };
    snap.version = capability_version(&snap);
    snap
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_snapshot_has_baseline_directives() {
        let snap = load_core_snapshot();
        for name in [
            "bg", "music", "sfx", "auto", "vfx", "cut", "video", "camera",
        ] {
            assert!(snap.directive(name).is_some(), "missing ::{name}");
        }
    }

    #[test]
    fn camera_has_timing_attrs() {
        let snap = load_core_snapshot();
        let cam = snap.directive("camera").unwrap();
        let names: Vec<_> = cam.attrs.iter().map(|a| a.name.as_str()).collect();
        for k in ["focus", "zoom", "duration", "wait"] {
            assert!(names.contains(&k), "camera missing {k}");
        }
    }

    #[test]
    fn music_action_enum_matches_spec() {
        let snap = load_core_snapshot();
        let e = snap.enums.get("musicAction").unwrap();
        assert!(e.contains(&"fade-out".to_string()));
    }

    #[test]
    fn core_baseline_enums_are_domains() {
        let snap = load_core_snapshot();
        for name in ["emotion", "mood", "volume", "anchor", "vfxType", "musicAction"] {
            assert!(
                snap.domains.contains_key(name),
                "missing core domain {name}: {:?}",
                snap.domains.keys().collect::<Vec<_>>()
            );
        }
        assert_eq!(snap.domains["emotion"].members, snap.enums["emotion"]);
    }
}
