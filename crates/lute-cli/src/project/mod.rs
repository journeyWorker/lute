//! Project collection: the deterministic `.lute` walk, per-file project
//! root resolution, and the per-root document grouping every project-wide
//! pass (`check-project`, `scenario`, `beats`, `lore`, …) builds on.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use lute_check::{fold_env, CheckInput};
use lute_core_span::{Diagnostic, Span, TextIndex};
use rayon::prelude::*;

use crate::cmd_scenario::assemble_root_scenario;
use crate::input::{assemble_input, read_document, BuiltInput};
use crate::input_cache::InputCache;

pub(crate) mod gate;
pub(crate) mod reconcile;

/// Recursively collect every `*.lute` file under `dir`, sorted byte-wise
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
pub(crate) fn find_lute_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("lute") {
                out.push(path);
            }
        }
    }
    out.sort();

    let mut seen_canonical = BTreeSet::new();
    let mut deduped = Vec::with_capacity(out.len());
    for path in out {
        let canonical = std::fs::canonicalize(&path)?;
        if seen_canonical.insert(canonical) {
            deduped.push(path);
        }
    }
    Ok(deduped)
}

/// Resolve the project root for `file` (found under `walk_root` by
/// [`find_lute_files`]): the NEAREST ancestor directory — starting at
/// `file`'s own parent, walking upward — whose `lute.project.yaml` exists.
/// Bounded below by `walk_root` itself, which is always the LAST directory
/// tested; the walk never ascends above it. Returns `walk_root` unchanged
/// when no ancestor up to and including it has a manifest, preserving
/// today's flat single-project behavior for a `walk_root` with no nested
/// subprojects. Deterministic and total: every path's `Path::parent()`
/// ancestry is finite, so the walk always terminates; the only filesystem
/// interaction is an existence check, never a read.
pub(crate) fn project_root_for(file: &Path, walk_root: &Path) -> PathBuf {
    let mut dir = file.parent().unwrap_or(walk_root);
    loop {
        if dir.join("lute.project.yaml").is_file() {
            return dir.to_path_buf();
        }
        if dir == walk_root {
            return walk_root.to_path_buf();
        }
        dir = match dir.parent() {
            Some(parent) => parent,
            None => return walk_root.to_path_buf(),
        };
    }
}

/// One resolved project root's docs, each paired with its parsed
/// `Document` and `fold_env`'s `FoldedEnv` — the per-root unit
/// `check-project` and `lute scenario` (T14) both group by.
pub(crate) type DocGroup = Vec<(PathBuf, lute_syntax::ast::Document, lute_check::FoldedEnv)>;
pub(crate) type ByRoot = BTreeMap<PathBuf, DocGroup>;

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
/// file.
pub(crate) fn collect_project_docs(
    dir: &Path,
    providers: Option<&Path>,
    single_root: bool,
) -> Result<(Vec<(PathBuf, lute_check::CheckResult)>, ByRoot), ExitCode> {
    collect_project_inputs(dir, providers, single_root)
        .map(|(file_results, by_root, _)| (file_results, by_root))
}

/// [`collect_project_docs`], also handing back each file's `(root, input)` —
/// aligned with the returned results — for `check-project`'s compile pass
/// ([`project_compile_pass`]).
///
/// [`project_compile_pass`]: crate::cmd_check_project::project_compile_pass
#[allow(clippy::type_complexity)]
pub(crate) fn collect_project_inputs(
    dir: &Path,
    providers: Option<&Path>,
    single_root: bool,
) -> Result<
    (
        Vec<(PathBuf, lute_check::CheckResult)>,
        ByRoot,
        Vec<(PathBuf, CheckInput)>,
    ),
    ExitCode,
