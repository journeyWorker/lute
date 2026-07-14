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
//!   edge between them. ALSO, PER RESOLVED PROJECT ROOT, `W-QUEST-REF-UNKNOWN`
//!   (dsl 0.5.1 §1.4): every referenced reserved `quest.<id>.state` /
//!   `quest.<id>.objectives.<oid>.done` path across the root's docs must
//!   resolve to a quest (and objective) some quest doc in the root DEFINES —
//!   a WARNING (never flips the exit verdict) naming the referencing
//!   document and the unresolved path; single-file `lute check` has no
//!   project graph and never emits it. Exit `0` clean, `1` when any file has
//!   an `Error` or any resolved root's quest-id pass finds a collision, `2`
//!   on an I/O failure. `--json` prints a structured report (per-file
//!   `CheckResult`s + the project-wide diagnostics); otherwise per-file
//!   human lines plus a project-wide section.
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
    check, check_definite_assignment, check_project_quest_ids, check_project_quest_refs,
    defassign, envelope, fold_env, parse_meta, CheckInput, Mode, Namespace, RelVocab,
};
use lute_core_span::{Diagnostic, Severity, Span};
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
    /// Project-wide, read-only reporting surface over everything the
    /// connectivity layer computes (dsl §5:571-584): the assembled node/edge
    /// graph, per-node reachability plus its declared `after` structure, and
    /// the Guaranteed/Possible envelope tables — including the
    /// `Possible \ Guaranteed` warning-grade reads `check-project` computes
    /// and drops by default (dsl §6). Evaluates no CEL, runs no Datalog,
    /// takes no mocks — pure graph math over declared structure, reusing the
    /// SAME per-root project-doc collection `check-project` builds (never a
    /// second file-walk/parse). Exit `0` on success, `2` on an I/O failure or
    /// an unresolvable node id.
    Scenario {
        /// Directory to walk recursively for `*.lute` files; also the
        /// project root passed to `load_project`, matching `check-project`'s
        /// own `dir` semantics.
        dir: PathBuf,
        /// Directory of pinned provider snapshots to resolve ids against.
        #[arg(long, value_name = "DIR")]
        providers: Option<PathBuf>,
        /// `reach`/`envelope` sub-view; omitted -> prints the assembled
        /// topological graph (dsl §5:574).
        #[command(subcommand)]
        command: Option<ScenarioCommand>,
    },
}

