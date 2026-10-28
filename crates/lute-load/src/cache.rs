//! Per-invocation memoization shared by input assembly, and the tracked
//! variant a long-lived process keeps across requests (spec 0.53.0 §3.5).

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use lute_check::{
    ComponentKey, ImportCache, ImportCounts, ImportKey, ImportNode, Memo, ProjectBase, VocabKey,
};
pub use lute_manifest::deps::{Dependency, DependencySink};
use lute_manifest::deps::{ListFilter, Recorded};
use lute_manifest::project::{
    load_project_in, project_providers_in, resolve_document_snapshot_in, resolve_permissions,
    ProjectConfig, ProjectLoadError, ResolveDiag,
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

impl ProvidersKey {
    fn new(explicit: Option<&Path>, root: Option<&Path>) -> Self {
        match explicit {
            Some(dir) => Self::Explicit(dir.to_path_buf()),
            None => Self::Project(root.map(Path::to_path_buf)),
        }
    }
}

/// What a [`ProjectBase`] depends on: the root, the document's profile and
/// plugin overrides, the permission profile and the providers directory.
type BaseKey = (SnapshotKey, Option<String>, ProvidersKey);

/// One entry of one [`InputCache`] memo.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum InputNode {
    Project(PathBuf),
    Providers(ProvidersKey),
    Snapshot(SnapshotKey),
    PluginOrigins(PathBuf),
    Base(BaseKey),
}

/// The memo keys one document's assembly used (spec 0.53.0 §3.5): the
/// manifest, the base (and through it the snapshot, providers and plugin
/// origins), the import graph, the vocabulary and the component set. A
/// tracked cache retains exactly the entries reachable from the
/// configurations its owner retains ([`InputCache::retain`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Configuration {
    project: Option<PathBuf>,
    base: BaseKey,
    imports: ImportKey,
    vocab: VocabKey,
    components: ComponentKey,
}

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

impl std::ops::AddAssign for InputCacheStats {
    fn add_assign(&mut self, rhs: Self) {
        self.bases += rhs.bases;
        self.vocabs += rhs.vocabs;
        self.assemblies += rhs.assemblies;
    }
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

/// How many entries each memo of a tracked [`InputCache`] holds and how
/// many dependency records they own (spec 0.53.0 §3.5, test 11).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct RetainedCounts {
    pub projects: usize,
    pub providers: usize,
    pub snapshots: usize,
    pub plugin_origins: usize,
    pub bases: usize,
    pub imports: usize,
    pub vocabs: usize,
    pub components: usize,
    pub files: usize,
    pub dependencies: usize,
}

impl std::fmt::Display for RetainedCounts {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "projects={} providers={} snapshots={} plugin_origins={} bases={} imports={} \
             vocabs={} components={} files={} dependencies={}",
            self.projects,
            self.providers,
            self.snapshots,
            self.plugin_origins,
            self.bases,
            self.imports,
            self.vocabs,
            self.components,
            self.files,
            self.dependencies
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

type Rec = Recorded<InputNode>;

/// The project input layer one command shares across its documents (ADR
/// D14): each value is built once per key and handed out behind `Arc`. A
/// command that reads two or more documents assembles them all through one
/// cache (spec 0.53.0 §3.1); the files are assumed not to change while it
/// lives. A [`Self::tracked`] cache also records what each entry was built
/// from and observed on disk, so a long-lived owner can revalidate
/// ([`Self::revalidate`]) and reclaim ([`Self::retain`]) it (§3.5).
#[derive(Debug, Default)]
pub struct InputCache {
    projects: Memo<PathBuf, LoadedProject, Rec>,
    providers: Memo<ProvidersKey, ProviderSet, Rec>,
    snapshots: Memo<SnapshotKey, (CapabilitySnapshot, Vec<ResolveDiag>), Rec>,
    plugin_origins: Memo<PathBuf, lute_check::rel_schema::PluginOrigins, Rec>,
    bases: Memo<BaseKey, ResolvedBase, Rec>,
    pub imports: ImportCache,
    /// [`Self::base`] builds: one per distinct [`BaseKey`].
    base_builds: AtomicUsize,
    /// Documents assembled through this cache.
    assemblies: AtomicUsize,
    /// Whether entries record their children and dependencies.
    tracked: bool,
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
    /// A cache that records, per entry, its children and the dependency
    /// records its build observed (spec 0.53.0 §3.5).
    pub fn tracked() -> Self {
        let mut cache = Self::default();
        cache.imports = ImportCache::tracked();
        cache.tracked = true;
        cache
    }

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

