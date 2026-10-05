//! Whole-project compile (the `compile --all` loop, in memory) and the
//! index a play runs over.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_compile::ExecutionIr;
use lute_manifest::schema::OccasionDecl;
use lute_trace::exec::record::NeedleVocab;
use lute_model::{relocate_imported_diags, ModelOptions, ProjectModel};
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

/// The command a project is compiled for, as its messages name it.
#[derive(Clone, Copy)]
pub(crate) struct Gate {
    /// `lute play`, `lute calendar`, `lute test`.
    pub cmd: &'static str,
    /// What the command refuses to do over a project that does not compile.
    pub refuses: &'static str,
}

pub(crate) const PLAY: Gate = Gate {
    cmd: "lute play",
    refuses: "play",
};
pub(crate) const CALENDAR: Gate = Gate {
    cmd: "lute calendar",
    refuses: "evaluate the calendar",
};
pub(crate) const TEST: Gate = Gate {
    cmd: "lute test",
    refuses: "play",
};

/// Compile every non-component document under `project_dir` in memory with
/// the `compile --all` gate — refusing to run `gate`'s command over a
/// project that does not wholly compile — and build its index. `Err`
/// carries the exit code after the diagnostics are printed.
pub(super) fn compile_project(project_dir: &Path, gate: Gate, matrix: &crate::EngineMatrix) -> Result<ExecProject, ExitCode> {
    let project_dir = lute_model::nearest_manifest_dir(project_dir)
        .unwrap_or_else(|| project_dir.to_path_buf());
    manifest_gate(&project_dir, gate.cmd)?;
    let source_model = build_model(&project_dir, gate.cmd)?;
    assemble_project_from_model(&project_dir, gate, matrix, &source_model)
}

/// Refuse a project whose manifests are invalid or whose chapters do not
/// gate, printing the verdicts. `Err` carries the exit code.
pub(crate) fn manifest_gate(project_dir: &Path, cmd: &str) -> Result<(), ExitCode> {
    match crate::manifests::validate_manifests_under(project_dir) {
        Ok(mut verdicts) => {
            crate::manifests::mark_inert_under(&mut verdicts, project_dir);
            if crate::manifests::report_and_gate(&verdicts)
                | crate::manifests::gate_chapters(&verdicts)
            {
                return Err(ExitCode::from(1));
            }
            Ok(())
        }
        Err(e) => {
            let e = lute_manifest::io_reason(&e);
            eprintln!(
                "{cmd}: cannot walk {} for manifests: {e}",
                project_dir.display()
            );
            Err(ExitCode::from(2))
        }
    }
}

/// Build the source model of the project nearest `project_dir` without the
/// manifest gate, for the differential oracle that drives the gates itself.
#[cfg(test)]
pub(crate) fn build_project_model(project_dir: &Path) -> Result<ProjectModel, ExitCode> {
    let project_dir = lute_model::nearest_manifest_dir(project_dir)
        .unwrap_or_else(|| project_dir.to_path_buf());
    build_model(&project_dir, "lute")
}

fn build_model(project_dir: &Path, cmd: &str) -> Result<ProjectModel, ExitCode> {
    ProjectModel::build_single_root(
        project_dir,
        &ModelOptions {
            providers: None,
            permission_profile: None,
            mode: lute_check::Mode::Ci,
            compile: true,
            wip: false,
        },
    )
    .map_err(|error| {
        eprintln!("{cmd}: cannot build {}: {error}", project_dir.display());
        ExitCode::from(1)
    })
}

