//! Plugin package loader (plugin §4). Reads a `plugins/<id>/` directory into a
//! `LoadedPlugin`, honoring `exports`, sorting files byte-wise, and rejecting
//! per-package duplicate ids within a kind. Never panics: every failure is a
//! `LoadError` in the returned vec.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::schema::*;
use crate::types::Type;

#[derive(Clone, Debug)]
pub struct LoadedPlugin {
    pub manifest: PluginManifest,
    pub directives: Vec<DirectiveDecl>,
    pub enums: BTreeMap<String, Vec<String>>,
    pub state_shapes: Vec<StateShape>,
    pub state_templates: Vec<StateTemplate>,
    pub providers: Vec<ProviderDecl>,
    pub bridge: Vec<BridgeCapability>,
    pub defs: Vec<DefDecl>,
    pub frontmatter: BTreeMap<String, Type>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LoadError {
    Manifest {
        dir: String,
        msg: String,
    },
    Parse {
        file: String,
        msg: String,
    },
    DuplicateId {
        kind: String,
        id: String,
    },
    MissingExportDir {
        export: String,
        path: String,
    },
    /// An I/O or encoding failure reading a listed file/dir.
    Io {
        path: String,
        msg: String,
    },
}

/// Read one plugin package. `dir` MUST contain `plugin.yaml`.
pub fn load_plugin_dir(dir: &Path) -> Result<LoadedPlugin, Vec<LoadError>> {
    let mut errs = Vec::new();

    let manifest_path = dir.join("plugin.yaml");
    let manifest: PluginManifest = match std::fs::read_to_string(&manifest_path) {
        Ok(s) => match serde_yaml::from_str(&s) {
            Ok(m) => m,
            Err(e) => {
                return Err(vec![LoadError::Manifest {
                    dir: dir.display().to_string(),
                    msg: e.to_string(),
                }])
            }
        },
        Err(e) => {
            return Err(vec![LoadError::Manifest {
                dir: dir.display().to_string(),
                msg: e.to_string(),
            }])
        }
    };

    let mut out = LoadedPlugin {
        manifest: manifest.clone(),
        directives: Vec::new(),
        enums: BTreeMap::new(),
        state_shapes: Vec::new(),
        state_templates: Vec::new(),
        providers: Vec::new(),
        bridge: Vec::new(),
        defs: Vec::new(),
        frontmatter: BTreeMap::new(),
    };

    // Read each declared export. A relative export path resolves under `dir`.
    for (export, rel) in &manifest.exports {
        let path = dir.join(rel);
        if !path.exists() {
            errs.push(LoadError::MissingExportDir {
                export: export.clone(),
                path: path.display().to_string(),
            });
            continue;
        }
        match export.as_str() {
            "directives" => read_kind::<DirectivesFile, _>(&path, &mut errs, |f, e| {
                merge_directives(&mut out.directives, f.directives, e)
            }),
            "state" => read_state(&path, &mut out, &mut errs),
            "providers" => read_kind::<ProvidersFile, _>(&path, &mut errs, |f, e| {
                merge_named(
                    &mut out.providers,
                    f.providers,
                    "provider",
                    |p| p.name.clone(),
                    e,
                )
            }),
            "bridge" => read_kind::<BridgeFile, _>(&path, &mut errs, |f, e| {
                merge_bridge(&mut out.bridge, f.bridge, e)
            }),
            "defs" => read_kind::<DefsFile, _>(&path, &mut errs, |f, e| {
                merge_named(&mut out.defs, f.defs, "def", |d| d.name.clone(), e)
            }),
            "enums" => read_enums(&path, &mut out.enums, &mut errs),
            "frontmatter" => read_kind::<FrontmatterFile, _>(&path, &mut errs, |f, e| {
                merge_frontmatter(&mut out.frontmatter, f.frontmatter, e)
            }),
            "docs" => { /* non-normative (plugin §6.7); skip */ }
            "assetkinds" => { /* plugin §6.9 deferred to a later plan; ignore for now */ }
            _ => { /* unknown export key: ignore (closed set enforced by validate) */ }
        }
    }

    if errs.is_empty() {
        Ok(out)
    } else {
        Err(errs)
    }
}

/// Scan `dir` for plugin packages (each immediate subdirectory containing a
/// `plugin.yaml`), in sorted order, and index by manifest id. A duplicate id
/// across packages is a `LoadError::DuplicateId { kind: "plugin", .. }` (the
/// later package is dropped). A missing `dir` yields an empty registry.
pub fn load_plugins_dir(dir: &Path) -> (crate::resolve::InstalledPlugins, Vec<LoadError>) {
    use crate::resolve::{InstalledPlugin, InstalledPlugins};
    let mut reg = InstalledPlugins::default();
    let mut errs = Vec::new();
    let mut subs: Vec<_> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.is_dir())
            .collect(),
        Err(_) => return (reg, errs),
    };
    subs.sort();
    for sub in subs {
        if !sub.join("plugin.yaml").is_file() {
            continue;
        }
        match load_plugin_dir(&sub) {
            Ok(loaded) => {
                let id = loaded.manifest.id.clone();
                if reg.by_id.contains_key(&id) {
                    errs.push(LoadError::DuplicateId {
                        kind: "plugin".into(),
                        id,
                    });
                } else {
                    reg.by_id.insert(id, InstalledPlugin { loaded });
                }
            }
            Err(mut e) => errs.append(&mut e),
        }
    }
    (reg, errs)
}

