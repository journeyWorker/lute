//! `lute` — the headless CLI wrapper around the `check()` core (Phase 5).
//!
//! Two subcommands, both thin shells over library code (arch: "`check()` is the
//! contract, not the LSP protocol" — the CLI adds argument parsing, file I/O, and
//! output formatting, and owns NO validation logic):
//!
//! - `lute check <file> [--json] [--providers <dir>]` — statically validate one
//!   `.lute` document against the built-in `lute.core` snapshot plus an optional
//!   pinned provider catalog. Exit `0` when clean, `1` when any `Error`-severity
//!   diagnostic is present (`CheckResult::ok`), `2` on an I/O failure. `--json`
//!   prints the serialized [`CheckResult`]; otherwise a human line per diagnostic.
//! - `lute check-project <dir> [--json] [--providers <dir>]` — recursively
//!   `check` every `*.lute` file under `<dir>` (deterministic sorted order),
//!   resolving EACH file's project root independently as its nearest
//!   ancestor directory containing a `lute.project.yaml` (bounded below by
//!   `<dir>` itself; falls back to `<dir>` when no ancestor has one) — so a
//!   `<dir>` containing nested subprojects checks each file against ITS OWN
//!   subproject, not the walk root. PLUS project-wide `<quest id>`
//!   uniqueness (dsl 0.2.0 §6.3), scoped PER RESOLVED PROJECT ROOT (two
//!   different subprojects declaring the same id is not a collision), for
//!   quest docs `check`'s own import-graph-scoped `E-QUEST-ID-DUP` (0.2.0
//!   F4) cannot see: two quest docs sharing an id with no `uses:`/`extends:`
//!   edge between them. Exit `0` clean, `1` when any file has an `Error` or
//!   any resolved root's quest-id pass finds a collision, `2` on an I/O
//!   failure. `--json` prints a structured report (per-file `CheckResult`s +
//!   the project-wide diagnostics); otherwise per-file human lines plus a
//!   project-wide section.
//! - `lute catalog refresh <dir>` — re-stamp every pinned provider snapshot in
//!   `<dir>` against the current `capabilityVersion` and clear its `stale` flag,
//!   rewriting each file in the flat on-disk format `ProviderSet::load` reads
//!   (plugin §10; "an explicit `catalog refresh` precedes a build"). Correctness
//!   never depends on a live/remote catalog — refresh only canonicalizes and
//!   re-stamps the already-pinned artifacts, so `refresh` then `load` round-trips.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use lute_check::{
    check, check_project_quest_ids, fold_env, parse_meta, CheckInput, Mode, Namespace, RelVocab,
};
use lute_core_span::{Diagnostic, Severity};
use lute_manifest::core::load_core_snapshot;
use lute_manifest::project::{load_project, resolve_document_snapshot};
use lute_manifest::provider::{ProviderSet, ProviderSnapshot};
use lute_manifest::relations::KindShape;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::{Literal, Type};
use lute_trace::{merge, parse_mock_yaml, MockSet, TraceExit, TraceReport};

#[derive(Parser)]
#[command(
    name = "lute",
    version,
    about = "Checker and compiler for .lute visual-novel scenarios"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Statically validate a `.lute` document.
    Check {
        /// Path to the `.lute` file to check.
        file: PathBuf,
        /// Emit the full `CheckResult` as JSON instead of human-readable lines.
        #[arg(long)]
        json: bool,
        /// Directory of pinned provider snapshots to resolve ids against.
        #[arg(long, value_name = "DIR")]
        providers: Option<PathBuf>,
        /// Project directory (`lute.project.yaml` + `plugins/`) whose installed
        /// plugins resolve the document's activated capability snapshot (plugin
        /// §4/§11). Omit for a core-only (`lute.core`) check.
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
    },
    /// Statically validate EVERY `.lute` document under a directory
    /// (recursively, deterministic sorted order), like `check` on each file,
    /// PLUS project-wide `<quest id>` uniqueness (dsl 0.2.0 §6.3) for quest
    /// docs `check`'s own import-graph-scoped `E-QUEST-ID-DUP` (0.2.0 F4)
    /// cannot see.
    CheckProject {
        /// Directory to walk recursively for `*.lute` files; also the
        /// project root passed to `load_project` (plugin §4/§11), so every
        /// file's capability resolution matches `lute check <file> --project
        /// <dir>`.
        dir: PathBuf,
        /// Emit a structured JSON report instead of human-readable lines.
        #[arg(long)]
        json: bool,
        /// Directory of pinned provider snapshots to resolve ids against.
        #[arg(long, value_name = "DIR")]
        providers: Option<PathBuf>,
    },
    /// Compile a checked `.lute` document to its JSON command-record artifact.
    Compile {
        /// Path to the `.lute` file to compile.
        file: PathBuf,
        /// On a failed gate, print the diagnostics as JSON instead of
        /// human-readable lines. (The artifact itself is always JSON.)
        #[arg(long)]
        json: bool,
        /// Directory of pinned provider snapshots to resolve ids against.
        #[arg(long, value_name = "DIR")]
        providers: Option<PathBuf>,
        /// Project directory (`lute.project.yaml` + `plugins/`) resolving the
        /// document's activated capability snapshot.
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
        /// Write the artifact here instead of stdout.
        #[arg(short = 'o', long = "out", value_name = "FILE")]
        out: Option<PathBuf>,
    },
    /// Back-fill a stable `code` into every untagged `:line` (dsl §12),
    /// rewriting the file in place.
    Tag {
        /// Path to the `.lute` file to tag.
        file: PathBuf,
    },
    /// Migrate a pre-0.2.2 document to 0.2.2 in place — `:line[speaker]{…}:
    /// text` → `@speaker{…}: text`, any other content line's leading `:`
    /// sigil → `@` (dsl §7.1, foundation C1), and `<choice>`/`<hub>` choice
    /// `as="…"` → `into="…"` (dsl §7.3). Byte-exact and comment-preserving;
    /// writes back only when something changed. Exit `0` on success, `2` on
    /// an I/O failure.
    Fix {
        /// Path to the `.lute` file to migrate.
        file: PathBuf,
    },
    /// Emit the project-resolved AUTHORING SURFACE for a `.lute` file — the
    /// directives/attrs/enums/asset-kinds/providers/state-schema/components +
    /// capabilityVersion an AI needs to WRITE valid Lute against THIS file's
    /// project. A capability query, NOT validation: reuses the SAME resolution
    /// (`build_input`/`fold_env`) check/compile use, and emits regardless of
    /// document diagnostics. Exit `0` on success, `2` on an I/O failure.
    Context {
        /// Path to the `.lute` file whose project surface to resolve.
        file: PathBuf,
        /// Emit the machine-readable JSON surface instead of a human outline.
        #[arg(long)]
        json: bool,
        /// Directory of pinned provider snapshots to resolve ids against.
        #[arg(long, value_name = "DIR")]
        providers: Option<PathBuf>,
        /// Project directory (`lute.project.yaml` + `plugins/`) whose installed
        /// plugins resolve the document's activated capability snapshot (plugin
        /// §4/§11). Omit for a core-only (`lute.core`) surface.
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
    },
    /// Preview a `.lute` document's behavior against author-supplied mocks —
    /// the D1-quarantined authoring evaluator (dsl 0.4.0 §4). Resolves the
    /// document identically to `check` (`build_input`), refuses (exit 1) a
    /// document with check errors OR invalid mocks (`E-TRACE-*`, rendered
    /// exactly like check diagnostics — run `check` first), then walks it
    /// once, deterministically, reporting every decision and why. Exit `0`
    /// complete, `1` refused, `2` I/O, `3` incomplete (an `unknown` guard
    /// halted the walk, dsl 0.4.0 §4.4/§4.5).
    Trace {
        /// Path to the `.lute` file to trace.
        file: PathBuf,
        /// A scalar state seed: a DECLARED state path and a literal,
        /// `<path>=<literal>` (repeatable).
        #[arg(long = "state", value_name = "PATH=LITERAL", value_parser = parse_state_flag)]
        state: Vec<(String, String)>,
        /// A ground fact, valid-now, over the declared vocabulary — e.g.
        /// `"inParty(shadowheart)"` (repeatable).
        #[arg(long = "fact", value_name = "REL(ARG…)")]
        fact: Vec<String>,
        /// A menu selection at a `<branch>`/`<hub>` id, in order:
        /// `<branchOrHubId>=<choiceId>[,<choiceId>…]` (repeatable; a hub may
        /// force a whole ordered visit sequence via one flag's comma list).
        #[arg(long = "choose", value_name = "ID=CHOICEID[,CHOICEID…]", value_parser = parse_choose_flag)]
        choose: Vec<(String, Vec<String>)>,
        /// Fire a quest capability/world event, in CLI order (repeatable).
        /// A built-in lifecycle event name (`questActive`/`questComplete`/
        /// `questFailed`) is `E-TRACE-EVENT` — those are engine-derived
        /// transitions, never user-fired (dsl 0.4.0 §4.3/§4.4).
        #[arg(long = "event", value_name = "NAME")]
        event: Vec<String>,
        /// Simulate accepting a `start`-less (accept-driven) quest, by id
        /// (repeatable). An unknown quest id, or one that carries a
        /// `start` predicate (declarative — needs no accept), is
        /// `E-TRACE-ACCEPT` (dsl 0.4.0 §4.3/§4.4).
        #[arg(long = "accept", value_name = "QUESTID")]
        accept: Vec<String>,
        /// A YAML document carrying the same five surfaces (`state:`/
        /// `facts:`/`choose:`/`events:`/`accepts:`, dsl 0.4.0 §4.3); CLI
        /// flags compose with it, the flag winning on a conflict.
        #[arg(long, value_name = "FILE")]
        mock: Option<PathBuf>,
        /// Emit the machine-readable `TraceReport` as JSON instead of the
        /// human transcript.
        #[arg(long)]
        json: bool,
        /// Directory of pinned provider snapshots to resolve ids against.
        #[arg(long, value_name = "DIR")]
        providers: Option<PathBuf>,
        /// Project directory (`lute.project.yaml` + `plugins/`) whose
        /// installed plugins resolve the document's activated capability
        /// snapshot (plugin §4/§11). Omit for a core-only (`lute.core`) trace.
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
    },
    /// Provider-catalog maintenance.
    #[command(subcommand)]
    Catalog(CatalogCommand),
}

