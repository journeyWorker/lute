---
title: "AI harness surface — 0.46.0"
status: Implemented
---
# AI harness surface

This page is the thin adapter contract for 0.46.0. The current language is
0.46.0; the CLI's machine-readable reports use a common JSON envelope. The
CLI is the boundary, not a chat app, session manager, MCP server, or
Lute-specific agent protocol
(`docs/design/architecture-direction.md:225-241`).

## Sequence

1. **Inspect:** `lute context <dir> --target <kind:key> --json` (or
   `lute context <file> --at <file>:<line>:<col> --json`).
2. **Plan:** construct a patch with the returned `projectRevision`, target
   keys, edits, and explicit `preserve` items.
3. **Patch:** `lute patch <dir> patch.json --dry-run --json`.
4. **Check/review:** inspect the semantic diff and diagnostics; apply the same
   patch without `--dry-run` only when the result is intended.
5. **Replay:** use the project's existing play/test/trace surface as the
   reference executor; a patch does not claim runtime success merely because
   static checks pass.

Every report JSON object has `command`, `schemaVersion`, and `ok`; `ok` is
`true` exactly when the process exits 0. A refusal or I/O failure has
`ok: false`; JSON-mode I/O failures use the `0.43.0.error` envelope and
retain the command name and structured `error`. `run --json` and
`play --events` are deliberately unwrapped runtime/artifact streams.


## Revision contract

`file` revisions are SHA-256 of exact bytes. `projectRevision` is SHA-256 over
sorted, length-delimited `(root-relative-path, fileRevision)` pairs for every
input read by the model. Context, patch, and diff return revisions. A patch
with a stale project or asserted file revision exits 2 with `E-PATCH-STALE` and
writes nothing.

## Context JSON

The context result contains `schemaVersion`, `projectRevision`, `files`,
`target`, `declared`, `references.in`/`out`, `affected`, `tests`, `plays`,
`vocabulary`, and `notIncluded`. The authoring-surface form
(`lute context <file> --json`) reports the `capabilitySnapshot` its vocabulary
was resolved under, the same value as the compiled IR envelope.
Static tests/plays are only scripts whose
steps or expectations name the target (scene/beat/entry/quest ID or lineId).
`witnessed` is reserved for scripts actually executed with `--run`; all other
scripts are listed in `notIncluded` with a count and reason. The result is
deterministic and bounded by `--max-items`; every truncation names the
collection and gives available and omitted counts. Dynamic/retrieval-only
relations and engine boundaries are explicitly listed in `notIncluded`, never
silently treated as absent.

The position query returns `expectedType`, `visibleSymbols`, resolved cursor
kind, and source span. Its resolver is the same `crates/lute-resolve` library
used by LSP features; the CLI does not implement a second symbol/type resolver.

## Patch JSON

```json
{
  "base": {
    "project": "sha256:…",
    "files": {"relative/file.lute": "sha256:…"}
  },
  "targets": ["kind:key"],
  "edits": [
    {"op":"replaceNode", "node":"kind:key", "text":"…"}
  ],
  "preserve": [
    {"ids": ["kind:key"]},
    {"lineIds": ["…"]},
    {"voiceKeys": ["…"]},
    {"choiceEffects": ["kind:key"]},
    {"rewards": ["kind:key"]},
    {"conditions": ["kind:key"]},
    {"reachability": ["kind:key"]},
    {"hostContracts": true},
    {"constraints": true}
  ]
}
```

`lute schema patch` describes this request under `inputs`
(`{argument: "PATCH", kind: "request", format: "json", fields, ops, preserve,
nodeKinds}`), read from the same table as the request parser
(`crates/lute-model/src/patch/request.rs`); the website's AI harness guide
("The patch request") documents every field, op, preserve key and node-key
kind with an example. Node keys come from `lute context --json`
(`affected[].key`, or `target.kind`:`target.key` for the target itself;
`target.key` alone is the bare key); a `replaceText` `span` has the shape of
`affected[].reasons[].sourceSpan`.

