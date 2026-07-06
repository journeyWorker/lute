# Editor support

Editor clients for the Lute scenario DSL (`.lute`). All three clients associate
`.lute` with the `lute` language and drive the same `lute-lsp` stdio language
server, so you get identical language intelligence everywhere.

## Prerequisite: install `lute-lsp`

Every client launches the `lute-lsp` binary from your `PATH`. Install it once:

```sh
cargo install --path crates/lute-lsp     # -> ~/.cargo/bin/lute-lsp
cargo install --path crates/lute-cli     # optional: the `lute` CLI checker
```

Or, for a dev checkout, `cargo build -p lute-lsp` and add `target/debug` to your
`PATH`. Confirm it resolves:

```sh
command -v lute-lsp
```

## Clients

| Editor | Setup | Highlighting |
| --- | --- | --- |
| **VS Code** | [`vscode/`](vscode) — `npm install`, then <kbd>F5</kbd> or `vsce package` + `code --install-extension`. See [`vscode/README.md`](vscode/README.md). | TextMate grammar ([`vscode/syntaxes/lute.tmLanguage.json`](vscode/syntaxes/lute.tmLanguage.json)) + LSP semantic tokens |
| **Neovim** | [`nvim/`](nvim) — drop `plugin/lute.lua` on `runtimepath` or use the nvim-lspconfig snippet. See [`nvim/README.md`](nvim/README.md). | tree-sitter grammar ([`../tree-sitter-lute/`](../tree-sitter-lute)) + LSP semantic tokens |
| **Oh My Pi** | [`../.omp/lsp.json`](../.omp/lsp.json) — auto-detects `lute-lsp` for `.lute` when the binary is on `PATH` and a root marker (`lute.project.yaml` / `.git`) is present. Zero extra setup. | LSP semantic tokens |

## Language features (from `lute-lsp`)

Once the server is running you get:

- **Diagnostics** — the full checker (project / plugin / `uses` / `extends` /
  components-aware), pushed as you type.
- **Hover** — types and docs for directives, refs, state paths, and attributes.
- **Completion** — directives, attributes, `@ref`s, state paths, choice ids.
- **Go-to-definition / references** — defs, components, schema declarations.
- **Folding & document symbols** — shots, timelines, branches, matches.
- **Semantic tokens** — layer-aware highlighting (content / staging / logic).

## Highlighting model

Highlighting is two layers that combine:

1. A **static grammar** per editor for an instant baseline — a **TextMate**
   grammar in VS Code, the **tree-sitter** grammar (`tree-sitter-lute/`) in
   Neovim.
2. **LSP semantic tokens** from `lute-lsp`, which refine the static grammar with
   project/plugin/schema knowledge the static grammar cannot see.

> **Note:** refreshing the tree-sitter grammar ([`../tree-sitter-lute/`](../tree-sitter-lute)) for
> DSL 0.1.0 is deferred to a later plan — the tree-sitter grammar update is out of scope for the
> 0.1.0 parser cutover (Plan A). Until then, `lute-lsp` semantic tokens carry the authoritative
> 0.1.0 highlighting.

Root markers used to locate a project: `lute.project.yaml`, then `.git`.
