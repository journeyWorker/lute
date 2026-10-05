//! Shared identity metadata for graph/source consumers.
//!
//! Identity is authored, computed from an authored key, or a warning-bearing
//! fallback.  Build-local addresses and source spans deliberately have no
//! representation here: neither can become a durable identity.

use serde::Serialize;

/// Where a canonical identity came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum IdentitySource {
    /// The source construct or declaration supplied the key.
    Authored,
    /// The key is canonicalized from an authored document/project boundary.
    Computed,
    /// A deterministic transition fallback was required; it is not stable.
    Fallback,
}

/// Identity metadata attached to a graph node.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IdentityMetadata {
    /// Normative identity-table row kind (`line`, `componentInstance`, etc.).
    pub kind: String,
    /// Source of the identity, never a byte span or `addr`.
    pub source: IdentitySource,
    /// Canonical/computed key exposed to consumers.
    pub computed: String,
    /// Whether the key is safe for durable joins and matching.
    pub stable: bool,
}

impl IdentityMetadata {
    pub fn authored(kind: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            source: IdentitySource::Authored,
            computed: key.into(),
            stable: true,
        }
    }

    pub fn computed(kind: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            source: IdentitySource::Computed,
            computed: key.into(),
            stable: true,
        }
    }

    pub fn fallback(kind: impl Into<String>, key: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            source: IdentitySource::Fallback,
            computed: key.into(),
            stable: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{IdentityMetadata, IdentitySource};

    #[test]
    fn authored_identity_is_stable_and_not_positional() {
        let identity = IdentityMetadata::authored("componentInstance", "hearthFire#opening");
        assert_eq!(identity.source, IdentitySource::Authored);
        assert!(identity.stable);
        assert_eq!(identity.computed, "hearthFire#opening");
    }

    #[test]
    fn fallback_identity_is_explicitly_unstable() {
        let identity = IdentityMetadata::fallback("line", "hearthFire#1.narrator_0010");
        assert_eq!(identity.source, IdentitySource::Fallback);
        assert!(!identity.stable);
    }
}
