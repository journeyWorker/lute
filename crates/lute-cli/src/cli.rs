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

#[derive(Subcommand)]
pub(crate) enum Command {
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
        /// §4/§11). Omit for the nearest `lute.project.yaml` above the file, or
        /// a core-only (`lute.core`) check when there is none.
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
        #[arg(long = "deny", value_name = "CODE", value_parser = codes::parse_deny_code)]
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
        #[arg(long = "deny", value_name = "CODE", value_parser = codes::parse_deny_code)]
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
        /// document's activated capability snapshot. Omit for the nearest
        /// `lute.project.yaml` above the file (as `lute check`); only an
        /// explicit `--project` gates on the reconciled project verdict.
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
        #[arg(long = "deny", value_name = "CODE", value_parser = codes::parse_deny_code)]
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
        /// Omit for the nearest `lute.project.yaml` above the file.
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
        /// §4/§11). Omit for the nearest `lute.project.yaml` above the file, or
        /// a core-only (`lute.core`) surface when there is none.
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
        /// snapshot (plugin §4/§11). Omit for the nearest `lute.project.yaml`
        /// above the file (as `lute check`), or a core-only (`lute.core`)
        /// trace when there is none; only an explicit `--project` gates on
        /// the reconciled project verdict.
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
        /// Leave out the candidates that were not eligible at each raise:
        /// the transcript keeps the winners, the lines, the quests and the
        /// expectations (round-5 T3-16). Without it, five or more `when:
        /// false` candidates at one raise fold into one count line.
        #[arg(long)]
        quiet: bool,
    },
    /// Run the project's scenario tests: every `*.test.yaml` under `dir`
    /// traces its scene (or, with `entry:`/`entries:`, presents its lore
    /// entries) against the declared mocks and asserts the declared
    /// expectations (transcript, offered options, state, quest status); every
    /// `*.play.yaml` under `dir` that carries an `expect:` is played and
    /// judged as `lute play` does (dsl 0.22.0 §4). Resolves each traced
    /// document identically to `lute trace` ([`build_input`]): against
    /// `--project`, else the nearest `lute.project.yaml` above the document
    /// (as `lute check`), so the document's `profile:`/`plugins:` frontmatter
    /// and the manifest's `defaults: uses:` hoist are both applied before
    /// tracing. A play runs `--project`, else the nearest `lute.project.yaml`
    /// above it.
    ///
    /// [`build_input`]: crate::input::build_input
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
        /// capability snapshot (plugin §4/§11). Omit for each document's
        /// nearest `lute.project.yaml`, or a core-only (`lute.core`) test.
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
        #[arg(long, value_name = "DIR", global = true)]
        providers: Option<PathBuf>,
        /// Output format: `text` (default), `json`, or `dot` (Graphviz).
        /// Accepted before or after the sub-view (`reach … --format json`).
        #[arg(long, value_name = "FORMAT", global = true)]
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
pub(crate) enum ScenarioCommand {
    /// Report a node's reachability verdict (Reachable/Unreachable/Unknown,
    /// T6) plus its declared `after` prerequisite structure (dsl §5:575) —
    /// or, with `--endings`, one row per ending (T3-20).
    Reach {
        /// A scene's canonical key (e.g. `marina.s01ep02`), a bundle beat's
        /// `<document id>.<beat id>`, or `quest:<id>` for a quest (dsl
        /// §4.4's `envelope quest:<id>` syntax); `scene:`/`beat:` prefixes
        /// disambiguate.
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
pub(crate) enum LocCommand {
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
pub(crate) enum CatalogCommand {
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