#[derive(Subcommand)]
enum CatalogCommand {
    /// Re-stamp and rewrite the pinned provider snapshots in a directory.
    Refresh {
        /// Directory holding the flat per-snapshot YAML files.
        dir: PathBuf,
        /// Project directory (`lute.project.yaml` + `plugins/`) whose resolved
        /// multi-plugin `capabilityVersion` stamps each snapshot instead of the
        /// core-only version (plugin §10/§13). Omit for the core baseline.
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
    },
}

/// Parse a `--state <path>=<literal>` flag into `(path, literal)` — a plain
/// clap `value_parser`, so a malformed flag (no `=`) is rejected by clap
/// ITSELF as a usage error (exit `2`, matching the `2` = "I/O/usage" tier of
/// the trace exit-code contract) before `run_trace` ever runs.
fn parse_state_flag(raw: &str) -> Result<(String, String), String> {
    raw.split_once('=')
        .map(|(path, literal)| (path.to_string(), literal.to_string()))
        .ok_or_else(|| format!("`--state` must be `<path>=<literal>`, got `{raw}`"))
}

/// Parse a `--choose <branchOrHubId>=<choiceId>[,<choiceId>…]` flag into
/// `(id, choice ids)` — a hub's comma list forces its whole ordered visit
/// sequence (dsl 0.4.0 §4.3/§4.4). Same clap-level rejection as
/// [`parse_state_flag`] for a malformed flag.
fn parse_choose_flag(raw: &str) -> Result<(String, Vec<String>), String> {
    let (id, rest) = raw.split_once('=').ok_or_else(|| {
        format!("`--choose` must be `<id>=<choiceId>[,<choiceId>...]`, got `{raw}`")
    })?;
    let choices: Vec<String> = rest.split(',').map(str::to_string).collect();
    if id.is_empty() || choices.iter().any(|c| c.is_empty()) {
        return Err(format!(
            "`--choose` must be `<id>=<choiceId>[,<choiceId>...]`, got `{raw}`"
        ));
    }
    Ok((id.to_string(), choices))
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Check {
            file,
            json,
            providers,
            project,
        } => run_check(&file, json, providers.as_deref(), project.as_deref()),
        Command::CheckProject {
            dir,
            json,
            providers,
        } => run_check_project(&dir, json, providers.as_deref()),
        Command::Compile {
            file,
            json,
            providers,
            project,
            out,
        } => run_compile(
            &file,
            json,
            providers.as_deref(),
            project.as_deref(),
            out.as_deref(),
        ),
        Command::Context {
            file,
            json,
            providers,
            project,
        } => run_context(&file, json, providers.as_deref(), project.as_deref()),
        Command::Trace {
            file,
            state,
            fact,
            choose,
            event,
            accept,
            mock,
            json,
            providers,
            project,
        } => run_trace(
            &file,
            state,
            fact,
            choose,
            event,
            accept,
            mock.as_deref(),
            json,
            providers.as_deref(),
            project.as_deref(),
        ),
        Command::Tag { file } => run_tag(&file),
        Command::Fix { file } => run_fix(&file),
        Command::Catalog(CatalogCommand::Refresh { dir, project }) => {
            run_refresh(&dir, project.as_deref())
        }
    }
}

