//! Provider snapshot loader (plugin §10).
//!
//! Compiler/checker correctness MUST NOT depend on a live catalog: a provider
//! resolves against a pinned snapshot artifact. When offline the LSP keeps the
//! stale snapshot and surfaces `IdStatus::Stale` rather than false unknown-id
//! errors (§10).

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// A pinned provider snapshot (plugin §10). `stale` records whether the LSP is
/// serving a snapshot it could not refresh, so absent ids downgrade to
/// `catalog-stale` instead of a hard unknown-id error.
///
/// ## On-disk format (Task 5.1)
/// Plugin spec Appendix B — "start flat; canonicalize later". Each snapshot is
/// one flat YAML file (`<dir>/<name>.yaml`) whose fields mirror spec §10 in
/// camelCase; a directory of such files is one [`ProviderSet`]. `generatedAt` /
/// `sourceRefs` from §10 are informative and omitted from the baseline artifact.
///
/// ```yaml
/// manifestVersion: "<capabilityVersion>"   # the snapshot was built for
/// providerVersion: "1"                      # the snapshot's own id
/// stale: false                              # served without a fresh refresh?
/// entries:                                  # provider name -> resolved ids
///   character: [bianca, ren]
/// ```
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderSnapshot {
    /// The `capabilityVersion` this snapshot was built for.
    pub manifest_version: String,
    /// The snapshot's own version/id.
    pub provider_version: String,
    /// Resolved ids, keyed by provider name.
    #[serde(default)]
    pub entries: BTreeMap<String, Vec<String>>,
    /// True when this snapshot is being served without a successful refresh.
    #[serde(default)]
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

    /// Load a set from a directory of flat per-snapshot YAML files (Task 5.1).
    ///
    /// Reads every `*.yaml`/`*.yml` entry in `dir`, in byte-wise filename order
    /// (deterministic — plugin §3.2 forbids file-iteration-order dependence),
    /// and deserializes each into a [`ProviderSnapshot`]. A missing or empty
    /// directory yields an empty set. Unparseable files are skipped rather than
    /// panicking: `check` MUST NOT abort on a corrupt catalog artifact — an id it
    /// cannot find simply resolves to [`IdStatus::Absent`]. Never panics.
    pub fn load(dir: impl AsRef<Path>) -> Self {
        let dir = dir.as_ref();
        let mut paths: Vec<_> = match std::fs::read_dir(dir) {
            Ok(rd) => rd
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| {
                    p.is_file()
                        && matches!(
                            p.extension().and_then(|x| x.to_str()),
                            Some("yaml") | Some("yml")
                        )
                })
                .collect(),
            Err(_) => return Self::default(),
        };
        paths.sort();
        let snaps = paths
            .iter()
            .filter_map(|p| std::fs::read_to_string(p).ok())
            .filter_map(|s| serde_yaml::from_str::<ProviderSnapshot>(&s).ok())
            .collect();
        Self { snaps }
    }

    /// The pinned snapshots this set resolves against, in load order. Consumers
    /// (`catalog refresh`) re-stamp and rewrite these.
    pub fn snapshots(&self) -> &[ProviderSnapshot] {
        &self.snaps
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
                return if s.stale {
                    IdStatus::Stale
                } else {
                    IdStatus::Absent
                };
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

    /// A unique temp dir under the OS temp root, created fresh. Avoids a dev-dep
    /// on `tempfile` — this crate has none and the round-trip test is small.
    fn temp_dir(tag: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("lute-provider-{tag}-{pid}-{n}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn load_missing_dir_is_empty_not_panic() {
        let set = ProviderSet::load("/no/such/lute/catalog/dir");
        assert!(set.snapshots().is_empty());
        assert_eq!(set.contains("character", "bianca"), IdStatus::Absent);
    }

    #[test]
    fn load_empty_dir_is_empty() {
        let dir = temp_dir("empty");
        let set = ProviderSet::load(&dir);
        assert!(set.snapshots().is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn write_then_load_round_trips_status() {
        let dir = temp_dir("roundtrip");
        let snap = ProviderSnapshot {
            manifest_version: "cap-v1".into(),
            provider_version: "7".into(),
            entries: [(
                "character".to_string(),
                vec!["bianca".to_string(), "ren".to_string()],
            )]
            .into(),
            stale: false,
        };
        let yaml = serde_yaml::to_string(&snap).unwrap();
        // camelCase wire form per spec §10.
        assert!(
            yaml.contains("manifestVersion:"),
            "wire form is camelCase: {yaml}"
        );
        std::fs::write(dir.join("core.yaml"), yaml).unwrap();

        let set = ProviderSet::load(&dir);
        assert_eq!(set.snapshots().len(), 1);
        assert_eq!(set.contains("character", "bianca"), IdStatus::Fresh);
        assert_eq!(set.contains("character", "ren"), IdStatus::Fresh);
        assert_eq!(set.contains("character", "ghost"), IdStatus::Absent);
        assert_eq!(set.snapshots()[0].manifest_version, "cap-v1");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn load_preserves_stale_semantics() {
        let dir = temp_dir("stale");
        let snap = ProviderSnapshot {
            manifest_version: "cap-v1".into(),
            provider_version: "1".into(),
            entries: [("character".to_string(), vec!["bianca".to_string()])].into(),
            stale: true,
        };
        std::fs::write(dir.join("core.yaml"), serde_yaml::to_string(&snap).unwrap()).unwrap();
        let set = ProviderSet::load(&dir);
        assert_eq!(set.contains("character", "ghost"), IdStatus::Stale);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn load_skips_unparseable_files() {
        let dir = temp_dir("corrupt");
        std::fs::write(dir.join("junk.yaml"), "this: [is: not: valid").unwrap();
        std::fs::write(dir.join("note.txt"), "ignored non-yaml").unwrap();
        let set = ProviderSet::load(&dir);
        assert!(
            set.snapshots().is_empty(),
            "corrupt yaml skipped, txt ignored"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
