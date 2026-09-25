//! Per-run memo of the inputs every document of a project shares.
//!
//! [`crate::build_input`] resolves, per document, the governing
//! `lute.project.yaml`, its provider catalog, the activated capability
//! snapshot (plugin load + assembly + `capabilityVersion` hash), and the
//! `uses:`/`components:` import DAGs. Across a project those are the SAME
//! handful of values recomputed once per scene, which made every
//! project-wide command quadratic-ish in practice. One [`InputCache`] lives
//! for one CLI invocation (no on-disk state, no invalidation) and computes
//! each distinct value once; every consumer still receives its own owned copy,
//! so results are identical to the uncached path.
//!
//! Keys:
//! - projects: the project root directory handed to `build_input`;
//! - providers: the explicit `--providers` directory, else the project root;
//! - snapshots: `(project root, scene profile, scene plugins)` — exactly the
//!   arguments `resolve_document_snapshot` reads;
//! - imports/components: `(importing directory, uses, extends)` /
//!   `(importing directory, components)`, see [`lute_check::ImportCache`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lute_check::{ImportCache, Memo};
use lute_manifest::project::{
    load_project, project_providers, resolve_document_snapshot, ProjectConfig, ResolveDiag,
};
use lute_manifest::provider::ProviderSet;
use lute_manifest::snapshot::CapabilitySnapshot;

/// A `load_project` outcome, shared.
pub(crate) type LoadedProject = Arc<Result<Option<ProjectConfig>, String>>;

/// A `resolve_document_snapshot` outcome, shared.
pub(crate) type ResolvedSnapshot = Arc<(CapabilitySnapshot, Vec<ResolveDiag>)>;

type SnapshotKey = (
    Option<PathBuf>,
    Option<String>,
    BTreeMap<String, serde_yaml::Value>,
);

#[derive(Clone, PartialEq, Eq, Hash)]
enum ProvidersKey {
    Explicit(PathBuf),
    Project(Option<PathBuf>),
}

#[derive(Default)]
pub(crate) struct InputCache {
    projects: Memo<PathBuf, LoadedProject>,
    providers: Memo<ProvidersKey, Arc<ProviderSet>>,
    snapshots: Memo<SnapshotKey, ResolvedSnapshot>,
    pub imports: ImportCache,
}

impl InputCache {
    /// `load_project(dir)`, once per directory.
    pub fn project(&self, dir: &Path) -> LoadedProject {
        self.projects
            .get_or_init(dir.to_path_buf(), || Arc::new(load_project(dir)))
    }

    /// The provider catalog precedence `build_input` applies (plugin §10):
    /// an explicit `--providers <dir>` wins, otherwise the loaded project's
    /// pinned catalog (`root` is the directory `project` was loaded from).
    pub fn providers(
        &self,
        explicit: Option<&Path>,
        root: Option<&Path>,
        project: Option<&ProjectConfig>,
    ) -> Arc<ProviderSet> {
        match explicit {
            Some(dir) => self
                .providers
                .get_or_init(ProvidersKey::Explicit(dir.to_path_buf()), || {
                    Arc::new(ProviderSet::load(dir))
                }),
            None => self
                .providers
                .get_or_init(ProvidersKey::Project(root.map(Path::to_path_buf)), || {
                    Arc::new(project_providers(project))
                }),
        }
    }

    /// `resolve_document_snapshot(project, profile, plugins)`, once per
    /// `(root, profile, plugins)` (`root` as for [`Self::providers`]).
    pub fn snapshot(
        &self,
        root: Option<&Path>,
        project: Option<&ProjectConfig>,
        profile: Option<&str>,
        plugins: &BTreeMap<String, serde_yaml::Value>,
    ) -> ResolvedSnapshot {
        let key = (
            root.map(Path::to_path_buf),
            profile.map(str::to_string),
            plugins.clone(),
        );
        self.snapshots.get_or_init(key, || {
            Arc::new(resolve_document_snapshot(project, profile, plugins))
        })
    }
}
