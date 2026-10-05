//! `lute check-project`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_core_span::{Diagnostic, Severity, Span};
use lute_check::CheckInput;
use lute_load::project_root_for;
use lute_model::relocate_imported_diags;
use crate::cmd_check::{engine_semantic_diags, merge_gate_diags};
use crate::manifests;
use crate::mockcheck;
use crate::output::{pretty_json, print_human, project_report_json, severity_str, DenyPolicy};
use crate::project::{canonical_path, collect_project_inputs, normalize_span_from_text};
/// Recursively `check` every `*.lute` under `dir` ([`collect_project_docs`],
/// nested per-file root resolution — each file resolves against its OWN
/// nearest ancestor `lute.project.yaml`, bounded below by `dir`), reconcile
/// the per-root project analysis ([`reconcile_collected`]), then print the
/// per-file + project-wide report (human or `--json`) and map the verdict to
/// an exit code: `0` clean, `1` when any file has a (post-suppression)
/// `Error` or any resolved root's quest-id/connectivity pass finds one, `2`
/// on an I/O failure walking `dir` or reading a file.
///
/// [`collect_project_docs`]: crate::project::collect_project_docs
pub(crate) fn run_check_project(
    dir: &Path,
    json: bool,
    providers: Option<&Path>,
    policy: &DenyPolicy,
    wip: bool,
    engine: Option<&Path>,
) -> ExitCode {
    let matrix = match crate::EngineMatrix::load(engine) {
        Ok(matrix) => matrix,
        Err(error) => {
            eprintln!("lute check-project: {error}");
            return ExitCode::from(2);
        }
    };
    // 0.10.0 §7 (D-D): validate EVERY manifest under the tree, once each,
    // before any document work. Anchored at the manifest's own path, which
    // the per-document `lute:` replay never carried.
    let manifest_invalid = match manifests::validate_manifests_under(dir) {
        Ok(verdicts) => manifests::report_and_gate(&verdicts),
        Err(e) => {
            let e = lute_manifest::io_reason(&e);
            eprintln!("lute: cannot walk {} for manifests: {e}", dir.display());
            return ExitCode::from(2);
        }
    };
    if manifest_invalid {
        return ExitCode::FAILURE;
    }

    let (mut file_results, by_root, models, resolve_errors) =
        match collect_project_inputs(dir, providers, false, wip) {
            Ok(v) => v,
            Err(code) => return code,
        };
    let inputs: BTreeMap<PathBuf, (PathBuf, &CheckInput)> = models
        .iter()
        .flat_map(|model| {
            model
                .documents()
                .iter()
                .map(|doc| (doc.path.clone(), (model.root().to_path_buf(), &doc.input)))
        })
        .collect();
    let mut project_diags: Vec<(PathBuf, lute_core_span::Diagnostic)> = models
        .iter()
        .flat_map(|model| model.project_diagnostics().iter().cloned())
        .collect();
    relocate_imported_diags(&mut file_results, &mut project_diags, dir);
    // Project constraint verdicts are anchored at their manifest declarations.
    for model in &models {
        let Some(project) = model.manifest() else { continue; };
        let docs: Vec<_> = model.documents().iter().map(|d| (d.path.clone(), d.doc.clone())).collect();
        let project_docs = model.project_docs();
        let folded_refs: Vec<_> = model.documents().iter().map(|d| &d.folded).collect();
        let slots = lute_check::clock_positions::project_objective_slot_results(&project_docs, &folded_refs);
        for result in lute_model::constraints::evaluate_constraints_with_foldeds(model.root(), project, &docs, &folded_refs, model.reconciled().scenarios.get(model.root()).expect("model scenario"), &slots) {
            if !result.declaration_errors.is_empty() {
                let manifest = model.root().join("lute.project.yaml");
                for error in &result.declaration_errors {
                    let mut diagnostic = manifests::as_diagnostic(&error.code, error.message.clone());
                    diagnostic.span = error.span;
                    project_diags.push((manifest.clone(), diagnostic));
                }
                continue;
            }
            if matches!(result.verdict, lute_model::constraints::ConstraintVerdict::Holds | lute_model::constraints::ConstraintVerdict::Unknown) { continue; }
            let Some(decl) = project.constraints.iter().find(|c| c.id == result.id) else { continue; };
            let manifest = model.root().join("lute.project.yaml");
            let text = std::fs::read_to_string(&manifest).unwrap_or_default();
            let idx = lute_core_span::TextIndex::new(&text);
            let span = lute_core_span::Span::from_bytes(&idx, decl.span.start.min(text.len()), decl.span.end.min(text.len()));
            let unknown = matches!(result.verdict, lute_model::constraints::ConstraintVerdict::Unknown);
            let severity = if unknown { Severity::Info } else if matches!(result.evidence, lute_core_span::Evidence::Bounded { .. }) && decl.severity == lute_manifest::constraints::ConstraintSeverity::Error { Severity::Warning } else { match decl.severity { lute_manifest::constraints::ConstraintSeverity::Error => Severity::Error, lute_manifest::constraints::ConstraintSeverity::Warning => Severity::Warning, lute_manifest::constraints::ConstraintSeverity::Info => Severity::Info } };
            let mut diagnostic = manifests::as_diagnostic("E-CONSTRAINT-VIOLATED", format!("constraint `{}` is {}", result.id, if unknown { "unknown" } else { "violated" }));
            diagnostic.severity = severity;
            diagnostic.span = span;
            diagnostic.evidence = Some(result.evidence.clone());
            for cause in result.related {
                let file = cause.provenance.clone().unwrap_or_else(|| manifest.display().to_string());
                diagnostic.related.push(lute_core_span::RelatedDiagnostic { file, diagnostic: cause });
            }
            project_diags.push((manifest, diagnostic));
        }
    }
    for (path, diagnostic) in &mut project_diags {
        if path.extension().and_then(|extension| extension.to_str()) == Some("lute") {
            continue;
        }
        let imported_by = inputs
            .values()
            .filter(|(root, _)| *root == project_root_for(path, dir))
            .count();
        if imported_by > 0 {
            let message = diagnostic
                .message
                .split(" (imported by ")
                .next()
                .unwrap_or(&diagnostic.message);
            diagnostic.message = format!(
                "{message} (imported by {imported_by} document{})",
                if imported_by == 1 { "" } else { "s" }
            );
        }
    }
    for model in &models {
        for doc in model.documents() {
            let (Some(artifact), Some(source_map)) = (&doc.artifact, &doc.source_map) else {
                continue;
            };
            if let Some((_, result)) = file_results.iter_mut().find(|(path, _)| path == &doc.path) {
                merge_gate_diags(result, engine_semantic_diags(&doc.input, artifact, source_map, &matrix));
            }
        }
    }

    for model in &models {
        let index_inputs: Vec<_> = model
            .documents()
            .iter()
            .filter_map(|doc| {
                Some(lute_compile::index::IndexInput {
                    path: doc.path.strip_prefix(model.root()).ok()?.to_string_lossy().replace('\\', "/"),
                    artifact_path: String::new(),
                    artifact: doc.artifact.as_ref()?,
                })
            })
            .collect();
        for collision in lute_compile::index::voice_key_collisions(&index_inputs) {
            project_diags.push((
                model.root().join("lute.project.yaml"),
                manifests::as_diagnostic(lute_compile::index::E_DUP_VOICEKEY, collision.to_string()),
            ));
        }
    }
    fold_inherited_version_stale(&mut file_results, &mut project_diags, &inputs);

    // dsl 0.10.0 §11.1 (**D-V**): `W-DOMAIN-UNREAD` is project-wide only. The
    // per-document halves ride on each `CheckResult`; the union and the
    // difference happen here, once, over the whole walk.
    //
    // Deliberately NOT inside `reconcile_collected`: `gate_for_doc` merges every
    // project-wide diagnostic anchored on a file INTO that file's single-document
    // verdict, so a `W-DOMAIN-UNREAD` produced there would surface from
    // `lute check <file> --project <dir>` and break D-V outright. The anchor
    // is the declaration (dsl 0.24 T3-6): an imported schema's canonical path,
    // printed walk-relative like every other check-project diagnostic.
    {
        let per_file: Vec<(PathBuf, &lute_check::DomainUse)> = file_results
            .iter()
            .map(|(p, r)| (p.clone(), &r.domain_use))
            .collect();
        let canon_dir = canonical_path(dir);
        for (path, mut d) in lute_check::check_project_domain_reads(&per_file) {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            d.span = normalize_span_from_text(&text, d.span);
            let shown = canon_dir
                .as_deref()
                .and_then(|c| path.strip_prefix(c).ok())
                .map_or_else(|| path.clone(), |rel| dir.join(rel));
            project_diags.push((shown, d));
        }
    }

    // dsl 0.24.0 (round-3 T3-16): `W-RELATION-UNREAD` / `W-DEF-UNUSED`, once
    // over the whole walk (every root: a schema shared by two roots is used
    // if either uses it), anchored at the declaring schema file or document
    // — outside `reconcile_collected` for the same D-V reason as above.
    {
        let texts: BTreeMap<&Path, String> = by_root
            .values()
            .flatten()
            .map(|(p, _, _)| (p.as_path(), std::fs::read_to_string(p).unwrap_or_default()))
            .collect();
        let docs: Vec<lute_check::UsageDoc<'_>> = by_root
            .values()
            .flatten()
            .map(|(p, doc, folded)| lute_check::UsageDoc {
                path: p,
                text: texts.get(p.as_path()).map_or("", String::as_str),
                doc,
                folded,
            })
            .collect();
        let foldeds: Vec<&lute_check::FoldedEnv> =
            by_root.values().flatten().map(|(_, _, f)| f).collect();
        let schema_texts: Vec<String> = lute_check::schema_sources(&foldeds)
            .iter()
            .map(|p| std::fs::read_to_string(p).unwrap_or_default())
            .collect();
        let extra: Vec<&str> = schema_texts.iter().map(String::as_str).collect();
        // An origin is the canonical schema path; print it walk-relative like
        // every other check-project diagnostic (`./world.schema.yaml:73:3`).
        let canon_dir = canonical_path(dir);
        for (path, mut d) in lute_check::check_project_usage(&docs, &extra) {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            d.span = normalize_span_from_text(&text, d.span);
            let shown = canon_dir
                .as_deref()
                .and_then(|c| path.strip_prefix(c).ok())
                .map_or_else(|| path.clone(), |rel| dir.join(rel));
            project_diags.push((shown, d));
        }
    }

    // 0.10.0 §8 (#31, D-E): every `mocks/*.yaml` under the root, validated
    // against the schema resolved for its `file:` subject. Anchored at the
    // mock, which is the file at fault — not at the subject scene, which is
    // where these diagnostics rendered before, at `:0:0`.
    match mockcheck::check_mocks_under(dir, &by_root, &inputs) {
        Ok(diags) => project_diags.extend(diags),
        Err(e) => {
            let e = lute_manifest::io_reason(&e);
            eprintln!("lute: cannot walk {} for mocks: {e}", dir.display());
            return ExitCode::from(2);
        }
    }

    // Causes print first, in the order a fix must follow: the manifest, then
    // plugins, then schemas (each group in path order), then the documents
    // (path order, per file), then the project-wide rows about documents and
    // mocks. A consequence reported in another file was already dropped
    // (`reconcile_collected`); this orders what remains.
    project_diags.sort_by_cached_key(|(path, _)| {
        let rank = cause_rank(path);
        let within = if rank < CauseRank::Document {
            path.clone()
        } else {
            PathBuf::new()
        };
        (rank, within)
    });
    let lead = project_diags.partition_point(|(path, _)| cause_rank(path) < CauseRank::Document);

    // §5 verdict: a promoted (denied) diagnostic — in a per-file result OR the
    // project-wide set — fails an otherwise-clean project.
    // A plugin error the walk checked past (printed on the `lute:` channel)
    // still fails the project.
    let project_ok = resolve_errors == 0
        && !project_diags
            .iter()
            .any(|(_, d)| d.severity == Severity::Error || policy.denied(d));
    let file_ok = |r: &lute_check::CheckResult| r.ok && !policy.any_denied(&r.diagnostics);
    let ok = project_ok && file_results.iter().all(|(_, r)| file_ok(r));

    if json {
        let report = match project_report_json(&file_results, &project_diags, ok, policy) {
            Ok(report) => report,
            Err(error) => {
                eprintln!("lute: failed to serialize result: {error}");
                return ExitCode::from(2);
            }
        };
        match pretty_json(&report) {
            Ok(s) => println!("{s}"),
            Err(error) => {
                eprintln!("lute: failed to serialize result: {error}");
                return ExitCode::from(2);
            }
        }
    } else {
        if file_results.is_empty() {
            println!("lute: no .lute files found under {}", dir.display());
        }
        let (causes, rest) = project_diags.split_at(lead);
        for (path, d) in causes {
            print_project_row(path, d, policy);
        }
        for (path, result) in &file_results {
            print_human(path, result, policy);
        }
        if !rest.is_empty() {
            println!("project-wide diagnostics:");
            for (path, d) in rest {
                print_project_row(path, d, policy);
            }
        }
        let project_error_count = project_diags
            .iter()
            .filter(|(_, d)| d.severity == Severity::Error || policy.denied(d))
            .count();
        let project_warning_count = project_diags.len() - project_error_count;
        let project_error_count = project_error_count + resolve_errors;
        if ok {
            println!(
                "ok: {} ({} file(s), {} project-wide warning(s))",
                dir.display(),
                file_results.len(),
                project_warning_count
            );
        } else {
            println!(
                "failed: {} ({} file(s), {} project-wide error(s), {} project-wide warning(s))",
                dir.display(),
                file_results.len(),
                project_error_count,
                project_warning_count
            );
        }
    }

    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Where a project-wide row's file sits in the order causes are fixed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum CauseRank {
    Manifest,
    Plugin,
    Schema,
    Document,
}

