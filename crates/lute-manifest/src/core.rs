//! Built-in `lute.core` capability snapshot (plugin §5, dsl Appendix A).
//!
//! The YAML assets under `assets/lute.core/` are embedded at compile time
//! via `include_str!` so the language's baseline directives, enums and host
//! tokens ship with the binary — no filesystem lookup, no network.
//! `load_core_snapshot` is the deterministic baseline every checker/LSP
//! consumer resolves on top of.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::{Arc, LazyLock};

use crate::schema::{
    DirectivesFile, EnumsFile, HostTokenDecl, HostTokensFile, PluginManifest, CORE_PLUGIN,
};
use crate::snapshot::{capability_version, CapabilitySnapshot, Domain, ResolvedPlugin};

const MANIFEST: &str = include_str!("../assets/lute.core/plugin.yaml");
const STAGING: &str = include_str!("../assets/lute.core/directives/staging.yaml");
const ENUMS: &str = include_str!("../assets/lute.core/enums.yaml");
const HOST_TOKENS: &str = include_str!("../assets/lute.core/hosttokens.yaml");

/// The `lute.core` tag of the walk terminator, `::end` (dsl 0.8.0). The ONE
/// directive declaring `terminatesWalk` ([`crate::validate::SEMANTICS_VOCAB`]),
/// shared as a const because three crates below the compiler dispatch on it and
/// MUST agree: `lute-check`'s `W-CODE-AFTER-END` dead-code pass and its
/// `<track>`-clip guard, `lute-compile`'s `lower_directive` (→ `Command::End`),
/// and `lute-trace`'s walk. Mirrors how `COMPONENT_BEGIN`/`COMPONENT_END` are
/// shared rather than re-spelled per crate.
pub const END_DIRECTIVE: &str = "end";

/// The `lute.core` tag of a position label (dsl 0.37.0 §3.5): `::label{name}`
/// binds a NAMED forward-jump target to whatever record follows it (or,
/// trailing past a section's last record, that section's one-past-end converge
/// position — mirrors an anonymous converge label,
/// `lute-compile::cfg::Emitter::bind`). Emits NO record of its own — dispatch
/// is by TAG (`lute-compile::lower::lower_directive`'s `label` arm returns
/// `None`), the SAME `terminatesWalk`-style tag dispatch [`END_DIRECTIVE`]
/// documents.
pub const LABEL_DIRECTIVE: &str = "label";

/// The attribute of [`LABEL_DIRECTIVE`] that names the label.
pub const LABEL_NAME_ATTR: &str = "name";

/// The `lute.core` tag of a jump (dsl 0.37.0 §3.5): `::jump{to [when] [max]}`,
/// forward, or back to an earlier label when it carries [`JUMP_MAX_ATTR`].
/// Unconditional or guarded via the SAME typed CEL `when` a content-line
/// guard gets (`lute_syntax::ast::Directive::when`). Shared as a const for the
/// same reason [`END_DIRECTIVE`] is: `lute-check`'s reachability/definite-
/// assignment/label passes, `lute-compile`'s `lower_directive`
/// (→ `Command::Jump`) and `normalize::synth_when_next_match`, and
/// `lute-trace`'s walk all dispatch on this tag and MUST agree.
pub const JUMP_DIRECTIVE: &str = "jump";

/// The attribute of [`JUMP_DIRECTIVE`] that bounds a backward jump (dsl
/// 0.51.0 §3.1): `::jump{to max}` may target an earlier `::label` and is
/// taken at most `max` times, a literal integer from 1 to 99. Shared by the
/// checker's label pass and its dead-code rule.
pub const JUMP_MAX_ATTR: &str = "max";

/// The `lute.core` tag of an on-stage character directive (dsl 0.37.0 §3.3):
/// `::actor{character [anchor] [action] [emotion] [costume]}`. Shared by the
/// checker's stage reducer, cast checks, the compiler and the trace walk.
pub const ACTOR_DIRECTIVE: &str = "actor";

/// The `lute.core` tag of a sequence reference (dsl 0.37.0 §3.3):
/// `::sequence{name}` names a member of the project `sequence` domain; `wait`
/// defaults to `true`. A reference, not an inline cinematic.
pub const SEQUENCE_DIRECTIVE: &str = "sequence";

