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
    check, check_definite_assignment, check_project_quest_ids, check_project_quest_refs, defassign,
    envelope, fold_env, CheckInput, Mode, Namespace, RelVocab,
};
use lute_core_span::{Diagnostic, Severity, Span, TextIndex};
use lute_manifest::core::load_core_snapshot;
use lute_manifest::project::{
    load_project, resolve_document_snapshot, resolve_permissions, ResolveDiag,
};
use lute_manifest::provider::{ProviderSet, ProviderSnapshot};
use lute_manifest::relations::KindShape;
use lute_manifest::snapshot::CapabilitySnapshot;
use lute_manifest::types::{Literal, Type};
use lute_trace::{merge, parse_mock_yaml, MockSet, TraceExit, TraceReport};
use rayon::prelude::*;

/// Append one formatted line to an output buffer — the EPIPE-safe
/// replacement for `println!` in a report that is written once through
/// [`write_stdout`] (T3-15: `println!` panics when the reader of a pipe goes
/// away, e.g. `lute scenario … | head`). Writing into a `String` cannot fail.
macro_rules! outln {
    ($out:expr) => {
        $out.push('\n')
    };
    ($out:expr, $($arg:tt)*) => {{
        use std::fmt::Write as _;
        let _ = writeln!($out, $($arg)*);
    }};
}

mod beats_cmd;
mod compile_all;
mod context;
mod doctor;
mod explain;
mod input_cache;
mod knowledge;
mod lint;
mod loc;
mod lore_report;
mod manifests;
mod mockcheck;
mod play;
mod play_expect;
mod refs;
mod rewrite;
mod runner;
mod scaffold;
mod scenario_fmt;
mod stream;
mod testcmd;

use input_cache::InputCache;

#[derive(Parser)]
#[command(
    name = "lute",
    version,
    about = "Checker, compiler, and toolchain for .lute branching game narratives"
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
        /// Trusted project profile whose permissions apply as an additional host
        /// ceiling. This never activates the profile's plugins or rewrites source.
        #[arg(long = "permission-profile", value_name = "NAME")]
        permission_profile: Option<String>,
        /// Promote every diagnostic with EXACTLY this code to an error for the
        /// verdict and exit code (repeatable) — rustc/clippy `-D` precedent
        /// (spec §5). An unknown code is a usage error (exit 2). Errors are
        /// never demotable (spec §6).
        #[arg(long = "deny", value_name = "CODE", value_parser = parse_deny_code)]
        deny: Vec<String>,
        /// Promote EVERY warning to an error for the verdict and exit code
        /// (spec §5).
        #[arg(long = "deny-warnings")]
        deny_warnings: bool,
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
        /// Promote every diagnostic with EXACTLY this code to an error for the
        /// verdict and exit code (repeatable) — applies to per-file AND
        /// project-wide diagnostics (spec §5). An unknown code is a usage error
        /// (exit 2). Errors are never demotable (spec §6).
        #[arg(long = "deny", value_name = "CODE", value_parser = parse_deny_code)]
        deny: Vec<String>,
        /// Promote EVERY warning to an error for the verdict and exit code
        /// (spec §5).
        #[arg(long = "deny-warnings")]
        deny_warnings: bool,
        /// Work in progress (dsl 0.23.0 §10): report `E-ENTRY-UNREACHABLE`,
        /// `E-BEAT-UNREACHABLE`, and `E-OBJECTIVE-UNSATISFIABLE` as warnings
        /// when only relations nothing produces yet (no seed, assert, rule,
        /// or reserved declaration) make the guard dead. dsl 0.26.0 §2.6: a
        /// relation only a component `::assert` with an unbound `@param`
        /// writes counts as unproduced for specific arguments, and a
        /// required `<objective quest=…>` on a child dead only for that
        /// reason is a warning too. A relation with no such component
        /// producer whose producers can never match stays an error.
        #[arg(long)]
        wip: bool,
    },
    /// Run configurable content/metric advisory lints over a `.lute`
    /// document or a directory tree (https://lute-lang.vercel.app/tooling/linting/).
    /// Distinct from `lute check`: this surface publishes advisory `L-*`
    /// findings governed by `<project root>/lute.lint.yaml` (rule levels,
    /// thresholds, ignore globs, project-local `custom:` rules) and never
    /// touches artifact identity. Documents are grouped by their nearest
    /// ancestor `lute.project.yaml` exactly as `check-project` does. Exit `0` clean
    /// or only sub-error findings, `1` any Error-severity finding
    /// (native or `--deny`-promoted, including `E-LINT-CONFIG`/
    /// `E-LINT-EXPR`), `2` I/O, malformed YAML, or usage.
    Lint {
        /// File or directory to lint (default: current directory).
        #[arg(default_value = ".")]
        path: PathBuf,
        /// Emit the structured report as JSON instead of human lines.
        #[arg(long)]
        json: bool,
        /// Promote every diagnostic with EXACTLY this code to an error
        /// (repeatable). Lint codes are dynamic (`L-*` from plugin/custom
        /// rule ids), so this accepts any `L-<CODE>` plus
        /// `E-LINT-CONFIG`/`E-LINT-EXPR`/`E-LINT-RULE`; a typo is a
        /// clap usage error (exit 2), never a silent no-op (spec §5).
        #[arg(long = "deny", value_name = "CODE", value_parser = lint::parse_lint_deny_code)]
        deny: Vec<String>,
        /// Promote EVERY warning to an error for the verdict and exit
        /// code (spec §5).
        #[arg(long = "deny-warnings")]
        deny_warnings: bool,
        /// Use this `lute.lint.yaml` instead of the per-root default
        /// (`<project root>/lute.lint.yaml`). Applied to every root in
        /// the target's grouping.
        #[arg(long = "config", value_name = "PATH")]
        config: Option<PathBuf>,
    },
    /// Compile a checked `.lute` document to its JSON command-record artifact,
    /// or — with `--all` — every document in a project plus a
    /// `project.index.json` unioning their vocabularies.
    Compile {
        /// Path to the `.lute` file to compile. Ignored (and optional) under
        /// `--all`, which takes its documents from `--project` instead.
        file: Option<PathBuf>,
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
        /// Trusted project profile whose permissions apply as an additional host
        /// ceiling to the selected document, or every document under `--all`.
        /// This never activates the profile's plugins or rewrites source.
        #[arg(long = "permission-profile", value_name = "NAME")]
        permission_profile: Option<String>,
        /// Write the artifact here instead of stdout. Under `--all` this is a
        /// required output DIRECTORY, not a file.
        #[arg(short = 'o', long = "out", value_name = "FILE")]
        out: Option<PathBuf>,
        /// Compile EVERY `*.lute` document under `--project <dir>` into
        /// `-o <dir>`, mirroring the project's own directory layout, and write
        /// a `project.index.json` whose `entities`/`enums`/`relations`/
        /// `seedFacts`/`rules`/`prereqEdges` are the UNION across all of them
        /// (`docs/runtime/execution-model.md` requires an engine to compute
        /// exactly that union before it can evaluate anything). Requires BOTH
        /// `--project` and `-o`; any other combination is a usage error.
        #[arg(long)]
        all: bool,
        /// Merge a locale bundle (`lute loc import`) into the artifact:
        /// `texts` on every line, `labels` on every choice/hub option, keyed by
        /// `lineId` (dsl 0.8.0 §7). `text`/`label` stay the source language.
        /// A record missing a declared locale is `W-L10N-MISSING`.
        #[arg(long, value_name = "FILE")]
        locales: Option<PathBuf>,
        /// Promote every diagnostic with EXACTLY this code to an error for the
        /// verdict and exit code (repeatable) — the same §5 policy `check`
        /// applies, over the warnings compile itself emits (`W-L10N-MISSING`).
        #[arg(long = "deny", value_name = "CODE", value_parser = parse_deny_code)]
        deny: Vec<String>,
        /// Promote EVERY warning to an error for the verdict and exit code
        /// (spec §5).
        #[arg(long = "deny-warnings")]
        deny_warnings: bool,
    },
    /// Incrementally compile body text from stdin against a checked scene
    /// prefix, flushing ordinary artifact snapshots as newline-delimited JSON.
    CompileStream {
        /// Path to the immutable `.lute` scene prefix.
        file: PathBuf,
        /// Directory of pinned provider snapshots to resolve ids against.
        #[arg(long, value_name = "DIR")]
        providers: Option<PathBuf>,
        /// Project directory (`lute.project.yaml` + `plugins/`) resolving the
        /// prefix's capability snapshot, defaults, components, and identity.
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
        /// Trusted project profile whose permissions apply as one frozen host
        /// ceiling to both the prefix and every appended body unit.
        #[arg(long = "permission-profile", value_name = "NAME")]
        permission_profile: Option<String>,
    },
    /// Back-fill a stable `code` into every untagged `:line` (dsl §12),
    /// rewriting the file in place — or every `.lute` file under a directory
    /// (recursive, sorted; dsl 0.22.0 §13).
    Tag {
        /// The `.lute` file to tag, or a directory to tag recursively.
        path: PathBuf,
        /// FORCE-renumber every line's `code` in clean document order
        /// (0010/0020/… per speaker per scope), rewriting existing codes.
        /// A drafting tool: refused when frontmatter declares `codesLocked:`
        /// (published codes key `lineId`/`voiceKey` — renumbering breaks the
        /// localization/voice join).
        #[arg(long)]
        force: bool,
    },
    /// Apply the mechanical, meaning-preserving migrations in place —
    /// `:line[speaker]{…}: text` → `@speaker{…}: text`, any other content
    /// line's leading `:` sigil → `@` (dsl §7.1, foundation C1),
    /// `<choice>`/`<hub>` choice `as="…"` → `into="…"` (dsl §7.3), and a
    /// literal-comparison `<when test="$ == …">` → `<when is="…">` (dsl
    /// 0.18.0 §3). Byte-exact and comment-preserving; writes back only when
    /// something changed. Exit `0` on success, `2` on an I/O failure. A
    /// directory migrates every `.lute` file under it (recursive, sorted).
    Fix {
        /// The `.lute` file to migrate, or a directory to migrate recursively.
        path: PathBuf,
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
        /// Trusted project profile whose permissions restrict the reported
        /// authoring surface without activating that profile's plugins.
        #[arg(long = "permission-profile", value_name = "NAME")]
        permission_profile: Option<String>,
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
        /// Raise an occasion after the quest walk settles, in CLI order
        /// (repeatable): each raise judges the `<objective on="<occasion>">`
        /// objectives of every active quest (dsl 0.21.0 §7a.2).
        #[arg(long = "occasion", value_name = "OCCASION")]
        occasion: Vec<String>,
        /// A YAML document carrying the same surfaces (`state:`/`facts:`/
        /// `choose:`/`events:`/`accepts:`/`visited:`/`occasions:`, dsl 0.4.0
        /// §4.3, 0.21.0 §7a); CLI flags compose with it, the flag winning on
        /// a conflict.
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
        /// Present ONE `<entry>` of a `kind: lore` document by id (dsl
        /// 0.19.0 §8): its lines, the `<match>` arm taken, and the
        /// `::set`/`::assert`/`::retract` a first read applies — or skips,
        /// when the mock seeds `entry.<id>.read: true`. REQUIRED for a lore
        /// document without `--beat` (it has no sequence to walk);
        /// `E-TRACE-ENTRY` (exit 1) on a non-lore document or an unknown id.
        #[arg(long, value_name = "ID")]
        entry: Option<String>,
        /// Present ONE bundle `<beat>` of a `kind: lore` document (dsl
        /// 0.23.0 §4), by its local id or its canonical `<document id>.<beat
        /// id>`: its body walked like a scene shot body (`--choose` picks),
        /// every effect applied, its `when` shown, not enforced.
        /// `E-TRACE-BEAT` (exit 1) on a non-lore document or an unknown id.
        #[arg(long, value_name = "ID", conflicts_with = "entry")]
        beat: Option<String>,
        /// Do not apply the project's seed facts and Datalog rules (dsl
        /// 0.22.0 §6): an unmocked derived atom is unknown and each derived
        /// read is noted, as in 0.21. Overrides the mock's `derive:`.
        #[arg(long)]
        no_derive: bool,
        /// Show every `<match>` subject and guard with its `@def`/`$`
        /// expanded, as the walk evaluated it; by default they read as the
        /// author wrote them (`@weekday`, `@atLeast(2)`). `--json` always
        /// carries the expansion, plus `authoredId`/`authoredGuard`.
        #[arg(long)]
        expand: bool,
    },
    /// Provider-catalog maintenance.
    #[command(subcommand)]
    Catalog(CatalogCommand),
    /// Scaffold a new Lute project directory: `lute.project.yaml`, a state
    /// schema, a starter vocabulary, starter documents, and ways to exercise
    /// them — ready for `lute check-project`.
    Init {
        /// Directory to create (must not already contain a `lute.project.yaml`).
        dir: PathBuf,
        /// Project template: `minimal` (default), `investigation` (a
        /// whodunit on the `beats` skeleton: `examine`/`interview`
        /// occasions, evidence lore, a negated derived rule, an accusation
        /// guarded by derived facts), or `beats` (an occasions plugin, beats
        /// with `id:`, a quest, lore, a play script and scenario tests, dsl
        /// 0.22.0 §13).
        #[arg(long, value_name = "NAME")]
        template: Option<String>,
    },
    /// Scaffold one new document into an existing project: `lute new scene
    /// <name> [--on <occasion> [--target <target>]]` / `lute new quest
    /// <name> [--start]` / `lute new lore <name>` / `lute new schema <name>`.
    /// Documents carry an `id:` (a dotted name keeps its dots: `isolde.night`
    /// → `id: isolde.night`) and omit what the manifest's `defaults:`
    /// supplies; outside a project `lute new` says so (and `--on` is
    /// refused).
    New {
        /// Document kind: `scene`, `quest`, `lore`, or `schema`.
        kind: String,
        /// The new document's name (file stem; `/` nests it in a subfolder:
        /// `lute new scene talk/tomas-evening`).
        name: String,
        /// The PROJECT directory (default: current directory) — not the
        /// destination folder. A directory inside a project that is not its
        /// root is refused with the `<sub>/<name>` spelling to use instead.
        #[arg(long, value_name = "PROJECT")]
        dir: Option<PathBuf>,
        /// Make the scene a beat answering this occasion (dsl 0.21.0 §3).
        #[arg(long, value_name = "OCCASION")]
        on: Option<String>,
        /// The target the beat answers for (a targeted occasion's
        /// `<prefix>.<member>`).
        #[arg(long, value_name = "TARGET", requires = "on")]
        target: Option<String>,
        /// `new quest` only: scaffold an auto-starting quest (`start="true"`)
        /// instead of the default accept-driven stub.
        #[arg(long)]
        start: bool,
    },
    /// Diagnose the local toolchain + project setup: versions, project
    /// manifest, provider snapshots, vocabulary slots, and editor integration
    /// hints. Exit `0` (it reports, never gates) unless `--strict`.
    Doctor {
        /// Project directory to inspect (default: current directory).
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Emit the report as JSON instead of human checklist lines.
        #[arg(long)]
        json: bool,
        /// Exit `1` when any check fails (a `✗`) — a stale running
        /// `lute-lsp`, another build beside `lute`, a stale snapshot, … — so
        /// a harness can refuse to start on a broken setup (dsl 0.26.0 §8).
        #[arg(long)]
        strict: bool,
    },
    /// Execute a COMPILED artifact (`lute compile` output) headlessly against
    /// a mock playthrough — the reference consumer of the runtime contract
    /// (docs/runtime/): command dispatch, CEL guards, facts + Datalog
    /// fixpoint, hubs, and quest lifecycle. Distinct from `lute trace`, which
    /// previews SOURCE; `run` consumes the artifact an engine would.
    Run {
        /// Path to the compiled artifact JSON.
        artifact: PathBuf,
        /// A YAML mock playthrough (same surfaces as `lute trace --mock`).
        #[arg(long, value_name = "FILE")]
        mock: Option<PathBuf>,
        /// Raise an occasion after a quest artifact's walk settles, after
        /// the mock's own `occasions:`, in CLI order (repeatable): each raise
        /// judges the `on="<occasion>"` objectives of every active quest
        /// (dsl 0.21.0 §7a.2).
        #[arg(long = "occasion", value_name = "OCCASION")]
        occasion: Vec<String>,
        /// Emit the machine-readable transcript as JSON.
        #[arg(long)]
        json: bool,
        /// Present ONE `entry` record of a lore artifact by id (dsl 0.19.0
        /// §8, docs/runtime/lore-entries.md). A lore artifact needs exactly
        /// one of `--entry`/`--beat` (exit 2 otherwise); refused (exit 2) on
        /// any other artifact kind.
        #[arg(long, value_name = "ID")]
        entry: Option<String>,
        /// Present ONE bundle `beat` record of a lore artifact (dsl 0.23.0
        /// §4) by canonical id `<document id>.<beat id>` (or the bare beat id
        /// when unambiguous): its body segment runs like a scene, every
        /// effect applied. Refused (exit 2) on any other artifact kind.
        #[arg(long, value_name = "ID", conflicts_with = "entry")]
        beat: Option<String>,
    },
    /// Play a story through a whole project as a sequence of raised
    /// occasions (dsl 0.21.0 §6). Compiles the WHOLE project in memory
    /// (scene, quest and lore documents — the gate and declaration union
    /// `compile --all` uses), then for every script step computes the beats
    /// answering the occasion, each candidate's verdict (`once` spending,
    /// `after:` over the live visited/completed/active sets, `when` through
    /// `lute run`'s evaluator), orders the eligible ones by priority then
    /// index order, presents the winner (or the step's `pick` on a `select:
    /// all` occasion) with the reference runner, and advances every quest
    /// lifecycle. A `::end` ends the presentation it runs in; the play goes
    /// on with the next step — a step `end: true` ends the playthrough.
    /// Exit `0` a complete walk, `1` a
    /// failed project compile or an ineligible `pick`, `2` an I/O/usage
    /// failure (a malformed script, an unknown occasion), `3` an incomplete
    /// walk (an unscripted choice/hub, or a `when`/effect this reference
    /// runtime cannot decide).
    Play {
        /// Project directory (`lute.project.yaml`) to play.
        dir: PathBuf,
        /// The play script (`*.play.yaml`): `steps:` — the occasions to
        /// raise (`{occasion, target?, pick?}`) and `{newRun: true}`
        /// boundaries — plus `state:`/`facts:` seeds and `choose:` branch
        /// decisions in the trace-mock grammar.
        #[arg(long, value_name = "FILE")]
        script: PathBuf,
        /// Emit the machine-readable transcript as JSON.
        #[arg(long)]
        json: bool,
        /// Do not apply the project's Datalog rules (dsl 0.22.0 §6): an
        /// unmocked derived atom is unknown and halts the walk incomplete.
        #[arg(long)]
        no_derive: bool,
        /// After the play, print the derivation tree of a ground atom — the
        /// rule used and each premise's own support, negated premises shown
        /// absent — or, when it does not hold, the failing premises of every
        /// rule that could conclude it (repeatable, dsl 0.22.0 §6).
        #[arg(long, value_name = "ATOM")]
        explain: Vec<String>,
        /// Print staging as the lowered IR records (`::background`,
        /// `::sprite`, injected preloads and pose resets) instead of the
        /// authored directives (`::bg`, `::auto`, …).
        #[arg(long)]
        ir: bool,
    },
    /// Run the project's scenario tests: every `*.test.yaml` under `dir`
    /// traces its scene (or, with `entry:`/`entries:`, presents its lore
    /// entries) against the declared mocks and asserts the declared
    /// expectations (transcript, offered options, state, quest status); every
    /// `*.play.yaml` under `dir` that carries an `expect:` is played and
    /// judged as `lute play` does (dsl 0.22.0 §4). Resolves each traced
    /// document identically to `lute trace` ([`build_input`]): with
    /// `--project`, the document's `profile:`/`plugins:` frontmatter and the
    /// manifest's `defaults: uses:` hoist are both applied before tracing;
    /// without it, a core-only (`lute.core`) resolution, unchanged from
    /// before. A play runs `--project`, else the nearest `lute.project.yaml`
    /// above it.
    Test {
        /// Directory to walk for `*.test.yaml` scenario tests and
        /// expect-carrying `*.play.yaml` plays (default: `.`).
        #[arg(default_value = ".")]
        dir: PathBuf,
        /// Emit the machine-readable report as JSON.
        #[arg(long)]
        json: bool,
        /// Directory of pinned provider snapshots to resolve ids against.
        #[arg(long, value_name = "DIR")]
        providers: Option<PathBuf>,
        /// Project directory (`lute.project.yaml` + `plugins/`) whose
        /// installed plugins resolve each traced document's activated
        /// capability snapshot (plugin §4/§11). Omit for a core-only
        /// (`lute.core`) test.
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
        /// Also report branch/arm coverage across the tested documents, and
        /// the project's documents no test traced and no play presented.
        #[arg(long)]
        coverage: bool,
        /// Do not apply the project's seed facts and Datalog rules (dsl
        /// 0.22.0 §6): an unmocked derived atom is unknown, as in 0.21.
        /// Overrides every test's and play script's own `derive:`.
        #[arg(long)]
        no_derive: bool,
    },
    /// Localization & production reporting over a project's content lines.
    #[command(subcommand)]
    Loc(LocCommand),
    /// The project's world-narrative map (dsl 0.19.0 §8): lore entries
    /// grouped by `target` and by `series` (in `order`), and for every
    /// relation asserted anywhere, which ground facts lore entries reveal,
    /// which scenes/quests reveal, and which both. Read-only; documents need
    /// not check clean. Exit `0` on success, `2` on an I/O failure.
    Lore {
        /// Directory to walk recursively for `*.lute` files.
        dir: PathBuf,
        /// Emit the report as JSON instead of human-readable lines.
        #[arg(long)]
        json: bool,
    },
    /// Who uses which engine content id (dsl 0.26.0 §2.5): every value of a
    /// directive attribute (`--attr give.item`) or every target of a reward
    /// kind (`--reward ITEM`), with the documents and lines using it — so a
    /// lead sees "who gives what" before a merge. Read-only; documents need
    /// not check clean. Exit `0` on success, `2` on an I/O or usage failure.
    Refs {
        /// Directory to walk recursively for `*.lute` files.
        dir: PathBuf,
        /// A directive attribute, `<directive>.<attr>` (repeatable).
        #[arg(long, value_name = "DIRECTIVE.ATTR")]
        attr: Vec<String>,
        /// A reward kind whose `target=` values to list (repeatable); a
        /// reward without a target is listed as `(no target)`.
        #[arg(long, value_name = "KIND")]
        reward: Vec<String>,
        /// Emit the report as JSON instead of human-readable lines.
        #[arg(long)]
        json: bool,
    },
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
        /// Output format: `text` (default), `json`, or `dot` (Graphviz).
        #[arg(long, value_name = "FORMAT")]
        format: Option<String>,
        /// Graph view: also draw fact-producer edges (dsl 0.26.0 §8) —
        /// `scene(A) -> scene(B) [hasItem(x)]` when B's gate (`when:` /
        /// `start=`) reads `holds(F)` and A asserts `F` (or a fact the rules
        /// deriving `F` need) — and layer over them where they close no
        /// cycle, so progress gated by facts shows in the layers.
        #[arg(long)]
        facts: bool,
        /// `reach`/`envelope` sub-view; omitted -> prints the assembled
        /// topological graph (dsl §5:574).
        #[command(subcommand)]
        command: Option<ScenarioCommand>,
    },
    /// Every beat of the project, per occasion and target, in selection
    /// order (priority descending, then project order) with its priority,
    /// `once`, `after:`, `when` and the static verdicts `check-project`
    /// reaches about it — unreachable, shadowed, tied, once-per-run over
    /// user state (dsl 0.23.0 §1). Read-only; documents need not check
    /// clean. Exit `0` on success, `2` on an I/O failure or an unknown
    /// `--occasion`.
    Beats {
        /// Directory to walk recursively for `*.lute` files.
        dir: PathBuf,
        /// Only this occasion's ladders (repeatable).
        #[arg(long, value_name = "OCCASION")]
        occasion: Vec<String>,
        /// Only the ladders raised for this target (repeatable).
        #[arg(long, value_name = "TARGET")]
        target: Vec<String>,
        /// Emit the report as JSON instead of human-readable lines.
        #[arg(long)]
        json: bool,
        /// Show each `when:` with its `@def`s expanded instead of as the
        /// author wrote it (`--json` always carries both).
        #[arg(long)]
        expand: bool,
    },
    /// Evaluate beat eligibility over a grid of state values (dsl 0.23.0
    /// §1): for every cell of the `--axis` product, starting from the
    /// `--script` (its save, then its steps replayed) or the declared
    /// defaults, the winner and the eligible beats it shadows for every
    /// listed occasion — play's own eligibility, no presentation. Beats
    /// never eligible in any cell, and beats eligible somewhere but
    /// presented in no cell (with what was presented over them), are listed
    /// at the end. Exit `0` on success, `1` when the project does not
    /// compile, `2` on an I/O or usage failure (including an axis that
    /// cannot be applied).
    Calendar {
        /// Project directory (`lute.project.yaml`).
        dir: PathBuf,
        /// One grid axis and its values, an inclusive integer range
        /// `run.day=1..7` or a list `run.slot=morning,afternoon,night`. The
        /// axis is a declared state path; `quest.<id>.state=…` seeds the
        /// quest's status; `quest.<id>.objectives.<oid>.done=true,false`
        /// sets objective progress; `holds(<fact>)=true,false` asserts or
        /// retracts a base fact; `visited('<id>')=true,false` puts a scene
        /// or bundle beat in or out of the visited set (repeatable; the
        /// first axis varies slowest).
        #[arg(long, value_name = "AXIS=VALUES", value_parser = play::calendar::parse_axis_flag)]
        axis: Vec<(String, Vec<String>)>,
        /// An occasion to evaluate (repeatable; default: every occasion a
        /// beat answers). `O@<axis>,…` varies only the named axes for `O`:
        /// it is evaluated where every other axis is at its first value
        /// (`O@run.day,run.slot=night` holds `run.slot` at `night`
        /// instead) and left blank elsewhere — `dayEnd@run.day` reads a
        /// once-a-day occasion once per day.
        #[arg(long, value_name = "OCCASION[@AXIS[=VALUE],…]")]
        occasion: Vec<String>,
        /// A relation whose facts to show per cell once it has settled
        /// (repeatable): in text a table with a row per first argument and a
        /// column per cell (`--facts at`: who is where, when); a per-cell
        /// list in `--json` / `--csv`.
        #[arg(long, value_name = "RELATION")]
        facts: Vec<String>,
        /// A target to raise targeted occasions for (repeatable; default:
        /// every target the occasion's beats name).
        #[arg(long, value_name = "TARGET")]
        target: Vec<String>,
        /// A play script: every cell starts from its save
        /// (`state:`/`facts:`/`visited:`/`presented:`/`quests:`/
        /// `entriesRead:`, dsl 0.22.0 §3) with its `steps:` replayed as
        /// `lute play` plays them.
        #[arg(long, value_name = "FILE")]
        script: Option<PathBuf>,
        /// Replay the `--script` only up to this step — a 1-based step
        /// number or a step `label:` — and evaluate from the state that
        /// step starts from (the step itself is not played).
        #[arg(long, value_name = "STEP", requires = "script")]
        until: Option<String>,
        /// Keep only the cells where this CEL condition holds (evaluated
        /// over the cell's state after the axes are applied) — prunes
        /// combinations of independent axes no run can reach.
        #[arg(long = "where", value_name = "CEL")]
        where_: Option<String>,
        /// Emit the grid as JSON.
        #[arg(long, conflicts_with = "csv")]
        json: bool,
        /// Emit one CSV row per cell and occasion/target.
        #[arg(long)]
        csv: bool,
    },
    /// Print the three independent version axes (docs/versioning.md): the
    /// TOOLCHAIN version (this CLI + workspace crates), the LANGUAGE version
    /// (the grammar/semantics the checker enforces), and the IR schema
    /// version (stamped as `irVersion` in every compiled artifact). Distinct
    /// from clap's built-in `--version`, which prints only the toolchain
    /// version. Human-readable lines by default; `--json` prints a single
    /// object `{"toolchain":…,"language":…,"ir":…}`.
    Version {
        /// Emit the three versions as one JSON object instead of human lines.
        #[arg(long)]
        json: bool,
    },
}

