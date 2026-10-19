//! The `lute` command line: the clap argument/subcommand definitions and
//! the flag value parsers they use.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

use crate::codes;
use crate::lint;
use crate::play;

#[derive(Parser)]
#[command(
    name = "lute",
    version,
    about = "Checker, compiler, and toolchain for .lute branching game narratives",
    args_conflicts_with_subcommands = true,
    arg_required_else_help = true
)]
pub(crate) struct Cli {
    /// Explain a diagnostic code (`E-SET-SHAPE`, any letter case): what
    /// raises it, the spec sections behind it, and its reference page.
    #[arg(long, value_name = "CODE", value_parser = codes::parse_explain_code)]
    pub(crate) explain: Option<&'static codes::Code>,
    #[command(subcommand)]
    pub(crate) command: Option<Command>,
}

/// `--project <DIR>`: the project a command works on (0.48.0 §2.1). Declared
/// once so every command that takes it shows one spelling and one help text.
#[derive(clap::Args)]
pub(crate) struct ProjectOpt {
    /// Project directory (`lute.project.yaml` + `plugins/`). Omit for the
    /// nearest `lute.project.yaml` at or above the file operand's directory
    /// (file commands) or the current directory. A file command names the
    /// project it discovered in a stderr note.
    #[arg(long = "project", value_name = "DIR")]
    pub(crate) project: Option<PathBuf>,
}

/// `--providers <DIR>`.
#[derive(clap::Args)]
pub(crate) struct ProvidersOpt {
    /// Directory of pinned provider snapshots to resolve ids against.
    #[arg(long = "providers", value_name = "DIR")]
    pub(crate) providers: Option<PathBuf>,
}

/// `--permission-profile <NAME>`.
#[derive(clap::Args)]
pub(crate) struct PermissionProfileOpt {
    /// Trusted project profile whose permissions apply as an additional host
    /// ceiling. This never activates the profile's plugins or rewrites source.
    #[arg(long = "permission-profile", value_name = "NAME")]
    pub(crate) permission_profile: Option<String>,
}

/// `--deny <CODE>` (repeatable) and `--deny-warnings`: the diagnostic gate.
#[derive(clap::Args)]
pub(crate) struct GateOpts {
    /// Promote every diagnostic with EXACTLY this code to an error for the
    /// verdict and exit code (repeatable), like rustc/clippy `-D`. An
    /// unknown code is a usage error (exit 2). Errors are never demotable.
    #[arg(long = "deny", value_name = "CODE", value_parser = codes::parse_deny_code)]
    pub(crate) deny: Vec<String>,
    /// Promote EVERY warning to an error for the verdict and exit code.
    #[arg(long = "deny-warnings")]
    pub(crate) deny_warnings: bool,
}

/// `--json`: print the report as one JSON document (the 0.43 envelope).
#[derive(clap::Args)]
pub(crate) struct JsonOpt {
    /// Print the result as one JSON document on stdout instead of
    /// human-readable lines.
    #[arg(long = "json")]
    pub(crate) json: bool,
}