/// Assemble the `CheckInput` for `file` exactly as `check` does: project
/// snapshot resolution (plugin §4/§11), provider-catalog precedence (plugin
/// §10), and `uses:`/`components:` imports resolved against the file's own
/// directory. `None` => the file could not be read (caller exits 2).
fn build_input(
    file: &Path,
    providers: Option<&Path>,
    project: Option<&Path>,
) -> Option<CheckInput> {
    let text = match std::fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("lute: cannot read {}: {e}", file.display());
            return None;
        }
    };

    // Resolve the capability snapshot the document is validated against. With
    // `--project`, load the project and assemble the scene's activated snapshot
    // (plugin §4/§11); without it, `resolve_document_snapshot(None, ..)` returns
    // the core-only `lute.core` baseline — behavior identical to before.
    let project = match project {
        Some(dir) => match load_project(dir) {
            Ok(p) => p,
            Err(e) => {
                // A malformed project must not silently mis-validate: surface it
                // and fall back to core-only rather than pretending it loaded.
                eprintln!("lute: {e}");
                None
            }
        },
        None => None,
    };

    // Provider catalog precedence (plugin §10): an explicit `--providers <dir>`
    // wins; otherwise auto-discover the project's pinned catalog through the
    // SAME shared helper the LSP uses, so the two surfaces resolve the same ids
    // for the same project; with neither, an empty set.
    let providers = match providers {
        Some(dir) => ProviderSet::load(dir),
        None => lute_manifest::project::project_providers(project.as_ref()),
    };

    // Lift the scene's frontmatter `profile`/`plugins` — both built-in keys, so a
    // default snapshot suffices to type them (they are not capability-gated).
    let (doc, _) = lute_syntax::parse(&text);
    let (meta0, _) = parse_meta(&doc.meta, &CapabilitySnapshot::default());

    let (snapshot, rdiags) =
        resolve_document_snapshot(project.as_ref(), meta0.profile.as_deref(), &meta0.plugins);
    for d in &rdiags {
        eprintln!("lute: {}: {}", d.code, d.message);
    }

    // Resolve the scene's `uses:` schema imports (dsl §9.2) and `components:`
    // component imports (dsl §13) relative to the scene's own directory; the LSP
    // resolves identically -> no divergence.
    let base = file.parent().unwrap_or_else(|| Path::new("."));
    let imports = lute_check::resolve_imports(base, &meta0.uses, &meta0.extends, doc.meta.span);
    let components = lute_check::resolve_components(base, &meta0.components, doc.meta.span);

    Some(CheckInput {
        text,
        uri: file.display().to_string(),
        snapshot,
        providers,
        // Batch/build analysis, not the interactive LSP default (both behave
        // identically today; the checker does not branch on mode yet).
        mode: Mode::Ci,
        imports,
        components,
    })
}