/// See [`Command::Scenario`].
#[derive(Subcommand)]
enum ScenarioCommand {
    /// Report a node's reachability verdict (Reachable/Unreachable/Unknown,
    /// T6) plus its declared `after` prerequisite structure (dsl §5:575).
    Reach {
        /// A scene's canonical key (e.g. `marina.s01ep02`), a bundle beat's
        /// `<document id>.<beat id>`, or `quest:<id>` for a quest (dsl
        /// §4.4's `envelope quest:<id>` syntax); `scene:`/`beat:` prefixes
        /// disambiguate.
        node_id: String,
    },
    /// Report the Guaranteed/Possible envelope tables for a node (T10) —
    /// full tables for a scene or an `after`-opted-in quest; defaults-only
    /// `D` plus an enrichment note for a bare quest (T12, dsl §4.4). Also
    /// prints the `Possible \ Guaranteed` warning-grade reads for the node
    /// (dsl §6) — suppressed by default in `check-project`, surfaced here.
    Envelope {
        /// A scene's canonical key, a bundle beat's `<document id>.<beat
        /// id>`, or `quest:<id>` for a quest.
        node_id: String,
    },
    /// Trace every fact-guarded condition — beat, entry, quest, objective,
    /// line, choice, arm, handler — grouped by document, to the relations it
    /// reads, and each relation to its producers through the rules:
    /// asserting documents, seed facts, the engine (reserved), or nothing
    /// (dsl 0.23.0 §1, 0.24.0 T3-1).
    Knowledge {
        /// Only this node: a scene key (every guard in the scene), a bundle
        /// beat key, an entry id, `<scene>#<branch>.<choice>` for one
        /// choice, a `<quest>.<objective>` id, or `quest:<id>`.
        #[arg(long = "for", value_name = "NODE")]
        for_node: Option<String>,
    },
}

