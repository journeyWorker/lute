//! Project collection: the deterministic `.lute` walk, per-file project
//! root resolution, and the per-root document grouping every project-wide
//! pass (whole-project `check`, `scenario`, `beats`, `lore`, …) builds on.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::envelope::{self, CliError};
use lute_core_span::{Diagnostic, Span, TextIndex};
/// (`PathBuf`'s `Ord` is byte-lexicographic) for deterministic output
/// regardless of the OS's directory-iteration order. Symlinked directories
/// are not followed (`read_dir`'s default — avoids an infinite walk on a
/// cyclic symlink). Any I/O error walking `dir` or a subdirectory is
/// surfaced to the caller rather than silently dropped — a project-wide
/// check must not silently under-report because one subdirectory failed to
/// list.
///
/// A symlinked FILE (unlike a symlinked directory) IS picked up by the walk
/// above — `DirEntry::file_type` reports the link's own type, not its
/// target's, so it never matches `is_dir()`, but its `entry.path()` still
/// ends in `.lute`. Left alone, a symlink alias and its target are the SAME
/// physical document reachable under two DISTINCT `PathBuf`s, which would
/// make `check_project_quest_ids` see every `<quest id>` in that document
/// TWICE and report a false cross-file `E-QUEST-ID-DUP` (0.2.1 review F2).
/// So every discovered path is canonicalized and deduped by that canonical
/// identity, keeping exactly one — the byte-sorted-FIRST — display path per
/// physical document (sorting first so the choice is deterministic and,
/// among an original file and its alias, prefers whichever path string sorts
/// first rather than depending on directory-iteration order). A canonicalize
/// failure (e.g. a dangling symlink) is surfaced exactly like every other
/// walk I/O error above, never silently skipped or panicked on.
pub(crate) use lute_load::find_lute_files;
use lute_load::nearest_manifest_dir;

/// One resolved project root's docs, each paired with its parsed
/// `Document` and `fold_env`'s `FoldedEnv` — the per-root unit
/// whole-project `check` and `lute scenario` (T14) both group by.
pub(crate) use lute_model::{ByRoot, DocGroup};

pub(crate) fn model_cli_error_with_message(
    dir: &Path,
    error: &lute_model::ModelError,
    display_message: String,
) -> CliError {
    let detail = error.to_string();
    match error {
        lute_model::ModelError::Io(_) => CliError::io(dir, display_message),
        lute_model::ModelError::Input(_) => {
            // A document the model could not read is an I/O failure on that
            // document; any other input failure is the input's fault.
            let unreadable = detail
                .strip_prefix("lute: cannot read ")
                .and_then(|rest| rest.rsplit_once(": ").map(|(path, _)| PathBuf::from(path)));
            let missing = detail
                .strip_prefix("missing pre-splice compile document for ")
                .map(PathBuf::from);
            match (unreadable, missing) {
                (Some(path), _) => CliError::io(&path, display_message),
                (None, Some(path)) => CliError::input(&path, display_message),
                (None, None) => CliError::input_without_path(display_message),
            }
        }
        lute_model::ModelError::Resolve(_) => CliError::input_without_path(display_message),
        lute_model::ModelError::Compile { path, .. } => CliError::input(path, display_message),
        lute_model::ModelError::Index(_) => CliError::input_without_path(display_message),
    }
}

/// Walk `dir` for `.lute` files ([`find_lute_files`]), `check()` +
/// `fold_env` each one, and group the parsed docs by resolved project root
/// — the shared file-collection step whole-project `check`, `lute scenario`
/// (T14), and the compile/trace project-aware gate (connectivity spec §5)
/// all build on top of, so they can never observe a DIFFERENT project
/// structure for the same `dir` (never a second file-walk/parse).
///
/// `single_root` picks the root-resolution rule (connectivity spec §5's
/// single-root vs nested distinction): `false` resolves EACH file's OWN
/// nearest ancestor root ([`project_root_for`], whole-project `check`/`lute
/// scenario`'s nested-subproject behavior); `true` treats `dir` itself as
/// THE single root for every file (capabilities AND connectivity resolve
/// against exactly `dir`, no nested nearest-root search — the compile/trace
/// `--project <dir>` gate).
///
/// `Err(ExitCode::from(2))` on the same I/O failures `run_check_project`
/// always had: the walk itself failing, or `build_input` unable to read a
/// file; `Err(ExitCode::from(1))` on a capability-resolution error (under
/// per-file roots), printed.
pub(crate) fn collect_project_docs(
    dir: &Path,
    providers: Option<&Path>,
    permission_profile: Option<&str>,
    single_root: bool,
) -> Result<(Vec<(PathBuf, lute_check::CheckResult)>, ByRoot), ExitCode> {
    collect_project_docs_in(
        &lute_model::ModelMemo::default(),
        dir,
        providers,
        permission_profile,
        single_root,
    )
}

