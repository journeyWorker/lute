//! Per-invocation memoization shared by input assembly.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use lute_check::{ImportCache, Memo, ProjectBase};
use lute_manifest::project::{
    load_project, project_providers, resolve_document_snapshot, resolve_permissions, ProjectConfig,
    ProjectLoadError, ResolveDiag,
};
use lute_manifest::provider::ProviderSet;
use lute_manifest::snapshot::CapabilitySnapshot;

pub type LoadedProject = Result<Option<ProjectConfig>, ProjectLoadError>;
pub type ResolvedSnapshot = Arc<(CapabilitySnapshot, Vec<ResolveDiag>)>;

type SnapshotKey = (
    Option<PathBuf>,
    Option<String>,
    BTreeMap<String, yaml_serde::Value>,
);

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum ProvidersKey {
    Explicit(PathBuf),
    Project(Option<PathBuf>),
}

/// What a [`ProjectBase`] depends on: the root, the document's profile and
/// plugin overrides, the permission profile and the providers directory.
type BaseKey = (SnapshotKey, Option<String>, ProvidersKey);

/// A memoized [`ProjectBase`] and the resolution diagnostics building it
/// produced (snapshot resolution, then the permission profile's).
#[derive(Debug)]
pub struct ResolvedBase {
    pub base: Arc<ProjectBase>,
    pub diags: Vec<ResolveDiag>,
}

/// The work an [`InputCache`] did: the bases and vocabularies it built (one
/// per distinct key, spec 0.53.0 §3.1) and the documents assembled through
/// it. `Display` is the counter part of the `LUTE_STATS=input-cache` line.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct InputCacheStats {
    pub bases: usize,
    pub vocabs: usize,
    pub assemblies: usize,
}

impl std::fmt::Display for InputCacheStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "bases={} vocabs={} assemblies={}",
            self.bases, self.vocabs, self.assemblies
        )
    }
}

/// Counters summed over the caches this process dropped.
struct DroppedTotals {
    bases: AtomicUsize,
    vocabs: AtomicUsize,
    assemblies: AtomicUsize,
}

static DROPPED: DroppedTotals = DroppedTotals {
    bases: AtomicUsize::new(0),
    vocabs: AtomicUsize::new(0),
    assemblies: AtomicUsize::new(0),
};

/// The [`InputCacheStats`] of every [`InputCache`] this process has dropped,
/// summed: a command's caches, a project model's included, once the command
/// returned. What `LUTE_STATS=input-cache` prints (spec 0.53.0 §3.7).
pub fn dropped_stats() -> InputCacheStats {
    InputCacheStats {
        bases: DROPPED.bases.load(Ordering::Relaxed),
        vocabs: DROPPED.vocabs.load(Ordering::Relaxed),
        assemblies: DROPPED.assemblies.load(Ordering::Relaxed),
    }
}

/// The project input layer one command shares across its documents (ADR
/// D14): each value is built once per key and handed out behind `Arc`. A
/// command that reads two or more documents assembles them all through one
/// cache (spec 0.53.0 §3.1); the files are assumed not to change while it
/// lives.
#[derive(Debug, Default)]
pub struct InputCache {
    projects: Memo<PathBuf, LoadedProject>,
    providers: Memo<ProvidersKey, ProviderSet>,
    snapshots: Memo<SnapshotKey, (CapabilitySnapshot, Vec<ResolveDiag>)>,
    plugin_origins: Memo<PathBuf, lute_check::rel_schema::PluginOrigins>,
    bases: Memo<BaseKey, ResolvedBase>,
    pub imports: ImportCache,
    /// [`Self::base`] builds: one per distinct [`BaseKey`].
    base_builds: AtomicUsize,
    /// Documents assembled through this cache.
    assemblies: AtomicUsize,
}

/// A dropped cache adds its counters to [`dropped_stats`].
impl Drop for InputCache {
    fn drop(&mut self) {
        let stats = self.stats();
        DROPPED.bases.fetch_add(stats.bases, Ordering::Relaxed);
        DROPPED.vocabs.fetch_add(stats.vocabs, Ordering::Relaxed);
        DROPPED
            .assemblies
            .fetch_add(stats.assemblies, Ordering::Relaxed);
    }
}

impl InputCache {
    /// What this cache built and assembled so far.
    pub fn stats(&self) -> InputCacheStats {
        InputCacheStats {
            bases: self.base_builds.load(Ordering::Relaxed),
            vocabs: self.imports.vocab_builds(),
            assemblies: self.assemblies.load(Ordering::Relaxed),
        }
    }