/// See [`Command::Loc`].
#[derive(Subcommand)]
enum LocCommand {
    /// Extract every translatable content line (stable `code`, speaker,
    /// text, choice labels) across a project to a localization export.
    Export {
        /// Directory to walk recursively for `*.lute` files.
        dir: PathBuf,
        /// Output format: `json` (default) or `csv`.
        #[arg(long, value_name = "FORMAT")]
        format: Option<String>,
        /// Write the export here instead of stdout.
        #[arg(short = 'o', long = "out", value_name = "FILE")]
        out: Option<PathBuf>,
    },
    /// Canonicalize translated `loc export` files into ONE locale bundle —
    /// the reverse direction (dsl 0.8.0 §7), consumed by
    /// `lute compile --locales <bundle.json>`.
    ///
    /// Accepts exactly what `export` writes, in either format (`.csv` → CSV,
    /// anything else → JSON). `export` carries no locale (it extracts the
    /// SOURCE language), so the locale tag is the FILE STEM — the normal
    /// workflow is one translated file per locale (`ja-JP.json`,
    /// `en-US.json`). A row carrying its own non-empty `locale`
    /// field/column overrides that, so a single merged file also works.
    ///
    /// Exit `0`, `1` on `E-LOCALE-BUNDLE` (unparseable input, a duplicate
    /// `lineId` within one locale, or an empty locale tag), `2` on I/O.
    Import {
        /// One translated export per locale.
        #[arg(required = true, value_name = "FILE")]
        files: Vec<PathBuf>,
        /// Write the bundle here instead of stdout.
        #[arg(short = 'o', long = "out", value_name = "FILE")]
        out: Option<PathBuf>,
    },
    /// Word-count and line-count report per document and per speaker.
    Report {
        /// Directory to walk recursively for `*.lute` files.
        dir: PathBuf,
        /// Emit the report as JSON instead of human table lines.
        #[arg(long)]
        json: bool,
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

/// The universe of diagnostic codes `--deny <CODE>` may name (spec §5). A
/// promotion targeting a code OUTSIDE this set is a clap usage error (exit 2) —
/// "a typo'd promotion MUST NOT silently protect nothing". No canonical runtime
/// registry of codes exists (they are `pub const`s and inline literals
/// scattered across the checker crates), so this curated list IS that registry,
/// kept in ONE place. Assembled by grepping every `"[EW]-…"` code literal in the
/// crates whose diagnostics `check`/`check-project` surface (`lute-check`,
/// `lute-syntax`, `lute-cel`, `lute-manifest`, `lute-core-span`; since
/// 0.10.0 §8 put the mock pass under `check-project`, `lute-trace`; and since
/// §9:962 put the compile gate under `lute check`, `lute-compile`); the
/// `every_check_emitted_code_is_deniable` test, a `#[cfg(test)]` unit test at
/// the bottom of THIS file, rescans those crates and fails if any emitted
/// code is missing here, so a newly-added code cannot silently fall outside
/// the deny universe. A SUPERSET is harmless
/// (denying a code `check` never emits merely protects nothing); a MISSING code
/// is the only defect, and that test guards exactly it. Sorted for readability.
const DENIABLE_CODES: &[&str] = &[
    "E-ACCEPT-TARGET",
    "E-AGE-GATE",
    "E-APP-READONLY",
    "E-ARM-DEAD",
    "E-AS-REMOVED",
    "E-ASSET-DECOMPOSE",
    "E-ASSET-SEGMENT",
    "E-ASSET-UNKNOWN-ID",
    "E-AT-CONTEXT",
    "E-ATTR-DEF-DYNAMIC",
    "E-ATTR-QUOTE",
    "E-ATTR-TYPE",
    "E-BAD-ENUM",
    "E-BEAT-ATTR",
    "E-BEAT-UNREACHABLE",
    "E-BRANCH-ALL-GUARDED",
    "E-BRANCH-EMPTY",
    "E-BRANCH-PROMPT",
    "E-BRANCH-TIMEOUT",
    "E-CAPABILITY-MISMATCH",
    "E-CAST-UNKNOWN",
    "E-CEL-PARSE",
    "E-CEL-PROFILE",
    "E-CEL-TYPE",
    "E-CHOICE-DUP",
    "E-CHOICE-ID-RESERVED",
    "E-CHOICELOG-READ",
    "E-CLIP-OVERLAP",
    "E-CLIP-TIMING",
    "E-CLOCK-DECL",
    "E-COMMENT-UNTERMINATED",
    "E-COMPILE-COMPONENT",
    "E-COMPILE-EXPAND",
    "E-COMPILE-INTERNAL",
    "E-COMPONENT-ARG",
    "E-COMPONENT-BODY",
    "E-COMPONENT-CYCLE",
    "E-COMPONENT-DUP",
    "E-COMPONENT-PARSE",
    "E-COMPONENT-STATE",
    "E-COMPONENT-UNDECLARED",
    "E-CONN-CYCLE",
    "E-CONN-EPISODE-ID-DUP",
    "E-CONN-FORMULA-TOO-COMPLEX",
    "E-CONN-PROFILE",
    "E-CONN-UNKNOWN-NODE",
    "E-CONN-UNREACHABLE",
    "E-CONTENT-LINE-BRACKET",
    "E-CONTENT-OUTSIDE-SHOT",
    "E-DATALOG-FUNCTION",
    "E-DATALOG-GUARD-FACT",
    "E-DATALOG-PARSE",
    "E-DATALOG-UNSAFE",
    "E-DATALOG-UNSTRATIFIED",
    "E-DEF-DECL",
    "E-DEFAULTS-KEY",
    "E-DELIVERY-CONFLICT",
    "E-DELIVERY-FLAG-VALUE",
    "E-DELIVERY-NARRATOR",
    "E-DEPENDS-CYCLE",
    "E-DEPENDS-UNRESOLVED",
    "E-DEPENDS-VERSION",
    "E-DERIVE-TIER",
    "E-DERIVE-UNDECLARED",
    "E-DERIVED-WRITE",
    "E-DOLLAR-OUTSIDE-MATCH",
    "E-DOMAIN-DUP",
    "E-DOMAIN-UNKNOWN",
    "E-DUP-BRANCH",
    "E-DUP-LINE-CODE",
    "E-DUP-TRACK",
    "E-DUP-VOICEKEY",
    "E-ENGINE-OWNED-WRITE",
    "E-ENTITY-KIND-CLASH",
    "E-ENTITY-KIND-SHAPE",
    "E-ENTRY-ATTR",
    "E-ENTRY-ID-DUP",
    "E-ENTRY-SERIES-ORDER",
    "E-ENTRY-UNREACHABLE",
    "E-ENUM-DEFAULT-NOT-MEMBER",
    "E-ENUM-EXITS-NOT-MEMBER",
    "E-ENUM-LABEL-NOT-MEMBER",
    "E-ENUM-MISSING-SEMANTICS",
    "E-ENUM-UNEXPECTED-SEMANTICS",
    "E-EXTENDS-RELATION-SIG",
    "E-EXTENDS-STATE-TYPE",
    "E-FACT-DOMAIN",
    "E-FACT-EXCLUSIVE",
    "E-FACT-TIER-WRITE",
    "E-FRONTMATTER-SCHEMA",
    "E-GRAMMAR-NOT-ADMITTED",
    "E-HUB-NO-EXIT",
    "E-IDENTITY-TEMPLATE",
    "E-INTERP-DEF",
    "E-INTERP-UNTERMINATED",
    "E-INTO-TARGET",
    "E-INTO-UNDECLARED",
    "E-INTO-VALUE",
    "E-KIND-MISSING",
    "E-KIND-NAME-CLASH",
    "E-LEGACY-CONTENT-SIGIL",
    "E-LOCALE-BUNDLE",
    "E-LOGIC-CONTENT",
    "E-LOWER-RECORD-FIELD",
    "E-LOWER-RECORD-UNKNOWN",
    "E-MARK-DUP",
    "E-MATCH-DUP-OTHERWISE",
    "E-MATCH-RELATION-SUBJECT",
    "E-MAYBE-UNSET",
    "E-META-ID",
    "E-META-MISSING",
    "E-META-PARSE",
    "E-META-UNKNOWN-KEY",
    "E-META-VALUE",
    "E-MISSING-ATTR",
    "E-MOCK-SUBJECT",
    "E-NEXT-BACKWARD",
    "E-NEXT-UNDEFINED",
    "E-NONEXHAUSTIVE",
    "E-OBJECTIVE-CONTRADICTION",
    "E-OBJECTIVE-ID-DUP",
    "E-OBJECTIVE-ID-MISSING",
    "E-OBJECTIVE-MISSING-DONE",
    "E-OBJECTIVE-QUEST-DONE",
    "E-OBJECTIVE-UNSATISFIABLE",
    "E-OCCASION-UNKNOWN",
    "E-ON-NO-EVENT",
    "E-PATH-IDENT",
    "E-PERMISSION-BRIDGE",
    "E-PERMISSION-DIRECTIVE",
    "E-PERMISSION-FACT",
    "E-PERMISSION-QUEST",
    "E-PERMISSION-REWARD",
    "E-PERMISSION-STATE",
    "E-PERSIST-REMOVED",
    "E-PLUGIN-ASSET-SEGMENT-TYPE",
    "E-PLUGIN-DUP-ACROSS",
    "E-PLUGIN-DUP-ID",
    "E-PLUGIN-INVALID-DIRECTIVE",
    "E-PLUGIN-IO",
    "E-PLUGIN-MANIFEST",
    "E-PLUGIN-MISSING-ACTIVE",
    "E-PLUGIN-MISSING-EXPORT",
    "E-PLUGIN-OPTION-TYPE",
    "E-PLUGIN-OPTION-UNKNOWN",
    "E-PLUGIN-PARSE",
    "E-PLUGIN-RESERVED-NAME",
    "E-PLUGIN-RESERVED-STAMP-ATTR",
    "E-PLUGIN-UNKNOWN-ASSETKIND",
    "E-PLUGIN-UNKNOWN-EXPORT",
    "E-PLUGIN-UNKNOWN-REWARD-TARGET",
    "E-PROFILE-EXTENDS-CYCLE",
    "E-PROFILE-UNKNOWN",
    "E-QUEST-ID-DUP",
    "E-QUEST-ID-MISSING",
    "E-QUEST-MULTI-PARENT",
    "E-QUEST-REF-UNKNOWN",
    "E-QUEST-RESERVED-DECL",
    "E-QUEST-RESERVED-WRITE",
    "E-QUEST-TIER-MIX",
    "E-QUEST-TREE-CYCLE",
    "E-QUEST-UNREACHABLE",
    "E-REF-ARG-TYPE",
    "E-REF-ARITY",
    "E-REF-TYPE",
    "E-RELATION-ARITY",
    "E-RELATION-DECL",
    "E-RELATION-DOMAIN",
    "E-RELATION-DUP",
    "E-RELATION-EMPTY",
    "E-RELATION-RESERVED-NAME",
    "E-RELATION-RESERVED-WRITE",
    "E-RELATION-UNKNOWN",
    "E-RETRACT-WILDCARD-ASSERT",
    "E-REWARD-ATTR",
    "E-REWARD-KIND",
    "E-REWARD-TARGET",
    "E-RULE-AGGREGATE-CYCLE",
    "E-RULE-EXCLUSIVE",
    "E-RULE-GUARD-DEF",
    "E-SET-OP-TYPE",
    "E-SET-TYPE",
    "E-STATE-COLLECTION",
    "E-STATE-DECL",
    "E-STATE-DECL-CONFLICT",
    "E-STATE-MAYBE-UNAVAILABLE",
    "E-STATE-NAMESPACE",
    "E-STATE-REDECLARE",
    "E-STATE-SHAPE-CYCLE",
    "E-STREAM-BODY",
    "E-STREAM-CLOSED",
    "E-STREAM-PREFIX-CHANGED",
    "E-STREAM-TEMPLATE",
    "E-STRING-ESCAPE",
    "E-TAG-INLINE-BODY",
    "E-TAG-NOT-ONE-LINE",
    "E-TEMPORAL-ARG",
    "E-TEST-FILE",
    "E-TEST-KEY",
    "E-TEST-LORE",
    "E-TEST-NO-EXPECT",
    "E-TIME-RESOLUTION",
    "E-TIMELINE-CONTENT",
    "E-TIMELINE-DURATION",
    "E-TITLE-PLACEMENT",
    "E-TRACE-ACCEPT",
    "E-TRACE-BEAT",
    "E-TRACE-CHOICE",
    "E-TRACE-ENTRY",
    "E-TRACE-EVENT",
    "E-TRACE-MOCK-FACT",
    "E-TRACE-MOCK-PARSE",
    "E-TRACE-MOCK-TYPE",
    "E-TRACE-MOCK-UNDECLARED",
    "E-TRACK-KEY",
    "E-UNCLASSIFIED",
    "E-UNCLOSED-TAG",
    "E-UNDECLARED",
    "E-UNDECLARED-REF",
    "E-UNKNOWN-ATTR",
    "E-UNKNOWN-DIRECTIVE",
    "E-UNKNOWN-EVENT",
    "E-UNKNOWN-ID",
    "E-UNKNOWN-KIND",
    "E-UNSET-LITERAL",
    "E-UNSET-UNCOVERED",
    "E-USES-CYCLE",
    "E-USES-DUP-DEF",
    "E-USES-DUP-RELATION",
    "E-USES-DUP-STATE",
    "E-USES-NOT-FOUND",
    "E-USES-PARSE",
    "E-VALIDAT-DERIVED",
    "E-WHEN-LITERAL-DOMAIN",
    "E-WHEN-PATTERN",
    "E-WHEN-RANGE",
    "E-WHEN-UNSET-SUBJECT",
    "E-WRITE-CONFLICT",
    "W-ASSET-PLACEHOLDER",
    "W-BEAT-ONCE-RUN-USER",
    "W-BEAT-PRIORITY-TIE",
    "W-BEAT-SHADOWED",
    "W-CAST-ABSENT",
    "W-CATALOG-STALE",
    "W-CODE-AFTER-END",
    "W-CODE-AFTER-NEXT",
    "W-COMPONENT-UNVERIFIED",
    "W-DEADLINE-BEFORE-DONE",
    "W-DEF-UNUSED",
    "W-DERIVE-NO-RULES",
    "W-DISPLAY-NAME-DUP",
    "W-DOMAIN-UNREAD",
    "W-ENTRY-REF-UNKNOWN",
    "W-ENTRY-WRITE-REREAD",
    "W-EXIT-INERT",
    "W-FACT-GUARANTEED",
    "W-INTO-SET-DUP",
    "W-L10N-MISSING",
    "W-LUTE-VERSION-STALE",
    "W-META-LEGACY",
    "W-OBJECTIVE-HIDDEN",
    "W-OTHERWISE-DEAD",
    "W-OVERLAP-ARMS",
    "W-PROJECT-INERT",
    "W-QUEST-HANDLER-DEAD",
    "W-QUEST-NEVER-ACCEPTED",
    "W-QUEST-REF-UNKNOWN",
    "W-QUEST-STATE-ISSET",
    "W-RELATION-UNREAD",
    "W-REWARD-DOUBLE-CREDIT",
    "W-STAGE-ABSENT",
    "W-TEXT-LOOKS-LIKE-REF",
    "W-TIMELINE-CLIPS",
    "W-TIMELINE-TOTAL",
    "W-TIMELINE-TRACKS",
    "W-TRACE-MOCK-UNPRODUCIBLE",
    "W-WHEN-TEST-LITERAL",
];

/// clap `value_parser` for `--deny <CODE>`: accept only a code in the known
/// universe ([`DENIABLE_CODES`]); a typo, a compile/trace-only code, or a
/// made-up string is a clap usage error (exit 2), never a silent no-op
/// (spec §5). Returning `Err` here is how clap produces the exit-2 usage error.
fn parse_deny_code(raw: &str) -> Result<String, String> {
    if DENIABLE_CODES.contains(&raw) {
        Ok(raw.to_string())
    } else {
        Err(format!(
            "unknown diagnostic code `{raw}` (not a known `lute check` code); a typo'd \
             `--deny` must not silently protect nothing (spec §5)"
        ))
    }
}

/// The `--deny <CODE>` / `--deny-warnings` promotion policy (spec §5). Errors are
/// never demotable (spec §6), so promotion only ever turns a NON-error into an
/// error for the verdict, exit code, and reported severity.
#[derive(Default)]
struct DenyPolicy {
    codes: BTreeSet<String>,
    warnings: bool,
}

impl DenyPolicy {
    fn new(deny: &[String], deny_warnings: bool) -> Self {
        DenyPolicy {
            codes: deny.iter().cloned().collect(),
            warnings: deny_warnings,
        }
    }

    /// `true` iff `d` is PROMOTED to an error by this policy: it is not ALREADY
    /// an error (errors are never demotable, and re-marking a native error
    /// `denied` would misrepresent it as a promotion) AND its code is named by
    /// `--deny` OR it is a warning under `--deny-warnings`. The JSON
    /// `denied: true` marker and the human `[denied]` marker fire iff this is
    /// `true`, so a promotion is always distinguishable from a native error
    /// (spec §5).
    fn denied(&self, d: &Diagnostic) -> bool {
        d.severity != Severity::Error
            && (self.codes.contains(&d.code) || (self.warnings && d.severity == Severity::Warning))
    }

    /// `true` iff any diagnostic in `diags` is promoted — the signal that flips
    /// an otherwise-clean verdict to failure (exit 1).
    fn any_denied(&self, diags: &[Diagnostic]) -> bool {
        diags.iter().any(|d| self.denied(d))
    }
}

/// Apply the §5 deny promotion to one diagnostic's already-serialized JSON
/// object: when `policy.denied(d)`, override `severity` to `"error"` and add the
/// additive `"denied": true` marker. lute-check's `Diagnostic` struct is
/// UNTOUCHED — the CLI owns `--json` serialization and wraps the promotion at
/// this layer (spec §5). No-op when the diagnostic is not promoted.
fn apply_deny_json(d: &Diagnostic, policy: &DenyPolicy, value: &mut serde_json::Value) {
    if policy.denied(d) {
        if let serde_json::Value::Object(map) = value {
            map.insert("severity".into(), serde_json::json!("error"));
            map.insert("denied".into(), serde_json::json!(true));
        }
    }
}

fn main() -> ExitCode {
    match Cli::parse().command {
        Command::Check {
            file,
            json,
            providers,
            project,
            permission_profile,
            deny,
            deny_warnings,
        } => run_check(
            &file,
            json,
            providers.as_deref(),
            project.as_deref(),
            permission_profile.as_deref(),
            &DenyPolicy::new(&deny, deny_warnings),
        ),
        Command::CheckProject {
            dir,
            json,
            providers,
            deny,
            deny_warnings,
            wip,
        } => run_check_project(
            &dir,
            json,
            providers.as_deref(),
            &DenyPolicy::new(&deny, deny_warnings),
            wip,
        ),
        Command::Lint {
            path,
            json,
            deny,
            deny_warnings,
            config,
        } => lint::run_lint(&path, json, &deny, deny_warnings, config.as_deref()),
        Command::Compile {
            file,
            json,
            providers,
            project,
            permission_profile,
            out,
            all,
            locales,
            deny,
            deny_warnings,
        } => dispatch_compile(
            file.as_deref(),
            json,
            providers.as_deref(),
            project.as_deref(),
            permission_profile.as_deref(),
            out.as_deref(),
            all,
            locales.as_deref(),
            &DenyPolicy::new(&deny, deny_warnings),
        ),
        Command::CompileStream {
            file,
            providers,
            project,
            permission_profile,
        } => stream::run(
            &file,
            providers.as_deref(),
            project.as_deref(),
            permission_profile.as_deref(),
        ),
        Command::Context {
            file,
            json,
            providers,
            project,
            permission_profile,
        } => run_context(
            &file,
            json,
            providers.as_deref(),
            project.as_deref(),
            permission_profile.as_deref(),
        ),
        Command::Trace {
            file,
            state,
            fact,
            choose,
            event,
            accept,
            occasion,
            mock,
            json,
            providers,
            project,
            entry,
            beat,
            no_derive,
            expand,
        } => run_trace(
            &file,
            state,
            fact,
            choose,
            event,
            accept,
            occasion,
            mock.as_deref(),
            json,
            providers.as_deref(),
            project.as_deref(),
            entry.as_deref(),
            beat.as_deref(),
            no_derive,
            expand,
        ),
        Command::Tag { path, force } => rewrite::run_tag(&path, force),
        Command::Fix { path } => rewrite::run_fix(&path),
        Command::Catalog(CatalogCommand::Refresh { dir, project }) => {
            run_refresh(&dir, project.as_deref())
        }
        Command::Init { dir, template } => scaffold::run_init(&dir, template.as_deref()),
        Command::New {
            kind,
            name,
            dir,
            on,
            target,
            start,
        } => scaffold::run_new(
            &kind,
            &name,
            dir.as_deref(),
            on.as_deref(),
            target.as_deref(),
            start,
        ),
        Command::Lore { dir, json } => lore_report::run_lore(&dir, json),
        Command::Refs {
            dir,
            attr,
            reward,
            json,
        } => refs::run_refs(&dir, &attr, &reward, json),
        Command::Doctor { dir, json, strict } => doctor::run_doctor(&dir, json, strict),
        Command::Run {
            artifact,
            mock,
            occasion,
            json,
            entry,
            beat,
        } => runner::run_artifact(
            &artifact,
            mock.as_deref(),
            occasion,
            json,
            entry.as_deref(),
            beat.as_deref(),
        ),
        Command::Play {
            dir,
            script,
            json,
            no_derive,
            explain,
            ir,
        } => play::run_play(&dir, &script, json, no_derive, &explain, ir),
        Command::Test {
            dir,
            json,
            providers,
            project,
            coverage,
            no_derive,
        } => testcmd::run_test(
            &dir,
            json,
            providers.as_deref(),
            project.as_deref(),
            coverage,
            no_derive,
        ),
        Command::Loc(LocCommand::Export { dir, format, out }) => {
            loc::run_export(&dir, format.as_deref().unwrap_or("json"), out.as_deref())
        }
        Command::Loc(LocCommand::Import { files, out }) => loc::run_import(&files, out.as_deref()),
        Command::Loc(LocCommand::Report { dir, json }) => loc::run_report(&dir, json),
        Command::Scenario {
            dir,
            providers,
            format,
            facts,
            command,
        } => {
            if facts && command.is_some() {
                eprintln!(
                    "lute scenario: --facts applies to the graph view only; \
                     `reach`/`envelope`/`knowledge` already show a node's facts"
                );
                return ExitCode::from(2);
            }
            match format.as_deref() {
                None | Some("text") => run_scenario(&dir, providers.as_deref(), command, facts),
                Some(fmt) => scenario_fmt::run(&dir, providers.as_deref(), command, fmt, facts),
            }
        }
        Command::Beats {
            dir,
            occasion,
            target,
            json,
            expand,
        } => beats_cmd::run_beats(&dir, &occasion, &target, json, expand),
        Command::Calendar {
            dir,
            axis,
            occasion,
            facts,
            target,
            script,
            until,
            where_,
            json,
            csv,
        } => play::calendar::run_calendar(
            &dir,
            &play::calendar::CalendarArgs {
                axes: &axis,
                occasions: &occasion,
                facts: &facts,
                targets: &target,
                script: script.as_deref(),
                until: until.as_deref(),
                where_: where_.as_deref(),
                json,
                csv,
            },
        ),
        Command::Version { json } => run_version(json),
    }
}

/// Print the three independent version axes (docs/versioning.md): the
/// TOOLCHAIN version (this CLI + the workspace crates, `CARGO_PKG_VERSION`),
/// the LANGUAGE version ([`lute_check::LUTE_LANG_VERSION`] — the grammar and
/// semantics the checker enforces), and the IR schema version
/// ([`lute_compile::LUTE_IR_VERSION`] — stamped as `irVersion` in every
/// compiled artifact). The three bump independently (a toolchain release need
/// not move the language, and vice versa). `--json` prints one stable-keyed
/// object (keys emitted in `toolchain`/`language`/`ir` order, values
/// JSON-escaped); human mode prints one labeled line each. Always exit `0`.
fn run_version(json: bool) -> ExitCode {
    let toolchain = env!("CARGO_PKG_VERSION");
    let language = lute_check::LUTE_LANG_VERSION;
    let ir = lute_compile::LUTE_IR_VERSION;
    if json {
        // Build the object by hand so the key order is fixed and the values
        // are correctly JSON-escaped (serde_json::to_string on a &str is
        // infallible — a Rust string is always valid UTF-8).
        println!(
            "{{\"toolchain\":{},\"language\":{},\"ir\":{}}}",
            serde_json::to_string(toolchain).expect("string serializes"),
            serde_json::to_string(language).expect("string serializes"),
            serde_json::to_string(ir).expect("string serializes"),
        );
    } else {
        println!("lute toolchain {toolchain}");
        println!("language      {language}");
        println!("IR schema     {ir}");
    }
    ExitCode::SUCCESS
}

/// A `CheckInput` plus the verdict of the capability-resolution step that
/// produced it.
pub(crate) struct BuiltInput {
    pub input: CheckInput,
    /// `true` when resolving the project/plugin snapshot emitted an
    /// `E-`-severity diagnostic (plugin 0.0.2 §2 option validation,
    /// `E-PLUGIN-MISSING-ACTIVE`, `E-IDENTITY-TEMPLATE`, …). These describe the
    /// PROJECT rather than a span in the document, so they print through the
    /// `lute:` channel instead of the per-document diagnostic list — but they
    /// are errors, and every gating command MUST fold this into its exit code.
    pub resolve_error: bool,
    /// The project-level problems resolution surfaced, in emission order: a
    /// `lute.project.yaml` that failed to load, then each
    /// [`resolve_document_snapshot`] diagnostic as `<code>: <message>`. Each is
    /// the BODY of one `lute: …` stderr line.
    ///
    /// RETURNED rather than printed because they describe the PROJECT, not this
    /// document: a caller that resolves many documents under one project would
    /// print the identical line once per file, and a caller that reports through
    /// a structured model (`lute doctor`) could not capture them at all. Every
    /// gating command calls [`BuiltInput::report_project_diags`] immediately, so
    /// its stderr is byte-identical to when `build_input` printed them itself.
    pub project_diags: Vec<String>,
    /// The document's own lifted frontmatter, as `build_input` already parsed it
    /// to resolve the snapshot. Carried because the domain vocabulary a document
    /// resolves includes its OWN inline `enums:`/`entities:` projection
    /// (`TypedMeta::domains`), which `merge_domains` needs alongside
    /// `input.imports` — see `doctor::resolved_domains`.
    pub meta: lute_check::TypedMeta,
    /// The governing manifest's `defaults:` (0.10.0 §6), as applied to this
    /// document's frontmatter. Carried so future consumers of `BuiltInput`
    /// can inspect the applied defaults without re-loading the project.
    #[allow(dead_code)]
    pub defaults: lute_manifest::project::MetaDefaults,
    /// Frozen project identity templates resolved by the same manifest load as
    /// the capability snapshot and defaults.
    pub identity: lute_manifest::project::IdentityTemplates,
}

impl BuiltInput {
    /// Print [`BuiltInput::project_diags`] on the `lute:` stderr channel — the
    /// exact lines `build_input` used to emit inline.
    pub fn report_project_diags(&self) {
        for m in &self.project_diags {
            eprintln!("lute: {m}");
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
    permission_profile: Option<&str>,
) -> Option<BuiltInput> {
    build_input_with(
        &InputCache::default(),
        file,
        providers,
        project,
        permission_profile,
    )
}

/// [`build_input`] against a per-run [`InputCache`], for a caller that
/// assembles many documents' inputs in one invocation.
fn build_input_with(
    cache: &InputCache,
    file: &Path,
    providers: Option<&Path>,
    project: Option<&Path>,
    permission_profile: Option<&str>,
) -> Option<BuiltInput> {
    match read_document(file) {
        Ok(text) => {
            Some(assemble_input(cache, file, text, providers, project, permission_profile).0)
        }
        Err(message) => {
            eprintln!("{message}");
            None
        }
    }
}

/// `file`'s text, or the `lute: cannot read …` line [`build_input`] prints.
fn read_document(file: &Path) -> Result<String, String> {
    std::fs::read_to_string(file).map_err(|e| format!("lute: cannot read {}: {e}", file.display()))
}

/// The body of [`build_input`] over already-read `text`, also handing back
/// `lute_syntax::parse(&text)` — parsed here to lift the frontmatter — so a
/// caller can reuse it (`lute_check::check_parsed`) instead of re-parsing.
fn assemble_input(
    cache: &InputCache,
    file: &Path,
    text: String,
    providers: Option<&Path>,
    project: Option<&Path>,
    permission_profile: Option<&str>,
) -> (BuiltInput, (lute_syntax::ast::Document, Vec<Diagnostic>)) {
    // Resolve the capability snapshot the document is validated against. With
    // `--project`, load the project and assemble the scene's activated snapshot
    // (plugin §4/§11); without it, `resolve_document_snapshot(None, ..)` returns
    // the core-only `lute.core` baseline — behavior identical to before.
    let mut project_diags: Vec<String> = Vec::new();
    let root = project;
    let loaded = root.map(|dir| cache.project(dir));
    let project = match loaded.as_deref() {
        Some(Ok(p)) => p.as_ref(),
        Some(Err(e)) => {
            // A malformed project must not silently mis-validate: surface it
            // and fall back to core-only rather than pretending it loaded.
            project_diags.push(e.clone());
            None
        }
        None => None,
    };

    // Provider catalog precedence (plugin §10): an explicit `--providers <dir>`
    // wins; otherwise auto-discover the project's pinned catalog through the
    // SAME shared helper the LSP uses, so the two surfaces resolve the same ids
    // for the same project; with neither, an empty set.
    let providers = ProviderSet::clone(&cache.providers(providers, root, project));

    // 0.10.0 §6: the governing manifest's `defaults:`, already canonicalised
    // at load (D-Z). Lifted BEFORE the frontmatter parse, because a defaulted
    // `uses:` has to reach `resolve_imports` below.
    let defaults = project.map(|p| p.defaults.clone()).unwrap_or_default();

    // Lift the scene's frontmatter `profile`/`plugins` — both built-in keys, so a
    // default snapshot suffices to type them (they are not capability-gated).
    let mut parsed = lute_syntax::parse(&text);
    lute_check::meta::apply_quest_tier_default(&mut parsed.0, &defaults);
    let doc = &parsed.0;
    let (meta0, _) = lute_check::meta::parse_meta_kind_with_defaults(
        &doc.meta,
        &CapabilitySnapshot::default(),
        lute_check::meta::MetaKind::Scene,
        &defaults,
    );

    let resolved = cache.snapshot(root, project, meta0.profile.as_deref(), &meta0.plugins);
    let (mut snapshot, mut rdiags) = (resolved.0.clone(), resolved.1.clone());
    if let Some(name) = permission_profile {
        match project.as_ref() {
            Some(config) => match resolve_permissions(config, name) {
                Ok(permissions) => snapshot.restrict_permissions(&permissions),
                Err(error) => rdiags.push(ResolveDiag {
                    code: error.code().to_string(),
                    message: error.to_string(),
                }),
            },
            None => rdiags.push(ResolveDiag {
                code: "E-PERMISSION-PROFILE".to_string(),
                message: format!(
                    "`--permission-profile {name}` requires a loaded `lute.project.yaml` from `--project <DIR>`"
                ),
            }),
        }
    }
    let mut resolve_error = !project_diags.is_empty();
    for d in &rdiags {
        project_diags.push(format!("{}: {}", d.code, d.message));
        // An `E-` resolve diagnostic is a build-failing error like any other
        // (dsl 0.1.0 Appendix E: severity is binary, `E-` gates). It travels
        // the `lute:` channel instead of the per-document diagnostic list
        // because it describes the PROJECT, not a span in this file — but it
        // must still set the exit code, or `E-PLUGIN-OPTION-TYPE` and friends
        // would print and pass.
        resolve_error |= d.code.starts_with("E-");
    }
    let identity = project
        .as_ref()
        .map(|p| p.identity.clone())
        .unwrap_or_default();

    // Resolve the scene's `uses:` schema imports (dsl §9.2) and `components:`
    // component imports (dsl §13) relative to the scene's own directory; the LSP
    // resolves identically -> no divergence.
    let base = file.parent().unwrap_or_else(|| Path::new("."));
    let imports = cache
        .imports
        .resolve(base, &meta0.uses, &meta0.extends, doc.meta.span);
    let components = cache
        .imports
        .resolve_components(base, &meta0.components, doc.meta.span);

    let built = BuiltInput {
        input: CheckInput {
            text,
            uri: file.display().to_string(),
            snapshot,
            providers,
            // Batch/build analysis, not the interactive LSP default (both behave
            // identically today; the checker does not branch on mode yet).
            mode: Mode::Ci,
            imports,
            components,
            defaults: defaults.clone(),
        },
        resolve_error,
        project_diags,
        meta: meta0,
        defaults,
        identity,
    };
    (built, parsed)
}

/// Every document under `root` whose `::use` names component `name`
/// (dsl 0.10.0 §9 rule 4).
///
/// Reuses [`find_lute_files`] — the same walk `collect_project_docs` performs at
/// its own first line — so "in the resolved project" means exactly what it
/// means for `check-project`, including its symlink canonicalization and
/// deduplication. Parses each candidate rather than running `check()` on it: at
/// this point only the `::use` graph matters, and a full check per file would
/// make the standalone leg quadratic in the project.
///
/// Byte-sorted, for a deterministic "first caller".
fn callers_of_component(root: &Path, name: &str) -> Vec<PathBuf> {
    let Ok(files) = find_lute_files(root) else {
        return Vec::new();
    };
    let mut out: Vec<PathBuf> = Vec::new();
    for file in &files {
        let Ok(text) = std::fs::read_to_string(file) else {
            continue;
        };
        let (doc, _diags) = lute_syntax::parse(&text);
        if document_uses_component(&doc, name) {
            out.push(file.clone());
        }
    }
    out.sort();
    out
}

/// True when any `::use` anywhere in `doc` names component `name`. Reads the
/// same attribute `lute_check`'s `fold_use` reads.
fn document_uses_component(doc: &lute_syntax::ast::Document, name: &str) -> bool {
    doc.shots
        .iter()
        .any(|shot| nodes_use_component(&shot.body, name))
        || doc
            .quests
            .iter()
            .any(|quest| nodes_use_component(&quest.body, name))
}

/// Whether `d` is a `::use` of `name`.
fn directive_uses_component(d: &lute_syntax::ast::Directive, name: &str) -> bool {
    d.tag == "use"
        && d.attrs.iter().any(|a| {
            a.key == "component"
                && matches!(&a.value, lute_syntax::ast::AttrValue::Str(s) if s == name)
        })
}

/// The recursion. Every node kind that can CONTAIN a `::use`, mirroring the node
/// set `lute-check`'s own walks recurse. Exhaustive on purpose: the next node
/// kind that can hold a `::use` must not be silently missed.
fn nodes_use_component(nodes: &[lute_syntax::ast::Node], name: &str) -> bool {
    use lute_syntax::ast::{Arm, ClipNode, Node};
    nodes.iter().any(|node| match node {
        Node::Directive(d) => directive_uses_component(d, name),
        Node::Branch(b) => b.choices.iter().any(|c| nodes_use_component(&c.body, name)),
        Node::Hub(h) => h.choices.iter().any(|c| nodes_use_component(&c.body, name)),
        Node::On(o) => nodes_use_component(&o.body, name),
        Node::Objective(o) => nodes_use_component(&o.body, name),
        Node::Match(m) => m.arms.iter().any(|arm| match arm {
            Arm::When { body, .. } | Arm::Otherwise { body, .. } => nodes_use_component(body, name),
        }),
        Node::Timeline(t) => t.tracks.iter().any(|track| {
            track.clips.iter().any(|clip| match &clip.node {
                ClipNode::Directive(d) => directive_uses_component(d, name),
                ClipNode::Set(_) => false,
            })
        }),
        Node::Line(_) | Node::Set(_) | Node::Assert(_) | Node::Retract(_) => false,
    })
}

/// The `component:` name a document declares, with its frontmatter span, or
/// `None` when it is not a component file.
///
/// Read through `lute_check`'s own frontmatter reader, never a filename
/// convention — a component is a document KIND, not a `.component.lute` suffix,
/// and `component_import.rs` treats a missing `component:` name as
/// `E-COMPONENT-PARSE` for exactly that reason. `TypedMeta.component` is `None`
/// for every other document kind, which makes it the discriminator rather than
/// something to compare a `kind:` against.
///
/// The default snapshot suffices: `component:` is a built-in key and is not
/// capability-gated, the same reason `build_input` types `profile`/`plugins`
/// against `CapabilitySnapshot::default()`. The diagnostics are discarded here —
/// `check()` reports them through its own run.
fn component_name_of(file: &Path) -> Option<(String, Span)> {
    let text = std::fs::read_to_string(file).ok()?;
    let (doc, _diags) = lute_syntax::parse(&text);
    let (typed, _mdiags) = lute_check::parse_meta_kind(
        &doc.meta,
        &lute_manifest::snapshot::CapabilitySnapshot::default(),
        lute_check::MetaKind::Component,
    );
    typed.component.map(|name| (name, doc.meta.span))
}

/// The diagnostics `lute compile` and `lute trace` produce AFTER the `check`
/// gate: `normalize_document` then `expand_document`, the same pair in the same
/// order both of them run (`lute_compile::compile_with_check` passes 2–3,
/// `lute_trace::trace_with_check` step 4).
///
/// `check()` runs neither. That is T9.12's root cause, and it is not confined to
/// components: on ANY document an `E-COMPILE-*` fault was invisible to
/// `lute check` and fatal to everything downstream of it. A scene whose `defs:`
/// bodies form a cycle reported `ok: … (0 warning(s))`, while `lute trace` on
/// the same file printed `E-COMPILE-EXPAND … def expansion cycle: a -> b -> a`
/// and then *"has check error(s) — run `lute check` first"*. `check` cannot emit
/// a code it never computes, so that advice was unfollowable **by
/// construction** for the whole class. Running the pass here is what makes it
/// followable — the false green is closed at the leg that was green, not by
/// quietening the leg that was right.
///
/// A COMPONENT's own `params:` are bound to a placeholder first, exactly as
/// `::use` binds them at a call site. `check` registers a component's params as
/// **bodiless markers** (`defs`/`def_types`/`def_params`, deliberately never
/// `def_bodies` — the D3 marker path `decide()` resolves them through), a shape
/// the expander has no notion of: it looks up `bodies` alone and calls any miss
/// `"names no known def body"`. Expanding a component AS A ROOT would therefore
/// report the absence of a call site as a fault of the component, which it is
/// not — `<match on="@p">` over a declared param is the one logic block a
/// component body admits (dsl 0.4.0 §6.2). Binding first measures the body,
/// which is the only thing this leg can decide.
fn compile_gate_diags(input: &CheckInput) -> Vec<Diagnostic> {
    let (mut doc, _parse_diags) = lute_syntax::parse(&input.text);
    let mut arena = lute_cel::CelArena::default();
    let _ = lute_cel::fill_document(&mut arena, &mut doc);
    let (folded, _, _) = fold_env(&doc, input);
    let cast = lute_check::declared_cast(&input.snapshot, &input.imports, &folded.typed.cast);
    let mut diags = lute_compile::normalize::normalize_document(
        &mut doc,
        &input.components,
        &cast,
        &folded.env.state,
    );
    let bodies = if folded.typed.component.is_some() {
        let mut bodies = folded.def_bodies.clone();
        for p in &folded.typed.params {
            // The param's own name: what a `::use` splices is the caller's
            // argument text, and any `@`/`$`-free stand-in measures the same
            // body. `or_insert` so a real def never loses its body to a param
            // that shadows its name.
            bodies
                .entry(p.name.clone())
                .or_insert_with(|| p.name.clone());
        }
        std::borrow::Cow::Owned(bodies)
    } else {
        std::borrow::Cow::Borrowed(&folded.def_bodies)
    };
    let table = lute_check::DefTable {
        bodies: &bodies,
        params: &folded.env.def_params,
    };
    diags.extend(lute_compile::expand::expand_document(&mut doc, &table));
    diags
}

/// Fold compile-stage diagnostics into a clean `check` verdict: skip any
/// already reported, restore document order (`check` hands back
/// `(byte_start, code)` order, so the merged list reads like one run), and
/// recompute `ok`. Shared by `lute check` and `check-project`.
fn merge_gate_diags(result: &mut lute_check::CheckResult, gate: Vec<Diagnostic>) {
    let mut added = false;
    for d in gate {
        let dup = result.diagnostics.iter().any(|e| {
            e.code == d.code && e.span.byte_start == d.span.byte_start && e.message == d.message
        });
        if !dup {
            result.diagnostics.push(d);
            added = true;
        }
    }
    if !added {
        return;
    }
    result.diagnostics.sort_by(|a, b| {
        a.span
            .byte_start
            .cmp(&b.span.byte_start)
            .then_with(|| a.code.cmp(&b.code))
    });
    result.ok = !result
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error);
}

/// `lute compile`/`lute trace` refusing a component file, at its frontmatter.
///
/// A component is not a root document. Its `params:` are bound at each `::use`,
/// so there is no standalone artifact to emit and no standalone walk to take —
/// binding a stand-in and walking anyway makes the trace FABRICATE a decision
/// (measured: `purser-interject.component.lute` reports
/// `trace complete: 1 decision; arms 1/2`, picking `<otherwise>` for a `@pressure`
/// no caller supplied), which is a false green in `trace` traded for a false
/// green in `check`.
///
/// So the invocation is refused, and refused for the reason that is true.
/// Before, the refusal leaked the expander's own internal invariant assertion
/// — `` `@pressure` names no known def body (gate should have caught this) `` —
/// and attributed it to `check`, which reported the same file `ok`. That is
/// T9.12: advice pointing at a tool that contradicted it.
fn component_root_diag(component: &str, at: Span) -> Diagnostic {
    Diagnostic {
        code: "E-COMPILE-COMPONENT".to_string(),
        severity: Severity::Error,
        message: format!(
            "`{component}` is a component (dsl §13): its `params:` are bound at each `::use`, so \
             it has no standalone compiled form — compile or trace a document that imports it. \
             `lute check` on this file gives the component's own verdict and `check-project` is \
             the deciding leg (dsl 0.10.0 §9)"
        ),
        span: at,
        layer: lute_core_span::Layer::Content,
        fixits: Vec::new(),
        provenance: None,
        covered: Vec::new(),
        related: Vec::new(),
    }
}

/// Every diagnostic that holds at EVERY call site, re-anchored inside the
/// component (dsl 0.10.0 §9 rule 4).
///
/// Runs `check()` once per caller — the same run `check-project` performs — and
/// INTERSECTS the component-body diagnostics by `(code, message)`, the same key
/// §9 rule 2's roll-up uses, and for the same reason: that string is
/// byte-identical across callers exactly when the problem is caller-independent.
/// A diagnostic present at only some callers drops out of the intersection and
/// stays with `check-project`, where the caller is visible.
///
/// The surviving diagnostics are then re-anchored from §9 rule 1's secondary
/// location onto the primary one, because HERE the component IS the document
/// being reported on, so its own position is representable and the
/// ``component `x` (path):`` prefix is redundant. Rule 1 measured those
/// line/columns as already resolved against the component's own source, so no
/// re-normalisation is needed.
fn caller_resolved_common(
    callers: &[PathBuf],
    component_file: &Path,
    providers: Option<&Path>,
    root: &Path,
) -> Vec<Diagnostic> {
    use std::collections::{BTreeMap, BTreeSet};

    // `related[].file` is `def.src.display().to_string()` and `def.src` is
    // CANONICAL (`resolve_components` canonicalises), while `component_file` is
    // whatever the user typed on the command line. Without this the intersection
    // is always empty and rule 4 silently does nothing.
    let component_file =
        std::fs::canonicalize(component_file).unwrap_or_else(|_| component_file.to_path_buf());

    let mut per_caller: Vec<BTreeSet<(String, String)>> = Vec::new();
    let mut sample: BTreeMap<(String, String), Diagnostic> = BTreeMap::new();
    let cache = InputCache::default();
    for caller in callers {
        let Some(built) = build_input_with(&cache, caller, providers, Some(root), None) else {
            continue;
        };
        let res = check(&built.input);
        let mut here: BTreeSet<(String, String)> = BTreeSet::new();
        for d in &res.diagnostics {
            // A component-body diagnostic for THIS component: §9 rule 1's
            // `related` entry names the component's source file.
            if !d
                .related
                .iter()
                .any(|r| Path::new(&r.file) == component_file)
            {
                continue;
            }
            let key = (d.code.clone(), d.message.clone());
            here.insert(key.clone());
            sample.entry(key).or_insert_with(|| d.clone());
        }
        per_caller.push(here);
    }
    let Some(first) = per_caller.first().cloned() else {
        return Vec::new();
    };
    let common: BTreeSet<(String, String)> = per_caller
        .iter()
        .skip(1)
        .fold(first, |acc, s| acc.intersection(s).cloned().collect());
    common
        .into_iter()
        .filter_map(|key| sample.remove(&key))
        .map(|mut d| {
            if let Some(r) = d.related.first() {
                d.span = r.diagnostic.span;
                d.message = r.diagnostic.message.clone();
            }
            d.related.clear();
            d
        })
        .collect()
}

/// Check a `.yaml`/`.yml` state-schema declaration file as a SCHEMA: no
/// `kind:`, no frontmatter envelope, no body. The whole file IS the
/// frontmatter, wrapped in a synthetic `Meta` and fed through
/// `MetaKind::Schema` — byte-for-byte the lift `schema_import::read_and_parse`
/// performs on the same file kind when it is reached through `uses:`, so the
/// two surfaces cannot disagree about whether a schema is valid (#21, T3.9).
///
/// Rendering, counting and the exit code all go through the same
/// `CheckResult` path `run_check` uses, so `--json` and the human summary read
/// identically for a schema and for a scene.
fn run_check_schema_yaml(file: &Path, json: bool, policy: &DenyPolicy) -> ExitCode {
    let text = match std::fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("lute: cannot read {}: {e}", file.display());
            return ExitCode::from(2);
        }
    };
    let byte_end = text.len();
    let meta = lute_syntax::ast::Meta {
        raw_yaml: text,
        span: Span {
            byte_start: 0,
            byte_end,
            line: 1,
            column: 1,
            utf16_range: (0, 0),
        },
    };
    let (_tm, mut diagnostics) = lute_check::parse_meta_kind(
        &meta,
        &CapabilitySnapshot::default(),
        lute_check::MetaKind::Schema,
    );
    for d in schema_as_imported_diags(file) {
        let dup = diagnostics.iter().any(|e| {
            e.code == d.code && e.message == d.message && e.span.byte_start == d.span.byte_start
        });
        if !dup {
            diagnostics.push(d);
        }
    }
    // The house zero-then-normalize convention: `meta_key_span` emits byte
    // offsets and leaves `line`/`column` at zero for `check`'s own pass, which
    // this surface bypasses.
    let idx = lute_core_span::TextIndex::new(&meta.raw_yaml);
    for d in &mut diagnostics {
        let start = d.span.byte_start.min(byte_end);
        let end = d.span.byte_end.min(byte_end).max(start);
        d.span = Span::from_bytes(&idx, start, end);
    }
    diagnostics.sort_by(|a, b| (a.span.byte_start, &a.code).cmp(&(b.span.byte_start, &b.code)));
    let result = lute_check::CheckResult {
        ok: !diagnostics.iter().any(|d| d.severity == Severity::Error),
        diagnostics,
        resolved: None,
        domain_use: lute_check::DomainUse::default(),
    };
    render_check_result(file, &result, json, policy)
}

/// What `check` reports about the schema `file` when a document imports it
/// — the problems that need the resolved schema (its own `uses:` /
/// `extends:`) and, inside a project, the project's vocabulary: entity and
/// fact validation, enum labels, the clock's paths and `raise` occasions
/// (dsl 0.24 T3-6). A document that only `uses:` the schema is checked in
/// memory, and every diagnostic it attributes to the schema (its `related`
/// entry in this file) is kept, at the schema's own line. The schema's
/// frontmatter diagnostics (`E-USES-PARSE`) are the caller's already.
fn schema_as_imported_diags(file: &Path) -> Vec<Diagnostic> {
    let Ok(canon) = std::fs::canonicalize(file) else {
        return Vec::new();
    };
    let (Some(base), Some(name)) = (canon.parent(), canon.file_name()) else {
        return Vec::new();
    };
    let project = nearest_manifest_dir(file).and_then(|dir| load_project(&dir).ok().flatten());
    let (snapshot, _) = resolve_document_snapshot(project.as_ref(), None, &Default::default());
    let text = format!(
        "---\nkind: scene\nid: schema-check\nuses: '{}'\n---\n\n## Check\n",
        name.to_string_lossy().replace('\'', "''")
    );
    let (doc, _) = lute_syntax::parse(&text);
    let (meta, _) = lute_check::meta::parse_meta_kind(
        &doc.meta,
        &CapabilitySnapshot::default(),
        lute_check::meta::MetaKind::Scene,
    );
    let input = CheckInput {
        uri: base.join("schema-check.lute").display().to_string(),
        snapshot,
        providers: lute_manifest::project::project_providers(project.as_ref()),
        mode: Mode::Ci,
        imports: lute_check::resolve_imports(base, &meta.uses, &meta.extends, doc.meta.span),
        components: lute_check::resolve_components(base, &[], doc.meta.span),
        defaults: Default::default(),
        text,
    };
    let here = canon.display().to_string();
    check(&input)
        .diagnostics
        .into_iter()
        .filter(|d| d.code != "E-USES-PARSE")
        .flat_map(|d| d.related)
        .filter(|r| r.file == here)
        .map(|r| r.diagnostic)
        .collect()
}

/// Run `check` over one file and print its result. Exit `0` clean / `1` on an
/// error diagnostic (native OR `--deny`-promoted, spec §5) / `2` on an I/O
/// failure.
fn run_check(
    file: &Path,
    json: bool,
    providers: Option<&Path>,
    project: Option<&Path>,
    permission_profile: Option<&str>,
    policy: &DenyPolicy,
) -> ExitCode {
    // #21 / T3.9: `lute check world.schema.yaml` is the obvious next command
    // after an E-USES-PARSE, and it parsed the YAML schema AS A SCENE:
    // E-KIND-MISSING, three E-META-MISSING, and one E-UNCLASSIFIED per line —
    // the same flood for a perfectly VALID schema, never mentioning the real
    // defect. An author who follows that advice adds `kind: scene` to their
    // state schema and destroys it. A `.yaml`/`.yml` target is a pure
    // declaration map (data-catalog foundation B2) and is checked as one.
    if matches!(
        file.extension().and_then(|e| e.to_str()),
        Some("yaml") | Some("yml")
    ) {
        return run_check_schema_yaml(file, json, policy);
    }
    // 0.21.1 T3-9 (ashen F2): a file inside a project is checked against that
    // project. Without the manifest there is no `uses:` schema, no `defaults:`
    // and no profile, so the check reported `E-UNDECLARED`/`E-DOMAIN-UNKNOWN`
    // for paths the project declares, and advice that would have broken it.
    let discovered = match project {
        Some(_) => None,
        None => nearest_manifest_dir(file),
    };
    if let Some(dir) = &discovered {
        let shown = cwd_relative(&dir.display().to_string());
        eprintln!(
            "lute: note: using project {} (nearest lute.project.yaml); pass --project to choose another",
            if shown.is_empty() { "." } else { shown.as_str() }
        );
    }
    let project = project.or(discovered.as_deref());
    let Some(built) = build_input(file, providers, project, permission_profile) else {
        return ExitCode::from(2);
    };
    // `build_input` no longer prints these itself (`lute doctor` folds them into
    // its checklist instead); every gating command emits them exactly as before.
    built.report_project_diags();
    let BuiltInput {
        input,
        resolve_error,
        ..
    } = built;
    // plugin 0.0.2 §2: an `E-` capability-resolution diagnostic (bad plugin
    // option, missing active plugin, bad identity template) is a build-failing
    // error; it printed above, and it MUST gate here or it would pass silently.
    if resolve_error {
        return ExitCode::from(1);
    }
    let mut result = check(&input);

    // dsl 0.10.0 §9:962: *"`lute trace` on a component and `lute check` on the
    // same file stop disagreeing"*. They disagreed because `check` stopped one
    // pass short of where `trace` and `compile` stop, so `trace` refused over a
    // fault and then told the author to run the one tool that could not see it.
    // `check` now runs that pass — see [`compile_gate_diags`].
    //
    // Only on a clean check, mirroring both downstream pipelines exactly: they
    // gate on `result.ok` first and reach `normalize`/`expand` only past it, and
    // the expander's `"gate should have caught this"` arms are written on that
    // assumption. Running it on a red document would report consequences of the
    // errors already printed.
    if result.ok {
        merge_gate_diags(&mut result, compile_gate_diags(&input));
    }

    // dsl 0.10.0 §9 rule 4 (**D-W**): a standalone component check either
    // forwards the caller-resolved verdict or refuses to claim `ok`. Until
    // 0.10.0 it did neither: a component that cannot work with ANY of its
    // callers reported `ok`, `check-project` reported the fault once per caller
    // at line 1 of the wrong file, and `lute trace` refused with advice that
    // could not be followed. This is what makes that advice followable.
    //
    // "With no caller in scope" is a DISJUNCTION — *"no project resolved, or no
    // document in the project imports this component"* — and only the second
    // disjunct was built: the whole block hung off `Some(root)`, so
    // `lute check some.component.lute` with no `--project` fell straight
    // through to the bare `ok` the clause forbids. Since 0.21.1 (T3-9) the
    // nearest `lute.project.yaml` is discovered, so "no project resolved"
    // now means the file sits under no manifest at all.
    //
    // Both disjuncts now reach the same reporting point. They do NOT share a
    // message, because they are not the same situation: "no project resolved"
    // means the tool could not look, "no document imports this" means it looked
    // and found nothing. The next step differs — supply a project, versus
    // discover the component is unused — so the verdict names which one it is.
    if let Some((component, at)) = component_name_of(file) {
        match project.map(|root| (root, callers_of_component(root, &component))) {
            // Report only what holds at EVERY call site: a diagnostic holding at
            // some but not all callers is caller-specific and stays with
            // `check-project`, where the caller is visible. Anchored inside the
            // component — that is the whole point of running it here.
            Some((root, callers)) if !callers.is_empty() => result
                .diagnostics
                .extend(caller_resolved_common(&callers, file, providers, root)),
            Some(_) => result
                .diagnostics
                .push(lute_check::component_unverified_diag(
                    &component,
                    at,
                    lute_check::ComponentScope::NoImporter,
                )),
            None => result
                .diagnostics
                .push(lute_check::component_unverified_diag(
                    &component,
                    at,
                    lute_check::ComponentScope::NoProject,
                )),
        }
        result.ok = !result
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error);
    }
    render_check_result(file, &result, json, policy)
}

