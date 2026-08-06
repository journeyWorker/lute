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
                      [--json] [--deny <CODE>]… [--deny-warnings]
```

Compile a document to its JSON command-record artifact (gated on a clean check). Exit **0** on success, **1** on a failed gate, **2** on I/O or serialization failure. The artifact is always JSON; `-o`/`--out` writes it to a file instead of stdout. With `--project`, the gate is the target's reconciled `check-project` verdict.

### `--all` — project-wide compile and index

`--all` compiles **every** `*.lute` document under `--project <DIR>` into `-o <DIR>`, mirroring the project's own layout (`quests/a.lute` → `<outdir>/quests/a.lute.json`), and writes a `<outdir>/project.index.json`. Under `--all`, `-o` is an output **directory**, not a file; it is created if absent. `*.component.lute` fragments are skipped — a component is inlined into its importers and has no artifact of its own.

`--all` requires **both** `--project` and `-o` and takes no `<file>`. Each of those three is checked independently and every violation is reported, then the command exits **2** without reading a document:

```console
$ lute compile --all
error: --all requires --project <DIR> (the document set and capability snapshot both resolve per project)
error: --all requires -o <DIR>, an output DIRECTORY (there is no single artifact to write to stdout)

Usage: lute compile --all --project <DIR> -o <DIR>
```

The index carries the document table plus the **union** of every artifact's `entities`, `enums`, `relations`, `seedFacts`, `rules`, and `prereqEdges` — the union [an engine must compute anyway](/tooling/runtime-contract/) before it can evaluate anything:

```json
{
  "irVersion": "0.10.0",
  "capabilityVersion": "…",
  "documents": [
    { "path": "quests/findKai.lute", "artifact": "quests/findKai.lute.json",
      "kind": "quest", "key": "findkai" },
    { "path": "scenes/opening.lute", "artifact": "scenes/opening.lute.json",
      "kind": "scene", "key": "narrator.s01ep01" }
  ],
  "entities": [], "enums": [], "relations": [], "seedFacts": [], "rules": [], "prereqEdges": []
}
```

`path` is the source, relative to the project root; `artifact` is its compiled output, relative to the output directory; `key` is the document's canonical node id — a scene's `{character}.{episodeId}`, or a quest document's **first** declared `<quest id>` (a quest pack's remaining ids stay recoverable from its own artifact's `quest` records). All paths are forward-slash relative, never absolute, so an index survives being copied between machines or packed into a game archive.

`documents` is sorted by `path` and every vocabulary array is deduplicated and totally ordered, so the index is byte-stable across runs. Unlike an artifact, which omits an empty vocabulary array, the index always emits all six — an engine unions them unconditionally, and an absent key would force it to distinguish "no relations" from "index too old to carry them".

### `--all` is all-or-nothing

Every document is compiled in memory and the index is built before anything touches the filesystem. Three things stop the write, each exiting **1** with nothing emitted:

- **A failed gate.** One document's diagnostics print, followed by `N of M document(s) failed; no output written`.
- **A `--deny`-promoted warning** (`--deny W-L10N-MISSING`, `--deny-warnings`): `--deny promoted N diagnostic(s); no output written`.
- **A vocabulary conflict.** Two documents declaring the same entity kind / enum / relation / prerequisite node with **different** signatures, or resolving different capability snapshots — never a silent pick. This is the one class `check-project` cannot see, because it validates each document against its own resolved vocabulary and never unions across independent documents:

```console
$ lute check-project .
ok: . (2 file(s), 0 project-wide warning(s))
$ lute compile --all --project . -o out
lute compile --all: relation `knows` is declared with conflicting signatures by two documents (`scenes/a.lute` and `scenes/b.lute`)
lute compile --all: 1 vocabulary conflict(s); no output written
```

An `E-`-severity capability-resolution diagnostic (a bad plugin option, a bad `identity:` template) also exits **1** — see [the AI harness guide](/tooling/ai-harness/#capability-resolution-errors-gate-the-exit-code).

`--all` writes, but never prunes: an artifact whose source document was deleted stays in the output directory. Build into a directory you own and clear.

### `--locales` — merge a translation bundle

`--locales <bundle.json>` merges a locale bundle (see [`loc import`](#loc-import)) into the artifact: `texts` on every line record and `labels` on every choice/hub option, both keyed by `lineId`. The source-language `text`/`label` is never overwritten, and both maps are omitted when empty — so a document compiled without `--locales` is byte-identical to before. A bundle entry matching nothing in this document is ignored; a bundle legitimately spans a whole project. It composes with `--all`, merging the one bundle into every artifact.

A translatable record missing a locale the bundle declares is `W-L10N-MISSING`, one per `(lineId, locale)` pair, written to stderr. It is a warning: the artifact still emits, carrying the source-language string. `--deny W-L10N-MISSING` (or `--deny-warnings`) promotes it, so CI can require a complete translation before anything ships:

<!-- lute-diagnostics -->
```console
$ lute compile scenes/opening.lute --project . --locales bundle.json --deny W-L10N-MISSING -o out.json
scenes/opening.lute:1:1: error [W-L10N-MISSING] [denied] no `ja-JP` text for `narrator.s01ep01.narrator_0020`
--deny promoted 1 diagnostic(s); no artifact emitted
```

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

Read-only reporting over the connectivity layer. With no subcommand, prints the assembled node/edge graph. `reach <nodeId>` reports a node's [reachability verdict](/connectivity/reachability/); `envelope <nodeId>` (or `envelope quest:<id>`) prints the [Guaranteed/Possible tables](/connectivity/envelopes/). `<nodeId>` is a scene's canonical key or `quest:<id>`. Exit **0** on success, **2** on I/O or an unresolvable node id.

`--format` selects the output shape of the bare graph view:

- `text` (default) — the topological layers, then one line per edge with the [atom kind(s)](/connectivity/scene-graph/#edge-kinds) that justify it in brackets.
- `json` — `{"roots":[{"root":…,"layers":[[…]],"nodes":[…],"edges":[…]}]}`. Each node is `{id, kind, prereq, reach}` (`prereq` is the raw declared formula, `null` for an entry node); each edge is `{from, to, kinds}`, where `kinds` is an array because one formula may reference the same node under more than one atom.
- `dot` — one Graphviz `digraph` per root; scenes are boxes, quests ellipses, and an `active`-only edge is drawn `[style=dashed]`.

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
