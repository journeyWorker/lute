//! dsl 0.27.0 §4: where the installed plugins declare what a document's
//! check judges — each occasion's `raisedWhen:` gate and each directive's
//! fact effects — so a fault in one is reported once, at the plugin file's
//! line ([`lute_check::rel_schema::PluginOrigins`]), not at every beat or
//! call that uses it. Line scans over the export files, never a second
//! YAML model: a declaration the scan cannot place keeps being reported at
//! its uses.

use std::path::{Path, PathBuf};


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