#[derive(Subcommand)]
pub(crate) enum Command {
    /// Statically validate `.lute` documents, or the whole project.
    ///
    /// With `<FILE>` operands, statically validate each document (or `.yaml`
    /// state schema) on its own, resolved in its project (`--project`, else
    /// the nearest `lute.project.yaml` above the file). That is the
    /// per-document analysis: relational guard decisions (dead guards over
    /// relations, `W-WIP`) and the project-wide checks need the whole-project
    /// mode. With no operand, check EVERY `.lute` document of the project
    /// (`--project <DIR>`, else the nearest `lute.project.yaml` above the
    /// current directory; recursively, in sorted order, each document
    /// resolving against its own nearest project) PLUS the project-wide
    /// checks: `<quest id>` uniqueness, quest references, connectivity,
    /// constraints, unread/unused declarations.
    Check {
        /// Documents to check; omit to check the whole project.
        #[arg(value_name = "FILE")]
        files: Vec<PathBuf>,
        #[command(flatten)]
        project: ProjectOpt,
        #[command(flatten)]
        providers: ProvidersOpt,
        #[command(flatten)]
        permission_profile: PermissionProfileOpt,
        /// Engine capability matrix YAML (defaults to the built-in reference matrix).
        #[arg(long, value_name = "FILE")]
        engine: Option<PathBuf>,
        /// Work in progress (whole-project mode only): a guard dead only
        /// because relations nothing produces yet (no seed, assert, rule, or
        /// reserved declaration) kill it is the warning `W-WIP`, whose
        /// message names the error (`E-ENTRY-UNREACHABLE`,
        /// `E-BEAT-UNREACHABLE`, `E-OBJECTIVE-UNSATISFIABLE`, `E-ARM-DEAD`)
        /// it is without the flag. A relation only a component `::assert`
        /// with an unbound `@param` writes counts as unproduced for specific
        /// arguments, and a required `<objective quest=…>` on a child dead
        /// only for that reason is a warning too. A relation with no such
        /// component producer whose producers can never match stays an error.
        #[arg(long)]
        wip: bool,
        #[command(flatten)]
        gate: GateOpts,
        #[command(flatten)]
        json: JsonOpt,
    },
    /// Report the reverse dependency closure for a project target.
    Impact {
        /// Project directory to inspect.
        dir: PathBuf,
        /// Target key (`fact:rel(args)`, `state:path`, or a declared node).
        target: String,
        /// Emit the normative structured JSON report.
        #[arg(long)]
        json: bool,
        /// Directory of pinned provider snapshots.
        #[arg(long, value_name = "DIR")]
        providers: Option<PathBuf>,
    },
    /// Report project author constraints.
    Constraints {
        /// Project directory to inspect.
        dir: PathBuf,
        /// Emit every constraint result as JSON.
        #[arg(long)]
        json: bool,
        /// Execute *.play.yaml and *.test.yaml scripts for witnesses.
        #[arg(long)]
        run: bool,
        /// Pinned providers.
        #[arg(long, value_name = "DIR")]
        providers: Option<PathBuf>,
    },
    /// Run configurable advisory content and metric lints (`L-*`) over documents or a tree.
    ///
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
        /// clap usage error (exit 2), never a silent no-op.
        #[arg(long = "deny", value_name = "CODE", value_parser = lint::parse_lint_deny_code)]
        deny: Vec<String>,
        /// Promote EVERY warning to an error for the verdict and exit
        /// code.
        #[arg(long = "deny-warnings")]
        deny_warnings: bool,
        /// Use this `lute.lint.yaml` instead of the per-root default
        /// (`<project root>/lute.lint.yaml`). Applied to every root in
        /// the target's grouping.
        #[arg(long = "config", value_name = "PATH")]
        config: Option<PathBuf>,
    },
    /// Compile a checked `.lute` document, or a whole project, to execution IR.
    ///
    /// Compile a checked `.lute` document to its JSON command-record execution
    /// IR (stdout, or `-o <FILE>`). With no document, compile every document
    /// of the project (`--project <DIR>`, else the nearest `lute.project.yaml`)
    /// into `-o <DIR>` plus a `project.index.json` unioning their
    /// vocabularies. With `--stream <FILE>`, incrementally compile body text
    /// from stdin against the checked scene prefix, flushing execution IR
    /// snapshots as newline-delimited JSON.
    Compile {
        /// Document to compile; omit to compile the whole project.
        #[arg(value_name = "FILE")]
        file: Option<PathBuf>,
        #[command(flatten)]
        project: ProjectOpt,
        #[command(flatten)]
        providers: ProvidersOpt,
        #[command(flatten)]
        permission_profile: PermissionProfileOpt,
        /// Write the primary output here instead of stdout: the execution IR
        /// file for a document, the output DIRECTORY (required) for a whole
        /// project, mirroring the project's own directory layout.
        #[arg(short = 'o', long = "output", value_name = "PATH")]
        output: Option<PathBuf>,
        /// Merge a locale bundle (`lute loc import`) into the execution IR:
        /// `texts` on every line and choice/hub option, `promptTexts` on a
        /// prompted branch/hub, keyed by `lineId`/`promptLineId`. `text` and
        /// `prompt` stay the source language. A record missing a declared
        /// locale is `W-L10N-MISSING`, a translation of a changed source is
        /// `W-L10N-STALE`.
        #[arg(long = "locale-bundle", value_name = "FILE")]
        locale_bundle: Option<PathBuf>,
        #[command(flatten)]
        gate: GateOpts,
        #[command(flatten)]
        json: JsonOpt,
        /// Stream NDJSON snapshots: read the body text from stdin, compile it
        /// incrementally against the scene prefix in `<FILE>`. Takes a
        /// document; not with `-o`, `--locale-bundle`, `--json`, `--deny` or
        /// `--deny-warnings`.
        #[arg(long)]
        stream: bool,
    },
    /// Lower a host CEL condition to an IR `exprNode` against a project.
    ///
    /// Lower a host condition (a CEL expression a game evaluates over a run
    /// state through the runtime's `evaluate` query) to an IR `exprNode`
    /// against a project. The condition is checked as the `when:` of a scene
    /// at the project root importing every schema file the project's
    /// documents import, with the project plugin set and the project index
    /// defs (parameterised ones included); expected type `bool`. On success
    /// stdout carries the `exprNode` as one line of JSON; diagnostics go to
    /// stderr. Exit `0` lowered, `1` a diagnostic error, `2` usage or I/O
    /// failure (including a directory without `lute.project.yaml`).
    Expr {
        /// The CEL condition, `@def` references included.
        #[arg(value_name = "CEL", allow_hyphen_values = true)]
        cel: String,
        /// Project directory (`lute.project.yaml`) the condition resolves
        /// against (default: current directory).
        #[arg(long, value_name = "DIR", default_value = ".")]
        project: PathBuf,
        /// Print one JSON object instead: `ok`, the index `celSlot` fields
        /// (`cel`, `authored` when `@def` expansion changed the input,
        /// `expr`) on success, and `diagnostics` as `lute check --json`
        /// reports them.
        #[arg(long)]
        json: bool,
    },
    /// Back-fill missing stable identity attributes (line `code`, `::use` `instance`).
    ///
    /// Back-fill missing stable identity attributes in the corpus: per-speaker
    /// line `code` values and `::use` component `instance` keys. Existing
    /// authored codes and instances are preserved byte-for-byte; only changed
    /// files are written. A second run is a no-op.
    Tag {
        /// The `.lute` file to tag, or a directory to tag recursively.
        path: PathBuf,
        /// FORCE-renumber every line's `code` in clean document order
        /// (0010/0020/… per speaker per scope), rewriting existing codes.
        /// A drafting tool: refused when frontmatter declares `codesLocked:`
        /// (published codes key `lineId`/`voiceKey` — renumbering breaks the
        /// localization/voice join). `--force` never changes an explicit
        /// component `instance`.
        #[arg(long)]
        force: bool,
    },
    /// Apply mechanical, meaning-preserving source migrations in place.
    ///
    /// Apply mechanical, meaning-preserving migrations in place: legacy
    /// line/choice syntax; literal `<when>` comparisons; 0.31 CEL
    /// fact-query/presence calls (`holds`/`count`/`countDistinct`/`validAt`/
    /// `isSet`) to their standard-CEL 0.32 forms; renamed domains in project,
    /// schema and plugin YAML (0.41.0); existing `luteVersion` stamps older
    /// than the toolchain, raised to it (0.41.1); a `player.bindings.yaml`
    /// `bindingsVersion` older than the toolchain line, raised to it
    /// (0.47.0); and, when the project's
    /// `volume` domain is exactly the old `[silent, down, normal, up, full]`,
    /// `down`/`up` to `quiet`/`loud` in commands, YAML references and
    /// `volumes` keys (0.42.0). Byte-exact and
    /// comment-preserving; writes back only when something changed. A
    /// directory migrates `.lute` and project/schema YAML files recursively,
    /// sorted; several paths migrate in the order given. Exit `0` on success,
    /// `2` on an I/O failure.
    Fix {
        /// The `.lute`/YAML files to migrate, or directories to migrate recursively.
        #[arg(required = true)]
        paths: Vec<PathBuf>,
    },
    /// Emit the project-resolved authoring surface for a `.lute` file.
    ///
    /// Emit the project-resolved AUTHORING SURFACE for a `.lute` file — the
    /// directives/attrs/enums/asset-kinds/providers/state-schema/components +
    /// capabilitySnapshot an AI needs to WRITE valid Lute against THIS file's
    /// project. A capability query, NOT validation: reuses the SAME resolution
    /// (`build_input`/`fold_env`) check/compile use, and emits regardless of
    /// document diagnostics. Exit `0` on success, `2` on an I/O failure.
    Context {
        /// Path to the `.lute` file whose project surface to resolve.
        file: PathBuf,
        /// Target a project graph node using its canonical `kind:key`.
        #[arg(long, conflicts_with = "at", value_name = "NODEKEY")]
        target: Option<String>,
        /// Resolve the source position `FILE:LINE:COLUMN`.
        #[arg(long, conflicts_with = "target", value_name = "FILE:LINE:COLUMN")]
        at: Option<String>,
        /// Bound each context collection independently.
        #[arg(long, default_value_t = 100)]
        max_items: usize,
        /// Execute the selected play/test script set to mark witnessed coverage.
        #[arg(long, value_name = "FILE")]
        run: Option<PathBuf>,
        /// Emit the machine-readable JSON surface instead of a human outline.
        #[arg(long)]
        json: bool,
        /// Directory of pinned provider snapshots to resolve ids against.
        #[arg(long, value_name = "DIR")]
        providers: Option<PathBuf>,
        /// Project directory (`lute.project.yaml` + `plugins/`) whose installed
        /// plugins resolve the document's activated capability snapshot. Omit
        /// for the nearest `lute.project.yaml` above the file, or a core-only
        /// (`lute.core`) surface when there is none.
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
        /// Trusted project profile whose permissions restrict the reported
        /// authoring surface without activating that profile's plugins.
        #[arg(long = "permission-profile", value_name = "NAME")]
        permission_profile: Option<String>,
    },
    /// Canonicalize one or more `.lute` or project declaration files.
    Fmt {
        /// File or directory to format (directory traversal is recursive).
        #[arg(required = true, value_name = "PATH")]
        paths: Vec<PathBuf>,
        /// Check canonical form without writing; exits 1 when changes are needed.
        #[arg(long)]
        check: bool,
    },
    /// Compare two project directories (or `git:<rev>` sides) semantically.
    Diff {
        /// The before project directory or `git:<rev>` side.
        before: PathBuf,
        /// The after project directory or `git:<rev>` side.
        after: PathBuf,
        /// Emit the versioned semantic diff as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Apply an atomic, checked source patch.
    Patch {
        /// Project directory to patch.
        dir: PathBuf,
        /// JSON patch request.
        patch: PathBuf,
        /// Run every stage without writing files.
        #[arg(long)]
        dry_run: bool,
        /// Emit only the versioned machine-readable result.
        #[arg(long)]
        json: bool,
    },
    /// Preview a document's behaviour against author-supplied mocks.
    ///
    /// Preview a `.lute` document's behavior against author-supplied mocks —
    /// an authoring evaluator, not the runtime. Resolves the document
    /// identically to `check`, refuses (exit 1) a
    /// document with check errors OR invalid mocks (`E-TRACE-*`, rendered
    /// exactly like check diagnostics — run `check` first), then walks it
    /// once, deterministically, reporting every decision and why. Exit `0`
    /// complete, `1` refused, `2` I/O, `3` incomplete (an `unknown` guard
    /// halted the walk).
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
        /// transitions, never user-fired.
        #[arg(long = "event", value_name = "NAME")]
        event: Vec<String>,
        /// Simulate accepting a `start`-less (accept-driven) quest, by id
        /// (repeatable). An unknown quest id, or one that carries a
        /// `start` predicate (declarative — needs no accept), is
        /// `E-TRACE-ACCEPT`.
        #[arg(long = "accept", value_name = "QUESTID")]
        accept: Vec<String>,
        /// Raise an occasion after the quest walk settles, in CLI order
        /// (repeatable): each raise judges the `<objective on="<occasion>">`
        /// objectives of every active quest. A raise for a target is
        /// `<occasion>@<target>`.
        #[arg(long = "occasion", value_name = "OCCASION[@TARGET]")]
        occasion: Vec<String>,
        /// Renamed `--occasion`; refused with the new spelling.
        #[arg(long = "on", value_name = "OCCASION", hide = true)]
        retired_on: Vec<String>,
        /// Refused: trace names a raise's target in `--occasion
        /// <occasion>@<target>`.
        #[arg(long, hide = true, value_name = "TARGET")]
        target: Vec<String>,
        /// A YAML document carrying the same surfaces (`state:`/`facts:`/
        /// `choose:`/`events:`/`accepts:`/`visited:`/`occasions:`); CLI flags
        /// compose with it, the flag winning on a conflict.
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
        /// snapshot. Omit for the nearest `lute.project.yaml`
        /// above the file (as `lute check`), or a core-only (`lute.core`)
        /// trace when there is none; only an explicit `--project` gates on
        /// the reconciled project verdict.
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
        /// Present ONE `<entry>` of a `kind: lore` document by id: its
        /// lines, the `<match>` arm taken, and the
        /// `::set`/`::assert`/`::retract` a first read applies — or skips,
        /// when the mock seeds `entry.<id>.read: true`. REQUIRED for a lore
        /// document without `--beat` (it has no sequence to walk);
        /// `E-TRACE-ENTRY` (exit 1) on a non-lore document or an unknown id.
        #[arg(long, value_name = "ID")]
        entry: Option<String>,
        /// Present ONE bundle `<beat>` of a `kind: lore` document, by its
        /// local id or its canonical `<document id>.<beat id>`: its body
        /// walked like a scene shot body (`--choose` picks),
        /// every effect applied, its `when` shown, not enforced.
        /// `E-TRACE-BEAT` (exit 1) on a non-lore document or an unknown id.
        #[arg(long, value_name = "ID", conflicts_with = "entry")]
        beat: Option<String>,
        /// Do not apply the project's seed facts and Datalog rules: an
        /// unmocked derived atom is unknown and each derived read is noted.
        /// Overrides the mock's `derive:`.
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
    /// Scaffold a new Lute project directory.
    ///
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
        /// with `id:`, a quest, lore, a play script and scenario tests).
        #[arg(long, value_name = "NAME")]
        template: Option<String>,
    },
    /// Scaffold one new document (scene, quest, lore, schema) into an existing project.
    ///
    /// Scaffold one new document into an existing project: `lute new scene
    /// <name> [--occasion <occasion> [--target <target>]]` / `lute new quest
    /// <name> [--start]` / `lute new lore <name>` / `lute new schema <name>`.
    /// The id comes from the name as typed (`isles.harborNight` → `id:
    /// isles.harborNight`; words of a phrase join in camel case, `The
    /// Epilogue` → `theEpilogue`) and the file is named after the id; a
    /// document omits what the manifest's `defaults:` supplies. Outside a
    /// project `lute new` says so (and `--occasion` is refused).
    New {
        /// Document kind: `scene`, `quest`, `lore`, or `schema`.
        kind: String,
        /// The new document's name (`/` nests it in a subfolder: `lute new
        /// scene talk/tomas-evening` → `scenes/talk/tomasEvening.lute`).
        name: String,
        /// The PROJECT directory (default: current directory) — not the
        /// destination folder. A directory inside a project that is not its
        /// root is refused with the `<sub>/<name>` spelling to use instead.
        #[arg(long, value_name = "PROJECT")]
        dir: Option<PathBuf>,
        /// Make the scene a beat answering this occasion. An occasion raised
        /// for a target needs `--target`.
        #[arg(long, value_name = "OCCASION")]
        occasion: Option<String>,
        /// Renamed `--occasion`; refused with the new spelling.
        #[arg(long = "on", value_name = "OCCASION", hide = true)]
        retired_on: Option<String>,
        /// The target the beat answers for (a targeted occasion's
        /// `<prefix>.<member>`).
        #[arg(long, value_name = "TARGET")]
        target: Option<String>,
        /// `new quest` only: scaffold an auto-starting quest (`start="true"`)
        /// instead of the default accept-driven stub.
        #[arg(long)]
        start: bool,
    },
    /// Diagnose the local toolchain and project setup.
    ///
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
        /// a harness can refuse to start on a broken setup.
        #[arg(long)]
        strict: bool,
    },
    /// Execute compiled execution IR headlessly against a mock playthrough.
    ///
    /// Execute a COMPILED execution IR (`lute compile` output) headlessly against
    /// a mock playthrough — the reference consumer of the runtime contract
    /// (docs/runtime/): command dispatch, CEL guards, facts + Datalog
    /// fixpoint, hubs, and quest lifecycle. Distinct from `lute trace`, which
    /// previews SOURCE; `run` consumes the execution IR an engine would.
    Run {
        /// Path to the compiled execution IR JSON.
        #[arg(value_name = "EXECUTION-IR")]
        artifact: PathBuf,
        /// Engine capability matrix YAML (defaults to the built-in reference matrix).
        #[arg(long, value_name = "FILE")]
        engine: Option<PathBuf>,
        /// A YAML mock playthrough (same surfaces as `lute trace --mock`).
        #[arg(long, value_name = "FILE")]
        mock: Option<PathBuf>,
        /// Raise an occasion after a quest execution IR's walk settles, after
        /// the mock's own `occasions:`, in CLI order (repeatable): each raise
        /// judges the `on="<occasion>"` objectives of every active quest.
        #[arg(long = "occasion", value_name = "OCCASION")]
        occasion: Vec<String>,
        /// Emit the machine-readable transcript as JSON.
        #[arg(long)]
        json: bool,
        /// Present ONE `entry` record of a lore execution IR by id
        /// (docs/runtime/lore-entries.md). A lore execution IR needs exactly
        /// one of `--entry`/`--beat` (exit 2 otherwise); refused (exit 2) on
        /// any other execution IR kind.
        #[arg(long, value_name = "ID")]
        entry: Option<String>,
        /// Present ONE bundle `beat` record of a lore execution IR by canonical
        /// id `<document id>.<beat id>` (or the bare beat id
        /// when unambiguous): its body segment runs like a scene, every
        /// effect applied. Refused (exit 2) on any other execution IR kind.
        #[arg(long, value_name = "ID", conflicts_with = "entry")]
        beat: Option<String>,
        /// Append one JSONL record for every CEL condition evaluation.
        #[arg(long, value_name = "FILE")]
        dump_conditions: Option<PathBuf>,
    },
    /// Play a story through a whole project as a sequence of raised occasions.
    ///
    /// Compiles the WHOLE project in memory
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
        /// Engine capability matrix YAML (defaults to the built-in reference matrix).
        #[arg(long, value_name = "FILE")]
        engine: Option<PathBuf>,
        /// Emit the resumable runtime event stream as compact JSON Lines:
        /// a seed/output pair followed by input/output pairs, including
        /// choice and bridge answers. Exclusive with `--json`, `--ir`,
        /// `--explain` and `--dump-conditions`.
        #[arg(long)]
        events: bool,
        /// Emit the machine-readable transcript as JSON.
        #[arg(long)]
        json: bool,
        /// Do not apply the project's Datalog rules: an unmocked derived
        /// atom is unknown and halts the walk incomplete.
        #[arg(long)]
        no_derive: bool,
        /// After the play, print the derivation tree of a ground atom — the
        /// rule used and each premise's own support, negated premises shown
        /// absent — or, when it does not hold, the failing premises of every
        /// rule that could conclude it (repeatable).
        #[arg(long, value_name = "ATOM")]
        explain: Vec<String>,
        /// Print staging as the lowered IR records (`::background`,
        /// `::sprite`, injected preloads and pose resets) instead of the
        /// authored directives (`::bg`, `::actor`, …).
        #[arg(long)]
        ir: bool,
        /// Leave out the candidates that were not eligible at each raise:
        /// the transcript keeps the winners, the lines, the quests and the
        /// expectations. Without it, five or more `when:
        /// false` candidates at one raise fold into one count line.
        #[arg(long)]
        quiet: bool,
        /// Append one JSONL record for every CEL condition evaluation.
        #[arg(long, value_name = "FILE")]
        dump_conditions: Option<PathBuf>,
    },
    /// Run the project's scenario tests (`*.test.yaml`) and expectation plays (`*.play.yaml`).
    ///
    /// Run the project's scenario tests: every `*.test.yaml` under `dir`
    /// traces its scene (or, with `entry:`/`entries:`, presents its lore
    /// entries) against the declared mocks and asserts the declared
    /// expectations (transcript, offered options, state, quest status); every
    /// `*.play.yaml` under `dir` that carries an `expect:` is played and
    /// judged as `lute play` does. Resolves each traced document
    /// identically to `lute trace`: against
    /// `--project`, else the nearest `lute.project.yaml` above the document
    /// (as `lute check`), so the document's `profile:`/`plugins:` frontmatter
    /// and the manifest's `defaults: uses:` hoist are both applied before
    /// tracing. A play runs `--project`, else the nearest `lute.project.yaml`
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
        /// capability snapshot. Omit for each document's
        /// nearest `lute.project.yaml`, or a core-only (`lute.core`) test.
        #[arg(long, value_name = "DIR")]
        project: Option<PathBuf>,
        /// Also report branch/arm coverage across the tested documents, and
        /// the project's documents no test traced and no play presented.
        #[arg(long)]
        coverage: bool,
        /// Do not apply the project's seed facts and Datalog rules: an
        /// unmocked derived atom is unknown. Overrides every test's and play
        /// script's own `derive:`.
        #[arg(long)]
        no_derive: bool,
    },
    /// Localization & production reporting over a project's content lines.
    #[command(subcommand)]
    Loc(LocCommand),
    /// Map the project's lore entries and which facts lore, scenes and quests reveal.
    ///
    /// The project's world-narrative map: lore entries
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
    /// List who uses which engine content id: directive attribute values or reward targets.
    ///
    /// Who uses which engine content id: every value of a
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
    /// Report project connectivity: the node graph, reachability and envelope tables.
    ///
    /// Project-wide, read-only reporting surface over everything the
    /// connectivity layer computes: the assembled node/edge
    /// graph, per-node reachability plus its declared `after` structure, and
    /// the Guaranteed/Possible envelope tables — including the
    /// `Possible \ Guaranteed` warning-grade reads `check-project` computes
    /// and drops by default. Evaluates no CEL, runs no Datalog,
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
        #[arg(long, value_name = "DIR", global = true)]
        providers: Option<PathBuf>,
        /// Output format: `text` (default), `json`, or `dot` (Graphviz).
        /// Accepted before or after the sub-view (`reach … --format json`).
        #[arg(long, value_name = "FORMAT", global = true)]
        format: Option<String>,
        /// Graph view: also draw fact-producer edges —
        /// `scene(A) -> scene(B) [hasItem(x)]` when B's gate (`when:` /
        /// `start=`) reads `holds(F)` and A asserts `F` (or a fact the rules
        /// deriving `F` need) — and layer over them where they close no
        /// cycle, so progress gated by facts shows in the layers.
        #[arg(long)]
        facts: bool,
        /// `reach`/`envelope` sub-view; omitted -> prints the assembled
        /// topological graph.
        #[command(subcommand)]
        command: Option<ScenarioCommand>,
    },
    /// List every beat per occasion and target in selection order, with static verdicts.
    ///
    /// Every beat of the project, per occasion and target, in selection
    /// order (priority descending, then project order) with its priority,
    /// `once`, `after:`, `when` and the static verdicts `check-project`
    /// reaches about it — unreachable, shadowed, tied, once-per-run over
    /// user state. Read-only; documents need not check
    /// clean. Exit `0` on success, `2` on an I/O failure or an unknown
    /// `--occasion`.
    Beats {
        /// Directory to walk recursively for `*.lute` files.
        dir: PathBuf,
        /// Only this occasion's ladders (repeatable).
        #[arg(long, value_name = "OCCASION")]
        occasion: Vec<String>,
        /// Renamed `--occasion`; refused with the new spelling.
        #[arg(long = "on", value_name = "OCCASION", hide = true)]
        retired_on: Vec<String>,
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
    /// Evaluate beat eligibility over a grid of state values.
    ///
    /// Evaluate beat eligibility over a grid of state values: for every
    /// cell of the `--axis` product, starting from the
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
        /// axis is a declared state path; `run.aff.*=6,7` sets every member
        /// of a `per:` family, `run.aff[run.route]=6,7` only the member the
        /// `--axis run.route` value names at each cell (the others keep
        /// their seed/default); `quest.<id>.state=…` seeds the
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
        /// Renamed `--occasion`; refused with the new spelling.
        #[arg(long = "on", value_name = "OCCASION", hide = true)]
        retired_on: Vec<String>,
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
        /// `entriesRead:`) with its `steps:` replayed as
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
    /// Print the toolchain, language and IR version axes.
    ///
    /// Print the three independent version axes (docs/versioning.md): the
    /// TOOLCHAIN version (this CLI + workspace crates), the LANGUAGE version
    /// (the grammar/semantics the checker enforces), and the IR schema
    /// version (stamped as `irVersion` in every compiled execution IR). Distinct
    /// from clap's built-in `--version`, which prints only the toolchain
    /// version. Human-readable lines by default; `--json` prints a single
    /// object `{"toolchain":…,"language":…,"ir":…}`.
    Version {
        /// Emit the three versions as one JSON object instead of human lines.
        #[arg(long)]
        json: bool,
    },
    /// Describe commands as JSON: arguments, options and machine-readable outputs.
    ///
    /// With no path, lists the top-level commands; with a path (`lute schema
    /// scenario reach`), describes that command: its positional arguments
    /// and options (name, whether required or repeatable, value name,
    /// possible values, default), its subcommands, and each machine-readable
    /// output (the flag selecting it, `report`/`artifact`/`stream`, and a
    /// report's `schemaVersion`). Always JSON; reads no files. Exit `0`, or
    /// `2` for an unknown command path.
    Schema {
        /// A command path, e.g. `scenario reach`; omitted to list the
        /// top-level commands.
        path: Vec<String>,
    },
}

