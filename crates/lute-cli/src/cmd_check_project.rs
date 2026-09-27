//! `lute check-project`.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_check::CheckInput;
use lute_core_span::{Diagnostic, Severity, Span};
use lute_manifest::project::load_project;
use rayon::prelude::*;

use crate::cmd_check::{compile_gate_diags, merge_gate_diags};
use crate::compile_all;
use crate::manifests;
use crate::mockcheck;
use crate::output::{apply_deny_json, print_human, severity_str, DenyPolicy};
use crate::project::reconcile::{
    reconcile_collected, relocate_imported_diags, rollup_component_body_diags,
};
use crate::project::{collect_project_inputs, normalize_span_from_text};

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
) -> ExitCode {
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

    let (file_results, by_root, inputs, resolve_errors) =
        match collect_project_inputs(dir, providers, false) {
            Ok(v) => v,
            Err(code) => return code,
        };
    // Keyed before `reconcile_collected` takes the (aligned) results.
    let inputs: BTreeMap<PathBuf, (PathBuf, CheckInput)> = file_results
        .iter()
        .map(|(p, _)| p.clone())
        .zip(inputs)
        .collect();
    let (mut file_results, mut project_diags, _nodes_by_path, _) =
        reconcile_collected(file_results, &by_root, wip);

    project_compile_pass(&mut file_results, &mut project_diags, &inputs);
    fold_inherited_version_stale(&mut file_results, &mut project_diags, &inputs);

    // Round-5 T3-4, then dsl 0.10.0 §9 rule 2.
    relocate_imported_diags(&mut file_results, &mut project_diags, dir);
    rollup_component_body_diags(&mut file_results);

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
        let canon_dir = std::fs::canonicalize(dir).ok();
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
        let canon_dir = std::fs::canonicalize(dir).ok();
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
        // Reuse each type's own `Serialize` impl (`CheckResult`/`Diagnostic`,
        // both defined — and derived — in lute-check/lute-core-span) and
        // merge in the file path as a sibling key, rather than declaring a
        // new wrapper type (would need `serde`'s derive macro as a direct
        // dependency this crate doesn't otherwise need). The §5 deny promotion
        // is overlaid at this CLI layer (`apply_deny_json` + a promoted `ok`),
        // never in lute-check's shape.
        let files_json: Vec<serde_json::Value> = file_results
            .iter()
            .map(|(path, result)| {
                let mut v = serde_json::to_value(result).unwrap_or_else(|_| serde_json::json!({}));
                if let Some(arr) = v.get_mut("diagnostics").and_then(|x| x.as_array_mut()) {
                    for (d, jd) in result.diagnostics.iter().zip(arr.iter_mut()) {
                        apply_deny_json(d, policy, jd);
                    }
                }
                if let serde_json::Value::Object(map) = &mut v {
                    map.insert("ok".into(), serde_json::json!(file_ok(result)));
                    map.insert("path".into(), path.display().to_string().into());
                }
                v
            })
            .collect();
        let project_json: Vec<serde_json::Value> = project_diags
            .iter()
            .map(|(path, d)| {
                let mut v = serde_json::to_value(d).unwrap_or_else(|_| serde_json::json!({}));
                apply_deny_json(d, policy, &mut v);
                if let serde_json::Value::Object(map) = &mut v {
                    map.insert("path".into(), path.display().to_string().into());
                }
                v
            })
            .collect();
        let report = serde_json::json!({
            "ok": ok,
            "files": files_json,
            "project_diagnostics": project_json,
        });
        match serde_json::to_string_pretty(&report) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("lute: failed to serialize result: {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        if file_results.is_empty() {
            println!("lute: no .lute files found under {}", dir.display());
        }
        for (path, result) in &file_results {
            print_human(path, result, policy);
        }
        if !project_diags.is_empty() {
            println!("project-wide diagnostics:");
            for (path, d) in &project_diags {
                let denied = policy.denied(d);
                if d.span.line == 0 && d.span.column == 0 {
                    // A right file with no right line (D-Z for manifests, D-AB
                    // for mocks): print no position rather than claiming `0:0`.
                    println!("{}", manifests::spanless_line(path, d, denied));
                    continue;
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

/// LH N18: a stale `luteVersion` a document inherits from its manifest's
/// `defaults:` is one fact about the manifest, not one per document. Every
/// per-file copy is dropped and one warning is reported per root, at the
/// manifest's `luteVersion:` line, counting the documents that inherit it.
fn fold_inherited_version_stale(
    file_results: &mut [(PathBuf, lute_check::CheckResult)],
    project_diags: &mut Vec<(PathBuf, Diagnostic)>,
    inputs: &BTreeMap<PathBuf, (PathBuf, CheckInput)>,
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

/// `check-project`'s compile pass (0.21.1): the checks that only exist once a
/// document has been compiled, or once every document of a root is in hand —
/// run here so `check-project` cannot pass a project `compile --all` and
/// `play` refuse.
///
/// Per document that passed the reconciled check (its own verdict plus every
/// project-wide error anchored on it — the same verdict `compile --all`
/// gates on): a component runs `lute check`'s compile gate
/// ([`compile_gate_diags`]); any other document is compiled
/// (`compile_with_check`) under its root's `identity:` templates, and its
/// compile-stage errors join its result — among them the post-expansion
/// `E-DUP-LINE-CODE` (T1-10). Per root: the single-snapshot gate
/// `build_index` enforces (`E-CAPABILITY-MISMATCH`, T1-11, over every
/// non-component document whether or not it checked clean) and
/// `E-DUP-VOICEKEY` (T1-9, over the compiled artifacts). Both are anchored
/// at the root's `lute.project.yaml` with no position — the manifest owns
/// the profile set and the identity templates that decide them.
pub(crate) fn project_compile_pass(
    file_results: &mut [(PathBuf, lute_check::CheckResult)],
    project_diags: &mut Vec<(PathBuf, Diagnostic)>,
    inputs: &BTreeMap<PathBuf, (PathBuf, CheckInput)>,
) {
    #[derive(Default)]
    struct RootBuild {
        snapshots: Vec<(String, String)>,
        artifacts: Vec<(String, lute_compile::Artifact)>,
    }
    let mut roots: BTreeMap<PathBuf, RootBuild> = BTreeMap::new();
    let mut identities = BTreeMap::new();
    // Every file an `E-` project diagnostic is anchored on: its compile is
    // blocked like a failing per-file check.
    let error_paths: BTreeSet<&PathBuf> = project_diags
        .iter()
        .filter(|(_, d)| d.severity == Severity::Error)
        .map(|(p, _)| p)
        .collect();
    // `(file index, root, rel, component)` of every unblocked document.
    let mut jobs: Vec<(usize, &PathBuf, String, bool)> = Vec::new();
    for (i, (path, result)) in file_results.iter().enumerate() {
        let Some((root, input)) = inputs.get(path) else {
            continue;
        };
        let component = compile_all::is_component_file(path);
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .replace('\\', "/");
        let build = roots.entry(root.clone()).or_default();
        if !component {
            build
                .snapshots
                .push((rel.clone(), input.snapshot.version.clone()));
        }
        if !result.ok || error_paths.contains(path) {
            continue;
        }
        if !component {
            identities.entry(root.clone()).or_insert_with(|| {
                load_project(root)
                    .ok()
                    .flatten()
                    .map(|p| p.identity)
                    .unwrap_or_default()
            });
        }
        jobs.push((i, root, rel, component));
    }

    // Each document compiles independently of every other: in parallel, then
    // applied in file order so artifacts and merged diagnostics are exactly
    // the sequential ones.
    let results: &[(PathBuf, lute_check::CheckResult)] = file_results;
    let outcomes: Vec<Result<lute_compile::Artifact, Vec<Diagnostic>>> = jobs
        .par_iter()
        .map(|&(i, root, _, component)| {
            let (path, result) = &results[i];
            let input = &inputs[path].1;
            if component {
                Err(compile_gate_diags(input))
            } else {
                lute_compile::compile_with_check(input, result.clone(), &identities[root])
            }
        })
        .collect();
    for ((i, root, rel, _), outcome) in jobs.into_iter().zip(outcomes) {
        match outcome {
            Ok(artifact) => roots
                .get_mut(root)
                .expect("every job's root was entered above")
                .artifacts
                .push((rel, artifact)),
            Err(diags) => merge_gate_diags(&mut file_results[i].1, diags),
        }
    }

    for (root, build) in roots {
        let manifest = root.join("lute.project.yaml");
        let anchor = if manifest.is_file() { manifest } else { root };
        // `E-` code => error; spanless, so it prints with no position.
        let project_error = manifests::as_diagnostic;
        let snapshots = build
            .snapshots
            .iter()
            .map(|(doc, version)| (doc.as_str(), version.as_str()));
        for e in lute_compile::index::capability_mismatches(snapshots) {
            project_diags.push((
                anchor.clone(),
                project_error(lute_compile::index::E_CAPABILITY_MISMATCH, e.to_string()),
            ));
        }
        let index_inputs: Vec<lute_compile::index::IndexInput<'_>> = build
            .artifacts
            .iter()
            .map(|(rel, artifact)| lute_compile::index::IndexInput {
                path: rel.clone(),
                artifact_path: String::new(),
                artifact,
            })
            .collect();
        for c in lute_compile::index::voice_key_collisions(&index_inputs) {
            project_diags.push((
                anchor.clone(),
                project_error(lute_compile::index::E_DUP_VOICEKEY, c.to_string()),
            ));
        }
    }
}