    /// The sink an entry's build records into, and the record it leaves.
    fn sink(&self) -> DependencySink {
        DependencySink::new(self.tracked)
    }

    fn record(&self, children: impl FnOnce() -> Vec<InputNode>, sink: DependencySink) -> Rec {
        if self.tracked {
            Recorded::new(children(), sink)
        } else {
            Recorded::default()
        }
    }

    pub fn project(&self, dir: &Path) -> Arc<LoadedProject> {
        self.projects.get_or_init_with(dir.to_path_buf(), || {
            let sink = self.sink();
            let loaded = load_project_in(dir, &sink);
            (loaded, self.record(Vec::new, sink))
        })
    }

    pub fn providers(
        &self,
        explicit: Option<&Path>,
        root: Option<&Path>,
        project: Option<&ProjectConfig>,
    ) -> Arc<ProviderSet> {
        let key = ProvidersKey::new(explicit, root);
        self.providers.get_or_init_with(key, || {
            let sink = self.sink();
            let set = match explicit {
                Some(dir) => ProviderSet::load_in(dir, &sink),
                None => project_providers_in(project, &sink),
            };
            let children = || match (explicit, root) {
                (None, Some(root)) => vec![InputNode::Project(root.to_path_buf())],
                _ => Vec::new(),
            };
            (set, self.record(children, sink))
        })
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
        self.snapshots.get_or_init_with(key, || {
            let sink = self.sink();
            let resolved = resolve_document_snapshot_in(project, profile, plugins, &sink);
            let children = || {
                root.map(|r| InputNode::Project(r.to_path_buf()))
                    .into_iter()
                    .collect()
            };
            (resolved, self.record(children, sink))
        })
    }

    pub fn plugin_origins(&self, plugins_dir: &Path) -> Arc<lute_check::rel_schema::PluginOrigins> {
        self.plugin_origins
            .get_or_init_with(plugins_dir.to_path_buf(), || {
                let sink = self.sink();
                let origins = plugin_origins(plugins_dir, &sink);
                (origins, self.record(Vec::new, sink))
            })
    }

    fn base_key(
        root: Option<&Path>,
        providers: Option<&Path>,
        profile: Option<&str>,
        plugins: &BTreeMap<String, yaml_serde::Value>,
        permission_profile: Option<&str>,
    ) -> BaseKey {
        (
            (
                root.map(Path::to_path_buf),
                profile.map(str::to_string),
                plugins.clone(),
            ),
            permission_profile.map(str::to_string),
            ProvidersKey::new(providers, root),
        )
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
        let key = Self::base_key(root, providers, profile, plugins, permission_profile);
        self.bases.get_or_init_with(key, || {
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
            let children = || {
                let mut children = vec![
                    InputNode::Snapshot((
                        root.map(Path::to_path_buf),
                        profile.map(str::to_string),
                        plugins.clone(),
                    )),
                    InputNode::Providers(ProvidersKey::new(providers, root)),
                ];
                children.extend(root.map(|r| InputNode::Project(r.to_path_buf())));
                children.extend(project.map(|p| InputNode::PluginOrigins(p.plugins_dir.clone())));
                children
            };
            let resolved = ResolvedBase {
                base: Arc::new(base),
                diags,
            };
            (resolved, self.record(children, DependencySink::default()))
        })
    }

