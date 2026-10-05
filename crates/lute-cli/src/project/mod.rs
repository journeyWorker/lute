//! Project collection: the deterministic `.lute` walk, per-file project
//! root resolution, and the per-root document grouping every project-wide
//! pass (`check-project`, `scenario`, `beats`, `lore`, …) builds on.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_core_span::{Diagnostic, Span, TextIndex};
use lute_load::nearest_manifest_dir;
use lute_model::assemble_root_scenario;
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


/// One resolved project root's docs, each paired with its parsed
/// `Document` and `fold_env`'s `FoldedEnv` — the per-root unit
/// `check-project` and `lute scenario` (T14) both group by.
pub(crate) use lute_model::{ByRoot, DocGroup};

/// Walk `dir` for `.lute` files ([`find_lute_files`]), `check()` +
/// `fold_env` each one, and group the parsed docs by resolved project root
/// — the shared file-collection step `check-project`, `lute scenario`
/// (T14), and the compile/trace project-aware gate (connectivity spec §5)
/// all build on top of, so they can never observe a DIFFERENT project
/// structure for the same `dir` (never a second file-walk/parse).
///
/// `single_root` picks the root-resolution rule (connectivity spec §5's
/// single-root vs nested distinction): `false` resolves EACH file's OWN
/// nearest ancestor root ([`project_root_for`], `check-project`/`lute
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
    single_root: bool,
) -> Result<(Vec<(PathBuf, lute_check::CheckResult)>, ByRoot), ExitCode> {
    if !single_root {
        let opts = lute_model::ModelOptions {
            providers: providers.map(Path::to_path_buf),
            permission_profile: None,
            mode: lute_check::Mode::Ci,
            compile: false,
            wip: false,
        };
        let mut file_results = Vec::new();
        let mut by_root = BTreeMap::new();
        for model in lute_model::ProjectModel::roots_under(dir, &opts).map_err(|error| {
            eprintln!("lute: cannot build project under {}: {error}", dir.display());
            ExitCode::from(2)
        })? {
            let root = model.root().to_path_buf();
            let mut group = Vec::new();
            for doc in model.documents() {
                file_results.push((doc.path.clone(), doc.check.clone()));
                group.push((doc.path.clone(), doc.doc.clone(), doc.folded.clone()));
            }
            by_root.insert(root, group);
        }
        return Ok((file_results, by_root));
    }
    let (file_results, by_root, _, resolve_errors) =
        collect_project_inputs(dir, providers, single_root, false)?;
    if resolve_errors > 0 && !single_root {
        return Err(ExitCode::from(1));
    }
    Ok((file_results, by_root))
}

/// [`collect_project_docs`], also handing back each file's `(root, input)` —
/// aligned with the returned results — for `check-project`'s compile pass
/// ([`project_compile_pass`]), and the number of capability-resolution
/// errors printed that did not stop the walk (a name declared twice keeps
/// its first declaration, so the documents are still checked); the caller
/// fails on them.
///
/// [`project_compile_pass`]: crate::cmd_check_project::project_compile_pass
#[allow(clippy::type_complexity)]
pub(crate) fn collect_project_inputs(
    dir: &Path,
    providers: Option<&Path>,
    single_root: bool,
    wip: bool,
) -> Result<
    (
        Vec<(PathBuf, lute_check::CheckResult)>,
        ByRoot,
        Vec<lute_model::ProjectModel>,
        usize,
    ),
    ExitCode,
> {
    let opts = lute_model::ModelOptions {
        providers: providers.map(Path::to_path_buf),
        permission_profile: None,
        mode: lute_check::Mode::Ci,
        compile: true,
        wip,
    };
    let models = if single_root {
        vec![lute_model::ProjectModel::build_single_root(dir, &opts).map_err(|error| {
            eprintln!("lute: cannot build project {}: {error}", dir.display());
            ExitCode::from(2)
        })?]
    } else {
        lute_model::ProjectModel::roots_under(dir, &opts).map_err(|error| {
            eprintln!("lute: cannot build project under {}: {error}", dir.display());
            ExitCode::from(2)
        })?
    };
    let mut file_results = Vec::new();
    let mut by_root = BTreeMap::new();
    let mut reported = BTreeSet::new();
    for model in &models {
        let root = model.root().to_path_buf();
        for doc in model.documents() {
            for message in &doc.project_diags {
                if reported.insert(message.clone()) {
                eprintln!("{}", lute_load::project_diag_line(message));
                }
            }
            file_results.push((doc.path.clone(), doc.check.clone()));
            by_root
                .entry(root.clone())
                .or_insert_with(Vec::new)
                .push((doc.path.clone(), doc.doc.clone(), doc.folded.clone()));
        }
    }
    let mut resolve_messages = BTreeSet::new();
    for model in &models {
        for doc in model.documents() {
            for message in &doc.project_diags {
                if message.starts_with("E-") {
                    resolve_messages.insert(message.clone());
                }
            }
        }
    }
    let resolve_errors = resolve_messages.len();
    Ok((file_results, by_root, models, resolve_errors))
}

