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

> **Note:** the tree-sitter grammar ([`../tree-sitter-lute/`](../tree-sitter-lute)) is
> **0.4.0-current** — it parses `@speaker{attrs}: text` content lines, `//` line comments,
> `{{…}}` interpolation, `<hub>` revisit blocks, `<when is>` literal patterns, the
> attrs-free `<otherwise>`, `<quest>`/`<objective>`/`<on>` nesting, the 0.3.0 relational-fact
> leaves `::assert{…}`/`::retract{…}` (`fact_pattern`/`fact_arg`/`wildcard`, dsl 0.3.0 §5 +
> Appendix C), and 0.4.0's `when=` content-line guard + param-scoped `<match>` in component
> bodies. It is the Neovim baseline highlighting/folding host; `lute-lsp` semantic tokens
> stay authoritative for the project/plugin/schema-aware refinement the static grammar cannot
> see (`::assert`/`::retract` get static-grammar coloring only — see
> [`../docs/architecture.md`](../docs/architecture.md#relational-facts--datalog-derivation-030)
> for the full 0.3.0 relational-layer surface).
>
> **0.4.0 adds zero new grammar nodes** (dsl 0.4.0 §3 B1 — vocabulary-and-tooling only,
> pinned by `tree-sitter-lute/test/corpus/writer_experience.txt` as a stable-tree regression
> test, not an assumption): `when=` is an ordinary `cel_attr` — parsed as
> `(cel_attr (cel_key) (cel_string …))`, identical to `<match on>`/`<when test>`/
> `<choice when>` — and a component-body `<match on="@tier">` is an ordinary `match`/`when`
> tree. Both therefore highlight for free through the existing generic captures in
> [`nvim/queries/lute/highlights.scm`](nvim/queries/lute/highlights.scm) — `(cel_attr
> (cel_key) @attribute)` for the `when` key, `(cel_string) @embedded` / `(cel_string (path)
> @property)` for its guard value — with no query changes, and no TextMate grammar changes,
> required for either 0.4.0 surface. See
> [`../docs/architecture.md`](../docs/architecture.md#writer-experience-040) for the full
> 0.4.0 writer-experience surface.

Root markers used to locate a project: `lute.project.yaml`, then `.git`.