/// Read a single YAML file OR every `*.yaml`/`*.yml` in a dir (sorted byte-wise),
/// deserialize each to `F`, and hand it to `merge`.
fn read_kind<F, M>(path: &Path, errs: &mut Vec<LoadError>, mut merge: M)
where
    F: serde::de::DeserializeOwned,
    M: FnMut(F, &mut Vec<LoadError>),
{
    for file in yaml_files(path, errs) {
        let s = match std::fs::read_to_string(&file) {
            Ok(s) => s,
            Err(e) => {
                errs.push(LoadError::Io {
                    path: file.display().to_string(),
                    msg: e.to_string(),
                });
                continue;
            }
        };
        match serde_yaml::from_str::<F>(&s) {
            Ok(f) => merge(f, errs),
            Err(e) => errs.push(LoadError::Parse {
                file: file.display().to_string(),
                msg: e.to_string(),
            }),
        }
    }
}

/// `state/` holds `shapes.yaml` (stateShapes) and/or `templates.yaml` (stateTemplates).
fn read_state(path: &Path, out: &mut LoadedPlugin, errs: &mut Vec<LoadError>) {
    for file in yaml_files(path, errs) {
        let s = match std::fs::read_to_string(&file) {
            Ok(s) => s,
            Err(e) => {
                errs.push(LoadError::Io {
                    path: file.display().to_string(),
                    msg: e.to_string(),
                });
                continue;
            }
        };
        if let Ok(f) = serde_yaml::from_str::<ShapesFile>(&s) {
            merge_named(
                &mut out.state_shapes,
                f.state_shapes,
                "shape",
                |s| s.name.clone(),
                errs,
            );
        } else if let Ok(f) = serde_yaml::from_str::<TemplatesFile>(&s) {
            merge_named(
                &mut out.state_templates,
                f.state_templates,
                "template",
                |t| t.name.clone(),
                errs,
            );
        } else {
            errs.push(LoadError::Parse {
                file: file.display().to_string(),
                msg: "not a state shapes/templates file".into(),
            });
        }
    }
}

