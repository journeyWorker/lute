---
title: CLI reference
description: Every lute subcommand — init, new, check, check-project, compile, run, trace, test, scenario, loc, context, tag, fix, doctor, catalog refresh, version — with its synopsis, key flags, and exit-code contract.
---

`lute` is the headless checker and compiler for `.lute` documents. The core `check()` is the contract; the CLI adds argument parsing, file I/O, and output formatting, and owns no validation logic. Two resolution flags recur: `--providers <DIR>` pins a directory of provider snapshots to resolve ids against, and `--project <DIR>` loads a `lute.project.yaml` + `plugins/` to resolve the document's activated capability snapshot (omit for a core-only `lute.core` check).

## check

```console
$ lute check <file> [--json] [--providers <DIR>] [--project <DIR>]
              [--deny <CODE>]… [--deny-warnings]
```

Statically validate one `.lute` document. Exit **0** clean, **1** when any `Error`-severity diagnostic is present, **2** on an I/O failure. `--json` prints the serialized `CheckResult`; otherwise a human line per diagnostic. `--deny <CODE>` (repeatable, rustc/clippy `-D` precedent, 0.6.1 §5) promotes every diagnostic with exactly that code to an error for the verdict and exit code, and `--deny-warnings` promotes every warning — a pipeline denies `W-UNPROVEN-RELATIONAL` to force human review of relational fact gates, `W-LUTE-VERSION-STALE` to reject a stale `luteVersion` stamp. A promoted diagnostic reports severity `error` with a `"denied": true` marker in `--json`; an unknown code in `--deny` is a usage error (exit **2**), and errors are never demotable.

## check-project

```console
$ lute check-project <dir> [--json] [--providers <DIR>] [--deny <CODE>]… [--deny-warnings]
```

Recursively `check` every `*.lute` file under `<dir>` in deterministic sorted order, each against its own nearest-ancestor `lute.project.yaml` root, **plus** project-wide `<quest id>` uniqueness and the connectivity passes (`E-CONN-*`, `W-QUEST-REF-UNKNOWN`, `E-STATE-MAYBE-UNAVAILABLE`). Exit **0** clean, **1** when any file has an error or a project-wide collision, **2** on I/O. The same `--deny <CODE>`/`--deny-warnings` promotion (see `check`) applies project-wide.

## compile

```console
$ lute compile <file> [--json] [--providers <DIR>] [--project <DIR>] [-o <FILE>]
                      [--locales <FILE>] [--deny <CODE>]… [--deny-warnings]
$ lute compile --all --project <DIR> -o <DIR> [--providers <DIR>] [--locales <FILE>]
```

Compile a document to its JSON command-record artifact (gated on a clean check). Exit **0** on success, **1** on a failed gate, **2** on I/O or serialization failure. The artifact is always JSON; `-o`/`--out` writes it to a file instead of stdout. With `--project`, the gate is the target's reconciled `check-project` verdict.

### `--all` — project-wide compile and index

`--all` compiles **every** `*.lute` document under `--project <DIR>` into `-o <DIR>`, mirroring the project's own layout (`quests/a.lute` → `<outdir>/quests/a.lute.json`), and writes a `<outdir>/project.index.json`. It requires both `--project` and `-o`, and takes no `<file>`; any other combination is a usage error (exit **2**). `*.component.lute` fragments are skipped — a component is inlined into its importers and has no artifact of its own.

The index carries the document table plus the **union** of every artifact's `entities`, `enums`, `relations`, `seedFacts`, `rules`, and `prereqEdges` — the union [an engine must compute anyway](/tooling/runtime-contract/) before it can evaluate anything:

```json
{
  "irVersion": "0.8.0",
  "capabilityVersion": "…",
  "documents": [
    { "path": "quests/a.lute", "artifact": "quests/a.lute.json", "kind": "quest", "key": "findKai" }
  ],
  "entities": [], "enums": [], "relations": [], "seedFacts": [], "rules": [], "prereqEdges": []
}
```

All paths are forward-slash relative (never absolute), `documents` is sorted by `path`, and every vocabulary array is deduplicated and totally ordered, so the index is byte-stable across runs. `--all` is all-or-nothing: a single document failing its gate prints the diagnostics and exits **1** having written nothing. Two documents declaring the same entity kind / enum / relation / prerequisite node with **different** signatures, or resolving different capability snapshots, is likewise an error (exit **1**) — never a silent pick.

### `--locales` — merge a translation bundle