/// Each project root under `dir` with its analysis model (`compile: false`),
/// built through `memo` — what [`collect_project_docs`] reads when it
/// resolves each file's own root. A command that also reads a model's
/// reconciled outputs (`lute scenario reach --endings`) takes them from
/// here instead of recomputing them. `Err` is the first build failure,
/// reported (exit `2`).
pub(crate) fn project_models_in(
    memo: &lute_model::ModelMemo,
    dir: &Path,
    providers: Option<&Path>,
    permission_profile: Option<&str>,
) -> Result<RootModels, ExitCode> {
    let opts = lute_model::ModelOptions {
        providers: providers.map(Path::to_path_buf),
        permission_profile: permission_profile.map(str::to_owned),
        mode: lute_check::Mode::Ci,
        compile: false,
        wip: false,
    };
    memo.roots_under(dir, &opts).map_err(|error| {
        let message = format!(
            "lute: cannot build project under {}: {error}",
            dir.display()
        );
        let envelope_error = model_cli_error_with_message(dir, &error, message.clone());
        eprintln!("{message}");
        envelope::fail_quietly(envelope_error)
    })
}

/// Each project root with its model, as [`project_models_in`] returns them.
pub(crate) type RootModels = Vec<(PathBuf, std::sync::Arc<lute_model::ProjectModel>)>;

/// The per-document analysis results and the per-root document groups of
/// `models`, in root and document order.
pub(crate) fn collected_docs(
    models: &RootModels,
) -> (Vec<(PathBuf, lute_check::CheckResult)>, ByRoot) {
    let file_results = models
        .iter()
        .flat_map(|(_, model)| model.documents())
        .map(|doc| (doc.path.clone(), doc.analysis_check().clone()))
        .collect();
    (file_results, root_groups(models))
}

/// The per-root document groups of `models`, in root and document order.
pub(crate) fn root_groups(models: &RootModels) -> ByRoot {
    models
        .iter()
        .map(|(root, model)| {
            let group = model
                .documents()
                .iter()
                .map(|doc| (doc.path.clone(), doc.doc.clone(), doc.folded.clone()))
                .collect();
            (root.clone(), group)
        })
        .collect()
}

/// The scenario `root`'s model assembled: the one assembly per root a
/// command reads (spec 0.53.0 §3.10). `None` when `models` has no `root`.
pub(crate) fn model_scenario<'m>(
    models: &'m RootModels,
    root: &Path,
) -> Option<&'m lute_model::RootScenario> {
    models
        .iter()
        .find(|(model_root, _)| model_root == root)
        .and_then(|(_, model)| model.reconciled().scenarios.get(model.root()))
}

/// [`collect_project_docs`] over `memo`'s models.
pub(crate) fn collect_project_docs_in(
    memo: &lute_model::ModelMemo,
    dir: &Path,
    providers: Option<&Path>,
    permission_profile: Option<&str>,
    single_root: bool,
) -> Result<(Vec<(PathBuf, lute_check::CheckResult)>, ByRoot), ExitCode> {
    if !single_root {
        return project_models_in(memo, dir, providers, permission_profile)
            .map(|models| collected_docs(&models));
    }
    let (file_results, by_root, _, resolve_errors) =
        collect_project_inputs_in(memo, dir, providers, permission_profile, single_root, false)?;
    if resolve_errors > 0 && !single_root {
        return Err(ExitCode::from(1));
    }
    Ok((file_results, by_root))
}

