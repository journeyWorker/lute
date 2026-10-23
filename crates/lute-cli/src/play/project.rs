//! Whole-project compile (the whole-project `compile` loop, in memory) and the
//! index a play runs over.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_compile::ExecutionIr;
use lute_load::nearest_manifest_dir;
use lute_model::{relocate_imported_diags, ModelMemo, ModelOptions, ProjectModel};
use lute_runtime::session::ExecProject;
use lute_trace::exec::record::NeedleVocab;

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

/// What a command that compiles the project passes the build: the provider
/// directory, the permission profile and the `--deny`/`--deny-warnings`
/// policy (0.48.0 §2.2). The default is the manifest's own catalog, no
/// profile, no promotion.
#[derive(Clone, Default)]
pub(crate) struct Build<'a> {
    pub providers: Option<&'a Path>,
    pub permission_profile: Option<&'a str>,
    pub policy: crate::DenyPolicy,
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
/// the whole-project `compile` gate — refusing to run `gate`'s command over a
/// project that does not wholly compile — and build its index, over
/// `memo`'s model of it. `Err` carries the exit code after the diagnostics
/// are printed.
pub(super) fn compile_project(
    memo: &ModelMemo,
    project_dir: &Path,
    gate: Gate,
    matrix: &crate::EngineMatrix,
    build: &Build,
) -> Result<(ExecProject, NeedleVocab), ExitCode> {
    compile_bundle(memo, project_dir, gate, matrix, build)?.load()
}

/// [`compile_project`] stopped at the bundle.
fn compile_bundle(
    memo: &ModelMemo,
    project_dir: &Path,
    gate: Gate,
    matrix: &crate::EngineMatrix,
    build: &Build,
) -> Result<PlayBundle, ExitCode> {
    let project_dir =
        nearest_manifest_dir(project_dir).unwrap_or_else(|| project_dir.to_path_buf());
    manifest_gate(&project_dir, gate.cmd)?;
    let source_model = memo
        .single_root(&project_dir, &model_options(build))
        .map_err(|error| {
            eprintln!(
                "{}: cannot build {}: {error}",
                gate.cmd,
                project_dir.display()
            );
            ExitCode::from(1)
        })?;
    assemble_bundle_from_model(&project_dir, gate, matrix, &source_model, build)
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
    let project_dir =
        nearest_manifest_dir(project_dir).unwrap_or_else(|| project_dir.to_path_buf());
    ProjectModel::build_single_root(&project_dir, &model_options(&Build::default())).map_err(
        |error| {
            eprintln!("lute: cannot build {}: {error}", project_dir.display());
            ExitCode::from(1)
        },
    )
}

/// The model a play compiles from: the project as whole-project `compile` sees it.
fn model_options(build: &Build) -> ModelOptions {
    ModelOptions {
        providers: build.providers.map(Path::to_path_buf),
        permission_profile: build.permission_profile.map(str::to_owned),
        mode: lute_check::Mode::Ci,
        compile: true,
        wip: false,
    }
}

/// The project of `source_model`, for the differential oracle.
#[cfg(test)]
pub(crate) fn assemble_project_from_model(
    project_dir: &Path,
    gate: Gate,
    matrix: &crate::EngineMatrix,
    source_model: &ProjectModel,
) -> Result<(ExecProject, NeedleVocab), ExitCode> {
    assemble_bundle_from_model(project_dir, gate, matrix, source_model, &Build::default())?.load()
}