/// The manifest is `lute.project.yaml`; a plugin file lies in a directory
/// holding a `plugin.yaml`; a schema is any other YAML file except a mock
/// (`mocks/*.yaml`); everything else — documents and mocks — is a
/// document-level row.
fn cause_rank(path: &Path) -> CauseRank {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    if name == "lute.project.yaml" {
        return CauseRank::Manifest;
    }
    if name == "plugin.yaml"
        || path
            .ancestors()
            .skip(1)
            .any(|dir| dir.join("plugin.yaml").is_file())
    {
        return CauseRank::Plugin;
    }
    let yaml = matches!(
        path.extension().and_then(|e| e.to_str()),
        Some("yaml" | "yml")
    );
    let mock = path
        .parent()
        .and_then(Path::file_name)
        .and_then(|n| n.to_str())
        == Some("mocks");
    if yaml && !mock {
        CauseRank::Schema
    } else {
        CauseRank::Document
    }
}

/// One project-wide row, human form.
fn print_project_row(path: &Path, d: &Diagnostic, policy: &DenyPolicy) {
    let denied = policy.denied(d);
    if d.span.line == 0 && d.span.column == 0 {
        // A right file with no right line (D-Z for manifests, D-AB for
        // mocks): print no position rather than claiming `0:0`.
        println!("{}", manifests::spanless_line(path, d, denied));
        return;
    }
    let marker = if denied { " [denied]" } else { "" };
    println!(
        "{}:{}:{}: {} [{}]{marker} {}",
        path.display(),
        d.span.line,
        d.span.column,
        if denied {
            "error"
        } else {
            severity_str(d.severity)
        },
        d.code,
        d.text(),
    );
}