    /// Every dependency record of every entry (spec 0.53.0 §3.5), memo by
    /// memo, in no particular order within one memo.
    pub fn dependencies(&self) -> Vec<Dependency> {
        let mut out = Vec::new();
        let mut collect = |m: &Rec| out.extend(m.records());
        self.projects.for_each_meta(&mut collect);
        self.providers.for_each_meta(&mut collect);
        self.snapshots.for_each_meta(&mut collect);
        self.plugin_origins.for_each_meta(&mut collect);
        self.bases.for_each_meta(&mut collect);
        out.extend(self.imports.dependencies());
        out
    }

    /// The [`Configuration`] `built`'s assembly used: `file` assembled under
    /// `root` with these `providers` and `permission_profile`, as
    /// [`crate::assemble_input_with_mode`] was called.
    pub fn configuration(
        file: &Path,
        providers: Option<&Path>,
        root: Option<&Path>,
        permission_profile: Option<&str>,
        built: &crate::BuiltInput,
    ) -> Configuration {
        let meta = &built.meta;
        let dir = crate::input::document_dir(file);
        Configuration {
            project: root.map(Path::to_path_buf),
            base: Self::base_key(
                root,
                providers,
                meta.profile.as_deref(),
                &meta.plugins,
                permission_profile,
            ),
            imports: ImportCache::import_key(dir, &meta.uses, &meta.extends),
            vocab: ImportCache::vocab_key(&built.input.base, dir, &meta.uses, &meta.extends),
            components: ImportCache::components_key(dir, &meta.components),
        }
    }

    /// Keep exactly the entries reachable from `configurations` (their
    /// keys, then each entry's recorded children) in every memo, with their
    /// dependency records; remove every other built entry. An entry still
    /// being built is kept.
    pub fn retain<'a>(&self, configurations: impl IntoIterator<Item = &'a Configuration>) {
        let mut todo = Vec::new();
        let mut import_roots = Vec::new();
        for c in configurations {
            todo.extend(c.project.clone().map(InputNode::Project));
            todo.push(InputNode::Base(c.base.clone()));
            import_roots.push(ImportNode::Imports(c.imports.clone()));
            import_roots.push(ImportNode::Vocab(c.vocab.clone()));
            import_roots.push(ImportNode::Components(c.components.clone()));
        }
        let mut marked: HashSet<InputNode> = HashSet::new();
        while let Some(node) = todo.pop() {
            if marked.contains(&node) {
                continue;
            }
            let children = match &node {
                InputNode::Project(k) => self.projects.meta(k, |m| m.children.clone()),
                InputNode::Providers(k) => self.providers.meta(k, |m| m.children.clone()),
                InputNode::Snapshot(k) => self.snapshots.meta(k, |m| m.children.clone()),
                InputNode::PluginOrigins(k) => self.plugin_origins.meta(k, |m| m.children.clone()),
                InputNode::Base(k) => self.bases.meta(k, |m| m.children.clone()),
            };
            todo.extend(children.into_iter().flatten());
            marked.insert(node);
        }
        self.projects
            .retain(|k| marked.contains(&InputNode::Project(k.clone())));
        self.providers
            .retain(|k| marked.contains(&InputNode::Providers(k.clone())));
        self.snapshots
            .retain(|k| marked.contains(&InputNode::Snapshot(k.clone())));
        self.plugin_origins
            .retain(|k| marked.contains(&InputNode::PluginOrigins(k.clone())));
        self.bases
            .retain(|k| marked.contains(&InputNode::Base(k.clone())));
        let import_marks = self.imports.mark(import_roots);
        self.imports.sweep(&import_marks);
    }