    /// Count one document assembled through this cache.
    pub(crate) fn count_assembly(&self) {
        self.assemblies.fetch_add(1, Ordering::Relaxed);
    }

    pub fn project(&self, dir: &Path) -> Arc<LoadedProject> {
        self.projects
            .get_or_init(dir.to_path_buf(), || load_project(dir))
    }

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
                    ProviderSet::load(dir)
                }),
            None => self
                .providers
                .get_or_init(ProvidersKey::Project(root.map(Path::to_path_buf)), || {
                    project_providers(project)
                }),
        }
    }

    pub fn snapshot(
        &self,
        root: Option<&Path>,
        project: Option<&ProjectConfig>,
        profile: Option<&str>,
        plugins: &BTreeMap<String, yaml_serde::Value>,
    ) -> ResolvedSnapshot {
        let key = (
            root.map(Path::to_path_buf),
            profile.map(str::to_string),
            plugins.clone(),
        );
        self.snapshots
            .get_or_init(key, || resolve_document_snapshot(project, profile, plugins))
    }

    pub fn plugin_origins(&self, plugins_dir: &Path) -> Arc<lute_check::rel_schema::PluginOrigins> {
        self.plugin_origins
            .get_or_init(plugins_dir.to_path_buf(), || plugin_origins(plugins_dir))
    }

    /// The [`ProjectBase`] documents of `root` with this `profile` and
    /// `plugins` frontmatter share (spec 0.49.0 §9.2): the snapshot with the
    /// project's identity flag and the permission profile applied once, the
    /// providers, the manifest's `defaults:` and the plugin origins.
    pub fn base(
        &self,
        root: Option<&Path>,
        project: Option<&ProjectConfig>,
        providers: Option<&Path>,
        profile: Option<&str>,
        plugins: &BTreeMap<String, yaml_serde::Value>,
        permission_profile: Option<&str>,
    ) -> Arc<ResolvedBase> {
        let providers_key = match providers {
            Some(dir) => ProvidersKey::Explicit(dir.to_path_buf()),
            None => ProvidersKey::Project(root.map(Path::to_path_buf)),
        };
        let key = (
            (
                root.map(Path::to_path_buf),
                profile.map(str::to_string),
                plugins.clone(),
            ),
            permission_profile.map(str::to_string),
            providers_key,
        );
        self.bases.get_or_init(key, || {
            self.base_builds.fetch_add(1, Ordering::Relaxed);
            let resolved = self.snapshot(root, project, profile, plugins);
            let (mut snapshot, mut diags) = (resolved.0.clone(), resolved.1.clone());
            snapshot.identity_require_stable =
                project.is_some_and(ProjectConfig::identity_require_stable);
            if let Some(name) = permission_profile {
                match project {
                    Some(config) => match resolve_permissions(config, name) {
                        Ok(permissions) => snapshot.restrict_permissions(&permissions),
                        Err(error) => diags.push(ResolveDiag {
                            span: None,
                            code: error.code().to_string(),
                            message: error.to_string(),
                        }),
                    },
                    None => diags.push(ResolveDiag {
                        span: None,
                        code: "E-PERMISSION-PROFILE".to_string(),
                        message: format!(
                            "`--permission-profile {name}` requires a loaded `lute.project.yaml` \
                             from `--project <DIR>`"
                        ),
                    }),
                }
            }
            let plugin_origins = project
                .map(|p| self.plugin_origins(&p.plugins_dir))
                .unwrap_or_default();
            let base = ProjectBase::new(
                Arc::new(snapshot),
                self.providers(providers, root, project),
                Arc::new(project.map(|p| p.defaults.clone()).unwrap_or_default()),
                plugin_origins,
            );
            ResolvedBase {
                base: Arc::new(base),
                diags,
            }
        })
    }
}