/// Read and parse each of `files` (under the walk root `dir`) as every
/// pass reads it: desugared against its own project's input (dsl 0.27.0
/// §6 — template beats, the keys `chapters:` derives, the `questTier` default), for
/// the report surfaces that read documents without checking them (`lute
/// refs`, `lute lore`). In `files` order, each with its parse diagnostics.
#[allow(clippy::type_complexity)]
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


/// The manifest a single-file command (`check`, `trace`, `compile`,
/// `compile-stream`, `context`) resolves `file` against when no `--project`
/// was passed: [`nearest_manifest_dir`], announced once on stderr. `None`
/// when `--project` was given (it wins) or no manifest sits above `file`.
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
    let shown = crate::output::cwd_relative(&dir.display().to_string());
    eprintln!(
        "lute: note: using project {} (nearest lute.project.yaml); pass --project to choose another",
        if shown.is_empty() { "." } else { shown.as_str() }
    );
    Some(dir)
}

/// Resolve a command's project using an explicit `--project` first and the
/// nearest manifest otherwise. Explicit paths stay byte-for-byte as supplied;
/// discovery emits the existing informational note exactly once.
pub(crate) fn resolve_project(file: &Path, project: Option<&Path>) -> Option<PathBuf> {
    project
        .map(Path::to_path_buf)
        .or_else(|| discover_project(file, None))
}

/// Return the canonical identity of an existing path. Callers that need to
/// preserve the old best-effort fallback can use the original path when this
/// returns `None`; keeping the policy in one helper avoids subtly different
/// canonicalization at CLI boundaries.
pub(crate) fn canonical_path(path: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok()
}

/// T1-14: the project's May producer set — `check-project`'s own
/// reachability-gated [`lute_check::connectivity::live_assert_relations`] —
/// for the root at `root`, the set `W-TRACE-MOCK-UNPRODUCIBLE` must judge a
/// mocked fact against. `single_root` mirrors [`collect_project_docs`]:
/// `true` for an explicit `--project <dir>` (every file resolves against
/// `dir`), `false` for a discovered manifest (nested subprojects keep their
/// own roots, and `root`'s group is the root project's). `None` when the
/// project cannot be collected (the collection already printed why); the
/// caller then judges the traced document alone, and the note says so.
pub(crate) fn project_assert_relations(
    root: &Path,
    single_root: bool,
    providers: Option<&Path>,
) -> Option<BTreeSet<String>> {
    let (file_results, by_root) = collect_project_docs(root, providers, single_root).ok()?;
    let group = by_root.get(root)?;
    let scenario = assemble_root_scenario(group, &file_results);
    let docs = lute_model::project_docs(group);
    Some(lute_check::connectivity::live_assert_relations(
        &docs,
        &scenario.reach,
        &scenario.ambiguous_quests,
        &scenario.unreachable_quests,
        &scenario.rel_vocab.effect_directives,
    ))
}

/// dsl 0.24.0 T3-15: every `<quest id>` declared under the `--project`
/// root — what `lute trace --project` settles its "existence is unverified"
/// notes against. `None` when the project cannot be collected.
pub(crate) fn project_quest_ids(root: &Path, providers: Option<&Path>) -> Option<BTreeSet<String>> {
    let (_, by_root) = collect_project_docs(root, providers, true).ok()?;
    let group = by_root.get(root)?;
    let docs = lute_model::project_docs(group);
    Some(lute_check::connectivity::quest_id_set(&docs))
}