/// Run `check` over one file and print its result. Exit `0` clean / `1` on an
/// error diagnostic / `2` on an I/O failure.
fn run_check(
    file: &Path,
    json: bool,
    providers: Option<&Path>,
    project: Option<&Path>,
) -> ExitCode {
    let Some(input) = build_input(file, providers, project) else {
        return ExitCode::from(2);
    };
    let result = check(&input);

    if json {
        // Serialization is infallible for this concrete, primitive-only shape.
        match serde_json::to_string_pretty(&result) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("lute: failed to serialize result: {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        print_human(file, &result);
    }

    if result.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

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
fn find_lute_files(dir: &Path) -> std::io::Result<Vec<PathBuf>> {
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
fn project_root_for(file: &Path, walk_root: &Path) -> PathBuf {
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

/// Run `check` over every `*.lute` file recursively found under `dir`
/// (sorted, deterministic, symlink-deduped — [`find_lute_files`]), but
/// resolving EACH file's project independently rather than reusing `dir` as
/// one flat project for every file: [`project_root_for`] walks from the
/// file's own directory upward for the NEAREST ancestor containing a
/// `lute.project.yaml`, bounded below by `dir` itself (falls back to `dir`
/// when no ancestor up to and including it has one — identical to the old
/// flat-project behavior). `build_input` is called with THAT resolved root,
/// so a file under a nested subproject (its own `lute.project.yaml` +
/// `plugins/` + `catalog/`) resolves against ITS OWN capability snapshot and
/// pinned catalog, matching `lute check <file> --project <that subproject>`
/// exactly — never the walk root's, when a nearer root exists.
///
/// THEN additionally cross-validates `<quest id>` uniqueness (dsl 0.2.0
/// §6.3, [`lute_check::check_project_quest_ids`]) — the residual `check()`'s
/// own `E-QUEST-ID-DUP` (0.2.0 F4, scoped to one document's OWN
/// `uses:`/`extends:` import graph) cannot see: two quest docs sharing an id
/// with no import edge between them at all. This pass is scoped PER
/// RESOLVED PROJECT ROOT, not pooled across the whole walked tree: the
/// walked docs are grouped by their resolved root (preserving each group's
/// relative file order) and both quest-id passes run independently within
/// each group, so the same id declared in two DIFFERENT subprojects is never
/// flagged as a collision — only a repeat WITHIN one resolved root is.
///
/// Neither surface is the sole authority: `check_project_quest_ids` only
/// ever sees the files THIS walk found within the SAME resolved-root group,
/// so it cannot re-derive an import-graph collision `check()` catches whose
/// OTHER party lives outside `dir` (0.2.1 review F1 — blanket-stripping
/// every per-file `E-QUEST-ID-DUP` and trusting the project pass alone
/// silently swallowed that case). So every per-file diagnostic is KEPT by
/// default; only a per-file `E-QUEST-ID-DUP` whose exact `(file, span)` is a
/// MEMBER of a colliding group WITHIN ITS OWN resolved root
/// ([`lute_check::colliding_occurrences`], run per group — every occurrence
/// of an id declared 2+ times among that group's docs, not just the ones
/// `check_project_quest_ids` itself emits a diagnostic for) is suppressed,
/// since that whole group is already covered by exactly one
/// `check_project_quest_ids` diagnostic. Membership, not anchor equality, is
/// the right test: the SAME real collision can anchor its per-file
/// diagnostic (fired wherever `check()`'s import resolution detected the
/// redeclare) on a different file than the one `check_project_quest_ids`
/// picks (it always skips the group's path-sorted-first occurrence) — see
/// [`lute_check::colliding_occurrences`]'s own doc comment.
///
/// Exit `0` clean, `1` when any file has a (post-suppression) `Error`
/// diagnostic or any resolved root's quest-id pass finds a collision, `2` on
/// an I/O failure walking `dir` or reading a file.
fn run_check_project(dir: &Path, json: bool, providers: Option<&Path>) -> ExitCode {
    let files = match find_lute_files(dir) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("lute: cannot walk {}: {e}", dir.display());
            return ExitCode::from(2);
        }
    };

    let mut file_results: Vec<(PathBuf, lute_check::CheckResult)> =
        Vec::with_capacity(files.len());
    let mut docs: Vec<(PathBuf, lute_syntax::ast::Document)> = Vec::with_capacity(files.len());
    // The resolved project root for `files[i]`/`docs[i]` (same index order)
    // — [`project_root_for`]'s nearest-ancestor `lute.project.yaml` lookup,
    // bounded below by the walk root `dir` itself.
    let mut roots: Vec<PathBuf> = Vec::with_capacity(files.len());

    for file in &files {
        let root = project_root_for(file, dir);
        let Some(input) = build_input(file, providers, Some(&root)) else {
            return ExitCode::from(2);
        };
        let (doc, _) = lute_syntax::parse(&input.text);
        docs.push((file.clone(), doc));

        let result = check(&input);
        file_results.push((file.clone(), result));
        roots.push(root);
    }

    // Scope project-wide <quest id> uniqueness (dsl 0.2.0 §6.3) PER RESOLVED
    // PROJECT ROOT rather than pooling across the whole walked tree: two
    // different subprojects each declaring the same id is not a collision,
    // only a repeat WITHIN one resolved root is. Group `docs` by `roots[i]`
    // (same index order), preserving each group's relative file order, then
    // run both quest-id passes independently per group and union the
    // results below.
    let mut by_root: BTreeMap<PathBuf, Vec<(PathBuf, lute_syntax::ast::Document)>> =
        BTreeMap::new();
    for (idx, entry) in docs.iter().enumerate() {
        by_root.entry(roots[idx].clone()).or_default().push(entry.clone());
    }

    let mut project_diags = Vec::new();
    // Every occurrence within its own resolved root already covers (see the
    // fn doc comment above) — used below to suppress ONLY the per-file
    // `E-QUEST-ID-DUP`s that pass demonstrably re-reports, never the ones it
    // structurally cannot see (an import-graph collision reaching outside
    // `dir`, or a same-id declare in a SIBLING project root).
    let mut covered = Vec::new();
    for group in by_root.values() {
        project_diags.extend(check_project_quest_ids(group));
        covered.extend(lute_check::colliding_occurrences(group));
    }
    for (path, result) in &mut file_results {
        result.diagnostics.retain(|d| {
            d.code != "E-QUEST-ID-DUP"
                || !covered.iter().any(|(p, s)| p == path && *s == d.span)
        });
        result.ok = !result
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error);
    }

    let project_ok = !project_diags
        .iter()
        .any(|(_, d)| d.severity == Severity::Error);
    let ok = project_ok && file_results.iter().all(|(_, r)| r.ok);

    if json {
        // Reuse each type's own `Serialize` impl (`CheckResult`/`Diagnostic`,
        // both defined — and derived — in lute-check/lute-core-span) and
        // merge in the file path as a sibling key, rather than declaring a
        // new wrapper type (would need `serde`'s derive macro as a direct
        // dependency this crate doesn't otherwise need).
        let files_json: Vec<serde_json::Value> = file_results
            .iter()
            .map(|(path, result)| {
                let mut v =
                    serde_json::to_value(result).unwrap_or_else(|_| serde_json::json!({}));
                if let serde_json::Value::Object(map) = &mut v {
                    map.insert("path".into(), path.display().to_string().into());
                }
                v
            })
            .collect();
        let project_json: Vec<serde_json::Value> = project_diags
            .iter()
            .map(|(path, d)| {
                let mut v = serde_json::to_value(d).unwrap_or_else(|_| serde_json::json!({}));
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
        if files.is_empty() {
            println!("lute: no .lute files found under {}", dir.display());
        }
        for (path, result) in &file_results {
            print_human(path, result);
        }
        if !project_diags.is_empty() {
            println!("project-wide quest-id collisions:");
            for (path, d) in &project_diags {
                println!(
                    "{}:{}:{}: {} [{}] {}",
                    path.display(),
                    d.span.line,
                    d.span.column,
                    severity_str(d.severity),
                    d.code,
                    d.message,
                );
            }
        }
        if ok {
            println!(
                "ok: {} ({} file(s), 0 project-wide collision(s))",
                dir.display(),
                file_results.len()
            );
        } else {
            println!(
                "failed: {} ({} file(s), {} project-wide collision(s))",
                dir.display(),
                file_results.len(),
                project_diags.len()
            );
        }
    }

    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Emit the project-resolved AUTHORING SURFACE for `file`: everything an AI
/// needs to WRITE valid Lute against THIS file's project — the resolved
/// directives/attrs/enums/asset-kinds/providers, the FOLDED state schema (author
/// `state:` ∪ `uses:` imports ∪ implicit `<branch>`/`<hub>` choice+visited slots
/// ∪ plugin-declared slots), the imported components, and the `capabilityVersion`
/// they were resolved under.
///
/// Reuses the SAME resolution `check`/`compile` use — `build_input` (project +
/// provider + import resolution) and `fold_env` (the folded schema) — so the
/// surface never diverges from what the checker validates against. It is a
/// capability QUERY, not validation: it emits the surface regardless of any
/// document diagnostics (`fold_env` is pure/total). Exit `0` on success, `2` on
/// an I/O failure (unreadable file), matching `run_check`.
fn run_context(
    file: &Path,
    json: bool,
    providers: Option<&Path>,
    project: Option<&Path>,
) -> ExitCode {
    let Some(input) = build_input(file, providers, project) else {
        return ExitCode::from(2);
    };
    // Parse + fold exactly as `compile` does (minus codegen): the folded env's
    // `.state` is the document's valid readable/writable state surface. No CEL
    // fill is needed — the schema fold reads structural ids/attrs, not CEL slots.
    let (doc, _) = lute_syntax::parse(&input.text);
    let (folded, _, _) = fold_env(&doc, &input);
    // The ACTUAL implicit choice slots (`scene.choices.<branchId|hubId>`): reuse
    // compile's own discriminator so the surface's enum domains match the compiled
    // state table byte-for-byte (choice ids ∪ `unset`) — no divergence. The set is
    // expansion-invariant, so the raw parsed `doc` yields the same paths.
    let branch_paths = lute_compile::collect_branch_paths(&doc);
    let surface = authoring_surface(
        &input,
        &folded.env.state,
        &folded.env.rel_vocab,
        &branch_paths,
    );

    if json {
        match serde_json::to_string_pretty(&surface) {
            Ok(s) => {
                if write_stdout(&format!("{s}\n")).is_err() {
                    return ExitCode::from(2);
                }
            }
            Err(e) => {
                eprintln!("lute: failed to serialize context: {e}");
                return ExitCode::from(2);
            }
        }
    } else if write_stdout(&context_outline(&surface)).is_err() {
        return ExitCode::from(2);
    }
    ExitCode::SUCCESS
}

/// Assemble the deterministic JSON authoring surface: every map is a BTreeMap
/// (key-sorted by construction) and every array is emitted in a stable order
/// (directives by name, state paths by path, components by name; attrs/params in
/// declaration order). `enums`/`assetKinds`/`providers` come straight off the
/// string (see `attr_type_str`/`state_type_str`). `branch_paths` marks the ACTUAL
/// implicit choice slots so their enum domains gain `unset` (matching compile).
/// `rel_vocab` is the ALREADY-MERGED relational vocabulary `fold_env` computes
/// (dsl 0.3.0 §3/§4, spec §5) — entity kinds, relations (+arity/domains/
/// `derive`), seed facts, rules, and project-level `enums:` — surfaced here
/// verbatim, no new resolution.
fn authoring_surface(
    input: &CheckInput,
    state: &lute_check::StateSchema,
    rel_vocab: &RelVocab,
    branch_paths: &BTreeSet<String>,
) -> serde_json::Value {
    use serde_json::{Map, Value};
    let snap = &input.snapshot;

    // Directives: BTreeMap key == directive name ⇒ iteration is name-sorted.
    // Attrs keep declaration order (their authoring/positional order).
    let directives: Vec<Value> = snap
        .directives
        .values()
        .map(|d| {
            let attrs: Vec<Value> = d
                .attrs
                .iter()
                .map(|a| {
                    let (ty, domain) = attr_type_str(&a.ty);
                    let mut o = Map::new();
                    o.insert("name".into(), a.name.clone().into());
                    o.insert("type".into(), ty.into());
                    o.insert("required".into(), a.required.into());
                    if let Some(dom) = domain {
                        o.insert("domain".into(), dom.into());
                    }
                    if let Some(def) = &a.default {
                        o.insert("default".into(), literal_json(def));
                    }
                    Value::Object(o)
                })
                .collect();
            let mut o = Map::new();
            o.insert("name".into(), d.name.clone().into());
            if let Some(layer) = &d.layer {
                o.insert("layer".into(), layer.clone().into());
            }
            o.insert("attrs".into(), attrs.into());
            o.insert("semantics".into(), d.semantics.clone().into());
            Value::Object(o)
        })
        .collect();

    // Folded state schema: BTreeMap key == path ⇒ iteration is path-sorted.
    let state_schema: Vec<Value> = state
        .decls
        .iter()
        .map(|(path, decl)| {
            // A path folded from a real `<branch>`/`<hub>` is an implicit choice
            // slot: its authorable enum domain is choice ids ∪ `unset` (compile's
            // state-table domain), NOT the folded members alone. Author enums at
            // any other path are not in `branch_paths` and keep their members.
            let (ty, domain) = state_type_str(branch_paths.contains(path), &decl.ty);
            let mut o = Map::new();
            o.insert("path".into(), path.clone().into());
            o.insert("type".into(), ty.into());
            o.insert("namespace".into(), namespace_str(decl.namespace).into());
            if let Some(def) = &decl.default {
                o.insert("default".into(), literal_json(def));
            }
            if let Some(dom) = domain {
                o.insert("domain".into(), dom.into());
            }
            Value::Object(o)
        })
        .collect();

    // Imported components (dsl §13): BTreeMap key == name ⇒ name-sorted; params
    // keep source (named-arg binding) order.
    let components: Vec<Value> = input
        .components
        .table
        .iter()
        .map(|(name, def)| {
            let params: Vec<Value> = def
                .params
                .iter()
                .map(|(pname, pty)| {
                    let (ty, domain) = attr_type_str(pty);
                    let mut o = Map::new();
                    o.insert("name".into(), pname.clone().into());
                    o.insert("type".into(), ty.into());
                    if let Some(dom) = domain {
                        o.insert("domain".into(), dom.into());
                    }
                    Value::Object(o)
                })
                .collect();
            let mut o = Map::new();
            o.insert("name".into(), name.clone().into());
            o.insert("params".into(), params.into());
            Value::Object(o)
        })
        .collect();

    // Entity kinds (dsl 0.3.0 §3.1): BTreeMap key == name ⇒ name-sorted. A
    // closed kind (`members: [...]`) carries its member list; an `open: true`
    // kind carries no member list (any id is legal); `Invalid` (neither/both)
    // is preserved as data (rel_schema.rs's discipline) rather than hidden.
    let entities: Vec<Value> = rel_vocab
        .kinds
        .iter()
        .map(|(name, decl)| {
            let mut o = Map::new();
            o.insert("name".into(), name.clone().into());
            match &decl.shape {
                KindShape::Members(members) => {
                    o.insert("shape".into(), "members".into());
                    o.insert("members".into(), members.clone().into());
                }
                KindShape::Open => {
                    o.insert("shape".into(), "open".into());
                }
                KindShape::Invalid => {
                    o.insert("shape".into(), "invalid".into());
                }
            }
            Value::Object(o)
        })
        .collect();

    // Relations (dsl 0.3.0 §4): BTreeMap key == name ⇒ name-sorted. `args` is
    // the ordered argument-domain (entity kind or enum) list; `arity` is its
    // length, surfaced explicitly so an AI need not count. `derive: true`
    // marks a Datalog-derived relation (no direct write tier, `tier_of`).
    let relations: Vec<Value> = rel_vocab
        .relations
        .iter()
        .map(|(name, decl)| {
            let mut o = Map::new();
            o.insert("name".into(), name.clone().into());
            o.insert("arity".into(), decl.args.len().into());
            o.insert("args".into(), decl.args.clone().into());
            o.insert("derive".into(), decl.derive.into());
            Value::Object(o)
        })
        .collect();

    // Seed facts (dsl 0.3.0 §4, D12): raw source text, in declaration order
    // (a `Vec`, not name-keyed — authoring order is meaningful, unlike the
    // name-sorted maps above).
    let facts: Vec<Value> = rel_vocab
        .facts
        .iter()
        .map(|f| Value::String(f.raw.clone()))
        .collect();

    // Rules (dsl 0.3.0 §7.1): raw source text, declaration order.
    let rules: Vec<Value> = rel_vocab
        .rules
        .iter()
        .map(|r| Value::String(r.raw.clone()))
        .collect();

    let mut root = Map::new();
    root.insert("capabilityVersion".into(), snap.version.clone().into());
    root.insert("directives".into(), directives.into());
    // enums/assetKinds/providers are BTreeMaps on the snapshot: their serde-JSON
    // objects are key-sorted by construction. `to_value` is infallible for these
    // concrete shapes; a defensive empty-object fallback keeps the surface total.
    root.insert(
        "enums".into(),
        serde_json::to_value(&snap.enums).unwrap_or_else(|_| serde_json::json!({})),
    );
    root.insert(
        "assetKinds".into(),
        serde_json::to_value(&snap.asset_kinds).unwrap_or_else(|_| serde_json::json!({})),
    );
    root.insert(
        "providers".into(),
        serde_json::to_value(&snap.providers).unwrap_or_else(|_| serde_json::json!({})),
    );
    root.insert("stateSchema".into(), state_schema.into());
    root.insert("components".into(), components.into());
    // Relational vocabulary (dsl 0.3.0 §3/§4, spec §5) — `entities`/`relations`/
    // `facts`/`rules` are new keys; `projectEnums` is the project-level
    // `enums:` (`rel_vocab.enums`), kept under its OWN key so it never
    // clobbers the plugin/core `enums` key above (a distinct vocabulary).
    root.insert("entities".into(), entities.into());
    root.insert("relations".into(), relations.into());
    root.insert("facts".into(), facts.into());
    root.insert("rules".into(), rules.into());
    root.insert(
        "projectEnums".into(),
        serde_json::to_value(&rel_vocab.enums).unwrap_or_else(|_| serde_json::json!({})),
    );
    Value::Object(root)
}

/// Render a state-path `Type` for parity with `lute_compile`'s `type_label`
/// (dsl §4.1): scalars + `enum`(+members); id-flavored types collapse to their
/// value-level label (`string`/`enum`) exactly as the compiled artifact's state
/// table does. `is_implicit` (path ∈ `collect_branch_paths`) marks a REAL
/// `<branch>`/`<hub>` choice slot: its enum domain is choice ids ∪ `unset` — the
/// author must write `<when is="unset">` for the pre-choice state — appended LAST,
/// byte-identical to `type_label(true, …)`. A plain author enum (`is_implicit ==
/// false`) keeps its folded members as the authorable domain, no `unset`.
fn state_type_str(is_implicit: bool, ty: &Type) -> (String, Option<Vec<String>>) {
    match ty {
        Type::Bool => ("bool".to_string(), None),
        Type::Number => ("number".to_string(), None),
        Type::Str => ("string".to_string(), None),
        Type::Enum(members) => {
            let mut domain = members.clone();
            if is_implicit {
                domain.push("unset".to_string());
            }
            ("enum".to_string(), Some(domain))
        }
        Type::List(_) => ("list".to_string(), None),
        Type::Record(_) => ("record".to_string(), None),
        Type::Map { .. } => ("map".to_string(), None),
        Type::EnumFromOption(_) => ("enum".to_string(), None),
        Type::ProviderRef(_) | Type::Domain(_) | Type::SlotId { .. } | Type::AssetKind(_) => {
            ("string".to_string(), None)
        }
        Type::NarrativeTime => ("narrativeTime".to_string(), None),
    }
}

/// Render an attr/param `Type` for the AUTHORING surface. The base labels match
/// `type_label` (`bool`/`number`/`string`/`enum`), but reference-bearing types
/// keep their target so an AI knows WHAT an id resolves against —
/// `providerRef:<catalog>`, `assetKind:<kind>`, `slotId:<namespace>`,
/// `enumFromOption:<option>` — and compound types name their element(s)
/// (`list<T>`, `map<K,V>`, `record`). An `enum` also carries its member domain.
fn attr_type_str(ty: &Type) -> (String, Option<Vec<String>>) {
    match ty {
        Type::Bool => ("bool".to_string(), None),
        Type::Number => ("number".to_string(), None),
        Type::Str => ("string".to_string(), None),
        Type::Enum(members) => ("enum".to_string(), Some(members.clone())),
        Type::List(inner) => (format!("list<{}>", attr_type_str(inner).0), None),
        Type::Record(_) => ("record".to_string(), None),
        Type::Map { key, value } => (
            format!("map<{},{}>", attr_type_str(key).0, attr_type_str(value).0),
            None,
        ),
        Type::EnumFromOption(opt) => (format!("enumFromOption:{opt}"), None),
        Type::ProviderRef(name) => (format!("providerRef:{name}"), None),
        Type::Domain(name) => (format!("domain:{name}"), None),
        Type::SlotId { namespace } => (format!("slotId:{namespace}"), None),
        Type::AssetKind(name) => (format!("assetKind:{name}"), None),
        Type::NarrativeTime => ("narrativeTime".to_string(), None),
    }
}

/// The state lifetime tier (dsl §9.1) as a lowercase string — tells an AI which
/// namespace a state path belongs to (`scene`/`run`/`user`/`app`).
fn namespace_str(ns: Namespace) -> &'static str {
    match ns {
        Namespace::Scene => "scene",
        Namespace::Run => "run",
        Namespace::User => "user",
        Namespace::App => "app",
        Namespace::Quest => "quest",
    }
}

/// Manifest `Literal` → JSON, mirroring `lute_compile`'s `literal_json`: an
/// integral float collapses to a JSON integer (`0`, not `0.0`) for a stable
/// authoring surface consistent with the compiled envelope.
fn literal_json(l: &Literal) -> serde_json::Value {
    match l {
        Literal::Bool(b) => serde_json::Value::Bool(*b),
        Literal::Num(n) if n.fract() == 0.0 && n.is_finite() && n.abs() < 9.0e15 => {
            serde_json::Value::from(*n as i64)
        }
        Literal::Num(n) => serde_json::Value::from(*n),
        Literal::Str(s) => serde_json::Value::String(s.clone()),
        Literal::List(xs) => serde_json::Value::Array(xs.iter().map(literal_json).collect()),
        Literal::Map(m) => serde_json::Value::Object(
            m.iter().map(|(k, v)| (k.clone(), literal_json(v))).collect(),
        ),
    }
}

/// A compact human outline of the authoring surface (non-`--json` mode): the
/// capabilityVersion, directive names + attr keys, enum names WITH their
/// members, state paths (with enum domains), the relational vocabulary
/// (entity kinds, relations w/ arity+domains+`derive`, seed facts, rules,
/// project-level enums), and component names. `--json` is the machine
/// surface; this is a short at-a-glance view.
fn context_outline(surface: &serde_json::Value) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "capabilityVersion: {}",
        surface["capabilityVersion"].as_str().unwrap_or("")
    );
    if let Some(dirs) = surface["directives"].as_array() {
        let _ = writeln!(out, "directives ({}):", dirs.len());
        for d in dirs {
            let name = d["name"].as_str().unwrap_or("");
            let layer = d["layer"]
                .as_str()
                .map(|l| format!(" [{l}]"))
                .unwrap_or_default();
            let attrs: Vec<&str> = d["attrs"]
                .as_array()
                .map(|a| a.iter().filter_map(|x| x["name"].as_str()).collect())
                .unwrap_or_default();
            let _ = writeln!(out, "  {name}{layer}: {}", attrs.join(", "));
        }
    }
    if let Some(enums) = surface["enums"].as_object() {
        // Members, not just names (spec §5) — an author choosing an
        // `emotion="…"` value sees the legal set without `--json`.
        let _ = writeln!(out, "enums ({}):", enums.len());
        for (name, members) in enums {
            let member_strs: Vec<&str> = members
                .as_array()
                .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
                .unwrap_or_default();
            let _ = writeln!(out, "  {name}: {}", member_strs.join(", "));
        }
    }
    if let Some(state) = surface["stateSchema"].as_array() {
        let _ = writeln!(out, "stateSchema ({}):", state.len());
        for s in state {
            let path = s["path"].as_str().unwrap_or("");
            let ty = s["type"].as_str().unwrap_or("");
            let dom = s["domain"]
                .as_array()
                .map(|d| {
                    let members: Vec<&str> = d.iter().filter_map(|x| x.as_str()).collect();
                    format!(" [{}]", members.join(", "))
                })
                .unwrap_or_default();
            let _ = writeln!(out, "  {path}: {ty}{dom}");
        }
    }
    // Relational vocabulary (dsl 0.3.0 §3/§4, spec §5): entity kinds,
    // relations (name/arity/domains/derive), seed facts, rules, and the
    // project-level `enums:` — kept separate from the plugin/core `enums`
    // block above.
    if let Some(entities) = surface["entities"].as_array() {
        if !entities.is_empty() {
            let _ = writeln!(out, "entities ({}):", entities.len());
            for e in entities {
                let name = e["name"].as_str().unwrap_or("");
                let shape = e["shape"].as_str().unwrap_or("");
                if shape == "members" {
                    let members: Vec<&str> = e["members"]
                        .as_array()
                        .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
                        .unwrap_or_default();
                    let _ = writeln!(out, "  {name}: {}", members.join(", "));
                } else {
                    let _ = writeln!(out, "  {name}: {shape}");
                }
            }
        }
    }
    if let Some(relations) = surface["relations"].as_array() {
        if !relations.is_empty() {
            let _ = writeln!(out, "relations ({}):", relations.len());
            for r in relations {
                let name = r["name"].as_str().unwrap_or("");
                let arity = r["arity"].as_u64().unwrap_or(0);
                let args: Vec<&str> = r["args"]
                    .as_array()
                    .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
                    .unwrap_or_default();
                let tag = if r["derive"].as_bool().unwrap_or(false) {
                    " [derive]"
                } else {
                    ""
                };
                let _ = writeln!(out, "  {name}/{arity}({}){tag}", args.join(", "));
            }
        }
    }
    if let Some(facts) = surface["facts"].as_array() {
        if !facts.is_empty() {
            let _ = writeln!(out, "facts ({}):", facts.len());
            for f in facts {
                let _ = writeln!(out, "  {}", f.as_str().unwrap_or(""));
            }
        }
    }
    if let Some(rules) = surface["rules"].as_array() {
        if !rules.is_empty() {
            let _ = writeln!(out, "rules ({}):", rules.len());
            for r in rules {
                let _ = writeln!(out, "  {}", r.as_str().unwrap_or(""));
            }
        }
    }
    if let Some(penums) = surface["projectEnums"].as_object() {
        if !penums.is_empty() {
            let _ = writeln!(out, "projectEnums ({}):", penums.len());
            for (name, members) in penums {
                let member_strs: Vec<&str> = members
                    .as_array()
                    .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
                    .unwrap_or_default();
                let _ = writeln!(out, "  {name}: {}", member_strs.join(", "));
            }
        }
    }
    if let Some(comps) = surface["components"].as_array() {
        if !comps.is_empty() {
            let names: Vec<&str> = comps.iter().filter_map(|c| c["name"].as_str()).collect();
            let _ = writeln!(out, "components ({}): {}", names.len(), names.join(", "));
        }
    }
    out
}

/// Run `compile` over one file. Exit `0` with the artifact on stdout (or
/// `-o <FILE>`), `1` when the check gate fails (diagnostics to stdout,
/// human or `--json`), `2` on I/O or serialization failure.
fn run_compile(
    file: &Path,
    json: bool,
    providers: Option<&Path>,
    project: Option<&Path>,
    out: Option<&Path>,
) -> ExitCode {
    let Some(input) = build_input(file, providers, project) else {
        return ExitCode::from(2);
    };
    match lute_compile::compile(&input) {
        Ok(artifact) => {
            let mut s = match serde_json::to_string_pretty(&artifact) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("lute: failed to serialize artifact: {e}");
                    return ExitCode::from(2);
                }
            };
            s.push('\n');
            match out {
                Some(path) => {
                    if let Err(e) = std::fs::write(path, &s) {
                        eprintln!("lute: cannot write {}: {e}", path.display());
                        return ExitCode::from(2);
                    }
                }
                None => {
                    if write_stdout(&s).is_err() {
                        return ExitCode::from(2);
                    }
                }
            }
            ExitCode::SUCCESS
        }
        Err(diags) => {
            let s = if json {
                let mut s = match serde_json::to_string_pretty(&diags) {
                    Ok(s) => s,
                    Err(e) => {
                        eprintln!("lute: failed to serialize diagnostics: {e}");
                        return ExitCode::from(2);
                    }
                };
                s.push('\n');
                s
            } else {
                let mut s = String::new();
                for d in &diags {
                    let _ = writeln!(
                        s,
                        "{}:{}:{}: {} [{}] {}",
                        file.display(),
                        d.span.line,
                        d.span.column,
                        severity_str(d.severity),
                        d.code,
                        d.message
                    );
                }
                let errors = diags
                    .iter()
                    .filter(|d| d.severity == Severity::Error)
                    .count();
                let _ = writeln!(s, "{errors} error(s); no artifact emitted");
                s
            };
            if write_stdout(&s).is_err() {
                return ExitCode::from(2);
            }
            ExitCode::FAILURE
        }
    }
}

