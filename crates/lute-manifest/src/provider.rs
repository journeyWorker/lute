//! Provider snapshot loader (plugin §10).
//!
//! Compiler/checker correctness MUST NOT depend on a live catalog: a provider
//! resolves against a pinned snapshot artifact. When offline the LSP keeps the
//! stale snapshot and surfaces `IdStatus::Stale` rather than false unknown-id
//! errors (§10).

use std::collections::BTreeMap;

/// A pinned provider snapshot (plugin §10). `stale` records whether the LSP is
/// serving a snapshot it could not refresh, so absent ids downgrade to
/// `catalog-stale` instead of a hard unknown-id error.
#[derive(Clone, Debug)]
pub struct ProviderSnapshot {
    /// The `capabilityVersion` this snapshot was built for.
    pub manifest_version: String,
    /// The snapshot's own version/id.
    pub provider_version: String,
    /// Resolved ids, keyed by provider name.
    pub entries: BTreeMap<String, Vec<String>>,
    /// True when this snapshot is being served without a successful refresh.
    pub stale: bool,
}

/// The set of provider snapshots a checker/LSP session resolves ids against.
#[derive(Clone, Debug, Default)]
pub struct ProviderSet {
    snaps: Vec<ProviderSnapshot>,
}

/// The resolution status of a provider id against the pinned snapshot set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IdStatus {
    /// The id is present in the provider's entries.
    Fresh,
    /// The id is absent but the owning snapshot is stale (offline) — a
    /// `catalog-stale` diagnostic, not a hard unknown-id error.
    Stale,
    /// The id is absent from a fresh snapshot, or the provider is unknown.
    Absent,
}

impl ProviderSet {
    /// Build a set from a single snapshot.
    pub fn from_one(s: ProviderSnapshot) -> Self {
        Self { snaps: vec![s] }
    }

    /// Resolve `id` against `provider` across the pinned snapshots. The first
    /// snapshot that declares the provider decides the outcome: present → Fresh;
    /// absent → Stale (if that snapshot is stale) or Absent.
    pub fn contains(&self, provider: &str, id: &str) -> IdStatus {
        for s in &self.snaps {
            if let Some(ids) = s.entries.get(provider) {
                if ids.iter().any(|x| x == id) {
                    return IdStatus::Fresh;
                }
                return if s.stale { IdStatus::Stale } else { IdStatus::Absent };
            }
        }
        IdStatus::Absent
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_id_in_fresh_snapshot_is_absent() {
        let ps = ProviderSnapshot {
            manifest_version: "v1".into(),
            provider_version: "1".into(),
            entries: [("character".to_string(), vec!["bianca".to_string()])].into(),
            stale: false,
        };
        let set = ProviderSet::from_one(ps);
        assert_eq!(set.contains("character", "bianca"), IdStatus::Fresh);
        assert_eq!(set.contains("character", "ghost"), IdStatus::Absent);
    }

    #[test]
    fn absent_id_in_stale_snapshot_is_stale() {
        let ps = ProviderSnapshot {
            manifest_version: "v1".into(),
            provider_version: "1".into(),
            entries: [("character".to_string(), vec!["bianca".to_string()])].into(),
            stale: true,
        };
        let set = ProviderSet::from_one(ps);
        assert_eq!(set.contains("character", "ghost"), IdStatus::Stale);
    }
}
