//! `lute catalog refresh`.

use lute_load::manifest_context;
use lute_manifest::core::load_core_snapshot;
use lute_manifest::provider::ProviderSnapshot;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::envelope::{self, CliError};

/// Re-stamp every provider snapshot in `dir` to the current `capabilitySnapshot`
/// and clear `stale`, rewriting each file in place (plugin §10). A missing dir is
/// created empty. Exit `0` on success, `2` on an I/O failure.
///
/// With `--project`, the stamp is the RESOLVED multi-plugin `capabilitySnapshot`
/// (no scene ⇒ default profile, via `resolve_document_snapshot`), matching what a
/// project build validates against (plugin §13). Without it, the core-only
/// (`lute.core`) version is used — behavior identical to before.
///
/// Refresh iterates the directory itself (rather than `ProviderSet::load`, which
/// discards filenames) so each snapshot rewrites to the file it came from.
pub(crate) fn run_refresh(
    project: Option<&Path>,
    providers: Option<&Path>,
    json: bool,
) -> ExitCode {
    // The project is `--project` or the nearest `lute.project.yaml` (0.48.1
    // §2.1); `--providers` names the directory, defaulting to the project's
    // `catalogDir`. With neither there is nothing to refresh.
    let project = project
        .map(Path::to_path_buf)
        .or_else(crate::project::discover_project_from_cwd);
    let project = project.as_deref();
    let dir: PathBuf = match (providers, project) {
        (Some(dir), _) => dir.to_path_buf(),
        (None, Some(root)) => {
            let opts = lute_model::ModelOptions {
                providers: None,
                permission_profile: None,
                mode: lute_check::Mode::Ci,
                compile: false,
                wip: false,
            };
            lute_model::ProjectModel::build_single_root(root, &opts)
                .ok()
                .and_then(|model| model.manifest().map(|config| config.catalog_dir.clone()))
                .unwrap_or_else(|| root.join("catalog"))
        }
        (None, None) => {
            return envelope::fail(
                CliError::usage(
                    "lute catalog refresh: no snapshot directory — pass --providers <DIR>, \
                     or run inside a project (or pass --project <DIR>) to refresh its catalogDir",
                )
                .with_argument("--providers"),
            );
        }
    };
    let dir = dir.as_path();
    if let Err(e) = std::fs::create_dir_all(dir) {
        let e = lute_manifest::io_reason(&e);
        return envelope::fail(CliError::io(
            dir,
            format!("lute: cannot create {}: {e}", dir.display()),
        ));
    }

    // Under a project, stamp the resolved snapshot's version (plugin §13). A
    // malformed project must not silently mis-stamp: surface it and fall back to
    // the core-only version rather than pretending it loaded.
    let version = match project {
        Some(p) => match manifest_context(p) {
            Ok(context) => context.snapshot.version,
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
            let e = lute_manifest::io_reason(&e);
            return envelope::fail(CliError::io(
                dir,
                format!("lute: cannot read {}: {e}", dir.display()),
            ));
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

    let mut refreshed: Vec<String> = Vec::new();
    let mut skipped: Vec<serde_json::Value> = Vec::new();
    for path in &paths {
        let raw = match std::fs::read_to_string(path) {
            Ok(r) => r,
            Err(e) => {
                let e = lute_manifest::io_reason(&e);
                return envelope::fail(CliError::io(
                    path,
                    format!("lute: cannot read {}: {e}", path.display()),
                ));
            }
        };
        let mut snap: ProviderSnapshot = match yaml_serde::from_str(&raw) {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "lute: skipping {} (not a provider snapshot): {e}",
                    path.display()
                );
                skipped.push(serde_json::json!({
                    "path": path.display().to_string(),
                    "reason": format!("not a provider snapshot: {e}"),
                }));
                continue;
            }
        };
        snap.manifest_version = version.clone();
        snap.stale = false;
        let out = match yaml_serde::to_string(&snap) {
            Ok(o) => o,
            Err(e) => {
                return envelope::fail(CliError::io(
                    path,
                    format!("lute: cannot serialize {}: {e}", path.display()),
                ));
            }
        };
        if let Err(e) = std::fs::write(path, out) {
            let e = lute_manifest::io_reason(&e);
            return envelope::fail(CliError::io(
                path,
                format!("lute: cannot write {}: {e}", path.display()),
            ));
        }
        refreshed.push(path.display().to_string());
    }

    let line = format!(
        "refreshed {} snapshot(s) in {} (capabilitySnapshot {version})",
        refreshed.len(),
        dir.display()
    );
    if !json {
        println!("{line}");
        return ExitCode::SUCCESS;
    }
    eprintln!("{line}");
    envelope::emit_report(
        envelope::version::CATALOG_REFRESH,
        0,
        serde_json::json!({
            "providers": dir.display().to_string(),
            "capabilitySnapshot": version,
            "refreshed": refreshed,
            "skipped": skipped,
        }),
    )
}
