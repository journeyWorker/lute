---
title: CLI reference
description: Every lute subcommand — check, check-project, compile, trace, scenario, context, tag, fix, catalog refresh — with its synopsis, key flags, and exit-code contract.
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
$ lute scenario <dir> [--providers <DIR>] [reach <nodeId> | envelope <nodeId>]
```

Read-only reporting over the connectivity layer. With no subcommand, prints the assembled node/edge graph. `reach <nodeId>` reports a node's [reachability verdict](/connectivity/reachability/); `envelope <nodeId>` (or `envelope quest:<id>`) prints the [Guaranteed/Possible tables](/connectivity/envelopes/). `<nodeId>` is a scene's canonical key or `quest:<id>`. Exit **0** on success, **2** on I/O or an unresolvable node id.

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
