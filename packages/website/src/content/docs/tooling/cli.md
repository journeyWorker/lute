---
title: CLI reference
description: Every lute subcommand — init, new, check, compile (incl. --stream), expr, run, play, trace, test, lint, scenario, beats, calendar, refs, loc, context, tag, fix, fmt, lore, doctor, catalog refresh, version — with its 0.48 synopsis, the shared --project discovery rule, --json reports, the removed 0.47 spellings, and the exit-code contract.
---

`lute` is the headless checker and compiler for `.lute` documents. The core `check()` is the contract; the CLI adds argument parsing, file I/O, and output formatting, and owns no validation logic. Since 0.48 the command line follows one grammar: one command per action, one option name per concept, the project always named `--project <DIR>`, and `--json` on every command that reports a result. Text is the default output everywhere; JSON is produced only with an explicit `--json`. [Project and discovery](#project-and-discovery) below says how a command finds its project and lists the option groups commands share; [Removed in 0.48](#removed-in-048) maps every 0.47 spelling to its replacement. On the permission-aware authoring commands below, `--permission-profile <NAME>` requires project resolution and applies that trusted profile's [permissions](/tooling/capability-permissions/) as an additional ceiling without activating its plugins or changing the source profile.

## Project and discovery

A command that works on a project takes it as `--project <DIR>` and only that way — never as an operand and never under another name. `--providers <DIR>` pins a directory of provider snapshots to resolve ids against.

- **Discovery.** When `--project` is absent, the project is the nearest ancestor directory containing `lute.project.yaml`, starting at the directory of each file operand for `check`, `compile`, `context` and `trace` (per file), else at the current directory. The discovered directory is used exactly as if it had been passed with `--project` — in particular `compile <FILE>` and `trace <FILE>` gate on the reconciled project verdict, and `trace` uses the project's single-root producer set. The stderr note `lute: note: using project <dir> (nearest lute.project.yaml); pass --project to choose another` is printed by the file commands, once per distinct project. With no manifest above the file, a single-file command resolves core-only (`lute.core`). Before 0.27.0 only `check` and `test` looked for the manifest, so `trace` could refuse with `E-UNDECLARED` a document `check` passed.
- **The project directory as walk root.** For every command that takes a directory, `--project <DIR>` (explicit or discovered) is the walk root: each document (or script) resolves against its nearest project at or below `<DIR>` — nested projects keep their own documents, and a directory without `lute.project.yaml` is a workspace whose documents resolve against their own projects, else core-only. Discovery itself only ever yields a directory with a manifest.
- **None found.** With no `--project` and no ancestor manifest, a command that needs a project — whole-project `check`, `compile` without a document, `impact`, `constraints`, `patch`, `play`, `test`, `loc export`, `loc report`, `scenario` and its subcommands, `beats`, `calendar`, `lore`, `refs`, `expr` — fails with `E-CLI-USAGE`, `argument: "--project"` and the message `lute <command>: no lute.project.yaml at or above <dir> — pass the project directory with --project <DIR>`. `check <FILE>…`, `compile <FILE>`, `context`, `trace`, `lint`, `fmt`, `fix`, `tag`, `new` and `doctor` keep working without a project.
- **`init` is the exception:** its operand is the directory to create (default `.`).

### Shared options

Options for one concept are declared once and mean the same on every command that takes them:

| Group | Options | Commands |
|---|---|---|
| Project | `--project <DIR>` | every command that works on a project |
| Providers | `--providers <DIR>` | `check`, `compile`, `context`, `trace`, `lint`, `impact`, `constraints`, `patch`, `play`, `test`, `scenario` (global), `calendar`, and `catalog refresh` (the directory it refreshes) |
| Permission profile | `--permission-profile <NAME>` | `check`, `compile`, `context`, `trace`, `expr`, `patch`, `play`, `test`, `calendar` |
| Diagnostic gate | `--deny <CODE>` (repeatable), `--deny-warnings` | `check`, `compile` (not with `--stream`), `lint`, `trace`, `expr`, `play`, `test`, `calendar` |
| JSON | `--json` | every command that reports a result, except `schema` (always JSON) and `loc export` (an artifact writer) |

`--deny*` promotes diagnostics of the verdict the command exits on: `trace` its check gate (refusal, exit **1**), `expr` the lowering diagnostics, `play` and `calendar` the project compile gate, `test` every check verdict a test consults (each traced document's check and each play's compile gate), so a denied warning fails that test. The one short alias is `-o` for `--output`.

### Project diagnostic levels (since 0.50)

A project can set the level of a checker warning in `lute.project.yaml`:

```yaml
diagnostics:
  W-DISPLAY-NAME-DUP: error
  W-STAGE-ABSENT: { level: off, paths: ["scenes/imported/**"] }
```

`diagnostics:` is a map from diagnostic code to either a level or `{ level, paths }`; one entry per code.

- **Levels:** `off`, `info`, `warn`, `error`.
- **`paths`** (optional): project-root-relative globs, with the lint `ignore:` syntax (`*` within a segment, `**` across segments). When `paths` is given the level applies only to diagnostics of documents whose root-relative path matches a glob; elsewhere the code keeps its native severity. A diagnostic without a document path (a manifest diagnostic) matches only an entry without `paths`.
- **Only `W-*` codes are configurable.** Errors are never demoted: an error is a contract violation. `L-*` lint rules keep their level in `lute.lint.yaml` `rules:`.

A malformed entry is **`E-DIAGNOSTICS-CONFIG`** (error, a manifest diagnostic). It is reported once per manifest by the CLI manifest pass and by the LSP; the entry is dropped, the rest loads, and the manifest is invalid:

| Fault | Message shape |
| --- | --- |
| an `E-*` code | `` `E-UNKNOWN-ATTR` is an error: an error is a contract violation and cannot be re-levelled; fix the reported source instead `` |
| an `L-*` code | `` `L-…` is a lint rule; set its level in `lute.lint.yaml` `rules:` `` |
| a code the toolchain does not define | `` `W-STAGE-ABSNT` is not a diagnostic code — did you mean `W-STAGE-ABSENT`? `` |
| a bad level | `` level `silent` is not one of `off`, `info`, `warn`, `error` `` |
| `paths: []`, a non-string, an absolute path or a `..` segment | `` `paths` takes project-relative globs, e.g. `scenes/imported/**` `` |

**Order of application**, at the one place that serializes and prints diagnostics (`check`, `compile`, `lint`, `trace` and `expr` read the project's `diagnostics:`: a document command each document's own project, a directory command the invoked root's; the simulating commands `test`, `play` and `calendar` apply no project levels and keep the plain `--deny`/`--deny-warnings` gate, so a project `error` level never fails a test):

1. the project level for the (code, path);
2. `--deny CODE` naming the code promotes it to error, **even when the project set it `off`** (an explicit command line beats the config);
3. `--deny-warnings` promotes what is still a warning after step 1. An `off` code stays off, and an `info` code stays info.

Levels change **only what is reported and the exit status**. They never change analyses, artifacts, constraint verdicts, the scenario graph, or what a `.play.yaml`/`.test.yaml` expectation observes. A project without `diagnostics:` produces byte-identical output.

**Markers and counts.** A code relevelled to `error` fails the verdict as `--deny` does; in JSON it carries `"severity": "error"` and the additive `"configured": "lute.project.yaml"`, and the human line gets the marker `[project]` beside the existing `[denied]`. A code relevelled to `info` or `warn` carries its new severity and the same `configured` marker. Suppressed (`off`) findings are counted:

- every human summary line that counts diagnostics appends `, N suppressed by lute.project.yaml` **only when N > 0**: `ok: ./scenes/a.lute (0 warning(s), 212 suppressed by lute.project.yaml)`;
- every JSON report that carries diagnostics (the `0.48.0.check` report per file and for the project, and the compile, lint, trace and expr envelopes) gains `"suppressed": { "<code>": n, … }` **only when non-empty**. It is additive, so report ids are unchanged.

```console
$ cd docs/examples/plugin-surfaces
$ lute check ./scenes/imported/street.lute
lute: note: using project . (nearest lute.project.yaml); pass --project to choose another
ok: ./scenes/imported/street.lute (0 warning(s), 1 suppressed by lute.project.yaml)
```

**LSP.** The language server applies the same levels with the document's root-relative path before publishing: an `off` diagnostic is not published, and a relevelled one is published at its new severity.

## Machine-readable output (0.43)

Text is the default everywhere; JSON is produced only with an explicit `--json`. No output depends on whether a stream is a terminal or on an environment variable, and `--format json` no longer exists. With `--json` a command prints exactly one JSON document on stdout — the report envelope, or the error envelope on a usage, I/O or input failure — and messages that would otherwise go to stdout move to stderr.

Every command has an **id**: its subcommand path as typed — `check`, `scenario reach`, `loc report`, `new scene`. Flags never change it. The same id appears in reports, error envelopes, `lute schema` and telemetry.

**Reports.** A report printed in JSON mode is one JSON object with three header fields beside the report's own: `command` (the id), `schemaVersion` (the payload's shape version) and `ok` — `true` exactly when the process exits **0**. Keys are emitted in sorted order; read them by name.

| Command | JSON flag | `schemaVersion` |
|---|---|---|
| `check` | `--json` | `0.48.0.check` |
| `impact` | `--json` | `0.43.0.impact` |
| `constraints` | `--json` | `0.49.0.constraints` |
| `lint` | `--json` | `0.43.0.lint` |
| `compile` | `--json` | `0.43.0.compile` |
| `expr` | `--json` | `0.43.0.expr` |
| `context` | `--json` | `0.43.0.surface` |
| `context` | `--json --node` | `0.36.0.context` |
| `context` | `--json --at` | `0.36.0.position` |
| `diff` | `--json` | `0.36.0.diff` |
| `patch` | `--json` | `0.43.0.patch` |
| `trace` | `--json` | `0.43.0.trace` |
| `play` | `--json` | `0.43.0.play` |
| `test` | `--json` | `0.43.0.test` |
| `scenario` | `--json` | `0.43.0.scenario` |
| `scenario reach` | `--json` | `0.43.0.scenario-reach` |
| `scenario envelope` | `--json` | `0.43.0.scenario-envelope` |
| `scenario knowledge` | `--json` | `0.43.0.scenario-knowledge` |
| `beats` | `--json` | `0.43.0.beats` |
| `calendar` | `--json` | `0.43.0.calendar` |
| `refs` | `--json` | `0.43.0.refs` |
| `lore` | `--json` | `0.43.0.lore` |
| `loc report` | `--json` | `0.47.0.loc-report` |
| `loc import` | `--json` | `0.48.0.loc-import` |
| `doctor` | `--json` | `0.43.0.doctor` |
| `tag` | `--json` | `0.48.0.tag` |
| `fix` | `--json` | `0.48.0.fix` |
| `fmt` | `--json` | `0.48.0.fmt` |
| `catalog refresh` | `--json` | `0.48.0.catalog-refresh` |
| `init` | `--json` | `0.48.0.init` |
| `new scene`, `new quest`, `new lore`, `new schema` | `--json` | `0.48.0.new` |
| `schema` | always | `0.48.0.schema` |

`check` has one report for both modes: `files[]` holds one entry per document checked, and `project_diagnostics` is present exactly when the whole project ran (see [`check`](#check)). Three reports that used to be bare arrays are objects now: `constraints` puts its rows under `constraints`; a `trace` refusal and a `compile` refusal put their diagnostics under `diagnostics`. `patch` reports whether the patch was accepted in `accepted` (its `ok` is the exit rule, see [`lute patch`](#lute-patch-0366)).

**Not reports.** Artifacts and engine contracts keep their own shape: the execution IR `compile` writes (and the project compile's output directory), the NDJSON of `compile --stream`, `run --json`, `play --events`, `loc export` rows (JSON by default, `--format csv` for CSV; `loc export` takes no `--json`) and `version --json`.

**Errors.** A run is in JSON mode when its arguments before `--` contain `--json`, or the command is `schema`. In JSON mode, a failure because the command line is invalid, a path cannot be read or written, or an input file is not valid prints the error envelope on stdout and still exits **2**; the human message stays on stderr, unchanged:

```json
{
  "command": "check",
  "error": {
    "code": "E-CLI-IO",
    "message": "lute: cannot read missing.lute: no such file or directory",
    "path": "missing.lute"
  },
  "ok": false,
  "schemaVersion": "0.43.0.error"
}
```

`error.code` is `E-CLI-USAGE` (invalid command line — `error.argument` names the argument and `error.hint` carries the parser's suggestion when it has one), `E-CLI-IO` (a path cannot be read or written; `error.path`) or `E-CLI-INPUT` (a file was read but is malformed, of the wrong kind or an unsupported version; `error.path` when one file is at fault). `lute --explain E-CLI-USAGE` explains each. The [removed 0.47 spellings](#removed-in-048) are `E-CLI-USAGE` errors too. `compile --stream` and `play --events` keep their stream protocols and never print the envelope; `patch` keeps its own refusal documents.

## schema

```console
$ lute schema [<COMMAND>]…
```

Describes the command line itself as the `schema` report (always JSON; reads no files). With no path it lists the top-level commands (`id`, `summary`, `hasSubcommands`); with a path (`lute schema scenario reach`) it describes that command: `arguments` and `options` (`name`, `required`, `repeatable`, `takesValue`, and `short`, `valueName`, `possibleValues`, `default`, `help` where they apply), `subcommands`, `outputs` — each machine-readable output with the `flag` selecting it, its `kind` (`report`, `artifact` or `stream`) and a report's `schemaVersion` — and `inputs` (0.47): one entry per structured input the command reads, an empty list for every command but `patch`, whose `FILE` request is described as `{ argument, kind: "request", format: "json", fields, ops, preserve, nodeKinds }` (see [the patch request](/tooling/ai-harness/#the-patch-request-047)). The data comes from the parser's own definition, so it cannot drift from `--help` or the patch request parser. `lute schema compile`, for example, lists the outputs `artifact` (no flag), `report` (`--json`, `0.43.0.compile`) and `stream` (`--stream`). A CI test walks this schema and fails when two commands spell one concept differently, so the grammar cannot drift. An unknown path is `E-CLI-USAGE`; exit **0**, or **2**.

## Telemetry

Each `lute` invocation appends one line to `telemetry.jsonl` in Lute's state directory — `$XDG_STATE_HOME/lute` when that is an absolute path, else `$HOME/.local/state/lute`; `%LOCALAPPDATA%\lute` on Windows:

```json
{"v":1,"ts":1760400000000,"lute":"0.51.0","command":"check","json":true,"exit":0,"durationMs":12}
```

That is the whole record: the command id (or `null`), whether the run was in JSON mode, the exit code, the wall time and the toolchain version. No argument or flag value, path, file content, output or environment is recorded, and nothing is sent anywhere — the file stays on your machine. It is rotated to `telemetry.jsonl.1` at 1 MiB; recording is best-effort and never changes a command's output or exit code. Turn it off with `LUTE_TELEMETRY=0` (or `false`, `off`, `no`) or `DO_NOT_TRACK=1`; `lute doctor` shows which (`• Telemetry: off (LUTE_TELEMETRY)`, or the file path when it is on). The repository's own tests, scripts and CI run with it off.

## check

```console
$ lute check [<FILE>]… [--project <DIR>] [--providers <DIR>] [--permission-profile <NAME>]
             [--engine <FILE>] [--wip] [--deny <CODE>]… [--deny-warnings] [--json]
```

Statically validate `.lute` documents — one command for both scopes.

- **Documents** — `lute check <FILE>…`. Each document is checked on its own, in operand order (`.yaml`/`.yml` schema operands too), resolved in its project (`--project`, else discovered from that file's directory, per file). Relational guard decisions — dead guards over relations, `W-WIP` — need the whole-project mode below. A directory operand is `E-CLI-USAGE` naming `--project` (`lute check: <DIR> is a directory — check the whole project with lute check --project <DIR>`). An unreadable operand ends the run with exit **2** and the `E-CLI-IO` envelope (no report); operands before it may already have printed their text lines.
- **Whole project** — `lute check` with no `<FILE>`: every document of the project (`--project <DIR>`, else discovered from the current directory) plus the project-wide checks, with nested projects grouped by their own manifests; see [Whole-project mode](#whole-project-mode). `--wip` is valid only here.

Each document is resolved in its project (its `defaults:`, profiles, and plugins): `--project`, else the nearest `lute.project.yaml` above that file, and stderr says so: `lute: note: using project <dir> (nearest lute.project.yaml); pass --project to choose another`. With no manifest above the file, the check is core-only. `--engine <FILE>` loads an immutable `lute.engine.yaml` matrix and performs author-time semantic negotiation after the exact-minor IR gate; each lowered construct requiring an unsupported id is reported as `E-CHECK-ENGINE-SEMANTICS` with its source span, semantic id, and engine name. This is not playback. Exit **0** clean, **1** when any `Error`-severity diagnostic is present, **2** on an I/O or matrix/version failure. `--permission-profile <NAME>` applies a trusted additional ceiling; denied authored effects are non-suppressible `E-PERMISSION-*` errors, and a missing project/profile or invalid policy is an explicit resolver error rather than an unrestricted fallback. `--json` prints the `0.48.0.check` report (below); otherwise a human line per diagnostic. `--deny <CODE>` (repeatable, rustc/clippy `-D` precedent, 0.6.1 §5) promotes every diagnostic with exactly that code to an error for the verdict and exit code, and `--deny-warnings` promotes every warning — a pipeline denies `W-FACT-GUARANTEED` (whole-project mode) to reject redundant relational guards, `W-LUTE-VERSION-STALE` to reject a stale `luteVersion` stamp. A promoted diagnostic reports severity `error` with a `"denied": true` marker in `--json`; an unknown code in `--deny` — including a removed one such as `W-UNPROVEN-RELATIONAL` (0.20.0) — is a usage error (exit **2**), and errors are never demotable.

### Check report

Both modes print one report, `0.48.0.check`, with `--json`:

```json
{
  "command": "check",
  "schemaVersion": "0.48.0.check",
  "ok": true,
  "files": [
    {"path": "scenes/a.lute", "ok": true, "diagnostics": [], "resolved": {"commands_preview": [], "injections": [], "timeline_tables": []}}
  ],
  "project_diagnostics": []
}
```

`files[]` has one entry per document checked, in the order checked: `path` as given or walked, `ok` after `--deny`, the `CheckResult` `diagnostics`, and `resolved` (`null` for a schema operand). A single document is `files[0]`. `project_diagnostics` (each a diagnostic plus `path`) is present exactly when the whole project ran. A run that ends with exit **1** still prints the report in JSON mode: an invalid manifest in whole-project mode gives `files: []` with the manifest diagnostics in `project_diagnostics` (their `path` is the manifest), and a plugin or profile resolve error for a document gives that document's entry with `ok: false` and the resolve diagnostics in its `diagnostics`.

### Whole-project mode

```console
$ lute check [--project <DIR>] [--providers <DIR>] [--permission-profile <NAME>] [--engine <FILE>]
             [--wip] [--deny <CODE>]… [--deny-warnings] [--json]
```

With no `<FILE>` operand, `lute check` checks the **whole project** — `--project <DIR>`, else the project discovered from the current directory (a directory is the walk root, so nested projects keep their own manifests): it recursively checks every `*.lute` file under it in deterministic sorted order (0.29.0: the report prints causes first — rows at `lute.project.yaml`, then plugins, then schemas, then each document in path order, then the project-wide rows), each against its own nearest-ancestor `lute.project.yaml` root, **plus** project-wide `<quest id>` uniqueness, the connectivity passes (`E-CONN-*`, `W-QUEST-REF-UNKNOWN`, `E-STATE-MAYBE-UNAVAILABLE`), and the [relational guard analysis](/state/facts-and-datalog/#how-whole-project-lute-check-analyzes-relational-guards) (dsl 0.20.0): every `holds(…)`/`count(…)` in a guard is decided impossible, guaranteed, or possible over the whole project, so a guard that can never hold is reported through its slot's dead-code error (`E-ARM-DEAD`, `E-ENTRY-UNREACHABLE`, `E-OBJECTIVE-UNSATISFIABLE`, `E-QUEST-UNREACHABLE`) and a redundant one as `W-FACT-GUARANTEED`. The project-wide beat and quest advisories run here too (dsl 0.22.0): `W-BEAT-PRIORITY-TIE` (beats on one `select: first` occasion, for the same or no target, with equal priority and `when`s not provably exclusive, so file order picks the winner), `W-BEAT-ONCE-RUN-USER` (a `once: run` beat whose `when` reads only user-tier state, so it replays every run), and `W-QUEST-HANDLER-DEAD` (an `<on event="questFailed">` on a quest that has no `fail`, no required subquest objective, and no parent quest, so it can never fail). It then compiles every document that checked clean under its project's `identity:`, so compile-stage errors fail here rather than only at `compile`: `E-DUP-VOICEKEY` (lines with different text on one `voiceKey` — since 0.22.0 the default `{prefix}.{speaker}-{code}` keeps scenes apart, so this is what a project pinning the older unprefixed `identity.voiceKey: "{speaker}-{code}"` risks) and `E-CAPABILITY-MISMATCH` (documents resolving two capability snapshots, which a project compile and `play` refuse). `--engine <FILE>` additionally negotiates the project's sorted `requiredSemantics` union and reports each contributing document/span with `E-CHECK-ENGINE-SEMANTICS`; it never opens playback. Exit **0** clean, **1** when any file has an error or a project-wide collision, **2** on I/O or matrix/version failure. The same `--deny <CODE>`/`--deny-warnings` promotion applies project-wide. `--wip` is valid only in this mode; with document operands it is `E-CLI-USAGE` (`argument: "--wip"`).

Since dsl 0.24.0 whole-project `check` also warns `W-QUEST-NEVER-ACCEPTED` (an accept-driven quest — no `start`, a root or an `activate="accept"` child — that no `::accept` in the project names), `W-RELATION-UNREAD` (a relation asserted, seeded or derived that no condition, rule body or def reads) and `W-DEF-UNUSED` (a `@def` nothing references; a schema or plugin def declared `host: true` is read by the host and exempt), each once per project at its declaration, and its `mocks/*.yaml` pass checks a mock's `bridges:` answers against the plugin calls of the document it names (`E-TRACE-MOCK-UNDECLARED` / `E-TRACE-MOCK-TYPE`).
 
In 0.40, when the project contains `player.bindings.yaml`, whole-project `check` also
validates the optional player bindings after compilation. It checks the
bindings schema, asset files under `assetsBase`, domain and cast keys, flow
occasions and targets, effect names and parameters. Binding failures use
`E-BINDING-SHAPE`, `E-BINDING-FILE`, `E-BINDING-KEY`, or
`E-BINDING-EFFECT`; missing and unused entries are
`W-BINDING-MISSING` and `W-BINDING-UNUSED`. JSON output includes diagnostics
anchored in `player.bindings.yaml`.

Since dsl 0.25.0: an `accepts:` mock or test no longer silences `W-QUEST-NEVER-ACCEPTED` (the message names it), a `<quest accept="external">` does, and `accept="external"` on a child that activates with its parent is `E-ACCEPT-TARGET`; [exclusive relations](/state/facts-and-datalog/#exclusive-relations-excludes) make a guard needing both dead, its negation guaranteed (`W-FACT-GUARANTEED`), an assert of one where the other holds on every route `E-FACT-EXCLUSIVE`, and a rule that can only break the exclusion `E-RULE-EXCLUSIVE`; beats of one [`share`](/language/beats/#one-event-several-places-share) key with different `once`s are `E-BEAT-ATTR`; and a bundle beat's `after=` names known nodes (`E-CONN-UNKNOWN-NODE`). A reserved relation's [`changedOn:`](/language/dialogue-and-cast/#presence-after-engine-events-changedon) orders cast presence over the scenario graph, so `assume: true` no longer covers a line after one of its occasions (`W-CAST-ABSENT`). A stale `luteVersion` that every document inherits from the manifest's `defaults:` is one `W-LUTE-VERSION-STALE` at that line of `lute.project.yaml`, ending `— every document inherits it (N documents)`, not one per document; a document-mode `lute check <FILE>` still reports it on the document.

Since dsl 0.26.0, for a project several authors write at once: `E-STATE-DECL-CONFLICT` compares every frontmatter `state:` declaration of one path — inline, or imported from a schema — and names both files and lines when `type`, `default`, `per` or `owner` disagree (`scene.*` paths are scene-local and exempt, and a declaration that refines one it `extends:` is no conflict); `E-REWARD-TARGET` checks a reward kind's `target:` contract — an `{ entity: <kind> }` or `{ provider: <name> }` contract validates the `target=` value, with a did-you-mean, and `required: true` rejects a reward with none; and `E-RULE-AGGREGATE-CYCLE` rejects a rule whose `count(…)` / `countDistinct(…)` reads a relation that depends on the rule's own head. Two advisories join them: `W-DISPLAY-NAME-DUP`, two speakers the dialogue box shows under one name (see [`lint`](#lint) and the [multi-author guide](/guides/multi-author/)), and `W-ENTRY-WRITE-REREAD`, an entry beat without `once` whose body has `::set` or `::retract` — it is presented again on every raise, but its writes apply on its first read in a run only (`::assert` is exempt: the fact holds for the rest of the run either way). Two documents declaring `run.mood` two ways, a misspelt reward target and a missing one, and a repeatable entry that pays out:

<!-- lute-diagnostics -->
```console
./scenes/town/welcome.lute:10:3: error [E-STATE-DECL-CONFLICT] state path `run.mood` is declared as int, default 0 at `./scenes/town/morning.lute:10` but as enum(calm|tense), default "calm" at `./scenes/town/welcome.lute:10`; every declaration of one path must agree on type, default, per and owner — they share one runtime value (declare it once in a schema both documents import)
./quests/fishing.lute:8:31: error [E-REWARD-TARGET] `goodRodd` is not a member of entity kind `bagItem`, which `<reward kind="ITEM">` targets — did you mean `goodRod`?
./quests/fishing.lute:9:3: error [E-REWARD-TARGET] `<reward kind="ITEM">` needs a `target=`: the reward kind declares `target: { required: true }`
```

<!-- lute-diagnostics unverified="verbatim lute check --project output, but composed at runtime: crates/lute-check/src/lore.rs Reread::message joins its remedies with `; `, so no single format! literal spans the text" -->
```console
./lore/contest.lute:17:3: warning [W-ENTRY-WRITE-REREAD] `<entry id="deskNote">` has no `once`, so every raise of `talk` can present it again in a run, but its `::set` applies on the first read in a run only — later reads skip it; to apply it on every presentation, put the body in a `<beat on="talk" target="npc.judge" once="false">` — a beat's writes apply each time it is presented; for one application per run, write `once="run"` on the entry; to keep the write to the first read on purpose, guard it with `when="!entry.deskNote.read"`
```

The same release makes the fact analysis behind these passes cost what the project reads, not beats × facts: the seeds' derived closure is prepared once per project root and extended per slot, so on Monster League (818 beats, 211 static facts) whole-project `check` fell from 6.1 s to about 1.2 s, with the same output.

`--wip` (dsl 0.23.0) is for a project whose content is still being written. `E-ENTRY-UNREACHABLE`, `E-BEAT-UNREACHABLE`, `E-OBJECTIVE-UNSATISFIABLE`, and `E-ARM-DEAD` are reported as the warning `W-WIP` — its message names the error code — when the guard is dead **only** because it reads a relation that nothing produces yet — no seed fact, no `::assert` anywhere, no rule, not `reserved`. Since dsl 0.26.0 §2.6 a relation whose only producer is a component `::assert` with an unbound `@param` — `hasBadge(@badge)` in a badge component no `::use` calls yet — counts as unproduced for specific arguments too, and a required `<objective quest=…>` whose child is dead only for that reason is a warning as well, so a lead can check a spine before the areas that fill it exist. A relation that has producers but can never match stays an error, and so does a downstream `E-CONN-UNREACHABLE` (a quest dead for want of unwritten content still feeds connectivity). An entry guarded on a `rumor` relation no document asserts yet:

<!-- lute-diagnostics unverified="verbatim lute check --project --wip output, but the message is composed at runtime: the E-ENTRY-UNREACHABLE literal plus the `--wip` suffix lute-check's wip_warning appends, so no single format! literal spans it" -->
```console
$ lute check --project . --wip
./lore/tomas.lute:22:74: warning [W-WIP] entry `noteWanted` is never eligible: its `when` guard `holds('rumor', ['mara'])` is provably false — no seed, assert, rule, or engine relation produces `rumor(mara)` under your declared routes — `E-ENTRY-UNREACHABLE` without `--wip`: it is dead only for want of producers not written yet (relations with no seed, assert, rule, or reserved declaration, or written only by a component `::assert` with an unbound `@param`)
```

Without `--wip` the same line is `error [E-ENTRY-UNREACHABLE]` and the command exits **1**; with it the project passes. [`lute scenario knowledge`](/tooling/overviews/#lute-scenario-knowledge) lists every such relation as `NO PRODUCER`. Since 0.23.0 the decider behind these three codes, `E-ARM-DEAD`, and `W-BEAT-PRIORITY-TIE` also reasons per path across `&&`/`||`, so a contradiction such as `run.slot == 'morning' && run.slot == 'evening'` is now reported where 0.22 missed it — see [how a `when` is decided](/language/beats/#how-a-when-is-decided).

Since 0.34.0, project-wide commands load one `ProjectModel` per nearest manifest root; whole-project `check`, `scenario`, `beats`, `impact`, `calendar`, `play`, and `test` therefore share one reconciled snapshot. Analysis-derived results carry an `evidence` level (`proven`, `witnessed`, `bounded`, `heuristic`, or `unknown`). Human output adds `[bounded: <scope>]`, `[heuristic]`, or `[unknown]`; it does not suffix `proven` or `witnessed`. A bounded result means “within this scope”, never “no path exists”.

Constraints declared in `lute.project.yaml` are also checked by whole-project `check`: only violations are diagnostics; holds and unknowns appear in the report below.

Summaries count by severity (0.41.0). `lute check <FILE>` ends each file with `ok: <path> (W warning(s))` or `failed: <path> (E error(s), W warning(s))`, and whole-project `check` with `ok: <dir> (F file(s), W project-wide warning(s))` or `failed: <dir> (F file(s), E project-wide error(s), W project-wide warning(s))`. An `info` diagnostic is never a warning: when a run has any, the count is appended (`, I info` / `, I project-wide info`); a run without one prints exactly the line above. `hint` diagnostics are not counted, and a `--deny`-promoted diagnostic counts as an error only.

A single-file check (and the language server) can add `W-PROJECT-CHECK-SKIPPED`, severity **info**, once per distinct quest id per document: the first read of `quest.<id>.*` whose id the document and its imports do not declare is named as not judged, because quest references are resolved only by whole-project `lute check` (`lute check --project <DIR>`). Whole-project check, `compile` and the wasm build never raise it. `ok` is unchanged; the info count shows in the summary, e.g. `ok: quests/q.lute (0 warning(s), 1 info)`.

A declaration that does not parse no longer cascades (0.41.0). A `type: number` — the type 0.31 removed — is `E-TYPE-NUMBER` at its `number`, naming the choice between `int` and `double` (and suggesting `int` when every value it takes is whole). The declaration stays in the environment by name: a plugin whose export holds it still loads its other entries, and what depends on it is not checked until it is fixed — one `W-DEPENDS-ON-INVALID` note (an info) per invalid declaration, at its first dependant, counts the rest, instead of an `E-CLOCK-DECL` or one `E-UNKNOWN-DIRECTIVE` per use.

## impact

```console
$ lute impact --node <NODE> [--project <DIR>] [--providers <DIR>] [--json]
```

Ask which project items depend on a target. `<NODE>` is the target key; targets are `state:<path>`,
`fact:<rel>(<args>)` (`_` is a wildcard), `relation:<rel>`, `scene:<id>`,
`quest:<id>`, `objective:<quest>.<id>`, `entry:<id>`, `beat:<id>`,
`occasion:<name>`, `def:<name>`, and `component:<name>`. The result is the
transitive reverse-dependency closure over a finite graph: an item is affected
when its eligibility, reachability, presented content, rewards, or effects read
something in the target's reverse closure. This is dependency existence, not a
claim that changing a value must flip the item. Cycles terminate via a visited
set; the strongest chain wins (`proven` > `witnessed` > `bounded` >
`heuristic` > `unknown`).

For example, `lute impact --project docs/examples/games/drowned-crown
--node fact:felled(regent)` reports affected lines, quests/objectives, rewards,
disclosures, beats/scenes, and downstream facts/state. Each item includes a
target-to-item reason chain; each link has an edge, reason, file, line, and
source span.

Since 0.52 (#210) the closure also follows the two anchor edges [`lute scenario`](#scenario) draws:
an [`[accept]`](/connectivity/scene-graph/#edge-kinds) edge from a unit that `::accept`s a quest,
labelled `accepts`, and a
[`[raises]`](/connectivity/scene-graph/#edges-that-declared-raises-draw) edge from a declared raise to
the scene or beat that answers it, labelled `raises`. An `accepts` link is `proven`. A `raises` link is
`proven` when at least one raise site behind it names its target statically, and `heuristic` when
every one reads `occasion.target`. In a project where scenes `a` and `b` both `::accept{quest="q"}`:

```console
$ lute impact --project . --node scene:a
downstream
  state:quest.q.state [proven]
    - accepts: scene(a) accepts quest(q) (scenes/a.lute:1)
    - writes: quest lifecycle state (quests/q.lute:6)
lines
  line:a.narrator_0010 [proven]
    - contains: scene contains line (scenes/a.lute:8)
objectives
  objective:q.o [proven]
    - accepts: scene(a) accepts quest(q) (scenes/a.lute:1)
    - contains: quest contains objective (quests/q.lute:7)
quests
  quest:q [proven]
    - accepts: scene(a) accepts quest(q) (scenes/a.lute:1)
```

`--json` has this shape (empty groups remain present):

```json
{
  "target": {"kind": "fact", "relation": "felled", "args": ["regent"]},
  "root": "docs/examples/games/drowned-crown",
  "items": {
    "lines": [{"key": "line:...", "lineId": "...", "speaker": "brann",
      "file": "lore/brann.lute", "line": 27, "evidence": "proven",
      "reasons": [{"edge": "gates", "reason": "holds('felled',['regent'])",
        "file": "lore/brann.lute", "line": 26,
        "sourceSpan": {"byte_start": 1106, "byte_end": 1172, "line": 26,
          "column": 140, "utf16_range": [1106, 1172]}}]}],
    "quests": [], "objectives": [], "rewards": [], "disclosures": [],
    "beats": [], "downstream": []
  }
}
```

## constraints

```console
$ lute constraints [--project <DIR>] [--providers <DIR>] [--script <FILE>]… [--all-scripts] [--json]
```

Declare project invariants under `constraints:` in `lute.project.yaml`.
Supported kinds and fields are:

```yaml
constraints:
  - id: regent-reachable
    kind: reachable
    node: scene:hub.regentFell
    severity: error
  - id: library-completes
    kind: completable
    quest: libraryKey
  - id: dead-brann-never-speaks
    kind: speaksOnlyWhen
    speaker: brann
    when: "!holds('felled', ['brann'])"
  - id: no-slot-only-progress
    kind: noSingleSlotProgress
    quest: '*'
```

`reachable` takes `node`; `completable` takes `quest`; `speaksOnlyWhen`
takes `speaker` and CEL `when`; `noSingleSlotProgress` takes `quest:<id>` or
`'*'`. Every declaration needs a unique non-empty `id`, a closed-set `kind`,
kind-specific fields, and optional `severity` (default `error`).

Each result is `holds`, `violated`, or `unknown`, with `evidence`; bounded
results include `scope: "declared clock windows; no path search"`. The report
always lists every constraint, including holds and unknowns. `--all-scripts` executes
every `*.play.yaml` and `*.test.yaml` of the project and records witnessed quest-completion
transitions for `completable`; `--script <FILE>` (repeatable) executes only the named scripts — a directory selects every script under it — and the two are exclusive (`E-CLI-USAGE`). Whole-project `check` reports only violations
(`E-CONSTRAINT-VIOLATED`) and malformed declarations (`E-CONSTRAINT-DECL`);
there is no `W-CONSTRAINT-UNKNOWN`.

The 0.49.0 report (`schemaVersion` `0.49.0.constraints`) gives every row a `subject` and a `reason`:

```json
{
  "command": "constraints",
  "constraints": [
    {
      "counterexamples": [],
      "declaration": {…},
      "declarationErrors": [],
      "evidence": "proven",
      "id": "first-reachable",
      "kind": "reachable",
      "reason": "unanchored: available from the start of play",
      "related": [],
      "root": ".",
      "scope": null,
      "subject": "quest:first",
      "verdict": "holds",
      "witnesses": []
    }
  ],
  "diagnostics": [],
  "ok": true,
  "schemaVersion": "0.49.0.constraints"
}
```

- `reason` is set on every verdict: for `holds`, the evidence (e.g. `connectivity proves reachable`); for `unknown`, why nothing was decided (e.g. `no play or test script was run (--all-scripts / --script)`, ``2 script(s) ran; none completed quest `first` ``, or the reachability reason). The text output prints it as a `  reason: …` line under `unknown`.
- `counterexamples` and `related` (`E-CONSTRAINT-VIOLATED`) are non-empty only on `violated`.
- `reachable` and `scenario reach` use one reachability claim; an unanchored quest is `reachable holds [proven]` (see [Reachability](/connectivity/reachability/)).
- `E-CONSTRAINT-DECL` entries are in `diagnostics`, not in `constraints`.
- A root whose `lute.project.yaml` exists but does not load prints the manifest diagnostics whole-project `check` prints and exits `1`; JSON has `ok: false`, `constraints: []` and the diagnostics (with `path` = the manifest). A declared constraint is never omitted without a diagnostic.

## Evidence levels

`proven` is a sound static proof; `witnessed` comes from a supplied run or
trace; `bounded` is sound only within its declared finite scope; `heuristic`
is conservative guidance; `unknown` means the analysis did not decide.
Diagnostics may carry optional JSON `evidence` and `scope` fields (bounded
diagnostics must include a non-empty scope). `trace`, `play`, `test`,
`calendar`, `scenario`, and `beats` add `evidence` to each verdict/result;
calendar aggregate results also carry `scope`. Parse, type, and declaration
errors omit evidence.

The impact and constraints sections were introduced in the historical 0.34.0
contract; the repository's current release is 0.51.0 and the AI edit-loop
commands are documented below.

## compile

```console
$ lute compile [<FILE>] [--project <DIR>] [--providers <DIR>] [--permission-profile <NAME>]
               [-o, --output <PATH>] [--locale-bundle <FILE>]
               [--deny <CODE>]… [--deny-warnings] [--json] [--stream]
```

Compile a document to its JSON command-record artifact (gated on a clean check and a compiler-side recheck of the effective permission policy). Exit **0** on success, **1** on a failed gate, **2** on I/O or serialization failure. The artifact is always JSON; `-o`/`--output` writes it to a file instead of stdout. The gate is the target's reconciled whole-project `check` verdict in its project — `--project`, else the nearest `lute.project.yaml` above the file (discovered exactly as if passed, with the same stderr note as `check`); with no manifest above the file, the gate is that file's own `lute check` verdict. A `--permission-profile` ceiling is checked again immediately before lowering, so a check result created under another policy cannot authorize forbidden IR.

`lute compile` takes three forms: `compile <FILE>` (one document, above), `compile` with no document (the whole project, next), and `compile --stream <FILE>` ([below](#--stream--compile-a-scene-from-a-live-body)). `lute schema compile` lists the outputs `artifact` (no flag), `report` (`--json`, `0.43.0.compile`) and `stream` (`--stream`).

### Project compile and index

`lute compile` with **no document operand** compiles **every** `*.lute` document of the project (`--project <DIR>`, else discovered from the current directory) into `-o <DIR>`, mirroring the project's own layout (`quests/a.lute` → `<outdir>/quests/a.lute.json`), and writes a `<outdir>/project.index.json`. Here `-o` is an output **directory**, not a file; it is created if absent. `*.component.lute` fragments are skipped — a component is inlined into its importers and has no artifact of its own. When `--permission-profile` is present, its ceiling applies independently to every source-selected profile; it never activates the named profile's plugins.

`-o` is **required** in this form: without it the command is `E-CLI-USAGE` (`argument: "--output"`, there is no single artifact to write to stdout), and with no `--project` and no ancestor manifest it is `E-CLI-USAGE` naming `--project` ([Project and discovery](#project-and-discovery)). Both exit **2** without reading a document.

The index carries the document table plus the **union** of every artifact's `entities`, `enums`, `relations`, `seedFacts`, `rules`, and `prereqEdges` — the union [an engine must compute anyway](/tooling/runtime-contract/) before it can evaluate anything:

```json
{
  "irVersion": "0.10.0",
  "capabilitySnapshot": "529a74383209e1736d16b44daa4c140501893cd74051ec6b53ad0c8ebfe457ae",
  "requiredSemantics": [
    "lute.core/1",
    "lute.quest.lifecycle/1"
  ],
  "documents": [
    {
      "path": "quests/findKai.lute",
      "artifact": "quests/findKai.lute.json",
      "kind": "quest",
      "key": "findKai"
    },
    {
      "path": "scenes/opening.lute",
      "artifact": "scenes/opening.lute.json",
      "kind": "scene",
      "key": "opening"
    }
  ],
  "entities": [],
  "enums": [],
  "relations": [],
  "seedFacts": [],
  "rules": [],
  "prereqEdges": []
}
```

`path` is the source, relative to the project root; `artifact` is its compiled output, relative to the output directory; `key` is the document's canonical node id — a scene's `id:`, or a quest document's **first** declared `<quest id>` (a quest pack's remaining ids stay recoverable from its own artifact's `quest` records). All paths are forward-slash relative, never absolute, so an index survives being copied between machines or packed into a game archive.

`documents` is sorted by `path` and every vocabulary array is deduplicated and totally ordered, so the index is byte-stable across runs. Unlike an artifact, which omits an empty vocabulary array, the index always emits all six — an engine unions them unconditionally, and an absent key would force it to distinguish "no relations" from "index too old to carry them".

### Project compile is all-or-nothing

Every document is compiled in memory and the index is built before anything touches the filesystem. Three things stop the write, each exiting **1** with nothing emitted:

- **A failed ordinary or permission gate.** One document's diagnostics print, followed by `N of M document(s) failed; no output written`. A host permission denial never leaves a denied artifact or partial index behind.
- **A `--deny`-promoted warning** (`--deny W-L10N-MISSING`, `--deny-warnings`): `--deny promoted N diagnostic(s); no output written`.
- **A vocabulary conflict.** Two documents declaring the same entity kind / enum / relation / prerequisite node with **different** signatures, or resolving different capability snapshots — never a silent pick. Whole-project `check` reports the snapshot case as `E-CAPABILITY-MISMATCH`. A signature conflict is the one class it cannot see, because it validates each document against its own resolved vocabulary and never unions across independent documents:

```console
$ lute check --project .
ok: . (2 file(s), 0 project-wide warning(s))
$ lute compile --project . -o out
lute compile: relation `knows` is declared with conflicting signatures by two documents (`scenes/a.lute` and `scenes/b.lute`)
lute compile: 1 vocabulary conflict(s); no output written
```

An `E-`-severity capability-resolution diagnostic (a bad plugin option, identity template, permission shape, or host permission-profile lookup) also exits **1** — see [the AI harness guide](/tooling/ai-harness/#capability-resolution-errors-gate-the-exit-code).

The project compile writes, but never prunes: an artifact whose source document was deleted stays in the output directory. Build into a directory you own and clear.

### `--locale-bundle` — merge a translation bundle

`--locale-bundle <bundle.json>` merges a locale bundle (see [`loc import`](#loc-import)) into the artifact: a `texts` map on every line record and on every choice/hub option, keyed by `lineId`, a `promptTexts` map on a branch or hub with `prompt=`, keyed by its `promptLineId`, plus `localeSegments` on a line that uses inline modifiers. The source-language `text` is never overwritten, and the maps are omitted when empty — so a document compiled without `--locale-bundle` is byte-identical to before. A bundle entry matching nothing in this document is ignored; a bundle legitimately spans a whole project. It composes with the project compile, merging the one bundle into every artifact. A translation whose inline modifiers (name, form, and attributes, as a multiset) differ from the source line's is not merged: `E-L10N-MODIFIERS`, and no artifact is emitted.

A translatable record missing a locale the bundle declares is `W-L10N-MISSING`, one per `(lineId, locale)` pair, written to stderr. It is a warning: the artifact still emits, carrying the source-language string. `--deny W-L10N-MISSING` (or `--deny-warnings`) promotes it, so CI can require a complete translation before anything ships:

<!-- lute-diagnostics -->
```console
$ lute compile scenes/opening.lute --project . --locale-bundle bundle.json --deny W-L10N-MISSING -o out.json
scenes/opening.lute:1:1: error [W-L10N-MISSING] [denied] no `ja-JP` text for `narrator.s01ep01.narrator_0020`
--deny promoted 1 diagnostic(s); no artifact emitted
```

A merged translation whose stored `sourceHash` (see [`loc import`](#loc-import)) differs from the hash of the record's current source text is `W-L10N-STALE`, one per `(lineId, locale)` pair: the line, option label or prompt changed after the export the translator worked from. The message names the lineId and the locale and says to re-export with `lute loc export` and update the translation. The translation is still merged. Like every warning it is promoted by `--deny W-L10N-STALE` or `--deny-warnings`, in a single-document compile and in the project compile. A `schemaVersion: 1` bundle carries no hashes and gets no stale check.

Since 0.51 the bundle also reaches a directive's [localizable text](/language/dialogue-and-cast/#localizable-directive-text-since-051): a bundle entry keyed by the `lineId` of a `text` attribute is merged into `localized.<attr>.texts` of its `plugin` record, as a line's translation goes into `texts`. `W-L10N-MISSING` and `W-L10N-STALE` apply to these entries with the same wording and the same `--deny` promotion, one per `(lineId, locale)` pair. `E-L10N-MODIFIERS` does not apply to them, because a `text` value has no inline modifiers. A document compiled without `--locale-bundle`, or without `text` attributes, is byte-identical to before.

### `--stream` — compile a scene from a live body

```console
$ lute compile --stream <FILE> [--project <DIR>] [--providers <DIR>]
                [--permission-profile <NAME>]
```

Resolve and check a complete scene template once — against `--project`, else the nearest
`lute.project.yaml` above it (0.27.0) — then read append-only ordinary
Lute section-body text from stdin. Each complete accepted line/directive/block is
compiled through the existing cumulative whole-document pipeline and flushed to
stdout as NDJSON: `start` with the initial full artifact, one `update` with a
full artifact per unit, then `finish` on successful EOF. A rejection writes an
`error` record with diagnostics and no `finish`. There is no envelope: the stream keeps its own protocol.

`--stream` takes exactly one document and conflicts with `-o`, `--locale-bundle`, `--json`, `--deny` and `--deny-warnings`; each conflict, and `--stream` without a document, is `E-CLI-USAGE` naming the conflicting option.

`--permission-profile <NAME>` freezes one trusted additional ceiling with the
project snapshot. It applies to both the fixed template and every appended body
unit; there is no mid-stream switch. A forbidden unit terminates with its
`E-PERMISSION-*` diagnostic before any update containing the denied IR.

Exit **0** only after successful EOF finalization, **1** on a
syntax/semantic/permission/compile/streaming rejection, and **2** on usage, I/O,
broken stdout, or invalid UTF-8. Authored `::end` is an ordinary runtime command;
it does not replace EOF or close stdin. The command never modifies the template
or performs remote/runtime effects. See the
[streaming continuation compiler guide](/tooling/continuation-compiler/) for
the record schema, cumulative cost, admitted body surface, permission freezing,
and consumer cursor/state rules.

## expr

```console
$ lute expr <EXPR> [--project <DIR>] [--permission-profile <NAME>]
             [--deny <CODE>]… [--deny-warnings] [--json]
```

Lower a host condition to an IR `exprNode` against a project, so a game can evaluate it over a run state with the runtime's `evaluate` query. The runtime ships no CEL parser: a host lowers its conditions at build time. `--project` defaults to the project discovered upward from the current directory (else `E-CLI-USAGE`, exit **2**; see [Project and discovery](#project-and-discovery)).

The condition is checked as the `when:` of a scene at the project root that imports every schema file the project's documents import, under the project's plugin set, with the [project index `defs`](/tooling/runtime-contract/#projectindexjson-in-041) — parameterised defs included — as its def table: the vocabulary the bundle carries. Its expected type is `bool`. Diagnostics are the checker's own codes (`E-CEL-PARSE`, `E-CEL-PROFILE`, `E-CEL-TYPE`, `E-UNDECLARED`, `E-UNDECLARED-REF`, `E-REF-ARITY`, `E-FACT-DOMAIN`, …) at `<expr>:<line>:<column>`, positions in the expression text. A condition that checks but does not lower is `E-CEL-PROFILE`.

A project that does not resolve — a manifest or plugin error, or a schema import some document names that does not resolve (`E-USES-NOT-FOUND`) — is an error with no position (`<expr>: error [CODE] …`); the expression is not checked.

On success stdout carries one line, the `exprNode` as JSON, and warnings go to stderr; exit **0**. On an error the diagnostics go to stderr, stdout stays empty, and the exit is **1**. In the `seven-days` example game:

```console
$ lute expr "run.day >= 3"
{"op":">=","l":{"path":"run.day"},"r":{"int":3}}
```

<!-- lute-diagnostics -->
```console
$ lute expr "@at(3)"
<expr>:1:1: error [E-REF-ARITY] `@at` expects 2 argument(s) but got 1
```

`--json` prints exactly one enveloped object with `command: "expr"`,
`schemaVersion: "0.43.0.expr"`, and `ok`; the compiled-condition fields
(`cel`, optional `authored`, `expr`, and `diagnostics`) remain at the top
level. On an error, the report likewise keeps `ok: false` and its
`diagnostics`:

```console
$ lute expr "@at(3, 'night')" --json
{"command":"expr","schemaVersion":"0.43.0.expr","ok":true,"cel":"(run.day == 3 && run.slot == 'night')","authored":"@at(3, 'night')","expr":{"op":"&&","l":{"op":"==","l":{"path":"run.day"},"r":{"int":3}},"r":{"op":"==","l":{"path":"run.slot"},"r":{"string":"night"}}},"diagnostics":[]}
```

## trace

```console
$ lute trace <FILE> [--project <DIR>] [--providers <DIR>] [--permission-profile <NAME>]
             [--deny <CODE>]… [--deny-warnings]
             [--state <PATH>=<LITERAL>]… [--fact <ATOM>]… [--choice <BRANCH>=<CHOICE>[,<CHOICE>…]]…
             [--event <NAME>]… [--accepted-quest <QUEST>]… [--occasion <OCCASION>[@<TARGET>]]…
             [--mock <FILE>] [--entry <ID> | --beat <ID>] [--no-derive] [--expand] [--json]
```

Preview a document against author-supplied mocks (see the [tracing guide](/tooling/tracing/)). Without `--project`, the project is the nearest `lute.project.yaml` above the file, discovered exactly as if passed (as for `check`; 0.27.0 — before, trace resolved core-only and refused what a project's `defaults:` supplied). `--deny`/`--deny-warnings` promote the check gate's diagnostics (a refusal, exit **1**), and `--permission-profile` is threaded into the project the trace builds. Exit **0** complete, **1** refused (check errors or invalid mocks — the `E-TRACE-*` codes render like check diagnostics — or, dsl 0.25.0, a write or a seed that makes two [exclusive relations](/tooling/tracing/#exclusive-relations) hold together, `E-FACT-EXCLUSIVE`), **2** I/O, **3** incomplete (an `unknown` guard halted the walk). `--occasion <OCCASION>` (repeatable, dsl 0.21.0) raises an occasion after a quest walk settles: every active quest's same-named `<on event="O">` handlers run first, then the `<objective on="O">` objectives of every active quest are judged; the mock file's `occasions:` list does the same, and its `visited:` list seeds the scenes `visited('<id>')` reads as presented (unlisted scenes are not visited). A quest walk settles as `lute play` settles it: a grant whose reward kind declares `credits:` adds its scalar amount to that path (the grant line ends `(credits <path> = <value>)`, JSON `credited`), and an objective's body plays once when the objective turns done — see [Rewards and objective bodies](/tooling/tracing/#rewards-and-objective-bodies).

A mock can also start from a save (dsl 0.22.0): `quests: { <id>: unset | active | complete | failed }` seeds `quest.<id>.state`, and `entriesRead: { run: [ids], user: [ids] }` seeds the entry read flags — `run:` both `entry.<id>.read` and `entry.<id>.everRead`, `user:` `entry.<id>.everRead` alone. Each follows the mock rules of the reserved path it spells — the document must read that path, or (for an entry) declare the entry.

**Derivation is on by default** (dsl 0.22.0). Trace loads the project's seed `facts:` and applies its Datalog rules (stratified negation) over the mocked and asserted facts, so a rule-derived fact satisfies a guard, `done`, or `start` without mocking the conclusion, and a derived fact that is false because a negated premise holds can be traced. Mocking a derived atom still works — it is a seed like any other. `--no-derive`, or `derive: false` in the mock (the flag wins), restores the 0.21 model: seeds are not loaded, an unmocked derived atom is unknown, and a note names each derived relation read. Trace, `test`, `run`, and `play` share one Datalog evaluator, so they cannot disagree about what a project's rules conclude.

`--entry <ID>` presents **one** `<entry>` of a [lore document](/language/lore-entries/) (dsl 0.19.0) instead of walking a sequence: its lines, the `<match>` arm taken, and the `::set` / `::assert` / `::retract` a first read applies — or skips, when the mock seeds `entry.<id>.read: true`. A lore document has no sequence to walk, so tracing one without `--entry` is a usage error (exit **2**, naming the declared entry ids); `--entry` on a scene or quest, or naming an id the document does not declare, is `E-TRACE-ENTRY` (exit **1**). For an unknown id the message lists the document's entries and beats, and when the id is a beat's it says to present it with `--beat`.

`--beat <ID>` (dsl 0.23.0) presents **one** [bundle beat](/tooling/play/#bundle-beats) of a lore document, by its local id or its canonical `<document id>.<beat id>`: its body is walked like a scene's (`--choice` decides its branches), every effect applies, and its `when` is noted, not enforced. A lore document takes `--entry` or `--beat`, not both; naming no beat of the document, or a document with no `<beat>`, is `E-TRACE-BEAT` (exit **1**). An occasion can be raised **for a target** as `--occasion <OCCASION>@<target>` (or `occasions: [talk@npc.oskar]` in the mock), which also judges the `<objective on="<name>" target="<target>">` objectives; a raise without the target leaves those alone. See [Bundle beats](/tooling/tracing/#bundle-beats) and [Targets, deadlines, and the previous run](/tooling/tracing/#targets-deadlines-and-the-previous-run).

Since dsl 0.24.0: a `<match subject="@def">` subject, a guard, and the coverage summary print a def reference as authored (`<match @weekday>`, `-> old (@atLeast(3))`), and `--expand` prints the expansion instead (`--json` keeps the expansion and adds `authoredId` / `authoredGuard` / `authoredLabel`). A mock's `bridges:` answers plugin calls that read a bridge result; an unanswered one leaves its result slots unknown, so a guard over them halts the walk with a `bridges:` hint. `--accepted-quest` takes an `activate="accept"` child quest, a `complete="any"` parent's untaken alternative reads `superseded from quest.<parent>`, a raise `E@target` runs the `<on event="E" target="…">` handlers for that target, and with a project trace settles whether a read quest exists. See [Tracing](/tooling/tracing/).

Since dsl 0.26.0: a taken `::jump` is followed to its `::label` (`<jump -> hall>`, JSON `{"kind": "jump", "to": "hall"}`), so `trace complete` means the walk reached the end of the document; an `::accept` in a quest `<on>` handler activates a quest of the same document, as in play; with a project `--accepted-quest` / `accepts:` resolve any quest of the project, and a quest document may seed its own `quest.<id>.*`; `--entry` also takes `<document id>.<entry id>`; and a [kind beat](/tooling/play/#kind-targets) reads its member from `--state occasion.target=<member>`. See [Following a taken `::jump`](/tooling/tracing/#following-a-taken-jump).

## scenario

```console
$ lute scenario [--project <DIR>] [--providers <DIR>] [--format text|dot] [--json] [--facts]
              [reach [--node <NODE> | --endings[=<OCCASION>]] | envelope --node <NODE> | knowledge [--node <NODE>]]
```

Read-only reporting over the connectivity layer. With no subcommand, prints the assembled node/edge graph. `reach --node <NODE>` reports a node's [reachability verdict](/connectivity/reachability/); `envelope --node <NODE>` (or `envelope --node quest:<id>`) prints the [Guaranteed/Possible tables](/connectivity/envelopes/) plus the [guaranteed facts](/connectivity/envelopes/#guaranteed-facts) at the node's entry (dsl 0.20.0; `envelope.guaranteedFacts` in `--json`, each `{fact, establishedBy}`). `<NODE>` is a scene's canonical key, a quest id, or a [bundle beat](/tooling/play/#bundle-beats)'s canonical `<document id>.<beat id>`. A bare id is looked up among all three; when it names more than one kind, the command refuses and asks for an explicit `scene:`, `quest:` or `beat:` prefix, which always selects that kind. Exit **0** on success, **2** on I/O or an unresolvable or ambiguous node id.

The envelope's `written by:` list names the writers that are provably before the node; a writer
reached only through an alternative edge is listed after it, as `also written, but not provably
before this node:`. Since 0.52 (#209) that covers `[accept]` edges as well as `[raises]` edges: of
several scenes that each `::accept` a quest, any one may be the one that accepted it, so none is
provably before it (see [envelopes](/connectivity/envelopes/#writers-on-alternative-edges)).

`--format` (`text` or `dot`) and `--json` select the output shape of the bare graph view; `--json` replaces 0.47's `--format json` with the same report and version (`0.43.0.scenario*`), and `--json` together with `--format` is `E-CLI-USAGE`:

- `text` (default) — the topological layers, then one line per edge with the [atom kind(s)](/connectivity/scene-graph/#edge-kinds) that justify it in brackets (or the anchor kind: `[accept]`, `[subquest]`, `[start]`), then — when any exist — the unanchored nodes under ``unanchored (no `after` / `follows` — available from the start of play; no prerequisites in this graph):``, one per line: each quest on no edge as `quest(<id>)`, and (dsl 0.25.0 §3) each scene or bundle beat without `after` whose `when` has a `visited()` conjunct, with the `after` to write (``beat(keeper.warning) — its `when` reads visited('keeper.greeting'), which gates it but draws no edge; write `after="visited('keeper.greeting')"` to anchor it``).
- `--json` — `{"roots":[{"root":…,"layers":[[…]],"nodes":[…],"edges":[…]}]}`. Each node is `{id, kind, prereq, reach}` (`kind` is `scene`, `quest`, `beat`, or `entry` — the source of a quest's `[start]` anchor, dsl 0.25.0 §4; `prereq` is the raw declared formula, `null` for an entry node); each edge is `{from, to, kinds}`, where `kinds` is an array because one formula may reference the same node under more than one atom. A root with unanchored nodes also carries `"unanchored": ["quest(<id>)", …]` and, for the scene and beat hints, `"unanchoredHints": { "<node>": "<hint>" }` (each omitted when empty).
- `dot` — one Graphviz `digraph` per root; scenes are boxes, quests ellipses, bundle beats `shape=note`, entries `shape=tab`, and an `active`-only edge is drawn `[style=dashed]`. An unanchored quest is a dashed blue ellipse labelled `quest(<id>) (unanchored)`.

**Fact edges** (dsl 0.26.0 §8). Progress gated by facts — a badge, a key item — draws no `after:` edge, so every gym sits in layer 0. `--facts` adds an edge `producer -> reader [fact]` wherever a scene, beat or quest's gate (`when:` / `start=`) reads `holds(F)` and the producer asserts `F`, or — through the rules — a fact a rule deriving `F` needs, written `[hasItem(goodRod), via canPass(pier)]`; the layers are then drawn over those edges too, wherever one closes no cycle. Text lists them under `fact edges (producer -> reader) [asserted fact]:` and heads the layers `topological layers (after: and fact edges):`; `--json` adds `factEdges`, each `{from, to, fact, via?, layered}`; `--format dot` draws them dotted, labelled with the fact. See [Story overviews](/tooling/overviews/#fact-edges-lute-scenario---facts).

**Endings** (0.27.0). The command exits 1 when a static verdict refutes an ending (0 otherwise, `unknown` included), so CI can gate on it, and a never-holds row cites the error inside the `when` it follows from (`caused by E-WHEN-LITERAL-DOMAIN at 7:99: …`, at the quoted literal; JSON `causes`, each `{code, line, column, message}`, the column counted in characters). `--format` and `--json` may come before or after `reach`. `reach --endings=<occasion>` lists every beat answering that occasion; bare `reach --endings` lists every beat — scene beat, bundle beat, entry — whose content can run `::end` (its own body or a component it `::use`s). One row per ending: its id and file, the `after:` verdict `reach --node <NODE>` gives, the `when` verdict whole-project `check` reaches (`E-BEAT-UNREACHABLE` / `E-ENTRY-UNREACHABLE`: never holds; `W-BEAT-SHADOWED`: never wins), and for a `when` nothing refutes, the state paths and fact atoms it reads with who produces them — the documents that write the path (their own `::set` or `<choice into>`, a `::use`d component's, a plugin directive's declared effects, a reward's `credits:`), the engine for an `owner: engine` path, or `nothing writes it` for a content path no write reaches; a fact's producers read as in `knowledge`. A summary line counts `N ending(s): A reachable, B unreachable, C unknown`. "Reachable" means nothing static refutes it; whether a written path can reach the value the `when` needs is not analysed, so a play presenting the ending is the proof, and `lute test --coverage` lists the beats no play presents. It takes no node (`reach --node <NODE> --endings` is a usage error, exit 2, as is an occasion no beat answers) and exits 0; `--json` gives `{roots: [{root, occasion, endings: [{id, kind, file, verdict, after, when, needs}], summary}]}`.

**Bundle beats are nodes** (dsl 0.23.0 §4). Every `<beat>` of a lore document is drawn as `beat(<document id>.<beat id>)` — an entry node in layer 0 with no edges when it declares no `after=`, selected by its occasion, target and `when` instead, and (dsl 0.25.0 §3) the dependent of the edges its `after=` draws when it does. `reach --node <beat id>` answers `Reachable`, then an `after:` line (`(none declared) — an entry node: …`, or the beat's `after=` formula), the file that declares it (`declared in:`), and its `on:`, `target:` and `when:` as written; `envelope --node <beat id>` prints the entry-floor tables, with the seed facts as its guaranteed facts. Since dsl 0.24.0 §2 an `after:` may name a bundle beat (`visited('<doc>.<beat>')`), and connectivity routes through it like a scene. See [The scene graph](/connectivity/scene-graph/#bundle-beats).

An unanchored quest (dsl 0.21.0 §7a.5) sits in no layer and on no edge, but it is not missing from the report: `reach --node quest:<id>` gives it the verdict ``Unanchored — a quest with no declared `follows` edge: available from the start of play; …`` (JSON `"reach": "unanchored"`), and its `after:` line reads `(none declared) — unanchored: this quest is in no prerequisite graph layer and on no edge; it is available from the start of play.` A quest anchored without `follows=` — by the `::accept`s that take it up, its parent (a subquest), or its `start` conjuncts (dsl 0.25.0 §4) — lists its anchors instead, `[start] entry(keeperLog)` one per line (`anchors` in JSON, each `{ kind, from }`).

Since 0.23.0 the graph view also ends with a `note:` listing the `completed()`/`active()`/`visited()` references it did not draw — a quest's edges come from its `follows`, its subquest tree, its `start` conjuncts and its `::accept`s, so a `completed()` naming a quest on no edge, and a quest's `visited()` read outside its anchoring `start` conjuncts, are listed (`omitted` in `--json`); see [What the scenario graph leaves out](/tooling/overviews/#what-the-scenario-graph-leaves-out). `knowledge [--node <NODE>]` traces every fact-guarded condition through the rules to the producers of its facts — asserting documents, seed facts, `reserved`, or `NO PRODUCER`. Since dsl 0.24.0 it covers every guard slot — beat, entry and objective guards, line `when=`, `<choice when>`, `<when>` arm tests, `::jump`/`::set` `when`, `<on when>`, reward `when`, quest `start`/`fail` and objective `until` — grouped by document with each guard's source line; a negation reads "holds unless defeated" with one `defeated when …` line per defeater — a derived defeater with every derivation route, `⇐ … / ⇐ …` (dsl 0.25.0 §9) — or "always holds (…) — cannot be defeated", and a derived atom's rules print once (later mentions say `traced above under …`). `knowledge` takes `--json` (before the subcommand; JSON elements carry `for` and `line`); `--node` takes a scene (every guard in it), a bundle beat, an entry id, `<quest>.<objective>`, `quest:<id>`, or `<scene>#<branch>.<choice>` for one choice, and an unmatched one is exit **2** with a did-you-mean. A rule's premises are traced too: an entity-kind atom reads as membership (`person(ada) — entity kind `person`; ada is a member`, not an undeclared relation), and a `cel(…)` premise names the state it reads (`cel("run.slot == 'evening'") — state condition on run.slot, decided at run time`); in `--json` a rule's premises are `{ relation, negated? }`, `{ entityKind }`, or `{ cel }`. `envelope`'s Facts rows use the same producer wording (`seed facts …`, `reserved — the engine asserts it`, `derived by 1 rule`). Both are described, with real output, in [Story overviews](/tooling/overviews/#lute-scenario-knowledge).

Since dsl 0.26.0 §6 a rule body may count (`canPass(earthGymDoor) :- count(hasBadge(_)) >= 7`), and `knowledge` traces such a premise as `count(hasBadge(_)) >= 7 — counts:` with the producers of the counted facts beneath it.

## beats

```console
$ lute beats [--project <DIR>] [--occasion <OCCASION>]… [--target <TARGET>]… [--expand] [--json]
```

Print every beat of the project as one ladder per occasion — and per target of a targeted occasion — in selection order (dsl 0.23.0): priority, beat and title, kind (`scene`, `entry`, `bundle`), `once` (`run`, `user`, `day`, `slot`, or `no` — a bundle beat's `once="day"` / `once="slot"` included — and `also`, and since dsl 0.25.0 a `share` key: `day, share solWarm`; `share` in `--json`), the whole-project `check` verdicts (`unreachable`, `shadowed`, `tied`, `once-run-user`), `after:` (a bundle beat's `after=` included), and `when` as the author wrote it — `@def` references included (dsl 0.24.0); `--expand` prints each `when` with its defs expanded, and `--json` carries both, `when` (expanded) and `whenAuthored`. `--occasion` and `--target` (both repeatable) filter the ladders; `--json` carries each verdict's full diagnostic. Read-only, and the project need not check clean. Exit **0** on success, **2** on I/O, a missing project or an unknown `--occasion`. See [Story overviews](/tooling/overviews/#lute-beats).

Since dsl 0.26.0: a fallback that an earlier, never-spent beat whose `when` it implies always beats shows `covered by <id>` in the verdict column (`coveredBy` in `--json`) — informational, not a diagnostic, so a lead can tell a fallback that still plays from one that no longer can; a [kind beat](/tooling/play/#kind-targets) is listed in the ladder of every member some beat names, and in a `kind:<kind>` ladder for the members no beat names on its own; and `--target` accepts any member of a targeted occasion's domain.

Since dsl 0.28.0: a [kind beat](/tooling/play/#kind-targets) or a `for=` beat whose `when` never holds for some member — as written (`occasion.target != 'village'`), or because nothing produces the fact it needs for that member — says `never for <member>` in its verdict column: a kind beat on that member's ladder, a `for=` beat on its occasion's ladder (`neverFor` in `--json`). whole-project `check` reports nothing, because the beat still plays for the other members.

## calendar

```console
$ lute calendar [--project <DIR>] [--providers <DIR>] [--permission-profile <NAME>]
                [--deny <CODE>]… [--deny-warnings]
                [--axis <AXIS>=<VALUES>]…
                [--occasion <OCCASION>]… [--occasion-axes <OCCASION>=<AXES>]… [--target <TARGET>]…
                [--relation <RELATION>]… [--script <FILE> [--until <STEP>]] [--where <EXPR>]
                [--format text|csv] [--json]
```

Evaluate `lute play`'s own eligibility at every cell of a grid of state values (dsl 0.23.0). For every cell and every occasion/target column, the grid shows the winner and `+N` shadowed eligible beats, the offered or sequence list, `-` when nothing is eligible, or `?` when an undecidable `when` decides the cell; the beats never eligible in any cell are listed at the end with the reason, and — since dsl 0.24.0 — so are the beats eligible in some cell but presented in none, with what was presented over them (`neverPresented`, each with `beatenBy`, in `--json`; a second table in `--format csv`). Each cell starts from the same world, gets its axis values written, and has its quests settled before its occasions are evaluated.

- `--axis <path>=<values>` (optional, repeatable) — an inclusive integer range (`run.day=1..7`) or a list (`run.slot=morning,evening`) over a declared path, checked against its type; the first axis varies slowest, and with no `--axis` the grid is one cell. `quest.<id>.state=unset,active,…` seeds the quest's status as a save's `quests:` does (`unset` and `active` also clear the objective progress a replayed route made), and `quest.<id>.objectives.<oid>.done` is accepted as written; any other `quest.*` path is a usage error. `holds(<fact>)=true,false` asserts or retracts a base fact of a declared relation; a derived relation is refused (`` `trusted(ada)` is derived by rules and cannot be asserted ``). `visited('<id>')=true,false` (dsl 0.24.0) puts a scene or bundle beat in or out of the visited set, so an `after: visited(…)` beat reads both ways. `clock[=<d1>..<d2>]` (dsl 0.24.0 §1) expands to every slot of those days in clock order (bare `clock`: one week). Anything else is a usage error that lists the axis kinds, with a did-you-mean for a mistyped state path.
- `--axis <family>.*=<values>` / `--axis '<family>[<axis>]=<values>'` (since 0.27.0) — one axis over a whole [`per:` family](/state/state-model/#one-path-per-entity-per): `run.aff.*=6,7` gives every member (`run.aff.ren`, `run.aff.kai`, …) the cell's value; `run.aff[run.route]=6,7` gives it only to the member the `--axis run.route` value names at that cell, and the other members keep their seed or default — route × own affection in one axis pair. The indexing axis must be an `--axis` of the same calendar naming at least one member of the family's kind (at a value naming none, the tied axis sets nothing and one cell stands for all its values); the bare family (`--axis run.aff=6,7`) is a usage error that names the three forms (`` `run.aff` is a `per: suitor` family — name a member (`run.aff.ren`), all of them (`run.aff.*`), or tie it to an axis (`run.aff[run.route]`) ``), and so is a second axis over a member the family axis already sets. Row labels, `--json` `at` keys and `--format csv` headers name the axis as written.
- `--occasion <OCCASION>` (repeatable) — the occasions to evaluate; default every occasion a beat answers.
- `--occasion-axes <OCCASION>=<AXES>` (repeatable; 0.47's `--occasion <O>@<AXES>`, dsl 0.24.0) — selects `O` and varies only the named axes for it: it is evaluated where every other axis is at its first value (`--occasion-axes dayEnd=run.day,run.slot=night` holds `run.slot` at `night` instead) and left blank elsewhere, so a once-a-day occasion reads once per day (`--json`: the column's `varies` and `heldAt`). Naming the same occasion in `--occasion` and `--occasion-axes`, or twice in `--occasion-axes`, is `E-CLI-USAGE`. With `--axis clock`, name the clock's axes — `--occasion-axes dayEnd=clock.day,clock.slot=night`, or the clock's own paths, `--occasion-axes dayEnd=run.day,run.slot=night` — to evaluate `O` once per day at one slot (the day's first slot when none is named; the day's last for the clock's `raise.dayEnd` occasion, where the clock raises it).
- With `--axis clock`, an occasion the clock's `raise:` names is judged only where `lute play`'s `advance:` raises it: the `slot` occasion at every position but the one the run starts at, `dayStart` at a day's first slot but never on the day the run starts, `dayEnd` at a day's last slot (and at the last position of a clock that ends); with the clock's `raiseAtStart: true`, the `slot` occasion and `dayStart` are raised where the run starts too. Every other cell reads `not raised`, with no winner, eligible list or shadowed beats (`--json`: `"notRaised": true` on the result, and a root `notRaised` list of `{ occasion, reason }`; `--format csv`: `not raised` in `notes`), and a header line says where the clock raises it. A beat eligible only in such cells is never eligible, with `` eligible at day 1 (Mon) morning, where the clock does not raise `dayStart` ``. The run starts where the save puts the clock, before any `--script` step. A `dayEnd` column that names its slot (`--occasion-axes dayEnd=clock.day,clock.slot=afternoon`) is judged there: an `advance: day` closes the day wherever the clock stands.
- `--target <TARGET>` (repeatable) — the targets a targeted occasion is raised for. By default a targeted occasion gets one column per target its beats name, not one per member of its domain; when no beat names a target, a single `(any)` column only its untargeted beats answer.
- `--relation <RELATION>` (repeatable; 0.47's `--facts`, dsl 0.24.0) — print the relation's facts in every settled cell: a table with a row per first argument and a column per cell (`--relation at`: who is where, when); a per-cell `facts` map in `--json`, a `facts:<rel>` column in `--format csv`.
- `--script <FILE>` — a play script: every cell starts from its save, with its `steps:` replayed exactly as `lute play` plays them.
- `--until <STEP>` — with `--script`, replay only the steps before this one — a 1-based step number or a step `label:` (an unknown label gets a did-you-mean); the step itself is not played.
- `--where <EXPR>` — keep only the cells where this condition holds after the axes are written and the quests settle, to prune combinations of independent axes no run reaches. It is plain CEL — an `@def` call is not expanded there — and a cell where it evaluates unknown is a usage error.
- `--json` / `--format csv` — machine-readable output (`--format` is `text`, the default, or `csv`; `--json` together with `--format` is `E-CLI-USAGE`); `--json` carries `from` (where the cells start), `pruned` (cells `--where` dropped), and per cell `notes`.

More than 10,000 cells is refused, and a replay that halts is a usage error. Exit **0** on success, **1** when the project does not compile, **2** on I/O or a usage error. See [Story overviews](/tooling/overviews/#lute-calendar).

## context

```console
$ lute context <FILE> [--project <DIR>] [--providers <DIR>] [--permission-profile <NAME>]
                      [--node <NODE> | --at <FILE>:<LINE>:<COLUMN>] [--max-items <N>]
                      [--script <FILE>]… [--all-scripts] [--json]
```

Emit the project-resolved **authoring surface** an AI or human needs to write valid Lute against this file's project — directives, attrs, enums, asset kinds, providers, state schema, relational vocabulary, delivery flags, inline text modifiers, referenced reserved quest paths, effective permission layers, and `capabilitySnapshot`. A capability query, not validation — it emits regardless of document diagnostics. Without `--project`, the nearest `lute.project.yaml` above the file is the project (0.27.0), as for `check`. `--node`, `--at`, `--max-items`, `--script` and `--all-scripts` select the [edit-surface report](#lute-context-edit-surface-0366) below. With `--permission-profile`, JSON `permissions` is `{ "layers": [...] }`, `bridges` contains only allowed bridge capability objects, `rewardKinds` is the allowed name-keyed object (empty when rewards are denied), and `questsAllowed` is a boolean. `directives` excludes both directive-denied entries and bridge directives whose `service/operation` is denied. External read-only state remains visible. Text output describes a compile-time authoring restriction and explicitly does not claim runtime sandboxing. Exit **0** on success, **2** on I/O; project/profile resolution errors are surfaced rather than treated as unrestricted.

Since 0.22.0 the surface also carries `defs` (each named condition's `name`, `type`, `params`, and `body`), the language's built-in directives under `builtinDirectives` (`::set`, `::assert`, `::retract`, `::accept`, `::use`, `::body`, `::jump`, `::label`, `::end`, `::clear`, each with its `syntax` and `meaning`), and `ids` — every scene, quest, and lore entry id in the project (`{ scenes, quests, entries }`; with no project, the document's own). A relation reports its `tier` and whether it is `reserved`, a state path declared `owner: engine` says so, an occasion carries its `description` and its `target` (`false`, `true`, or a `{ prefix, entity }` domain — human: `talk (select: first, target: npc.<npc>)`), and the human outline prints each imported component's parameters with their types. See the [AI harness guide](/tooling/ai-harness/#prompt-context-lute-context---json).

`directives` lists each core and plugin directive with its `attrs` (`name`, `type`, `required`) and `semantics`; a domain-valued attribute's `type` is `domain:<name>` (human outline: `camera: focus: string, framing: domain:framing, move: domain:cameraMove, transition: domain:transition, …`, `music: playback: domain:musicPlayback, …`). `deliveryFlags` lists `{mono}`, `{os}`, and `{vo}` with their meanings — `mono` is only for the effective `pov` or a speaker in `monoSpeakers`. `textModifiers` is the inline modifier surface of a content line's text: the core `pause` and `speed`, the project's `textStyle` members (or a note that the domain is not declared), and how a modified line lowers. The core-only `--json` excerpt:

```json
{
  "capabilitySnapshot": "529a74383209e1736d16b44daa4c140501893cd74051ec6b53ad0c8ebfe457ae",
  "textModifiers": {
    "core": [
      {
        "form": "leaf",
        "meaning": "a pause of `s` seconds (required, non-negative) at that point of the line",
        "name": "pause",
        "syntax": ":pause{s=0.5}"
      },
      {
        "form": "span",
        "meaning": "the wrapped text is delivered at `rate` (required, positive); a nested speed uses the innermost rate",
        "name": "speed",
        "syntax": ":speed[text]{rate=1.25}"
      }
    ],
    "lowering": "the IR `text` is the plain derivation (markup removed, escapes decoded, `{{…}}` kept, one `placeholders` entry each); a modified line adds `segments`: text runs {text, styles?, rate?} and {pause} leaves; in `text`, segment runs and `texts` a literal `{{` is written `\\{{` and a literal backslash directly before `{{` is doubled"
  }
}
```

## tag

```console
$ lute tag [<PATH>]… [--renumber [--commit]] [--check] [--json]
```

Back-fill a stable `code` into every untagged `:line`, and a missing `instance` key into every component invocation, rewriting the files in place; a document already fully tagged is left byte-identical. With no `<PATH>` the target is `.`. Exit **0** on success, **1** when a document was refused, **2** on I/O.

- **Default (no `--renumber`)** back-fills missing codes and instance keys and writes the files.
- **`--renumber`** is the dangerous mode: it renumbers every line's `code` in clean document order instead (`0010`/`0020`/… per speaker per scope) — a drafting tool, refused for a document whose frontmatter declares `codesLocked:` (published codes are `lineId`/`voiceKey` identity; missing component instance keys are still added) or that has structural errors. **Without `--commit` it writes nothing** and prints what `--commit` would do — one line per changed code, `<path>:<line>: <old or (none)> -> <new>`, one line per file that would get instance keys, each refusal as `--commit` would print it — then `lute: would renumber <N> line(s) in <M> of <T> file(s); pass --commit to apply`. Exit **0**, or **1** when a file is refused (as `--commit` would). **`--renumber --commit`** applies it.
- **`--check`** writes nothing and exits **1** when the selected mode would change any file, else **0** (a refusal is still **1**). `--commit` without `--renumber`, or together with `--check`, is `E-CLI-USAGE`.

Since 0.51 `lute tag` also back-fills a `code` into every untagged directive that has a [`text` attribute](/language/dialogue-and-cast/#localizable-directive-text-since-051), in the order the compiler uses, so the directive's `lineId` (`{prefix}~{tag}~{code}~{attr}`) becomes reproducible from the source. `codesLocked:` applies to these codes as it does to line codes.

`--json` prints the `0.48.0.tag` report: `{mode: "tag"|"renumber", applied, files: [{path, changed, lines, instances, refused?, renumbered?: [{line, from, to}]}]}` — `applied` is `true` only when files were written, `lines` counts the codes added or renumbered, `instances` the instance keys added, `refused` carries the refusal message, and `renumbered` appears only in renumber mode, with `from` `null` for a line that had no code.

`<PATH>` may be a directory (dsl 0.22.0), and with no `<PATH>` it is `.`: every `.lute` file under it is rewritten, recursively, in the same sorted order whole-project `check` walks. Each changed file gets a line naming it, a file with nothing to do stays silent, and a summary closes the run; a refused or unreadable file is reported and the walk goes on, and the exit code is the worst outcome across the files:

```console
$ lute tag scenes
lute: scenes/town/day-end.lute: tagged 1 line(s)
lute: scenes/town/morning.lute: tagged 1 line(s)
lute: scenes/town/welcome.lute: tagged 1 line(s)
lute: scenes/talk/mara-first.lute: tagged 3 line(s)
lute: scenes/talk/mara-idle.lute: tagged 2 line(s)
lute: tagged 8 line(s) in 5 of 5 file(s)
```

## fix

```console
$ lute fix [<PATH>]… [--check] [--json]
```

Apply the mechanical, meaning-preserving migrations in place. Byte-exact and comment-preserving; writes back only when something changed, and running it again applies nothing. Each `<PATH>` (default `.`) is a `.lute` file, a project/schema YAML file, or a directory — every `.lute` and YAML file under it, recursively and in sorted order; several paths are migrated in the order given. Each changed file gets its own line, then a summary (`lute: applied N fix(es) in M of K file(s)`); a single file with nothing to change prints `lute: nothing to fix`. Exit **0** on success, **2** on I/O — the worst outcome across the files. `--check` writes nothing and exits **1** when any fix applies. `--json` prints the `0.48.0.fix` report, `{applied, files: [{path, changed, fixes}]}`, `fixes` being the count the text output prints.

The 0.37 surface rewrites, each of which `lute check` reports with the new spelling and a note that `lute fix` rewrites it:

| Before | After |
| ------ | ----- |
| `::auto{…}` | `::actor{…}` |
| `::cut{…}` | `::cg{…}` |
| `::next{to="x"}` | `::jump{to="x"}` |
| `::mark{id="x"}` | `::label{name="x"}` |
| `::music{action=…}` | `::music{playback=…}` |
| `::cg{action=…}` / `::video{action=…}` | `::cg{display=…}` / `::video{display=…}` |
| `<match on="…">` | `<match subject="…">` |
| `<choice label="…">` | `<choice text="…">` |
| `@speaker{id="x"}: …` | `::label{name="x"}` on the line before, and the line without `id=` |

A renamed domain is renamed in project, schema and plugin YAML too (0.41.0): a key directly under a top-level `enums:` or `entities:` block and the scalar of every `domain:` type reference — `musicAction` → `musicPlayback` (renamed in 0.37.0); each rename counts as one fix. A mapping that already declares the new name keeps the old key, and `lute fix` says so on stderr: `` lute fix: <file>:<line>: kept `musicAction:` — `musicPlayback:` is already declared ``. Until then, an `E-DOMAIN-UNKNOWN` for the new name ends with `` — this project declares `musicAction`, renamed `musicPlayback` in 0.37.0; run `lute fix` ``.

The YAML passes read the text as bytes, so Korean, an em dash or any other UTF-8 in a scalar, a quoted string or a comment is carried through byte for byte (0.41.1; 0.41.0 panicked on it).

An existing language stamp older than the toolchain is raised to it (0.41.1): `defaults: luteVersion` in `lute.project.yaml` and a `.lute` document's own frontmatter `luteVersion`, each one fix, with the quotes and comment around the value kept — so the `W-LUTE-VERSION-STALE` it answered goes away. A missing stamp is not added, and a newer or non-`MAJOR.MINOR.PATCH` stamp is left alone.

The `bindingsVersion` of a `player.bindings.yaml` that is an older `MAJOR.MINOR` than the toolchain line (the toolchain version's `MAJOR.MINOR`) is raised to that line (0.47), one fix, with the quotes and comment around the value kept — so the `W-BINDINGS-VERSION-STALE` it answered goes away. A newer or non-`MAJOR.MINOR` value (leading zeros included) and a file written as one flow mapping are left alone; see [the `bindingsVersion` rule](/player/bindings/#bindingsversion).

The `volume` scale is renamed in a project that still uses the old documented one (0.42): when the project's folded `volume` domain is exactly `[silent, down, normal, up, full]` — those members in that order, in every document that sees a `volume` domain — `down` becomes `quiet` and `up` becomes `loud` in the declaration, in every `volume=` directive value and in the keys of `player.bindings.yaml`'s `volumes:`, one fix per token. A project that chose its own levels (a subset, other names, another order) or whose documents disagree is not touched; a `.lute` file outside any project is judged by its own frontmatter.

An author key in snake case becomes lowerCamelCase only where the rewrite is unambiguous. Inline text modifiers are never invented; an existing one is kept byte for byte. The earlier migrations still apply: `:line[speaker]{…}: text` → `@speaker{…}: text`, leading `:` sigil → `@`, choice `as="…"` → `into="…"`, a literal-comparison `<when test="$ == 'gold'">` → `<when is="gold">` (`W-WHEN-TEST-LITERAL`), and 0.31 CEL fact-query and presence calls to their standard-CEL forms.

A 0.36 scene:

```lute expect="E-MATCH-NO-SUBJECT,E-RENAMED-ATTR,E-RENAMED-DIRECTIVE,E-RENAMED-TAG-ATTR"
---
kind: scene
id: pier
title: The pier
state:
  run.day: { type: int, default: 1 }
---
## Arrival
::cut{action="show" assetId="cg.pier"}
<branch id="ask">
  <choice id="stay" label="Stay">
    ::next{to="farewell"}
  </choice>
  <choice id="go" label="Leave">
    @mara: Fine.
  </choice>
</branch>
<match on="run.day">
  <when is="1">
    @narrator: The first morning.
  </when>
  <otherwise>
    @narrator: Another morning.
  </otherwise>
</match>
@mara{id="farewell"}: Safe travels.
```

```console
$ lute fix scenes/pier.lute lore
lute: scenes/pier.lute: applied 8 fix(es)
lute: lore/dock.lute: applied 8 fix(es)
lute: applied 16 fix(es) in 2 of 2 file(s)
$ lute fix scenes/pier.lute lore
lute: applied 0 fix(es) in 0 of 2 file(s)
```

The scene now checks clean:

```lute check
---
kind: scene
id: pier
title: The pier
state:
  run.day: { type: int, default: 1 }
---
## Arrival
::cg{display="show" assetId="cg.pier"}
<branch id="ask">
  <choice id="stay" text="Stay">
    ::jump{to="farewell"}
  </choice>
  <choice id="go" text="Leave">
    @mara: Fine.
  </choice>
</branch>
<match subject="run.day">
  <when is="1">
    @narrator: The first morning.
  </when>
  <otherwise>
    @narrator: Another morning.
  </otherwise>
</match>
::label{name="farewell"}
@mara: Safe travels.
```

`lute fix` never guesses intent. These are reported by `lute check` and migrated by hand:

- camera `zoom`, `moveX`, `moveY`, `shake`, `reset`, and `easing` (`E-CAMERA-REMOVED`) — declare `framing`, `cameraMove`, or `transition` members and write `framing=`, `move=`, or `transition=`;
- cg `full` (`E-CG-LAYOUT`) — declare a `cgLayout` member and write `layout=`;
- music `track` and sfx `name` (`E-REMOVED-ATTR`) — use `assetId` (and `sound` for a sound effect);
- a declared `musicAction` domain — rename it `musicPlayback` in every document, schema, and plugin that declares it (`E-DOMAIN-UNKNOWN` otherwise);
- a `{mono}` line whose speaker is neither the effective `pov` nor listed in `monoSpeakers` (`E-MONO-POV`, `E-MONO-NO-POV`) — set `pov:` or `monoSpeakers:` in the document or the project `defaults:`;
- project `sequence:` (`E-REMOVED-PROJECT-KEY`) and a `<scene>` tag (`E-REMOVED-TAG`);
- a body `# Title` heading (`E-INERT-TITLE`) — move it to frontmatter `title:`.

## catalog refresh

```console
$ lute catalog refresh [--project <DIR>] [--providers <DIR>] [--json]
```

Re-stamp every pinned provider snapshot in `--providers <DIR>` against the current `capabilitySnapshot` and clear its `stale` flag (see [providers & catalog](/tooling/providers-and-catalog/)). `--providers` defaults to the project's `catalogDir` (default `catalog/`); the project (`--project`, else discovered) supplies the stamp — the project's multi-plugin version. With neither a project nor `--providers` the command is `E-CLI-USAGE` naming `--providers`. `--json` prints the `0.48.0.catalog-refresh` report, `{providers, capabilitySnapshot, refreshed: [path], skipped: [{path, reason}]}`. Exit **0** on success, **2** on I/O.

## init

```console
$ lute init [<DIR>] [--template <NAME>] [--json]
```

Scaffold a new Lute project directory, ready for `lute check`. `<DIR>` defaults to `.`. The command refuses (exit **2**, nothing written) when any file it would write already exists, naming it — not only an existing `lute.project.yaml`. `--template` selects the starter content:

- `minimal` (default) — a core-only `lute.project.yaml`, a state schema, a vocabulary schema, a starter scene with an `id:`, a trace mock, and a passing scenario test in `tests/`.
- `investigation` — the worked whodunit.
  Since dsl 0.24.0 it is a small whodunit built on the `beats` skeleton: a `case.occasions` plugin raising `arrive`, the targeted `examine` (`item.<evidence>`) and `interview` (`npc.<suspect>`), and `accuse`; evidence lore that `::assert`s what is on record; derived rules with stratified negation (a suspect is `cleared` by an alibi unless evidence contradicts it; the `culprit` is implicated and not cleared); an accept-driven quest; an accusation whose choices are guarded by `holds('culprit', ['…'])`; `plays/the-case.play.yaml` with `expect:`, and a scenario test. whole-project `check`, `test` and `play` pass as scaffolded, and its README shares the `beats` commands.
- `beats` (dsl 0.22.0) — a game driven by [beats and occasions](/tooling/play/): a project-local occasions plugin (one occasion with a `{ prefix, entity }` target domain), a manifest whose `defaults:` supply `luteVersion` and `uses`, a `world.schema.yaml` with an `owner: engine` clock and shorthand `defs`, scene beats with `id:`, a quest, lore entry beats, a play script with an `engine:` step and `expect:`s, and scenario tests. Whole-project `check`, `test`, and `play` all pass as scaffolded.

None of the templates pins `identity:` — the default `voiceKey` already carries the scene prefix. The command prints every file it created and the next commands to run, in the 0.48 spelling (its generated README uses it too). `--json` prints the `0.48.0.init` report, `{dir, template, files: [path]}`. Exit **0** on success, **2** on I/O, an unknown template, or a refused overwrite.

## lore

```console
$ lute lore [--project <DIR>] [--json]
```

Print the project's **world-narrative map** (dsl 0.19.0): every [lore entry](/language/lore-entries/) under the project directory grouped by `target` and by `series` (in `order`), then, for every relation asserted anywhere in the project, each ground fact and who reveals it — lore entries, bundle beats, scenes/quests, or `both` (more than one of those) — with the entries, beats, and documents that do. Beats are listed under their target beside the entries, labelled `beat` and followed by their occasion: a [bundle beat](/tooling/play/#bundle-beats) under its canonical `<document id>.<beat id>`, a scene beat under its scene key.

```text
  item.rusty_key
    beat  lore.dock.talk  "The key"  lore/dock.lute  (on talk)
    rustyKey  [item]  lore/ship.lute
```

A fact a bundle beat asserts is credited to the beat (a `beats:` line), not to the entries; a scene beat's asserts stay its scene's. `--json` emits the same report as an object with `targets`, `series`, and `relations` arrays: a beat row carries `"kind": "beat"` and `on`, and each fact carries `entries`, `beats`, and `documents`, with `revealedBy` one of `entries`, `beats`, `scenes`, or `both`. Read-only; documents need not check clean. Exit **0** on success, **2** on I/O.

Since dsl 0.24.0 each entry and beat row is followed by its `when`, and the report ends with a **Derived** section: for every derived relation, each conclusion the rules can reach from what the project asserts, the rule instance behind it (`⇐` its premises, each with its source), the evidence it rests on, and the conditions it gates (`--json`: `derived`, each fact with `fact`, `from`, `evidence`, `gates`):

```text
Derived (what the rules can conclude from what the project asserts)
  trusted
    trusted(ada)
      ⇐ met(ada) [scene `inn.ada` (scenes/inn/ada.lute)], not rumor(ada)
      evidence: scene `inn.ada` (scenes/inn/ada.lute)
      gates: objective `ferry.word` (quests/ferry.lute), scene `inn.again` (scenes/inn/again.lute)
```

## refs

```console
$ lute refs [--project <DIR>] [--attr <DIRECTIVE>.<ATTR>]… [--reward <KIND>]… [--json]
```

Who uses which engine content id (dsl 0.26.0 §2.5): every value of a directive attribute (`--attr give.item`) or every `target=` of a reward kind (`--reward ITEM`), with the documents and lines using it, so a lead sees who gives what before merging several authors' work — the same rod handed out by three areas, a TM spelt two ways. Both flags repeat and combine; naming neither is a usage error (exit **2**). A value that reaches the attribute through a component — `::use{component="gift" item="goodRod"}` over a body `::give{item=@item}`, through nested `::use`s too — is listed at the `::use` line that binds it, ``(via component `gift`)``; a reward with no `target=` is listed as `(no target)`; an attribute or kind nothing uses reads `no uses`.

```console
$ lute refs --project . --attr give.item --reward ITEM
::give.item: 2 value(s)
  `goodRod` — 1 use(s) in 1 document(s)
    scenes/wren.lute:15 (via component `gift`)
  `potion` — 1 use(s) in 1 document(s)
    scenes/wren.lute:13
reward ITEM: 2 value(s)
  (no target) — 1 use(s) in 1 document(s)
    quests/fishing.lute:9
  `goodRod` — 1 use(s) in 1 document(s)
    quests/fishing.lute:8
```

`--json` emits `{ "queries": [ { "kind": "attr" | "reward", "name", "values": [ { "value", "uses": [ { "document", "line", "via"? } ] } ] } ] }`, with `value` `null` for `(no target)` and `via` the component name. The report lists, it does not validate: an attribute typed by an entity kind (`item: { type: { entity: bagItem } }`) and a reward kind's `target:` contract are what `check` judges. Read-only; documents need not check clean. Exit **0** on success, **2** on I/O or a usage failure. See the [multi-author guide](/guides/multi-author/).

## new

```console
$ lute new scene <NAME> [--project <DIR>] [--occasion <OCCASION>] [--target <TARGET>] [--json]
$ lute new quest <NAME> [--project <DIR>] [--start] [--json]
$ lute new lore <NAME> [--project <DIR>] [--json]
$ lute new schema <NAME> [--project <DIR>] [--json]
```

Scaffold one new document into an existing project. The first argument is the document kind (`scene`, `quest`, `lore`, or `schema` — each is a subcommand); `<NAME>` gives the document its `id:`, and the file is named after that id. The id keeps the case you type (`isles.harborNight` stays `isles.harborNight`); the words of a phrase and the parts of a `-` name join in camel case (`The Epilogue` → `theEpilogue`, `the-hunt` → `theHunt`), and a dotted name keeps its dots. A `/` in the name nests the file in a subfolder and, for a scene, dots the id by folder: `lute new scene talk/tomas-evening` writes `scenes/talk/tomasEvening.lute` with `id: talk.tomasEvening`. `lute new quest lamp.oil` writes `quests/lamp.oil.lute` with `id: lamp.oil`, and `lute new lore old-map` writes `lore/oldMap.lute` with `id: oldMap` and one `<entry>` attached to `item.oldMap` — no `quest.` or `lore.` prefix is added. Every document omits whatever the manifest's `defaults:` already supplies (such as `luteVersion` or `uses`). `lute new quest` scaffolds an **accept-driven** stub — no `start`, with a comment naming the `::accept{quest="…"}` that begins it; `--start` writes the auto-starting `start="true"` form instead. `--project` names the **project** (default: discovered upward from the current directory), and the document lands under the enclosing project's root, the directory holding its `lute.project.yaml`. `--json` prints the `0.48.0.new` report, `{kind, id, path}`. Outside any project, `lute new` says so on stderr and writes a self-contained document.

A `--project` inside a project but not at its root is refused (exit **2**, nothing written), with the spelling to use instead — it used to write to `<root>/scenes/<name>.lute` without a word:

```console
$ lute new scene tavi-shell --project scenes/talk
lute new: `--project scenes/talk` resolves to `/tmp/tp/town/scenes/talk`, which is inside the project at `/tmp/tp/town` (its lute.project.yaml) but not its root; `--project` names the project, not the destination folder — did you mean `lute new scene talk/tavi-shell`? (nothing was written)
```

Without `--project`, `lute new` discovers the project upward from the current directory, so it also works from a folder below the project root and the document still lands under the root: `cd scenes && lute new scene tavi-shell` writes `scenes/tavi-shell.lute` in the project that holds `lute.project.yaml`.

Scenes, quests and lore share one set of document ids, so a name whose id another document already declares is refused, and nothing is written:

```console
$ lute new quest town/welcome
lute new: `scenes/town/welcome.lute` already declares `id: town.welcome` — scenes, quests and lore share one set of document ids; pick another name; nothing was written
```

`--occasion <OCCASION>` makes the scene a [beat](/language/beats/) answering that occasion, and `--target <TARGET>` names what a targeted occasion is raised for, a `<prefix>.<member>` of its target domain. Both are checked against the project before anything is written — an occasion the project's plugins do not declare, or a target outside the occasion's domain, is exit **2** with a did-you-mean and no file:

```console
$ lute new scene explore/gate --occasion explore --target place.villag
lute new: target `place.villag` is outside occasion `explore`'s domain `place.<place>` (`place.lantern`, `place.gallery`, `place.landing`, `place.cottage`, `place.village`) — did you mean `place.village`?; nothing was written
```

A targeted occasion needs `--target`: a scene without `target:` would play for every target.

```console
$ lute new scene explore/gate --occasion explore
lute new: occasion `explore` is raised for a target, and a scene without `target:` would play for every one — pass `--target place.<member>`, the target this scene answers; nothing was written
```

`--occasion O` always writes `on: O` (0.29.0). When `O` is the occasion a chain of the manifest's [`chapters:`](/getting-started/connect-scenes/#the-chapters-chapters) answers, the output adds how to make the scene a chapter instead: list its id in that chain's `scenes:` and drop `on:`.

`--target` without `--occasion` is a usage error, and so is `--occasion` on anything but a scene. Outside a project `--occasion` is refused, since no occasion is declared. Exit **0** on success, **2** on I/O, an invalid kind, a refused `--occasion`/`--target`, an id another document declares, or an explicit `--project` below the project root.

## doctor

```console
$ lute doctor [--project <DIR>] [--strict] [--json]
```

Diagnose the local toolchain and project setup: the version axes, the project manifest, the content documents, play scripts (`*.play.yaml`) and scenario tests (`*.test.yaml`), provider snapshots, the active plugins, every declared occasion with the number of beats answering it, the declared vocabulary slots, and editor integration. `--project <DIR>` is the project directory to inspect (default: the project discovered from the current directory; with none found, doctor reports the missing manifest as before). The document scan walks the project directory. Provider snapshots are looked for in the manifest's `catalogDir:` (default `catalog/`), the directory `check` reads; with none there the line says `no pinned provider snapshots`. The editor check runs `lute-lsp --version` on the first `lute-lsp` on `PATH` and flags one that reports another version — or none, as a server older than 0.22.0 does — with how to reinstall it. It also compares the `lute-lsp` beside the running `lute` with the one on `PATH` (`lute-lsp beside lute`; `siblingLanguageServer` in `--json`): another build first on `PATH` — same version or not — fails the check, naming both binaries, with the fix: put the `lute` directory first on `PATH`, or point the editor's language server at the `lute-lsp` beside it.

```console
$ lute doctor --project .
  • toolchain version: 0.22.0
  • language version: 0.22.0
  • IR schema version: 0.22.0
  ✓ lute.project.yaml: found at ./lute.project.yaml
  ✓ content documents: 7 `.lute` file(s) under .
  • play scripts: 1 `*.play.yaml`
  • scenario tests: 2 `*.test.yaml`
  • provider snapshots: no pinned provider snapshots
  • active plugins: game.occasions 0.1.0
  • occasions (beats answering): 3 declared, 7 beat(s) — dayEnd (1), townVisit (2), talk (4)
  • vocabulary slots declared: emotion, action (exits: fadeOut/hide), anchor (default: center), mood, volume, musicPlayback, vfxType
  • VS Code extension: not detectable from the CLI
  ✗ lute-lsp on PATH: /usr/local/bin/lute-lsp reports no version (older than 0.22.0) — differs from lute 0.22.0
      → reinstall the language server from this toolchain (`cargo install --path crates/lute-lsp`) and restart the editor
```

An editor keeps running the `lute-lsp` it started, so reinstalling the toolchain does not reach an open editor. On macOS and Linux doctor therefore also lists every running `lute-lsp` server (`runningLanguageServers` in `--json`) and flags one whose binary was replaced after the server started, or whose binary reports another version, with the fix — restart the editor or its language server:

```console
  ✗ running lute-lsp: pid 12431 (/usr/local/bin/lute-lsp) started before its binary was replaced, so it runs an older build
      → restart the editor (or its language server) so it launches this toolchain's lute-lsp
```

`--json` emits the same checks as one object, `{ "dir", "checks": { <key>: { label, ok, detail, hint } } }` (`ok` is `null` for an informational line). By default doctor is a report, never a gate: exit **0** whatever the checks find. `--strict` (dsl 0.26.0) exits **1** when any check fails (`✗`) — a stale running `lute-lsp`, another build beside `lute`, a stale snapshot — so a harness or CI step can refuse to start on a broken setup. Exit **2** when `--project <DIR>` cannot be read. Since dsl 0.26.0 a replaced `lute-lsp` also says so itself in the editor — see [Editors](/tooling/editors/#a-server-older-than-its-binary).

## run

```console
$ lute run <FILE> [--engine <FILE>] [--mock <FILE>] [--occasion <OCCASION>[@<TARGET>]]…
              [--entry <ID> | --beat <ID>] [--conditions-output <FILE>] [--json]
```

Execute a **compiled artifact** (`lute compile` output) headlessly against a mock playthrough — the reference consumer of the [runtime contract](/tooling/runtime-contract/): command dispatch, CEL guards, the facts + Datalog fixpoint, hubs, and quest lifecycle. `--engine <FILE>` loads the engine's immutable `lute.engine.yaml`, checks the artifact's exact-major.minor IR line **first**, then compares every compiler-derived `requiredSemantics` id with the matrix's `supportedIds` **before playback**. A missing id refuses the run with exit **2** and `E-ENGINE-SEMANTICS`; no command, condition, asset, or bridge executes. Malformed matrices use `E-ENGINE-MATRIX`; an unsupported IR line uses `E-ENGINE-IR-VERSION`; unknown ids use `E-SEMANTICS-UNKNOWN`. Omit `--engine` to use the built-in `reference` matrix. `--mock` is a YAML playthrough (the same surfaces as `lute trace --mock`); `--conditions-output <FILE>` writes the conditions the run evaluated to `<FILE>`; `--json` emits the machine-readable transcript. Exit **0** on a complete run, **1** refused, **2** on I/O or negotiation failure, **3** incomplete.

Since dsl 0.24.0 the mock's `bridges: { <tag>: [ {<field>: value}, … ] }` answers plugin calls that read a bridge result, one answer per call of the tag in call order: the answered values are written to the result slots and the `plugin` record carries `"answered": [{"field", "value"}, …]` (human: `plugin check (bridge answered: passed=true, margin=3)`). A call with no answer keeps its record's `unresolvedEffects` and the walk goes on. A failing `quest` record carries `failedBy`, and a quest failure prints its reason (`quest X -> failed (by)`, `(until)`, `(fail)`, `(cascade)`, `(superseded)`).

For a quest artifact, `--occasion <OCCASION>` (repeatable, dsl 0.21.0) raises an occasion after the walk settles — after the mock's own `occasions:`, in CLI order — judging the `<objective on="O">` objectives of every active quest; each raise is an `{"kind": "occasion", "occasion": "O"}` record (human: `  occasion O`). A quest with no `start` is accept-driven here as in an engine: it stays `unset` until the mock's `accepts:` names it or an `accept` record runs. A scene's `::accept{quest="<id>"}` is an `{"kind": "accept", "position": "<position>", "quest": "<id>"}` record (human: `<position>  quest <id> accepted`); when the walk already knows the quest to be past `unset`, the record carries `"ignored": "already <state>"` and the line ends ` (already <state> — ignored)`. The mock's `visited:` list seeds the scenes `visited('<id>')` reads as presented.

`--entry <ID>` presents one `entry` record of a **lore artifact** (dsl 0.19.0; [engine contract](https://github.com/journeyWorker/lute/blob/main/docs/runtime/lore-entries.md)): the transcript reports whether it is a first read and whether its `when` holds, runs its body segment, applies first-read effects (or records them as skipped once `entry.<id>.read` is seeded `true`), and then sets `entry.<id>.read`. It is required for a lore artifact and refused on any other kind (both exit **2**).

`--beat <ID>` (dsl 0.23.0) presents one **bundle beat** of a lore artifact instead — the `beat` record named by its canonical `<document id>.<beat id>` (or the bare beat id when unambiguous) and its body segment, run like a scene with every effect applied. A lore artifact needs exactly one of `--entry` / `--beat`; an id the artifact does not declare, or `--beat` on another artifact kind, is exit **2** (`` lute run: `--beat oskar.hnut` names no beat in this artifact (declared: oskar.hunt, oskar.rumor) ``). `--occasion <OCCASION>@<target>` raises an occasion for a target, judging a quest's `<objective on="<name>" target="<target>">` objectives (human: `  occasion talk → npc.oskar`); a raise for another target, or none, does not judge them.

## play

```console
$ lute play --script <FILE> [--project <DIR>] [--providers <DIR>] [--permission-profile <NAME>]
            [--engine <FILE>] [--events | --json] [--no-derive] [--derivation <ATOM>]…
            [--ir] [--quiet] [--conditions-output <FILE>] [--deny <CODE>]… [--deny-warnings]
```

Play a story through a WHOLE project as a sequence of raised **occasions** (dsl 0.21.0) — the reference-runtime consumer of [beats and occasions](/tooling/play/). The project (`--project`, else discovered) is compiled once, in memory, with the same gate and declaration union the project compile uses (scene, quest, and lore documents). `--engine <FILE>` loads `lute.engine.yaml`, performs the exact-major.minor IR gate first, then compares the project-index `requiredSemantics` union with `supportedIds`. A missing id refuses with exit **2** and `E-ENGINE-SEMANTICS` before any step, command, condition, asset, or bridge runs; malformed matrices and unsupported IR lines use `E-ENGINE-MATRIX` / `E-ENGINE-IR-VERSION`. Omit it for the built-in `reference` matrix. The required `--script` is a `*.play.yaml` file with a closed key set. Its `steps:` each do one thing — raise an `occasion:` (with `target:`, `pick:`, and a step-local `choose:` that replaces the script's `choose:` key by key for that presentation), start a `newRun:`, write what the engine owns with `engine:` (`state:` literals or `{ add: <n> }`, `facts:`, `retract:`), fire a world `event:`, move a declared clock with `advance: slot | day | <n>` (dsl 0.24.0: writes the clock's paths, settles the quests, raises `dayEnd` / `dayStart` at each midnight it crosses and the `slot` occasion once where it stops, per the clock's `raise:`; it may carry the same moment's `engine:` writes, which land where the clock arrives — after any `dayEnd` / `dayStart` on the way, before the final settle and raise), or end the playthrough with `end: true` — and any step may carry `label:`, `repeat: <n>`, its own `bridges:` answers, and `expect: { winner, offered, notOffered, presented, options, quests, state, facts, notFacts }`; `- include: <file>` splices another file's steps in place. Beside `steps:`, the script takes the `lute trace --mock` grammars for `state:`, `facts:`, `choose:` and `bridges:`, a save to start from (`visited:`, `presented: { run, user }`, `quests:`, `entriesRead: { run, user }`), `derive:`, and a top-level `expect: { exit, quests, state, facts, notFacts, transcriptContains, transcriptLacks }` judging the end of the play (dsl 0.22.0). Each occasion step lists the occasion's candidate beats with their verdicts, presents the winner (or the step's `pick` on a `select: all` occasion; `pick: none`, or no `pick` when nothing is eligible, presents nothing) through `lute run`'s reference evaluator, and advances every quest lifecycle, so later `when` conditions and `after: completed(…)` see real progress. `--json` emits the same transcript as one object.

Since dsl 0.26.0: `advance: { to: <slot> }` / `advance: { to: { weekday, slot } }` moves the clock to the next such position (forward only, never zero steps), and a step `expect:` takes `clock: { weekday, slot, day }` (since 0.28, also `ended`); `engine: { accept: [quest ids] }` accepts an accept-driven quest mid-play (`quest <id> accepted (engine)`); an entry may be named `<document id>.<entry id>` in `pick:`, `entriesRead:` and the selection expectations; a beat targeting `kind:<kind>` answers every member of the kind; a guarded directive that did not run prints `skip ::give{item="potion"} — when: false`; a bridge answer is typed by its result slot or the capability's `result:`, and an untyped one is refused (exit **2**); and a project whose documents declare one state path with two types is refused (exit **1**). The project loads once per play — on Monster League a play load fell from about 12 s to about 1.5 s. See [Playing a story](/tooling/play/).
`--events` emits a resumable runtime event stream as compact JSON Lines: the first line contains the seed and its output, then each input the player takes is paired with its output (including `choose` and `bridgeResult` answers). Every output carries `eventVersion: "0.51.0"`. It cannot be combined with `--json`, `--ir`, `--derivation`, or `--conditions-output`.

`--no-derive` (or the script's `derive: false`; the flag wins) stops applying the project's Datalog rules, so an unmocked derived atom is unknown and halts the walk incomplete. `--derivation <ATOM>` (repeatable) prints, after the play, the derivation tree of a ground atom — the rule used and each premise's own support (seed fact, asserted, or derived in turn), negated premises shown `(absent)` — or, when it does not hold, every rule that could conclude it with its failing premises; `--json` carries the same tree under `explain`.

Exit **0** complete (every step played, or an `end: true` step ended the playthrough) with every expectation met, **1** an error (the project fails to compile, a vocabulary conflict, a `pick` that is not eligible, a `select: all` step with eligible beats and no `pick`, a step whose own `bridges:` answers went unconsumed, two [exclusive relations](/tooling/play/#exclusive-relations) holding together — `✗ exclusive: …` at the write, dsl 0.25.0 — or an `expect:` miss — each miss names the step, its `label:`, and the actual value), **2** a usage/I/O failure (a malformed script, an unknown occasion or `expect:` key, a step target outside its occasion's target domain, a save id the project does not declare, a `bridges:` answer that fits no call or lacks a field content reads, an `include:` cycle, an `engine:` write that does not fit its declared type or moves the clock backward — or any `quest.*` write, since quest status is the lifecycle's and a save seeds it with top-level `quests:` — or an unreadable project), **3** incomplete (an unscripted choice or hub, a `when` or quest objective the reference runtime cannot decide, `now()`/`validAt()`, or a plugin call with no `bridges:` answer, which halts at the call). A `choose:` decision that is not offered — an ineligible choice, or a `once` hub option already taken (`E-TRACE-CHOICE`) — is exit **1**, like an ineligible `pick`. `lute test` runs every play script that carries an `expect:`. Script format, selection order, and transcript shapes: [Playing a story](/tooling/play/).

## test

```console
$ lute test [<PATH>]… [--project <DIR>] [--providers <DIR>] [--permission-profile <NAME>]
            [--coverage] [--no-derive] [--deny <CODE>]… [--deny-warnings] [--json]
```

Run the project's scenario tests. With no operand, every `*.test.yaml` under the project directory (`--project`, else discovered from the current directory) traces its scene or quest — or presents a lore document's entries or one of its bundle beats — against the declared mocks and asserts the declared expectations, and every `*.play.yaml` there that carries an `expect:` (on a step or at the top level) is played exactly as [`lute play`](#play) plays it and judged by its own expectations (dsl 0.22.0). Operands select scripts instead: each `<PATH>` is a script file (`*.test.yaml` or `*.play.yaml`), or a directory walked for `*.test.yaml` and expectation `*.play.yaml`; any other file is exit **2**. Each script resolves against its nearest project at or below the project directory (`--project` is the walk root, as in [Project and discovery](#project-and-discovery)), as `lute trace` resolves one document. When `lute test` discovered the project (no `--project`), standard error says so once per project — `lute: note: scenario tests use project . (nearest lute.project.yaml); pass --project to choose another`; with `--project` it prints no note (since 0.52, #214), as other commands do. With no manifest above the document the trace is core-only. `--json` emits the machine-readable report; `--providers` pins a snapshot directory; `--permission-profile` applies its ceiling; `--deny`/`--deny-warnings` promote every check verdict a test consults (each traced document's check and each play's compile gate), so a denied warning fails that test. `--no-derive` turns derivation off for every test and play, overriding their own `derive:` keys (see [trace](#trace)). Exit **0** when every test and play passes, **1** on a failure, **2** on I/O or a malformed test file.

Each test and play prints one `PASS`/`FAIL` line naming its file and what it ran, a failure followed by its misses, then a summary:

```console
$ lute test --project .
PASS  ./tests/lamp-quest.test.yaml  (./tests/../quests/lamp.lute)
PASS  ./tests/mara-first.test.yaml  (./tests/../scenes/talk/mara-first.lute)
FAIL  ./plays/first-day.play.yaml  (play of .)
      step 6 at townVisit: expect winner: expected town.welcome, actual town.morning

2 passed, 1 failed
```

A test whose `file:` names a document that does not exist fails on its own as `E-TEST-FILE`, and the rest of the suite still runs:

<!-- lute-diagnostics unverified="verbatim lute test output; crates/lute-cli/src/testcmd.rs writes the `error [E-TEST-FILE] ` prefix inside the same format! literal as the message, so the message alone matches no literal" -->
```console
FAIL  ./tests/old-shed.test.yaml  (./scenes/old-shed.lute)
        error [E-TEST-FILE] `file: ../scenes/old-shed.lute` names no document (./scenes/old-shed.lute does not exist)
```

A play passes when every step and top-level expectation holds and the play completed. **A play that halts fails** — an unscripted choice, or a `when` the reference runtime cannot decide — unless its top-level `expect:` declares the exit (`expect: { end: incomplete }`); the failure names why it stopped. In `--json`, each `tests` entry carries `"kind": "test"` or `"kind": "play"`; a play's misses are in `misses`, each `{ step, label, repetition, occasion, key, expected, actual }` (`step` is `null` for a top-level expectation).

A failing test says why its walk stopped: the unresolved guards and the `state:`/`facts:` entries that would decide them (`--json`: `unresolved`, each with `atoms` and `supply`).

`lute test` loads and checks each project **once** (dsl 0.26.0 §1) and shares it across every test and play under it: the producer set and quest ids are collected once, a play project is compiled once for all its plays — its compile diagnostics print once, not once per play — and then the tests, and then the plays, run in parallel on every logical core (set `RAYON_NUM_THREADS` to cap the threads). Results are reported in the usual file order, each one's standard error replayed in that order.

`--coverage` also reports branch/arm coverage and lists the **untested** units — over the project directory (`--project`, else discovered), so `lute test tests --coverage` still measures the whole project. The unit is the beat, not the file (0.27.0): every bundle beat (`<document id>.<beat id>`) and lore entry is its own unit, and a scene or quest document is one unit; components are untestable and never listed. A unit is covered when a test presents it (a scene test traces the scene; `beat:`, `entry:` and `entries:` present theirs) or a play presents it — through an `occasion:` step or through the occasions a clock raises after an `advance:` step. The branch/hub rows count the options a play picked as well as the ones a traced path chose, under the same `{file}:{id}` row; match arms and `when=` guard outcomes are merged from expect-carrying plays too, keyed by source site, so a play-only arm is reported as executed. Last comes the ending-proof view: the beats no play presented. A scene test traces a beat from mocked state, so it does not show a run reaches it; a play does:

```console
coverage over 2 traced path(s) and 1 play(s) (plays count toward what they presented, the choices they picked and the arms they executed):
  branch/hub maraAsk (scenes/talk/mara-first.lute:maraAsk): 2/2 chosen [lamp, leave]
  2 untested unit(s) under . — no *.test.yaml presents them and no play presents them:
    lore/endings/ren.lute: ember
    scenes/talk/mara-idle.lute
  1 more beat(s) no play presents (a test traces them; only a play proves they are reached in play):
    scenes/talk/mara-first.lute
```

Paths are relative to the directory `lute test` runs in. A `<match>` a component `::use` expanded is its own row, at the component's `file:line:column` and naming the use (`components/card.lute:7:1 (card#1 in scenes/s.lute)`); a line `when=` guard is a `guard` row, `taken` and/or `skipped`, not a match.

`--json` carries the same under `coverage` (`tracedPaths`, `plays`, `choices`, `arms`, `untested`, `notPresentedByPlay`); each `arms` row has `kind` `match` or `guard`, and each unit is `{file, id, doc, local, kind}` (`kind` is `scene`, `quest`, `document`, `beat` or `entry`). `id` is the unit's project-wide id: an entry's id is global to the project, a bundle beat's is `<document id>.<beat id>`, a scene's its key. `doc` is the document's id and `local` the id as the document writes it (`null` for a whole document). `notPresentedByPlay` lists the beats a test traces but no play presents; an untested beat is listed once, under `untested`.

A `*.test.yaml` file declares:

```yaml
file: ../scenes/confrontation.lute   # the .lute under test, relative to THIS file (tests/ → ../scenes/…)
# optional mock surfaces — identical to `lute trace --mock`:
state:   { run.trueKiller: blake }
facts:   ["implicates(ledger, blake)"]
choose:  { accuse: accuseBlake }
events:  [npcSpoke]
accepts: [identifyKiller]
expect:
  transcriptContains: ["Case closed."]   # substrings that must appear in the transcript
  transcriptLacks: ["You let her go."]   # substrings that must NOT appear
  options: { accuse: [accuseBlake, accuseMoss] }  # the exact options offered at a branch/hub
  state: { run.accused: blake }          # path: literal assertions after the walk
  facts: ["points(blake)"]               # atoms that hold when the walk ends, after derivation
  notFacts: ["points(cass)"]             # atoms that do not hold then
  end: complete                         # complete | terminal | incomplete | error
```

`file:` is required; every mock surface and every `expect:` key is optional. The mock surfaces also include `visited:` and `occasions:` (dsl 0.21.0), the save seeds `quests:` and `entriesRead:` (dsl 0.22.0), and `derive:`, exactly as in a [trace mock](#trace). `expect.transcriptContains` lists substrings that must appear in the transcript and `expect.transcriptLacks` substrings that must not. `expect.options` maps a `<branch>`/`<hub>` id to the exact set of options the walk offered there, order-insensitive and across all its presentations; a mismatch names both sets, and a branch the walk never presented fails as such. `expect.state` maps a state path to the literal it must hold after the walk — compared against the value the walk ends with, read the way trace reads it: the walk's last write (a [credited reward](/tooling/tracing/#rewards-and-objective-bodies) included), else the test's `state:` seed, else the declared `default:` — and `expect.end` asserts how the walk ended (`complete`, `terminal` when the project's `terminal:` holds at its end, `incomplete`, or `error`). `expect.facts` and `expect.notFacts` list ground atoms that must hold, or must not, when the walk ends — after derivation, so a rule's conclusion is asserted directly. A miss names both sides (`facts points(blake): expected holds, got does not hold`), and an atom whose derivation read undecided state is `unknown`, which satisfies neither list.

Since dsl 0.24.0 a test takes the mock's `bridges:` key too (see [Bridge answers](/tooling/tracing/#bridge-answers)), and three expectations read more: `expect.accepts: [quest ids]` asserts the quests the scene's `::accept`s took, as a set (`accepts: expected [toll], got [parley]`); `expect.options` of a `<hub>` is every choice eligible at any of its visits, unioned (it was always `[]`); and `transcriptContains` / `transcriptLacks` match only the content lines that played, each in the form `@speaker: text` — the form a play script matches, so `"@narrator: Always shown."` works in both, and a guarded line that never played no longer satisfies `transcriptContains`.

A needle with no attribute block matches a line whatever its delivery attributes. Since dsl 0.27.0 a needle's own attribute block is judged (0.26.0 dropped it): a needle line written `@mara{emotion="content"}: You're new.` matches only a line whose attributes include `emotion="content"` (the line may carry more), so a line pasted from a play or a trace matches the line it came from and not the same words said another way. A `transcriptLacks` needle with a block therefore holds when the words are said with other attributes — write it bare (`"@mara: You're new."`) to mean "never said". The block's keys and values are checked first: a key a transcript line never shows (with a did-you-mean), a value outside its domain, a non-integer `variant`, or a value on a delivery flag makes the test invalid (`E-TEST-NEEDLE`, reported as a `FAIL` like `E-TEST-KEY`), in `transcriptContains` and `transcriptLacks` alike. A `transcriptContains` miss quotes the presented line nearest to the needle, as it was said: `` transcriptContains "@mara: Tomas keeps the oil. Ask her.": absent (nearest line: "@mara{emotion=\"delighted\"}: Would you? Tomas keeps the oil. Ask him.") (expected present) ``. Since 0.27.0 the nearest line is a line of the needle's speaker first, then the one the needle needs the fewest edits to occur in, so a short line is no longer "near" a long needle (round-5 T3-16).

**An incomplete walk fails.** When an unknown guard halts the trace, the expectations after it were never walked, so the test fails — whatever else it asserts — unless it declares `expect: { end: incomplete }`. Derivation is on (see [trace](#trace)): a rule-derived fact follows from the test's `facts:` and the project's seed facts, with no need to mock the conclusion. **Migration from 0.21:** a test that relied on an unmocked derived atom being unknown (exit `incomplete`), or on a seeded relation reading empty, now sees the derived or seeded answer; pin `derive: false` to keep the old verdict.

**`end: complete` means the end of the document** (dsl 0.26.0 §7). The walk follows a taken `::jump` to its `::label`, as play does, so `end: complete` holds only when the walk reached the end — not when it stopped at a jump — and the transcript and state expectations see what play sees (see [Following a taken `::jump`](/tooling/tracing/#following-a-taken-jump)). The walk also applies an `::accept` in a quest `<on>` handler to a quest of the same document, so `expect: { quests: { second: active } }` holds as it does in play; `accepts:` resolves quests project-wide, a quest another document declares included; and a quest document may seed its own `quest.<id>.*` — `quests: { lampOut: active }` in a test of `quests/lamp.lute` — which starts the quest there. Before 0.26.0 each of these disagreed with `lute play`.

`file:` may name a lore document when the test says what to present: `entry: <id>`, or `entries: [ids]` to present several in order with the read flags set between them, so a repeated id is a re-read that skips first-read effects — or `beat: <id>`, one [bundle beat](/tooling/tracing/#bundle-beats) by its bare or canonical `<document id>.<beat id>`, walked as `lute trace --beat` walks it. A test takes `beat:` or `entry:`/`entries:`, not both (exit **2**), and a lore test that names none is `E-TEST-LORE`, which lists the declared entry ids and beat ids. Two entries, read in order:

```yaml
file: ../lore/tomas.lute
entries: [tomasOil, tomasBusy]
quests: { lampOut: active }            # seeds quest.lampOut.state, which tomasOil's `when` reads
expect:
  transcriptContains: ["Top shelf.", "Busy."]
```

An entry may be named `<document id>.<entry id>` in `entry:`, `entries:` and `expect.eligible` keys (`entries: [lore.tomas.tomasBusy]`, dsl 0.26.0 §8). A bundle beat that targets a [whole kind](/tooling/play/#kind-targets) reads its member from a seed, `state: { occasion.target: r16Gus }`, typed by the kind, as a play step's `target` would bind it.

Trace presents an entry or a beat whether or not its `when` holds — the `when` is the engine's gate, shown, not enforced. `lute test` enforces it (dsl 0.26.0 §7): a test that presents an entry, a bundle beat or a scene beat its mocks make ineligible — its `when` is false, its `after:` does not hold, its `once: user` is spent, or an entry's `once="run"` / `once="user"` is spent — **fails** unless it asserts `expect.eligible`, since the engine would never present it and the walk proves nothing about play. An entry's `once` is spent by its read flags, which the mock's `entriesRead:` seeds (`run:` sets `entry.<id>.read` and `everRead`, `user:` sets `everRead` alone) and an earlier presentation in the same test sets, so the failure names which: ``it is `once="run"` and already spent — the mocked `entriesRead: { run: [<id>] }` read it (`entry.<id>.read`)``. Without the `quests:` seed, `quest.lampOut.state` reads `unset` and Tomas's oil entry is not eligible:

```yaml
# tests/tomas-oil.test.yaml
file: ../lore/tomas.lute
entry: tomasOil
expect:
  transcriptContains: ["Top shelf."]
```

```console
FAIL  ./tests/tomas-oil.test.yaml  (./tests/../lore/tomas.lute)
      eligible tomasOil: not eligible under these mocks (its `when` is false) — the engine would never present it, so the walk proves nothing about play; fix the mocks, or assert `expect: { eligible: { tomasOil: false } }` (the body is then not walked)
```

The failure names the premise that is false and, where one exists, the mock that makes it hold: ``its `when` (user.bond.mara >= 1) is false``, ``its `after: visited('town.welcome')` is false — mock `visited: [town.welcome]` ``, or ``it is `once: user` and the mocked `visited:` already lists it``. Before 0.26.0 such a test passed with a note, so a contract test of another author's scene stayed green after that scene could no longer be presented.

An `expect.eligible: { id: true }` that misses names the same premise (round-5 T3-12): ``eligible ember: expected true, got false — its `after="visited('ren.confession')"` is false — mock `visited: [ren.confession]` `` for a bundle beat's `after=`, ``… — its `when` (run.fish == 1) is false`` for an entry or beat. Trace and test judge every presented scene, entry and bundle beat by the one rule `lute play` and `lute calendar` select by — `once`, `after`, `spentBy`, then `when`.

`expect.eligible` asserts the verdict: `true` or `false` for every entry and beat the test presents, or a mapping from id to verdict (`eligible: { tomasOil: false }`; a beat may be named by its bare id). A `false` verdict may name the premise that must close it, `eligible: { mara.first: { false: once } }` — one of `once`, `after`, `spentBy`, `when`, `gate` (the occasion's `raisedWhen:`) or `terminal` — so the test fails when the beat is closed for another reason (`eligible mara.first: expected false (when), got false (once)`). With it asserted an ineligible body is not walked: an `eligible: false` test needs no bridge answers for a body the engine never plays, and a transcript expectation on that body fails as absent. Since dsl 0.26.0 it works on a scene beat (`on:`) too, judged by the scene's `when`, `after:` and `once`. A map key may also name an entry or bundle beat of the file that the test did not present (dsl 0.24.0): it is judged alone, under the same mocks, so a lore test may carry a map-form `eligible:` without presenting anything. A `when` the mocks leave undecided matches neither verdict. A miss reads `eligible tomasOil: expected true, got false`.

`expect.quests` (dsl 0.21.0 §7a.4) asserts a quest document's lifecycle outcome directly — the state each quest ended the trace in, one of `unset`, `active`, `complete`, `failed`:

```yaml
file: ../quests/hold.lute
visited: [haven.shed]
occasions: [runEnd]
expect:
  quests: { holdLine: complete, sideJob: unset }
```

A mismatch fails as `quests holdLine: expected "complete", got "active"`; a value outside the four states, or a quest id the traced document does not declare, fails the test with a message naming it. (The top-level `quests:` key is the seed the walk *starts* from; `expect.quests` is the outcome it must *end* in.)

## lint

```console
$ lute lint [<PATH>] [--project <DIR>] [--providers <DIR>] [--config <FILE>]
            [--deny <CODE>]… [--deny-warnings] [--json]
```

Run the advisory content lints — line length, dialogue ratio, emotion streaks, missing assets, and project-local rules — over a file or a directory tree (default: the current directory; with `--project <DIR>` and no `<PATH>`, the project directory). The linear-VN norms (`L-SECTION-STARTS-WITH-BACKGROUND`, `L-DIALOGUE-RATIO`, `L-SCENE-LENGTH-SPREAD`) judge only linear scenes, never beats, components, quests, or lore. Documents are grouped by their nearest `lute.project.yaml` (with `--project <DIR>`, `<DIR>` is the walk floor: each document resolves against its nearest project at or below it), and each project's `lute.lint.yaml` (or `--config <FILE>`) sets rule levels, thresholds, ignore globs, and `custom:` rules. Findings are `L-*` codes, separate from `lute check`: lints never enter the capability snapshot or change an artifact. `--deny`/`--deny-warnings` promote findings as in `check`. Exit **0** clean or only sub-error findings, **1** any error-severity finding (including `E-LINT-CONFIG`/`E-LINT-EXPR`), **2** on I/O, malformed YAML, or usage. Rules, metrics, and the config format: [Linting](/tooling/linting/).

`lute lint <DIR>` uses the project (0.49.0): it discovers the nearest `lute.project.yaml` at or above `<DIR>`, applies that project's `lute.lint.yaml`, and prints findings relative to the project root. `lute lint <DIR> --project <P>` lints `<DIR>` inside `<P>`; a `<DIR>` outside `<P>` is `E-CLI-USAGE` (`lute lint: <DIR> is not inside the project <P>`).

Since dsl 0.26.0 lint also reports `W-DISPLAY-NAME-DUP`, the advisory whole-project `check` reports too: two speakers the dialogue box would show under one name — two cast entries with the same `name:`, a cast name equal to a `::use{… name="…"}` display string, or two such strings for different speakers (`who=`). A cast entry marked `sharedName: true` is an intended role name several speakers share and is not counted. The code is not a `lute.lint.yaml` rule; `--deny W-DISPLAY-NAME-DUP` makes it an error. See [Linting](/tooling/linting/#display-names).

## loc export

```console
$ lute loc export [--project <DIR>] [--format csv] [-o, --output <PATH>]
```

Extract every translatable content line — the stable `code`, speaker, text, choice text, and branch/hub prompt — across the project to a localization export. The exported `text` keeps the source markup: inline modifiers (`:pause{s=0.5}`, `:speed[…]{rate=…}`, text-style spans) and `{{…}}` interpolations, which a translation must carry over with the same modifier set. JSON is the default and `--format csv` selects CSV (`--format json` is `E-CLI-USAGE` — omit it); `-o`/`--output` writes to a file instead of stdout. `loc export` is an artifact writer and takes no `--json`. Exit **0** on success, **2** on I/O.

Each row also carries the `lineId` the compiler will stamp on that record — the join `loc import` and `compile --locale-bundle` key on. It is `null` (JSON) or empty (CSV) for a line with no authored `code`, whose id the compiler back-fills from the post-expansion command stream and which no source-only walk can reproduce: run `lute tag` first, and the advisory `N lines untagged — run lute tag` on stderr goes away with it.

Row kinds are `line`, `choice` (an option label, `key` `{branchOrHubId}.{choiceId}`) and `prompt` (a branch's or hub's `prompt=`, `key` `{branchOrHubId}.prompt`, lineId `{prefix}.{branchOrHubId}.prompt`), in document order; a prompt row comes before the options of its branch or hub. Every row carries `sourceHash`: `"sha256:<hex>"` of the record's **plain** source text as the compiler emits it — a line's text with inline modifiers and escapes removed, an option's text, a prompt. Translators leave it unchanged; `loc import` stores it so `compile --locale-bundle` can tell when the source changed since (`W-L10N-STALE`). In CSV it is the last column:

```text
kind,file,line,lineId,code,speaker,key,text,sourceFile,sourceLine,sourceHash
```

Since 0.51 there is a fourth kind, `directive`: one row per [`text` attribute](/language/dialogue-and-cast/#localizable-directive-text-since-051) of a plugin directive, in document order (a directive's attributes in declaration order). Its `key` is `{tag}.{attr}`, its `lineId` is `{prefix}~{tag}~{code}~{attr}`, and its `code` is the directive's authored `code`. As for a line, `lineId` is reproducible only when `code` is authored; otherwise it is `null` (JSON) or empty (CSV), and the stderr advisory counts them (`N directive texts untagged — run lute tag`). The JSON row carries `kind`, `file`, `line`, `lineId`, `code`, `key`, `text`, `source` and `sourceHash`. In CSV the existing columns are used, with `kind` `directive`, `code` and `key` filled and `speaker` empty; no column is added. The `::banner` of [`docs/examples/flow-and-text`](https://github.com/journeyWorker/lute/tree/main/docs/examples/flow-and-text) (`lute loc export --project docs/examples/flow-and-text`):

```json
{
  "code": "0010",
  "file": "docs/examples/flow-and-text/scenes/hall.lute",
  "key": "banner.title",
  "kind": "directive",
  "line": 18,
  "lineId": "hall~banner~0010~title",
  "source": null,
  "sourceHash": "sha256:fde24517faf3…",
  "text": "Welcome to the hall, {{userName}}"
}
```

```text
directive,docs/examples/flow-and-text/scenes/hall.lute,18,hall~banner~0010~title,0010,,banner.title,"Welcome to the hall, {{userName}}",,,sha256:fde24517faf3…
```

(The hash is shortened here.)

## loc import

```console
$ lute loc import <FILE>… [-o, --output <PATH>] [--json]
```

Canonicalize translated `loc export` files into one **locale bundle** — the reverse direction, consumed by `lute compile --locale-bundle`. Exit **0** on success, **1** on `E-LOCALE-BUNDLE`, **2** on I/O. `--json` prints the `0.48.0.loc-import` report, `{output, locales, lines, skipped, diagnostics: [{path, code, message}]}`, and requires `--output`.

Input is exactly what `export` writes, in either format (`.csv` → CSV, anything else → JSON). `export` carries no locale, because it extracts the *source* language — so the normal workflow is **one file per locale**: copy the export to `ja-JP.json`, translate the `text` values, and the file **stem** is the locale tag. A row carrying its own non-empty `locale` field (JSON) or `locale` column (CSV) overrides that, so a single merged file spanning every locale also works.

```json
{
  "schemaVersion": 2,
  "locales": ["en-US", "ja-JP"],
  "entries": {
    "marina.s01ep02.marina_0010": { "en-US": "Hello there.", "ja-JP": "こんにちは。" }
  },
  "sourceHashes": {
    "marina.s01ep02.marina_0010": {
      "en-US": "sha256:23ea498e82f4…",
      "ja-JP": "sha256:23ea498e82f4…"
    }
  }
}
```

(Hashes shortened here.) `sourceHashes` holds each row's `sourceHash` per lineId **and** locale (two locales may be translated from different exports); it is always written, `{}` when no row carried a hash. Every map is sorted, so the bundle is byte-stable: importing the same inputs twice produces identical bytes. `compile --locale-bundle` still reads a `schemaVersion: 1` bundle (no `sourceHashes`, no stale check). An unparseable input, a `lineId` appearing twice within one locale, or an empty locale tag is `E-LOCALE-BUNDLE`, reported with the offending file and row. A row with **no** `lineId` is skipped rather than rejected — an untagged line simply has no stable identity yet — and a single stderr summary counts them.

Since 0.51 `loc import` accepts rows of `kind: "directive"` and keys the bundle by their `lineId` like every other row; an unknown `kind` is still refused. A directive row with no `lineId` (no authored `code`) is skipped like any untagged row.

## loc report

```console
$ lute loc report [--project <DIR>] [--json]
```

Word-count and line-count report per document and per speaker — a production-planning view over the same content lines. Choice labels are counted (`choices`) but their words are not; branch/hub prompts are counted (`prompts`, since 0.47 — report version `0.47.0.loc-report`) and their words are included in the document and project word totals (they have no speaker). `--json` emits the report as JSON instead of human table lines. Exit **0** on success, **2** on I/O.

Since 0.51 a directive's `text` attributes count like prompts: their words join the document and project word totals (they have no speaker). In the JSON a document and `totals` carry `directiveTexts` only when it is nonzero; the human table gains a `directive texts` column, and its summary a `N directive text(s)` count, only when the project has any.

## --explain

```console
$ lute --explain <CODE>
```

Explain one diagnostic code — the bracketed `CODE` in `file:line:col: error [CODE] message`, in any letter case (`e-set-shape` works): its severity, what raises it, the spec sections behind it, and the link to its entry in the [diagnostics reference](/reference/diagnostics/), which lists every code. A code it does not know is a usage error (exit **2**); otherwise it exits **0**. This is a top-level flag, not a subcommand, and it is unrelated to [`lute play --derivation <ATOM>`](#play), which prints the derivation of a fact after a play.

```
$ lute --explain E-LEGACY-CONTENT-SIGIL
E-LEGACY-CONTENT-SIGIL (error)

A content line uses the old `:` speaker sigil, which `@` replaced — write `@speaker{…}: text` instead.

Spec: dsl §7.1
More: https://lute-lang.vercel.app/reference/diagnostics/#e-legacy-content-sigil
```

## version

```console
$ lute version [--json]
```

Print the three independent version axes ([versioning](https://github.com/journeyWorker/lute/blob/main/docs/versioning.md)): the **toolchain** version (this CLI and the workspace crates), the **language** version (the grammar/semantics the checker enforces), and the **IR** schema version (stamped as `irVersion` in every compiled artifact). Distinct from clap's built-in `--version`, which prints only the toolchain version; the language server answers the same flag, `lute-lsp --version` printing `lute-lsp <version>` (which [`doctor`](#doctor) compares against this CLI). `--json` prints one object `{"toolchain":…,"language":…,"ir":…}`; human mode prints one labeled line each. Always exits **0**.

## `lute fmt` (0.36.6)

## `lute diff` (0.36.6)

## `lute patch` (0.36.6)

## `lute context` edit surface (0.36.6)

```console
$ lute fmt [<PATH>]… [--check] [--json]
$ lute diff <BEFORE> <AFTER> [--json]
$ lute patch <FILE> [--project <DIR>] [--providers <DIR>] [--permission-profile <NAME>]
             [--dry-run] [--json]
$ lute context <FILE> [--project <DIR>] --node <NODE> [--max-items <N>]
               [--script <FILE>]… [--all-scripts] [--json]
$ lute context <FILE> [--project <DIR>] --at <FILE>:<LINE>:<COLUMN> [--max-items <N>]
               [--script <FILE>]… [--all-scripts] [--json]
```

`<NODE>` is the target key `<kind>:<key>`.

### Identity migration in `tag` and `diff` (0.36.6)

`lute tag <PATH>` now back-fills missing line `code` values and missing
component invocation keys. It preserves every authored value, writes
`instance="use-001"`, `use-002`, … in source order within each owner and
component, and is idempotent. `--renumber --commit` still retags line codes only; it never
rewrites an explicit `instance`.
With `identity.requireStable: true`, projects diagnose untagged lines and
`::use` calls until tagging completes. In JSON diff output, an authored
`identity.renames` ledger produces a row shaped
`{kind:"renamed", node:<to>, from:<old>, to:<new>, before, after, locations:[old,new], declarationLocation:{file,span}}`.
A removed or added save-shaped identity without a matching ledger entry carries
`unmappedIdentity: true`; Lute never infers a rename from position or text.

```lute check
---
kind: scene
id: cli-identity
---
## Opening
@narrator{code="intro"}: Ready.
```
`fmt` formats `.lute`, project/schema YAML, and configured plugin YAML in
deterministic order (the default `<PATH>` is `.`). YAML values are opaque; it never reformats arbitrary YAML.
Without `--check`, it writes canonical text; `--check` writes nothing and exits
1 if any selected file differs, 0 when canonical, and 2 on I/O, UTF-8, or
parse failure. `--json` prints the `0.48.0.fmt` report, `{applied, files: [{path, changed}]}`, over every selected file.

`diff` compares directories or `git:<revision>` sides and emits
`schemaVersion: "0.36.0.diff"`, `before`, `after`, and sorted semantic
`changes`; formatting-only changes yield an empty array. When the other side
is a directory inside the current git work tree, a `git:` side is archived
from that subtree only (`git archive <rev> -- <its path>`) and rooted there,
so `lute diff git:HEAD story` from the repository root compares the `story/`
project and a symlink elsewhere in the repository does not matter (one inside
the compared tree is still refused; the directory name is a literal path,
and a directory missing at that revision is `E-CLI-IO` naming both); otherwise (the other side outside the
repository, or both sides `git:`) the current directory's tree is archived
(0.47). Exit 0 means a
comparison was produced; exit 2 means invalid input, materialization, parse,
or I/O failure.

`patch` takes the patch request as its one operand, `<FILE>`; the project is `--project` or discovered from the current directory (a second operand is `E-CLI-USAGE` naming `--project`), and `--providers` and `--permission-profile` reach the model the patch is applied against. It accepts the strict request shape documented in [the patch request](/tooling/ai-harness/#the-patch-request-047)
(fields, ops, preserve keys, node-key kinds, an example); `lute schema patch`
describes it under `inputs`.
Operations are `replaceNode`, `insertBefore`, `insertAfter`, `replaceAttr`,
`removeNode`, `createFile`, `moveFile`, and revision-guarded `replaceText`.
Preserve claims include `ids`, positional `lineIds`, `voiceKeys`,
`choiceEffects`, `rewards`, `conditions`, `reachability`, `hostContracts`,
and `constraints`.
A patch report uses `schemaVersion: "0.43.0.patch"` and keeps the existing
`before`, `after`, `diff`, and `writes` fields. Its `accepted` field says
whether the patch passed patch validation; the envelope's `ok` instead follows
the process exit rule. Thus an accepted patch with semantic changes has
`accepted: true`, `ok: false`, and exit **1** so the caller can review the
diff; an accepted, meaning-preserving patch has both `accepted: true` and
`ok: true` with exit **0**. `--dry-run` uses the same decisions without
writing. Refusals exit **2** and retain their patch codes
(`E-PATCH-STALE`, `E-PATCH-TARGET`, `E-PATCH-EDIT`, `E-PATCH-FORMAT`,
`E-PATCH-CHECK`, `E-PATCH-PRESERVE`); an unreadable patch file uses the JSON
error envelope. Several edits in one file are resolved against the original
text, and only the lines they touch are formatted. A `.lute` file that was
`lute fmt --check` clean stays clean; in any other file the touched lines end
up canonical. A patch that would break this is refused with `E-PATCH-FORMAT`
naming the file, and nothing is written (0.47).

`context` returns `schemaVersion`, `projectRevision`, `files`, `target`,
`declared`, `references`, `affected`, `tests`, `plays`, `vocabulary`, and
`notIncluded`; bounds are reported rather than silently dropped. `--script <FILE>`
(repeatable; a file, or a directory selecting every script under it) executes those scripts and marks executed evidence as `witnessed`; `--all-scripts` executes every play/test script of the project. The two are exclusive (`E-CLI-USAGE`).

## Removed in 0.48

Every spelling below was valid in 0.47.2 and is gone. For one minor release each is answered with an `E-CLI-USAGE` error (exit **2**) whose message names the replacement and, where the rest of the command line allows, the full rewritten command — ``lute check-project docs/x`` answers ``lute check-project is now `lute check --project docs/x` ``. In JSON mode (`--json` before `--`) the error envelope carries `argument` (the removed spelling) and `hint` (the replacement). A removed command is not a command: its error envelope has `command: null`, and telemetry records no command id for it.

| Removed | Replacement |
|---|---|
| command `check-project [<DIR>]` | `check --project <DIR>` |
| command `compile-stream <FILE>` | `compile --stream <FILE>` |
| `compile --all` | `compile --project <DIR> -o <DIR>` (no document) |
| `--out` (`compile`, `loc export`, `loc import`) | `-o, --output` |
| `compile --locales` | `--locale-bundle` |
| a project directory operand (`impact`, `constraints`, `patch <DIR> <PATCH>`, `doctor`, `play`, `test`, `loc export`, `loc report`, `beats`, `calendar`, `scenario`, `lore`, `refs`) and `check <DIR>` | `--project <DIR>` |
| `catalog refresh <DIR>` | `--providers <DIR>` |
| `new <KIND> <NAME> --dir <DIR>` | `--project <DIR>` (`new <kind> <NAME>` itself still parses: the kinds are subcommands) |
| `impact <DIR> <TARGET>` | `--project <DIR> --node <TARGET>` |
| `context --target`, `--run` | `--node`, `--script` |
| `constraints --run` | `--all-scripts` |
| `scenario reach <NODE>`, `scenario envelope <NODE>`, `scenario knowledge --for` | `--node <NODE>` |
| `scenario --format json` | `--json` |
| `play --explain`, `--dump-conditions` | `--derivation`, `--conditions-output` |
| `run --dump-conditions` | `--conditions-output` |
| `trace --choose`, `--accept` | `--choice`, `--accepted-quest` |
| `calendar --facts`, `--csv`, `--occasion O@<AXES>` | `--relation`, `--format csv`, `--occasion-axes O=<AXES>` |
| `loc export --format json` | (omit: JSON is the default) |
| `tag --force` | `--renumber --commit` |

The undocumented `--on` refusals of `trace`, `beats`, `calendar` and `new`, and `trace`'s `--target` refusal, are gone too: those spellings are plain unknown-argument errors.
