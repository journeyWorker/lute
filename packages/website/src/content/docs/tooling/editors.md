---
title: Editors and LSP
description: Editor support for .lute — installing lute-lsp, wiring the VS Code, Neovim, and Oh My Pi clients, and the language features the server provides.
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

- **Diagnostics** — the full checker (project / plugin / `uses` / `extends` / components-aware), pushed as you type.
- **Hover** — types and docs for directives, refs, state paths, and attributes.
- **Completion** — directives, attributes, `@ref`s, state paths, choice ids.
- **Go-to-definition / references** — defs, components, schema declarations.
- **Folding & document symbols** — shots, timelines, branches, matches.
- **Semantic tokens** — layer-aware highlighting (content / staging / logic).

## Highlighting model

Highlighting is two layers that combine. A **static grammar** per editor gives an instant baseline — a TextMate grammar in VS Code, the tree-sitter grammar in Neovim. **LSP semantic tokens** from `lute-lsp` then refine it with project / plugin / schema knowledge the static grammar cannot see. The tree-sitter grammar parses `@speaker{attrs}: text` content lines, `//` comments, `{{…}}` interpolation, `<hub>` blocks, `<when is>` patterns, `<quest>`/`<objective>`/`<on>` nesting, and the `::assert`/`::retract` relational leaves; `lute-lsp`'s semantic tokens stay authoritative for the project-aware refinement.