/// Render one `CheckResult` (`--json` or human) and derive the exit code: `0`
/// clean, `1` on a native OR `--deny`-promoted error. Shared by `run_check`
/// and `run_check_schema_yaml` so a schema and a scene cannot drift in
/// wording, JSON shape or verdict.
fn render_check_result(
    file: &Path,
    result: &lute_check::CheckResult,
    json: bool,
    policy: &DenyPolicy,
) -> ExitCode {
    // §5 verdict: a promoted (denied) diagnostic fails an otherwise-clean run.
    let ok = result.ok && !policy.any_denied(&result.diagnostics);

    if json {
        // Wrap the promotion at the CLI layer (spec §5): serialize lute-check's
        // own `CheckResult` shape, then overlay `severity: "error"` +
        // `denied: true` on each promoted diagnostic and the promoted `ok`.
        let mut value = match serde_json::to_value(result) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("lute: failed to serialize result: {e}");
                return ExitCode::from(2);
            }
        };
        if let Some(arr) = value.get_mut("diagnostics").and_then(|v| v.as_array_mut()) {
            for (d, jd) in result.diagnostics.iter().zip(arr.iter_mut()) {
                apply_deny_json(d, policy, jd);
            }
        }
        if let serde_json::Value::Object(map) = &mut value {
            map.insert("ok".into(), serde_json::json!(ok));
        }
        match serde_json::to_string_pretty(&value) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("lute: failed to serialize result: {e}");
                return ExitCode::from(2);
            }
        }
    } else {
        print_human(file, result, policy);
    }

    if ok {
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
fn collect_project_docs(
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
#[allow(clippy::type_complexity)]
fn collect_project_inputs(
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
fn normalize_span_from_text(text: &str, span: Span) -> Span {
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

/// The converged result of [`compute_conn_fixpoint`]'s monotone iteration
/// (dsl 0.4.0 §4.2's relational-objective-liveness CLOSURE, connectivity
/// design spec §4.2 -- reviewer finding: a single round misses a
/// multi-hop chain, e.g. a dead required objective -> a scene gated on
/// `completed()` that scene becomes unreachable -> its own `::assert`
/// producer drops -> a relation elsewhere goes non-producible -> ANOTHER
/// quest's required objective dies -> repeat).
struct ConnFixpoint {
    reach: BTreeMap<lute_check::connectivity::NodeId, lute_check::connectivity::Reachability>,
    reach_diags: Vec<(PathBuf, Diagnostic)>,
    /// dsl 0.20.0 §3/§4: the root's fact envelope at the converged `reach` —
    /// the one the project-level relational guard pass decides with.
    fact_env: lute_check::FactEnv,
    /// dsl 0.20.0 §6: per scene key, the facts guaranteed on arrival —
    /// `lute scenario`'s fact envelope.
    scene_must: BTreeMap<String, Vec<lute_check::fact_env::MustFact>>,
    dead_required_objective_quests: BTreeSet<String>,
    unreachable_quests: BTreeSet<String>,
}

/// Iterate `reach -> live assert sites -> May (fact envelope) ->
/// dead_required_objective_quests / dead_lifecycle_quests -> grow
/// unreachable_quests` to a FINITE FIXPOINT (dsl 0.4.0 §8.2 rule C4 + design
/// spec §4.2's closure, dsl 0.20.0 §3), shared by [`run_check_project`] and
/// [`assemble_root_scenario`].
///
/// **Fix 1 (reviewer, soundness/false-positive):** `live_assert_sites`'s
/// per-quest host-liveness check is seeded ONLY from
/// `lifecycle_unreachable_quests` (`start=false`/`fail=true` -- 0.4 §5.3:
/// `fail` "precedes completion... fails at the first evaluation instant",
/// i.e. the body genuinely NEVER executes), never from the GROWING combined
/// set. A quest with a dead REQUIRED objective can still ACTIVATE and run
/// its OTHER body nodes (an optional objective's own `::assert`, a
/// top-level assert, …) -- "can never COMPLETE" is not "never ACTIVATES".
/// Conflating the two would wrongly drop a still-live producer and cascade
/// a FALSE `E-OBJECTIVE-UNSATISFIABLE` onto an unrelated, genuinely-alive
/// objective. `reach` (scene-node liveness) is NOT similarly restricted --
/// a scene whose ONLY declared route runs through a now-unreachable
/// `completed(Q)` gate really is never entered, so ITS assert sites really
/// do drop; that is the intended closure, not a false positive.
///
/// **Fix 2 (advisory, completeness): finite fixpoint, not one round.** Each
/// iteration recomputes `reach` from the CURRENT `unreachable_quests`, then
/// the live assert sites / the may set / the dead-quest sets from that
/// `reach`, then grows `unreachable_quests` by the union. The composition is
/// MONOTONE over the finite quest-id domain:
/// - `eval_reach`'s `And`/`Or` lattice is monotone in `unreachable_quests`
///   (more unreachable input never turns a node MORE reachable) -> `reach`
///   only ever loses `Reachable`/`Unknown` entries to `Unreachable` as the
///   set grows, never the reverse.
/// - `live_assert_sites`'s scene branch reads `reach` directly -> the live
///   site set can only SHRINK (or stay the same) as `reach` tightens.
/// - `MaySet::build` is a monotone least fixpoint over its seeds -> fewer
///   live asserts can only shrink `May`, never grow it.
/// - a smaller `May` only turns *possible* `holds`/`count` verdicts into
///   decided ones (Kleene composition, R1-R5, never un-decides a value) --
///   so both dead-quest sets only grow.
///
/// So `unreachable_quests` is monotone NON-DECREASING, bounded above by the
/// full finite `quest_ids` set (every id either set can ever contain is
/// itself one of this root's declared quests) -- the loop terminates in AT
/// MOST `quest_ids.len() + 1` rounds (it either adds >=1 new id, or
/// stabilizes and returns). Every id ever added is PROVABLY dead/unreachable
/// at the round it was added -- monotone growth of a provable-only set can
/// never introduce a false positive.
fn compute_conn_fixpoint(
    group: &[(PathBuf, lute_syntax::ast::Document)],
    group_full: &DocGroup,
    file_results: &[(PathBuf, lute_check::CheckResult)],
    conn_graph: &lute_check::connectivity::ConnGraph,
    quest_ids: &BTreeSet<String>,
    ambiguous_quests: &BTreeSet<String>,
) -> ConnFixpoint {
    let lifecycle_unreachable_quests =
        lute_check::connectivity::unreachable_quest_ids(group, file_results);
    let mut root_vocab = lute_check::RootVocab::default();
    for (_path, _doc, folded) in group_full {
        root_vocab.add(&folded.env.rel_vocab, &folded.env.domains);
    }
    // seven F3: a document whose frontmatter does not parse may produce facts
    // nothing here can see — no guard is dead for want of them.
    root_vocab.note_unreadable_documents(group);
    // dsl 0.23.0 §9: seeds nothing can remove — a negated rule atom over one
    // of them never holds.
    let stable = lute_check::stable_seeds(group, &root_vocab);
    let mut unreachable_quests = lifecycle_unreachable_quests.clone();
    // dsl 0.10.0 §5.1 (D-N), dsl 0.20.0 §5: grows inside the loop like
    // `newly_dead`, and is tracked separately so the two derived causes keep
    // distinct verdict text.
    let mut dead_lifecycle_quests: BTreeSet<String> = BTreeSet::new();
    // dsl 0.20.0 §4: the must sets, computed ONCE from the first round's may
    // set. `May` only shrinks across rounds and `Must` reads it solely to
    // prove negated rule atoms, so the first (largest) `May` keeps `Must`
    // sound for every later round — and a fixed `Must` keeps the dead-quest
    // sets monotone (the termination argument above).
    let mut must: Option<lute_check::FactMust> = None;
    let foldeds: Vec<&lute_check::FoldedEnv> = group_full.iter().map(|(_, _, f)| f).collect();
    loop {
        let (reach, reach_diags) = lute_check::connectivity::check_reachability(
            conn_graph,
            quest_ids,
            ambiguous_quests,
            &unreachable_quests,
        );
        let live_facts = lute_check::connectivity::live_assert_sites(
            group,
            &reach,
            ambiguous_quests,
            &lifecycle_unreachable_quests,
        )
        .into_iter()
        .filter_map(|(_path, a)| lute_check::GroundFact::from_pattern(&a.pattern));
        let may = lute_check::MaySet::build(&root_vocab, live_facts, &stable);
        let must = must.get_or_insert_with(|| {
            lute_check::compute_must(group, &foldeds, conn_graph, &root_vocab, &may)
        });
        let fact_env = lute_check::FactEnv::new(may, must.slots.clone());
        let mut newly_dead: BTreeSet<String> = BTreeSet::new();
        // A quest whose `start=` decides false (or `fail=` true) only under
        // the fact envelope can never complete. The project guard pass says
        // so as a diagnostic, but that diagnostic lands in `project_diags`
        // and NEVER in `file_results`, so `unreachable_quest_ids` — which
        // scans the per-file `check()` output — cannot see it. Connectivity
        // reads the FACT from `dead_lifecycle_quests` instead, exactly as it
        // reads `dead_required_objective_quests` rather than the diagnostic
        // that cause emits.
        let mut newly_dead_lifecycle: BTreeSet<String> = BTreeSet::new();
        for (path, doc, folded) in group_full {
            newly_dead.extend(lute_check::fact_check::dead_required_objective_quests(
                path,
                doc,
                folded,
                &fact_env,
                ambiguous_quests,
            ));
            newly_dead_lifecycle.extend(lute_check::fact_check::dead_lifecycle_quests(
                path,
                doc,
                folded,
                &fact_env,
                ambiguous_quests,
            ));
        }
        // Both derived sets only ever GROW, so the fixpoint argument in this
        // function's doc holds.
        dead_lifecycle_quests.extend(newly_dead_lifecycle);
        let grown: BTreeSet<String> = lifecycle_unreachable_quests
            .iter()
            .cloned()
            .chain(dead_lifecycle_quests.iter().cloned())
            .chain(newly_dead)
            .collect();
        if grown == unreachable_quests {
            // A dead-lifecycle quest is a LIFECYCLE cause, not a
            // dead-objective one: it must reach `reach_verdict_text`'s
            // `E-QUEST-UNREACHABLE` branch, not the `E-OBJECTIVE-UNSATISFIABLE`
            // one that is checked first. Subtract it here so the two causes
            // keep their own text.
            let dead_required_objective_quests: BTreeSet<String> = unreachable_quests
                .difference(&lifecycle_unreachable_quests)
                .filter(|id| !dead_lifecycle_quests.contains(*id))
                .cloned()
                .collect();
            return ConnFixpoint {
                reach,
                reach_diags,
                fact_env,
                scene_must: must.scene_entry.clone(),
                dead_required_objective_quests,
                unreachable_quests,
            };
        }
        unreachable_quests = grown;
    }
}

/// Reconcile the per-root project analysis over ALREADY-COLLECTED docs
/// ([`collect_project_docs`]): run each resolved root's `<quest id>`
/// uniqueness pass (dsl 0.2.0 §6.3, [`lute_check::check_project_quest_ids`]),
/// quest-ref pass (dsl 0.5.1 §1.4), and the T5–T11 connectivity graph /
/// reachability / envelope analyses, then RECONCILE the per-file diagnostics
/// against the project-wide proof: suppress a per-file `E-QUEST-ID-DUP` the
/// project pass already covers (only within its OWN resolved root — never one
/// reaching outside the walk root or a sibling root, [`lute_check::colliding_occurrences`]),
/// and reclassify every entry-dependent, in-scope, non-tainted `E-MAYBE-UNSET`
/// against the connectivity envelope (dropped when `Guaranteed`,
/// dropped-and-suppressed when `Possible\Guaranteed`, replaced by error-grade
/// `E-STATE-MAYBE-UNAVAILABLE` when `∉ Possible`). Returns the reconciled
/// per-file results (each `ok` recomputed) plus the project-wide diagnostics
/// (spans normalized against each file's own text — [`normalize_span_from_text`]).
///
/// The connectivity fixpoint ([`compute_conn_fixpoint`]) and this
/// reconciliation live HERE, in `lute-cli`, never in `lute-check` (which stays
/// FS-free and format-free). Shared by [`run_check_project`] (grouping +
/// human/JSON output) and [`reconciled_project_results`] (the compile/trace
/// project-aware §5 gate).
/// `wip` grades the fact-envelope dead-guard verdicts for `check-project
/// --wip` (dsl 0.23.0 §10).
#[allow(clippy::type_complexity)]
fn reconcile_collected(
    mut file_results: Vec<(PathBuf, lute_check::CheckResult)>,
    by_root: &ByRoot,
    wip: bool,
) -> (
    Vec<(PathBuf, lute_check::CheckResult)>,
    Vec<(PathBuf, Diagnostic)>,
    BTreeMap<PathBuf, Vec<(lute_check::connectivity::NodeId, Span)>>,
) {
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
    // The lore mirror (dsl 0.19.0 §3): every `<entry id>` / `(series, order)`
    // occurrence the project entry pass already reports, used below to
    // suppress the per-file `E-ENTRY-ID-DUP` / `E-ENTRY-SERIES-ORDER` twins.
    let mut entry_covered = Vec::new();
    // Spec §5 project gate side channel (additive; NEVER affects
    // `project_diags`, so `check-project`'s output is byte-identical).
    // Accumulated across every resolved root so the single-root gate
    // (`reconciled_project_results`) can map its target document to the
    // NodeId(s) it hosts; the on/downstream-of-cycle test itself is decided
    // by topological-order exclusion in `reconciled_project_results`.
    let mut nodes_by_path: BTreeMap<PathBuf, Vec<(lute_check::connectivity::NodeId, Span)>> =
        BTreeMap::new();
    // First result index per path — the answer `iter().find(p == path)`
    // gave, without an O(files) scan per document.
    let mut result_ix: std::collections::HashMap<PathBuf, usize> =
        std::collections::HashMap::with_capacity(file_results.len());
    for (i, (p, _)) in file_results.iter().enumerate() {
        result_ix.entry(p.clone()).or_insert(i);
    }
    for (root, group_full) in by_root {
        let plain_group: Vec<(PathBuf, lute_syntax::ast::Document)> = group_full
            .iter()
            .map(|(p, d, _)| (p.clone(), d.clone()))
            .collect();
        let group = &plain_group;
        let mut group_ix: std::collections::HashMap<&Path, usize> =
            std::collections::HashMap::with_capacity(group_full.len());
        for (i, (p, _, _)) in group_full.iter().enumerate() {
            group_ix.entry(p.as_path()).or_insert(i);
        }
        let beat_foldeds: Vec<&lute_check::FoldedEnv> =
            group_full.iter().map(|(_, _, f)| f).collect();
        let mocked = mockcheck::mocked_accepts_under(root);
        // The standalone project passes read only this root's documents —
        // never one another's output or the connectivity chain's — so they
        // run in parallel with it; every result is appended below in the
        // fixed order the passes always ran in.
        type Pass<'a> = Box<dyn Fn() -> Vec<(PathBuf, Diagnostic)> + Send + Sync + 'a>;
        let standalone: Vec<Pass<'_>> = vec![
            Box::new(|| check_project_quest_ids(group)),
            Box::new(|| check_project_quest_refs(group)),
            // dsl 0.21.0 §7a.3: every `::accept` names an accept-driven quest.
            Box::new(|| lute_check::check_project_accepts(group)),
            // dsl 0.24.0 §2: an accept-driven quest no `::accept`, mock, or test accepts.
            Box::new(|| lute_check::check_project_never_accepted(group, &mocked)),
            // dsl 0.19.0 §3/§5: project-wide entry id / series-order uniqueness
            // and `entry.<id>.read` references (the quest passes' lore mirror).
            Box::new(|| lute_check::check_project_entry_ids(group)),
            Box::new(|| lute_check::check_project_entry_refs(group)),
            // dsl 2026-08-31 §4 (subquest design): structural checks over the
            // parent→child tree implied by every `<objective quest="c">`. Sits
            // next to the existing quest-ref pass because the two ask the same
            // question at two different depths -- ref pass on the READ side
            // (`quest.<id>.state` from anywhere), tree pass on the STRUCTURAL
            // side (parent quest naming the child). Both are project-wide
            // because a `quest=` reference can name a quest in a sibling file.
            Box::new(|| lute_check::check_project_quest_tree(group)),
            // dsl 0.22.0 §7: `<on event="questFailed">` on a quest that cannot
            // fail (project-wide: a parent in another file can cascade-fail it).
            Box::new(|| lute_check::check_project_quest_handlers(group)),
            Box::new(|| lute_check::connectivity::check_conn_episode_dup(group)),
            // dsl 0.26.0 §2.1: every declaration of one state path agrees.
            Box::new(|| lute_check::state_decls::check_project_state_decls(group, &beat_foldeds)),
            // dsl 0.26.0 §2.8: advisory — two speakers sharing a display name.
            Box::new(|| {
                let casts: Vec<_> = beat_foldeds.iter().map(|f| &f.cast).collect();
                let use_lines: Vec<_> = beat_foldeds.iter().map(|f| &f.use_lines).collect();
                let origins: Vec<_> = beat_foldeds
                    .iter()
                    .map(|f| &f.env.rel_vocab.origins.cast)
                    .collect();
                // A name nobody speaks is reported at the cast entry.
                let home = |id: &str| {
                    let project = load_project(root).ok().flatten();
                    let plugins = project.as_ref().map(|p| p.plugins_dir.as_path());
                    lint::cast_home(root, plugins, &origins, id)
                };
                lute_check::display_names::check_display_names(group, &casts, &use_lines, &home)
            }),
        ];
        let (chain, (standalone_diags, (ladder, producers))) = rayon::join(
            || {
                let key_set = lute_check::connectivity::scene_key_set(group);
                let quest_ids = lute_check::connectivity::quest_id_set(group);
                let node_diags =
                    lute_check::connectivity::resolve_nodes(group, &key_set, &quest_ids);
                let (conn_graph, cycle_diags) =
                    lute_check::connectivity::assemble_graph(group, &key_set, &quest_ids);
                // T7/T14/Fix2 wiring: `compute_conn_fixpoint` iterates the
                // reach/live-assert/may-set/dead-quest composition to a finite
                // fixpoint (see its own doc comment for the termination + soundness
                // argument) -- `ambiguous_quests` is shared with the envelope wiring
                // below.
                let ambiguous_quests = lute_check::connectivity::ambiguous_quest_ids(group);
                let fp = compute_conn_fixpoint(
                    group,
                    group_full,
                    &file_results,
                    &conn_graph,
                    &quest_ids,
                    &ambiguous_quests,
                );
                (
                    key_set,
                    node_diags,
                    conn_graph,
                    cycle_diags,
                    ambiguous_quests,
                    fp,
                )
            },
            || {
                rayon::join(
                    || standalone.par_iter().map(|pass| pass()).collect::<Vec<_>>(),
                    || {
                        (
                            lute_check::beats::presence_ladder(group, &beat_foldeds),
                            lute_check::cast::fact_producers(group),
                        )
                    },
                )
            },
        );
        let (key_set, node_diags, conn_graph, cycle_diags, ambiguous_quests, fp) = chain;
        // `standalone_diags` keeps the fixed order the passes always ran in.
        for diags in standalone_diags {
            project_diags.extend(diags);
        }
        project_diags.extend(node_diags);
        project_diags.extend(cycle_diags);
        // Spec §5 gate side channel (additive, no diagnostic effect): record
        // every node's (id, span) keyed by its declaring file, so the gate can
        // anchor a TARGET-owned `E-CONN-CYCLE` even when the emitted cycle
        // diagnostic landed on a different node's file.
        for info in conn_graph.nodes.values() {
            nodes_by_path
                .entry(info.path.clone())
                .or_default()
                .push((info.id.clone(), info.span));
        }
        project_diags.extend(fp.reach_diags);
        // dsl 2026-08-31 §4 extension: `E-QUEST-UNREACHABLE` propagates one
        // edge UP a subquest tree — a required `<objective quest="c">` on a
        // dead child can never complete (§2.1's synthesized predicate
        // `quest.c.state == 'complete'` never fires). Sits AFTER the
        // fixpoint because it consumes `fp.unreachable_quests` — the union
        // of lifecycle-dead, dead-`start`, and dead-required-objective
        // consequences the fixpoint has just settled. `optional` is
        // filtered inside the helper, matching §2.1's own carve-out.
        //
        // dsl 0.20.0 §5: every guard slot below is re-decided under the
        // root's fact envelope (built once, by the fixpoint above). Under
        // `--wip` (dsl 0.23.0 §10, 0.26.0 §2.6) the envelope carries its
        // work-in-progress twin: relations nothing produces yet, or only a
        // component `::assert` with an unbound `@param` writes, may hold
        // anything there.
        let wip_env = wip.then(|| {
            let mut vocab = lute_check::RootVocab::default();
            for (_, _, folded) in group_full {
                vocab.add(&folded.env.rel_vocab, &folded.env.domains);
            }
            let unproduced = lute_check::unproduced_relations(group, &vocab);
            fp.fact_env.clone().with_wip(&vocab, &unproduced)
        });
        match &wip_env {
            None => project_diags.extend(lute_check::check_project_subquest_unsatisfiable(
                group,
                &fp.unreachable_quests,
            )),
            // dsl 0.26.0 §2.6: a child unreachable only for want of
            // producers not written yet grades its parent's objective a
            // warning. `firm` re-derives the fixpoint's causes ignoring
            // verdicts the work-in-progress twin does not share.
            Some(env) => {
                let mut firm =
                    lute_check::connectivity::unreachable_quest_ids(group, &file_results);
                for (path, doc, folded) in group_full {
                    firm.extend(lute_check::fact_check::dead_required_objective_quests(
                        path,
                        doc,
                        folded,
                        env,
                        &ambiguous_quests,
                    ));
                    firm.extend(lute_check::fact_check::dead_lifecycle_quests(
                        path,
                        doc,
                        folded,
                        env,
                        &ambiguous_quests,
                    ));
                }
                firm.retain(|q| fp.unreachable_quests.contains(q));
                let pending: BTreeSet<String> =
                    fp.unreachable_quests.difference(&firm).cloned().collect();
                project_diags.extend(lute_check::check_project_subquest_unsatisfiable(
                    group, &firm,
                ));
                project_diags.extend(
                    lute_check::check_project_subquest_unsatisfiable(group, &pending)
                        .into_iter()
                        .map(|(path, mut d)| {
                            d.severity = Severity::Warning;
                            d.message.push_str(
                                " — a warning under `--wip`: the child is unreachable only for \
                                 want of producers not written yet (dsl 0.26.0 §2.6)",
                            );
                            (path, d)
                        }),
                );
            }
        }
        let fact_env = wip_env.as_ref().unwrap_or(&fp.fact_env);
        // Only verdicts the facts newly make decidable are added; one the
        // per-file `check()` already reported for the same slot is not
        // repeated.
        // Per document, independent: in parallel, appended in walk order.
        // Beside it, dsl 0.21.0 §5 / 0.22.0 §13: `W-BEAT-SHADOWED` — a
        // `select: first` beat an earlier-ordered, always-eligible,
        // never-spent beat on the same occasion always beats (project order
        // is the selection tiebreak) — and `W-BEAT-PRIORITY-TIE`, which (dsl
        // 0.26.0 §8) reads the fact envelope's must sets and the root's
        // assert sites; appended after the cast pass, where it always was.
        let (guard_diags, beat_diags): (Vec<Vec<Diagnostic>>, _) = rayon::join(
            || {
                group_full
                    .par_iter()
                    .map(|(path, doc, folded)| {
                        let reported = result_ix
                            .get(path)
                            .map_or(&[][..], |&i| file_results[i].1.diagnostics.as_slice());
                        lute_check::check_fact_guards(path, doc, folded, fact_env, reported)
                    })
                    .collect()
            },
            || lute_check::check_project_beats(group, &beat_foldeds, &producers, Some(fact_env)),
        );
        for ((path, _, _), diags) in group_full.iter().zip(guard_diags) {
            for d in diags {
                project_diags.push((path.clone(), d));
            }
        }
        // dsl 0.24.0 §4: `W-CAST-ABSENT` re-decided under the fact envelope,
        // the beat ladders and the root's assert sites — a line the Must set,
        // the beats a ladder must have spent first, or a fact only its own
        // unit produces shows its speaker present at is dropped. dsl 0.25.0
        // §6: a line that follows a `changedOn` occasion in the scenario
        // graph is decided without `assume: true` for that relation — added.
        // (`ladder` and `producers` were computed alongside the fixpoint.)
        let after = lute_check::cast::occasions_before(group, &beat_foldeds, &conn_graph);
        let no_ladder = BTreeMap::new();
        let no_after = BTreeMap::new();
        for (path, doc, folded) in group_full {
            if let Some(r) = result_ix.get(path).map(|&i| &mut file_results[i].1) {
                let project = lute_check::cast::PresenceProject {
                    env: fact_env,
                    ladder: ladder.get(path).unwrap_or(&no_ladder),
                    producers: &producers,
                    after: after.get(path).unwrap_or(&no_after),
                };
                let added = lute_check::cast::reconcile_presence(
                    &mut r.diagnostics,
                    path,
                    doc,
                    folded,
                    &project,
                );
                if !added.is_empty() {
                    let text = std::fs::read_to_string(path).unwrap_or_default();
                    for mut d in added {
                        d.span = normalize_span_from_text(&text, d.span);
                        let at = r.diagnostics.partition_point(|x| {
                            (x.span.byte_start, &x.code) <= (d.span.byte_start, &d.code)
                        });
                        r.diagnostics.insert(at, d);
                    }
                }
            }
        }
        project_diags.extend(beat_diags);
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
            let Some((scene_path, _)) = occurrences.first() else {
                continue;
            };
            let Some((_, doc, folded)) =
                group_ix.get(scene_path.as_path()).map(|&i| &group_full[i])
            else {
                continue;
            };
            let all_nodes: Vec<lute_syntax::ast::Node> = doc
                .shots
                .iter()
                .flat_map(|s| s.body.iter().cloned())
                .collect();
            // dsl 0.24.0: the same scope and beat-`when` assumption `check()`
            // walks with, so both passes see the same entry-dependent reads.
            let scope = defassign::Scope::of(folded);
            let beat_when = folded.typed.beat.as_ref().and_then(|b| b.when.as_ref());
            let (local_diags, assigned, reads) =
                check_definite_assignment(&all_nodes, &scope, beat_when);
            // T4.4/T4.6 carry-forward parity (dsl §7 soundness invariant): the
            // real `check()` pipeline (`check.rs::suppress_exhaustive_subject_reads`)
            // drops any `E-MAYBE-UNSET` whose span is a domain-exhaustive
            // `<match>` subject BEFORE `file_results` is ever populated -- a
            // read like that never earns a per-file `E-MAYBE-UNSET` standalone,
            // so it must never be treated as "entry-dependent" here either, or
            // this project-level recomputation (which calls
            // `check_definite_assignment` raw, unaware of that later
            // suppression) would newly error a file `check()` reports clean.
            let exhaustive_spans = defassign::exhaustive_match_subject_spans(&all_nodes, &scope);
            let is_exhaustive_subject = |span: &Span| {
                exhaustive_spans
                    .iter()
                    .any(|s| s.byte_start == span.byte_start && s.byte_end == span.byte_end)
            };
            per_doc.scene.insert(
                key.clone(),
                (
                    envelope::guaranteed(&assigned),
                    envelope::possible_writes(&all_nodes),
                ),
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
        for (path, d) in envelope::check_envelope(&conn_graph, &envs, &tainted, &reads_per_scene) {
            if d.severity == Severity::Error {
                project_diags.push((path, d));
            }
        }
        for (key, occurrences) in &key_set {
            let node_id = lute_check::connectivity::NodeId::Scene(key.clone());
            // Only reconcile (drop) a read's per-file `E-MAYBE-UNSET` when
            // its node has a REAL envelope to reclassify against: present
            // in `envs` AND not `tainted`. Per-node cycle recovery (spec
            // §4.1): a node ON or DOWNSTREAM of an `E-CONN-CYCLE` is the
            // ONLY kind `propagate` omits from `envs` (a cycle-independent
            // node keeps a real entry and IS reconciled here) — such a node
            // is exactly as untrustworthy as a tainted one: `check_envelope`
            // above already skips it (no replacement diagnostic emitted for
            // it either), so dropping its per-file diagnostic here would
            // silently lose a genuine local maybe-unset error with nothing
            // to replace it.
            if tainted.contains(&node_id) || !envs.contains_key(&node_id) {
                continue;
            }
            let Some((scene_path, _)) = occurrences.first() else {
                continue;
            };
            let Some(sites) = sites_per_scene.get(key) else {
                continue;
            };
            for (span, message) in sites {
                reconciled_reads.push((scene_path.clone(), *span, message.clone()));
            }
        }
        covered.extend(lute_check::colliding_occurrences(group));
        entry_covered.extend(lute_check::colliding_entry_occurrences(group));
    }
    for (path, result) in &mut file_results {
        result.diagnostics.retain(|d| {
            let quest_dup_covered = d.code == "E-QUEST-ID-DUP"
                && covered.iter().any(|(p, s)| p == path && *s == d.span);
            let entry_dup_covered = (d.code == lute_check::E_ENTRY_ID_DUP
                || d.code == lute_check::E_ENTRY_SERIES_ORDER)
                && entry_covered.iter().any(|(p, s)| p == path && *s == d.span);
            let envelope_reconciled = d.code == "E-MAYBE-UNSET"
                && reconciled_reads
                    .iter()
                    .any(|(p, s, m)| p == path && *s == d.span && *m == d.message);
            !quest_dup_covered && !entry_dup_covered && !envelope_reconciled
        });
        result.ok = !result
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error);
    }
    // Defect fix (persona review, connectivity T-final): every project-wide
    // diagnostic anchored via `lute_check::meta::meta_key_span` (the
    // `E-CONN-EPISODE-ID-DUP`/`E-CONN-UNKNOWN-NODE`/`E-CONN-CYCLE`/
    // `E-CONN-UNREACHABLE` scene anchors) carries a CORRECT byte range but
    // a ZEROED `line`/`column` -- that helper's own documented contract:
    // "`crate::check`'s `normalize_spans` recomputes them from the byte
    // offsets." Per-file diagnostics get that treatment inside `check()`
    // itself; these are assembled here, project-wide, and never pass
    // through it, so they printed `0:0` verbatim. Mirror the SAME
    // normalization here, per diagnostic's own file text -- a `Span` that
    // already carries a real line/col (a quest's parser-produced
    // `id_span`/`after_span`, or `E-STATE-MAYBE-UNAVAILABLE`'s read-site
    // span) recomputes identically from the SAME byte offsets against the
    // SAME source text, so this is a no-op for those, never a regression.
    let mut project_diag_text_cache: BTreeMap<PathBuf, String> = BTreeMap::new();
    for (path, d) in &mut project_diags {
        let text = project_diag_text_cache
            .entry(path.clone())
            .or_insert_with(|| std::fs::read_to_string(path.as_path()).unwrap_or_default());
        d.span = normalize_span_from_text(text, d.span);
    }
    (file_results, project_diags, nodes_by_path)
}

/// dsl 0.10.0 §9 rule 2: fold identical component-body diagnostics across
/// callers into one, keeping the first in byte-sorted path order and
/// summarising the rest.
///
/// `validate_components` runs once per importing document, so N callers of one
/// broken component produce N separate `check()` runs and N identical
/// diagnostics — eleven modules, eleven identical messages, at line 1 column 1
/// of eleven files that are all correct. The roll-up belongs here because this
/// is the first place those runs meet.
///
/// The key is `(code, message)`. After §9 rule 1 a component-body message reads
/// ``component `{name}` ({src}): {original}``, which is byte-identical across
/// callers exactly when the problem is caller-INDEPENDENT and different when it
/// is not — `E-BAD-ENUM` enumerates the resolved domain, so two callers with
/// different vocabularies differ in the message itself. That is rule 2's
/// boundary precisely, and it needs no marker field: a caller-specific fault
/// stays with its own caller, where the caller is visible.
fn rollup_component_body_diags(file_results: &mut [(PathBuf, lute_check::CheckResult)]) {
    use std::collections::BTreeMap;

    // Pass 1: count, in byte-sorted path order, which is `collect_project_docs`'
    // own order — so "the first" is deterministic without re-sorting.
    let mut counts: BTreeMap<(String, String), usize> = BTreeMap::new();
    for (path, result) in file_results.iter() {
        for d in &result.diagnostics {
            if is_component_body_diag(d, path) {
                *counts
                    .entry((d.code.clone(), d.message.clone()))
                    .or_insert(0) += 1;
            }
        }
    }

    // Pass 2: keep the first of each group, annotate it, drop the rest.
    let mut seen: std::collections::BTreeSet<(String, String)> = Default::default();
    for (path, result) in file_results.iter_mut() {
        let mut kept = Vec::with_capacity(result.diagnostics.len());
        for mut d in std::mem::take(&mut result.diagnostics) {
            if !is_component_body_diag(&d, path) {
                kept.push(d);
                continue;
            }
            let key = (d.code.clone(), d.message.clone());
            if !seen.insert(key.clone()) {
                continue; // a later caller reporting the same problem
            }
            let others = counts.get(&key).copied().unwrap_or(1).saturating_sub(1);
            if others > 0 {
                d.message = format!("{} (+{others} more caller{})", d.message, plural(others));
            }
            kept.push(d);
        }
        result.diagnostics = kept;
        result.ok = !result
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error);
    }
}

/// A diagnostic surfaced from ANOTHER file: it carries a `related` entry whose
/// `file` is not the document it is reported on (§9 rule 1's cross-file
/// attribution). That is what distinguishes a component-body report from an
/// ordinary local diagnostic, without a new marker field.
fn is_component_body_diag(d: &Diagnostic, path: &Path) -> bool {
    let here = path.display().to_string();
    d.related.iter().any(|r| r.file != here)
}

fn plural(n: usize) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// Recursively `check` every `*.lute` under `dir` ([`collect_project_docs`],
/// nested per-file root resolution — each file resolves against its OWN
/// nearest ancestor `lute.project.yaml`, bounded below by `dir`), reconcile
/// the per-root project analysis ([`reconcile_collected`]), then print the
/// per-file + project-wide report (human or `--json`) and map the verdict to
/// an exit code: `0` clean, `1` when any file has a (post-suppression)
/// `Error` or any resolved root's quest-id/connectivity pass finds one, `2`
/// on an I/O failure walking `dir` or reading a file.
fn run_check_project(
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
            eprintln!("lute: cannot walk {} for manifests: {e}", dir.display());
            return ExitCode::from(2);
        }
    };
    if manifest_invalid {
        return ExitCode::FAILURE;
    }

    let (file_results, by_root, inputs) = match collect_project_inputs(dir, providers, false) {
        Ok(v) => v,
        Err(code) => return code,
    };
    // Keyed before `reconcile_collected` takes the (aligned) results.
    let inputs: BTreeMap<PathBuf, (PathBuf, CheckInput)> = file_results
        .iter()
        .map(|(p, _)| p.clone())
        .zip(inputs)
        .collect();
    let (mut file_results, mut project_diags, _nodes_by_path) =
        reconcile_collected(file_results, &by_root, wip);

    project_compile_pass(&mut file_results, &mut project_diags, &inputs);
    fold_inherited_version_stale(&mut file_results, &mut project_diags, &inputs);

    // dsl 0.10.0 §9 rule 2.
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
            eprintln!("lute: cannot walk {} for mocks: {e}", dir.display());
            return ExitCode::from(2);
        }
    }

    // §5 verdict: a promoted (denied) diagnostic — in a per-file result OR the
    // project-wide set — fails an otherwise-clean project.
    let project_ok = !project_diags
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
                    d.message,
                );
            }
        }
        let project_error_count = project_diags
            .iter()
            .filter(|(_, d)| d.severity == Severity::Error || policy.denied(d))
            .count();
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
fn project_compile_pass(
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

/// The reconciled project analysis for the compile/trace project-aware gate
/// (connectivity design spec §5): per-document reconciled `CheckResult`s
/// (keyed by display path, [`BTreeMap`]-sorted for determinism) plus the
/// project-wide diagnostics. Produced by [`reconciled_project_results`] over a
/// SINGLE-ROOT collection (the whole `--project <dir>` is ONE root — §5's
/// single-root rule), reusing the SAME [`reconcile_collected`] analysis
struct ReconciledProject {
    per_doc: BTreeMap<PathBuf, lute_check::CheckResult>,
    project_diagnostics: Vec<(PathBuf, Diagnostic)>,
    /// Spec §5 gate signal: every node that is ON or DOWNSTREAM of a
    /// prerequisite cycle — absent from the sound topological order while its
    /// root has a cycle (`node_cycle_degraded`). A target hosting any of these
    /// blocks, even when the emitted `E-CONN-CYCLE` was anchored to a DIFFERENT
    /// node's file (see [`project_gate_result`]). Complete by Kahn's
    /// construction — no under-approximation of overlapping cycles.
    cycle_degraded: BTreeSet<lute_check::connectivity::NodeId>,
    /// Every graph node's `(id, span)` keyed by its declaring document — the
    /// gate's target-path -> hosted-nodes lookup.
    nodes_by_path: BTreeMap<PathBuf, Vec<(lute_check::connectivity::NodeId, Span)>>,
}

/// Collect + reconcile every `.lute` under `dir`, treating `dir` itself as THE
/// single project root for every file (connectivity spec §5: `--project <dir>`
/// resolves BOTH capabilities and connectivity against exactly that `<dir>`,
/// `load_project(dir)`, no nested nearest-root search — that directory-walk
/// discovery is `check-project`'s alone). The reusable seam the compile/trace
/// gate pulls the target document's reconciled `CheckResult` from.
/// `Err(ExitCode::from(2))` on the same I/O failures [`collect_project_docs`]
/// surfaces.
fn reconciled_project_results(
    dir: &Path,
    providers: Option<&Path>,
) -> Result<ReconciledProject, ExitCode> {
    let (file_results, by_root) = collect_project_docs(dir, providers, true)?;
    // Spec §5 gate: a target blocks when a node it hosts is ON or DOWNSTREAM
    // of a prerequisite cycle — i.e. absent from the sound topological order
    // while its root has a cycle. Decided by `node_cycle_degraded` over the
    // SAME `assemble_root_scenario` analysis `lute scenario` reports from, so a
    // target's gate verdict never disagrees with its `scenario reach` view.
    // This topological-order exclusion is COMPLETE (Kahn frees exactly the
    // cycle-INDEPENDENT nodes), where a DFS back-edge stack slice
    // under-approximates overlapping cycles. Built BEFORE `reconcile_collected`
    // consumes `file_results`.
    let mut cycle_degraded: BTreeSet<lute_check::connectivity::NodeId> = BTreeSet::new();
    for group_full in by_root.values() {
        let scenario = assemble_root_scenario(group_full, &file_results);
        for node in scenario.graph.nodes.keys() {
            if node_cycle_degraded(&scenario, node) {
                cycle_degraded.insert(node.clone());
            }
        }
    }
    let (file_results, project_diagnostics, nodes_by_path) =
        reconcile_collected(file_results, &by_root, false);
    Ok(ReconciledProject {
        per_doc: file_results.into_iter().collect(),
        project_diagnostics,
        cycle_degraded,
        nodes_by_path,
    })
}

/// The project-aware gate verdict for one `file` compiled/traced under
/// `--project <dir>` (connectivity design spec §5). Runs the SINGLE-ROOT
/// project reconciliation ([`reconciled_project_results`]) and returns the
/// TARGET document's own reconciled `CheckResult` MERGED with every
/// project-wide diagnostic anchored on that same file — so an
/// `E-STATE-MAYBE-UNAVAILABLE`/`E-CONN-*` fault on the target's OWN
/// `after`/reads blocks it, while a SIBLING document's project-only fault does
/// NOT (§5). Its `ok` is recomputed over the merged set.
///
/// **Out-of-tree (normative, §5):** if the canonicalized `file` is NOT within
/// `dir`'s recursively-collected `.lute` set, this errors EXPLICITLY
/// (`ExitCode::from(2)`) rather than silently falling back to a standalone
/// `check` — a silent fallback would mask a mistyped path or wrong `--project`.
fn project_gate_result(
    file: &Path,
    dir: &Path,
    providers: Option<&Path>,
) -> Result<lute_check::CheckResult, ExitCode> {
    let reconciled = reconciled_project_results(dir, providers)?;
    let target_canon = match std::fs::canonicalize(file) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("lute: cannot read {}: {e}", file.display());
            return Err(ExitCode::from(2));
        }
    };
    // Match the target within the project by CANONICAL identity (the collected
    // display path may differ in form from the CLI-supplied `file`, and
    // `find_lute_files` already dedupes symlink aliases by canonical identity).
    let matched = reconciled.per_doc.iter().find(|(path, _)| {
        std::fs::canonicalize(path)
            .map(|c| c == target_canon)
            .unwrap_or(false)
    });
    let Some((matched_key, base)) = matched else {
        eprintln!(
            "lute: {} is not within --project {} (the connectivity gate requires the target to be part of the project)",
            file.display(),
            dir.display()
        );
        return Err(ExitCode::from(2));
    };
    Ok(gate_for_doc(&reconciled, matched_key, base))
}