/// The `lute.core` tag of the stage-clearing leaf (dsl 0.24.0 §4): `::clear`
/// takes every character on stage off it. Shared as a const for the same
/// reason [`END_DIRECTIVE`] is: `lute-check`'s stage reducer (the exits and
/// `W-STAGE-ABSENT`), `lute-compile`'s lowering (one `sprite` exit record per
/// character, no record of its own) and `lute-trace`'s walk all dispatch on
/// this tag and MUST agree.
pub const CLEAR_DIRECTIVE: &str = "clear";

/// The core directives with **no stamp carrier** (0.49.0 §4.1): `::jump`
/// lowers to an unstamped `jump` record, `::label` to no record, and
/// `::clear` to no record of its own (its injected exits depend on who is on
/// stage, so an empty stage has none). A plugin `stampAttrs` attribute or a
/// core timing key (`wait`/`duration`/`delay`) on one of these would be
/// dropped, so the checker refuses it (`E-UNKNOWN-ATTR`). Every other core
/// directive lowers to a stamped record.
pub const UNSTAMPED_DIRECTIVES: [&str; 3] = [JUMP_DIRECTIVE, LABEL_DIRECTIVE, CLEAR_DIRECTIVE];

/// The attributes a content line owns (dsl 0.2.2 §7.1, §D7). `lute-compile`'s
/// `lower_line` reads exactly these and consumes this slice directly, so the
/// checker and the compiler cannot drift: a key here is a record field
/// (`wait`: the record's `timing.wait`, 0.49.0 §4.2), a key NOT here may be a
/// plugin stamp, which lowers into the record's stamp instead (plugin §14.1).
/// `duration`/`delay` are not line keys. One of the per-surface lists that
/// plugin assembly (stamp shadowing, 0.50.0 §3.2) and the checker share.
pub const LINE_ATTRS: &[&str] = &[
    "code",
    "emotion",
    "variant",
    "action",
    "dialogMotion",
    "mono",
    "os",
    "vo",
    "as",
    "wait",
];

/// A content line's guard, `when=`: the parser takes it off the attribute
/// list, so it is not in [`LINE_ATTRS`], but the line owns it.
pub const LINE_GUARD_ATTR: &str = "when";

/// A content line's removed attributes: `id=` is `E-RENAMED-ATTR` (dsl
/// 0.37.0 §3.5, write `::label{name}` instead), reported before any stamp
/// lookup, so a stamp of that name could never be read on a line.
pub const LINE_RETIRED_ATTRS: &[&str] = &["id"];

/// `<branch>`'s attributes (dsl 0.10.0 §4, + `prompt`/`timeout`/`default`,
/// dsl 0.11.0).
pub const BRANCH_ATTRS: &[&str] = &["id", "prompt", "timeout", "default"];

/// `<hub>`'s attributes (dsl 0.10.0 §4, + `prompt`, dsl 0.23.0 §4).
pub const HUB_ATTRS: &[&str] = &["id", "prompt"];

/// A `<choice>`'s attributes inside a `<branch>` (dsl 0.10.0 D-L).
pub const BRANCH_CHOICE_ATTRS: &[&str] = &["id", "text", "when", "into", "value"];

/// A `<choice>`'s attributes inside a `<hub>`: the branch set plus the
/// hub-choice flags `once` and `exit` (dsl 0.10.0 D-L).
pub const HUB_CHOICE_ATTRS: &[&str] = &["id", "text", "when", "into", "value", "once", "exit"];

/// A `<choice>`'s renamed or removed attributes, each reported by its own
/// code: `label` (`E-RENAMED-TAG-ATTR`, now `text`), `as`
/// (`E-AS-REMOVED`) and `persist` (`E-PERSIST-REMOVED`). Not permitted, but
/// never reported as unknown, and never a stamp name on `<choice>`.
pub const CHOICE_RETIRED_ATTRS: &[&str] = &["label", "as", "persist"];

/// Failure loading one of the embedded `lute.core` assets.
#[derive(Clone, Debug)]
pub enum CoreLoadError {
    AssetParse {
        asset: &'static str,
        source: Arc<yaml_serde::Error>,
    },
}

impl fmt::Display for CoreLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AssetParse { asset, source } => {
                write!(f, "core {asset} must parse: {source}")
            }
        }
    }
}

impl std::error::Error for CoreLoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::AssetParse { source, .. } => Some(source.as_ref()),
        }
    }
}

