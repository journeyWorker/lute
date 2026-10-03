//! Project manifest context for consumers that need the resolved capability surface.

use std::collections::BTreeMap;
use std::path::Path;

use lute_manifest::project::{load_project, project_providers, resolve_document_snapshot, ProjectConfig};
use lute_manifest::provider::ProviderSet;
use lute_manifest::snapshot::CapabilitySnapshot;

/// Manifest-derived inputs shared by CLI and other adapters.
#[derive(Clone, Debug)]
pub struct ManifestContext {
    pub project: Option<ProjectConfig>,
    pub snapshot: CapabilitySnapshot,
    pub providers: ProviderSet,
}

/// Load one project manifest and resolve its default capability surface.
///
/// Keeping this operation in the model prevents adapters from each owning a
/// subtly different manifest read/provider/snapshot assembly path.
pub fn manifest_context(root: &Path) -> Result<ManifestContext, String> {
    let project = load_project(root)?;
    let (snapshot, _) = resolve_document_snapshot(project.as_ref(), None, &BTreeMap::new());
    let providers = project_providers(project.as_ref());
    Ok(ManifestContext { project, snapshot, providers })
}
/// Resolve a capability snapshot from an already-loaded project config.
pub fn resolve_snapshot(project: Option<&ProjectConfig>) -> CapabilitySnapshot {
    resolve_document_snapshot(project, None, &BTreeMap::new()).0
}
