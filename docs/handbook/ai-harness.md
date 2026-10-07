---
title: "AI harness surface — 0.37.0"
status: Draft
---

# AI harness surface

This page is the thin adapter contract for 0.37.0. The current language is
`docs/proposals/scenario-dsl/0.37.0.md`; the context/patch/diff source-edit
contract it relies on was specified in `docs/proposals/scenario-dsl/0.35.0.md`
and `0.36.0.md`, and 0.37.0 leaves it unchanged. This page only gives a harness
the stable command sequence and JSON boundaries. The CLI is the boundary, not a
chat app, session manager, MCP server, or Lute-specific agent protocol
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

Operations are `replaceNode`, `insertBefore`, `insertAfter`, `replaceAttr`,
`removeNode`, `createFile`, `moveFile`, and exact-revision `replaceText`.
Targets must be span-bearing source nodes. Project/dependency-only nodes with
no source anchor are refused with `E-PATCH-TARGET`; schema state declarations
expose YAML spans and are editable via `replaceText`. The patch stages all
edits, formats touched regions, rebuilds/checks the model, computes semantic
diff, enforces preserve, then writes all files or none.

An accepted patch exits 1 when it changes semantics (review the reported diff)
and 0 when it doesn't. Refusal codes are `E-PATCH-STALE`, `E-PATCH-TARGET`,
`E-PATCH-EDIT`, `E-PATCH-CHECK`, and `E-PATCH-PRESERVE`; each refusal exits 2
and includes structured details. I/O failures exit 2 with `error.kind: "io"`
and no code. `--dry-run` performs every stage except writing.

## Diff JSON

`lute diff <before> <after> --json` accepts directories or `git:<rev>` sides.
Git materialization admits only regular files/directories, refuses symlinks,
absolute paths, and `..` archive entries, and deletes its private temporary
directory afterward. The result includes both revisions and a sorted `changes`
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

The successful patch JSON has `schemaVersion: "0.36.0.patch"` (unchanged in
0.37.0: the patch report contract did not change, so its schema id keeps the
release that last changed it), `ok: true`,
`before`, `after`, `diff`, and `writes`. `diff.changes` is the semantic
review surface; formatting-only edits have no changes. Apply the identical
request only after review:

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