impl Command {
    /// The command id (dsl 0.43.0 §3.1): the subcommand path as typed.
    pub(crate) fn id(&self) -> &'static str {
        match self {
            Command::Check { .. } => "check",
            Command::Impact { .. } => "impact",
            Command::Constraints { .. } => "constraints",
            Command::Lint { .. } => "lint",
            Command::Compile { .. } => "compile",
            Command::Expr { .. } => "expr",
            Command::Tag { .. } => "tag",
            Command::Fix { .. } => "fix",
            Command::Context { .. } => "context",
            Command::Fmt { .. } => "fmt",
            Command::Diff { .. } => "diff",
            Command::Patch { .. } => "patch",
            Command::Trace { .. } => "trace",
            Command::Catalog(CatalogCommand::Refresh { .. }) => "catalog refresh",
            Command::Init { .. } => "init",
            Command::New { .. } => "new",
            Command::Doctor { .. } => "doctor",
            Command::Run { .. } => "run",
            Command::Play { .. } => "play",
            Command::Test { .. } => "test",
            Command::Loc(LocCommand::Export { .. }) => "loc export",
            Command::Loc(LocCommand::Import { .. }) => "loc import",
            Command::Loc(LocCommand::Report { .. }) => "loc report",
            Command::Lore { .. } => "lore",
            Command::Refs { .. } => "refs",
            Command::Scenario { command, .. } => match command {
                None => "scenario",
                Some(ScenarioCommand::Reach { .. }) => "scenario reach",
                Some(ScenarioCommand::Envelope { .. }) => "scenario envelope",
                Some(ScenarioCommand::Knowledge { .. }) => "scenario knowledge",
            },
            Command::Beats { .. } => "beats",
            Command::Calendar { .. } => "calendar",
            Command::Version { .. } => "version",
            Command::Schema { .. } => "schema",
        }
    }
}

