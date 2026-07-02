//! `lute` — the headless CLI wrapper around the `check()` core (Phase 5).
//!
//! Two subcommands, both thin shells over library code (arch: "`check()` is the
//! contract, not the LSP protocol" — the CLI adds argument parsing, file I/O, and
//! output formatting, and owns NO validation logic):
//!
//! - `lute check <file> [--json] [--providers <dir>]` — statically validate one
//!   `.lute` document against the built-in `lute.core` snapshot plus an optional
//!   pinned provider catalog. Exit `0` when clean, `1` when any `Error`-severity
//!   diagnostic is present (`CheckResult::ok`), `2` on an I/O failure. `--json`
//!   prints the serialized [`CheckResult`]; otherwise a human line per diagnostic.
//! - `lute catalog refresh <dir>` — re-stamp every pinned provider snapshot in
//!   `<dir>` against the current `capabilityVersion` and clear its `stale` flag,
//!   rewriting each file in the flat on-disk format `ProviderSet::load` reads
//!   (plugin §10; "an explicit `catalog refresh` precedes a build"). Correctness
//!   never depends on a live/remote catalog — refresh only canonicalizes and
//!   re-stamps the already-pinned artifacts, so `refresh` then `load` round-trips.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use lute_check::{check, parse_meta, CheckInput, Mode};
use lute_core_span::Severity;
use lute_manifest::core::load_core_snapshot;
use lute_manifest::project::{load_project, resolve_document_snapshot};
use lute_manifest::provider::{ProviderSet, ProviderSnapshot};
use lute_manifest::snapshot::CapabilitySnapshot;

#[derive(Parser)]
#[command(
    name = "lute",
    about = "Static checker for .lute visual-novel scenarios"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Statically validate a `.lute` document.
    Check {
        /// Path to the `.lute` file to check.
        file: PathBuf,
        /// Emit the full `CheckResult` as JSON instead of human-readable lines.
        #[arg(long)]
        json: bool,
        /// Directory of pinned provider snapshots to resolve ids against.
        #[arg(long, value_name = "DIR")]
        providers: Option<PathBuf>,
        /// Project directory (`lute.project.yaml` + `plugins/`) whose installed
        /// plugins resolve the document's activated capability snapshot (plugin
        /// §4/§11). Omit for a core-only (`lute.core`) check.
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
    },
    /// Provider-catalog maintenance.
    #[command(subcommand)]
    Catalog(CatalogCommand),
}

#[derive(Subcommand)]
enum CatalogCommand {
    /// Re-stamp and rewrite the pinned provider snapshots in a directory.
    Refresh {
        /// Directory holding the flat per-snapshot YAML files.
        dir: PathBuf,
        /// Project directory (`lute.project.yaml` + `plugins/`) whose resolved
        /// multi-plugin `capabilityVersion` stamps each snapshot instead of the
        /// core-only version (plugin §10/§13). Omit for the core baseline.
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Check {
            file,
            json,
            providers,
            project,
        } => run_check(&file, json, providers.as_deref(), project.as_deref()),
        Command::Catalog(CatalogCommand::Refresh { dir, project }) => {
            run_refresh(&dir, project.as_deref())
        }
    }
}

/// Run `check` over one file and print its result. Exit `0` clean / `1` on an
/// error diagnostic / `2` on an I/O failure.
fn run_check(
    file: &Path,
    json: bool,
    providers: Option<&Path>,
    project: Option<&Path>,
) -> ExitCode {
    let text = match std::fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("lute: cannot read {}: {e}", file.display());
            return ExitCode::from(2);
        }
    };

    let providers = match providers {
        Some(dir) => ProviderSet::load(dir),
        None => ProviderSet::default(),
    };

    // Resolve the capability snapshot the document is validated against. With
    // `--project`, load the project and assemble the scene's activated snapshot
    // (plugin §4/§11); without it, `resolve_document_snapshot(None, ..)` returns
    // the core-only `lute.core` baseline — behavior identical to before.
    let project = match project {
        Some(dir) => match load_project(dir) {
            Ok(p) => p,
            Err(e) => {
                // A malformed project must not silently mis-validate: surface it
                // and fall back to core-only rather than pretending it loaded.
                eprintln!("lute: {e}");
                None
            }
        },
        None => None,
    };

    // Lift the scene's frontmatter `profile`/`plugins` — both built-in keys, so a
    // default snapshot suffices to type them (they are not capability-gated).
    let (doc, _) = lute_syntax::parse(&text);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());

    let (snapshot, rdiags) =
        resolve_document_snapshot(project.as_ref(), meta0.profile.as_deref(), &meta0.plugins);
    for d in &rdiags {
        eprintln!("lute: resolve: {}", d.message);
    }

    let input = CheckInput {
        text,
        uri: file.display().to_string(),
        snapshot,
        providers,
        // Batch/build analysis, not the interactive LSP default (both behave
        // identically today; the checker does not branch on mode yet).
        mode: Mode::Ci,
    };
    let result = check(&input);

    if json {
        // Serialization is infallible for this concrete, primitive-only shape.
        match serde_json::to_string_pretty(&result) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("lute: failed to serialize result: {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        print_human(file, &result);
    }

    if result.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// One `file:line:col: severity [CODE] message` line per diagnostic, then a
/// summary. Mirrors the sorted order `check()` already applied.
fn print_human(file: &Path, result: &lute_check::CheckResult) {
    let path = file.display();
    for d in &result.diagnostics {
        println!(
            "{path}:{}:{}: {} [{}] {}",
            d.span.line,
            d.span.column,
            severity_str(d.severity),
            d.code,
            d.message,
        );
    }
    let errors = result
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    let warnings = result
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .count();
    if result.ok {
        println!("ok: {path} ({warnings} warning(s))");
    } else {
        println!("failed: {path} ({errors} error(s), {warnings} warning(s))");
    }
}

fn severity_str(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
        Severity::Hint => "hint",
    }
}

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
fn run_refresh(dir: &Path, project: Option<&Path>) -> ExitCode {
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
