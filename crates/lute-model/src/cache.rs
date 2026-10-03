//! Per-invocation memoization shared by model document assembly.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use lute_check::{ImportCache, Memo};
use lute_manifest::project::{
    load_project, project_providers, resolve_document_snapshot, ProjectConfig, ResolveDiag,
};
use lute_manifest::provider::ProviderSet;
use lute_manifest::snapshot::CapabilitySnapshot;

pub type LoadedProject = Arc<Result<Option<ProjectConfig>, String>>;
pub type ResolvedSnapshot = Arc<(CapabilitySnapshot, Vec<ResolveDiag>)>;

type SnapshotKey = (Option<PathBuf>, Option<String>, BTreeMap<String, serde_yaml::Value>);

#[derive(Clone, PartialEq, Eq, Hash)]
enum ProvidersKey {
    Explicit(PathBuf),
    Project(Option<PathBuf>),
}

#[derive(Default)]
pub struct InputCache {
    projects: Memo<PathBuf, LoadedProject>,
    providers: Memo<ProvidersKey, Arc<ProviderSet>>,
    snapshots: Memo<SnapshotKey, ResolvedSnapshot>,
    plugin_origins: Memo<PathBuf, Arc<lute_check::rel_schema::PluginOrigins>>,
    pub imports: ImportCache,
}

impl InputCache {
    pub fn project(&self, dir: &Path) -> LoadedProject {
        self.projects
            .get_or_init(dir.to_path_buf(), || Arc::new(load_project(dir)))
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
                    Arc::new(ProviderSet::load(dir))
                }),
            None => self
                .providers
                .get_or_init(ProvidersKey::Project(root.map(Path::to_path_buf)), || {
                    Arc::new(project_providers(project))
                }),
        }
    }

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

    pub fn plugin_origins(
        &self,
        plugins_dir: &Path,
    ) -> Arc<lute_check::rel_schema::PluginOrigins> {
        self.plugin_origins
            .get_or_init(plugins_dir.to_path_buf(), || {
                Arc::new(plugin_origins(plugins_dir))
            })
    }
}

fn plugin_origins(plugins_dir: &Path) -> lute_check::rel_schema::PluginOrigins {
    use lute_check::rel_schema::{effect_origin_key, map_entry_offset, DeclOrigin};
    use lute_core_span::{Span, TextIndex};

    let mut out = lute_check::rel_schema::PluginOrigins::default();
    for file in export_files(plugins_dir, "occasions") {
        let Ok(text) = std::fs::read_to_string(&file) else { continue };
        let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&text) else { continue };
        let Some(occasions) = value.get("occasions").and_then(|o| o.as_mapping()) else { continue };
        let canon = std::fs::canonicalize(&file).unwrap_or_else(|_| file.clone());
        let idx = TextIndex::new(&text);
        for (name, body) in occasions {
            let entry = name.as_str().and_then(|n| map_entry_offset(&text, "occasions", n));
            let fields = body.get("payload").and_then(|p| p.as_mapping());
            if let (Some(name), Some(entry), Some(fields)) = (name.as_str(), entry, fields) {
                let payload = text[entry..].find("payload").map_or(entry, |o| entry + o);
                for field in fields.keys().filter_map(|k| k.as_str()) {
                    let start = text[payload..].find(field).map_or(payload, |o| payload + o);
                    out.payloads.entry(effect_origin_key(name, field)).or_insert(DeclOrigin {
                        file: canon.clone(),
                        span: Span::from_bytes(&idx, start, start + field.len()),
                    });
                }
            }
            let (Some(name), Some(gate)) = (
                name.as_str(),
                body.get("raisedWhen").and_then(|g| g.as_str()),
            ) else { continue };
            let Some(entry) = map_entry_offset(&text, "occasions", name) else { continue };
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
        let Ok(text) = std::fs::read_to_string(&file) else { continue };
        let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&text) else { continue };
        let Some(directives) = value.get("directives").and_then(|d| d.as_sequence()) else { continue };
        let canon = std::fs::canonicalize(&file).unwrap_or_else(|_| file.clone());
        let idx = TextIndex::new(&text);
        for d in directives {
            let Some(name) = d.get("name").and_then(|n| n.as_str()) else { continue };
            let Some(effects) = d.get("effects") else { continue };
            let Some(block) = name_line(&text, name) else { continue };
            for list in ["asserts", "retracts"] {
                let facts = effects.get(list).and_then(|l| l.as_sequence());
                for fact in facts.into_iter().flatten().filter_map(|f| f.as_str()) {
                    let Ok(parsed) = lute_manifest::schema::FactEffect::try_from(fact.to_string()) else { continue };
                    let Some(o) = text[block..].find(fact) else { continue };
                    let start = block + o;
                    out.effect_facts.entry(effect_origin_key(name, &parsed.to_string())).or_insert(DeclOrigin {
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
        let Some(rest) = t.strip_prefix("name:") else { continue };
        if rest.trim().trim_matches(['"', '\'']) == name { return Some(at); }
    }
    None
}

fn export_files(plugins_dir: &Path, export: &str) -> Vec<PathBuf> {
    let yaml_files = |p: PathBuf| -> Vec<PathBuf> {
        if !p.is_dir() { return vec![p]; }
        let mut fs: Vec<PathBuf> = std::fs::read_dir(&p)
            .into_iter().flatten().filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|f| matches!(f.extension().and_then(|e| e.to_str()), Some("yaml" | "yml")))
            .collect();
        fs.sort();
        fs
    };
    let mut subs: Vec<PathBuf> = std::fs::read_dir(plugins_dir)
        .into_iter().flatten().filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("plugin.yaml").is_file()).collect();
    subs.sort();
    let mut out = Vec::new();
    for sub in subs {
        let Ok(text) = std::fs::read_to_string(sub.join("plugin.yaml")) else { continue };
        let Ok(manifest) = serde_yaml::from_str::<serde_yaml::Value>(&text) else { continue };
        let Some(rel) = manifest.get("exports").and_then(|e| e.get(export)).and_then(serde_yaml::Value::as_str) else { continue };
        out.extend(yaml_files(sub.join(rel)));
    }
    out
}