fn plugin_origins(plugins_dir: &Path) -> lute_check::rel_schema::PluginOrigins {
    use lute_check::rel_schema::{effect_origin_key, map_entry_offset, DeclOrigin};
    use lute_core_span::{Span, TextIndex};

    let mut out = lute_check::rel_schema::PluginOrigins::default();
    for file in export_files(plugins_dir, "occasions") {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let Ok(value) = yaml_serde::from_str::<yaml_serde::Value>(&text) else {
            continue;
        };
        let Some(occasions) = value.get("occasions").and_then(|o| o.as_mapping()) else {
            continue;
        };
        let canon = std::fs::canonicalize(&file).unwrap_or_else(|_| file.clone());
        let idx = TextIndex::new(&text);
        for (name, body) in occasions {
            let entry = name
                .as_str()
                .and_then(|n| map_entry_offset(&text, "occasions", n));
            let fields = body.get("payload").and_then(|p| p.as_mapping());
            if let (Some(name), Some(entry), Some(fields)) = (name.as_str(), entry, fields) {
                let payload = text[entry..].find("payload").map_or(entry, |o| entry + o);
                for field in fields.keys().filter_map(|k| k.as_str()) {
                    let start = text[payload..].find(field).map_or(payload, |o| payload + o);
                    out.payloads
                        .entry(effect_origin_key(name, field))
                        .or_insert(DeclOrigin {
                            file: canon.clone(),
                            span: Span::from_bytes(&idx, start, start + field.len()),
                        });
                }
            }
            let (Some(name), Some(gate)) = (
                name.as_str(),
                body.get("raisedWhen").and_then(|g| g.as_str()),
            ) else {
                continue;
            };
            let Some(entry) = map_entry_offset(&text, "occasions", name) else {
                continue;
            };
            let (start, len) = text[entry..]
                .find(gate)
                .map_or((entry, name.len()), |o| (entry + o, gate.len()));
            out.gates.entry(name.to_string()).or_insert(DeclOrigin {
                file: canon.clone(),
                span: Span::from_bytes(&idx, start, start + len),
            });
        }
    }
    for file in export_files(plugins_dir, "directives") {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let Ok(value) = yaml_serde::from_str::<yaml_serde::Value>(&text) else {
            continue;
        };
        let Some(directives) = value.get("directives").and_then(|d| d.as_sequence()) else {
            continue;
        };
        let canon = std::fs::canonicalize(&file).unwrap_or_else(|_| file.clone());
        let idx = TextIndex::new(&text);
        for d in directives {
            let Some(name) = d.get("name").and_then(|n| n.as_str()) else {
                continue;
            };
            let Some(effects) = d.get("effects") else {
                continue;
            };
            let Some(block) = name_line(&text, name) else {
                continue;
            };
            for list in ["asserts", "retracts"] {
                let facts = effects.get(list).and_then(|l| l.as_sequence());
                for fact in facts.into_iter().flatten().filter_map(|f| f.as_str()) {
                    let Ok(parsed) = lute_manifest::schema::FactEffect::try_from(fact.to_string())
                    else {
                        continue;
                    };
                    let Some(o) = text[block..].find(fact) else {
                        continue;
                    };
                    let start = block + o;
                    out.effect_facts
                        .entry(effect_origin_key(name, &parsed.to_string()))
                        .or_insert(DeclOrigin {
                            file: canon.clone(),
                            span: Span::from_bytes(&idx, start, start + fact.len()),
                        });
                }
            }
        }
    }
    out
}

fn name_line(text: &str, name: &str) -> Option<usize> {
    let mut off = 0;
    for line in text.split_inclusive('\n') {
        let at = off;
        off += line.len();
        let t = line.trim_start().trim_start_matches("- ").trim_start();
        let Some(rest) = t.strip_prefix("name:") else {
            continue;
        };
        if rest.trim().trim_matches(['"', '\'']) == name {
            return Some(at);
        }
    }
    None
}

fn export_files(plugins_dir: &Path, export: &str) -> Vec<PathBuf> {
    let yaml_files = |p: PathBuf| -> Vec<PathBuf> {
        if !p.is_dir() {
            return vec![p];
        }
        let mut fs: Vec<PathBuf> = std::fs::read_dir(&p)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|f| matches!(f.extension().and_then(|e| e.to_str()), Some("yaml" | "yml")))
            .collect();
        fs.sort();
        fs
    };
    let mut subs: Vec<PathBuf> = std::fs::read_dir(plugins_dir)
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("plugin.yaml").is_file())
        .collect();
    subs.sort();
    let mut out = Vec::new();
    for sub in subs {
        let Ok(text) = std::fs::read_to_string(sub.join("plugin.yaml")) else {
            continue;
        };
        let Ok(manifest) = yaml_serde::from_str::<yaml_serde::Value>(&text) else {
            continue;
        };
        let Some(rel) = manifest
            .get("exports")
            .and_then(|e| e.get(export))
            .and_then(yaml_serde::Value::as_str)
        else {
            continue;
        };
        out.extend(yaml_files(sub.join(rel)));
    }
    out
}