fn build_core_snapshot() -> Result<CapabilitySnapshot, CoreLoadError> {
    let manifest: PluginManifest =
        yaml_serde::from_str(MANIFEST).map_err(|e| CoreLoadError::AssetParse {
            asset: "plugin.yaml",
            source: Arc::new(e),
        })?;
    let staging: DirectivesFile =
        yaml_serde::from_str(STAGING).map_err(|e| CoreLoadError::AssetParse {
            asset: "directives/staging.yaml",
            source: Arc::new(e),
        })?;
    let enums: EnumsFile = yaml_serde::from_str(ENUMS).map_err(|e| CoreLoadError::AssetParse {
        asset: "enums.yaml",
        source: Arc::new(e),
    })?;
    let host_tokens: HostTokensFile =
        yaml_serde::from_str(HOST_TOKENS).map_err(|e| CoreLoadError::AssetParse {
            asset: "hosttokens.yaml",
            source: Arc::new(e),
        })?;

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
            option_defaults: crate::assemble::option_defaults(&manifest),
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
        // 0.50.0 §4.1: `userName`. Not folded into the version (only
        // non-core tokens are), so the core stamp is unchanged.
        host_tokens: host_tokens
            .host_tokens
            .into_iter()
            .map(|(name, body)| {
                let decl = HostTokenDecl {
                    ty: body.ty,
                    plugin: CORE_PLUGIN.to_string(),
                };
                (name, decl)
            })
            .collect(),
        ..Default::default()
    };
    snap.directive_owners = snap
        .directives
        .keys()
        .map(|name| (name.clone(), "lute.core".to_string()))
        .collect();
    snap.version = capability_version(&snap);
    Ok(snap)
}
static CORE: LazyLock<Result<CapabilitySnapshot, CoreLoadError>> =
    LazyLock::new(build_core_snapshot);

/// The embedded `lute.core` snapshot, borrowed for the program's lifetime
/// (for a holder that borrows a snapshot, such as a project document set).
///
/// The assets are part of this crate and are validated by the manifest tests;
/// a failure here indicates a broken binary rather than author input.
pub fn core_snapshot() -> &'static CapabilitySnapshot {
    match &*CORE {
        Ok(snapshot) => snapshot,
        Err(error) => panic!("{error}"),
    }
}

/// Load the embedded `lute.core` snapshot (an owned copy of
/// [`core_snapshot`]).
pub fn load_core_snapshot() -> CapabilitySnapshot {
    core_snapshot().clone()
}