/// Write `s` to stdout as raw bytes, returning any I/O error instead of
/// panicking the way `print!`/`println!` do when the pipe is closed (EPIPE,
/// e.g. `lute compile f.lute | head`). Callers map `Err` to exit `2`, matching
/// the `-o` file-write error path (compiler CLI spec: `2` on an I/O failure).
fn write_stdout(s: &str) -> std::io::Result<()> {
    let mut o = std::io::stdout().lock();
    o.write_all(s.as_bytes())?;
    o.flush()
}

/// Run `trace` over one file (dsl 0.4.0 §4.3/§4.5): resolve the document
/// IDENTICALLY to `check`/`compile` ([`build_input`]), load + merge the
/// `--mock` file with the CLI's own `--state`/`--fact`/`--choose`/`--event`/
/// `--accept` flags into one [`MockSet`] ([`merge`] — "CLI flags compose with
/// the file; on a conflict the flag wins"), then hand off to
/// [`lute_trace::trace_document`] — the entire §4.3 mock-validation gate,
/// the §4.4 walk, and the §4.5 report are ITS concern; this function owns
/// only flag assembly, file I/O, and the exit-code/render mapping.
///
/// Exit codes (§4.5): `0` [`TraceExit::Complete`], `1`
/// [`TraceExit::Refused`] (a document check error OR an invalid mock — the
/// `E-TRACE-*` diagnostics render in EXACTLY [`print_diagnostics`]'s
/// check-diagnostic line format; a refusal whose diagnostics are NOT all
/// `E-TRACE-*` came from the `check` gate itself, so a "run `lute check`
/// first" hint is appended), `2` I/O (unreadable `.lute`/`--mock` file, or a
/// malformed `--mock` YAML document — the same tier `run_check`/`run_compile`
/// use for a read failure), `3` [`TraceExit::Incomplete`] (an `unknown`
/// guard halted the walk, or an unresolved objective/quest atom).
fn run_trace(
    file: &Path,
    state: Vec<(String, String)>,
    fact: Vec<String>,
    choose: Vec<(String, Vec<String>)>,
    event: Vec<String>,
    accept: Vec<String>,
    mock: Option<&Path>,
    json: bool,
    providers: Option<&Path>,
    project: Option<&Path>,
) -> ExitCode {
    let Some(input) = build_input(file, providers, project) else {
        return ExitCode::from(2);
    };

    let file_mocks = match mock {
        Some(path) => {
            let text = match std::fs::read_to_string(path) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("lute: cannot read {}: {e}", path.display());
                    return ExitCode::from(2);
                }
            };
            match parse_mock_yaml(&text) {
                Ok(m) => m,
                Err(d) => {
                    // A malformed `--mock` YAML document is a file-level I/O/
                    // format failure, not a schema-validation refusal — `2`,
                    // matching `run_check`'s/`run_compile`'s read-failure tier.
                    eprintln!(
                        "lute: {}:{}:{}: [{}] {}",
                        path.display(),
                        d.span.line,
                        d.span.column,
                        d.code,
                        d.message
                    );
                    return ExitCode::from(2);
                }
            }
        }
        None => MockSet::default(),
    };

    // `--state`/`--mock` literals and `--choose` targets carry no real
    // source text, so every flag-origin entry is spanned at the same
    // zeroed placeholder ([`lute_trace::mock`]'s own "CLI-arg synthetic
    // span" convention — that helper is `pub(crate)` there, so this mirrors
    // it byte-for-byte rather than reaching into the crate's internals).
    let span = lute_core_span::Span { byte_start: 0, byte_end: 0, line: 0, column: 0, utf16_range: (0, 0) };
    let flag_mocks = MockSet {
        state: state.into_iter().map(|(path, literal)| (path, literal, span)).collect(),
        facts: fact,
        choose: choose.into_iter().collect(),
        events: event,
        accepts: accept,
    };

    let mocks = merge(file_mocks, flag_mocks);
    let (report, exit) = lute_trace::trace_document(&input, mocks);

    match exit {
        TraceExit::Complete => {
            print_trace_report(&report, json);
            ExitCode::SUCCESS
        }
        TraceExit::Incomplete => {
            print_trace_report(&report, json);
            ExitCode::from(3)
        }
        TraceExit::Refused(diags) => {
            if json {
                match serde_json::to_string_pretty(&diags) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        eprintln!("lute: failed to serialize diagnostics: {e}");
                        return ExitCode::from(2);
                    }
                }
            } else {
                print_diagnostics(file, &diags);
                // Every `E-TRACE-*` code is mock/choice validation (D1
                // quarantine: `lute-check` cannot know that vocabulary, so
                // its OWN diagnostics never carry it) — a refusal carrying
                // anything else came from the `check` gate itself (§4.3:
                // "MUST refuse a document with check errors ... run `check`
                // first").
                if diags.iter().any(|d| !d.code.starts_with("E-TRACE-")) {
                    println!(
                        "trace refused: {} has check error(s) — run `lute check` first",
                        file.display()
                    );
                } else {
                    println!("trace refused: {} — invalid mock input", file.display());
                }
            }
            ExitCode::from(1)
        }
    }
}