> {
    let files = find_lute_files(dir).map_err(|e| {
        eprintln!("lute: cannot walk {}: {e}", dir.display());
        ExitCode::from(2)
    })?;

    // One document's contribution, computed independently of every other —
    // so the files are checked in parallel against one shared per-run
    // [`InputCache`], then folded back IN WALK ORDER below: stderr lines,
    // early exits, and every returned vector are exactly the sequential ones.
    struct Checked {
        root: PathBuf,
        built: BuiltInput,
        /// `None` when the resolve-error gate below stops at this file.
        analysis: Option<(
            lute_syntax::ast::Document,
            lute_check::FoldedEnv,
            lute_check::CheckResult,
        )>,
    }
    let cache = InputCache::default();
    let checked: Vec<Result<Checked, String>> = files
        .par_iter()
        .map(|file| {
            let root = if single_root {
                dir.to_path_buf()
            } else {
                project_root_for(file, dir)
            };
            let text = read_document(file)?;
            let (built, parsed) = assemble_input(&cache, file, text, providers, Some(&root), None);
            let analysis = (!built.resolve_error || single_root).then(|| {
                let input = &built.input;
                let mut doc = parsed.0.clone();
                // dsl 0.27.0 §6: template beats are ordinary beats to every pass.
                let _ = lute_check::desugar_document(&mut doc, input);
                // dsl 0.24.0 §4: the project passes (fact Must/may, connectivity)
                // see an effects component's writes where its `::use` performs them.
                lute_check::splice_component_effects(&mut doc, &input.components, &input.snapshot);
                let (folded, _, _) = fold_env(&doc, input);
                let result = lute_check::check_parsed(input, parsed);
                (doc, folded, result)
            });
            Ok(Checked {
                root,
                built,
                analysis,
            })
        })
        .collect();

    let mut file_results: Vec<(PathBuf, lute_check::CheckResult)> = Vec::with_capacity(files.len());
    let mut by_root: ByRoot = BTreeMap::new();
    let mut inputs: Vec<(PathBuf, CheckInput)> = Vec::with_capacity(files.len());
    for (file, checked) in files.iter().zip(checked) {
        let Checked {
            root,
            built,
            analysis,
        } = checked.map_err(|message| {
            eprintln!("{message}");
            ExitCode::from(2)
        })?;
        // Per file, exactly as `build_input` printed them before: this loop
        // resolves each document's own root, so the lines stay one-per-document.
        built.report_project_diags();
        // plugin 0.0.2 §2: an `E-` capability-resolution diagnostic (bad plugin
        // option, missing active plugin, bad identity template) is a
        // build-failing error; it printed above, and it MUST gate or it would
        // pass silently.
        //
        // ONLY under per-file root resolution (`single_root == false`). With
        // `single_root == true` the caller has deliberately forced every file
        // under ONE root to reconcile a DIFFERENT target document's envelope
        // (connectivity spec §5) — a sibling that legitimately belongs to a
        // nested subproject then resolves against the wrong `lute.project.yaml`
        // and reports e.g. `E-PROFILE-UNKNOWN` for a profile its own project
        // does define. That is an artifact of the forced root, not a fault of
        // the document being compiled, and must not fail it. `check-project`
        // (which uses each file's own nearest root) still catches the real ones.
        let Some((doc, folded, result)) = analysis else {
            return Err(ExitCode::from(1));
        };
        by_root
            .entry(root.clone())
            .or_default()
            .push((file.clone(), doc, folded));
        file_results.push((file.clone(), result));
        inputs.push((root, built.input));
    }

    Ok((file_results, by_root, inputs))
}

/// Read and parse each of `files` (under the walk root `dir`) as every
/// pass reads it: desugared against its own project's input (dsl 0.27.0
/// §6/§8 — template beats, `sequence:` keys, the `questTier` default), for
/// the report surfaces that read documents without checking them (`lute
/// refs`, `lute lore`). In `files` order, each with its parse diagnostics.
#[allow(clippy::type_complexity)]
pub(crate) fn parse_project_docs(
    dir: &Path,
    files: &[PathBuf],
) -> Vec<std::io::Result<(lute_syntax::ast::Document, Vec<Diagnostic>)>> {
    let cache = InputCache::default();
    files
        .par_iter()
        .map(|file| {
            let text = std::fs::read_to_string(file)?;
            let root = project_root_for(file, dir);
            let (built, (mut doc, diags)) =
                assemble_input(&cache, file, text, None, Some(&root), None);
            let _ = lute_check::desugar_document(&mut doc, &built.input);
            Ok((doc, diags))
        })
        .collect()
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

/// The directory whose `lute.project.yaml` governs `file`: the nearest
/// ancestor (starting at `file`'s own directory) that has one, or `None`.
/// Unlike [`project_root_for`] this is not bounded by a walk root — a single
/// file or test directory handed to `trace`/`test` still belongs to the
/// project it sits in.
pub(crate) fn nearest_manifest_dir(file: &Path) -> Option<PathBuf> {
    let abs = std::fs::canonicalize(file).ok()?;
    let start = if abs.is_dir() {
        abs.as_path()
    } else {
        abs.parent()?
    };
    start
        .ancestors()
        .find(|d| d.join("lute.project.yaml").is_file())
        .map(Path::to_path_buf)
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
    Some(lute_check::connectivity::live_assert_relations(
        &scenario.docs,
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
    let docs: Vec<(PathBuf, lute_syntax::ast::Document)> = by_root
        .get(root)?
        .iter()
        .map(|(p, d, _)| (p.clone(), d.clone()))
        .collect();
    Some(lute_check::connectivity::quest_id_set(&docs))
}
