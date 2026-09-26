//! Whole-project compile (the `compile --all` loop, in memory) and the
//! index a play runs over.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_compile::Artifact;
use lute_manifest::schema::OccasionDecl;
use lute_trace::exec::record::NeedleVocab;
use lute_trace::exec::session::ExecProject;
use lute_trace::exec::BridgeReads;

/// `path` relative to `root`, forward-slash joined — the project-relative
/// artifact identity `compile_all.rs`'s private `rel_slash` uses.
fn project_rel(path: &Path, root: &Path) -> Option<String> {
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

/// Compile every non-component document under `project_dir` in memory with
/// the `compile --all` gate — refusing to play a project that does not
/// wholly compile — and build its index. `Err` carries the exit code after
/// the diagnostics are printed.
pub(super) fn compile_project(project_dir: &Path) -> Result<ExecProject, ExitCode> {
    match crate::manifests::validate_manifests_under(project_dir) {
        Ok(mut verdicts) => {
            crate::manifests::mark_inert_under(&mut verdicts, project_dir);
            if crate::manifests::report_and_gate(&verdicts)
                | crate::manifests::gate_sequences(&verdicts)
            {
                return Err(ExitCode::from(1));
            }
        }
        Err(e) => {
            eprintln!(
                "lute play: cannot walk {} for manifests: {e}",
                project_dir.display()
            );
            return Err(ExitCode::from(2));
        }
    }

    let reconciled = crate::reconciled_project_results(project_dir, None)?;
    // Round-5 G-17: a schema or plugin fault every importing document
    // shares is reported once, at its own line, as `check-project` folds it
    // — not once per document that fails to compile over it.
    let faults = reconciled.schema_faults(project_dir);
    if !faults.is_empty() {
        for line in &faults {
            println!("{line}");
        }
        eprintln!(
            "lute play: {} schema or plugin error(s) every importing document shares; refusing \
             to play",
            faults.len()
        );
        return Err(ExitCode::from(1));
    }
    let identity = lute_manifest::project::load_project(project_dir)
        .ok()
        .flatten()
        .map(|p| p.identity)
        .unwrap_or_default();

    let mut compiled: BTreeMap<String, Artifact> = BTreeMap::new();
    let mut occasions: BTreeMap<String, OccasionDecl> = BTreeMap::new();
    let mut failures: BTreeMap<PathBuf, String> = BTreeMap::new();
    let policy = crate::DenyPolicy::default();
    let mut world_events: BTreeSet<String> = BTreeSet::new();
    // dsl 0.26.0 §3.1: the bridge capabilities' `result:` types, per tag.
    let mut bridge_types = BridgeReads::default();
    let cache = crate::InputCache::default();
    let mut display_names: BTreeMap<String, String> = BTreeMap::new();
    let mut needles = NeedleVocab::default();

    for (file, base) in &reconciled.per_doc {
        if crate::compile_all::is_component_file(file) {
            continue;
        }
        let Some(rel) = project_rel(file, project_dir) else {
            eprintln!(
                "lute play: {} is not under {}",
                file.display(),
                project_dir.display()
            );
            return Err(ExitCode::from(2));
        };
        let Some(built) = crate::build_input_with(&cache, file, None, Some(project_dir), None)
        else {
            return Err(ExitCode::from(2));
        };
        built.report_project_diags();
        if built.resolve_error {
            return Err(ExitCode::from(1));
        }
        for (name, decl) in &built.input.snapshot.occasions {
            occasions
                .entry(name.clone())
                .or_insert_with(|| decl.clone());
        }
        world_events.extend(built.input.snapshot.events.keys().cloned());
        for (id, m) in
            lute_check::cast::declared_cast(&built.input.snapshot, &built.input.imports, &[])
        {
            if let Some(name) = m.name {
                display_names.entry(id).or_insert(name);
            }
        }
        bridge_types = bridge_types.with_result_types(&built.input.snapshot);
        needles.union(NeedleVocab::of(&built.input, &built.meta));
        let gate = crate::gate_for_doc(&reconciled, file, base);
        match lute_compile::compile_with_check(&built.input, gate, &identity) {
            Ok(artifact) => {
                compiled.insert(rel, artifact);
            }
            Err(diags) => {
                failures.insert(
                    file.clone(),
                    crate::render_diagnostics(file, &diags, &policy),
                );
            }
        }
    }

    if !failures.is_empty() {
        for rendered in failures.values() {
            print!("{rendered}");
        }
        eprintln!(
            "lute play: {} of {} document(s) failed to compile; refusing to play",
            failures.len(),
            failures.len() + compiled.len()
        );
        return Err(ExitCode::from(1));
    }

    let mut project = ExecProject::assemble(
        &compiled,
        occasions,
        world_events,
        bridge_types,
        display_names,
    )
    .map_err(|(code, lines)| {
        for line in &lines {
            eprintln!("{line}");
        }
        ExitCode::from(code)
    })?;
    project.needles = needles;
    project.sequence_after = lute_manifest::project::load_project(project_dir)
        .ok()
        .flatten()
        .map(|c| lute_check::sequence::derived_afters(&c.defaults, &project.occasions))
        .unwrap_or_default();
    Ok(project)
}

/// Compile the project a play runs over ([`compile_project`]); `dir` must be
/// a directory (exit 2 with the usage error).
pub(super) fn compile_play_project(dir: &Path) -> Result<ExecProject, (ExitCode, String)> {
    if !dir.is_dir() {
        return Err((
            ExitCode::from(2),
            format!("{} is not a project directory", dir.display()),
        ));
    }
    compile_project(dir).map_err(|code| (code, String::new()))
}
