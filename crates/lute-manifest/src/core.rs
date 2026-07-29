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
/// walk terminator `::end`, stamped with a deterministic `capabilityVersion`
/// (plugin §13).
///
/// dsl 0.9.0 D-A: it carries NO vocabulary members. `assets/lute.core/
/// enums.yaml` is empty, so both `enums` and `domains` come out empty; the
/// seven domain names survive only as attribute types in `staging.yaml`, and
/// every member comes from a project schema or a plugin.
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

    // Seed `domains` from the same core enum map that seeds `enums` below
    // (mirrors the plugin-loop fold in `assemble.rs`, which does the identical
    // `name -> Domain { members }` mapping for each active plugin's `enums`
    // export). `enums.yaml` is empty as of dsl 0.9.0 D-A, so both maps come
    // out empty — the fold stays because it is what keeps them in sync BY
    // CONSTRUCTION: were the asset ever to regrow a member, it would land in
    // both views at once instead of leaving `domains` a silently incomplete
    // one.
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
    use crate::types::Type;

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

    /// dsl 0.9.0 D-A: `musicAction` used to be a six-member core enum. It
    /// survives as a SLOT — `::music{action}` still names it, so a project or
    /// plugin declaring `musicAction` members gets them checked — but the core
    /// itself ships none. Asserting the absence, rather than dropping the
    /// test, is what keeps `fade-out` and friends from creeping back in.
    #[test]
    fn music_action_is_a_slot_with_no_members() {
        let snap = load_core_snapshot();
        let action = snap
            .directive("music")
            .and_then(|d| d.attrs.iter().find(|a| a.name == "action"))
            .expect("missing music.action");
        assert_eq!(action.ty, Type::Domain("musicAction".into()));
        assert_eq!(snap.enums.get("musicAction"), None);
        assert!(snap.domains.get("musicAction").is_none());
    }

    /// dsl 0.9.0 D-A: the core's vocabulary surface is exactly this set of
    /// domain NAMES, referenced from attribute types and populated by nobody.
    /// (`emotion` and the content-line `action` are the two further slots, but
    /// they are named by `lute-check`'s content-line pass rather than by a
    /// directive attr, so they are out of this snapshot's reach.) Pinning the
    /// exact set makes an accidental new slot — or a silently dropped one —
    /// a failure here rather than an `E-DOMAIN-UNKNOWN` in an author's file.
    #[test]
    fn core_domain_slots_are_declared_as_attr_types() {
        let snap = load_core_snapshot();
        let mut slots: Vec<&str> = snap
            .directives
            .values()
            .flat_map(|d| &d.attrs)
            .filter_map(|a| match &a.ty {
                Type::Domain(n) => Some(n.as_str()),
                _ => None,
            })
            .collect();
        slots.sort_unstable();
        slots.dedup();
        assert_eq!(slots, ["action", "anchor", "mood", "musicAction", "vfxType", "volume"]);
        for slot in slots {
            assert!(
                !snap.domains.contains_key(slot),
                "the core must ship no members for slot {slot}"
            );
        }
    }

    /// dsl 0.9.0 D-A: the core declares SLOTS, never MEMBERS. A concrete
    /// vocabulary in the binary is a category error for a general authoring
    /// tool — this test is the guard that keeps one from creeping back.
    #[test]
    fn core_ships_no_vocabulary_members() {
        let snap = load_core_snapshot();
        assert!(snap.enums.is_empty(), "core enums: {:?}", snap.enums);
        assert!(snap.domains.is_empty(), "core domains: {:?}", snap.domains.keys());
    }

    /// dsl 0.9.0 D-A: the two attrs that were free strings become checkable.
    #[test]
    fn slot_attrs_are_domain_typed() {
        let snap = load_core_snapshot();
        let ty = |dir: &str, attr: &str| {
            snap.directive(dir)
                .and_then(|d| d.attrs.iter().find(|a| a.name == attr))
                .map(|a| a.ty.clone())
                .unwrap_or_else(|| panic!("missing {dir}.{attr}"))
        };
        assert_eq!(ty("auto", "action"), Type::Domain("action".into()));
        assert_eq!(ty("auto", "anchor"), Type::Domain("anchor".into()));
        assert_eq!(ty("music", "mood"), Type::Domain("mood".into()));
        assert_eq!(ty("vfx", "type"), Type::Domain("vfxType".into()));
    }
}
