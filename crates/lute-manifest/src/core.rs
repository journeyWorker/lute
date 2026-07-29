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

/// The `lute.core` tag of the walk terminator, `::end` (dsl 0.8.0). The ONE
/// directive declaring `terminatesWalk` ([`crate::validate::SEMANTICS_VOCAB`]),
/// shared as a const because three crates below the compiler dispatch on it and
/// MUST agree: `lute-check`'s `W-CODE-AFTER-END` dead-code pass and its
/// `<track>`-clip guard, `lute-compile`'s `lower_directive` (→ `Command::End`),
/// and `lute-trace`'s walk. Mirrors how `COMPONENT_BEGIN`/`COMPONENT_END` are
/// shared rather than re-spelled per crate.
pub const END_DIRECTIVE: &str = "end";

/// Build the built-in `lute.core` capability snapshot: all dsl Appendix A
/// baseline directives (bg/music/sfx/auto/vfx/cut/video/camera) plus the 0.8.0
/// walk terminator `::end`, plus the core enums, stamped with a deterministic
/// `capabilityVersion` (plugin §13).
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
        .map(|(k, v)| (k.clone(), v.clone().into_domain()))
        .collect();

    let mut snap = CapabilitySnapshot {
        plugins,
        directives,
        domains,
        enums: enums
            .enums
            .iter()
            .map(|(k, v)| (k.clone(), v.clone().into_domain().members))
            .collect(),
        ..Default::default()
    };
    snap.version = capability_version(&snap);
    snap
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `lute.core` baseline is CLOSED (dsl Appendix A + the 0.8.0
    /// terminator): exactly these nine directives, no more. Asserting the
    /// exact set — not just presence — is what makes an accidental
    /// addition/removal in `staging.yaml` a test failure rather than a
    /// silent vocabulary change every downstream `E-UNKNOWN-DIRECTIVE`
    /// decision depends on.
    #[test]
    fn core_snapshot_has_baseline_directives() {
        let snap = load_core_snapshot();
        let names: Vec<&str> = snap.directives.keys().map(String::as_str).collect();
        assert_eq!(
            names,
            ["auto", "bg", "camera", "cut", END_DIRECTIVE, "music", "sfx", "vfx", "video"],
            "the lute.core baseline is exactly 9 directives"
        );
    }

    /// dsl 0.8.0: `::end` is the walk terminator — an optional `reason`
    /// string and the `terminatesWalk` flag, nothing else.
    #[test]
    fn end_declares_optional_reason_and_terminates_walk() {
        let snap = load_core_snapshot();
        let end = snap.directive(END_DIRECTIVE).expect("missing ::end");
        assert_eq!(end.semantics, ["terminatesWalk"]);
        assert!(crate::validate::validate_directive(end).is_empty());
        let [reason] = &end.attrs[..] else { panic!("::end declares exactly one attr") };
        assert_eq!(reason.name, "reason");
        assert!(!reason.required);
        assert_eq!(reason.ty, crate::types::Type::Str);
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
