//! `lute.project.yaml` loader + the single shared document resolver (plugin §11).
//!
//! This module is the one place both the CLI and the LSP resolve a scene's
//! capability surface, so they build byte-identical snapshots — the
//! no-divergence linchpin (plugin §11). `load_project` reads a project's
//! `profiles` graph + `defaultProfile` + optional `pluginsDir` into a
//! [`ProfileGraph`] plus a resolved plugins directory; `resolve_document_snapshot`
//! composes the already-built pieces (`load_plugins_dir` → `resolve_activation`
//! → `assemble_snapshot`) into a deterministic snapshot, folding every
//! `LoadError`/`ResolveError`/`AssembleError` into a [`ResolveDiag`]. It never
//! panics.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::core::load_core_snapshot;
use crate::loader::load_plugins_dir;
use crate::resolve::{resolve_activation, ActivationMap, Profile, ProfileGraph};
use crate::snapshot::CapabilitySnapshot;
use crate::types::Literal;

/// A loaded `lute.project.yaml`: the resolved profile graph plus the absolute
/// plugins directory the registry loads from.
#[derive(Clone, Debug)]
pub struct ProjectConfig {
    pub graph: ProfileGraph,
    /// Resolved plugins dir (`project_dir.join(pluginsDir)`; defaults to
    /// `project_dir/plugins/`).
    pub plugins_dir: PathBuf,
}

/// A resolution diagnostic surfaced to the caller (folded into the check
/// result). For now the message is the `Debug` form of the underlying
/// `LoadError`/`ResolveError`/`AssembleError`; a structured diagnostic mapping is
/// a later refinement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolveDiag {
    pub message: String,
}

/// Raw `lute.project.yaml` shape (plugin §11). `profiles` is a map of name →
/// `{ extends?, plugins: map<id, true|options-map> }`.
#[derive(Debug, Deserialize)]
struct RawProject {
    #[serde(rename = "pluginsDir")]
    plugins_dir: Option<String>,
    #[serde(rename = "defaultProfile")]
    default_profile: String,
    #[serde(default)]
    profiles: BTreeMap<String, RawProfile>,
}

#[derive(Debug, Deserialize)]
struct RawProfile {
    #[serde(default)]
    extends: Option<String>,
    /// Each entry activates a plugin: `true` (presence-only) or a mapping of
    /// option values. Kept as raw YAML so `true` and a map coexist under one key.
    #[serde(default)]
    plugins: BTreeMap<String, serde_yaml::Value>,
}

/// Normalize a single `profiles[..].plugins` entry value into an option map:
/// `true` (or any non-mapping scalar) → empty map (plugin §11: presence
/// activates); a mapping → `Literal::from_yaml` per value.
fn plugin_options(value: &serde_yaml::Value) -> BTreeMap<String, Literal> {
    match Literal::from_yaml(value) {
        Some(Literal::Map(m)) => m,
        _ => BTreeMap::new(),
    }
}

/// Read `<project_dir>/lute.project.yaml` into a [`ProjectConfig`].
///
/// Distinguishes an absent config from a broken one (plugin §11): a missing
/// file → `Ok(None)` (the document legitimately resolves core-only); a read
/// error other than not-found or a YAML parse/deserialize error → `Err(msg)`
/// so the caller can surface it instead of silently mis-validating; a valid
/// file → `Ok(Some(cfg))`.
pub fn load_project(project_dir: &Path) -> Result<Option<ProjectConfig>, String> {
    let path = project_dir.join("lute.project.yaml");
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
    };
    let raw: RawProject =
        serde_yaml::from_str(&text).map_err(|e| format!("invalid {}: {e}", path.display()))?;

    let mut profiles = BTreeMap::new();
    for (name, rp) in raw.profiles {
        let plugins: ActivationMap = rp
            .plugins
            .iter()
            .map(|(id, value)| (id.clone(), plugin_options(value)))
            .collect();
        profiles.insert(
            name,
            Profile {
                extends: rp.extends,
                plugins,
            },
        );
    }

    let graph = ProfileGraph {
        profiles,
        default_profile: raw.default_profile,
    };
    let plugins_dir = project_dir.join(raw.plugins_dir.as_deref().unwrap_or("plugins/"));

    Ok(Some(ProjectConfig { graph, plugins_dir }))
}

/// The ONE resolution both CLI and LSP call (plugin §11). Given a project (or
/// `None` for core-only) and the scene's parsed frontmatter (profile + plugins),
/// resolve activation and assemble the snapshot deterministically. Returns the
/// snapshot plus any resolution diagnostics (load errors / unresolved depends /
/// cycles / assembly dup ids). Never panics.
pub fn resolve_document_snapshot(
    project: Option<&ProjectConfig>,
    scene_profile: Option<&str>,
    scene_plugins: &BTreeMap<String, serde_yaml::Value>,
) -> (CapabilitySnapshot, Vec<ResolveDiag>) {
    let Some(project) = project else {
        return (load_core_snapshot(), Vec::new());
    };

    let mut diags = Vec::new();

    // 1. Load every installed plugin package; surface load errors.
    let (registry, load_errs) = load_plugins_dir(&project.plugins_dir);
    diags.extend(load_errs.into_iter().map(|e| ResolveDiag {
        message: format!("{e:?}"),
    }));

    // 2. Pick the profile: scene override, else the graph's default.
    let selected = scene_profile.unwrap_or(project.graph.default_profile.as_str());

    // 3. Convert scene-local `plugins:` frontmatter to an ActivationMap.
    let scene_local: ActivationMap = scene_plugins
        .iter()
        .map(|(id, value)| (id.clone(), plugin_options(value)))
        .collect();

    // 4. Resolve activation (§11.1 order + §11.2 merge).
    let active = match resolve_activation(&project.graph, selected, &scene_local, &registry) {
        Ok(active) => active,
        Err(e) => {
            diags.push(ResolveDiag {
                message: format!("{e:?}"),
            });
            // No conforming activation → fall back to the core-only baseline so
            // the caller still gets a usable snapshot.
            return (load_core_snapshot(), diags);
        }
    };

    // 5. Assemble the merged snapshot; surface assembly errors.
    let (snapshot, assemble_errs) = crate::assemble::assemble_snapshot(&active, &registry);
    diags.extend(assemble_errs.into_iter().map(|e| ResolveDiag {
        message: format!("{e:?}"),
    }));

    (snapshot, diags)
}
