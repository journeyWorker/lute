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
//! Nothing here re-derives project structure. The document set and each
//! document's checks and artifacts come from one [`lute_model::ProjectModel`]
//! build for the whole project, rather than one pass per file. This keeps
//! `compile --all` aligned with the project's model semantics.
//!
//! ## All-or-nothing
//! Every document is compiled IN MEMORY first. A single failing gate prints its
//! diagnostics and exits `1` having written nothing — a half-written output
//! directory is worse than no output, because a build system would happily ship
//! it.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_compile::index::{build_index, voice_key_collisions, IndexInput, E_DUP_VOICEKEY};
use lute_compile::locale::LocaleBundle;
use lute_compile::ExecutionIr;

use lute_model::{relocate_imported_diags, ModelError, ModelOptions, ProjectModel};
use crate::{render_diagnostics, DenyPolicy};

/// The project index's fixed file name inside the output directory.
const INDEX_FILE: &str = "project.index.json";

/// A component document is a FRAGMENT, not an addressable document: it is
/// inlined into each importer by `normalize_document`, has no identity prefix of
/// checks it standalone (a broken component must be reported where it lives);
/// `--all` skips it, because there is nothing to emit.
///
/// Schema documents need no rule at all — they are `*.schema.yaml`, and
/// [`crate::find_lute_files`] only ever yields `*.lute`.
pub(crate) fn is_component_file(path: &Path) -> bool {
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
    artifact: ExecutionIr,
}