/// [`assemble_project_from_model`] stopped at the bundle.
fn assemble_bundle_from_model(
    project_dir: &Path,
    gate: Gate,
    matrix: &crate::EngineMatrix,
    source_model: &ProjectModel,
    build: &Build,
) -> Result<PlayBundle, ExitCode> {
    let Gate { cmd, refuses } = gate;
    let policy = &build.policy;
    // A selecting memo's model is spelled from the root its command named
    // first (spec 0.49.0 §9.6); paths are shown as `project_dir` spells them.
    let shown = |path: &Path| -> PathBuf {
        path.strip_prefix(source_model.root())
            .map_or_else(|_| path.to_path_buf(), |rest| project_dir.join(rest))
    };
    if source_model.has_resolution_errors() {
        for document in source_model.documents() {
            for diagnostic in &document.resolve_diags {
                eprintln!(
                    "{cmd}: {} [{}] {}",
                    shown(&document.path).display(),
                    diagnostic.code,
                    diagnostic.message
                );
            }
        }
        eprintln!("{cmd}: project resolution failed; refusing to {refuses}");
        return Err(ExitCode::from(1));
    }
    let mut project_diagnostics: Vec<_> = source_model
        .project_diagnostics()
        .iter()
        .map(|(path, diagnostic)| (shown(path), diagnostic.clone()))
        .collect();
    let mut per_doc: Vec<_> = source_model
        .documents()
        .iter()
        .map(|source| (shown(&source.path), source.check.clone()))
        .collect();
    relocate_imported_diags(&mut per_doc, &mut project_diagnostics, project_dir);
    for (path, diagnostic) in &mut project_diagnostics {
        if path.extension().and_then(|extension| extension.to_str()) == Some("lute") {
            continue;
        }
        let message = diagnostic
            .message
            .split(" (imported by ")
            .next()
            .unwrap_or(&diagnostic.message);
        let count = per_doc.len();
        diagnostic.message = format!(
            "{message} (imported by {count} document{})",
            if count == 1 { "" } else { "s" }
        );
    }
    let project_errors: Vec<_> = project_diagnostics
        .iter()
        .filter(|(_, d)| d.severity == lute_core_span::Severity::Error || policy.denied(d))
        .collect();
    if !project_errors.is_empty() {
        for (path, diagnostic) in &project_errors {
            print!(
                "{}",
                crate::render_diagnostics(path, std::slice::from_ref(diagnostic), policy)
            );
        }
        eprintln!(
            "{cmd}: {} project diagnostic(s); refusing to {refuses}",
            project_errors.len()
        );
        return Err(ExitCode::from(1));
    }
    let mut compiled: BTreeMap<String, ExecutionIr> = BTreeMap::new();
    let mut failures: BTreeMap<PathBuf, String> = BTreeMap::new();
    let mut needles = NeedleVocab::default();
    let mut source_maps = CoverageMaps::default();
    for source in source_model.documents() {
        let Some(rel) = project_rel(&shown(&source.path), project_dir) else {
            continue;
        };
        if let Some(map) = &source.source_map {
            source_maps.documents.insert(rel.clone(), map.clone());
        }
        if let Some((name, _)) = crate::cmd_check::component_name_of(&source.path) {
            source_maps.components.insert(name, rel);
        }
    }
    let mut chapter_afters: BTreeSet<String> = BTreeSet::new();

    for source in source_model.documents() {
        let file = &shown(&source.path);
        if crate::compile_all::is_component_file(file) {
            continue;
        }
        let Some(rel) = project_rel(file, project_dir) else {
            eprintln!(
                "{cmd}: {} is not under {}",
                file.display(),
                project_dir.display()
            );
            return Err(ExitCode::from(2));
        };
        let input = &source.input;
        needles.union(lute_trace::exec::record::needle_vocab(
            input,
            &source.folded.typed,
        ));
        let (mut desugared, _) = lute_syntax::parse(&input.text);
        lute_check::chapters::apply_chapters(
            &mut desugared,
            input.defaults(),
            &input.snapshot().occasions,
        );
        chapter_afters.extend(lute_check::chapters::derived_after(
            &desugared,
            &source.folded.typed,
        ));
        if !source.check.ok || policy.any_denied(&source.check.diagnostics) {
            failures.insert(
                file.clone(),
                crate::render_diagnostics(file, &source.check.diagnostics, policy),
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

    let unions = crate::bundle::collect_index_unions(source_model);
    let bundle = crate::bundle::build_bundle(&compiled, &unions).map_err(|(code, lines)| {
        for line in &lines {
            eprintln!("{line}");
        }
        ExitCode::from(code)
    })?;
    let authored = compiled
        .iter()
        .filter_map(|(path, artifact)| {
            let authored = artifact
                .commands
                .iter()
                .filter_map(|command| command.authored())
                .map(|(position, text)| (position.to_string(), text.to_string()))
                .collect::<BTreeMap<_, _>>();
            (!authored.is_empty()).then(|| (path.clone(), authored))
        })
        .collect();
    Ok(PlayBundle {
        bundle,
        authored,
        chapter_afters,
        needles,
        coverage_maps: source_maps,
    })
}

/// Source locations needed to merge play match/guard decisions into coverage.
#[derive(Clone, Debug, Default)]
pub(crate) struct CoverageMaps {
    pub(crate) documents: BTreeMap<String, lute_compile::source_map::SourceMap>,
    pub(crate) components: BTreeMap<String, String>,
}

/// A compiled project as the runtime loads it (spec 0.38.0 §4.1), plus what
/// `lute play` reports beside the runtime's events: authored directive
/// text, chapter-derived `after:` provenance and the transcript needle
/// vocabulary.
pub(crate) struct PlayBundle {
    pub(crate) bundle: lute_runtime::index::Bundle,
    authored: BTreeMap<String, BTreeMap<String, String>>,
    chapter_afters: BTreeSet<String>,
    needles: NeedleVocab,
    coverage_maps: CoverageMaps,
}

impl PlayBundle {
    fn load_inner(self) -> Result<(ExecProject, NeedleVocab, CoverageMaps), ExitCode> {
        let mut project = ExecProject::load(self.bundle).map_err(|(code, lines)| {
            for line in &lines {
                eprintln!("{line}");
            }
            ExitCode::from(code)
        })?;
        project.authored = self.authored;
        project.chapter_afters = self.chapter_afters;
        Ok((project, self.needles, self.coverage_maps))
    }

    /// The project a command walks: the runtime bundle plus report data.
    pub(crate) fn load(self) -> Result<(ExecProject, NeedleVocab), ExitCode> {
        let (project, needles, _) = self.load_inner()?;
        Ok((project, needles))
    }

    /// The project plus source locations used by `lute test --coverage`.
    pub(crate) fn load_with_coverage(
        self,
    ) -> Result<(ExecProject, NeedleVocab, CoverageMaps), ExitCode> {
        self.load_inner()
    }
}

/// Compile the project `gate`'s command runs over ([`compile_project`]);
/// `dir` must be a directory (exit 2 with the usage error).
pub(super) fn compile_play_project(
    memo: &ModelMemo,
    dir: &Path,
    gate: Gate,
    matrix: &crate::EngineMatrix,
    build: &Build,
) -> Result<(ExecProject, NeedleVocab), (ExitCode, String)> {
    compile_play_bundle(memo, dir, gate, matrix, build)?
        .load()
        .map_err(|code| (code, String::new()))
}

/// [`compile_play_project`] stopped at the bundle.
pub(super) fn compile_play_bundle(
    memo: &ModelMemo,
    dir: &Path,
    gate: Gate,
    matrix: &crate::EngineMatrix,
    build: &Build,
) -> Result<PlayBundle, (ExitCode, String)> {
    if !dir.is_dir() {
        return Err((
            ExitCode::from(2),
            format!("{} is not a project directory", dir.display()),
        ));
    }
    compile_bundle(memo, dir, gate, matrix, build).map_err(|code| (code, String::new()))
}