/// Render one [`TraceReport`] to stdout — `--json` -> [`TraceReport::render_json`]
/// (§4.5 machine form), otherwise [`TraceReport::render_human`] (the
/// transcript already ends in `\n`, so `print!` avoids a doubled blank line).
fn print_trace_report(report: &TraceReport, json: bool) {
    if json {
        println!("{}", report.render_json());
    } else {
        print!("{}", report.render_human());
    }
}

/// Back-fill a stable `code` into every untagged `:line` (dsl §12), rewriting
/// the file in place. A thin shell over [`lute_check::tag_document`] (the pure
/// core that owns the tagging logic): read the file, tag, and — only when at
/// least one line was tagged — write the result back. Exit `0` on success
/// (whether or not anything changed), `2` on an I/O failure (like `run_check`).
fn run_tag(file: &Path) -> ExitCode {
    let text = match std::fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("lute: cannot read {}: {e}", file.display());
            return ExitCode::from(2);
        }
    };

    let out = lute_check::tag_document(&text);

    // Never partial-writes: only touch the file when a `code` was actually
    // added, so an already-tagged document is left byte-identical (idempotent).
    if out.added > 0 {
        if let Err(e) = std::fs::write(file, &out.text) {
            eprintln!("lute: cannot write {}: {e}", file.display());
            return ExitCode::from(2);
        }
        println!("lute: tagged {} line(s)", out.added);
    } else {
        println!("lute: already tagged");
    }

    ExitCode::SUCCESS
}