/// The spec §5 gate verdict for ONE already-reconciled document: its own
/// reconciled `CheckResult`, MERGED with every project-wide diagnostic anchored
/// on that same file — so an `E-STATE-MAYBE-UNAVAILABLE`/`E-CONN-*` fault on
/// this document's OWN `after`/reads blocks it, while a SIBLING's project-only
/// fault does not. `ok` is recomputed over the merged set.
///
/// Split out of [`project_gate_result`] so `compile --all`
/// ([`compile_all::run`]) can gate EVERY document off ONE project
/// reconciliation instead of re-running the whole single-root collection once
/// per file (which would be quadratic in project size, and could in principle
/// observe a project mid-edit differently between passes).
fn gate_for_doc(
    reconciled: &ReconciledProject,
    path: &PathBuf,
    base: &lute_check::CheckResult,
) -> lute_check::CheckResult {
    let mut result = base.clone();
    // §5: block on the TARGET's own reconciled diagnostics only — merge in
    // every project-wide diagnostic anchored on this same file (its own
    // `E-STATE-MAYBE-UNAVAILABLE`/`E-CONN-*`), never a sibling's.
    for (p, d) in &reconciled.project_diagnostics {
        if p == path {
            result.diagnostics.push(d.clone());
        }
    }
    // Spec §5 (topological-order exclusion): a target that HOSTS a node ON or
    // DOWNSTREAM of an `after`-precedence cycle must block even when the ONE
    // emitted `E-CONN-CYCLE` was anchored to a DIFFERENT node's file — the
    // target may be a non-anchored cyclic/downstream node with no own read
    // fault, which the merge above would let through. Synthesize a
    // TARGET-anchored `E-CONN-CYCLE` (reusing `lute_check`'s own
    // constructor/code) so `ok` recomputes false. Guarded on `E_CONN_CYCLE` NOT
    // already present so an already-anchored node (or one already carrying a
    // retained `E-MAYBE-UNSET`) never double-reports the cycle. Membership is
    // `cycle_degraded` — absence from the sound topological order given a cycle
    // (`node_cycle_degraded`), COMPLETE (no under-approximation of overlapping
    // cycles) and, per spec §5, covering BOTH on-cycle and strictly-downstream
    // targets.
    let already_cyclic = result
        .diagnostics
        .iter()
        .any(|d| d.code == lute_check::connectivity::E_CONN_CYCLE);
    if !already_cyclic {
        if let Some((id, span)) = reconciled.nodes_by_path.get(path).and_then(|hosted| {
            hosted
                .iter()
                .find(|(id, _)| reconciled.cycle_degraded.contains(id))
        }) {
            // Normalize line/col from the target's own text (mirrors
            // `reconcile_collected`'s project-diag normalization) — the raw
            // `character:`-key node span carries zeroed line/col otherwise.
            let text = std::fs::read_to_string(path).unwrap_or_default();
            let span = normalize_span_from_text(&text, *span);
            result
                .diagnostics
                .push(lute_check::connectivity::cycle_diag(
                    format!(
                    "prerequisite cycle: this document hosts `{id}`, which is on or downstream \
                     of an `after`-precedence cycle (E-CONN-CYCLE); no evaluation order can \
                     satisfy every `after` (dsl §2.4/§4.1 §A)"
                ),
                    span,
                ));
        }
    }
    result.ok = !result
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error);
    result
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

/// A bare scene-key, `quest:<id>`, `scene:<key>`, or `beat:<doc>.<beat>`
/// node reference, parsed from a `scenario reach`/`scenario envelope` CLI
/// argument (dsl §4.4's `envelope quest:<id>` syntax; `scene:<key>` and
/// `beat:<key>` are its symmetric counterparts -- see [`resolve_node_ref`]'s
/// doc comment for why explicit prefixes exist).
enum NodeRef {
    Scene(String),
    Quest(String),
    /// A bundle beat's canonical id (dsl 0.23.0 §4).
    Beat(String),
}

/// Parse an EXPLICIT `quest:<id>` / `scene:<key>` / `beat:<key>` prefix
/// only -- `None` for a bare (unprefixed) string, which [`resolve_node_ref`]
/// resolves against actual project candidates instead of guessing. An
/// explicit prefix is always authoritative: `quest:foo` is ALWAYS a quest
/// lookup and `scene:foo` is ALWAYS a scene lookup, never re-tried as
/// another kind (that would silently paper over a genuine "no such quest"
/// typo).
fn parse_node_ref_prefix(raw: &str) -> Option<NodeRef> {
    if let Some(id) = raw.strip_prefix("quest:") {
        return Some(NodeRef::Quest(id.to_string()));
    }
    if let Some(key) = raw.strip_prefix("scene:") {
        return Some(NodeRef::Scene(key.to_string()));
    }
    if let Some(key) = raw.strip_prefix("beat:") {
        return Some(NodeRef::Beat(key.to_string()));
    }
    None
}

