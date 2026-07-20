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
```

Compile a document to its JSON command-record artifact (gated on a clean check). Exit **0** on success, **1** on a failed gate, **2** on I/O or serialization failure. The artifact is always JSON; `-o`/`--out` writes it to a file instead of stdout. With `--project`, the gate is the target's reconciled `check-project` verdict.

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