/// Migrate a pre-0.2.2 document to 0.2.2 in place (dsl §7.1, §7.3), rewriting
/// the file only when a span was actually changed. A thin shell over
/// [`lute_check::fix_document`] (the pure core that owns the migration:
/// `:line[speaker]` → `@speaker`, any other content line's leading `:` sigil
/// → `@`, then `<choice>`/`<hub>` choice `as` → `into`): read the file,
/// migrate, and — only when at least one edit applied — write the result
/// back, so an already-0.2.2 document is left byte-identical (idempotent).
/// Exit `0` on success (whether or not anything changed), `2` on an I/O failure
/// (like `run_tag`).
fn run_fix(file: &Path) -> ExitCode {
    let text = match std::fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("lute: cannot read {}: {e}", file.display());
            return ExitCode::from(2);
        }
    };

    let out = lute_check::fix_document(&text);

    if out.changed > 0 {
        if let Err(e) = std::fs::write(file, &out.text) {
            eprintln!("lute: cannot write {}: {e}", file.display());
            return ExitCode::from(2);
        }
        println!("lute: migrated {} edit(s) to 0.2.2", out.changed);
    } else {
        println!("lute: already 0.2.2");
    }

    ExitCode::SUCCESS
}

/// One `file:line:col: severity [CODE] message` line per diagnostic. A
/// primary that collapsed same-root repeats (dsl 0.4.0 §8.2 C1/C5) appends a
/// trailing ` (+N more: 12:3, 47:9, …)` — line:column, comma-joined, document
/// order. Shared by [`print_human`] (the `check`/`compile` diagnostic list)
/// and `run_trace`'s Refused rendering (dsl 0.4.0 §4.5: "the `E-TRACE-*`
/// codes render exactly as check diagnostics do") — ONE line format, never a
/// second convention.
fn print_diagnostics(file: &Path, diagnostics: &[Diagnostic]) {
    let path = file.display();
    for d in diagnostics {
        let more = if d.covered.is_empty() {
            String::new()
        } else {
            let locs: Vec<String> = d
                .covered
                .iter()
                .map(|s| format!("{}:{}", s.line, s.column))
                .collect();
            format!(" (+{} more: {})", locs.len(), locs.join(", "))
        };
        println!(
            "{path}:{}:{}: {} [{}] {}{more}",
            d.span.line,
            d.span.column,
            severity_str(d.severity),
            d.code,
            d.message,
        );
        // dsl 0.5.0 §2.2: an `E-COMPONENT-PARSE` (or any diagnostic) carrying
        // `related` sub-diagnostics from ANOTHER file (e.g. a failed
        // component import's own parse errors) — print each indented under
        // the parent line, `related.file` in place of the importer's path,
        // so the author sees what actually failed without a separate
        // `check` of the component.
        for r in &d.related {
            println!(
                "    {}:{}:{}: {} [{}] {}",
                r.file,
                r.diagnostic.span.line,
                r.diagnostic.span.column,
                severity_str(r.diagnostic.severity),
                r.diagnostic.code,
                r.diagnostic.message,
            );
        }
    }
}