fn node_ref_to_id(node: &NodeRef) -> lute_check::connectivity::NodeId {
    match node {
        NodeRef::Scene(key) => lute_check::connectivity::NodeId::Scene(key.clone()),
        NodeRef::Quest(id) => lute_check::connectivity::NodeId::Quest(id.clone()),
        NodeRef::Beat(key) => lute_check::connectivity::NodeId::Beat(key.clone()),
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
    /// Every bundle beat's canonical id (dsl 0.23.0 §4) with its
    /// declarations — each is a [`lute_check::connectivity::NodeId::Beat`]
    /// graph node (lamplight N8, ashen N9).
    beat_keys: BTreeMap<String, Vec<(PathBuf, Span)>>,
    quest_ids: BTreeSet<String>,
    ambiguous_quests: BTreeSet<String>,
    unreachable_quests: BTreeSet<String>,
    /// The subset of `unreachable_quests` that is unreachable via a
    /// PROVABLY dead REQUIRED objective (dsl 0.4.0 §8.2 rule C4 -- the
    /// cause C4 deliberately does NOT surface as a standalone
    /// `E-QUEST-UNREACHABLE`) -- kept SEPARATE from the lifecycle cause
    /// (`start=false`/`fail=true`) so [`reach_verdict_text`] can name the
    /// correct diagnostic code for each cause, never misattributing a C4
    /// note to the suppressed standalone code.
    dead_required_objective_quests: BTreeSet<String>,
    /// `D` (dsl §4.3 spec lines 442-448): the project-resolved `run.*`/
    /// `user.*` schema-defaulted set, unioned across every doc's own
    /// resolved schema in this root — [`envelope::quest_envelope`]'s own
    /// defaults-only floor.
    envelope_d: BTreeSet<String>,
    /// This root's plain (doc-stripped-of-`FoldedEnv`) docs — quest
    /// envelope printing needs the `&Quest` struct itself
    /// ([`envelope::quest_envelope`]'s signature), never re-parsed here.
    docs: Vec<(PathBuf, lute_syntax::ast::Document)>,
    /// T8/T9's per-document write sets, KEPT rather than consumed. Inverting
    /// `per_doc.scene` names the WRITERS of a path (#15, T9.14); the envelope
    /// already computed it and dropped it on the floor.
    per_doc: envelope::PerDocEffects,
    /// The root's relational vocabulary — declared relations, their arity and
    /// `derive` flag, the `facts:` seeds and the rules. The envelope tables
    /// are scalar-only, so at the scene whose every line is gated on who is
    /// awake the tool that exists to say what is true on arrival did not
    /// mention the subject (#15, T4.7).
    rel_vocab: lute_check::RelVocab,
    /// dsl 0.20.0 §6: per scene key, the facts guaranteed on arrival (the
    /// fact envelope beside the scalar one), each with where it is
    /// established.
    scene_must: BTreeMap<String, Vec<lute_check::fact_env::MustFact>>,
}

/// Assemble [`RootScenario`] for one resolved root's docs — mirrors
/// `run_check_project`'s own per-root block (T5 `assemble_graph`, T6
/// `check_reachability`, T8/T9 `PerDocEffects`, T10 `propagate`) verbatim,
/// minus the diagnostic emission (`lute scenario` reports, never gates).
fn assemble_root_scenario(
    group_full: &DocGroup,
    file_results: &[(PathBuf, lute_check::CheckResult)],
) -> RootScenario {
    let docs: Vec<(PathBuf, lute_syntax::ast::Document)> = group_full
        .iter()
        .map(|(p, d, _)| (p.clone(), d.clone()))
        .collect();
    let key_set = lute_check::connectivity::scene_key_set(&docs);
    let quest_ids = lute_check::connectivity::quest_id_set(&docs);
    let beat_keys = lute_check::connectivity::bundle_beat_key_set(&docs);
    let (graph, _cycle_diags) =
        lute_check::connectivity::assemble_graph(&docs, &key_set, &quest_ids);
    // T7/T14/Fix2 wiring: shares `compute_conn_fixpoint`'s finite-fixpoint
    // iteration with `run_check_project` (see that fn's own doc comment
    // for the termination + soundness argument) -- never re-derived
    // independently.
    let ambiguous_quests = lute_check::connectivity::ambiguous_quest_ids(&docs);
    let fp = compute_conn_fixpoint(
        &docs,
        group_full,
        file_results,
        &graph,
        &quest_ids,
        &ambiguous_quests,
    );
    let reach = fp.reach;
    let unreachable_quests = fp.unreachable_quests;
    let dead_required_objective_quests = fp.dead_required_objective_quests;
    let scene_must = fp.scene_must;

    let mut per_doc = envelope::PerDocEffects::default();
    let mut envelope_d: BTreeSet<String> = BTreeSet::new();
    let mut reads_per_scene: BTreeMap<String, Vec<(String, Span)>> = BTreeMap::new();
    let mut rel_vocab = lute_check::RelVocab::default();
    for (_path, doc, folded) in group_full {
        envelope_d.extend(envelope::schema_defaults(&folded.env.state));
        // Every doc in one resolved root folds the SAME imported vocabulary;
        // taking the last non-empty one matches how `check-project`'s own
        // project-wide relational passes read it.
        if !folded.env.rel_vocab.relations.is_empty() {
            rel_vocab = (*folded.env.rel_vocab).clone();
        }
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
    let mut group_ix: std::collections::HashMap<&Path, usize> =
        std::collections::HashMap::with_capacity(group_full.len());
    for (i, (p, _, _)) in group_full.iter().enumerate() {
        group_ix.entry(p.as_path()).or_insert(i);
    }
    for (key, occurrences) in &key_set {
        let Some((scene_path, _)) = occurrences.first() else {
            continue;
        };
        let Some((_, doc, folded)) = group_ix.get(scene_path.as_path()).map(|&i| &group_full[i])
        else {
            continue;
        };
        let all_nodes: Vec<lute_syntax::ast::Node> = doc
            .shots
            .iter()
            .flat_map(|s| s.body.iter().cloned())
            .collect();
        let scope = defassign::Scope::of(folded);
        let beat_when = folded.typed.beat.as_ref().and_then(|b| b.when.as_ref());
        let (_local_diags, assigned, reads) =
            check_definite_assignment(&all_nodes, &scope, beat_when);
        // Same T4.4/T4.6 carry-forward parity fix as `run_check_project`'s
        // T11 wiring above (dsl §7 soundness invariant) -- `lute scenario
        // envelope`/`reach` must not classify a domain-exhaustive `<match>`
        // subject read as entry-dependent either.
        let exhaustive_spans = defassign::exhaustive_match_subject_spans(&all_nodes, &scope);
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
            (
                envelope::guaranteed(&assigned),
                envelope::possible_writes(&all_nodes),
            ),
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
        beat_keys,
        quest_ids,
        ambiguous_quests,
        unreachable_quests,
        dead_required_objective_quests,
        envelope_d,
        docs,
        per_doc,
        rel_vocab,
        scene_must,
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
            NodeRef::Beat(key) => scenario
                .graph
                .nodes
                .contains_key(&lute_check::connectivity::NodeId::Beat(key.clone())),
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
        lute_check::PrereqFormula::Active(id) => {
            format!("active({})", quote_cel_string(id))
        }
        lute_check::PrereqFormula::And(l, r) => {
            format!("({} && {})", format_prereq(l), format_prereq(r))
        }
        lute_check::PrereqFormula::Or(l, r) => {
            format!("({} || {})", format_prereq(l), format_prereq(r))
        }
    }
}

/// Quote+escape a `visited`/`completed`/`active` atom id for CEL-like
/// rendering.
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
/// has no `reach` entry — a plain (no-`after`) quest is never a graph node at
/// all, and a graph node absent from `reach` is ON or DOWNSTREAM of a
/// prerequisite cycle (`E-CONN-CYCLE`): per-node cycle recovery (spec §4.1)
/// means `assemble_graph` omits exactly those nodes from `topo_order`/`reach`
/// while cycle-independent nodes keep their real verdicts.
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
        NodeId::Quest(id) if scenario.dead_required_objective_quests.contains(id) => {
            "Unreachable — this quest has a provably dead REQUIRED objective, so it can never \
             complete (E-OBJECTIVE-UNSATISFIABLE, dsl 0.4 §5.3/§8.2 rule C4), under your \
             declared routes."
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
        // dsl 0.21.0 §7a.5: a declared quest without `after=` is not a graph
        // node at all — UNANCHORED, available from the start of play.
        NodeId::Quest(id)
            if !scenario
                .graph
                .nodes
                .contains_key(&NodeId::Quest(id.clone())) =>
        {
            UNANCHORED_VERDICT.to_string()
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
        _ => "Unknown — this node is on or downstream of a prerequisite cycle (E-CONN-CYCLE); \
              its reachability is unavailable under your declared routes."
            .to_string(),
    }
}

/// dsl 0.21.0 §7a.5: the reach verdict of a declared quest without `after=`.
/// Its leading word is the JSON/DOT `unanchored` token's source
/// ([`scenario_fmt`]'s `reach_token` keys off it, like every other verdict).
const UNANCHORED_VERDICT: &str = "Unanchored — a quest with no declared `after` prerequisite: \
     available from the start of play; the connectivity layer holds no prerequisites for it, so \
     only its quest lifecycle (`start`, or an accept) decides when it activates.";

/// dsl 0.21.0 §7a.5: the declared quests the prerequisite graph does not
/// hold (no `after=`, no `::accept` anchor — dsl 0.24.0 §2 — and no
/// subquest tree or `start` anchor — dsl 0.25.0 §4), in id order — the
/// `unanchored` list every `lute scenario` graph view prints beside the
/// layers.
fn unanchored_quests(
    quest_ids: &BTreeSet<String>,
    graph: &lute_check::connectivity::ConnGraph,
) -> Vec<lute_check::connectivity::NodeId> {
    quest_ids
        .iter()
        .map(|id| lute_check::connectivity::NodeId::Quest(id.clone()))
        .filter(|node| !graph.nodes.contains_key(node))
        .collect()
}

/// Print `node`'s declared `after` STRUCTURE (dsl §5:575) — the raw formula
/// shape, `&&`/`||` intact (Main review: never flattened into a
/// predecessor list that could misrepresent a disjunction as a joint
/// requirement), plus each directly-referenced node's own reachability as
/// supplementary context (explicitly labeled "referenced", never "route" —
/// the formula above IS the route structure).
fn print_prereq_structure(
    out: &mut String,
    scenario: &RootScenario,
    node: &lute_check::connectivity::NodeId,
) {
    use lute_check::connectivity::PrereqState;
    match scenario.graph.nodes.get(node).map(|info| &info.prereq) {
        None if matches!(node, lute_check::connectivity::NodeId::Quest(id) if scenario.quest_ids.contains(id)) =>
        {
            outln!(
                out,
                "  after: (none declared) — unanchored: this quest is in no prerequisite graph \
                 layer and on no edge; it is available from the start of play."
            );
        }
        _ if matches!(node, lute_check::connectivity::NodeId::Beat(_)) => {
            print_bundle_beat_selection(out, scenario, node);
        }
        None | Some(PrereqState::Absent) => {
            outln!(
                out,
                "  after: (none declared) — this node is an entry point."
            );
        }
        Some(PrereqState::Invalid) => {
            outln!(
                out,
                "  after: (malformed — E-CONN-PROFILE; structure unavailable)"
            );
        }
        Some(prereq @ PrereqState::Valid(f)) => {
            outln!(out, "  after: {}", format_prereq(f));
            print_referenced(
                out,
                scenario,
                prereq,
                "`after` above for the && / || structure",
            );
        }
        Some(prereq @ PrereqState::Anchored(anchors)) => {
            outln!(
                out,
                "  after: (none declared) — anchored (dsl 0.24.0 §2, 0.25.0 §4); each anchor \
                 holds before it activates, through any one of its sources:"
            );
            for a in anchors {
                let from: Vec<String> = a.from.iter().map(|n| n.to_string()).collect();
                outln!(out, "    [{}] {}", a.kind.as_str(), from.join(" || "));
            }
            print_referenced(out, scenario, prereq, "the anchors above");
        }
    }
}

/// Each node `prereq` names, with its own reach verdict — context for the
/// structure printed above it (`see`), never a route list.
fn print_referenced(
    out: &mut String,
    scenario: &RootScenario,
    prereq: &lute_check::connectivity::PrereqState,
    see: &str,
) {
    let targets = prereq.referenced(&scenario.graph.nodes);
    if targets.is_empty() {
        return;
    }
    outln!(
        out,
        "  referenced node(s) (see {see} — this is NOT a flat requirement list):"
    );
    for target in &targets {
        outln!(
            out,
            "    - {target}: {}",
            reach_verdict_text(scenario, target)
        );
    }
}

/// A bundle beat's reach report (dsl 0.23.0 §4): its `after=` (dsl 0.25.0
/// §3) as a scene's `after:` is printed — the formula and the nodes it
/// references — else it is an entry node; then what selects it: its
/// occasion, target and `when`, printed as authored so the reader sees why
/// it is on the graph.
fn print_bundle_beat_selection(
    out: &mut String,
    scenario: &RootScenario,
    node: &lute_check::connectivity::NodeId,
) {
    use lute_check::connectivity::PrereqState;
    let Some(info) = scenario.graph.nodes.get(node) else {
        return;
    };
    match &info.prereq {
        prereq @ PrereqState::Valid(f) => {
            outln!(out, "  after: {}", format_prereq(f));
            print_referenced(
                out,
                scenario,
                prereq,
                "`after` above for the && / || structure",
            );
        }
        PrereqState::Invalid => {
            outln!(
                out,
                "  after: (malformed — E-CONN-PROFILE; structure unavailable)"
            );
        }
        _ => outln!(
            out,
            "  after: (none declared) — an entry node: it plays when its occasion is raised and \
             its `when` holds."
        ),
    }
    let lute_check::connectivity::NodeId::Beat(key) = node else {
        return;
    };
    let beat = scenario
        .docs
        .iter()
        .filter(|(p, _)| *p == info.path)
        .flat_map(|(_, d)| {
            let doc_id = lute_check::connectivity::bundle_id(d);
            d.beats.iter().map(move |b| (doc_id.clone(), b))
        })
        .find(|(doc_id, b)| {
            doc_id
                .as_deref()
                .is_some_and(|d| lute_check::bundles::bundle_beat_key(d, &b.id) == *key)
        })
        .map(|(_, b)| b);
    outln!(out, "  declared in: {}", info.path.display());
    if let Some(beat) = beat {
        if let Some((on, _)) = &beat.on {
            outln!(out, "  on: {on}");
        }
        if let Some((target, _)) = &beat.target {
            outln!(out, "  target: {target}");
        }
        if let Some(when) = &beat.when {
            outln!(out, "  when: {}", when.raw);
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
            let roots: Vec<String> = matches
                .iter()
                .map(|(r, _)| r.display().to_string())
                .collect();
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
/// - An EXPLICIT `quest:<id>` / `scene:<key>` / `beat:<key>` prefix
///   ([`parse_node_ref_prefix`]) is always authoritative — never re-tried as
///   another kind.
/// - A BARE (unprefixed) string is resolved against ACTUAL project
///   candidates: a declared scene key, a declared quest id, or a bundle
///   beat's canonical id (dsl 0.23.0 §4) in some root. Exactly one kind
///   matching → use it (the overwhelmingly common case — no prefix needed
///   at all). Two or more kinds matching is genuinely ambiguous — none is
///   silently preferred; the user is told to disambiguate with an explicit
///   prefix (mirrors [`primary_node_ambiguity_note`]'s honesty pattern:
///   never silently pick one candidate over another equally-valid one).
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
    let raw = node_id_raw.to_string();
    let mut found: Vec<(NodeRef, Vec<(&'a PathBuf, RootScenario)>)> = [
        NodeRef::Scene(raw.clone()),
        NodeRef::Quest(raw.clone()),
        NodeRef::Beat(raw),
    ]
    .into_iter()
    .map(|r| {
        let matches = find_matching_roots(by_root, file_results, &r);
        (r, matches)
    })
    .filter(|(_, matches)| !matches.is_empty())
    .collect();
    match found.len() {
        0 => {
            eprintln!("lute: unknown node `{node_id_raw}` under {}", dir.display());
            Err(ExitCode::from(2))
        }
        1 => {
            let (node_ref, matches) = found.pop().expect("len == 1");
            pick_unique_root(matches, dir, node_id_raw)
                .map(|(root, scenario)| (node_ref, root, scenario))
        }
        _ => {
            let kinds: Vec<&str> = found
                .iter()
                .map(|(r, _)| match r {
                    NodeRef::Scene(_) => "a scene key",
                    NodeRef::Quest(_) => "a quest id",
                    NodeRef::Beat(_) => "a bundle beat id",
                })
                .collect();
            let prefixes: Vec<String> = found
                .iter()
                .map(|(r, _)| match r {
                    NodeRef::Scene(_) => format!("`scene:{node_id_raw}`"),
                    NodeRef::Quest(_) => format!("`quest:{node_id_raw}`"),
                    NodeRef::Beat(_) => format!("`beat:{node_id_raw}`"),
                })
                .collect();
            eprintln!(
                "lute: node `{node_id_raw}` matches {} in this project -- ambiguous (none is \
                 silently preferred); disambiguate with an explicit {} prefix",
                kinds.join(" and "),
                prefixes.join(" or "),
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
        NodeRef::Beat(key) => {
            let occurrences = scenario.beat_keys.get(key)?;
            (occurrences.len() > 1).then(|| {
                format!(
                    "ambiguous bundle beat id (E-CONN-EPISODE-ID-DUP): `{key}` is declared by {} \
                     different lore documents in this project root, so a single reach/envelope \
                     report cannot be given.",
                    occurrences.len()
                )
            })
        }
    }
}

fn run_scenario_reach(
    out: &mut String,
    dir: &Path,
    by_root: &ByRoot,
    file_results: &[(PathBuf, lute_check::CheckResult)],
    node_id_raw: &str,
) -> ExitCode {
    let (node_ref, root, scenario) = match resolve_node_ref(dir, by_root, file_results, node_id_raw)
    {
        Ok(v) => v,
        Err(code) => return code,
    };
    let node_id = node_ref_to_id(&node_ref);
    outln!(out, "project root: {}", root.display());
    if let Some(note) = primary_node_ambiguity_note(&scenario, &node_ref) {
        outln!(out, "reach {node_id}: unavailable -- {note}");
        return ExitCode::SUCCESS;
    }
    outln!(out, "reach {node_id}:");
    outln!(
        out,
        "  verdict: {}",
        reach_verdict_text(&scenario, &node_id)
    );
    print_prereq_structure(out, &scenario, &node_id);
    ExitCode::SUCCESS
}

/// dsl 0.20.0 §6: the fact envelope — each fact guaranteed on arrival with
/// where it is established (an assert site, an enclosing guard, a seed).
fn print_must_facts(out: &mut String, facts: &[lute_check::fact_env::MustFact]) {
    if facts.is_empty() {
        outln!(out, "    (none)");
    }
    for m in facts {
        outln!(out, "    - {} ({})", m.fact, m.provenance);
    }
}

/// Every node from which `node` is transitively reachable in the prerequisite
/// graph — the writers whose writes provably happen BEFORE control reaches
/// it, which is the only thing a PRE-ENTRY envelope may claim. `g.edges` is
/// keyed `prerequisite -> dependent`, so this is a reverse walk.
///
/// `node` itself is excluded unless a cycle puts it upstream of itself; that
/// is deliberate — the tables are explicitly "before its own writes".
fn ancestors_of(
    g: &lute_check::connectivity::ConnGraph,
    node: &lute_check::connectivity::NodeId,
) -> BTreeSet<lute_check::connectivity::NodeId> {
    let mut rev: BTreeMap<
        &lute_check::connectivity::NodeId,
        Vec<&lute_check::connectivity::NodeId>,
    > = BTreeMap::new();
    for (prereq, deps) in &g.edges {
        for dep in deps {
            rev.entry(dep).or_default().push(prereq);
        }
    }
    let mut seen: BTreeSet<lute_check::connectivity::NodeId> = BTreeSet::new();
    let mut stack: Vec<&lute_check::connectivity::NodeId> =
        rev.get(node).cloned().unwrap_or_default();
    while let Some(n) = stack.pop() {
        if !seen.insert(n.clone()) {
            continue;
        }
        if let Some(ps) = rev.get(n) {
            stack.extend(ps.iter().copied());
        }
    }
    seen
}

/// Invert `PerDocEffects` into `path -> (writers upstream of `node`, the
/// rest)`. A scene contributes its own `possible_writes`; a quest contributes
/// its `writesOnComplete`, which is what draws manifest-gap.lute's completion
/// handler as a writer of `run.vesnaTrust` — the edge nobody could see,
/// though what-vesna-carries activates on exactly that path (#15, T9.14).
///
/// The split is load-bearing, and the plan asked for the join UNSPLIT. A flat
/// project-wide list names `scene(haven.s01ep09)` as a writer in
/// `haven.s01ep01`'s PRE-ENTRY envelope, which is false — eight scenes
/// separate them — and, being project-wide, it renders identically at every
/// node, which is the very defect #15 opens with. Dropping the non-upstream
/// half instead loses `quest(manifestGap)` from `quest:whatVesnaCarries`,
/// #15's own last verify bullet, because a no-`after` quest is never a graph
/// node and manifestGap is not upstream of it in any case. Both halves are
/// reported, each labelled with what is actually known about it: a
/// non-upstream writer may be downstream, unordered, or ungraphed, so the
/// second label claims only that its write is NOT provably before this node.
type WriterSplit = BTreeMap<String, (BTreeSet<String>, BTreeSet<String>)>;

fn writers_of(scenario: &RootScenario, node: &lute_check::connectivity::NodeId) -> WriterSplit {
    let upstream = ancestors_of(&scenario.graph, node);
    let mut out: WriterSplit = BTreeMap::new();
    let mut record = |path: &String, id: lute_check::connectivity::NodeId, label: String| {
        let entry = out.entry(path.clone()).or_default();
        if upstream.contains(&id) {
            entry.0.insert(label);
        } else {
            entry.1.insert(label);
        }
    };
    for (key, (_guaranteed, possible)) in &scenario.per_doc.scene {
        for path in possible {
            record(
                path,
                lute_check::connectivity::NodeId::Scene(key.clone()),
                format!("scene({key})"),
            );
        }
    }
    for (id, writes) in &scenario.per_doc.quest_writes_on_complete {
        for path in writes {
            record(
                path,
                lute_check::connectivity::NodeId::Quest(id.clone()),
                format!("quest({id}) on completion"),
            );
        }
    }
    out
}

fn join(set: &BTreeSet<String>) -> String {
    set.iter().cloned().collect::<Vec<_>>().join(", ")
}

/// Print a path set with each path's writers named beside it.
fn print_path_set_with_writers(out: &mut String, paths: &BTreeSet<String>, writers: &WriterSplit) {
    if paths.is_empty() {
        outln!(out, "    (none)");
        return;
    }
    for path in paths {
        let (upstream, other) = match writers.get(path) {
            Some(w) => (&w.0, &w.1),
            None => (&BTreeSet::new(), &BTreeSet::new()),
        };
        // A path in the envelope that nothing upstream writes got there from
        // the schema-default floor `D`. Saying so is the answer to "why is
        // this readable"; a blank column would read as a missing join.
        let head = if upstream.is_empty() {
            "(nothing on a declared route reaching here — schema default only)".to_string()
        } else {
            join(upstream)
        };
        let tail = if other.is_empty() {
            String::new()
        } else {
            format!(
                "; also written, but not provably before this node: {}",
                join(other)
            )
        };
        outln!(out, "    - {path}   written by: {head}{tail}");
    }
}

/// The relational layer the scalar envelope tables cannot show: every
/// declared relation, whether static analysis can produce it, and who makes
/// it true — in `lute scenario knowledge`'s words (T3-2: asserting
/// documents, seed facts, the engine for a `reserved` relation, the rules).
/// `producible` reads the SAME reachability-gated assert sites
/// (`live_assert_relations`) the `check-project` fact envelope seeds from;
/// this renders relation-level facts rather than deciding anything (#15,
/// T4.7).
fn print_facts_section(out: &mut String, scenario: &RootScenario, root: &Path) {
    let vocab = &scenario.rel_vocab;
    if vocab.relations.is_empty() {
        return;
    }
    let live = lute_check::connectivity::live_assert_relations(
        &scenario.docs,
        &scenario.reach,
        &scenario.ambiguous_quests,
        &scenario.unreachable_quests,
    );
    let producible = lute_check::producible::producible(vocab, &live);
    let per_doc = lute_check::connectivity::assert_relations_per_doc(&scenario.docs);

    outln!(
        out,
        "  Facts (the relational layer — declared relations, how each becomes true):"
    );
    for (name, decl) in &vocab.relations {
        let prod = if producible.get(name).copied().unwrap_or(false) {
            "producible"
        } else {
            "NOT producible by any declared route"
        };
        let writers: Vec<String> = per_doc
            .iter()
            .filter(|(_, rels)| rels.contains(name))
            .map(|(p, _)| p.strip_prefix(root).unwrap_or(p).display().to_string())
            .collect();
        let sites: Vec<&str> = writers.iter().map(String::as_str).collect();
        let by = knowledge::relation_producers(name, vocab, &sites);
        outln!(out, "    - {name}/{} ({prod}) — {by}", decl.args.len());
    }
    if !vocab.rules.is_empty() {
        outln!(out, "  Rules:");
        for r in &vocab.rules {
            outln!(out, "    - {}", r.raw);
        }
    }
}

/// True when the project's prerequisite graph contains a cycle (`E-CONN-CYCLE`,
/// dsl §2.4/§4.1 §A). Kahn's algorithm in `assemble_graph` emits every node
/// EXCEPT the cycle members and everything transitively downstream of one, so
/// a graph is cyclic iff `topo_order` is shorter than the node set — a
/// self-contained signal that needs no diagnostic replay.
fn graph_has_cycle(scenario: &RootScenario) -> bool {
    scenario.graph.topo_order.len() < scenario.graph.nodes.len()
}

/// True when `node` is ON or DOWNSTREAM of a prerequisite cycle
/// (`E-CONN-CYCLE`, dsl §2.4/§4.1 §A) — per-node cycle degradation (spec
/// §4.1). `assemble_graph` excludes exactly those nodes from `topo_order`, so
/// [`lute_check::connectivity::check_reachability`] AND [`envelope::propagate`]
/// (each iterating `topo_order`) populate NEITHER `reach` NOR `envs` for them;
/// a cycle-INDEPENDENT node keeps its real verdict and is never degraded. The
/// test is a node absent from `reach` in a root that does contain a cycle —
/// the same absence [`reach_verdict_text`]'s cycle arm keys off, reused
/// verbatim so a node's reach verdict and its envelope note never disagree.
fn node_cycle_degraded(scenario: &RootScenario, node: &lute_check::connectivity::NodeId) -> bool {
    !scenario.reach.contains_key(node) && graph_has_cycle(scenario)
}

/// Print the explicit per-node `E-CONN-CYCLE` degradation note (C-honesty,
/// persona review), mirroring [`reach_verdict_text`]'s cycle wording so a
/// cyclic-degraded node's envelope is never silently indistinguishable from a
/// genuinely-empty one. Printed ONLY for a node on or downstream of the cycle
/// (see [`node_cycle_degraded`]); a cycle-independent node prints its real
/// tables with no note. Prepended before the tables (which fall back to the
/// schema-default D/D floor when this node's `envs` entry is absent).
fn print_cycle_envelope_note(out: &mut String) {
    outln!(
        out,
        "  note: envelope unavailable — this node is on or downstream of a prerequisite cycle \
         (E-CONN-CYCLE); the Guaranteed/Possible tables below cannot be computed under your \
         declared routes and fall back to the schema-default floor."
    );
}

/// Print a scene node's Guaranteed/Possible envelope tables (T10) plus its
/// `Possible \ Guaranteed` warning-grade READS (contract #2): T11's
/// [`envelope::check_envelope`] already computes BOTH grades together and
/// returns them — `check-project` filters to `Severity::Error` only and
/// drops the warning grade; this RE-derives the SAME call, singleton-scoped
/// to `key` so every returned diagnostic necessarily belongs to this node,
/// and keeps the warning grade instead. Never a second classification pass
/// — `check_envelope` is reused verbatim, never re-implemented.
///
/// A bundle beat (`node_id` a `NodeId::Beat`) prints through here too: an
/// edgeless entry node, so its tables are the entry floor.
fn print_scene_envelope(
    out: &mut String,
    scenario: &RootScenario,
    node_id: &lute_check::connectivity::NodeId,
    key: &str,
    root: &Path,
) {
    outln!(
        out,
        "envelope for {node_id} (pre-entry — state available when control REACHES this node, \
         before its own writes):"
    );
    if node_cycle_degraded(scenario, node_id) {
        print_cycle_envelope_note(out);
    }
    if scenario.tainted.contains(node_id) {
        outln!(
            out,
            "  note: this node's envelope is a defaults-only placeholder -- its `after` \
             formula is malformed or references an unresolved node (E-CONN-PROFILE/\
             E-CONN-UNKNOWN-NODE)."
        );
    }
    let env = scenario
        .envs
        .get(node_id)
        .cloned()
        .unwrap_or_else(|| envelope::Env {
            guaranteed: scenario.envelope_d.clone(),
            possible: scenario.envelope_d.clone(),
        });
    let writers = writers_of(scenario, node_id);
    outln!(
        out,
        "  Guaranteed (safe to read under your declared routes):"
    );
    print_path_set_with_writers(out, &env.guaranteed, &writers);
    // T3-15: Possible ⊇ Guaranteed; print only what is new beside the table
    // above instead of every guaranteed path a second time.
    let possible_only: BTreeSet<String> =
        env.possible.difference(&env.guaranteed).cloned().collect();
    outln!(
        out,
        "  Possible (set on SOME but not every declared route reaching this node; the \
         Guaranteed paths above are not repeated):"
    );
    print_path_set_with_writers(out, &possible_only, &writers);
    outln!(
        out,
        "  Guaranteed facts (hold on every declared route reaching this node, dsl 0.20.0 §4):"
    );
    print_must_facts(out, scenario.scene_must.get(key).map_or(&[], Vec::as_slice));

    let mut single: BTreeMap<String, Vec<(String, Span)>> = BTreeMap::new();
    if let Some(reads) = scenario.reads_per_scene.get(key) {
        single.insert(key.to_string(), reads.clone());
    }
    let diags =
        envelope::check_envelope(&scenario.graph, &scenario.envs, &scenario.tainted, &single);
    outln!(
        out,
        "  Possible \\ Guaranteed -- warning-grade reads (set on SOME but not every declared \
         route; suppressed by default in `check-project`, dsl §6, surfaced here per §5):"
    );
    let mut any = false;
    for (path, d) in &diags {
        if d.severity != Severity::Warning {
            continue;
        }
        any = true;
        outln!(
            out,
            "    - {}:{}:{}: {}",
            path.display(),
            d.span.line,
            d.span.column,
            d.message
        );
    }
    if !any {
        outln!(out, "    (none)");
    }
    print_facts_section(out, scenario, root);
}

/// Print a quest node's envelope (T12 [`envelope::quest_envelope`]) — full
/// tables for an `after`-opted-in quest, defaults-only `D` plus the
/// enrichment note for a bare quest (dsl §4.4) — plus its `Possible \
/// Guaranteed` SET as plain inventory. [`envelope::check_envelope`] is
/// SCENE-ONLY by design (its own doc comment: quest reads stay
/// `check_quest_guard_defassign`'s territory), so this is NEVER labeled
/// as the T11 warning-grade read-site class (Main review) — there is no
/// read-SITE list for a quest at all, only the plain set difference.
fn print_quest_envelope(
    out: &mut String,
    scenario: &RootScenario,
    id: &str,
    quest: &lute_syntax::ast::Quest,
    root: &Path,
) {
    let node_id = lute_check::connectivity::NodeId::Quest(id.to_string());
    outln!(
        out,
        "envelope for {node_id} (pre-entry — state available when control REACHES this node, \
         before its own writes):"
    );
    // The E-CONN-CYCLE degradation note applies ONLY to a graph-positioned
    // quest (`after.is_some()`) that is itself cyclic/downstream:
    // `quest_envelope` returns the defaults-only D/D floor for an after-less
    // quest REGARDLESS of graph topology, so such a quest's tables did NOT
    // degrade due to the cycle -- and `node_cycle_degraded` would misfire on
    // it (a no-`after` quest is never a graph node, so it is trivially absent
    // from `reach`), so the `after.is_some()` guard is REQUIRED. A cycle-
    // independent `after` quest keeps its real tables with no note (per-node
    // recovery, spec §4.1); only a cyclic/downstream one prints the note.
    if quest.after.is_some() && node_cycle_degraded(scenario, &node_id) {
        print_cycle_envelope_note(out);
    }
    let qe = envelope::quest_envelope(quest, &scenario.graph, &scenario.envs, &scenario.envelope_d);
    let writers = writers_of(scenario, &node_id);
    outln!(
        out,
        "  Guaranteed (safe to read under your declared routes):"
    );
    print_path_set_with_writers(out, &qe.env.guaranteed, &writers);
    // T3-15: Possible ⊇ Guaranteed, so the full set printed every guaranteed
    // path twice; only the difference is new information.
    let possible_only: BTreeSet<String> = qe
        .env
        .possible
        .difference(&qe.env.guaranteed)
        .cloned()
        .collect();
    // #33 / T4.10: this sentence named `T11` (an internal task label) and
    // `check_quest_guard_defassign` (a Rust function) at an AUTHOR. Neither
    // appears anywhere on the website, so neither is lookupable. The
    // distinction the sentence exists to draw is real and is kept; only the
    // vocabulary changes. The doc comment above keeps both terms — that reader
    // has the source open, which is exactly the audience this message was
    // wrongly addressed to.
    outln!(
        out,
        "  Possible (set on SOME but not every declared route reaching this quest, dsl §4.4; \
         the Guaranteed paths above are not repeated) -- inventory only: unlike a scene's, this \
         list is not a set of warned read sites; a quest's guard reads are checked where they \
         are written, not against this table:"
    );
    print_path_set_with_writers(out, &possible_only, &writers);
    if qe.enrichment_note {
        outln!(
            out,
            "  note: this quest declares no `after` attribute, so this is the defaults-only \
             `D` table (dsl §4.4); declaring `after` on quest:{id} would enrich this table \
             with the full project-resolved envelope."
        );
    }
    print_facts_section(out, scenario, root);
}

fn run_scenario_envelope(
    out: &mut String,
    dir: &Path,
    by_root: &ByRoot,
    file_results: &[(PathBuf, lute_check::CheckResult)],
    node_id_raw: &str,
) -> ExitCode {
    let (node_ref, root, scenario) = match resolve_node_ref(dir, by_root, file_results, node_id_raw)
    {
        Ok(v) => v,
        Err(code) => return code,
    };
    outln!(out, "project root: {}", root.display());
    if let Some(note) = primary_node_ambiguity_note(&scenario, &node_ref) {
        let node_id = node_ref_to_id(&node_ref);
        outln!(out, "envelope for {node_id}: unavailable -- {note}");
        return ExitCode::SUCCESS;
    }
    match &node_ref {
        NodeRef::Scene(key) | NodeRef::Beat(key) => {
            print_scene_envelope(out, &scenario, &node_ref_to_id(&node_ref), key, root)
        }
        NodeRef::Quest(id) => {
            let Some(quest) = scenario
                .docs
                .iter()
                .flat_map(|(_, d)| d.quests.iter())
                .find(|q| &q.id == id)
            else {
                eprintln!("lute: internal error: quest `{id}` resolved but no declaration found");
                return ExitCode::from(2);
            };
            print_quest_envelope(out, &scenario, id, quest, root);
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

/// The comma-joined atom-kind token(s) justifying the `from -> to` edge
/// (lang 0.8.0) — `visited`, `completed`, `active`, or a combination when one
/// formula reaches the same node through more than one atom. Rendered from
/// `ConnGraph::edge_kinds_for`'s `BTreeSet`, so the order is `EdgeKind`'s own
/// and the output is deterministic. A `?` marks the impossible case of an
/// `edges` pair with no recorded kind — never fabricates a kind it cannot
/// read off the graph.
pub(crate) fn edge_kinds_text(
    graph: &lute_check::connectivity::ConnGraph,
    from: &lute_check::connectivity::NodeId,
    to: &lute_check::connectivity::NodeId,
) -> String {
    match graph.edge_kinds_for(from, to) {
        Some(kinds) => kinds
            .iter()
            .map(|k| k.as_str())
            .collect::<Vec<_>>()
            .join(", "),
        None => "?".to_string(),
    }
}

fn print_graph_for_root(
    out: &mut String,
    root: &Path,
    graph: &lute_check::connectivity::ConnGraph,
    unanchored: &[lute_check::connectivity::NodeId],
    when_visited: &[(lute_check::connectivity::NodeId, Vec<String>)],
    facts: Option<&FactGraph>,
) {
    outln!(out, "project root: {}", root.display());
    if graph.nodes.is_empty() {
        outln!(out, "  (no scene/quest nodes)");
        print_unanchored(out, unanchored, when_visited);
        return;
    }
    let layers = topo_layers(facts.map_or(graph, |f| &f.layered));
    if facts.is_some() {
        outln!(out, "  topological layers (after: and fact edges):");
    } else {
        outln!(out, "  topological layers:");
    }
    for (i, layer) in layers.iter().enumerate() {
        let names: Vec<String> = layer.iter().map(|n| n.to_string()).collect();
        outln!(out, "    layer {i}: {}", names.join(", "));
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
        outln!(
            out,
            "    (unlayered -- part of a prerequisite cycle, E-CONN-CYCLE): {}",
            stuck.join(", ")
        );
    }
    outln!(out, "  edges (prerequisite -> dependent) [atom kind(s)]:");
    let mut printed_any = false;
    for (from, targets) in &graph.edges {
        for to in targets {
            outln!(
                out,
                "    {from} -> {to} [{}]",
                edge_kinds_text(graph, from, to)
            );
            printed_any = true;
        }
    }
    if !printed_any {
        outln!(out, "    (none)");
    }
    if let Some(facts) = facts {
        outln!(out, "  fact edges (producer -> reader) [asserted fact]:");
        if facts.edges.is_empty() {
            outln!(out, "    (none)");
        }
        for (e, layered) in &facts.edges {
            let note = if *layered {
                ""
            } else if graph.nodes.contains_key(&e.from) {
                " (not layered: it closes a cycle)"
            } else {
                " (not layered: the producer is no graph node)"
            };
            outln!(
                out,
                "    {} -> {} [{}]{note}",
                e.from,
                e.to,
                fact_edge_label(e)
            );
        }
    }
    print_unanchored(out, unanchored, when_visited);
}

/// dsl 0.21.0 §7a.5: a quest without `after=` is in no layer and on no edge,
/// and used to be absent from this report entirely. Named here instead.
/// dsl 0.25.0 §3: so is a beat whose `when` reads `visited()` but that
/// declares no `after` — gated, yet drawn as an entry point — with the
/// `after` that would draw its edge ([`when_visited_hint`]).
fn print_unanchored(
    out: &mut String,
    unanchored: &[lute_check::connectivity::NodeId],
    when_visited: &[(lute_check::connectivity::NodeId, Vec<String>)],
) {
    if unanchored.is_empty() && when_visited.is_empty() {
        return;
    }
    outln!(
        out,
        "  unanchored (no `after` — available from the start of play; no prerequisites in this \
         graph):"
    );
    for node in unanchored {
        outln!(out, "    {node}");
    }
    for (node, ids) in when_visited {
        outln!(out, "    {node} — {}", when_visited_hint(node, ids));
    }
}

/// dsl 0.25.0 §3: the hint for a beat gated by `visited()` conjuncts of its
/// `when` that draw no edge — the `after` to write instead.
pub(crate) fn when_visited_hint(node: &lute_check::connectivity::NodeId, ids: &[String]) -> String {
    let reads: Vec<String> = ids.iter().map(|k| format!("visited('{k}')")).collect();
    let formula = reads.join(" && ");
    let write = match node {
        lute_check::connectivity::NodeId::Scene(_) => format!("`after: \"{formula}\"`"),
        _ => format!("`after=\"{formula}\"`"),
    };
    format!(
        "its `when` reads {}, which gates it but draws no edge; write {write} to anchor it \
         (dsl 0.25.0 §3)",
        reads.join(", ")
    )
}

/// dsl 0.23.0 §1: the prerequisite references the graph does not draw —
/// counted and named, so a missing edge is explained where it is missed. A
/// quest's edges come from its `after`, its subquest tree, its top-level
/// `start` conjuncts and its `::accept`s (dsl 0.24.0 §2, 0.25.0 §4).
fn print_omitted(out: &mut String, omitted: &[lute_check::connectivity::OmittedRef]) {
    use lute_check::connectivity::OmittedRef;
    if omitted.is_empty() {
        return;
    }
    outln!(
        out,
        "  note: {} `visited()`/`completed()`/`active()` reference(s) not drawn — a quest's \
         edges come from its `after`, its subquest tree, its `start` conjuncts and its \
         `::accept`s:",
        omitted.len()
    );
    for r in omitted {
        match r {
            OmittedRef::Lifecycle { from, kind, quest } => outln!(
                out,
                "    {from} -> {}(\"{quest}\") — quest({quest}) is on no edge (no `after`, tree, \
                 `start` anchor or `::accept`)",
                kind.as_str()
            ),
            // A condition read gates the quest but is no anchor; copying it
            // into `after` would replace the quest's real anchors (its
            // `::accept`s, tree, `start`) with a possibly backwards edge, so
            // the note never suggests it (summer S1).
            OmittedRef::Visited { quest, scene, slot } => outln!(
                out,
                "    quest({quest}) reads visited('{scene}') in its {slot} — a condition read, \
                 not an anchor"
            ),
        }
    }
}

fn run_scenario_graph(out: &mut String, by_root: &ByRoot, facts: bool) -> ExitCode {
    if by_root.is_empty() {
        outln!(out, "lute: no .lute files found");
        return ExitCode::SUCCESS;
    }
    for (root, group_full) in by_root {
        let docs: Vec<(PathBuf, lute_syntax::ast::Document)> = group_full
            .iter()
            .map(|(p, d, _)| (p.clone(), d.clone()))
            .collect();
        let key_set = lute_check::connectivity::scene_key_set(&docs);
        let quest_ids = lute_check::connectivity::quest_id_set(&docs);
        let (graph, _cycle_diags) =
            lute_check::connectivity::assemble_graph(&docs, &key_set, &quest_ids);
        let when_visited = lute_check::connectivity::when_visited_unanchored(&docs, &graph);
        let fact_graph = facts.then(|| FactGraph::of(group_full, &docs, &graph));
        print_graph_for_root(
            out,
            root,
            &graph,
            &unanchored_quests(&quest_ids, &graph),
            &when_visited,
            fact_graph.as_ref(),
        );
        print_omitted(
            out,
            &lute_check::connectivity::omitted_refs(&docs, &graph, &quest_ids),
        );
    }
    ExitCode::SUCCESS
}

/// dsl 0.26.0 §8 (T3-11, `lute scenario --facts`): the root's fact-producer
/// edges ([`lute_check::fact_edges::fact_edges`]) and the graph the layers
/// are drawn from — `after:` edges plus every fact edge between two graph
/// nodes, each added in edge order only where it closes no cycle (a fact
/// edge is a "may", so a cycle through one says nothing about `after:`).
pub(crate) struct FactGraph {
    /// Every edge, with whether it is layered.
    pub edges: Vec<(lute_check::fact_edges::FactEdge, bool)>,
    pub layered: lute_check::connectivity::ConnGraph,
}

impl FactGraph {
    pub(crate) fn of(
        group_full: &DocGroup,
        docs: &[(PathBuf, lute_syntax::ast::Document)],
        graph: &lute_check::connectivity::ConnGraph,
    ) -> Self {
        use lute_check::connectivity::NodeId;
        let foldeds: Vec<&lute_check::FoldedEnv> = group_full.iter().map(|(_, _, f)| f).collect();
        let mut layered = graph.clone();
        let reaches = |g: &lute_check::connectivity::ConnGraph, from: &NodeId, to: &NodeId| {
            let mut seen = BTreeSet::new();
            let mut stack = vec![from];
            while let Some(n) = stack.pop() {
                if n == to {
                    return true;
                }
                if seen.insert(n) {
                    stack.extend(g.edges.get(n).into_iter().flatten());
                }
            }
            false
        };
        let edges = lute_check::fact_edges::fact_edges(docs, &foldeds, graph)
            .into_iter()
            .map(|e| {
                let joins = graph.nodes.contains_key(&e.from)
                    && graph.nodes.contains_key(&e.to)
                    && (layered
                        .edges
                        .get(&e.from)
                        .is_some_and(|t| t.contains(&e.to))
                        || !reaches(&layered, &e.to, &e.from));
                if joins {
                    layered
                        .edges
                        .entry(e.from.clone())
                        .or_default()
                        .insert(e.to.clone());
                }
                (e, joins)
            })
            .collect();
        FactGraph { edges, layered }
    }
}

/// One fact edge's bracket: the asserted fact, and the gate's derived fact
/// it serves.
pub(crate) fn fact_edge_label(e: &lute_check::fact_edges::FactEdge) -> String {
    match &e.via {
        Some(via) => format!("{}, via {via}", e.fact),
        None => e.fact.clone(),
    }
}

/// `lute scenario` dispatch (dsl §5:571-584): reuses [`collect_project_docs`]
/// — the SAME per-root doc collection `check-project` builds — then routes
/// to the bare graph view, `reach`, or `envelope`.
fn run_scenario(
    dir: &Path,
    providers: Option<&Path>,
    command: Option<ScenarioCommand>,
    facts: bool,
) -> ExitCode {
    let (file_results, by_root) = match collect_project_docs(dir, providers, false) {
        Ok(v) => v,
        Err(code) => return code,
    };
    // T3-15: the report is built into one buffer and written once through
    // [`write_stdout`] — `lute scenario … | head` used to panic on EPIPE
    // from a bare `println!`.
    let mut out = String::new();
    let code = match command {
        None => run_scenario_graph(&mut out, &by_root, facts),
        Some(ScenarioCommand::Reach { node_id }) => {
            run_scenario_reach(&mut out, dir, &by_root, &file_results, &node_id)
        }
        Some(ScenarioCommand::Envelope { node_id }) => {
            run_scenario_envelope(&mut out, dir, &by_root, &file_results, &node_id)
        }
        Some(ScenarioCommand::Knowledge { for_node }) => {
            knowledge::run_text(&mut out, &by_root, &file_results, for_node.as_deref())
        }
    };
    if write_stdout(&out).is_err() {
        return ExitCode::from(2);
    }
    code
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
    permission_profile: Option<&str>,
) -> ExitCode {
    let Some(built) = build_input(file, providers, project, permission_profile) else {
        return ExitCode::from(2);
    };
    built.report_project_diags();
    let BuiltInput {
        input,
        resolve_error,
        ..
    } = built;
    // plugin 0.0.2 §2: an `E-` capability-resolution diagnostic (bad plugin
    // option, missing active plugin, bad identity template) is a build-failing
    // error; it printed above, and it MUST gate here or it would pass silently.
    if resolve_error {
        return ExitCode::from(1);
    }
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
    let mut surface = authoring_surface(
        &input,
        &folded.env.state,
        &folded.env.rel_vocab,
        &branch_paths,
        &reserved_quest_paths,
    );
    // dsl 0.22.0 §13: defs, built-in directives, project ids.
    context::extend_surface(&mut surface, &folded, file, project);

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
        .filter(|d| snap.permissions.allows_directive(&d.name))
        .filter(|d| {
            d.bridge.as_ref().is_none_or(|bridge| {
                snap.permissions
                    .allows_bridge(&bridge.service, &bridge.operation)
            })
        })
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

    // Bridge/reward vocabularies are authoring surfaces, not merely metadata.
    // Keep only entries admitted by the same effective snapshot policy the
    // checker and compiler enforce.
    let bridges: Vec<Value> = snap
        .bridge_capabilities
        .values()
        .filter(|bridge| {
            snap.permissions
                .allows_bridge(&bridge.service, &bridge.operation)
        })
        .map(|bridge| serde_json::to_value(bridge).unwrap_or(Value::Null))
        .collect();
    let reward_kinds = if snap.permissions.allows_rewards() {
        serde_json::to_value(&snap.reward_kinds).unwrap_or_else(|_| serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

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
            // dsl 0.22.0 §1.2: engine-owned paths are read-only to content.
            if let Some(owner) = &decl.owner {
                o.insert(
                    "owner".into(),
                    serde_json::to_value(owner).unwrap_or(Value::Null),
                );
            }
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
            // The write tier (`run` when unset) and whether only the engine
            // asserts it (`reserved: true`) — a derived relation has neither.
            if !decl.derive {
                o.insert(
                    "tier".into(),
                    decl.tier.clone().unwrap_or_else(|| "run".into()).into(),
                );
            }
            o.insert("reserved".into(), decl.reserved.into());
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
        (
            "mono",
            "interior monologue / thought (not spoken aloud in-scene)",
        ),
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
    root.insert(
        "permissions".into(),
        serde_json::to_value(&snap.permissions)
            .unwrap_or_else(|_| serde_json::json!({ "layers": [] })),
    );
    root.insert("directives".into(), directives.into());
    root.insert("bridges".into(), bridges.into());
    root.insert("rewardKinds".into(), reward_kinds);
    // dsl 0.23.0 §7: the declared cast (plugins ∪ imported schemas) the
    // speakers are checked against; omitted while speakers are shape-only.
    let cast = lute_check::declared_cast(&input.snapshot, &input.imports, &[]);
    if !cast.is_empty() {
        root.insert(
            "cast".into(),
            serde_json::to_value(&cast).unwrap_or_else(|_| serde_json::json!({})),
        );
    }
    // dsl 0.21.0 §2: the occasion vocabulary beats answer with `on:`
    // (empty = shape-only). Key-sorted by the snapshot's BTreeMap.
    root.insert(
        "occasions".into(),
        serde_json::to_value(&snap.occasions).unwrap_or_else(|_| serde_json::json!({})),
    );
    root.insert(
        "questsAllowed".into(),
        snap.permissions.allows_quests().into(),
    );
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
    root.insert(
        "reservedQuestPaths".into(),
        reserved_quest_paths_json.into(),
    );
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
        Type::ProviderRef(_)
        | Type::Domain(_)
        | Type::Entity(_)
        | Type::SlotId { .. }
        | Type::AssetKind(_) => ("string".to_string(), None),
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
        Type::Entity(kind) => (format!("entity:{kind}"), None),
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
            m.iter()
                .map(|(k, v)| (k.clone(), literal_json(v)))
                .collect(),
        ),
    }
}

/// A compact human outline of the authoring surface (non-`--json` mode): the
/// capabilityVersion, directive names + attr keys + semantics flags, enum
/// names WITH their members, state paths (with enum domains), the referenced
/// reserved quest paths (dsl 0.5.1 §2), the relational vocabulary (entity
/// kinds, relations w/ arity+domains+`derive`, seed facts, rules,
/// project-level enums), the fixed delivery-flag vocabulary (dsl 0.5.1 §3),
/// and component names. `--json` is the machine surface; this is a short
/// at-a-glance view.
fn context_outline(surface: &serde_json::Value) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "capabilityVersion: {}",
        surface["capabilityVersion"].as_str().unwrap_or("")
    );
    let permissions = serde_json::to_string(&surface["permissions"])
        .unwrap_or_else(|_| "{\"layers\":[]}".to_string());
    let _ = writeln!(
        out,
        "permissions: {permissions} (authoring/compile-time restrictions; not runtime sandbox enforcement)"
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
            // #32 / T2.5: `--json` has always carried these and the human
            // outline dropped them. `mayExitCharacter` is the machine-readable
            // statement that `::auto` is the construct that ends a presence,
            // and it is on no page of the shipped website.
            let semantics: Vec<&str> = d["semantics"]
                .as_array()
                .map(|a| a.iter().filter_map(|x| x.as_str()).collect())
                .unwrap_or_default();
            let sem = if semantics.is_empty() {
                String::new()
            } else {
                format!("   [{}]", semantics.join(" "))
            };
            let _ = writeln!(out, "  {name}{layer}: {}{sem}", attrs.join(", "));
        }
    }
    if let Some(bridges) = surface["bridges"].as_array() {
        let _ = writeln!(out, "bridges ({}):", bridges.len());
        for bridge in bridges {
            let service = bridge["service"].as_str().unwrap_or("");
            let operation = bridge["operation"].as_str().unwrap_or("");
            let _ = writeln!(out, "  {service}/{operation}");
        }
    }
    if let Some(cast) = surface.get("cast").and_then(serde_json::Value::as_object) {
        let _ = writeln!(out, "cast ({}):", cast.len());
        for (id, m) in cast {
            match m["name"].as_str() {
                Some(name) => {
                    let _ = writeln!(out, "  {id} — {name}");
                }
                None => {
                    let _ = writeln!(out, "  {id}");
                }
            }
        }
    }
    if let Some(reward_kinds) = surface["rewardKinds"].as_object() {
        let _ = writeln!(out, "rewardKinds ({}):", reward_kinds.len());
        for name in reward_kinds.keys() {
            let _ = writeln!(out, "  {name}");
        }
    }
    if let Some(occasions) = surface["occasions"].as_object() {
        let _ = writeln!(out, "occasions ({}):", occasions.len());
        for (name, o) in occasions {
            let select = o["select"].as_str().unwrap_or("first");
            // dsl 0.22.0 §8: a domain target names its vocabulary.
            let target = match &o["target"] {
                serde_json::Value::Bool(true) => ", target".to_string(),
                serde_json::Value::Object(d) => format!(
                    ", target: {}.<{}>",
                    d.get("prefix").and_then(|v| v.as_str()).unwrap_or(""),
                    d.get("entity").and_then(|v| v.as_str()).unwrap_or("")
                ),
                _ => String::new(),
            };
            let description = o["description"]
                .as_str()
                .map(|d| format!(" — {d}"))
                .unwrap_or_default();
            let _ = writeln!(out, "  {name} (select: {select}{target}){description}");
        }
    }
    let _ = writeln!(
        out,
        "questsAllowed: {}",
        surface["questsAllowed"].as_bool().unwrap_or(true)
    );
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
            let owner = s["owner"]
                .as_str()
                .map(|o| format!(" (owner: {o})"))
                .unwrap_or_default();
            let _ = writeln!(out, "  {path}: {ty}{dom}{owner}");
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
                // `[derive]`, or the write tier plus `reserved` (engine-asserted).
                let tag = if r["derive"].as_bool().unwrap_or(false) {
                    " [derive]".to_string()
                } else {
                    let tier = r["tier"].as_str().unwrap_or("run");
                    if r["reserved"].as_bool().unwrap_or(false) {
                        format!(" [{tier}, reserved]")
                    } else {
                        format!(" [{tier}]")
                    }
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
            // Signatures, not just names: `::use` binds these by name.
            let _ = writeln!(out, "components ({}):", comps.len());
            for c in comps {
                let params: Vec<String> = c["params"]
                    .as_array()
                    .map(|ps| {
                        ps.iter()
                            .map(|p| {
                                let dom = p["domain"]
                                    .as_array()
                                    .map(|d| {
                                        let m: Vec<&str> =
                                            d.iter().filter_map(|x| x.as_str()).collect();
                                        format!("[{}]", m.join(", "))
                                    })
                                    .unwrap_or_default();
                                format!(
                                    "{}: {}{dom}",
                                    p["name"].as_str().unwrap_or(""),
                                    p["type"].as_str().unwrap_or("")
                                )
                            })
                            .collect()
                    })
                    .unwrap_or_default();
                let _ = writeln!(
                    out,
                    "  {}({})",
                    c["name"].as_str().unwrap_or(""),
                    params.join(", ")
                );
            }
        }
    }
    context::outline_extras(&mut out, surface);
    out
}

/// Route `lute compile` to the single-file ([`run_compile`]) or whole-project
/// ([`compile_all::run`]) path, rejecting every flag combination that means
/// neither.
///
/// `--all` REQUIRES `--project <dir>` (it has no other way to know which
/// documents belong to the project, and the capability snapshot resolves per
/// project) and `-o <dir>` (there is no single artifact to put on stdout). It
/// also takes no `<file>`: naming one would imply the other documents are
/// somehow secondary, which they are not. Every violation is exit `2`, the
/// usage tier — clap cannot express these dependencies itself, so they are
/// checked here and reported in clap's own voice.
#[allow(clippy::too_many_arguments)]
fn dispatch_compile(
    file: Option<&Path>,
    json: bool,
    providers: Option<&Path>,
    project: Option<&Path>,
    permission_profile: Option<&str>,
    out: Option<&Path>,
    all: bool,
    locales: Option<&Path>,
    policy: &DenyPolicy,
) -> ExitCode {
    if !all {
        let Some(file) = file else {
            eprintln!(
                "error: the following required arguments were not provided:\n  <FILE>\n\n\
                 Usage: lute compile <FILE>\n       lute compile --all --project <DIR> -o <DIR>"
            );
            return ExitCode::from(2);
        };
        return run_compile(
            file,
            json,
            providers,
            project,
            permission_profile,
            out,
            locales,
            policy,
        );
    }

    let mut usage: Vec<&str> = Vec::new();
    if project.is_none() {
        usage.push("--all requires --project <DIR> (the document set and capability snapshot both resolve per project)");
    }
    if out.is_none() {
        usage.push("--all requires -o <DIR>, an output DIRECTORY (there is no single artifact to write to stdout)");
    }
    if file.is_some() {
        usage.push("--all takes no <FILE>: it compiles every document under --project");
    }
    if !usage.is_empty() {
        for message in usage {
            eprintln!("error: {message}");
        }
        eprintln!("\nUsage: lute compile --all --project <DIR> -o <DIR>");
        return ExitCode::from(2);
    }
    let bundle = match locales.map(load_locale_bundle).transpose() {
        Ok(b) => b,
        Err(code) => return code,
    };
    compile_all::run(
        project.expect("checked above"),
        out.expect("checked above"),
        providers,
        permission_profile,
        json,
        bundle.as_ref(),
        policy,
    )
}

/// Run `compile` over one file. Exit `0` with the artifact on stdout (or
/// `-o <FILE>`), `1` when the check gate fails (diagnostics to stdout,
/// human or `--json`), `2` on I/O or serialization failure.
///
/// With `--locales <bundle.json>` the compiled artifact additionally carries
/// per-record locale texts ([`load_locale_bundle`] then
/// [`lute_compile::locale::merge_locales`], dsl 0.8.0 §7). Any resulting
/// `W-L10N-MISSING` prints to STDERR — stdout may be carrying the artifact —
/// and, when `--deny` promotes it, flips the verdict to `1` with NO artifact
/// written.
fn run_compile(
    file: &Path,
    json: bool,
    providers: Option<&Path>,
    project: Option<&Path>,
    permission_profile: Option<&str>,
    out: Option<&Path>,
    locales: Option<&Path>,
    policy: &DenyPolicy,
) -> ExitCode {
    // Loaded BEFORE the compile so a malformed bundle fails fast, before any
    // work — and, with `-o`, before the previous artifact is overwritten.
    let bundle = match locales.map(load_locale_bundle).transpose() {
        Ok(b) => b,
        Err(code) => return code,
    };
    let Some(built) = build_input(file, providers, project, permission_profile) else {
        return ExitCode::from(2);
    };
    built.report_project_diags();
    let BuiltInput {
        input,
        resolve_error,
        ..
    } = built;
    // plugin 0.0.2 §2: an `E-` capability-resolution diagnostic (bad plugin
    // option, missing active plugin, bad identity template) is a build-failing
    // error; it printed above, and it MUST gate here or it would pass silently.
    if resolve_error {
        return ExitCode::from(1);
    }
    // Project-aware gate (connectivity spec §5): WITH `--project <dir>` the
    // target compiles against its RECONCILED `check-project` verdict (an
    // envelope-Guaranteed `run.*`/`user.*` read no longer blocks; a read no
    // route guarantees blocks with `E-STATE-MAYBE-UNAVAILABLE`). WITHOUT it,
    // the standalone single-file `check` gate, unchanged.
    // 0.8.0 §9: the `identity:` block templates `lineId`/`voiceKey`. It is a
    // PROJECT setting, so it only applies on the `--project` path; a loose
    // scene keeps `IdentityTemplates::default()`, i.e. 0.7.0's pair. A project
    // that fails to load already printed its error in `build_input`; falling
    // back to the default here matches that core-only degradation.
    let (gate, identity) = match project {
        Some(dir) => {
            let identity = load_project(dir)
                .ok()
                .flatten()
                .map(|p| p.identity)
                .unwrap_or_default();
            match project_gate_result(file, dir, providers) {
                Ok(gate) => (gate, identity),
                Err(code) => return code,
            }
        }
        None => (check(&input), Default::default()),
    };

    // A component is not a root document (see [`component_root_diag`]): there is
    // no standalone artifact to emit. Refused AFTER the gate, so a component
    // with real check errors still reports them.
    let compiled = match component_name_of(file).filter(|_| gate.ok) {
        Some((component, at)) => Err(vec![component_root_diag(&component, at)]),
        None => lute_compile::compile_with_check(&input, gate, &identity),
    };
    match compiled {
        Ok(mut artifact) => {
            // dsl 0.8.0 §7: merge strictly downstream of the addressing pass —
            // `compile_with_check` has already stamped every final `lineId`,
            // which is the ONLY key a bundle joins on.
            if let Some(bundle) = &bundle {
                let missing = lute_compile::locale::merge_locales(&mut artifact, bundle);
                // STDERR, not stdout: without `-o` the artifact itself is on
                // stdout, and a warning line in the middle of it would make
                // the compile output unparseable.
                eprint!("{}", render_diagnostics(file, &missing, policy));
                let denied = missing.iter().filter(|d| policy.denied(d)).count();
                if denied > 0 {
                    eprintln!("--deny promoted {denied} diagnostic(s); no artifact emitted");
                    return ExitCode::FAILURE;
                }
            }
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

/// Read + parse a `--locales <bundle.json>` file (dsl 0.8.0 §7). A missing or
/// unreadable file is I/O (`2`, matching every other file the CLI opens); a
/// file that IS readable but is not a bundle is `E-LOCALE-BUNDLE` (`1`) — the
/// same code `lute loc import` reports for a malformed input, so the two ends
/// of the round trip name the same defect the same way.
fn load_locale_bundle(path: &Path) -> Result<lute_compile::locale::LocaleBundle, ExitCode> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        eprintln!("lute: cannot read {}: {e}", path.display());
        ExitCode::from(2)
    })?;
    lute_compile::locale::LocaleBundle::parse(&text).map_err(|msg| {
        eprintln!("{}: error [{}] {msg}", path.display(), loc::E_LOCALE_BUNDLE);
        ExitCode::FAILURE
    })
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
    ))
}

/// dsl 0.24.0 T3-15: every `<quest id>` declared under the `--project`
/// root — what `lute trace --project` settles its "existence is unverified"
/// notes against. `None` when the project cannot be collected.
fn project_quest_ids(root: &Path, providers: Option<&Path>) -> Option<BTreeSet<String>> {
    let (_, by_root) = collect_project_docs(root, providers, true).ok()?;
    let docs: Vec<(PathBuf, lute_syntax::ast::Document)> = by_root
        .get(root)?
        .iter()
        .map(|(p, d, _)| (p.clone(), d.clone()))
        .collect();
    Some(lute_check::connectivity::quest_id_set(&docs))
}

/// Run `trace` over one file (dsl 0.4.0 §4.3/§4.5): resolve the document
/// IDENTICALLY to `check`/`compile` ([`build_input`]), load + merge the
/// `--mock` file with the CLI's own `--state`/`--fact`/`--choose`/`--event`/
/// `--accept`/`--occasion` flags into one [`MockSet`] ([`merge`] — "CLI flags compose with
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
#[allow(clippy::too_many_arguments)]
fn run_trace(
    file: &Path,
    state: Vec<(String, String)>,
    fact: Vec<String>,
    choose: Vec<(String, Vec<String>)>,
    event: Vec<String>,
    accept: Vec<String>,
    occasion: Vec<String>,
    mock: Option<&Path>,
    json: bool,
    providers: Option<&Path>,
    project: Option<&Path>,
    entry: Option<&str>,
    beat: Option<&str>,
    no_derive: bool,
    expand: bool,
) -> ExitCode {
    let Some(built) = build_input(file, providers, project, None) else {
        return ExitCode::from(2);
    };
    built.report_project_diags();
    let BuiltInput {
        input,
        resolve_error,
        ..
    } = built;
    // plugin 0.0.2 §2: an `E-` capability-resolution diagnostic (bad plugin
    // option, missing active plugin, bad identity template) is a build-failing
    // error; it printed above, and it MUST gate here or it would pass silently.
    if resolve_error {
        return ExitCode::from(1);
    }

    // dsl 0.19.0 §8: a lore document is looked up, not played — there is no
    // sequence to walk, so tracing one without `--entry` / `--beat` (dsl
    // 0.23.0 §4) is a usage error. (Either flag on a non-lore document / an
    // unknown id is `E-TRACE-ENTRY` / `E-TRACE-BEAT`, refused by `lute_trace`
    // below.)
    if entry.is_none() && beat.is_none() {
        let (doc, _) = lute_syntax::parse(&input.text);
        let (folded, _, _) = lute_check::fold_env(&doc, &input);
        if folded.doc_kind == lute_check::DocKind::Lore {
            let mut ways = Vec::new();
            if !doc.entries.is_empty() || doc.beats.is_empty() {
                let ids: Vec<&str> = doc.entries.iter().map(|e| e.id.as_str()).collect();
                ways.push(format!(
                    "`--entry <id>` to present one entry (declared: {})",
                    ids.join(", ")
                ));
            }
            if !doc.beats.is_empty() {
                let ids: Vec<String> = doc
                    .beats
                    .iter()
                    .map(|b| match folded.typed.id.as_deref() {
                        Some(doc_id) => lute_check::bundle_beat_key(doc_id, &b.id),
                        None => b.id.clone(),
                    })
                    .collect();
                ways.push(format!(
                    "`--beat <id>` to present one bundle beat (declared: {})",
                    ids.join(", ")
                ));
            }
            eprintln!(
                "lute trace: {} is a lore document — pass {} (dsl 0.19.0 §8, 0.23.0 §4)",
                file.display(),
                ways.join(" or ")
            );
            return ExitCode::from(2);
        }
    }

    let file_mocks = match mock {
        Some(path) => {
            let text = match std::fs::read_to_string(path) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("lute: cannot read {}: {e}", path.display());
                    return ExitCode::from(2);
                }
            };
            // D-AC: the command line supplies the subject and it wins. A
            // `file:` that names a DIFFERENT document is the error — the two
            // ways of saying what a mock is for must not disagree in silence.
            match lute_trace::mock_subject(&text) {
                Ok(Some(rel)) => {
                    let base = path.parent().unwrap_or_else(|| Path::new("."));
                    let named = std::fs::canonicalize(base.join(&rel)).ok();
                    let target = std::fs::canonicalize(file).ok();
                    if named.is_none() || named != target {
                        eprintln!(
                            "lute: {}: [{}] `file: {rel}` names a different document than the one \
                             traced ({}) — the mock's subject and the command line must agree \
                             (0.10.0 §8)",
                            path.display(),
                            lute_trace::E_MOCK_SUBJECT,
                            file.display()
                        );
                        return ExitCode::from(2);
                    }
                }
                Ok(None) => {}
                Err(d) => {
                    eprintln!("lute: {}: [{}] {}", path.display(), d.code, d.message);
                    return ExitCode::from(2);
                }
            }
            match parse_mock_yaml(&text) {
                Ok(m) => m,
                Err(d) => {
                    // A malformed `--mock` YAML document is a file-level I/O/
                    // format failure, not a schema-validation refusal — `2`,
                    // matching `run_check`'s/`run_compile`'s read-failure tier.
                    //
                    // Rendered WITHOUT a line:column (D-AB). Every mock
                    // diagnostic carries `synthetic_span()`'s all-zeros, so
                    // the old `{line}:{column}` printed `mock.yaml:0:0` — a
                    // position that does not exist, which is the exact defect
                    // §8 opens with. The subject arm above already renders
                    // this way; now the grammar arm does too.
                    eprintln!("lute: {}: [{}] {}", path.display(), d.code, d.message);
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
    let span = lute_core_span::Span {
        byte_start: 0,
        byte_end: 0,
        line: 0,
        column: 0,
        utf16_range: (0, 0),
    };
    let flag_mocks = MockSet {
        state: state
            .into_iter()
            .map(|(path, literal)| (path, literal, span))
            .collect(),
        facts: fact,
        choose: choose.into_iter().collect(),
        events: event,
        accepts: accept,
        occasions: occasion,
        visited: Vec::new(),
        derive: no_derive.then_some(false),
        bridges: Default::default(),
        bridge_spans: Default::default(),
        gate_eligibility: false,
        project_quests: None,
    };

    let mut mocks = merge(file_mocks, flag_mocks);
    // dsl 0.26.0 §7 (T3-5): `--accept` / `accepts:` resolve against every
    // quest of the project, not only this document's.
    if !mocks.accepts.is_empty() {
        mocks.project_quests = project.and_then(|dir| project_quest_ids(dir, providers));
    }
    // Project-aware gate (connectivity spec §5, mirrors `run_compile`): WITH
    // `--project <dir>` trace gates on the target's RECONCILED `check-project`
    // verdict; WITHOUT it, the standalone single-file `check` gate, unchanged.
    // The D1 quarantine holds — reconciliation is pure graph math, never
    // CEL/Datalog evaluation.
    let gate = match project {
        Some(dir) => match project_gate_result(file, dir, providers) {
            Ok(gate) => gate,
            Err(code) => return code,
        },
        None => check(&input),
    };

    // A component is not a root document (see [`component_root_diag`]). Refused
    // AFTER the gate above, so a component carrying real check errors still
    // reports them first — the kind refusal is what replaces the bare `ok`
    // path, not the diagnostic path.
    if gate.ok {
        if let Some((component, at)) = component_name_of(file) {
            let diag = component_root_diag(&component, at);
            if json {
                match serde_json::to_string_pretty(&[&diag]) {
                    Ok(s) => println!("{s}"),
                    Err(e) => {
                        eprintln!("lute: failed to serialize diagnostics: {e}");
                        return ExitCode::from(2);
                    }
                }
            } else {
                print_diagnostics(file, std::slice::from_ref(&diag), &DenyPolicy::default());
                println!(
                    "trace refused: {} is a component — trace a document that `::use`s it",
                    file.display()
                );
            }
            return ExitCode::from(1);
        }
    }

    // T1-14: judge mocked facts against the project's producers, not this
    // document's alone — `--project` when given, else the nearest manifest.
    // Only worth collecting when a fact was mocked at all.
    let project_asserts = if mocks.facts.is_empty() {
        None
    } else {
        match project {
            Some(dir) => project_assert_relations(dir, true, providers),
            None => nearest_manifest_dir(file)
                .and_then(|root| project_assert_relations(&root, false, providers)),
        }
    };
    let (mut report, exit) = match (entry, beat) {
        (Some(id), _) => {
            lute_trace::trace_entry_with_check(&input, gate, mocks, id, project_asserts.as_ref())
        }
        (None, Some(id)) => {
            lute_trace::trace_beat_with_check(&input, gate, mocks, id, project_asserts.as_ref())
        }
        (None, None) => lute_trace::trace_with_check(&input, gate, mocks, project_asserts.as_ref()),
    };
    // T3-15: `--project` knows every quest of the project — settle the
    // "existence is unverified" notes instead of repeating them.
    if !report.foreign_quests.is_empty() {
        if let Some(declared) = project.and_then(|dir| project_quest_ids(dir, providers)) {
            report.verify_quests(&declared);
        }
    }

    match exit {
        TraceExit::Complete => print_trace_report(&report, json, expand, ExitCode::SUCCESS),
        TraceExit::Incomplete => print_trace_report(&report, json, expand, ExitCode::from(3)),
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
                // dsl 0.25.0 §1: a walk-time exclusive-relations refusal keeps
                // its transcript — the `✗ exclusive` line sits at the write.
                let exclusive = !diags.is_empty()
                    && diags
                        .iter()
                        .all(|d| d.code == lute_check::fact_check::E_FACT_EXCLUSIVE);
                if exclusive && write_stdout(&report.render_human()).is_err() {
                    return ExitCode::from(2);
                }
                // A `bridges:` answer's diagnostic is anchored in the mock's
                // own text (dsl 0.24.0 §5), so it renders against the mock.
                let (at_mock, at_doc): (Vec<_>, Vec<_>) = diags
                    .iter()
                    .cloned()
                    .partition(|d| d.provenance.as_deref() == Some(lute_trace::MOCK_TEXT));
                print_diagnostics(file, &at_doc, &DenyPolicy::default());
                print_diagnostics(mock.unwrap_or(file), &at_mock, &DenyPolicy::default());
                // Every `E-TRACE-*` code is mock/choice validation (D1
                // quarantine: `lute-check` cannot know that vocabulary, so
                // its OWN diagnostics never carry it) — a refusal carrying
                // anything else came from the `check` gate itself (§4.3:
                // "MUST refuse a document with check errors ... run `check`
                // first").
                if exclusive {
                    println!(
                        "trace refused: {} — exclusive relations hold together (dsl 0.25.0 §1)",
                        file.display()
                    );
                } else if diags.iter().any(|d| !d.code.starts_with("E-TRACE-")) {
                    println!(
                        "trace refused: {} has check error(s) — run `lute check` first",
                        file.display()
                    );
                } else if diags.iter().all(|d| d.code == lute_trace::E_TRACE_ENTRY) {
                    println!("trace refused: {} — invalid `--entry`", file.display());
                } else if diags.iter().all(|d| d.code == lute_trace::E_TRACE_BEAT) {
                    println!("trace refused: {} — invalid `--beat`", file.display());
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
/// transcript already ends in `\n`; `--expand` →
/// [`TraceReport::render_human_expanded`]) — through [`write_stdout`], so a
/// closed pipe is an I/O exit `2` rather than a panic. `code` is the verdict
/// to return when the write succeeds.
fn print_trace_report(report: &TraceReport, json: bool, expand: bool, code: ExitCode) -> ExitCode {
    let text = if json {
        format!("{}\n", report.render_json())
    } else if expand {
        report.render_human_expanded()
    } else {
        report.render_human()
    };
    if write_stdout(&text).is_err() {
        return ExitCode::from(2);
    }
    code
}

/// One `file:line:col: severity [CODE] message` line per diagnostic. A
/// primary that collapsed same-root repeats (dsl 0.4.0 §8.2 C1/C5) appends a
/// trailing ` (+N more: 12:3, 47:9, …)` — line:column, comma-joined, document
/// order. Shared by [`print_human`] (the `check`/`compile` diagnostic list),
/// `run_trace`'s Refused rendering (dsl 0.4.0 §4.5: "the `E-TRACE-*` codes
/// render exactly as check diagnostics do"), and `compile --locales`'
/// `W-L10N-MISSING` stream — ONE line format, never a second convention.
///
/// Rendered to a `String` rather than printed so a caller whose STDOUT is
/// carrying an artifact can send the same bytes to stderr instead
/// ([`print_diagnostics`] is the stdout wrapper every prior caller uses).
fn render_diagnostics(file: &Path, diagnostics: &[Diagnostic], policy: &DenyPolicy) -> String {
    let path = file.display();
    let mut out = String::new();
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
        // §5 promotion: a denied diagnostic prints `error` with a `[denied]`
        // marker so it is distinguishable from a native error.
        let denied = policy.denied(d);
        let marker = if denied { " [denied]" } else { "" };
        let _ = writeln!(
            out,
            "{path}:{}:{}: {} [{}]{marker} {}{more}",
            d.span.line,
            d.span.column,
            if denied {
                "error"
            } else {
                severity_str(d.severity)
            },
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
            let _ = writeln!(
                out,
                "    {}:{}:{}: {} [{}] {}",
                cwd_relative(&r.file),
                r.diagnostic.span.line,
                r.diagnostic.span.column,
                severity_str(r.diagnostic.severity),
                r.diagnostic.code,
                r.diagnostic.message,
            );
        }
    }
    out
}

/// `file` as the author should read it on a `related` sub-line: a canonical
/// component path (the identity `lute-check` keeps in `related.file`) shown
/// relative to the current directory when it lies under it, the way the
/// primary path already reads (0.21.1 T3-7). Anything else prints unchanged.
fn cwd_relative(file: &str) -> String {
    let path = Path::new(file);
    std::env::current_dir()
        .and_then(std::fs::canonicalize)
        .ok()
        .filter(|_| path.is_absolute())
        .and_then(|cwd| path.strip_prefix(cwd).ok().map(|p| p.display().to_string()))
        .unwrap_or_else(|| file.to_string())
}

/// [`render_diagnostics`] to stdout — the sink every `check`/`trace` caller
/// has always used.
fn print_diagnostics(file: &Path, diagnostics: &[Diagnostic], policy: &DenyPolicy) {
    print!("{}", render_diagnostics(file, diagnostics, policy));
}

/// A summary line per diagnostic (via [`print_diagnostics`]), then a
/// pass/fail count summary. Mirrors the sorted order `check()` already
/// applied.
fn print_human(file: &Path, result: &lute_check::CheckResult, policy: &DenyPolicy) {
    let path = file.display();
    print_diagnostics(file, &result.diagnostics, policy);
    // §8.3: counting is by primaries — collapse (0.4.0 T14) already reduced
    // `result.diagnostics` to one entry per root cause, so a plain count needs
    // no change here. Five reads of one typo are ONE error. A `--deny`-promoted
    // warning counts as an error (spec §5), never also as a warning.
    let errors = result
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error || policy.denied(d))
        .count();
    let warnings = result
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning && !policy.denied(d))
        .count();
    let ok = result.ok && !policy.any_denied(&result.diagnostics);
    if ok {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// [`DENIABLE_CODES`] is the sole `--deny` universe: it MUST be sorted +
    /// deduped (so `contains` is meaningful and the list is auditable) and every
    /// entry MUST match the diagnostic-code shape (spec §5).
    #[test]
    fn deniable_codes_wellformed() {
        let re_ok = |c: &str| {
            let mut parts = c.splitn(2, '-');
            let head = parts.next().unwrap_or("");
            let rest = parts.next().unwrap_or("");
            matches!(head, "E" | "W")
                && !rest.is_empty()
                && rest
                    .chars()
                    .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '-')
        };
        for c in DENIABLE_CODES {
            assert!(re_ok(c), "malformed code in DENIABLE_CODES: {c}");
        }
        let mut sorted = DENIABLE_CODES.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(
            sorted.as_slice(),
            DENIABLE_CODES,
            "DENIABLE_CODES must be sorted and deduped"
        );
    }

    /// The drift guard below scans the five CHECK crates; `lute-cli/src` is
    /// deliberately not among them (`testcmd.rs` holds the literal
    /// `"E-TRACE-"` — a prefix, not a code — which the shape test would
    /// accept). So the two codes `lute test` emits from
    /// [`crate::testcmd`] are registered by hand, and a hand registration
    /// needs a hand guard: dropping either one is otherwise silent, since
    /// sortedness still holds without it.
    #[test]
    fn the_harness_own_codes_are_deniable() {
        for code in [
            "E-TEST-FILE",
            "E-TEST-KEY",
            "E-TEST-LORE",
            "E-TEST-NO-EXPECT",
        ] {
            assert!(
                DENIABLE_CODES.contains(&code),
                "{code} is emitted by crates/lute-cli/src/testcmd.rs and MUST be deniable; \
                 the drift guard does not scan this crate"
            );
        }
    }

    /// Drift guard (spec §5): every `"[EW]-…"` diagnostic-code literal in the
    /// crates whose diagnostics `check`/`check-project` surface MUST be in
    /// [`DENIABLE_CODES`], so a newly-added code cannot silently fall outside the
    /// deny universe. A code in the list but not emitted is harmless (protects
    /// nothing); a code emitted but NOT listed is the defect this catches. Paths
    /// are relative to this crate's dir (the house `../<crate>` idiom).
    #[test]
    fn every_check_emitted_code_is_deniable() {
        use std::collections::BTreeSet;
        let known: BTreeSet<&str> = DENIABLE_CODES.iter().copied().collect();
        let crates = [
            "../lute-check/src",
            "../lute-syntax/src",
            "../lute-cel/src",
            "../lute-manifest/src",
            "../lute-core-span/src",
            // 0.10.0 §8: `check-project` now emits `lute-trace`'s mock codes,
            // so they are inside the deny universe and inside this guard.
            "../lute-trace/src",
            // dsl 0.10.0 §9:962: `lute check` now runs the compile gate
            // (`normalize` + `expand`) that `trace`/`compile` run, so
            // `lute-compile`'s codes are ones `check` surfaces.
            "../lute-compile/src",
        ];
        let is_code = |c: &str| {
            let mut parts = c.splitn(2, '-');
            matches!(parts.next(), Some("E") | Some("W"))
                && parts.next().is_some_and(|rest| {
                    !rest.is_empty()
                        && rest
                            .chars()
                            .all(|ch| ch.is_ascii_uppercase() || ch.is_ascii_digit() || ch == '-')
                })
        };
        let mut missing: BTreeSet<String> = BTreeSet::new();
        for dir in crates {
            let mut stack = vec![PathBuf::from(dir)];
            while let Some(p) = stack.pop() {
                let Ok(rd) = std::fs::read_dir(&p) else {
                    continue;
                };
                for entry in rd.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        stack.push(path);
                    } else if path.extension().and_then(|x| x.to_str()) == Some("rs") {
                        let Ok(text) = std::fs::read_to_string(&path) else {
                            continue;
                        };
                        // Scan every double-quoted literal for a diagnostic code.
                        for chunk in text.split('"').skip(1).step_by(2) {
                            if is_code(chunk) && !known.contains(chunk) {
                                missing.insert(chunk.to_string());
                            }
                        }
                    }
                }
            }
        }
        assert!(
            missing.is_empty(),
            "diagnostic code(s) emitted by check crates but absent from DENIABLE_CODES \
             (add them so `--deny <code>` accepts them): {missing:?}"
        );
    }
}
