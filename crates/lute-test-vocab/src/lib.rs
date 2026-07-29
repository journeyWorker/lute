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
pub fn vocab_snapshot() -> CapabilitySnapshot {
    let mut snap = load_core_snapshot();
    for (name, dom) in test_domains() {
        snap.enums.insert(name.clone(), dom.members.clone());
        snap.domains.insert(name, dom);
    }
    snap
}
