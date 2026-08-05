//! `lute compile --all` — compile a whole project and index it.
//!
//! `lute compile` handles ONE file, and `--project <dir>` only resolves that
//! file's capability snapshot. But `docs/runtime/execution-model.md` requires an
//! engine to UNION `entities`/`enums`/`relations`/`seedFacts`/`rules`/
//! `prereqEdges` across every document's artifact before it can evaluate
//! anything — so before 0.8.0 every adopter re-implemented that union by hand,
//! each with its own conflict policy and its own bugs. `--all` ships it:
//! per-document artifacts mirroring the project's own layout, plus a
//! `project.index.json` carrying the union ([`lute_compile::index`]).
//!
//! ## What it reuses, and why that matters
//! Nothing here re-derives project structure. The document set comes from the
//! SAME [`crate::find_lute_files`] walk `check-project` uses, and each
//! document's gate verdict from the SAME single-root
//! [`crate::reconciled_project_results`] + [`crate::gate_for_doc`] pair a
//! single-file `compile --project` runs — computed ONCE for the whole project
//! rather than once per file (which would be quadratic, and could observe a
//! project being edited underneath it differently on each pass). So
//! `compile --all` and `compile <file> --project <dir>` can never disagree about
//! whether a document compiles.
//!
//! ## All-or-nothing
//! Every document is compiled IN MEMORY first. A single failing gate prints its
//! diagnostics and exits `1` having written nothing — a half-written output
//! directory is worse than no output, because a build system would happily ship
//! it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_compile::index::{build_index, IndexInput};
use lute_compile::locale::LocaleBundle;
use lute_compile::Artifact;
use lute_manifest::project::load_project;

use crate::{
    build_input, gate_for_doc, reconciled_project_results, render_diagnostics, DenyPolicy,
};

/// The project index's fixed file name inside the output directory.
const INDEX_FILE: &str = "project.index.json";

/// A component document is a FRAGMENT, not an addressable document: it is
/// inlined into each importer by `normalize_document`, has no identity prefix of
/// its own, and produces no artifact anyone could execute. `check-project` still
/// checks it standalone (a broken component must be reported where it lives);
/// `--all` skips it, because there is nothing to emit.
///
/// Schema documents need no rule at all — they are `*.schema.yaml`, and
/// [`crate::find_lute_files`] only ever yields `*.lute`.
fn is_component_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.ends_with(".component.lute"))
}

/// `path` relative to `root`, joined with forward slashes. `None` when `path` is
/// not under `root` — impossible for the walk's own output, but this never
/// fabricates a path rather than asserting.
///
/// Forward slashes are normative, not cosmetic: an index is a build output that
/// gets copied between machines and packed into game archives, so a
/// backslash-separated Windows path baked into it would be unreadable
/// everywhere else.
fn rel_slash(path: &Path, root: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let mut out = String::new();
    for c in rel.components() {
        if !out.is_empty() {
            out.push('/');
        }
        out.push_str(&c.as_os_str().to_string_lossy());
    }
    (!out.is_empty()).then_some(out)
}

/// One compiled document, held until the whole project is known good.
struct Compiled {
    /// Source path, forward-slash relative to the project root.
    rel: String,
    /// Where the artifact goes, absolute.
    out_path: PathBuf,
    /// Its path relative to the output directory (`documents[].artifact`).
    artifact_rel: String,
    artifact: Artifact,
}