/// [`collect_project_docs`], also handing back each file's `(root, input)` —
/// aligned with the returned results — for whole-project `check`'s compile pass
/// ([`project_compile_pass`]), and the number of capability-resolution
/// errors printed that did not stop the walk (a name declared twice keeps
/// its first declaration, so the documents are still checked); the caller
/// fails on them.
///
/// [`project_compile_pass`]: crate::cmd_check_project::project_compile_pass
#[expect(
    clippy::type_complexity,
    reason = "the callers destructure the four-part result by position; it is the tuple `collect_project_inputs_in` returns"
)]
pub(crate) fn collect_project_inputs(
    dir: &Path,
    providers: Option<&Path>,
    permission_profile: Option<&str>,
    single_root: bool,
    wip: bool,
) -> Result<
    (
        Vec<(PathBuf, lute_check::CheckResult)>,
        ByRoot,
        Vec<std::sync::Arc<lute_model::ProjectModel>>,
        usize,
    ),
    ExitCode,
> {
    collect_project_inputs_in(
        &lute_model::ModelMemo::default(),
        dir,
        providers,
        permission_profile,
        single_root,
        wip,
    )
}

/// [`collect_project_inputs`] over `memo`'s models.
#[expect(
    clippy::type_complexity,
    reason = "the callers destructure the four-part result by position; an alias would name a tuple that exists only as this return"
)]
pub(crate) fn collect_project_inputs_in(
    memo: &lute_model::ModelMemo,
    dir: &Path,
    providers: Option<&Path>,
    permission_profile: Option<&str>,
    single_root: bool,
    wip: bool,
) -> Result<
    (
        Vec<(PathBuf, lute_check::CheckResult)>,
        ByRoot,
        Vec<std::sync::Arc<lute_model::ProjectModel>>,
        usize,
    ),
    ExitCode,
> {
    let models = compiled_models_in(memo, dir, providers, permission_profile, single_root, wip)?;
    report_project_diags(&models);
    let mut file_results = Vec::new();
    let mut by_root = BTreeMap::new();
    for (root, model) in &models {
        for doc in model.documents() {
            file_results.push((doc.path.clone(), doc.check.clone()));
            by_root.entry(root.clone()).or_insert_with(Vec::new).push((
                doc.path.clone(),
                doc.doc.clone(),
                doc.folded.clone(),
            ));
        }
    }
    let mut resolve_messages = BTreeSet::new();
    for (_, model) in &models {
        for doc in model.documents() {
            for message in &doc.project_diags {
                if message.starts_with("E-") {
                    resolve_messages.insert(message.clone());
                }
            }
        }
    }
    let resolve_errors = resolve_messages.len();
    let models = models.into_iter().map(|(_, model)| model).collect();
    Ok((file_results, by_root, models, resolve_errors))
}

/// The compiled models [`collect_project_inputs_in`] reads: `dir` as THE
/// single root, or each root under it.
fn compiled_models_in(
    memo: &lute_model::ModelMemo,
    dir: &Path,
    providers: Option<&Path>,
    permission_profile: Option<&str>,
    single_root: bool,
    wip: bool,
) -> Result<RootModels, ExitCode> {
    let opts = lute_model::ModelOptions {
        providers: providers.map(Path::to_path_buf),
        permission_profile: permission_profile.map(str::to_owned),
        mode: lute_check::Mode::Ci,
        compile: true,
        wip,
    };
    if single_root {
        let model = memo.single_root(dir, &opts).map_err(|error| {
            let message = format!("lute: cannot build project {}: {error}", dir.display());
            let envelope_error = model_cli_error_with_message(dir, &error, message.clone());
            eprintln!("{message}");
            envelope::fail_quietly(envelope_error)
        })?;
        Ok(vec![(dir.to_path_buf(), model)])
    } else {
        memo.roots_under(dir, &opts).map_err(|error| {
            let message = format!(
                "lute: cannot build project under {}: {error}",
                dir.display()
            );
            let envelope_error = model_cli_error_with_message(dir, &error, message.clone());
            eprintln!("{message}");
            envelope::fail_quietly(envelope_error)
        })
    }
}

/// Print every distinct project-resolution line `models`' documents carry,
/// in document order — what a gating command emits once per run.
fn report_project_diags(models: &RootModels) {
    let mut reported = BTreeSet::new();
    for doc in models.iter().flat_map(|(_, model)| model.documents()) {
        for message in &doc.project_diags {
            if reported.insert(message) {
                eprintln!("{}", lute_load::project_diag_line(message));
            }
        }
    }
}

