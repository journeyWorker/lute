---
title: Editors and LSP
description: Editor support for .lute — installing lute-lsp, wiring clients, sharing project/plugin/capability-permission diagnostics and completions with the CLI checker, and a server that notices its own binary was replaced.
---

All editor clients associate `.lute` with the `lute` language and drive the same `lute-lsp` stdio language server, so you get identical language intelligence everywhere. A project root is located by the markers `lute.project.yaml`, then `.git`.

## Install `lute-lsp`

Every client launches the `lute-lsp` binary from your `PATH`. Install it once:

```console
$ cargo install --path crates/lute-lsp     # -> ~/.cargo/bin/lute-lsp
$ cargo install --path crates/lute-cli     # optional: the `lute` CLI checker
```

For a dev checkout, `cargo build -p lute-lsp` and add `target/debug` to your `PATH`. Confirm it resolves:

```console
$ command -v lute-lsp
```

## Clients

| Editor | Setup | Static highlighting |
|---|---|---|
| **VS Code** | `editors/vscode/` — `npm install`, then <kbd>F5</kbd>, or `vsce package` + `code --install-extension`. | TextMate grammar + LSP semantic tokens |
| **Neovim** | `editors/nvim/` — drop `plugin/lute.lua` on `runtimepath`, or use the nvim-lspconfig snippet. | tree-sitter grammar (`tree-sitter-lute/`) + LSP semantic tokens |
| **Oh My Pi** | `.omp/lsp.json` — auto-detects `lute-lsp` for `.lute` when the binary is on `PATH` and a root marker is present. Zero extra setup. | LSP semantic tokens |

## Language features

Once the server is running you get:

- **Diagnostics** — the full checker (project / plugin / `uses` / `extends` / components / [capability-permission](/tooling/capability-permissions/) aware), pushed as you type. Forbidden authored effects use the same `E-PERMISSION-*` codes and spans as the CLI.
- **Hover** — types and docs for directives, refs, state paths, and attributes.
- **Completion** — directives, attributes, `@ref`s, state paths, choice ids. Directive candidates forbidden by the document's effective project/profile permissions are omitted; a bridge directive is omitted when either its directive name or its `service/operation` is denied.
- **Go-to-definition / references** — defs, components, schema declarations.
- **Folding & document symbols** — sections, timelines, branches, matches.
- **Semantic tokens** — layer-aware highlighting (content / staging / logic).

The LSP resolves the document's project, `global`/ancestor/selected profile
permissions, and capabilities through the same manifest API as the CLI. It does
not implement a second authorization algorithm. Editor filtering is guidance,
not a security boundary: a host accepting source independently pins its trusted
ceiling with CLI `--permission-profile`, then checks or compiles under that
ceiling.

## A server older than its binary

An editor starts `lute-lsp` once and keeps it running, so reinstalling the toolchain — or rebuilding a dev checkout — replaces the binary under a server that still runs the old build. Before dsl 0.26.0 that server went on publishing the old build's diagnostics, often hundreds of false ones per save. Since dsl 0.26.0 the server compares its binary file — length, modification time, inode — with the one it started from on every analysis. Once the file has been replaced or removed, it publishes one diagnostic in place of any results: severity error at the top of the document, code `lute-lsp-stale`, source `lute-lsp`, naming its own version:

> stale server: lute-lsp `<version>` was started from `<path>`, which has been replaced since, so its diagnostics would come from an older build — restart the language server

Checker-backed requests — hover, completion, go-to-definition, references and code actions — answer nothing until the server restarts, rather than answer from the old build. Restart the language server (or the editor), and the new build takes over.

[`lute doctor`](/tooling/cli/#doctor) finds the same condition from the command line — a running `lute-lsp` started before its binary was replaced, another build beside `lute`, a `lute-lsp` on `PATH` that reports another version — and `lute doctor --strict` exits 1 when any check fails, so a harness can refuse to start on a stale setup:

```console
$ lute doctor --project . --strict
…
  ✗ running lute-lsp: pid 12430 (/Users/you/.bun/bin/lute-lsp) started before its binary was replaced, so it runs an older build
      → restart the editor (or its language server) so it launches this toolchain's lute-lsp
$ echo $?
1
```

## The input cache across requests (0.53)

Before 0.53 every request — analysis on open and on change, hover, completion, go-to-definition — folded the project's base and vocabulary again, about 40 ms of CPU per request on a project of 4 800 vocabulary slots. The server now keeps **one input cache per project root** (and one for documents under no root) across requests, together with a record of everything the cache observed on disk. The analyzed document's own text is never cached: it comes from the request.

Every request validates the record before it reuses the cache, and a failed validation replaces the cache with a fresh one:

- **Saved inputs.** The manifest, schemas reached through `uses:`/`extends:`, components, plugins and providers are read from disk. A created, deleted or edited file, a symlink retargeted anywhere on its path, and a file added to or removed from a directory the cache listed (a `defaults:` glob, `catalogDir`, `pluginsDir`) all fail validation. A file whose length, modification time, change time and identity are unchanged is not re-read; a touch that leaves the content unchanged keeps the cache. A file written within two seconds of being read is compared by content.
- **Watched files.** The server registers file watchers for `**/lute.project.yaml`, `**/*.{yaml,yml,json}` and `**/*.lute` when the client supports dynamic registration, and handles `workspace/didChangeWatchedFiles`; a `textDocument/didSave` of an input does the same. A notified path is marked dirty, and the next validation compares its **content**, not its timestamps. A client without file watching still gets the per-request validation.
- **Unsaved schema edits are not read.** Schemas, plugins and components come from disk, not from open buffers — as before 0.53. Only the analyzed document's text comes from its buffer, so an edit to a schema is an input only once it is saved.
- **Retention is bounded.** The cache keeps the configurations (profile, `plugins:`, `uses:`, `extends:`) of the open documents, plus the four most recently superseded ones, so flipping between a few profiles while editing does not rebuild. Everything else is released after each request.

One edit is not detected by the request-time check: a change that preserves every field of the file's metadata (length, modification time, change time, identity) — possible only by tampering with file-system metadata, such as setting the clock back and rewriting in place. A server restart, or a watched-file or save notification for that path, revalidates it.

On a 400-scene project, the median warm request fell from 38.74 ms to 0.187 ms at 4 800 slots (0.952 ms to 0.160 ms at 150), with one base and one vocabulary build instead of one per request (`lute-bench lsp-requests`, debug build).

## Highlighting model

Highlighting is two layers that combine. A **static grammar** per editor gives an instant baseline — a TextMate grammar in VS Code, the tree-sitter grammar in Neovim. **LSP semantic tokens** from `lute-lsp` then refine it with project / plugin / schema knowledge the static grammar cannot see. The tree-sitter grammar parses `@speaker{attrs}: text` content lines, `//` comments, `{{…}}` interpolation, `<hub>` blocks, `<when is>` patterns, `<quest>`/`<objective>`/`<on>` nesting, and the `::assert`/`::retract` relational leaves; `lute-lsp`'s semantic tokens stay authoritative for the project-aware refinement.