Operations are `replaceNode`, `insertBefore`, `insertAfter`, `replaceAttr`,
`removeNode`, `createFile`, `moveFile`, and exact-revision `replaceText`.
Targets must be span-bearing source nodes. Project/dependency-only nodes with
no source anchor are refused with `E-PATCH-TARGET`; schema state declarations
expose YAML spans and are editable via `replaceText`. The patch stages all
edits, formats touched regions, rebuilds/checks the model, computes semantic
diff, enforces preserve, then writes all files or none. Every edit of a file is
resolved against the original text and the file's touched regions are mapped
through all of its edits, so several edits in one file do not shift each other
(0.47). Before `accepted`, each region-formatted `.lute` file must satisfy: a
file that was `lute fmt --check` clean is still clean; in any other file,
formatting the touched lines again changes nothing. A violation refuses the
patch with `E-PATCH-FORMAT` naming the file.

An accepted patch exits 1 when it changes semantics (review the reported diff)
and 0 when it doesn't. Refusal codes are `E-PATCH-STALE`, `E-PATCH-TARGET`,
`E-PATCH-EDIT`, `E-PATCH-FORMAT`, `E-PATCH-CHECK`, and `E-PATCH-PRESERVE`; each refusal exits 2
and includes structured details. I/O failures exit 2 with `error.kind: "io"`
and no code. `--dry-run` performs every stage except writing.

## Diff JSON

`lute diff <before> <after> --json` accepts directories or `git:<rev>` sides.
Git materialization admits only regular files/directories, refuses symlinks,
absolute paths, and `..` archive entries, and deletes its private temporary
directory afterward. When the other side is a directory inside the current
work tree, the git side is archived from that subtree only (`git -C <toplevel>
archive <rev> -- <path>`) and rooted there, so special files elsewhere in the
repository do not matter (the path is literal; a directory missing at that
revision is `E-CLI-IO` naming both); otherwise (the other side outside the repository, or
both sides `git:`) the current directory's tree is archived (0.47). The result includes both revisions and a sorted `changes`
array. Changes match by NodeKey and report added, removed, moved, or per-node
semantic fields; formatting-only changes produce an empty array. Guards compare
as exact-profile canonical CEL ASTs (literal kind preserved); unparsable
conditions produce `conditionUnparsable` and fail relevant preserve checks.
Scheduling inputs and authored-content reward identity are compared as
specified by the proposal. Spans and build-local command `position` do not make a
semantic change.

Consumers MUST pin the returned JSON `schemaVersion`, treat refusal as a
failure rather than retrying with a new base, and retain the diff/diagnostics
for human review. Unknown fields are not silently interpreted in this pre-1.0
internal surface.

## Complete worked loop

First snapshot the source (and do this before composing any edit):

```console
$ lute tag /tmp/drowned-crown
$ lute context /tmp/drowned-crown --target quest:libraryKey --max-items 20 --json > context.json
```

`lute tag` is an editing prerequisite when a line has no explicit `lineId`.
It writes stable positional line IDs (the position is part of the identity);
do not invent IDs in a patch or renumber existing lines.

Inspect `context.json`, then plan a request whose `base.project` and
`base.files` are copied exactly from it. Preview it without writing:

```console
$ lute patch /tmp/drowned-crown planned-patch.json --dry-run --json
```

The successful patch JSON has `schemaVersion: "0.43.0.patch"`, `accepted`,
`ok`, `before`, `after`, `diff`, and `writes`. An accepted patch with semantic
changes has `accepted: true` but `ok: false` and exits 1; formatting-only
accepted patches have `ok: true`. `diff.changes` is the semantic review
surface; formatting-only edits have no changes. Apply the identical request
only after review:

```console
$ lute patch /tmp/drowned-crown planned-patch.json --json
$ lute check-project /tmp/drowned-crown
```

Replay with the project's own `lute test`, `lute play`, or trace scripts; static
checking is not runtime proof. Finally compare the resulting revision and
semantic diff:

```console
$ lute diff /tmp/drowned-crown-before /tmp/drowned-crown --json
```

For cursor-oriented inspection use `lute context <dir> --at <file>:<line>:<column>
--max-items N [--run <FILE>] --json`. `--run` requires a script file; its
executed scripts are reported as `witnessed`, while omitted scripts are
explicitly reported in `notIncluded`.