pub(crate) fn assemble_project_from_model(
    project_dir: &Path,
    gate: Gate,
    matrix: &crate::EngineMatrix,
    source_model: &ProjectModel,
) -> Result<ExecProject, ExitCode> {
    let Gate { cmd, refuses } = gate;
    let policy = crate::DenyPolicy::default();
    if source_model.has_resolution_errors() {
        for document in source_model.documents() {
            for diagnostic in &document.resolve_diags {
                eprintln!(
                    "{cmd}: {} [{}] {}",
                    document.path.display(),
                    diagnostic.code,
                    diagnostic.message
                );
            }
        }
        eprintln!("{cmd}: project resolution failed; refusing to {refuses}");
        return Err(ExitCode::from(1));
    }
    let mut project_diagnostics = source_model.project_diagnostics().to_vec();
    let mut per_doc: Vec<_> = source_model
        .documents()
        .iter()
        .map(|source| (source.path.clone(), source.check.clone()))
        .collect();
    relocate_imported_diags(&mut per_doc, &mut project_diagnostics, &project_dir);
    for (path, diagnostic) in &mut project_diagnostics {
        if path.extension().and_then(|extension| extension.to_str()) == Some("lute") {
            continue;
        }
        let message = diagnostic.message.split(" (imported by ").next().unwrap_or(&diagnostic.message);
        let count = per_doc.len();
        diagnostic.message = format!(
            "{message} (imported by {count} document{})",
            if count == 1 { "" } else { "s" }
        );
    }
    let project_errors: Vec<_> = project_diagnostics
        .iter()
        .filter(|(_, d)| d.severity == lute_core_span::Severity::Error)
        .collect();
    if !project_errors.is_empty() {
        for (path, diagnostic) in &project_errors {
            print!(
                "{}",
                crate::render_diagnostics(path, std::slice::from_ref(diagnostic), &policy)
            );
        }
        eprintln!(
            "{cmd}: {} project diagnostic(s); refusing to {refuses}",
            project_errors.len()
        );
        return Err(ExitCode::from(1));
    }
    let mut occasions: BTreeMap<String, OccasionDecl> = BTreeMap::new();
    let mut compiled: BTreeMap<String, ExecutionIr> = BTreeMap::new();
    let mut failures: BTreeMap<PathBuf, String> = BTreeMap::new();
    let mut world_events: BTreeSet<String> = BTreeSet::new();
    let mut bridge_types = BridgeReads::default();
    let mut display_names: BTreeMap<String, String> = BTreeMap::new();
    let mut needles = NeedleVocab::default();
    let mut chapter_afters: BTreeSet<String> = BTreeSet::new();

    for source in source_model.documents() {
        let file = &source.path;
        if crate::compile_all::is_component_file(file) {
            continue;
        }
        let Some(rel) = project_rel(file, &project_dir) else {
            eprintln!(
                "{cmd}: {} is not under {}",
                file.display(),
                project_dir.display()
            );
            return Err(ExitCode::from(2));
        };
        let input = &source.input;
        for (name, decl) in &input.snapshot.occasions {
            occasions.entry(name.clone()).or_insert_with(|| decl.clone());
        }
        world_events.extend(input.snapshot.events.keys().cloned());
        for (id, m) in lute_check::cast::declared_cast(&input.snapshot, &input.imports, &[]) {
            if let Some(name) = m.name {
                display_names.entry(id).or_insert(name);
            }
        }
        bridge_types = bridge_types.with_result_types(&input.snapshot);
        needles.union(NeedleVocab::of(input, &source.folded.typed));
        let (mut desugared, _) = lute_syntax::parse(&input.text);
        lute_check::chapters::apply_chapters(
            &mut desugared,
            &input.defaults,
            &input.snapshot.occasions,
        );
        chapter_afters.extend(lute_check::chapters::derived_after(
            &desugared,
            &source.folded.typed,
        ));
        if !source.check.ok {
            failures.insert(
                file.clone(),
                crate::render_diagnostics(file, &source.check.diagnostics, &policy),
            );
            continue;
        }
        let Some(artifact) = source.artifact.clone() else {
            continue;
        };
        if let Err(e) = matrix.negotiate(&serde_json::to_value(&artifact).unwrap_or_default()) {
            eprintln!("{cmd}: {e}");
            return Err(ExitCode::from(2));
        }
        compiled.insert(rel, artifact);
    }

    if !failures.is_empty() {
        for rendered in failures.values() {
            print!("{rendered}");
        }
        eprintln!(
            "{cmd}: {} of {} document(s) failed to compile; refusing to {refuses}",
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
    project.chapter_afters = chapter_afters;
    Ok(project)
}

/// Compile the project `gate`'s command runs over ([`compile_project`]);
/// `dir` must be a directory (exit 2 with the usage error).
pub(super) fn compile_play_project(
    dir: &Path,
    gate: Gate,
    matrix: &crate::EngineMatrix,
) -> Result<ExecProject, (ExitCode, String)> {
    if !dir.is_dir() {
        return Err((
            ExitCode::from(2),
            format!("{} is not a project directory", dir.display()),
        ));
    }
    compile_project(dir, gate, matrix).map_err(|code| (code, String::new()))
}