/// Compile every document under `project` into `out_dir` and write the index.
/// See the module doc. Exit `0`, `1` on any gate failure / vocabulary conflict /
/// `--deny`-promoted warning, `2` on I/O.
pub fn run(
    project: &Path,
    out_dir: &Path,
    providers: Option<&Path>,
    json: bool,
    bundle: Option<&LocaleBundle>,
    policy: &DenyPolicy,
) -> ExitCode {
    // 0.10.0 §7 (D-D): `compile` aligns to `check`. `--all` forces every
    // document onto the invoked root, so before this it opened NO nested
    // manifest — T1.10: an inner `identity:` block quietly not applied on a
    // project whose whole localization pipeline is keyed on `lineId`.
    match crate::manifests::validate_manifests_under(project) {
        Ok(verdicts) => {
            if crate::manifests::report_and_gate(&verdicts) {
                return ExitCode::from(1);
            }
        }
        Err(e) => {
            eprintln!("lute: cannot walk {} for manifests: {e}", project.display());
            return ExitCode::from(2);
        }
    }
    // ONE project reconciliation for every document (module doc).
    let reconciled = match reconciled_project_results(project, providers) {
        Ok(r) => r,
        Err(code) => return code,
    };
    // 0.8.0 §9: `identity:` templates are a PROJECT setting, so every document
    // in the project shares one resolved pair — loaded once, exactly as
    // `run_compile`'s own `--project` arm loads it.
    let identity = load_project(project)
        .ok()
        .flatten()
        .map(|p| p.identity)
        .unwrap_or_default();

    let mut compiled: Vec<Compiled> = Vec::new();
    // Path-keyed so the failure report is byte-sorted regardless of which
    // document failed first.
    let mut failures: BTreeMap<PathBuf, String> = BTreeMap::new();
    let mut denied = 0usize;
    let mut warnings = String::new();

    // `per_doc` is a `BTreeMap` — already path-sorted, so `documents` and every
    // conflict message below is deterministic without a second sort.
    for (file, base) in &reconciled.per_doc {
        if is_component_file(file) {
            continue;
        }
        let Some(rel) = rel_slash(file, project) else {
            eprintln!(
                "lute compile --all: {} is not under --project {}",
                file.display(),
                project.display()
            );
            return ExitCode::from(2);
        };
        let Some(built) = build_input(file, providers, Some(project)) else {
            return ExitCode::from(2);
        };
        built.report_project_diags();
        let crate::BuiltInput { input, resolve_error, .. } = built;
        // plugin 0.0.2 §2: an `E-` capability-resolution diagnostic (bad plugin
        // option, missing active plugin, bad identity template) is a build-failing
        // error; it printed above, and it MUST gate here or it would pass silently.
        if resolve_error {
            return ExitCode::from(1);
        }
        let gate = gate_for_doc(&reconciled, file, base);
        match lute_compile::compile_with_check(&input, gate, &identity) {
            Ok(mut artifact) => {
                if let Some(bundle) = bundle {
                    let missing = lute_compile::locale::merge_locales(&mut artifact, bundle);
                    denied += missing.iter().filter(|d| policy.denied(d)).count();
                    warnings.push_str(&render_diagnostics(file, &missing, policy));
                }
                compiled.push(Compiled {
                    artifact_rel: format!("{rel}.json"),
                    out_path: out_dir.join(format!("{rel}.json")),
                    rel,
                    artifact,
                });
            }
            Err(diags) => {
                let rendered = if json {
                    match serde_json::to_string_pretty(&diags) {
                        Ok(mut s) => {
                            s.push('\n');
                            s
                        }
                        Err(e) => {
                            eprintln!("lute: failed to serialize diagnostics: {e}");
                            return ExitCode::from(2);
                        }
                    }
                } else {
                    render_diagnostics(file, &diags, policy)
                };
                failures.insert(file.clone(), rendered);
            }
        }
    }

    // Warnings first: they belong to documents that DID compile, and a reader
    // should see them above whatever verdict follows.
    eprint!("{warnings}");

    if !failures.is_empty() {
        for rendered in failures.values() {
            print!("{rendered}");
        }
        eprintln!(
            "lute compile --all: {} of {} document(s) failed; no output written",
            failures.len(),
            failures.len() + compiled.len()
        );
        return ExitCode::FAILURE;
    }
    if denied > 0 {
        eprintln!("--deny promoted {denied} diagnostic(s); no output written");
        return ExitCode::FAILURE;
    }

    let index = match build_index(
        lute_compile::LUTE_IR_VERSION,
        &compiled
            .iter()
            .map(|c| IndexInput {
                path: c.rel.clone(),
                artifact_path: c.artifact_rel.clone(),
                artifact: &c.artifact,
            })
            .collect::<Vec<_>>(),
    ) {
        Ok(index) => index,
        Err(errors) => {
            for e in &errors {
                eprintln!("lute compile --all: {e}");
            }
            eprintln!(
                "lute compile --all: {} vocabulary conflict(s); no output written",
                errors.len()
            );
            return ExitCode::FAILURE;
        }
    };
    let index_json = match index.to_json() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("lute: failed to serialize {INDEX_FILE}: {e}");
            return ExitCode::from(2);
        }
    };

    // Everything is known good: only NOW does anything touch the filesystem.
    for c in &compiled {
        let mut s = match serde_json::to_string_pretty(&c.artifact) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("lute: failed to serialize artifact for {}: {e}", c.rel);
                return ExitCode::from(2);
            }
        };
        s.push('\n');
        if let Some(parent) = c.out_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                eprintln!("lute: cannot create {}: {e}", parent.display());
                return ExitCode::from(2);
            }
        }
        if let Err(e) = std::fs::write(&c.out_path, &s) {
            eprintln!("lute: cannot write {}: {e}", c.out_path.display());
            return ExitCode::from(2);
        }
    }
    if let Err(e) = std::fs::create_dir_all(out_dir) {
        eprintln!("lute: cannot create {}: {e}", out_dir.display());
        return ExitCode::from(2);
    }
    let index_path = out_dir.join(INDEX_FILE);
    if let Err(e) = std::fs::write(&index_path, index_json.as_bytes()) {
        eprintln!("lute: cannot write {}: {e}", index_path.display());
        return ExitCode::from(2);
    }

    eprintln!(
        "lute compile --all: {} document(s) -> {}",
        compiled.len(),
        out_dir.display()
    );
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn component_fragments_are_skipped_and_ordinary_documents_are_not() {
        assert!(is_component_file(Path::new("a/reaction.component.lute")));
        assert!(!is_component_file(Path::new("a/component.lute")));
        assert!(!is_component_file(Path::new("a/scene.lute")));
    }

    #[test]
    fn relative_paths_are_forward_slashed_and_never_escape_the_root() {
        let root = Path::new("/p");
        assert_eq!(
            rel_slash(Path::new("/p/quests/a.lute"), root).as_deref(),
            Some("quests/a.lute")
        );
        assert_eq!(rel_slash(Path::new("/p/a.lute"), root).as_deref(), Some("a.lute"));
        assert_eq!(rel_slash(Path::new("/other/a.lute"), root), None);
        assert_eq!(rel_slash(root, root), None, "the root itself is not a document");
    }
}
