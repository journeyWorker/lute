//! dsl 0.27.0 §4: where the installed plugins declare what a document's
//! check judges — each occasion's `raisedWhen:` gate and each directive's
//! fact effects — so a fault in one is reported once, at the plugin file's
//! line ([`lute_check::rel_schema::PluginOrigins`]), not at every beat or
//! call that uses it. Line scans over the export files, never a second
//! YAML model: a declaration the scan cannot place keeps being reported at
//! its uses.

use std::path::{Path, PathBuf};

use lute_check::rel_schema::{effect_origin_key, map_entry_offset, DeclOrigin, PluginOrigins};
use lute_core_span::{Span, TextIndex};

/// The [`PluginOrigins`] of every plugin package under `plugins_dir`.
pub(crate) fn plugin_origins(plugins_dir: &Path) -> PluginOrigins {
    let mut out = PluginOrigins::default();
    for file in export_files(plugins_dir, "occasions") {
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&text) else {
            continue;
        };
        let Some(occasions) = value.get("occasions").and_then(|o| o.as_mapping()) else {
            continue;
        };
        let canon = std::fs::canonicalize(&file).unwrap_or_else(|_| file.clone());
        let idx = TextIndex::new(&text);
        for (name, body) in occasions {
            // G-6: each `payload:` field, at its key inside the entry.
            let entry = name
                .as_str()
                .and_then(|n| map_entry_offset(&text, "occasions", n));
            let fields = body.get("payload").and_then(|p| p.as_mapping());
            if let (Some(name), Some(entry), Some(fields)) = (name.as_str(), entry, fields) {
                let payload = text[entry..].find("payload").map_or(entry, |o| entry + o);
                for field in fields.keys().filter_map(|k| k.as_str()) {
                    let start = text[payload..].find(field).map_or(payload, |o| payload + o);
                    out.payloads
                        .entry(lute_check::rel_schema::effect_origin_key(name, field))
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
            // The gate's text inside the entry, else the entry key.
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
        let Ok(value) = serde_yaml::from_str::<serde_yaml::Value>(&text) else {
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

/// Byte offset of directive `name`'s `name: <name>` entry line.
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

/// Every YAML file of export `export` of every plugin package under
/// `plugins_dir` (packages and files byte-sorted, as the loader reads them).
pub(crate) fn export_files(plugins_dir: &Path, export: &str) -> Vec<PathBuf> {
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
        let Ok(manifest) = serde_yaml::from_str::<serde_yaml::Value>(&text) else {
            continue;
        };
        let Some(rel) = manifest
            .get("exports")
            .and_then(|e| e.get(export))
            .and_then(serde_yaml::Value::as_str)
        else {
            continue;
        };
        out.extend(yaml_files(sub.join(rel)));
    }
    out
}