fn read_enums(path: &Path, dst: &mut BTreeMap<String, Vec<String>>, errs: &mut Vec<LoadError>) {
    for file in yaml_files(path, errs) {
        let s = match std::fs::read_to_string(&file) {
            Ok(s) => s,
            Err(e) => {
                errs.push(LoadError::Io {
                    path: file.display().to_string(),
                    msg: e.to_string(),
                });
                continue;
            }
        };
        match serde_yaml::from_str::<EnumsFile>(&s) {
            Ok(f) => {
                for (k, v) in f.enums {
                    if dst.insert(k.clone(), v).is_some() {
                        errs.push(LoadError::DuplicateId {
                            kind: "enum".into(),
                            id: k,
                        });
                    }
                }
            }
            Err(e) => errs.push(LoadError::Parse {
                file: file.display().to_string(),
                msg: e.to_string(),
            }),
        }
    }
}

/// Every `*.yaml`/`*.yml` under `path` (a dir), sorted byte-wise; or `[path]`
/// itself if `path` is a file (plugin §4 sort determinism). A `read_dir` failure
/// or any per-entry error is surfaced as `LoadError::Io` rather than silently
/// dropped, so a listed-but-unreadable export dir never loads as empty.
///
/// NOTE: the dir-enumeration failure paths (`read_dir` Err, per-entry Err) are
/// not portably testable — they require an inaccessible/racing directory — so
/// they are hardened by construction with no dedicated test.
fn yaml_files(path: &Path, errs: &mut Vec<LoadError>) -> Vec<std::path::PathBuf> {
    if path.is_file() {
        return vec![path.to_path_buf()];
    }
    let entries = match std::fs::read_dir(path) {
        Ok(entries) => entries,
        Err(e) => {
            errs.push(LoadError::Io {
                path: path.display().to_string(),
                msg: e.to_string(),
            });
            return Vec::new();
        }
    };
    let mut v = Vec::new();
    for entry in entries {
        let p = match entry {
            Ok(entry) => entry.path(),
            Err(e) => {
                errs.push(LoadError::Io {
                    path: path.display().to_string(),
                    msg: e.to_string(),
                });
                continue;
            }
        };
        if p.is_file()
            && matches!(
                p.extension().and_then(|x| x.to_str()),
                Some("yaml") | Some("yml")
            )
        {
            v.push(p);
        }
    }
    v.sort();
    v
}

fn merge_named<T, K: Fn(&T) -> String>(
    dst: &mut Vec<T>,
    items: Vec<T>,
    kind: &str,
    key: K,
    errs: &mut Vec<LoadError>,
) {
    let mut seen: BTreeSet<String> = dst.iter().map(&key).collect();
    for it in items {
        let id = key(&it);
        if !seen.insert(id.clone()) {
            errs.push(LoadError::DuplicateId {
                kind: kind.into(),
                id,
            });
        } else {
            dst.push(it);
        }
    }
}

fn merge_directives(
    dst: &mut Vec<DirectiveDecl>,
    items: Vec<DirectiveDecl>,
    errs: &mut Vec<LoadError>,
) {
    merge_named(dst, items, "directive", |d| d.name.clone(), errs);
}

fn merge_bridge(
    dst: &mut Vec<BridgeCapability>,
    items: Vec<BridgeCapability>,
    errs: &mut Vec<LoadError>,
) {
    let mut seen: BTreeSet<(String, String)> = dst
        .iter()
        .map(|b| (b.service.clone(), b.operation.clone()))
        .collect();
    for b in items {
        let k = (b.service.clone(), b.operation.clone());
        if !seen.insert(k) {
            errs.push(LoadError::DuplicateId {
                kind: "bridge".into(),
                id: format!("{}.{}", b.service, b.operation),
            });
        } else {
            dst.push(b);
        }
    }
}

fn merge_frontmatter(
    dst: &mut BTreeMap<String, Type>,
    items: Vec<FrontmatterDecl>,
    errs: &mut Vec<LoadError>,
) {
    for f in items {
        if dst.insert(f.key.clone(), f.schema).is_some() {
            errs.push(LoadError::DuplicateId {
                kind: "frontmatter".into(),
                id: f.key,
            });
        }
    }
}