/// See [`Command::Scenario`].
#[derive(Subcommand)]
pub(crate) enum ScenarioCommand {
    /// Report a node's reachability verdict and its `after` prerequisite structure.
    ///
    /// Report a node's reachability verdict (Reachable/Unreachable/Unknown)
    /// plus its declared `after` prerequisite structure — or, with
    /// `--endings`, one row per ending.
    Reach {
        /// A scene's canonical key (e.g. `marina.s01ep02`), a bundle beat's
        /// `<document id>.<beat id>`, or `quest:<id>` for a quest (as
        /// `envelope quest:<id>`); `scene:`/`beat:` prefixes disambiguate.
        #[arg(required_unless_present = "endings", conflicts_with = "endings")]
        node_id: Option<String>,
        /// Every ending instead of one node: with `=<occasion>`, every beat
        /// answering that occasion; bare, every beat whose content can run
        /// `::end` (its own body or a `::use`d component). Each row has the
        /// `after:` verdict, the `when` verdict `check-project` reaches
        /// (never holds / never wins), and what a satisfiable `when` reads
        /// with who produces it.
        #[arg(long, value_name = "OCCASION", num_args = 0..=1, default_missing_value = "", require_equals = true)]
        endings: Option<String>,
    },
    /// Report the Guaranteed/Possible envelope tables for a node.
    ///
    /// Report the Guaranteed/Possible envelope tables for a node — full
    /// tables for a scene or a quest with `follows=`; defaults-only `D`
    /// plus an enrichment note for a bare quest. Also prints the
    /// `Possible \ Guaranteed` warning-grade reads for the node —
    /// suppressed by default in `check-project`, surfaced here.
    Envelope {
        /// A scene's canonical key, a bundle beat's `<document id>.<beat
        /// id>`, or `quest:<id>` for a quest.
        node_id: String,
    },
    /// Trace every fact-guarded condition, grouped by document.
    ///
    /// Trace every fact-guarded condition — beat, entry, quest, objective,
    /// line, choice, arm, handler — grouped by document, to the relations it
    /// reads, and each relation to its producers through the rules:
    /// asserting documents, seed facts, the engine (reserved), or nothing.
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
pub(crate) enum LocCommand {
    /// Extract every translatable content line across a project to a locale file.
    ///
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
    /// Canonicalize translated `loc export` files into one locale bundle.
    ///
    /// Canonicalize translated `loc export` files into ONE locale bundle —
    /// the reverse direction, consumed by
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
pub(crate) enum CatalogCommand {
    /// Re-stamp and rewrite the pinned provider snapshots in a directory.
    Refresh {
        /// Directory holding the flat per-snapshot YAML files.
        dir: PathBuf,
        /// Project directory (`lute.project.yaml` + `plugins/`) whose resolved
        /// multi-plugin `capabilitySnapshot` stamps each snapshot instead of the
        /// core-only version. Omit for the core baseline.
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