`--locales <bundle.json>` merges a locale bundle (see [`loc import`](#loc-import)) into the artifact: `texts` on every line record and `labels` on every choice/hub option, both keyed by `lineId`. The source-language `text`/`label` is never overwritten, and both maps are omitted when empty — so a document compiled without `--locales` is byte-identical to before. A translatable record missing a locale the bundle declares is `W-L10N-MISSING`, one per `(lineId, locale)` pair, written to stderr; `--deny W-L10N-MISSING` (or `--deny-warnings`) promotes it to an error, so CI can require a complete translation.

## trace

```console
$ lute trace <file> [--state P=L]… [--fact "R(A…)"]… [--choose ID=C[,C]]…
              [--event N]… [--accept Q]… [--mock <FILE>] [--json]
              [--providers <DIR>] [--project <DIR>]
```

Preview a document against author-supplied mocks (see the [tracing guide](/tooling/tracing/)). Exit **0** complete, **1** refused (check errors or invalid mocks — the `E-TRACE-*` codes render like check diagnostics), **2** I/O, **3** incomplete (an `unknown` guard halted the walk).

## scenario

```console
$ lute scenario <dir> [--providers <DIR>] [--format text|json|dot]
              [reach <nodeId> | envelope <nodeId>]
```

Read-only reporting over the connectivity layer. With no subcommand, prints the assembled node/edge graph. `reach <nodeId>` reports a node's [reachability verdict](/connectivity/reachability/); `envelope <nodeId>` (or `envelope quest:<id>`) prints the [Guaranteed/Possible tables](/connectivity/envelopes/). `<nodeId>` is a scene's canonical key or `quest:<id>`. `--format` selects the output shape of the bare graph view: `text` (default, the topological layers), `json` (a stable-keyed `{"roots":[{"root":…,"nodes":[…],"edges":[…],"reach":{…}}]}` document), or `dot` (one Graphviz `digraph` per root). Exit **0** on success, **2** on I/O or an unresolvable node id.

## context

```console
$ lute context <file> [--json] [--providers <DIR>] [--project <DIR>]
```

Emit the project-resolved **authoring surface** an AI or human needs to write valid Lute against this file's project — directives, attrs, enums, asset kinds, providers, state schema, relational vocabulary, delivery flags, referenced reserved quest paths, and `capabilityVersion`. A capability query, not validation — it emits regardless of document diagnostics. Exit **0** on success, **2** on I/O.

## tag

```console
$ lute tag <file>
```

Back-fill a stable `code` into every untagged `:line`, rewriting the file in place. Exit **0** on success, **2** on I/O.

## fix

```console
$ lute fix <file>
```

Migrate a pre-0.2.2 document to 0.2.2 in place — `:line[speaker]{…}: text` → `@speaker{…}: text`, leading `:` sigil → `@`, and choice `as="…"` → `into="…"`. Byte-exact and comment-preserving; writes back only when something changed. Exit **0** on success, **2** on I/O.

## catalog refresh

```console
$ lute catalog refresh <dir> [--project <DIR>]
```

Re-stamp every pinned provider snapshot in `<dir>` against the current `capabilityVersion` and clear its `stale` flag (see [providers & catalog](/tooling/providers-and-catalog/)). Exit **0** on success, **2** on I/O.

## init

```console
$ lute init <dir> [--template minimal|investigation]
```

Scaffold a new Lute project directory — a `lute.project.yaml`, a state schema, a starter scene, and a trace mock, ready for `lute check-project`. `<dir>` must not already contain a `lute.project.yaml`. `--template` selects the starter content: `minimal` (default) or `investigation` (the worked whodunit). Exit **0** on success, **2** on I/O or a refused overwrite.

## new

```console
$ lute new <scene|quest|schema> <name> [--dir <DIR>]
```

Scaffold one new document into an existing project. The first argument is the document kind (`scene`, `quest`, or `schema`); `<name>` is the file stem and id. `--dir` is the project directory to scaffold into (default: the current directory). Exit **0** on success, **2** on I/O or an invalid kind.

## doctor

```console
$ lute doctor [<dir>] [--json]
```

Diagnose the local toolchain and project setup: the version axes, the project manifest, provider snapshots, and editor-integration hints. `<dir>` is the project directory to inspect (default: the current directory). `--json` emits the report as JSON instead of the human checklist. Exit **0** when every check passes, **1** when a check reports a problem, **2** on I/O.

## run

```console
$ lute run <artifact> [--mock <FILE>] [--json]
```

Execute a **compiled artifact** (`lute compile` output) headlessly against a mock playthrough — the reference consumer of the [runtime contract](/tooling/runtime-contract/): command dispatch, CEL guards, the facts + Datalog fixpoint, hubs, and quest lifecycle. Distinct from `lute trace`, which previews *source*; `run` consumes the artifact an engine would. `--mock` is a YAML playthrough (the same surfaces as `lute trace --mock`); `--json` emits the machine-readable transcript. Exit **0** on a complete run, **1** refused, **2** on I/O, **3** incomplete.

## test

```console
$ lute test [<dir>] [--json] [--providers <DIR>] [--coverage]
```

Run the project's scenario tests: every `*.test.yaml` under `<dir>` (default: the current directory) traces its scene against the declared mocks and asserts the declared expectations. `--json` emits the machine-readable report; `--providers` pins a snapshot directory; `--coverage` also reports branch/arm coverage across the tested documents. Exit **0** when every test passes, **1** on a test failure, **2** on I/O.

A `*.test.yaml` file declares:

```yaml
file: scenes/confrontation.lute   # path to the .lute under test, relative to this file
# optional mock surfaces — identical to `lute trace --mock`:
state:   { run.trueKiller: blake }
facts:   ["implicates(ledger, blake)"]
choose:  { accuse: accuseBlake }
events:  [questComplete]
accepts: [identifyKiller]
expect:
  transcriptContains: ["Case closed."]   # substrings that must appear in the transcript
  state: { run.accused: blake }          # path: literal assertions after the walk
  exit: complete                         # complete | incomplete
```

`file:` is required; every mock surface and every `expect:` key is optional. `expect.transcriptContains` lists substrings that must appear in the transcript, `expect.state` maps a state path to the literal it must hold after the walk, and `expect.exit` asserts the terminal verdict (`complete` or `incomplete`).

## loc export

```console
$ lute loc export <dir> [--format json|csv] [-o <FILE>]
```

Extract every translatable content line — the stable `code`, speaker, text, and choice labels — across a project to a localization export. `--format` is `json` (default) or `csv`; `-o`/`--out` writes to a file instead of stdout. Exit **0** on success, **2** on I/O.

Each row also carries the `lineId` the compiler will stamp on that record — the join `loc import` and `compile --locales` key on. It is `null` (JSON) or empty (CSV) for a line with no authored `code`, whose id the compiler back-fills from the post-expansion command stream and which no source-only walk can reproduce: run `lute tag` first, and the advisory `N lines untagged — run lute tag` on stderr goes away with it.

## loc import

```console
$ lute loc import <file>… [-o <FILE>]
```

Canonicalize translated `loc export` files into one **locale bundle** — the reverse direction, consumed by `lute compile --locales`. Exit **0** on success, **1** on `E-LOCALE-BUNDLE`, **2** on I/O.

Input is exactly what `export` writes, in either format (`.csv` → CSV, anything else → JSON). `export` carries no locale, because it extracts the *source* language — so the normal workflow is **one file per locale**: copy the export to `ja-JP.json`, translate the `text`/`label` values, and the file **stem** is the locale tag. A row carrying its own non-empty `locale` field (JSON) or `locale` column (CSV) overrides that, so a single merged file spanning every locale also works.

```json
{
  "schemaVersion": 1,
  "locales": ["en-US", "ja-JP"],
  "entries": {
    "bianca.s01ep02.bianca_0010": { "en-US": "Hello there.", "ja-JP": "こんにちは。" }
  }
}
```

`locales` and `entries` are both sorted, so the bundle is byte-stable: importing the same inputs twice produces identical bytes. An unparseable input, a `lineId` appearing twice within one locale, or an empty locale tag is `E-LOCALE-BUNDLE`, reported with the offending file and row. A row with **no** `lineId` is skipped rather than rejected — an untagged line simply has no stable identity yet — and a single stderr summary counts them.

## loc report

```console
$ lute loc report <dir> [--json]
```

Word-count and line-count report per document and per speaker — a production-planning view over the same content lines. `--json` emits the report as JSON instead of human table lines. Exit **0** on success, **2** on I/O.

## version

```console
$ lute version [--json]
```

Print the three independent version axes ([versioning](https://github.com/journeyWorker/lute/blob/main/docs/versioning.md)): the **toolchain** version (this CLI and the workspace crates), the **language** version (the grammar/semantics the checker enforces), and the **IR** schema version (stamped as `irVersion` in every compiled artifact). Distinct from clap's built-in `--version`, which prints only the toolchain version. `--json` prints one object `{"toolchain":…,"language":…,"ir":…}`; human mode prints one labeled line each. Always exits **0**.
