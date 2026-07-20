# Lute for VS Code

VS Code extension for the Lute scenario DSL: `.lute` language association,
`lute-lsp` client (diagnostics, hover, completion, go-to-definition, references,
folding, symbols, semantic-token highlighting), a TextMate grammar for baseline
syntax highlighting, and comment/bracket configuration.

## Prerequisite: the `lute-lsp` server binary

The extension spawns the `lute-lsp` language server over stdio. Install it so it
is discoverable:

```sh
cargo install --path crates/lute-lsp   # -> ~/.cargo/bin/lute-lsp
```

(Alternatively `cargo build -p lute-lsp` and add `target/debug` to `PATH`.)

### How the binary is resolved

On activation the extension looks for `lute-lsp` in this order:

1. **The `lute.lsp.path` setting** — set it to an absolute path to the binary
   (Settings → *Lute: Lsp: Path*, or `"lute.lsp.path": "/abs/path/to/lute-lsp"`
   in `settings.json`). Use this when the server is not on `PATH`.
2. **`PATH`** — when the setting is empty (the default), the plain `lute-lsp`
   command is spawned and resolved through your `PATH`.

If neither resolves, `client.start()` fails and the extension shows an error
toast naming where it looked and how to fix it (install the binary or set
`lute.lsp.path`). Baseline TextMate highlighting still works without the server.

> **Planned: auto-download.** A future release will optionally download a
> `lute-lsp` build matching the extension version into the extension's global
> storage when neither the setting nor `PATH` resolves. It is **not** implemented
> in this release — the two-step resolution above is the current behavior.

## Develop / run from source

```sh
cd editors/vscode
npm install          # pulls vscode-languageclient
```

Then open this folder in VS Code and press <kbd>F5</kbd> ("Run Extension") to
launch an Extension Development Host with the Lute extension loaded. Open any
`.lute` file (e.g. `docs/examples/showcase/episode01.lute`) to activate it.

## Package / install

```sh
cd editors/vscode
npx @vscode/vsce package                 # produces lute-<version>.vsix
code --install-extension lute-*.vsix     # install into your VS Code
```

`node_modules/` and `*.vsix` are gitignored; the production `vscode-languageclient`
dependency is bundled into the `.vsix` by `vsce`.

## What's in here

| File | Purpose |
| --- | --- |
| `package.json` | Extension manifest: `lute` language + `.lute` association, grammar contribution, `vscode-languageclient` dependency. |
| `extension.js` | Plain-JS activation: starts the `lute-lsp` stdio `LanguageClient` for `language: "lute"`. |
| `language-configuration.json` | Block comments `/* */`, brackets, auto-closing/surrounding pairs. |
| `syntaxes/lute.tmLanguage.json` | Baseline TextMate grammar (`source.lute`): frontmatter, `#`/`##` headings, `::directives`, `<tags>`, `@ref`, `/* */` comments, `:line[speaker]`, attributes. LSP semantic tokens refine on top. |

## Highlighting

Two layers combine:

1. **TextMate grammar** (`syntaxes/lute.tmLanguage.json`) — static, instant baseline.
2. **LSP semantic tokens** — the `lute-lsp` server refines highlighting with
   project/plugin/schema awareness.
