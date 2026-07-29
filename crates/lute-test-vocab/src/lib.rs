//! Shared test vocabulary (dsl 0.9.0 D-A/D-F).
//!
//! From 0.9.0 the core ships NO domain members, so a fixture that writes
//! `emotion="delighted"` must declare that vocabulary exactly as a real
//! project does. This crate is that declaration, shared by `lute-check`'s and
//! `lute-compile`'s test suites as a dev-dependency: ONE definition, so a
//! fixture can never silently depend on a member the core used to provide and
//! the two suites can never drift apart.
//!
//! `publish = false`; nothing outside `#[cfg(test)]` code should depend on it.

use std::collections::BTreeMap;

use lute_manifest::core::load_core_snapshot;
use lute_manifest::snapshot::{CapabilitySnapshot, Domain};

pub fn closed(members: &[&str]) -> Domain {
    Domain {
        members: members.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }
}

/// Every domain the test fixtures reference, with the member semantics dsl
/// 0.9.0 D-D requires for `action` and `anchor`.
pub fn test_domains() -> BTreeMap<String, Domain> {
    let mut d = BTreeMap::new();
    d.insert(
        "emotion".to_string(),
        closed(&[
            "neutral",
            "surprised",
            "delighted",
            "shy",
            "content",
            "angry",
            "sad",
        ]),
    );
    d.insert(
        "mood".to_string(),
        closed(&["peaceful", "tense", "romantic", "sad", "upbeat"]),
    );
    d.insert(
        "volume".to_string(),
        closed(&["silent", "down", "normal", "up", "full"]),
    );
    d.insert(
        "vfxType".to_string(),
        closed(&[
            "whiteOut", "blackOut", "rain", "snow", "leaves", "petals", "raindrop",
        ]),
    );
    d.insert(
        "musicAction".to_string(),
        closed(&["start", "change", "stop", "resume", "fade-out"]),
    );
    d.insert(
        "anchor".to_string(),
        Domain {
            members: vec!["left".into(), "center".into(), "right".into()],
            open: false,
            default: Some("center".into()),
            exits: Vec::new(),
        },
    );
    d.insert(
        "action".to_string(),
        Domain {
            members: [
                "fade-in-up",
                "fade-in-slow",
                "slide-in-left",
                "walk-in",
                "idle",
                "wave",
                "sway",
                "lean",
                "pose-turn",
                "pose-lean",
                "fade-out",
                "fade-out-down",
                "fade-out-slow",
                "hide",
            ]
            .iter()
            .map(|s| s.to_string())
            .collect(),
            open: false,
            default: None,
            exits: ["fade-out", "fade-out-down", "fade-out-slow", "hide"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        },
    );
    d
}

/// The core snapshot with [`test_domains`] folded in — the drop-in replacement
/// for `load_core_snapshot()` in any fixture that uses vocabulary attrs.
///
/// `load_core_snapshot()` stamps `snap.version` before returning, so folding
/// the vocabulary in leaves that stamp describing a snapshot we no longer have
/// (`snap.enums` gains `action`, and `enums` is hashed). Re-stamp so every
/// fixture using this helper emits the `capabilityVersion` of the snapshot it
/// actually compiled against.
pub fn vocab_snapshot() -> CapabilitySnapshot {
    let mut snap = load_core_snapshot();
    for (name, dom) in test_domains() {
        snap.enums.insert(name.clone(), dom.members.clone());
        snap.domains.insert(name, dom);
    }
    snap.version = lute_manifest::snapshot::capability_version(&snap);
    snap
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The helper adds an `action` key to `snap.enums`, and `enums` is folded
    /// into the content stamp (`capability_version`), so a snapshot carrying
    /// the test vocabulary MUST NOT present the bare core's `version`.
    /// `load_core_snapshot()` stamps before we fold the domains in, so the
    /// stamp has to be recomputed after the fold or every compile path that
    /// switched to this helper would emit a stale `capabilityVersion`.
    #[test]
    fn vocab_snapshot_restamps_its_content_version() {
        let core = load_core_snapshot();
        let vocab = vocab_snapshot();
        assert_ne!(
            vocab.version, core.version,
            "vocab_snapshot() must re-stamp: its enums differ from the bare core's"
        );
        assert_eq!(
            vocab.version,
            lute_manifest::snapshot::capability_version(&vocab),
            "the stamp must be the hash of the snapshot actually returned"
        );
    }
}