/// `root`'s model as [`collect_project_docs_in`] reads it: the compiled
/// single-root model (its project lines printed, as the collection prints
/// them) or the analysis model of the root found under `root`.
fn root_model_in(
    memo: &lute_model::ModelMemo,
    root: &Path,
    providers: Option<&Path>,
    permission_profile: Option<&str>,
    single_root: bool,
) -> Option<std::sync::Arc<lute_model::ProjectModel>> {
    let models = if single_root {
        let models = compiled_models_in(memo, root, providers, permission_profile, true, false);
        let models = models.ok()?;
        report_project_diags(&models);
        models
    } else {
        project_models_in(memo, root, providers, permission_profile).ok()?
    };
    models
        .into_iter()
        .find(|(model_root, _)| model_root == root)
        .map(|(_, model)| model)
}

/// Read and parse each of `files` (under the walk root `dir`) as every
/// pass reads it: desugared against its own project's input (dsl 0.27.0
/// §6 — template beats, the keys `chapters:` derives, the `questTier` default), for
/// the report surfaces that read documents without checking them (`lute
/// refs`, `lute lore`). In `files` order, each with its parse diagnostics.
pub(crate) fn parse_project_docs(
    dir: &Path,
    files: &[PathBuf],
) -> Vec<std::io::Result<(lute_syntax::ast::Document, Vec<Diagnostic>)>> {
    lute_load::parse_project_docs(dir, files)
}

/// Re-derive `span`'s `line`/`column`/`utf16_range` from its byte offsets
/// against `text`, mirroring `lute_check::check`'s own (private)
/// `normalize_spans`/`fix_up` treatment for per-file diagnostics exactly
/// (clamp to text length, snap to char boundaries so `Span::from_bytes`
/// never slices mid-code-point, then recompute via [`TextIndex`]) --
/// project-wide diagnostics (`connectivity.rs`'s `meta_key_span`-anchored
/// `E-CONN-*` family) are assembled OUTSIDE `check()`'s own pipeline, so
/// they never otherwise receive this normalization and print a ZEROED
/// `0:0` line/column despite carrying a correct byte range. Idempotent on
/// an already-normalized span (a quest's parser-produced `id_span`/
/// `after_span`) since it recomputes from the exact same byte offsets
/// against the same source text -- never a regression for those.
pub(crate) fn normalize_span_from_text(text: &str, span: Span) -> Span {
    let len = text.len();
    let mut start = span.byte_start.min(len);
    let mut end = span.byte_end.min(len).max(start);
    while start > 0 && !text.is_char_boundary(start) {
        start -= 1;
    }
    while end < len && !text.is_char_boundary(end) {
        end += 1;
    }
    let idx = TextIndex::new(text);
    Span::from_bytes(&idx, start, end)
}

/// The manifest a file command (`check`, `compile`, `context`, `trace`)
/// resolves `file` against when no `--project` was passed (0.48.0 §2.1):
/// [`nearest_manifest_dir`], announced once per distinct project on stderr.
/// `None` when `--project` was given (it wins) or no manifest sits above
/// `file`. The discovered directory is used exactly as if it had been passed
/// with `--project`.
///
/// One helper so the commands cannot disagree about which project a file
/// belongs to: `lute check` found the nearest manifest since 0.21.1 while
/// `trace` and `compile` did not, so a scene inheriting `defaults.uses`
/// checked clean and then was refused by `trace` with `E-UNDECLARED` and
/// advice to run `lute check` (0.27 prerelease FS-F2).
pub(crate) fn discover_project(file: &Path, project: Option<&Path>) -> Option<PathBuf> {
    if project.is_some() {
        return None;
    }
    let dir = nearest_manifest_dir(file)?;
    static NOTED: std::sync::Mutex<BTreeSet<PathBuf>> = std::sync::Mutex::new(BTreeSet::new());
    let first = NOTED
        .lock()
        .map_or(true, |mut noted| noted.insert(dir.clone()));
    if first {
        let shown = crate::output::cwd_relative(&dir.display().to_string());
        eprintln!(
            "lute: note: using project {} (nearest lute.project.yaml); pass --project to choose another",
            if shown.is_empty() { "." } else { shown.as_str() }
        );
    }
    Some(dir)
}

/// Resolve a command's project using an explicit `--project` first and the
/// nearest manifest otherwise. Explicit paths stay byte-for-byte as supplied;
/// discovery emits the informational note once per distinct project.
pub(crate) fn resolve_project(file: &Path, project: Option<&Path>) -> Option<PathBuf> {
    project
        .map(Path::to_path_buf)
        .or_else(|| discover_project(file, None))
}