/// A summary line per diagnostic (via [`print_diagnostics`]), then a
/// pass/fail count summary. Mirrors the sorted order `check()` already
/// applied.
fn print_human(file: &Path, result: &lute_check::CheckResult) {
    let path = file.display();
    print_diagnostics(file, &result.diagnostics);
    // §8.3: counting is by primaries — collapse (0.4.0 T14) already reduced
    // `result.diagnostics` to one entry per root cause, so a plain count needs
    // no change here. Five reads of one typo are ONE error.
    let errors = result
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    let warnings = result
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .count();
    if result.ok {
        println!("ok: {path} ({warnings} warning(s))");
    } else {
        println!("failed: {path} ({errors} error(s), {warnings} warning(s))");
    }
}

fn severity_str(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
        Severity::Hint => "hint",
    }
}

/// Re-stamp every provider snapshot in `dir` to the current `capabilityVersion`
/// and clear `stale`, rewriting each file in place (plugin §10). A missing dir is
/// created empty. Exit `0` on success, `2` on an I/O failure.
///
/// With `--project`, the stamp is the RESOLVED multi-plugin `capabilityVersion`
/// (no scene ⇒ default profile, via `resolve_document_snapshot`), matching what a
/// project build validates against (plugin §13). Without it, the core-only
/// (`lute.core`) version is used — behavior identical to before.
///
/// Refresh iterates the directory itself (rather than `ProviderSet::load`, which
/// discards filenames) so each snapshot rewrites to the file it came from.
fn run_refresh(dir: &Path, project: Option<&Path>) -> ExitCode {
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("lute: cannot create {}: {e}", dir.display());
        return ExitCode::from(2);
    }

    // Under a project, stamp the resolved snapshot's version (plugin §13). A
    // malformed project must not silently mis-stamp: surface it and fall back to
    // the core-only version rather than pretending it loaded.
    let version = match project {
        Some(p) => match load_project(p) {
            Ok(cfg) => {
                resolve_document_snapshot(cfg.as_ref(), None, &BTreeMap::new())
                    .0
                    .version
            }
            Err(e) => {
                eprintln!("lute: {e}");
                load_core_snapshot().version
            }
        },
        None => load_core_snapshot().version,
    };

    let entries = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(e) => {
            eprintln!("lute: cannot read {}: {e}", dir.display());
            return ExitCode::from(2);
        }
    };

    let mut paths: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.is_file()
                && matches!(
                    p.extension().and_then(|x| x.to_str()),
                    Some("yaml") | Some("yml")
                )
        })
        .collect();
    paths.sort();

    let mut refreshed = 0usize;
    for path in &paths {
        let raw = match std::fs::read_to_string(path) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("lute: cannot read {}: {e}", path.display());
                return ExitCode::from(2);
            }
        };
        let mut snap: ProviderSnapshot = match serde_yaml::from_str(&raw) {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "lute: skipping {} (not a provider snapshot): {e}",
                    path.display()
                );
                continue;
            }
        };
        snap.manifest_version = version.clone();
        snap.stale = false;
        let out = match serde_yaml::to_string(&snap) {
            Ok(o) => o,
            Err(e) => {
                eprintln!("lute: cannot serialize {}: {e}", path.display());
                return ExitCode::from(2);
            }
        };
        if let Err(e) = std::fs::write(path, out) {
            eprintln!("lute: cannot write {}: {e}", path.display());
            return ExitCode::from(2);
        }
        refreshed += 1;
    }

    println!(
        "refreshed {refreshed} snapshot(s) in {} (capabilityVersion {version})",
        dir.display()
    );
    ExitCode::SUCCESS
}