/// LH N18: a stale `luteVersion` a document inherits from its manifest's
/// `defaults:` is one fact about the manifest, not one per document. Every
/// per-file copy is dropped and one warning is reported per root, at the
/// manifest's `luteVersion:` line, counting the documents that inherit it.
fn fold_inherited_version_stale(
    file_results: &mut [(PathBuf, lute_check::CheckResult)],
    project_diags: &mut Vec<(PathBuf, Diagnostic)>,
    inputs: &BTreeMap<PathBuf, (PathBuf, &CheckInput)>,
) {
    let inherited = |d: &Diagnostic| {
        d.code == lute_check::W_LUTE_VERSION_STALE
            && d.message.starts_with(lute_check::INHERITED_LUTE_VERSION)
    };
    let mut roots: BTreeMap<PathBuf, (Diagnostic, usize)> = BTreeMap::new();
    for (path, result) in file_results.iter_mut() {
        let Some((root, _)) = inputs.get(path) else {
            continue;
        };
        let Some(d) = result.diagnostics.iter().find(|d| inherited(d)).cloned() else {
            continue;
        };
        result.diagnostics.retain(|d| !inherited(d));
        roots.entry(root.clone()).or_insert((d, 0)).1 += 1;
    }
    for (root, (mut d, n)) in roots {
        let manifest = root.join("lute.project.yaml");
        let text = std::fs::read_to_string(&manifest).unwrap_or_default();
        let at = text
            .match_indices("luteVersion")
            .find(|(i, _)| {
                text[i + "luteVersion".len()..]
                    .trim_start()
                    .starts_with(':')
            })
            .map_or(0, |(i, _)| i);
        let end = if text.is_empty() {
            0
        } else {
            at + "luteVersion".len()
        };
        d.span = Span::from_bytes(&lute_core_span::TextIndex::new(&text), at, end);
        d.message.push_str(&format!(
            " — every document inherits it ({n} document{})",
            if n == 1 { "" } else { "s" }
        ));
        project_diags.push((manifest, d));
    }
}