    /// The entries each memo holds and the dependency records they own.
    pub fn retained(&self) -> RetainedCounts {
        let mut dependencies = 0;
        let mut count = |m: &Rec| dependencies += m.dependency_count();
        let projects = self.projects.for_each_meta(&mut count);
        let providers = self.providers.for_each_meta(&mut count);
        let snapshots = self.snapshots.for_each_meta(&mut count);
        let plugin_origins = self.plugin_origins.for_each_meta(&mut count);
        let bases = self.bases.for_each_meta(&mut count);
        let ImportCounts {
            imports,
            vocabs,
            components,
            files,
            dependencies: import_dependencies,
        } = self.imports.counts();
        RetainedCounts {
            projects,
            providers,
            snapshots,
            plugin_origins,
            bases,
            imports,
            vocabs,
            components,
            files,
            dependencies: dependencies + import_dependencies,
        }
    }

    /// Whether every dependency record of every entry still holds
    /// ([`Dependency::revalidate`]): lookups re-resolved, listings re-listed,
    /// files `stat`ed and, when changed, `dirty` or racy, hashed. A record
    /// whose file is unchanged takes the new fingerprint.
    pub fn revalidate(&self, dirty: &dyn Fn(&Dependency) -> bool) -> bool {
        let mut ok = true;
        let mut check = |m: &Rec| ok = ok && m.revalidate(dirty);
        self.projects.for_each_meta(&mut check);
        self.providers.for_each_meta(&mut check);
        self.snapshots.for_each_meta(&mut check);
        self.plugin_origins.for_each_meta(&mut check);
        self.bases.for_each_meta(&mut check);
        ok && self.imports.revalidate(dirty)
    }

    /// Whether any dependency record names `path` ([`Dependency::names`]).
    pub fn names(&self, path: &Path) -> bool {
        let mut hit = false;
        let mut probe = |m: &Rec| hit = hit || m.names(path);
        self.projects.for_each_meta(&mut probe);
        self.providers.for_each_meta(&mut probe);
        self.snapshots.for_each_meta(&mut probe);
        self.plugin_origins.for_each_meta(&mut probe);
        self.bases.for_each_meta(&mut probe);
        hit || self.imports.names(path)
    }
}

fn plugin_origins(
    plugins_dir: &Path,
    sink: &DependencySink,
) -> lute_check::rel_schema::PluginOrigins {
    use lute_check::rel_schema::{effect_origin_key, map_entry_offset, DeclOrigin};
    use lute_core_span::{Span, TextIndex};

    let mut out = lute_check::rel_schema::PluginOrigins::default();
    for file in export_files(plugins_dir, "occasions", sink) {
        let Ok(text) = sink.read_to_string(&file) else {
            continue;
        };
        let Ok(value) = yaml_serde::from_str::<yaml_serde::Value>(&text) else {
            continue;
        };
        let Some(occasions) = value.get("occasions").and_then(|o| o.as_mapping()) else {
            continue;
        };
        let canon = sink.canonicalize(&file).unwrap_or_else(|_| file.clone());
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
    for file in export_files(plugins_dir, "directives", sink) {
        let Ok(text) = sink.read_to_string(&file) else {
            continue;
        };
        let Ok(value) = yaml_serde::from_str::<yaml_serde::Value>(&text) else {
            continue;
        };
        let Some(directives) = value.get("directives").and_then(|d| d.as_sequence()) else {
            continue;
        };
        let canon = sink.canonicalize(&file).unwrap_or_else(|_| file.clone());
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

fn export_files(plugins_dir: &Path, export: &str, sink: &DependencySink) -> Vec<PathBuf> {
    let yaml_files = |p: PathBuf| -> Vec<PathBuf> {
        if !sink.is_dir(&p) {
            return vec![p];
        }
        sink.list(&p, ListFilter::Yaml)
            .map(|l| l.entries.into_iter().map(|e| e.path).collect())
            .unwrap_or_default()
    };
    let subs: Vec<PathBuf> = sink
        .list(plugins_dir, ListFilter::Dirs)
        .map(|l| l.entries.into_iter().map(|e| e.path).collect())
        .unwrap_or_default();
    let mut out = Vec::new();
    for sub in subs {
        if !sink.is_file(&sub.join("plugin.yaml")) {
            continue;
        }
        let Ok(text) = sink.read_to_string(&sub.join("plugin.yaml")) else {
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