/// Compile every document under `project` into `out_dir` and write the index.
/// See the module doc. Exit `0`, `1` on any gate failure / vocabulary conflict /
/// `--deny`-promoted warning, `2` on I/O.
pub fn run(
    project: &Path,
    out_dir: &Path,
    providers: Option<&Path>,
    permission_profile: Option<&str>,
    json: bool,
    bundle: Option<&LocaleBundle>,
    policy: &DenyPolicy,
) -> ExitCode {
    // 0.10.0 §7 (D-D): `compile` aligns to `check`. `--all` forces every
    // document onto the invoked root, so before this it opened NO nested
    // manifest — T1.10: an inner `identity:` block quietly not applied on a
    // project whose whole localization pipeline is keyed on `lineId`.
    match crate::manifests::validate_manifests_under(project) {
        Ok(mut verdicts) => {
            // D-S: `--all` forces ONE governing root, so every OTHER manifest
            // under the tree is inert. Warn only where that inertness would
            // have changed the resolved surface.
            crate::manifests::mark_inert_under(&mut verdicts, project);
            if crate::manifests::report_and_gate(&verdicts)
                | crate::manifests::gate_chapters(&verdicts)
            {
                return ExitCode::from(1);
            }
        }
        Err(e) => {
            let e = lute_manifest::io_reason(&e);
            eprintln!("lute: cannot walk {} for manifests: {e}", project.display());
            return ExitCode::from(2);
        }
    }
    let opts = ModelOptions {
        providers: providers.map(Path::to_path_buf),
        permission_profile: permission_profile.map(str::to_owned),
        mode: lute_check::Mode::Ci,
        compile: true,
        wip: false,
    };
    let model = match ProjectModel::build(project, &opts) {
        Ok(model) => model,
        Err(ModelError::Compile { path, diagnostics }) => {
            if json {
                match serde_json::to_string_pretty(&diagnostics) {
                    Ok(mut rendered) => {
                        rendered.push('\n');
                        print!("{rendered}");
                    }
                    Err(e) => {
                        eprintln!("lute: failed to serialize diagnostics: {e}");
                        return ExitCode::from(2);
                    }
                }
            } else {
                print!("{}", render_diagnostics(&path, &diagnostics, policy));
            }
            return ExitCode::FAILURE;
        }
        Err(ModelError::Index(errors)) => {
            for e in &errors {
                eprintln!(
                    "lute compile --all: {}",
                    lute_core_span::plain_message(&e.to_string())
                );
            }
            eprintln!(
                "lute compile --all: {} vocabulary conflict(s); no output written",
                errors.len()
            );
            return ExitCode::FAILURE;
        }
        Err(error) => {
            eprintln!("lute: cannot build project {}: {error}", project.display());
            return ExitCode::from(2);
        }
    };
    if model.has_resolution_errors() {
        for document in model.documents() {
            for diagnostic in &document.resolve_diags {
                eprintln!(
                    "lute compile --all: {} [{}] {}",
                    document.path.display(),
                    diagnostic.code,
                    diagnostic.message
                );
            }
        }
        eprintln!("lute compile --all: project resolution failed; no output written");
        return ExitCode::FAILURE;
    }
    let mut project_diagnostics = model.project_diagnostics().to_vec();
    let mut per_doc: Vec<_> = model
        .documents()
        .iter()
        .map(|doc| (doc.path.clone(), doc.check.clone()))
        .collect();
    relocate_imported_diags(&mut per_doc, &mut project_diagnostics, project);
    let project_errors: Vec<_> = project_diagnostics
        .iter()
        .filter(|(_, diagnostic)| diagnostic.severity == lute_core_span::Severity::Error)
        .collect();
    if !project_errors.is_empty() {
        for (_, diagnostic) in project_errors {
            eprintln!("lute compile --all: {}", diagnostic.text());
        }
        eprintln!("lute compile --all: project diagnostics prevent output");
        return ExitCode::FAILURE;
    }

    let mut compiled = Vec::new();
    let mut failures = BTreeMap::new();
    let mut denied = 0usize;
    let mut warnings = String::new();
    for doc in model.documents() {
        if is_component_file(&doc.path) {
            continue;
        }
        let Some(rel) = rel_slash(&doc.path, project) else {
            eprintln!(
                "lute compile --all: {} is not under --project {}",
                doc.path.display(),
                project.display()
            );
            return ExitCode::from(2);
        };
        if !doc.check.ok {
            let rendered = if json {
                match serde_json::to_string_pretty(&doc.check.diagnostics) {
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
                render_diagnostics(&doc.path, &doc.check.diagnostics, policy)
            };
            failures.insert(doc.path.clone(), rendered);
            continue;
        }
        let Some(mut artifact) = doc.artifact.clone() else {
            continue;
        };
        if let Some(bundle) = bundle {
            let merged = lute_compile::locale::merge_locales(&mut artifact, bundle);
            // dsl 0.37.0 §6: a refused translation (`E-L10N-MODIFIERS`)
            // fails its document like a check error.
            if merged.iter().any(|d| d.severity == lute_core_span::Severity::Error) {
                failures.insert(doc.path.clone(), render_diagnostics(&doc.path, &merged, policy));
                continue;
            }
            denied += merged.iter().filter(|d| policy.denied(d)).count();
            warnings.push_str(&render_diagnostics(&doc.path, &merged, policy));
        }
        compiled.push(Compiled {
            artifact_rel: format!("{rel}.json"),
            out_path: out_dir.join(format!("{rel}.json")),
            rel,
            artifact,
        });
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

    let inputs: Vec<IndexInput<'_>> = compiled
        .iter()
        .map(|c| IndexInput {
            path: c.rel.clone(),
            artifact_path: c.artifact_rel.clone(),
            artifact: &c.artifact,
        })
        .collect();
    // 0.21.1 T1-9: every document compiled on its own, so only here can two
    // documents' lines be seen landing on one `voiceKey` — one recording for
    // lines that say different things. Not part of `build_index`: `play` builds
    // the same index and has no voice assets to protect.
    let collisions = voice_key_collisions(&inputs);
    if !collisions.is_empty() {
        for c in &collisions {
            eprintln!("lute compile --all: error [{E_DUP_VOICEKEY}] {c}");
        }
        eprintln!(
            "lute compile --all: {} voiceKey collision(s); no output written",
            collisions.len()
        );
        return ExitCode::FAILURE;
    }
    let index = match build_index(lute_compile::LUTE_IR_VERSION, &inputs) {
        Ok(index) => index,
        Err(errors) => {
            for e in &errors {
                eprintln!(
                    "lute compile --all: {}",
                    lute_core_span::plain_message(&e.to_string())
                );
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
                eprintln!("lute: failed to serialize execution IR for {}: {e}", c.rel);
                return ExitCode::from(2);
            }
        };
        s.push('\n');
        if let Some(parent) = c.out_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                let e = lute_manifest::io_reason(&e);
                eprintln!("lute: cannot create {}: {e}", parent.display());
                return ExitCode::from(2);
            }
        }
        if let Err(e) = std::fs::write(&c.out_path, &s) {
            let e = lute_manifest::io_reason(&e);
            eprintln!("lute: cannot write {}: {e}", c.out_path.display());
            return ExitCode::from(2);
        }
    }
    if let Err(e) = std::fs::create_dir_all(out_dir) {
        let e = lute_manifest::io_reason(&e);
        eprintln!("lute: cannot create {}: {e}", out_dir.display());
        return ExitCode::from(2);
    }
    let index_path = out_dir.join(INDEX_FILE);
    if let Err(e) = std::fs::write(&index_path, index_json.as_bytes()) {
        let e = lute_manifest::io_reason(&e);
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
        assert_eq!(
            rel_slash(Path::new("/p/a.lute"), root).as_deref(),
            Some("a.lute")
        );
        assert_eq!(rel_slash(Path::new("/other/a.lute"), root), None);
        assert_eq!(
            rel_slash(root, root),
            None,
            "the root itself is not a document"
        );
    }
}
