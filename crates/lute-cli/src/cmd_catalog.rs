//! `lute catalog refresh`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_manifest::core::load_core_snapshot;
use lute_manifest::project::{load_project, resolve_document_snapshot};
use lute_manifest::provider::ProviderSnapshot;

/// Re-stamp every provider snapshot in `dir` to the current `capabilityVersion`
/// and clear `stale`, rewriting each file in place (plugin §10). A missing dir is
/// created empty. Exit `0` on success, `2` on an I/O failure.
///
/// With `--project`, the stamp is the RESOLVED multi-plugin `capabilityVersion`
/// (no scene ⇒ default profile, via `resolve_document_snapshot`), matching what a
/// project build validates against (plugin §13). Without it, the core-only
/// (`lute.core`) version is used — behavior identical to before.
///
/// Refresh iterates the directory itself (rather than `ProviderSet::load`, which
/// discards filenames) so each snapshot rewrites to the file it came from.
pub(crate) fn run_refresh(dir: &Path, project: Option<&Path>) -> ExitCode {
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("lute: cannot create {}: {e}", dir.display());
        return ExitCode::from(2);
    }

    // Under a project, stamp the resolved snapshot's version (plugin §13). A
    // malformed project must not silently mis-stamp: surface it and fall back to
    // the core-only version rather than pretending it loaded.
    let version = match project {
        Some(p) => match load_project(p) {
            Ok(cfg) => {
                resolve_document_snapshot(cfg.as_ref(), None, &BTreeMap::new())
                    .0
                    .version
            }
            Err(e) => {
                eprintln!("lute: {e}");
                load_core_snapshot().version
            }
        },
        None => load_core_snapshot().version,
    };

    let entries = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) => {
            eprintln!("lute: cannot read {}: {e}", dir.display());
            return ExitCode::from(2);
        }
    };

    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.is_file()
                && matches!(
                    p.extension().and_then(|x| x.to_str()),
                    Some("yaml") | Some("yml")
                )
        })
        .collect();
    paths.sort();

    let mut refreshed = 0usize;
    for path in &paths {
        let raw = match std::fs::read_to_string(path) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("lute: cannot read {}: {e}", path.display());
                return ExitCode::from(2);
            }
        };
        let mut snap: ProviderSnapshot = match serde_yaml::from_str(&raw) {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "lute: skipping {} (not a provider snapshot): {e}",
                    path.display()
                );
                continue;
            }
        };
        snap.manifest_version = version.clone();
        snap.stale = false;
        let out = match serde_yaml::to_string(&snap) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("lute: cannot serialize {}: {e}", path.display());
                return ExitCode::from(2);
            }
        };
        if let Err(e) = std::fs::write(path, out) {
            eprintln!("lute: cannot write {}: {e}", path.display());
            return ExitCode::from(2);
        }
        refreshed += 1;
    }

    println!(
        "refreshed {refreshed} snapshot(s) in {} (capabilityVersion {version})",
        dir.display()
    );
    ExitCode::SUCCESS
}