/// See [`Command::Scenario`].
#[derive(Subcommand)]
enum ScenarioCommand {
    /// Report a node's reachability verdict (Reachable/Unreachable/Unknown,
    /// T6) plus its declared `after` prerequisite structure (dsl §5:575).
    Reach {
        /// A scene's canonical key (e.g. `bianca.s01ep02`), or `quest:<id>`
        /// for a quest (dsl §4.4's `envelope quest:<id>` syntax).
        node_id: String,
    },
    /// Report the Guaranteed/Possible envelope tables for a node (T10) —
    /// full tables for a scene or an `after`-opted-in quest; defaults-only
    /// `D` plus an enrichment note for a bare quest (T12, dsl §4.4). Also
    /// prints the `Possible \ Guaranteed` warning-grade reads for the node
    /// (dsl §6) — suppressed by default in `check-project`, surfaced here.
    Envelope {
        /// A scene's canonical key, or `quest:<id>` for a quest.
        node_id: String,
    },
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
        Command::Scenario {
            dir,
            providers,
            command,
        } => run_scenario(&dir, providers.as_deref(), command),
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

/// One resolved project root's docs, each paired with its parsed
/// `Document` and `fold_env`'s `FoldedEnv` — the per-root unit
/// `check-project` and `lute scenario` (T14) both group by.
type DocGroup = Vec<(PathBuf, lute_syntax::ast::Document, lute_check::FoldedEnv)>;
type ByRoot = BTreeMap<PathBuf, DocGroup>;

/// Walk `dir` for `.lute` files ([`find_lute_files`]), `check()` +
/// `fold_env` each one against its OWN resolved project root
/// ([`project_root_for`]), and group the parsed docs by that root — the
/// shared file-collection step `check-project` and `lute scenario` (T14)
/// both build on top of, so the two commands can never observe a DIFFERENT
/// project structure for the same `dir` (never a second file-walk/parse).
/// `Err(ExitCode::from(2))` on the same I/O failures `run_check_project`
/// always had: the walk itself failing, or `build_input` unable to read a
/// file.
fn collect_project_docs(
    dir: &Path,
    providers: Option<&Path>,
) -> Result<(Vec<(PathBuf, lute_check::CheckResult)>, ByRoot), ExitCode> {
    let files = find_lute_files(dir).map_err(|e| {
        eprintln!("lute: cannot walk {}: {e}", dir.display());
        ExitCode::from(2)
    })?;

    let mut file_results: Vec<(PathBuf, lute_check::CheckResult)> =
        Vec::with_capacity(files.len());
    let mut docs: Vec<(PathBuf, lute_syntax::ast::Document)> = Vec::with_capacity(files.len());
    let mut foldeds: Vec<lute_check::FoldedEnv> = Vec::with_capacity(files.len());
    let mut roots: Vec<PathBuf> = Vec::with_capacity(files.len());

    for file in &files {
        let root = project_root_for(file, dir);
        let Some(input) = build_input(file, providers, Some(&root)) else {
            return Err(ExitCode::from(2));
        };
        let (doc, _) = lute_syntax::parse(&input.text);
        let (folded, _, _) = fold_env(&doc, &input);
        foldeds.push(folded);
        docs.push((file.clone(), doc));

        let result = check(&input);
        file_results.push((file.clone(), result));
        roots.push(root);
    }

    let mut by_root: ByRoot = BTreeMap::new();
    for (idx, entry) in docs.iter().enumerate() {
        by_root
            .entry(roots[idx].clone())
            .or_default()
            .push((entry.0.clone(), entry.1.clone(), foldeds[idx].clone()));
    }

    Ok((file_results, by_root))
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
    let (mut file_results, by_root) = match collect_project_docs(dir, providers) {
        Ok(v) => v,
        Err(code) => return code,
    };

    let mut project_diags = Vec::new();
    // T11: every ENTRY-DEPENDENT, RUN/USER-TIER read at a NON-TAINTED scene
    // node that per-file `check()` already flagged `E-MAYBE-UNSET` gets
    // RECLASSIFIED against the project envelope below (mirrors the
    // `E-QUEST-ID-DUP` retain-pass precedent, §5). Matched by (path, span,
    // exact message) -- NOT (path, span) alone: `check_reads`/
    // `apply_condition` give every path in ONE CEL slot the SAME `Span`
    // (defassign.rs has no per-path span), so a mixed expression like
    // `run.upstream && scene.local` has BOTH reads at an IDENTICAL span --
    // only the message (which embeds the exact path text verbatim,
    // uniquely) tells them apart. `envelope::in_envelope_scope` is applied
    // BEFORE a site ever enters this list, so an out-of-scope `scene.*`/
    // `quest.*`/`app.*` `E-MAYBE-UNSET` is NEVER reconciled (T11 only ever
    // classifies `run.*`/`user.*`). Every reconciled site's per-file
    // `E-MAYBE-UNSET` is dropped in the retain pass further down,
    // REGARDLESS of the reclassification's outcome (`Guaranteed` → dropped
    // with no replacement; `Possible\Guaranteed` → dropped, warning-grade
    // `E-STATE-MAYBE-UNAVAILABLE` computed-and-discarded, default-
    // suppressed per dsl §4.3/§5 until T14's `lute scenario envelope`
    // exists; `∉ Possible` → dropped, replaced by an error-grade
    // `E-STATE-MAYBE-UNAVAILABLE` in `project_diags`). A TAINTED node's
    // reads are never added here -- its `Env` is untrustworthy, so its
    // per-file `E-MAYBE-UNSET` stays exactly as `check()` reported it.
    let mut reconciled_reads: Vec<(PathBuf, Span, String)> = Vec::new();
    // Every occurrence within its own resolved root already covers (see the
    // fn doc comment above) — used below to suppress ONLY the per-file
    // `E-QUEST-ID-DUP`s that pass demonstrably re-reports, never the ones it
    // structurally cannot see (an import-graph collision reaching outside
    // `dir`, or a same-id declare in a SIBLING project root).
    let mut covered = Vec::new();
    for group_full in by_root.values() {
        let plain_group: Vec<(PathBuf, lute_syntax::ast::Document)> =
            group_full.iter().map(|(p, d, _)| (p.clone(), d.clone())).collect();
        let group = &plain_group;
        project_diags.extend(check_project_quest_ids(group));
        project_diags.extend(check_project_quest_refs(group));
        project_diags.extend(lute_check::connectivity::check_conn_episode_dup(group));
        let key_set = lute_check::connectivity::scene_key_set(group);
        let quest_ids = lute_check::connectivity::quest_id_set(group);
        project_diags.extend(lute_check::connectivity::resolve_nodes(group, &key_set, &quest_ids));
        let (conn_graph, cycle_diags) =
            lute_check::connectivity::assemble_graph(group, &key_set, &quest_ids);
        project_diags.extend(cycle_diags);
        // T7/T14 wiring note: `unreachable_quest_ids` extracts the REAL
        // per-root unreachable-quest set from this same `file_results`
        // pass (span-matched against `E-QUEST-UNREACHABLE`, verified
        // exact -- both `group` and `file_results` parse the SAME source
        // text deterministically, so spans always agree). No empty-set
        // stub remains: `E-CONN-UNREACHABLE` fires for real here.
        let unreachable_quests =
            lute_check::connectivity::unreachable_quest_ids(group, &file_results);
        // T6 review + review-2: `completed(Q)` must consult the FULL
        // per-root declared quest-id set (`quest_ids`, same collection T4
        // already computed above) and treat any id with more than one
        // declaration as ambiguous (`ambiguous_quest_ids`) -- never a
        // false `Reachable`/`Unreachable` guess for it.
        let ambiguous_quests = lute_check::connectivity::ambiguous_quest_ids(group);
        let (reach, reach_diags) = lute_check::connectivity::check_reachability(
            &conn_graph,
            &quest_ids,
            &ambiguous_quests,
            &unreachable_quests,
        );
        project_diags.extend(reach_diags);
        // T7: `producible()` rule-dependency walk + relational-objective-
        // liveness (dsl §4.2/§B). `live_assert_relations` is the
        // reachability-GATED assert-site base case: every relation with an
        // `::assert{R(…)}` site in a node this root's `reach` map did NOT
        // prove `Unreachable` (`Reachable` OR `Unknown` both seed
        // producibility -- provable-only, excluding `Unknown` would risk a
        // false-dead claim on a node this pass simply cannot resolve).
        // Each doc's OWN `folded` (from `fold_env`, above) drives its OWN
        // `producible()` map AND the liveness scan's `decide()` context --
        // a doc can only gate an objective on a relation/def ITS OWN fold
        // declares.
        let live_asserts = lute_check::connectivity::live_assert_relations(
            group,
            &reach,
            &ambiguous_quests,
            &unreachable_quests,
        );
        let no_params: BTreeMap<String, lute_check::DomainInfo> = BTreeMap::new();
        for (path, doc, folded) in group_full {
            let producible =
                lute_check::producible::producible(&folded.env.rel_vocab, &live_asserts);
            let defs = lute_check::DefTable {
                bodies: &folded.def_bodies,
                params: &folded.env.def_params,
            };
            // `<objective>` attrs never have `$` in scope (mirrors
            // `check_objective_reach`'s own `base_ctx`); component params
            // are empty here (an objective is never authored inside a
            // standalone component-file self-check).
            let ctx = lute_check::DecideCtx {
                schema: &folded.env.state,
                dollar: None,
                params: &no_params,
            };
            for d in
                lute_check::producible::scan_objective_liveness(doc, &producible, &defs, &ctx)
            {
                project_diags.push((path.clone(), d));
            }
        }
        // T10/T11: connectivity envelope (dsl §4.3). `PerDocEffects`
        // populated from T8 (per-scene `guaranteed`/`possible_writes`,
        // recomputed here from this root's own docs+resolved schema, keyed
        // by the SAME canonical key as `NodeId::Scene` -- the key's FIRST
        // `key_set` occurrence, mirroring `assemble_graph`'s own node
        // anchor) and T9 (`writes_on_complete` per quest id, EVERY resolved
        // quest present as a key incl. empty-write; an empty or AMBIGUOUS
        // id is omitted -- absence is `propagate`'s resolvability signal).
        // `d` = project-resolved `run.*`/`user.*` schema-default set (dsl
        // §4.3 spec lines 442-448), unioned across every doc's own resolved
        // schema in this root.
        let mut per_doc = envelope::PerDocEffects::default();
        let mut envelope_d: BTreeSet<String> = BTreeSet::new();
        let mut reads_per_scene: BTreeMap<String, Vec<(String, Span)>> = BTreeMap::new();
        // Per non-tainted, in-scope, entry-dependent read site: the exact
        // per-file `E-MAYBE-UNSET` diagnostic it would earn, keyed by
        // canonical scene key. Built HERE (not after `propagate`) because
        // it needs `local_diags`, discarded everywhere else -- `reads[i]`
        // and the i-th `E-MAYBE-UNSET` in `local_diags` are pushed
        // TOGETHER, unconditionally, at the SAME `check_read` call site
        // (defassign.rs), so zipping them by position is exact, not a
        // heuristic.
        let mut sites_per_scene: BTreeMap<String, Vec<(Span, String)>> = BTreeMap::new();
        for (_path, doc, folded) in group_full {
            envelope_d.extend(envelope::schema_defaults(&folded.env.state));
            for quest in &doc.quests {
                if quest.id.is_empty() || ambiguous_quests.contains(&quest.id) {
                    continue;
                }
                per_doc.quest_writes_on_complete.insert(
                    quest.id.clone(),
                    envelope::writes_on_complete(quest, &folded.env.state),
                );
            }
        }
        for (key, occurrences) in &key_set {
            let Some((scene_path, _)) = occurrences.first() else { continue };
            let Some((_, doc, folded)) = group_full.iter().find(|(p, _, _)| p == scene_path)
            else {
                continue;
            };
            let all_nodes: Vec<lute_syntax::ast::Node> =
                doc.shots.iter().flat_map(|s| s.body.iter().cloned()).collect();
            let (local_diags, assigned, reads) =
                check_definite_assignment(&all_nodes, &folded.env.state);
            // T4.4/T4.6 carry-forward parity (dsl §7 soundness invariant): the
            // real `check()` pipeline (`check.rs::suppress_exhaustive_subject_reads`)
            // drops any `E-MAYBE-UNSET` whose span is a domain-exhaustive
            // `<match>` subject BEFORE `file_results` is ever populated -- a
            // read like that never earns a per-file `E-MAYBE-UNSET` standalone,
            // so it must never be treated as "entry-dependent" here either, or
            // this project-level recomputation (which calls
            // `check_definite_assignment` raw, unaware of that later
            // suppression) would newly error a file `check()` reports clean.
            let exhaustive_spans =
                defassign::exhaustive_match_subject_spans(&all_nodes, &folded.env.state);
            let is_exhaustive_subject = |span: &Span| {
                exhaustive_spans
                    .iter()
                    .any(|s| s.byte_start == span.byte_start && s.byte_end == span.byte_end)
            };
            per_doc.scene.insert(
                key.clone(),
                (envelope::guaranteed(&assigned), envelope::possible_writes(&all_nodes)),
            );
            let maybe_unset_messages: Vec<&str> = local_diags
                .iter()
                .filter(|d| d.code == "E-MAYBE-UNSET")
                .map(|d| d.message.as_str())
                .collect();
            debug_assert_eq!(
                reads.len(),
                maybe_unset_messages.len(),
                "check_definite_assignment must push exactly one E-MAYBE-UNSET per \
                 entry-dependent read, in the same order"
            );
            let paired: Vec<((String, Span), &str)> =
                reads.into_iter().zip(maybe_unset_messages).collect();
            let sites: Vec<(Span, String)> = paired
                .iter()
                .filter(|((path, span), _)| {
                    envelope::in_envelope_scope(path) && !is_exhaustive_subject(span)
                })
                .map(|((_, span), msg)| (*span, (*msg).to_string()))
                .collect();
            let reads: Vec<(String, Span)> = paired
                .into_iter()
                .filter(|((_, span), _)| !is_exhaustive_subject(span))
                .map(|(r, _)| r)
                .collect();
            sites_per_scene.insert(key.clone(), sites);
            reads_per_scene.insert(key.clone(), reads);
        }
        let (envs, tainted) = envelope::propagate(&conn_graph, &per_doc, &envelope_d);
        // `check_envelope` returns BOTH grades together (see its own doc
        // comment); only the error grade joins `project_diags` -- the
        // warning grade is intentionally computed-and-discarded here (dsl
        // §4.3/§5: default-suppressed until T14's `lute scenario envelope`
        // exists to surface it). EVERY entry-dependent, in-scope,
        // non-tainted read is reconciled below regardless of its own
        // classification outcome.
        for (path, d) in
            envelope::check_envelope(&conn_graph, &envs, &tainted, &reads_per_scene)
        {
            if d.severity == Severity::Error {
                project_diags.push((path, d));
            }
        }
        for (key, occurrences) in &key_set {
            let node_id = lute_check::connectivity::NodeId::Scene(key.clone());
            // Only reconcile (drop) a read's per-file `E-MAYBE-UNSET` when
            // its node has a REAL envelope to reclassify against: present
            // in `envs` AND not `tainted`. A node absent from `envs`
            // (e.g. a graph-wide `E-CONN-CYCLE` empties `topo_order`, so
            // `propagate` never inserts an entry for ANY node in that
            // root -- not just the cyclic ones) is exactly as untrustworthy
            // as a tainted one: `check_envelope` above already skips it
            // (no replacement diagnostic emitted for it either), so
            // dropping its per-file diagnostic here would silently lose a
            // genuine local maybe-unset error with nothing to replace it.
            if tainted.contains(&node_id) || !envs.contains_key(&node_id) {
                continue;
            }
            let Some((scene_path, _)) = occurrences.first() else { continue };
            let Some(sites) = sites_per_scene.get(key) else { continue };
            for (span, message) in sites {
                reconciled_reads.push((scene_path.clone(), *span, message.clone()));
            }
        }
        covered.extend(lute_check::colliding_occurrences(group));
    }
    for (path, result) in &mut file_results {
        result.diagnostics.retain(|d| {
            let quest_dup_covered =
                d.code == "E-QUEST-ID-DUP" && covered.iter().any(|(p, s)| p == path && *s == d.span);
            let envelope_reconciled = d.code == "E-MAYBE-UNSET"
                && reconciled_reads
                    .iter()
                    .any(|(p, s, m)| p == path && *s == d.span && *m == d.message);
            !quest_dup_covered && !envelope_reconciled
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
        if file_results.is_empty() {
            println!("lute: no .lute files found under {}", dir.display());
        }
        for (path, result) in &file_results {
            print_human(path, result);
        }
        if !project_diags.is_empty() {
            println!("project-wide diagnostics:");
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
        let project_error_count =
            project_diags.iter().filter(|(_, d)| d.severity == Severity::Error).count();
        let project_warning_count = project_diags.len() - project_error_count;
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

// ===========================================================================
// `lute scenario` (connectivity T14, dsl §5:571-584) — project-wide,
// read-only reporting surface over everything §4 computes. Evaluates no
// CEL, runs no Datalog, takes no mocks: pure graph math over declared
// structure, reusing [`collect_project_docs`]'s SAME per-root doc grouping
// `check-project` builds (never a second file-walk/parse) plus the SAME
// `lute_check::connectivity`/`envelope` analyses `check-project`'s own
// per-root pass calls (never duplicated math — only the presentation, and
// the omission of diagnostics, differ).
// ===========================================================================

/// A bare scene-key, `quest:<id>`, or `scene:<key>` node reference, parsed
/// from a `scenario reach`/`scenario envelope` CLI argument (dsl §4.4's
/// `envelope quest:<id>` syntax; `scene:<key>` is this branch's symmetric
/// counterpart -- see [`resolve_node_ref`]'s doc comment for why both
/// explicit prefixes exist).
enum NodeRef {
    Scene(String),
    Quest(String),
}

/// Parse an EXPLICIT `quest:<id>` / `scene:<key>` prefix only -- `None` for
/// a bare (unprefixed) string, which [`resolve_node_ref`] resolves against
/// actual project candidates instead of guessing. An explicit prefix is
/// always authoritative: `quest:foo` is ALWAYS a quest lookup and
/// `scene:foo` is ALWAYS a scene lookup, never re-tried as the other kind
/// (that would silently paper over a genuine "no such quest" typo).
fn parse_node_ref_prefix(raw: &str) -> Option<NodeRef> {
    if let Some(id) = raw.strip_prefix("quest:") {
        return Some(NodeRef::Quest(id.to_string()));
    }
    if let Some(key) = raw.strip_prefix("scene:") {
        return Some(NodeRef::Scene(key.to_string()));
    }
    None
}

fn node_ref_to_id(node: &NodeRef) -> lute_check::connectivity::NodeId {
    match node {
        NodeRef::Scene(key) => lute_check::connectivity::NodeId::Scene(key.clone()),
        NodeRef::Quest(id) => lute_check::connectivity::NodeId::Quest(id.clone()),
    }
}

/// Everything `lute scenario` needs for ONE resolved project root, built
/// from the SAME `lute_check::connectivity`/`envelope` analyses
/// `run_check_project`'s own per-root pass calls (T5/T6/T8/T9/T10) — never
/// re-derived independently. Unlike `check-project`, this never scans for
/// project diagnostics (`E-CONN-*`/`E-STATE-MAYBE-UNAVAILABLE`): a
/// read-only reporting surface, not a pass/fail gate (dsl §5:571-584).
struct RootScenario {
    graph: lute_check::connectivity::ConnGraph,
    reach: BTreeMap<lute_check::connectivity::NodeId, lute_check::connectivity::Reachability>,
    envs: BTreeMap<lute_check::connectivity::NodeId, envelope::Env>,
    tainted: BTreeSet<lute_check::connectivity::NodeId>,
    reads_per_scene: BTreeMap<String, Vec<(String, Span)>>,
    key_set: BTreeMap<String, Vec<(PathBuf, Span)>>,
    quest_ids: BTreeSet<String>,
    ambiguous_quests: BTreeSet<String>,
    unreachable_quests: BTreeSet<String>,
    /// `D` (dsl §4.3 spec lines 442-448): the project-resolved `run.*`/
    /// `user.*` schema-defaulted set, unioned across every doc's own
    /// resolved schema in this root — [`envelope::quest_envelope`]'s own
    /// defaults-only floor.
    envelope_d: BTreeSet<String>,
    /// This root's plain (doc-stripped-of-`FoldedEnv`) docs — quest
    /// envelope printing needs the `&Quest` struct itself
    /// ([`envelope::quest_envelope`]'s signature), never re-parsed here.
    docs: Vec<(PathBuf, lute_syntax::ast::Document)>,
}

/// Assemble [`RootScenario`] for one resolved root's docs — mirrors
/// `run_check_project`'s own per-root block (T5 `assemble_graph`, T6
/// `check_reachability`, T8/T9 `PerDocEffects`, T10 `propagate`) verbatim,
/// minus the diagnostic emission (`lute scenario` reports, never gates).
fn assemble_root_scenario(
    group_full: &DocGroup,
    file_results: &[(PathBuf, lute_check::CheckResult)],
) -> RootScenario {
    let docs: Vec<(PathBuf, lute_syntax::ast::Document)> =
        group_full.iter().map(|(p, d, _)| (p.clone(), d.clone())).collect();
    let key_set = lute_check::connectivity::scene_key_set(&docs);
    let quest_ids = lute_check::connectivity::quest_id_set(&docs);
    let (graph, _cycle_diags) =
        lute_check::connectivity::assemble_graph(&docs, &key_set, &quest_ids);
    let unreachable_quests = lute_check::connectivity::unreachable_quest_ids(&docs, file_results);
    let ambiguous_quests = lute_check::connectivity::ambiguous_quest_ids(&docs);
    let (reach, _reach_diags) = lute_check::connectivity::check_reachability(
        &graph,
        &quest_ids,
        &ambiguous_quests,
        &unreachable_quests,
    );

    let mut per_doc = envelope::PerDocEffects::default();
    let mut envelope_d: BTreeSet<String> = BTreeSet::new();
    let mut reads_per_scene: BTreeMap<String, Vec<(String, Span)>> = BTreeMap::new();
    for (_path, doc, folded) in group_full {
        envelope_d.extend(envelope::schema_defaults(&folded.env.state));
        for quest in &doc.quests {
            if quest.id.is_empty() || ambiguous_quests.contains(&quest.id) {
                continue;
            }
            per_doc.quest_writes_on_complete.insert(
                quest.id.clone(),
                envelope::writes_on_complete(quest, &folded.env.state),
            );
        }
    }
    for (key, occurrences) in &key_set {
        let Some((scene_path, _)) = occurrences.first() else { continue };
        let Some((_, doc, folded)) = group_full.iter().find(|(p, _, _)| p == scene_path) else {
            continue;
        };
        let all_nodes: Vec<lute_syntax::ast::Node> =
            doc.shots.iter().flat_map(|s| s.body.iter().cloned()).collect();
        let (_local_diags, assigned, reads) =
            check_definite_assignment(&all_nodes, &folded.env.state);
        // Same T4.4/T4.6 carry-forward parity fix as `run_check_project`'s
        // T11 wiring above (dsl §7 soundness invariant) -- `lute scenario
        // envelope`/`reach` must not classify a domain-exhaustive `<match>`
        // subject read as entry-dependent either.
        let exhaustive_spans =
            defassign::exhaustive_match_subject_spans(&all_nodes, &folded.env.state);
        let reads: Vec<(String, Span)> = reads
            .into_iter()
            .filter(|(_, span)| {
                !exhaustive_spans
                    .iter()
                    .any(|s| s.byte_start == span.byte_start && s.byte_end == span.byte_end)
            })
            .collect();
        per_doc.scene.insert(
            key.clone(),
            (envelope::guaranteed(&assigned), envelope::possible_writes(&all_nodes)),
        );
        reads_per_scene.insert(key.clone(), reads);
    }
    let (envs, tainted) = envelope::propagate(&graph, &per_doc, &envelope_d);

    RootScenario {
        graph,
        reach,
        envs,
        tainted,
        reads_per_scene,
        key_set,
        quest_ids,
        ambiguous_quests,
        unreachable_quests,
        envelope_d,
        docs,
    }
}

/// Find EVERY resolved root (sorted, deterministic — [`ByRoot`] is a
/// `BTreeMap`) whose docs declare `node` (a scene key in `scene_key_set` or
/// a declared `<quest id>`), returning each match's root path alongside its
/// assembled [`RootScenario`]. A scene/quest id is only unique WITHIN one
/// resolved project root (dsl §2.3/§6.3) — the SAME id may legitimately
/// exist in two independently resolved sibling roots (the bare `lute
/// scenario` graph view already shows both). Callers MUST treat 2+ matches
/// as an ambiguous lookup (Main review: never silently pick the
/// lexicographically-first root), never collapse to one.
fn find_matching_roots<'a>(
    by_root: &'a ByRoot,
    file_results: &[(PathBuf, lute_check::CheckResult)],
    node: &NodeRef,
) -> Vec<(&'a PathBuf, RootScenario)> {
    let mut out = Vec::new();
    for (root, group_full) in by_root {
        let scenario = assemble_root_scenario(group_full, file_results);
        let present = match node {
            NodeRef::Scene(key) => scenario.key_set.contains_key(key),
            NodeRef::Quest(id) => scenario.quest_ids.contains(id),
        };
        if present {
            out.push((root, scenario));
        }
    }
    out
}

/// Render a [`lute_check::PrereqFormula`] back to CEL-like text, fully
/// parenthesized so the `&&`/`||` nesting is always visible — a
/// `visited(A) || visited(B)` node is reachable via A OR B, never rendered
/// as a flat list that could blur that into "requires A and B" (Main
/// review: routes must never be flattened away).
fn format_prereq(f: &lute_check::PrereqFormula) -> String {
    match f {
        lute_check::PrereqFormula::Visited(key) => format!("visited({})", quote_cel_string(key)),
        lute_check::PrereqFormula::Completed(id) => {
            format!("completed({})", quote_cel_string(id))
        }
        lute_check::PrereqFormula::And(l, r) => {
            format!("({} && {})", format_prereq(l), format_prereq(r))
        }
        lute_check::PrereqFormula::Or(l, r) => {
            format!("({} || {})", format_prereq(l), format_prereq(r))
        }
    }
}

/// Quote+escape a `visited`/`completed` atom id for CEL-like rendering.
/// JSON string-literal escaping (`serde_json::to_string`) is a safe,
/// well-tested superset of what a CEL string literal needs
/// (backslash/quote/control-char escaping) — a raw `format!("\"{id}\"")`
/// interpolation (Main review) would render an id containing an embedded
/// `"`, `\`, or control character verbatim, breaking the printed
/// structure's own quoting. `String` -> JSON serialization is infallible
/// (a Rust `String` is always valid UTF-8, which `serde_json` always
/// accepts), so the `Result` is unwrapped unconditionally.
fn quote_cel_string(s: &str) -> String {
    serde_json::to_string(s).expect("String -> JSON serialization is infallible")
}

/// The reachability CLAIM for `node` (dsl §2.6: worded "under your declared
/// routes", never an unconditional runtime claim — Main review: the hedge
/// belongs on the claim itself). Falls back to the quest-lifecycle rules
/// ([`lute_check::connectivity::check_reachability`]'s own `Completed`
/// precedence, mirrored here as a standalone top-level query) when `node`
/// has no `reach` entry — a plain (no-`after`) quest is never a graph node
/// at all, and an empty `reach` map means the project's prerequisite graph
/// has a cycle (`E-CONN-CYCLE`; `assemble_graph` leaves `topo_order`/
/// `reach` empty in that case).
fn reach_verdict_text(scenario: &RootScenario, node: &lute_check::connectivity::NodeId) -> String {
    use lute_check::connectivity::{NodeId, Reachability};
    if let Some(r) = scenario.reach.get(node) {
        return match r {
            Reachability::Reachable => {
                "Reachable — a satisfiable route exists under your declared routes.".to_string()
            }
            Reachability::Unreachable => "Unreachable — no satisfiable route exists under your \
                 declared routes (E-CONN-UNREACHABLE, dsl §4.1)."
                .to_string(),
            Reachability::Unknown => "Unknown — this analysis cannot prove reachability either \
                 way under your declared routes."
                .to_string(),
        };
    }
    match node {
        NodeId::Quest(id) if scenario.ambiguous_quests.contains(id) => {
            "Unknown — ambiguous quest id (more than one declaration) under your declared \
             routes."
                .to_string()
        }
        NodeId::Quest(id) if scenario.unreachable_quests.contains(id) => {
            "Unreachable — quest lifecycle proves this quest can never complete \
             (E-QUEST-UNREACHABLE), under your declared routes."
                .to_string()
        }
        // Main review fix: an id referenced by a formula but never declared
        // anywhere in this root (E-CONN-UNKNOWN-NODE's own concern) must
        // read Unknown -- checked BEFORE the "plain quest, no `after`"
        // fallback below, since an undeclared id is trivially also absent
        // from `graph.nodes` and would otherwise be misreported Reachable.
        NodeId::Quest(id) if !scenario.quest_ids.contains(id) => {
            "Unknown — this quest id is not declared anywhere in this project root \
             (E-CONN-UNKNOWN-NODE), under your declared routes."
                .to_string()
        }
        NodeId::Quest(id) if !scenario.graph.nodes.contains_key(&NodeId::Quest(id.clone())) => {
            "Reachable — a plain quest with no declared `after` prerequisite, reachable by \
             default quest lifecycle under your declared routes."
                .to_string()
        }
        // Same fix for a `visited(Y)` atom targeting an undeclared scene
        // key -- every DECLARED scene is unconditionally a graph node
        // (`assemble_graph`), so only an undeclared key reaches here
        // without also being mid-cycle; checked before the cycle fallback.
        NodeId::Scene(key) if !scenario.key_set.contains_key(key) => {
            "Unknown — this scene key is not declared anywhere in this project root \
             (E-CONN-UNKNOWN-NODE), under your declared routes."
                .to_string()
        }
        _ => "Unknown — the project's prerequisite graph has a cycle (E-CONN-CYCLE); \
              reachability is unavailable under your declared routes."
            .to_string(),
    }
}

/// Print `node`'s declared `after` STRUCTURE (dsl §5:575) — the raw formula
/// shape, `&&`/`||` intact (Main review: never flattened into a
/// predecessor list that could misrepresent a disjunction as a joint
/// requirement), plus each directly-referenced node's own reachability as
/// supplementary context (explicitly labeled "referenced", never "route" —
/// the formula above IS the route structure).
fn print_prereq_structure(scenario: &RootScenario, node: &lute_check::connectivity::NodeId) {
    use lute_check::connectivity::PrereqState;
    match scenario.graph.nodes.get(node).map(|info| &info.prereq) {
        None | Some(PrereqState::Absent) => {
            println!("  after: (none declared) — this node is an entry point.");
        }
        Some(PrereqState::Invalid) => {
            println!("  after: (malformed — E-CONN-PROFILE; structure unavailable)");
        }
        Some(PrereqState::Valid(f)) => {
            println!("  after: {}", format_prereq(f));
            let mut targets: BTreeSet<lute_check::connectivity::NodeId> = BTreeSet::new();
            for atom in lute_check::atoms(f) {
                targets.insert(match atom {
                    lute_check::Atom::Visited(key) => lute_check::connectivity::NodeId::Scene(key),
                    lute_check::Atom::Completed(id) => lute_check::connectivity::NodeId::Quest(id),
                });
            }
            if !targets.is_empty() {
                println!(
                    "  referenced node(s) (see `after` above for the && / || structure — this \
                     is NOT a flat requirement list):"
                );
                for target in &targets {
                    println!("    - {target}: {}", reach_verdict_text(scenario, target));
                }
            }
        }
    }
}

/// Reduce an already-computed [`find_matching_roots`] result to exactly
/// ONE matching root, or `Err(ExitCode::from(2))` with a clear stderr
/// message when it is declared in ZERO roots (unknown node) or 2+ roots
/// (ambiguous — Main review: a scene/quest id is only unique WITHIN one
/// resolved root, dsl §2.3/§6.3; the SAME id may legitimately exist in
/// independent sibling roots, so this NEVER silently picks the
/// lexicographically-first one).
fn pick_unique_root<'a>(
    mut matches: Vec<(&'a PathBuf, RootScenario)>,
    dir: &Path,
    node_id_raw: &str,
) -> Result<(&'a PathBuf, RootScenario), ExitCode> {
    match matches.len() {
        0 => {
            eprintln!("lute: unknown node `{node_id_raw}` under {}", dir.display());
            Err(ExitCode::from(2))
        }
        1 => Ok(matches.pop().expect("len == 1")),
        n => {
            let roots: Vec<String> =
                matches.iter().map(|(r, _)| r.display().to_string()).collect();
            eprintln!(
                "lute: node `{node_id_raw}` is declared in {n} different project roots under \
                 {} -- ambiguous (a scene/quest id is only unique WITHIN one resolved project \
                 root, dsl §2.3/§6.3); narrow the directory argument to a single project root: \
                 {}",
                dir.display(),
                roots.join(", ")
            );
            Err(ExitCode::from(2))
        }
    }
}

/// Resolve `node_ref` to exactly ONE matching root's [`RootScenario`] --
/// thin wrapper: [`find_matching_roots`] then [`pick_unique_root`].
fn resolve_unique_root<'a>(
    dir: &Path,
    by_root: &'a ByRoot,
    file_results: &[(PathBuf, lute_check::CheckResult)],
    node_ref: &NodeRef,
    node_id_raw: &str,
) -> Result<(&'a PathBuf, RootScenario), ExitCode> {
    let matches = find_matching_roots(by_root, file_results, node_ref);
    pick_unique_root(matches, dir, node_id_raw)
}

/// Resolve a RAW `scenario reach`/`scenario envelope` CLI argument to its
/// [`NodeRef`] plus the single matching root's [`RootScenario`].
///
/// ## Why bare strings are never guessed
/// A scene's canonical key (`{character}.{episodeId}`,
/// `meta::canonical_episode_key`) is an UNVALIDATED, author-controlled
/// string — `character`/`episodeId` accept arbitrary YAML scalars, no
/// charset restriction — so a scene key CAN literally begin with
/// `quest:` (e.g. `character: "quest:foo"`). Unconditionally reserving
/// that prefix for quest lookups (the original design) would make such a
/// scene permanently unselectable. The fix:
/// - An EXPLICIT `quest:<id>` / `scene:<key>` prefix ([`parse_node_ref_prefix`])
///   is always authoritative — never re-tried as the other kind.
/// - A BARE (unprefixed) string is resolved against ACTUAL project
///   candidates: if it matches a declared scene key in some root, and/or
///   a declared quest id in some root. Exactly one kind matching → use
///   it (the overwhelmingly common case — no prefix needed at all).
///   BOTH kinds matching (some root has a scene key AND some root/the
///   same root has a quest id, both equal to the raw string) is
///   genuinely ambiguous — neither is silently preferred; the user is
///   told to disambiguate with an explicit prefix (mirrors
///   [`primary_node_ambiguity_note`]'s honesty pattern: never silently
///   pick one candidate over another equally-valid one).
fn resolve_node_ref<'a>(
    dir: &Path,
    by_root: &'a ByRoot,
    file_results: &[(PathBuf, lute_check::CheckResult)],
    node_id_raw: &str,
) -> Result<(NodeRef, &'a PathBuf, RootScenario), ExitCode> {
    if let Some(explicit) = parse_node_ref_prefix(node_id_raw) {
        return resolve_unique_root(dir, by_root, file_results, &explicit, node_id_raw)
            .map(|(root, scenario)| (explicit, root, scenario));
    }
    let scene_ref = NodeRef::Scene(node_id_raw.to_string());
    let quest_ref = NodeRef::Quest(node_id_raw.to_string());
    let scene_matches = find_matching_roots(by_root, file_results, &scene_ref);
    let quest_matches = find_matching_roots(by_root, file_results, &quest_ref);
    match (scene_matches.is_empty(), quest_matches.is_empty()) {
        (false, true) => pick_unique_root(scene_matches, dir, node_id_raw)
            .map(|(root, scenario)| (scene_ref, root, scenario)),
        (true, false) => pick_unique_root(quest_matches, dir, node_id_raw)
            .map(|(root, scenario)| (quest_ref, root, scenario)),
        (true, true) => {
            eprintln!("lute: unknown node `{node_id_raw}` under {}", dir.display());
            Err(ExitCode::from(2))
        }
        (false, false) => {
            eprintln!(
                "lute: node `{node_id_raw}` matches BOTH a scene key and a quest id in this \
                 project -- ambiguous (neither is silently preferred); disambiguate with an \
                 explicit `scene:{node_id_raw}` or `quest:{node_id_raw}` prefix",
            );
            Err(ExitCode::from(2))
        }
    }
}

/// `Some(message)` when the PRIMARY requested node itself is ambiguous
/// WITHIN its resolved root — a duplicated scene key
/// (`E-CONN-EPISODE-ID-DUP`, T3: 2+ scene documents computing the same
/// canonical key) or a duplicated quest id (`E-QUEST-ID-DUP`) — in which
/// case neither `reach` nor `envelope` has a single well-defined
/// declaration to report on. Callers MUST check this BEFORE any deeper
/// analysis so neither command ever silently displays one
/// arbitrarily-chosen declaration's data as if it were authoritative
/// (Main review: symmetric honesty treatment for scenes and quests —
/// `assemble_root_scenario`'s own `key_set[key].first()` / graph
/// admission both already pick an arbitrary declaration internally,
/// mirroring `assemble_graph`'s own "anchored at first occurrence"
/// precedent, which is fine for the underlying graph math but must never
/// be surfaced to the user as if it were an unambiguous answer).
fn primary_node_ambiguity_note(scenario: &RootScenario, node_ref: &NodeRef) -> Option<String> {
    match node_ref {
        NodeRef::Scene(key) => {
            let occurrences = scenario.key_set.get(key)?;
            (occurrences.len() > 1).then(|| {
                format!(
                    "ambiguous scene key (E-CONN-EPISODE-ID-DUP): `{key}` is computed by {} \
                     different scene documents in this project root, so a single \
                     reach/envelope report cannot be given.",
                    occurrences.len()
                )
            })
        }
        NodeRef::Quest(id) => scenario.ambiguous_quests.contains(id).then(|| {
            format!(
                "ambiguous quest id (E-QUEST-ID-DUP): `{id}` has more than one declaration in \
                 this project root, so a single reach/envelope report cannot be given."
            )
        }),
    }
}

fn run_scenario_reach(
    dir: &Path,
    by_root: &ByRoot,
    file_results: &[(PathBuf, lute_check::CheckResult)],
    node_id_raw: &str,
) -> ExitCode {
    let (node_ref, root, scenario) =
        match resolve_node_ref(dir, by_root, file_results, node_id_raw) {
            Ok(v) => v,
            Err(code) => return code,
        };
    let node_id = node_ref_to_id(&node_ref);
    println!("project root: {}", root.display());
    if let Some(note) = primary_node_ambiguity_note(&scenario, &node_ref) {
        println!("reach {node_id}: unavailable -- {note}");
        return ExitCode::SUCCESS;
    }
    println!("reach {node_id}:");
    println!("  verdict: {}", reach_verdict_text(&scenario, &node_id));
    print_prereq_structure(&scenario, &node_id);
    ExitCode::SUCCESS
}

fn print_path_set(set: &BTreeSet<String>) {
    if set.is_empty() {
        println!("    (none)");
    } else {
        for p in set {
            println!("    - {p}");
        }
    }
}

/// Print a scene node's Guaranteed/Possible envelope tables (T10) plus its
/// `Possible \ Guaranteed` warning-grade READS (contract #2): T11's
/// [`envelope::check_envelope`] already computes BOTH grades together and
/// returns them — `check-project` filters to `Severity::Error` only and
/// drops the warning grade; this RE-derives the SAME call, singleton-scoped
/// to `key` so every returned diagnostic necessarily belongs to this node,
/// and keeps the warning grade instead. Never a second classification pass
/// — `check_envelope` is reused verbatim, never re-implemented.
fn print_scene_envelope(scenario: &RootScenario, key: &str) {
    let node_id = lute_check::connectivity::NodeId::Scene(key.to_string());
    println!("envelope for {node_id}:");
    if scenario.tainted.contains(&node_id) {
        println!(
            "  note: this node's envelope is a defaults-only placeholder -- its `after` \
             formula is malformed or references an unresolved node (E-CONN-PROFILE/\
             E-CONN-UNKNOWN-NODE)."
        );
    }
    let env = scenario.envs.get(&node_id).cloned().unwrap_or_else(|| envelope::Env {
        guaranteed: scenario.envelope_d.clone(),
        possible: scenario.envelope_d.clone(),
    });
    println!("  Guaranteed (safe to read under your declared routes):");
    print_path_set(&env.guaranteed);
    println!("  Possible (set on at least one declared route reaching this node):");
    print_path_set(&env.possible);

    let mut single: BTreeMap<String, Vec<(String, Span)>> = BTreeMap::new();
    if let Some(reads) = scenario.reads_per_scene.get(key) {
        single.insert(key.to_string(), reads.clone());
    }
    let diags =
        envelope::check_envelope(&scenario.graph, &scenario.envs, &scenario.tainted, &single);
    println!(
        "  Possible \\ Guaranteed -- warning-grade reads (set on SOME but not every declared \
         route; suppressed by default in `check-project`, dsl §6, surfaced here per §5):"
    );
    let mut any = false;
    for (path, d) in &diags {
        if d.severity != Severity::Warning {
            continue;
        }
        any = true;
        println!("    - {}:{}:{}: {}", path.display(), d.span.line, d.span.column, d.message);
    }
    if !any {
        println!("    (none)");
    }
}

/// Print a quest node's envelope (T12 [`envelope::quest_envelope`]) — full
/// tables for an `after`-opted-in quest, defaults-only `D` plus the
/// enrichment note for a bare quest (dsl §4.4) — plus its `Possible \
/// Guaranteed` SET as plain inventory. [`envelope::check_envelope`] is
/// SCENE-ONLY by design (its own doc comment: quest reads stay
/// `check_quest_guard_defassign`'s territory), so this is NEVER labeled
/// as the T11 warning-grade read-site class (Main review) — there is no
/// read-SITE list for a quest at all, only the plain set difference.
fn print_quest_envelope(scenario: &RootScenario, id: &str, quest: &lute_syntax::ast::Quest) {
    let node_id = lute_check::connectivity::NodeId::Quest(id.to_string());
    println!("envelope for {node_id}:");
    let qe =
        envelope::quest_envelope(quest, &scenario.graph, &scenario.envs, &scenario.envelope_d);
    println!("  Guaranteed (safe to read under your declared routes):");
    print_path_set(&qe.env.guaranteed);
    println!("  Possible (set on at least one declared route reaching this node):");
    print_path_set(&qe.env.possible);
    let warn: BTreeSet<String> =
        qe.env.possible.difference(&qe.env.guaranteed).cloned().collect();
    println!(
        "  Possible \\ Guaranteed -- inventory only (paths set on SOME but not every declared \
         route reaching this quest, dsl §4.4). This is NOT the T11 warning-grade read-site \
         class -- quest read diagnostics are `check_quest_guard_defassign`'s separate \
         territory (that class is scene-only, see the scene envelope's own section):"
    );
    print_path_set(&warn);
    if qe.enrichment_note {
        println!(
            "  note: this quest declares no `after` attribute, so this is the defaults-only \
             `D` table (dsl §4.4); declaring `after` on quest:{id} would enrich this table \
             with the full project-resolved envelope."
        );
    }
}

fn run_scenario_envelope(
    dir: &Path,
    by_root: &ByRoot,
    file_results: &[(PathBuf, lute_check::CheckResult)],
    node_id_raw: &str,
) -> ExitCode {
    let (node_ref, root, scenario) =
        match resolve_node_ref(dir, by_root, file_results, node_id_raw) {
            Ok(v) => v,
            Err(code) => return code,
        };
    println!("project root: {}", root.display());
    if let Some(note) = primary_node_ambiguity_note(&scenario, &node_ref) {
        let node_id = node_ref_to_id(&node_ref);
        println!("envelope for {node_id}: unavailable -- {note}");
        return ExitCode::SUCCESS;
    }
    match &node_ref {
        NodeRef::Scene(key) => print_scene_envelope(&scenario, key),
        NodeRef::Quest(id) => {
            let Some(quest) =
                scenario.docs.iter().flat_map(|(_, d)| d.quests.iter()).find(|q| &q.id == id)
            else {
                eprintln!("lute: internal error: quest `{id}` resolved but no declaration found");
                return ExitCode::from(2);
            };
            print_quest_envelope(&scenario, id, quest);
        }
    }
    ExitCode::SUCCESS
}

/// Group `g`'s nodes into deterministic topological WAVES (Kahn's
/// algorithm, but collecting every currently-zero-in-degree node as ONE
/// layer at a time rather than draining a ready-queue one node at a time
/// like [`lute_check::connectivity::assemble_graph`]'s own internal
/// `topo_sort`) — a presentation concern specific to `lute scenario`'s
/// graph view, layered here rather than in `lute-check` (which only needs
/// the flat order). A node stuck in a prerequisite cycle never becomes
/// ready and is simply absent from every layer (already `E-CONN-CYCLE`'s
/// problem, reported by `check-project`, not this read-only view's).
fn topo_layers(
    g: &lute_check::connectivity::ConnGraph,
) -> Vec<Vec<lute_check::connectivity::NodeId>> {
    let mut in_degree: BTreeMap<lute_check::connectivity::NodeId, usize> =
        g.nodes.keys().map(|id| (id.clone(), 0)).collect();
    for targets in g.edges.values() {
        for target in targets {
            *in_degree.entry(target.clone()).or_insert(0) += 1;
        }
    }
    let mut layers = Vec::new();
    loop {
        let mut ready: Vec<lute_check::connectivity::NodeId> = in_degree
            .iter()
            .filter(|&(_, &d)| d == 0)
            .map(|(id, _)| id.clone())
            .collect();
        if ready.is_empty() {
            break;
        }
        ready.sort();
        for id in &ready {
            in_degree.remove(id);
            if let Some(targets) = g.edges.get(id) {
                for target in targets {
                    if let Some(d) = in_degree.get_mut(target) {
                        *d -= 1;
                    }
                }
            }
        }
        layers.push(ready);
    }
    layers
}

fn print_graph_for_root(root: &Path, graph: &lute_check::connectivity::ConnGraph) {
    println!("project root: {}", root.display());
    if graph.nodes.is_empty() {
        println!("  (no scene/quest nodes)");
        return;
    }
    let layers = topo_layers(graph);
    println!("  topological layers:");
    for (i, layer) in layers.iter().enumerate() {
        let names: Vec<String> = layer.iter().map(|n| n.to_string()).collect();
        println!("    layer {i}: {}", names.join(", "));
    }
    let layered: BTreeSet<lute_check::connectivity::NodeId> =
        layers.iter().flatten().cloned().collect();
    if layered.len() < graph.nodes.len() {
        let stuck: Vec<String> = graph
            .nodes
            .keys()
            .filter(|id| !layered.contains(id))
            .map(|n| n.to_string())
            .collect();
        println!(
            "    (unlayered -- part of a prerequisite cycle, E-CONN-CYCLE): {}",
            stuck.join(", ")
        );
    }
    println!("  edges (prerequisite -> dependent):");
    let mut printed_any = false;
    for (from, targets) in &graph.edges {
        for to in targets {
            println!("    {from} -> {to}");
            printed_any = true;
        }
    }
    if !printed_any {
        println!("    (none)");
    }
}

fn run_scenario_graph(by_root: &ByRoot) -> ExitCode {
    if by_root.is_empty() {
        println!("lute: no .lute files found");
        return ExitCode::SUCCESS;
    }
    for (root, group_full) in by_root {
        let docs: Vec<(PathBuf, lute_syntax::ast::Document)> =
            group_full.iter().map(|(p, d, _)| (p.clone(), d.clone())).collect();
        let key_set = lute_check::connectivity::scene_key_set(&docs);
        let quest_ids = lute_check::connectivity::quest_id_set(&docs);
        let (graph, _cycle_diags) =
            lute_check::connectivity::assemble_graph(&docs, &key_set, &quest_ids);
        print_graph_for_root(root, &graph);
    }
    ExitCode::SUCCESS
}

/// `lute scenario` dispatch (dsl §5:571-584): reuses [`collect_project_docs`]
/// — the SAME per-root doc collection `check-project` builds — then routes
/// to the bare graph view, `reach`, or `envelope`.
fn run_scenario(
    dir: &Path,
    providers: Option<&Path>,
    command: Option<ScenarioCommand>,
) -> ExitCode {
    let (file_results, by_root) = match collect_project_docs(dir, providers) {
        Ok(v) => v,
        Err(code) => return code,
    };
    match command {
        None => run_scenario_graph(&by_root),
        Some(ScenarioCommand::Reach { node_id }) => {
            run_scenario_reach(dir, &by_root, &file_results, &node_id)
        }
        Some(ScenarioCommand::Envelope { node_id }) => {
            run_scenario_envelope(dir, &by_root, &file_results, &node_id)
        }
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
    // dsl 0.5.1 §2: the reserved `quest.<id>.state` / `quest.<id>.objectives.<oid>.done`
    // paths this document actually REFERENCES (any CEL slot) — reuses `lute-trace`'s
    // own walk ([`lute_trace::collect_referenced_reserved_quest_paths`], §1.1's
    // "does the document reference this exact path" test) so `context` never
    // diverges from what `trace --state` admits on a reserved path.
    let reserved_quest_paths = lute_trace::collect_referenced_reserved_quest_paths(&doc);
    let surface = authoring_surface(
        &input,
        &folded.env.state,
        &folded.env.rel_vocab,
        &branch_paths,
        &reserved_quest_paths,
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
/// verbatim, no new resolution. `reserved_quest_paths` (dsl 0.5.1 §2) is the
/// set of reserved `quest.<id>.state`/`quest.<id>.objectives.<oid>.done`
/// paths this document actually REFERENCES (already computed by the
/// caller via `lute_trace::collect_referenced_reserved_quest_paths`) —
/// surfaced under its OWN `reservedQuestPaths` key, clearly separate from
/// the ordinary (author-declared/folded) `stateSchema`: these paths are
/// never declared by the document, only implicitly readable.
fn authoring_surface(
    input: &CheckInput,
    state: &lute_check::StateSchema,
    rel_vocab: &RelVocab,
    branch_paths: &BTreeSet<String>,
    reserved_quest_paths: &BTreeSet<String>,
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

    // dsl 0.5.1 §2: the reserved quest paths this document actually
    // REFERENCES (`reserved_quest_paths`, already a `BTreeSet` ⇒ path-sorted),
    // each carrying its fixed reserved-namespace domain (§1) the same way an
    // ordinary `stateSchema` entry carries its `domain` — kept under its OWN
    // key, never merged into `stateSchema`, since these paths are implicit
    // (the document never declares them).
    let reserved_quest_paths_json: Vec<Value> = reserved_quest_paths
        .iter()
        .map(|path| {
            let (ty, domain) = reserved_quest_path_type(path);
            let mut o = Map::new();
            o.insert("path".into(), path.clone().into());
            o.insert("type".into(), ty.into());
            o.insert("namespace".into(), "quest".into());
            if let Some(dom) = domain {
                o.insert("domain".into(), dom.into());
            }
            Value::Object(o)
        })
        .collect();

    // dsl 0.5.1 §3: the fixed, always-present set of content-line delivery
    // flags — `{mono}`/`{os}`/`{vo}` — with their normative meanings, in
    // spec declaration order.
    let delivery_flags: Vec<Value> = [
        ("mono", "interior monologue / thought (not spoken aloud in-scene)"),
        (
            "os",
            "off-screen: the speaker is heard but not currently staged/visible",
        ),
        (
            "vo",
            "voiceover: narration-style delivery layered over the scene",
        ),
    ]
    .into_iter()
    .map(|(flag, meaning)| {
        let mut o = Map::new();
        o.insert("flag".into(), flag.into());
        o.insert("meaning".into(), meaning.into());
        Value::Object(o)
    })
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
    // dsl 0.5.1 §2/§3: the referenced reserved quest paths and the fixed
    // delivery-flag vocabulary — new, always-present authoring-surface keys.
    root.insert("reservedQuestPaths".into(), reserved_quest_paths_json.into());
    root.insert("deliveryFlags".into(), delivery_flags.into());
    Value::Object(root)
}

/// The domain of a reserved quest path (dsl 0.2.0 §5.2 / 0.5.1 §1): a
/// `quest.<id>.state` path is the fixed lifecycle enum
/// `active`/`complete`/`failed`/`unset`; a `quest.<id>.objectives.<oid>.done`
/// path is a plain `bool` (no domain, mirroring `state_type_str`'s scalar
/// arms). The shape mirrors `lute-trace`'s own reserved-path shape test
/// (`is_reserved_quest_path`, dsl 0.2.0 §5.2) — this function is only ever
/// called on a path already known (by construction of
/// `reserved_quest_paths`) to match one of the two reserved shapes, so no
/// third arm is needed.
fn reserved_quest_path_type(path: &str) -> (&'static str, Option<Vec<String>>) {
    if path.ends_with(".state") {
        (
            "enum",
            Some(vec![
                "active".to_string(),
                "complete".to_string(),
                "failed".to_string(),
                "unset".to_string(),
            ]),
        )
    } else {
        ("bool", None)
    }
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
/// members, state paths (with enum domains), the referenced reserved quest
/// paths (dsl 0.5.1 §2), the relational vocabulary (entity kinds, relations
/// w/ arity+domains+`derive`, seed facts, rules, project-level enums), the
/// fixed delivery-flag vocabulary (dsl 0.5.1 §3), and component names.
/// `--json` is the machine surface; this is a short at-a-glance view.
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
    // dsl 0.5.1 §2: the reserved quest paths this document REFERENCES —
    // kept as its own section, clearly separate from the ordinary
    // (author-declared/folded) `stateSchema` above; omitted entirely when
    // the document references none (the reserved namespace is unbounded).
    if let Some(reserved) = surface["reservedQuestPaths"].as_array() {
        if !reserved.is_empty() {
            let _ = writeln!(out, "reservedQuestPaths ({}):", reserved.len());
            for s in reserved {
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
    }
    // dsl 0.5.1 §3: the fixed `{mono}`/`{os}`/`{vo}` delivery-flag
    // vocabulary — always present (a document either uses a flag or
    // doesn't; the set itself is fixed and never varies per document).
    if let Some(flags) = surface["deliveryFlags"].as_array() {
        let _ = writeln!(out, "deliveryFlags ({}):", flags.len());
        for f in flags {
            let flag = f["flag"].as_str().unwrap_or("");
            let meaning = f["meaning"].as_str().unwrap_or("");
            let _ = writeln!(out, "  {{{flag}}}: {meaning}");
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
