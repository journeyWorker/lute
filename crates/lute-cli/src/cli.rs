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
/// `global`, so `scenario` (the one command with sub-views) also accepts it
/// after the sub-view; a no-op on a command without subcommands.
#[derive(clap::Args)]
pub(crate) struct ProjectOpt {
    /// Project directory (`lute.project.yaml` + `plugins/`). Omit for the
    /// nearest `lute.project.yaml` at or above the file operand's directory
    /// (file commands) or the current directory. A file command names the
    /// project it discovered in a stderr note.
    #[arg(long = "project", value_name = "DIR", global = true)]
    pub(crate) project: Option<PathBuf>,
}

/// `--providers <DIR>`.
#[derive(clap::Args)]
pub(crate) struct ProvidersOpt {
    /// Directory of pinned provider snapshots: the ids resolve against them
    /// (`catalog refresh` re-stamps the ones in it; default the project's
    /// `catalogDir`).
    #[arg(long = "providers", value_name = "DIR", global = true)]
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
/// `global` for `scenario`'s sub-views, like [`ProjectOpt`].
#[derive(clap::Args)]
pub(crate) struct JsonOpt {
    /// Print the result as one JSON document on stdout instead of
    /// human-readable lines.
    #[arg(long = "json", global = true)]
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
        /// Target key (`fact:rel(args)`, `state:path`, or a declared node).
        #[arg(long, value_name = "NODE")]
        node: String,
        #[command(flatten)]
        project: ProjectOpt,
        #[command(flatten)]
        providers: ProvidersOpt,
        #[command(flatten)]
        json: JsonOpt,
    },
    /// Report project author constraints.
    Constraints {
        #[command(flatten)]
        project: ProjectOpt,
        #[command(flatten)]
        providers: ProvidersOpt,
        /// Execute only this play/test script for witnesses (repeatable); a
        /// directory selects every `*.play.yaml` and `*.test.yaml` under it.
        #[arg(long = "script", value_name = "FILE", conflicts_with = "all_scripts")]
        script: Vec<PathBuf>,
        /// Execute every `*.play.yaml` and `*.test.yaml` script of the
        /// project for witnesses.
        #[arg(long = "all-scripts")]
        all_scripts: bool,
        #[command(flatten)]
        json: JsonOpt,
    },
    /// Run configurable advisory content and metric lints (`L-*`) over documents or a tree.
    ///
    /// Run configurable content/metric advisory lints over a `.lute`
    /// document or a directory tree (https://lute-lang.vercel.app/tooling/linting/).
    /// Distinct from `lute check`: this surface publishes advisory `L-*`
    /// findings governed by `<project root>/lute.lint.yaml` (rule levels,
    /// thresholds, ignore globs, project-local `custom:` rules) and never
    /// touches artifact identity. Documents are grouped by their nearest
    /// ancestor `lute.project.yaml` exactly as the whole-project `lute check`
    /// does; with `--project <DIR>` that grouping is bounded below by `<DIR>`
    /// and `<PATH>` defaults to it. Exit `0` clean
    /// or only sub-error findings, `1` any Error-severity finding
    /// (native or `--deny`-promoted, including `E-LINT-CONFIG`/
    /// `E-LINT-EXPR`), `2` I/O, malformed YAML, or usage.
    Lint {
        /// File or directory to lint (default: the `--project` directory,
        /// else the current directory).
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,
        #[command(flatten)]
        project: ProjectOpt,
        #[command(flatten)]
        providers: ProvidersOpt,
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
        #[arg(long = "config", value_name = "FILE")]
        config: Option<PathBuf>,
        #[command(flatten)]
        json: JsonOpt,
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
    /// against a project (`--project`, else the nearest `lute.project.yaml`
    /// at or above the current directory). The condition is checked as the
    /// `when:` of a scene at the project root importing every schema file
    /// the project's documents import, with the project plugin set and the
    /// project index defs (parameterised ones included); expected type
    /// `bool`. On success stdout carries the `exprNode` as one line of JSON;
    /// diagnostics go to stderr. Exit `0` lowered, `1` a diagnostic error
    /// (or a `--deny`-promoted one), `2` usage or I/O failure (including no
    /// `lute.project.yaml`).
    Expr {
        /// The CEL condition, `@def` references included.
        #[arg(value_name = "EXPR", allow_hyphen_values = true)]
        cel: String,
        #[command(flatten)]
        project: ProjectOpt,
        #[command(flatten)]
        permission_profile: PermissionProfileOpt,
        #[command(flatten)]
        gate: GateOpts,
        #[command(flatten)]
        json: JsonOpt,
    },
    /// Back-fill missing stable identity attributes (line `code`, `::use` `instance`).
    ///
    /// Back-fill missing stable identity attributes in the corpus: per-speaker
    /// line `code` values and `::use` component `instance` keys. Existing
    /// authored codes and instances are preserved byte-for-byte; only changed
    /// files are written. A second run is a no-op. `--renumber` is the
    /// dangerous mode (it rewrites published codes): it previews the
    /// renumbering by default and writes only with `--commit`.
    Tag {
        /// `.lute` files, or directories to tag recursively (default `.`).
        #[arg(value_name = "PATH")]
        paths: Vec<PathBuf>,
        /// Renumber every line's `code` in clean document order
        /// (0010/0020/… per speaker per scope), rewriting existing codes.
        /// A drafting tool: prints one `<path>:<line>: <old> -> <new>` line
        /// per change and writes nothing unless `--commit`. Refused when
        /// frontmatter declares `codesLocked:` (published codes key
        /// `lineId`/`voiceKey` — renumbering breaks the localization/voice
        /// join). Never changes an explicit component `instance`.
        #[arg(long)]
        renumber: bool,
        /// Apply the renumbering `--renumber` previews.
        #[arg(long, requires = "renumber", conflicts_with = "check")]
        commit: bool,
        /// Write nothing; exit `1` when the selected mode would change a file.
        #[arg(long)]
        check: bool,
        #[command(flatten)]
        json: JsonOpt,
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
        /// The `.lute`/YAML files to migrate, or directories to migrate
        /// recursively (default `.`).
        #[arg(value_name = "PATH")]
        paths: Vec<PathBuf>,
        /// Write nothing; exit `1` when any fix applies.
        #[arg(long)]
        check: bool,
        #[command(flatten)]
        json: JsonOpt,
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
        #[command(flatten)]
        project: ProjectOpt,
        #[command(flatten)]
        providers: ProvidersOpt,
        #[command(flatten)]
        permission_profile: PermissionProfileOpt,
        /// Target a project graph node using its canonical `kind:key`.
        #[arg(long, conflicts_with = "at", value_name = "NODE")]
        node: Option<String>,
        /// Resolve the source position `FILE:LINE:COLUMN`.
        #[arg(long, conflicts_with = "node", value_name = "FILE:LINE:COLUMN")]
        at: Option<String>,
        /// Bound each context collection independently.
        #[arg(long = "max-items", value_name = "N", default_value_t = 100)]
        max_items: usize,
        /// Execute this play/test script (repeatable) to mark witnessed
        /// coverage with `--node`; a directory selects every script under it.
        #[arg(long = "script", value_name = "FILE", conflicts_with = "all_scripts")]
        script: Vec<PathBuf>,
        /// Execute every play/test script of the project to mark witnessed
        /// coverage with `--node`.
        #[arg(long = "all-scripts")]
        all_scripts: bool,
        #[command(flatten)]
        json: JsonOpt,
    },
    /// Canonicalize one or more `.lute` or project declaration files.
    Fmt {
        /// Files or directories to format; directory traversal is recursive
        /// (default `.`).
        #[arg(value_name = "PATH")]
        paths: Vec<PathBuf>,
        /// Check canonical form without writing; exits 1 when changes are needed.
        #[arg(long)]
        check: bool,
        #[command(flatten)]
        json: JsonOpt,
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
        /// JSON patch request.
        #[arg(value_name = "FILE")]
        file: PathBuf,
        #[command(flatten)]
        project: ProjectOpt,
        #[command(flatten)]
        providers: ProvidersOpt,
        #[command(flatten)]
        permission_profile: PermissionProfileOpt,
        /// Run every stage without writing files.
        #[arg(long)]
        dry_run: bool,
        #[command(flatten)]
        json: JsonOpt,
    },
    /// Preview a document's behaviour against author-supplied mocks.
    ///
    /// Preview a `.lute` document's behavior against author-supplied mocks —
    /// an authoring evaluator, not the runtime. Resolves the document
    /// identically to `check` (in its `--project`, else the nearest
    /// `lute.project.yaml` above it, gating on the reconciled project
    /// verdict), refuses (exit 1) a
    /// document with check errors (or a `--deny`-promoted diagnostic) OR
    /// invalid mocks (`E-TRACE-*`, rendered
    /// exactly like check diagnostics — run `check` first), then walks it
    /// once, deterministically, reporting every decision and why. Exit `0`
    /// complete, `1` refused, `2` I/O, `3` incomplete (an `unknown` guard
    /// halted the walk).
    Trace {
        /// Path to the `.lute` file to trace.
        file: PathBuf,
        #[command(flatten)]
        project: ProjectOpt,
        #[command(flatten)]
        providers: ProvidersOpt,
        #[command(flatten)]
        permission_profile: PermissionProfileOpt,
        #[command(flatten)]
        gate: GateOpts,
        /// A scalar state seed: a DECLARED state path and a literal,
        /// `<path>=<literal>` (repeatable).
        #[arg(long = "state", value_name = "PATH=LITERAL", value_parser = parse_state_flag)]
        state: Vec<(String, String)>,
        /// A ground fact, valid-now, over the declared vocabulary — e.g.
        /// `"inParty(shadowheart)"` (repeatable).
        #[arg(long = "fact", value_name = "ATOM")]
        fact: Vec<String>,
        /// A menu selection at a `<branch>`/`<hub>` id, in order:
        /// `<branchOrHubId>=<choiceId>[,<choiceId>…]` (repeatable; a hub may
        /// force a whole ordered visit sequence via one flag's comma list).
        #[arg(long = "choice", value_name = "BRANCH=CHOICE", value_parser = parse_choice_flag)]
        choice: Vec<(String, Vec<String>)>,
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
        #[arg(long = "accepted-quest", value_name = "QUEST")]
        accepted_quest: Vec<String>,
        /// Raise an occasion after the quest walk settles, in CLI order
        /// (repeatable): each raise judges the `<objective on="<occasion>">`
        /// objectives of every active quest. A raise for a target is
        /// `<occasion>@<target>`.
        #[arg(long = "occasion", value_name = "OCCASION")]
        occasion: Vec<String>,
        /// A YAML document carrying the same surfaces (`state:`/`facts:`/
        /// `choose:`/`events:`/`accepts:`/`visited:`/`occasions:`); CLI flags
        /// compose with it, the flag winning on a conflict.
        #[arg(long, value_name = "FILE")]
        mock: Option<PathBuf>,
        #[command(flatten)]
        json: JsonOpt,
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
        /// walked like a scene shot body (`--choice` picks),
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
    /// them — ready for `lute check`. Defaults to the current directory; it
    /// refuses (exit `2`, nothing written) when any file it would write
    /// already exists.
    Init {
        /// Directory to create or fill (default `.`).
        #[arg(value_name = "DIR")]
        dir: Option<PathBuf>,
        /// Project template: `minimal` (default), `investigation` (a
        /// whodunit on the `beats` skeleton: `examine`/`interview`
        /// occasions, evidence lore, a negated derived rule, an accusation
        /// guarded by derived facts), or `beats` (an occasions plugin, beats
        /// with `id:`, a quest, lore, a play script and scenario tests).
        #[arg(long, value_name = "NAME")]
        template: Option<String>,
        #[command(flatten)]
        json: JsonOpt,
    },
    /// Scaffold one new document (scene, quest, lore, schema) into a project.
    ///
    /// Scaffold one new document into the project (`--project`, else the
    /// nearest `lute.project.yaml` at or above the current directory): `lute
    /// new scene <name> [--occasion <occasion> [--target <target>]]` / `lute
    /// new quest <name> [--start]` / `lute new lore <name>` / `lute new
    /// schema <name>`.
    /// The id comes from the name as typed (`isles.harborNight` → `id:
    /// isles.harborNight`; words of a phrase join in camel case, `The
    /// Epilogue` → `theEpilogue`) and the file is named after the id; a
    /// document omits what the manifest's `defaults:` supplies. Outside a
    /// project `lute new` says so (and `--occasion` is refused).
    #[command(subcommand)]
    New(NewCommand),
    /// Diagnose the local toolchain and project setup.
    ///
    /// Diagnose the local toolchain + project setup: versions, project
    /// manifest, provider snapshots, vocabulary slots, and editor integration
    /// hints. Exit `0` (it reports, never gates) unless `--strict`. The
    /// project is `--project`, else the nearest `lute.project.yaml` at or
    /// above the current directory; with none, the current directory is
    /// scanned and the missing manifest is reported.
    Doctor {
        #[command(flatten)]
        project: ProjectOpt,
        #[command(flatten)]
        json: JsonOpt,
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
        #[arg(value_name = "FILE")]
        artifact: PathBuf,
        /// Engine capability matrix YAML (defaults to the built-in reference matrix).
        #[arg(long, value_name = "FILE")]
        engine: Option<PathBuf>,
        /// A YAML mock playthrough (same surfaces as `lute trace --mock`).
        #[arg(long, value_name = "FILE")]
        mock: Option<PathBuf>,
        /// Raise an occasion after a quest execution IR's walk settles, after
        /// the mock's own `occasions:`, in CLI order (repeatable): each raise
        /// judges the `on="<occasion>"` objectives of every active quest. A
        /// raise for a target is `<occasion>@<target>`.
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
        #[arg(long = "conditions-output", value_name = "FILE")]
        conditions_output: Option<PathBuf>,
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
        #[command(flatten)]
        project: ProjectOpt,
        #[command(flatten)]
        providers: ProvidersOpt,
        #[command(flatten)]
        permission_profile: PermissionProfileOpt,
        #[command(flatten)]
        gate: GateOpts,
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
        /// `--derivation` and `--conditions-output`.
        #[arg(long)]
        events: bool,
        #[command(flatten)]
        json: JsonOpt,
        /// Do not apply the project's Datalog rules: an unmocked derived
        /// atom is unknown and halts the walk incomplete.
        #[arg(long)]
        no_derive: bool,
        /// After the play, print the derivation tree of a ground atom — the
        /// rule used and each premise's own support, negated premises shown
        /// absent — or, when it does not hold, the failing premises of every
        /// rule that could conclude it (repeatable).
        #[arg(long = "derivation", value_name = "ATOM")]
        derivation: Vec<String>,
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
        #[arg(long = "conditions-output", value_name = "FILE")]
        conditions_output: Option<PathBuf>,
    },
    /// Run the project's scenario tests (`*.test.yaml`) and expectation plays (`*.play.yaml`).
    ///
    /// Run the project's scenario tests: every `*.test.yaml` under the
    /// `<PATH>` operands (default: the project directory) traces its scene
    /// (or, with `entry:`/`entries:`, presents its lore
    /// entries) against the declared mocks and asserts the declared
    /// expectations (transcript, offered options, state, quest status); every
    /// `*.play.yaml` under them that carries an `expect:` is played and
    /// judged as `lute play` does. A `<PATH>` is a script file or a directory
    /// walked for scripts. The project is `--project`, else the nearest
    /// `lute.project.yaml` at or above the current directory; each script
    /// resolves, like `lute trace`, against its nearest `lute.project.yaml`
    /// at or below the project directory (as `lute check`), so the document's
    /// `profile:`/`plugins:` frontmatter and the manifest's `defaults: uses:`
    /// hoist are both applied before tracing. A play runs its nearest
    /// project. `--deny` / `--deny-warnings` promote the diagnostics of every
    /// check a test consults, so a denied warning fails that test.
    Test {
        /// Scripts to run: a `*.test.yaml` / `*.play.yaml` file, or a
        /// directory walked for them (default: the project directory).
        #[arg(value_name = "PATH")]
        paths: Vec<PathBuf>,
        #[command(flatten)]
        project: ProjectOpt,
        #[command(flatten)]
        providers: ProvidersOpt,
        #[command(flatten)]
        permission_profile: PermissionProfileOpt,
        #[command(flatten)]
        gate: GateOpts,
        #[command(flatten)]
        json: JsonOpt,
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
        #[command(flatten)]
        project: ProjectOpt,
        #[command(flatten)]
        json: JsonOpt,
    },
    /// List who uses which engine content id: directive attribute values or reward targets.
    ///
    /// Who uses which engine content id: every value of a
    /// directive attribute (`--attr give.item`) or every target of a reward
    /// kind (`--reward ITEM`), with the documents and lines using it — so a
    /// lead sees "who gives what" before a merge. Read-only; documents need
    /// not check clean. Exit `0` on success, `2` on an I/O or usage failure.
    Refs {
        #[command(flatten)]
        project: ProjectOpt,
        /// A directive attribute, `<directive>.<attr>` (repeatable).
        #[arg(long, value_name = "DIRECTIVE.ATTR")]
        attr: Vec<String>,
        /// A reward kind whose `target=` values to list (repeatable); a
        /// reward without a target is listed as `(no target)`.
        #[arg(long, value_name = "KIND")]
        reward: Vec<String>,
        #[command(flatten)]
        json: JsonOpt,
    },
    /// Report project connectivity: the node graph, reachability and envelope tables.
    ///
    /// Project-wide, read-only reporting surface over everything the
    /// connectivity layer computes: the assembled node/edge
    /// graph, per-node reachability plus its declared `after` structure, and
    /// the Guaranteed/Possible envelope tables — including the
    /// `Possible \ Guaranteed` warning-grade reads the whole-project `check`
    /// computes and drops by default. Evaluates no CEL, runs no Datalog,
    /// takes no mocks — pure graph math over declared structure, reusing the
    /// SAME per-root project-doc collection the whole-project `check` builds
    /// (never a second file-walk/parse). Exit `0` on success, `2` on an I/O
    /// failure or an unresolvable node id.
    Scenario {
        #[command(flatten)]
        project: ProjectOpt,
        #[command(flatten)]
        providers: ProvidersOpt,
        /// Text rendering: `text` (default) or `dot` (Graphviz). `--json`
        /// replaces the former `--format json`.
        #[arg(long, value_name = "FORMAT", global = true, value_parser = ["text", "dot"])]
        format: Option<String>,
        #[command(flatten)]
        json: JsonOpt,
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
    /// `once`, `after:`, `when` and the static verdicts the whole-project
    /// `check` reaches about it — unreachable, shadowed, tied, once-per-run over
    /// user state. Read-only; documents need not check
    /// clean. Exit `0` on success, `2` on an I/O failure or an unknown
    /// `--occasion`.
    Beats {
        #[command(flatten)]
        project: ProjectOpt,
        /// Only this occasion's ladders (repeatable).
        #[arg(long, value_name = "OCCASION")]
        occasion: Vec<String>,
        /// Only the ladders raised for this target (repeatable).
        #[arg(long, value_name = "TARGET")]
        target: Vec<String>,
        #[command(flatten)]
        json: JsonOpt,
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
        #[command(flatten)]
        project: ProjectOpt,
        #[command(flatten)]
        providers: ProvidersOpt,
        #[command(flatten)]
        permission_profile: PermissionProfileOpt,
        #[command(flatten)]
        gate: GateOpts,
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
        /// beat answers).
        #[arg(long, value_name = "OCCASION")]
        occasion: Vec<String>,
        /// An occasion to evaluate that varies only the named axes:
        /// `<occasion>=<axes>` (repeatable), `<axes>` a comma list of axis
        /// names. The occasion is evaluated where every other axis is at its
        /// first value (`dayEnd=run.day,run.slot=night` holds `run.slot` at
        /// `night` instead) and left blank elsewhere — `dayEnd=run.day`
        /// reads a once-a-day occasion once per day. It also selects the
        /// occasion; do not name it with `--occasion` too.
        #[arg(long = "occasion-axes", value_name = "OCCASION=AXES", value_parser = parse_occasion_axes_flag)]
        occasion_axes: Vec<(String, String)>,
        /// A relation whose facts to show per cell once it has settled
        /// (repeatable): in text a table with a row per first argument and a
        /// column per cell (`--relation at`: who is where, when); a per-cell
        /// list in `--json` / `--format csv`.
        #[arg(long, value_name = "RELATION")]
        relation: Vec<String>,
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
        #[arg(long = "where", value_name = "EXPR")]
        where_: Option<String>,
        /// Output format: `text` (default) or `csv` (one row per cell and
        /// occasion/target). Not with `--json`.
        #[arg(long, value_name = "FORMAT", value_parser = ["text", "csv"])]
        format: Option<String>,
        #[command(flatten)]
        json: JsonOpt,
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
            Command::New(NewCommand::Scene { .. }) => "new scene",
            Command::New(NewCommand::Quest { .. }) => "new quest",
            Command::New(NewCommand::Lore { .. }) => "new lore",
            Command::New(NewCommand::Schema { .. }) => "new schema",
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
        /// `envelope --node quest:<id>`); `scene:`/`beat:` prefixes disambiguate.
        #[arg(
            long,
            value_name = "NODE",
            required_unless_present = "endings",
            conflicts_with = "endings"
        )]
        node: Option<String>,
        /// Every ending instead of one node: with `=<occasion>`, every beat
        /// answering that occasion; bare, every beat whose content can run
        /// `::end` (its own body or a `::use`d component). Each row has the
        /// `after:` verdict, the `when` verdict the whole-project `check`
        /// reaches (never holds / never wins), and what a satisfiable `when`
        /// reads with who produces it.
        #[arg(long, value_name = "OCCASION", num_args = 0..=1, default_missing_value = "", require_equals = true)]
        endings: Option<String>,
    },
    /// Report the Guaranteed/Possible envelope tables for a node.
    ///
    /// Report the Guaranteed/Possible envelope tables for a node — full
    /// tables for a scene or a quest with `follows=`; defaults-only `D`
    /// plus an enrichment note for a bare quest. Also prints the
    /// `Possible \ Guaranteed` warning-grade reads for the node —
    /// suppressed by default in the whole-project `check`, surfaced here.
    Envelope {
        /// A scene's canonical key, a bundle beat's `<document id>.<beat
        /// id>`, or `quest:<id>` for a quest.
        #[arg(long, value_name = "NODE")]
        node: String,
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
        #[arg(long, value_name = "NODE")]
        node: Option<String>,
    },
}

/// See [`Command::New`]: one subcommand per document kind, with the options
/// only that kind uses.
#[derive(Subcommand)]
pub(crate) enum NewCommand {
    /// Scaffold a scene (a beat with `--occasion`).
    Scene {
        /// The new document's name (`/` nests it in a subfolder: `lute new
        /// scene talk/tomas-evening` → `scenes/talk/tomasEvening.lute`).
        #[arg(value_name = "NAME")]
        name: String,
        #[command(flatten)]
        project: ProjectOpt,
        /// Make the scene a beat answering this occasion. An occasion raised
        /// for a target needs `--target`.
        #[arg(long, value_name = "OCCASION")]
        occasion: Option<String>,
        /// The target the beat answers for (a targeted occasion's
        /// `<prefix>.<member>`).
        #[arg(long, value_name = "TARGET")]
        target: Option<String>,
        #[command(flatten)]
        json: JsonOpt,
    },
    /// Scaffold a quest.
    Quest {
        /// The new document's name (`/` nests it in a subfolder).
        #[arg(value_name = "NAME")]
        name: String,
        #[command(flatten)]
        project: ProjectOpt,
        /// Scaffold an auto-starting quest (`start="true"`) instead of the
        /// default accept-driven stub.
        #[arg(long)]
        start: bool,
        #[command(flatten)]
        json: JsonOpt,
    },
    /// Scaffold a lore document.
    Lore {
        /// The new document's name (`/` nests it in a subfolder).
        #[arg(value_name = "NAME")]
        name: String,
        #[command(flatten)]
        project: ProjectOpt,
        #[command(flatten)]
        json: JsonOpt,
    },
    /// Scaffold a state schema (`<name>.schema.yaml`).
    Schema {
        /// The new schema's name.
        #[arg(value_name = "NAME")]
        name: String,
        #[command(flatten)]
        project: ProjectOpt,
        #[command(flatten)]
        json: JsonOpt,
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
        #[command(flatten)]
        project: ProjectOpt,
        /// Output format: `csv`. JSON is the default; omit `--format` for it.
        #[arg(long, value_name = "FORMAT", value_parser = ["csv"])]
        format: Option<String>,
        /// Write the export here instead of stdout.
        #[arg(short = 'o', long = "output", value_name = "PATH")]
        output: Option<PathBuf>,
    },
    /// Canonicalize translated `loc export` files into one locale bundle.
    ///
    /// Canonicalize translated `loc export` files into ONE locale bundle —
    /// the reverse direction, consumed by
    /// `lute compile --locale-bundle <bundle.json>`.
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
    ///
    /// `--json` prints the `0.48.0.loc-import` report on stdout and so needs
    /// `-o, --output` (the bundle is the artifact).
    Import {
        /// One translated export per locale.
        #[arg(required = true, value_name = "FILE")]
        files: Vec<PathBuf>,
        /// Write the bundle here instead of stdout.
        #[arg(short = 'o', long = "output", value_name = "PATH")]
        output: Option<PathBuf>,
        #[command(flatten)]
        json: JsonOpt,
    },
    /// Word-count and line-count report per document and per speaker.
    Report {
        #[command(flatten)]
        project: ProjectOpt,
        #[command(flatten)]
        json: JsonOpt,
    },
}

#[derive(Subcommand)]
pub(crate) enum CatalogCommand {
    /// Re-stamp and rewrite the pinned provider snapshots in a directory.
    Refresh {
        /// `--providers` is the directory holding the flat per-snapshot YAML
        /// files to refresh (default: the project's `catalogDir`); the project
        /// (`--project`, else the nearest `lute.project.yaml`) stamps them
        /// with its resolved multi-plugin `capabilitySnapshot`.
        #[command(flatten)]
        project: ProjectOpt,
        #[command(flatten)]
        providers: ProvidersOpt,
        #[command(flatten)]
        json: JsonOpt,
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

/// Parse a `--choice <branchOrHubId>=<choiceId>[,<choiceId>…]` flag into
/// `(id, choice ids)` — a hub's comma list forces its whole ordered visit
/// sequence (dsl 0.4.0 §4.3/§4.4). Same clap-level rejection as
/// [`parse_state_flag`] for a malformed flag.
fn parse_choice_flag(raw: &str) -> Result<(String, Vec<String>), String> {
    let (id, rest) = raw.split_once('=').ok_or_else(|| {
        format!("`--choice` must be `<branch>=<choice>[,<choice>...]`, got `{raw}`")
    })?;
    let choices: Vec<String> = rest.split(',').map(str::to_string).collect();
    if id.is_empty() || choices.iter().any(|c| c.is_empty()) {
        return Err(format!(
            "`--choice` must be `<branch>=<choice>[,<choice>...]`, got `{raw}`"
        ));
    }
    Ok((id.to_string(), choices))
}

/// Parse a `--occasion-axes <occasion>=<axes>` flag into `(occasion, axes)`;
/// `<axes>` is the comma list the grid understands (`run.day,run.slot=night`),
/// validated where the grid reads it. Rejected by clap itself, like
/// [`parse_state_flag`], when there is no `=` or either side is empty.
fn parse_occasion_axes_flag(raw: &str) -> Result<(String, String), String> {
    match raw.split_once('=') {
        Some((occasion, axes)) if !occasion.is_empty() && !axes.is_empty() => {
            Ok((occasion.to_string(), axes.to_string()))
        }
        _ => Err(format!(
            "`--occasion-axes` must be `<occasion>=<axes>`, got `{raw}`"
        )),
    }
}