/// Fallible form of [`load_core_snapshot`] for callers that need controlled
/// initialization errors.
pub fn try_load_core_snapshot() -> Result<CapabilitySnapshot, CoreLoadError> {
    CORE.clone()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Type;

    /// The `lute.core` baseline is CLOSED (dsl Appendix A + the 0.8.0
    /// terminator + the jump/label pair + the 0.24.0 `::clear` + the 0.37.0
    /// `::sequence`): exactly these thirteen directives, no more. Asserting
    /// the exact set — not just presence — is what makes an accidental
    /// addition/removal in `staging.yaml` a test failure rather than a silent
    /// vocabulary change every downstream `E-UNKNOWN-DIRECTIVE` decision
    /// depends on.
    #[test]
    fn core_snapshot_has_baseline_directives() {
        let snap = load_core_snapshot();
        let names: Vec<&str> = snap.directives.keys().map(String::as_str).collect();
        assert_eq!(
            names,
            [
                ACTOR_DIRECTIVE,
                "bg",
                "camera",
                "cg",
                CLEAR_DIRECTIVE,
                END_DIRECTIVE,
                JUMP_DIRECTIVE,
                LABEL_DIRECTIVE,
                "music",
                SEQUENCE_DIRECTIVE,
                "sfx",
                "vfx",
                "video",
            ],
            "the lute.core baseline is exactly 13 directives"
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
        let [reason] = &*end.attrs else {
            panic!("::end declares exactly one attr")
        };
        assert_eq!(reason.name, "reason");
        assert!(!reason.required);
        assert_eq!(reason.ty, crate::types::Type::Str);
    }

    /// dsl 0.37.0 §3.3: camera values are domain members; the numeric
    /// transform fields are gone.
    #[test]
    fn camera_has_domain_slots_and_timing_attrs() {
        let snap = load_core_snapshot();
        let cam = snap.directive("camera").unwrap();
        let names: Vec<_> = cam.attrs.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "focus",
                "framing",
                "move",
                "transition",
                "duration",
                "delay",
                "wait"
            ]
        );
    }

    /// dsl 0.37.0 §3.2: `musicPlayback` (formerly `musicPlayback`) is a SLOT —
    /// `::music{playback}` names it, so a project or plugin declaring its
    /// members gets them checked — but the core itself ships none.
    #[test]
    fn music_playback_is_a_slot_with_no_members() {
        let snap = load_core_snapshot();
        let playback = snap
            .directive("music")
            .and_then(|d| d.attrs.iter().find(|a| a.name == "playback"))
            .expect("missing music.playback");
        assert_eq!(playback.ty, Type::Domain("musicPlayback".into()));
        assert_eq!(snap.enums.get("musicPlayback"), None);
        assert!(!snap.domains.contains_key("musicPlayback"));
    }

    /// dsl 0.9.0 D-A / 0.37.0 §3.2: the core's vocabulary surface is exactly
    /// this set of domain NAMES, referenced from attribute types and populated
    /// by nobody. (The content-line `emotion`/`action` and `textStyle` are
    /// named by `lute-check`'s content-line pass rather than by a directive
    /// attr only.) Pinning the exact set makes an accidental new slot — or a
    /// silently dropped one — a failure here rather than an
    /// `E-DOMAIN-UNKNOWN` in an author's file.
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
        assert_eq!(
            slots,
            [
                "action",
                "anchor",
                "cameraMove",
                "cgLayout",
                "costume",
                "emotion",
                "framing",
                "mood",
                "musicPlayback",
                "sequence",
                "transition",
                "vfxType",
                "volume"
            ]
        );
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
        assert!(
            snap.domains.is_empty(),
            "core domains: {:?}",
            snap.domains.keys()
        );
    }

    /// dsl 0.9.0 D-A: the two attrs that were free strings become checkable.
    #[test]
    fn slot_attrs_are_domain_typed() {
        let snap = load_core_snapshot();
        let ty = |dir: &str, attr: &str| {
            snap.directive(dir)
                .and_then(|d| d.attrs.iter().find(|a| a.name == attr))
                .map_or_else(|| panic!("missing {dir}.{attr}"), |a| a.ty.clone())
        };
        assert_eq!(ty(ACTOR_DIRECTIVE, "action"), Type::Domain("action".into()));
        assert_eq!(ty(ACTOR_DIRECTIVE, "anchor"), Type::Domain("anchor".into()));
        assert_eq!(
            ty(ACTOR_DIRECTIVE, "emotion"),
            Type::Domain("emotion".into())
        );
        assert_eq!(
            ty(ACTOR_DIRECTIVE, "costume"),
            Type::Domain("costume".into())
        );
        assert_eq!(ty("camera", "move"), Type::Domain("cameraMove".into()));
        assert_eq!(ty("cg", "layout"), Type::Domain("cgLayout".into()));
        assert_eq!(
            ty(SEQUENCE_DIRECTIVE, "name"),
            Type::Domain("sequence".into())
        );
        assert_eq!(ty("music", "mood"), Type::Domain("mood".into()));
        assert_eq!(ty("vfx", "type"), Type::Domain("vfxType".into()));
    }

    /// 0.50.0 §4.1: the core declares exactly `userName`, a string, owned by
    /// `lute.core`, and its name is an identifier like every host token.
    #[test]
    fn core_declares_the_user_name_host_token() {
        let snap = load_core_snapshot();
        let tokens: Vec<(&str, &HostTokenDecl)> = snap
            .host_tokens
            .iter()
            .map(|(n, t)| (n.as_str(), t))
            .collect();
        assert_eq!(
            tokens,
            [(
                "userName",
                &HostTokenDecl {
                    ty: crate::schema::HostTokenType::Str,
                    plugin: CORE_PLUGIN.to_string(),
                }
            )]
        );
        assert!(snap.host_tokens.keys().all(|n| crate::ident::is_ident(n)));
    }

    /// 0.50.0 §4.1: core host tokens are not folded into
    /// `capabilitySnapshot`, so the core stamp is the one it was before
    /// `userName` was declared.
    #[test]
    fn core_host_tokens_leave_the_core_stamp_unchanged() {
        let mut snap = load_core_snapshot();
        let stamped = snap.version.clone();
        snap.host_tokens.clear();
        assert_eq!(capability_version(&snap), stamped);
    }
}