/// The nearest ancestor of the current directory (itself included) holding a
/// `lute.project.yaml`, spelled relative to the current directory (`.`,
/// `..`, `../..`) so a directory command prints the same paths as one given
/// that directory. Silent: the stderr note is for file commands.
pub(crate) fn discover_project_from_cwd() -> Option<PathBuf> {
    let mut dir = PathBuf::from(".");
    let depth = std::env::current_dir().ok()?.ancestors().count();
    for _ in 0..depth {
        if dir.join(crate::manifests::MANIFEST_FILE).is_file() {
            return Some(dir);
        }
        dir = if dir == Path::new(".") {
            PathBuf::from("..")
        } else {
            dir.join("..")
        };
    }
    None
}

/// The project a command that needs one works on (0.48.0 §2.1): `--project`
/// as given, else the nearest ancestor of the current directory with a
/// `lute.project.yaml`. With neither, `E-CLI-USAGE` naming `--project`.
pub(crate) fn require_project(command: &str, project: Option<&Path>) -> Result<PathBuf, CliError> {
    if let Some(project) = project {
        return Ok(project.to_path_buf());
    }
    discover_project_from_cwd().ok_or_else(|| {
        let here = std::env::current_dir()
            .map_or_else(|_| ".".to_string(), |dir| dir.display().to_string());
        CliError::usage(format!(
            "lute {command}: no lute.project.yaml at or above {here} — pass the project directory with --project <DIR>"
        ))
        .with_argument("--project")
    })
}

/// Return the canonical identity of an existing path. Callers that need to
/// preserve the old best-effort fallback can use the original path when this
/// returns `None`; keeping the policy in one helper avoids subtly different
/// canonicalization at CLI boundaries.
pub(crate) fn canonical_path(path: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}

/// T1-14: the project's May producer set — whole-project `check`'s own
/// reachability-gated [`lute_check::connectivity::live_assert_relations`] —
/// for the root at `root`, the set `W-TRACE-MOCK-UNPRODUCIBLE` must judge a
/// mocked fact against. `single_root` mirrors [`collect_project_docs`]:
/// `true` for an explicit `--project <dir>` (every file resolves against
/// `dir`), `false` for a discovered manifest (nested subprojects keep their
/// own roots, and `root`'s group is the root project's). `None` when the
/// project cannot be collected (the collection already printed why); the
/// caller then judges the traced document alone, and the note says so.
pub(crate) fn project_assert_relations_in(
    memo: &lute_model::ModelMemo,
    root: &Path,
    single_root: bool,
    providers: Option<&Path>,
    permission_profile: Option<&str>,
) -> Option<BTreeSet<String>> {
    let model = root_model_in(memo, root, providers, permission_profile, single_root)?;
    model_assert_relations(&model)
}

/// The relations `model`'s root can assert (`lute_check::connectivity::live_assert_relations`),
/// judged on the scenario the model assembled: no second assembly (spec
/// 0.53.0 §3.10).
pub(crate) fn model_assert_relations(model: &lute_model::ProjectModel) -> Option<BTreeSet<String>> {
    let scenario = model.reconciled().scenarios.get(model.root())?;
    Some(lute_check::connectivity::live_assert_relations(
        &model.project_docs(),
        &scenario.reach,
        &scenario.ambiguous_quests,
        &scenario.unreachable_quests,
        &scenario.rel_vocab.effect_directives,
    ))
}

/// dsl 0.24.0 T3-15: every `<quest id>` declared under the `--project`
/// root — what `lute trace --project` settles its "existence is unverified"
/// notes against, from `memo`'s model. `None` when the project cannot be
/// collected.
pub(crate) fn project_quest_ids_in(
    memo: &lute_model::ModelMemo,
    root: &Path,
    providers: Option<&Path>,
    permission_profile: Option<&str>,
) -> Option<BTreeSet<String>> {
    let model = root_model_in(memo, root, providers, permission_profile, true)?;
    Some(model_quest_ids(&model))
}

/// Every `<quest id>` `model`'s documents declare.
pub(crate) fn model_quest_ids(model: &lute_model::ProjectModel) -> BTreeSet<String> {
    lute_check::connectivity::quest_id_set(&model.project_docs())
}
