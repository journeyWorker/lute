# Editor Support — VS Code + Neovim + Oh My Pi — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development.
> **DOCS-IN-SYNC:** update READMEs (editors/ + root) in the same commits.

**Goal:** Ship editor clients so `lute-lsp` (the built stdio LSP) delivers Lute language support in **VS Code**, **Neovim**, and the **Oh My Pi** harness. Client glue only — no checker/LSP-server changes.

**Architecture:** `lute-lsp` is a stdio LSP (diagnostics/hover/completion/definition/references/folding/symbols/semantic-tokens, project/plugin/uses/extends/components-aware). Each editor needs (1) `.lute` → `lute` language association and (2) a way to launch `lute-lsp`. Highlighting comes from the LSP's semantic tokens plus, per editor, a TextMate grammar (VS Code) or the tree-sitter grammar (`tree-sitter-lute/`, Neovim). The `lute-lsp` binary is expected on `PATH` via `cargo install --path crates/lute-lsp`.

**Tech Stack:** VS Code extension (Node, `vscode-languageclient`), Neovim (Lua), OMP LSP config (JSON), TextMate grammar (JSON).

## Global Constraints
- `export PATH="$HOME/.cargo/bin:$PATH"` every shell. Worktree `/Users/journey/Workspace/lute/.worktrees/lute-lsp-rust` (branch `feat/lute-lsp-rust`); ABSOLUTE worktree paths for edits; cargo/git cwd = worktree.
- Client/config/docs only — NO changes to `crates/` source. All new files committable (`.omp/`, `editors/`, root not gitignored; `/target`, `/.superpowers/` are).
- Everything references `lute-lsp` on PATH (installed via `cargo install --path crates/lute-lsp`) — do NOT hardcode absolute machine paths in committed config.
- Validate what's validatable headless (JSON parses; `node --check` JS; `nvim --headless` loads Lua; `npm install` succeeds). VS Code full activation can't be tested headless — validate structure + document install.

## Prerequisite (document in READMEs; do NOT run in the build)
`cargo install --path crates/lute-lsp` (→ `~/.cargo/bin/lute-lsp`) and `cargo install --path crates/lute-cli` (→ `lute`). Or `cargo build` + add `target/debug` to PATH.

---

## Task E1: Oh My Pi LSP registration
**Files:** create `.omp/lsp.json` (repo root).
Per `omp://lsp-config.md`, register a custom server so the OMP `lsp` tool auto-detects `lute-lsp` for `.lute` when the binary is on PATH and a root marker is present:
```json
{
  "servers": {
    "lute-lsp": {
      "command": "lute-lsp",
      "fileTypes": [".lute"],
      "rootMarkers": ["lute.project.yaml", ".git"]
    }
  }
}
```
- [ ] Write `.omp/lsp.json` (valid JSON — `python3 -m json.tool` or `node -e` to confirm parses).
- [ ] Commit: `feat(editors): register lute-lsp with Oh My Pi for .lute (omp lsp-config)`

## Task E2: Neovim client
**Files:** create `editors/nvim/plugin/lute.lua`, `editors/nvim/queries/lute/highlights.scm` (if a minimal highlight query is feasible from the tree-sitter grammar), `editors/nvim/README.md`.
- `plugin/lute.lua`: `vim.filetype.add({ extension = { lute = "lute" } })` + an autocmd on FileType `lute` that calls `vim.lsp.start({ name = "lute-lsp", cmd = { "lute-lsp" }, root_dir = vim.fs.dirname((vim.fs.find({ "lute.project.yaml", ".git" }, { upward = true }) or {})[1]) or vim.fn.getcwd() })`. Guard against a missing `lute-lsp` on PATH (only start if `vim.fn.executable("lute-lsp") == 1`).
- README: (a) drop `plugin/lute.lua` into a runtimepath dir OR the nvim-lspconfig snippet; (b) tree-sitter: register `tree-sitter-lute` as a parser for nvim-treesitter (a `parser_config.lute = { install_info = { url = "<repo>/tree-sitter-lute", files = {"src/parser.c"} }, filetype = "lute" }` snippet) for syntax highlighting; note the grammar lives in `tree-sitter-lute/`.
- [ ] Validate: `nvim --headless -c 'luafile editors/nvim/plugin/lute.lua' -c 'echo "ok"' -c 'q'` loads with no error (if `nvim` is available; else `luac`/lua syntax check or skip with a note).
- [ ] Commit: `feat(editors): Neovim client (filetype + lute-lsp autostart + tree-sitter notes)`

## Task E3: VS Code extension
**Files:** `editors/vscode/{package.json, extension.js, language-configuration.json, syntaxes/lute.tmLanguage.json, README.md, .vscodeignore, .gitignore}`.
- `package.json`: `name: "lute"`, `engines.vscode`, `activationEvents: ["onLanguage:lute"]`, `main: "./extension.js"`, `contributes.languages: [{ id: "lute", extensions: [".lute"], aliases: ["Lute"], configuration: "./language-configuration.json" }]`, `contributes.grammars: [{ language: "lute", scopeName: "source.lute", path: "./syntaxes/lute.tmLanguage.json" }]`, `dependencies: { "vscode-languageclient": "^9" }`, scripts.
- `extension.js`: on activate, start a `LanguageClient` with `serverOptions = { command: "lute-lsp", transport: TransportKind.stdio }` (or `{ run/debug: { command: "lute-lsp" } }`) and `clientOptions = { documentSelector: [{ scheme: "file", language: "lute" }] }`; deactivate stops it. Plain JS (no TS build) to keep it runnable after `npm install`.
- `language-configuration.json`: `comments: { blockComment: ["/*", "*/"] }`, brackets/autoClosingPairs for `()[]{}<>` and quotes.
- `syntaxes/lute.tmLanguage.json`: minimal TextMate grammar (scopeName `source.lute`) highlighting: `## ...` shot headings, `::name` directives, `<tag ...>`/`</tag>` blocks, `@ref`, `/* */` comments, `:line[speaker]`. Baseline only; LSP semantic tokens refine.
- `.gitignore`: `node_modules/`, `*.vsix`. README: `npm install`, F5 to run / `vsce package` + `code --install-extension lute-*.vsix`; requires `lute-lsp` on PATH.
- [ ] Validate: all JSON files parse; `node --check editors/vscode/extension.js`; `cd editors/vscode && npm install` succeeds (network). If npm/network unavailable, document + still validate JSON/JS syntax.
- [ ] Commit: `feat(editors): VS Code extension (lute-lsp client + TextMate grammar + language config)`

## Task E4: Docs
**Files:** create `editors/README.md`; modify root `README.md`.
- `editors/README.md`: overview — install `lute-lsp` (cargo install), then per-editor pointers (VS Code / Neovim / Oh My Pi), what language features you get, and that highlighting = semantic tokens + (VS Code TextMate | Neovim tree-sitter).
- root `README.md`: add an "Editor support" section linking `editors/` + the `.omp/lsp.json`.
- [ ] Commit: `docs(editors): editor-support README + root README section`

## Verification (controller, after review)
- `.omp/lsp.json` valid; the OMP `lsp` tool serves `.lute` once `lute-lsp` is on PATH (controller installs the binary + exercises the `lsp` tool on `docs/examples/showcase/episode01.lute`).
- `node --check` + JSON parse for VS Code; `nvim --headless` load for Neovim (if available).
- `cargo test --workspace` unaffected (no crate source touched).

## Self-Review
- Three clients, all launching `lute-lsp` on PATH; `.lute` associated; highlighting path per editor.
- No machine-specific absolute paths committed; prerequisite documented.
- READMEs (editors + root) synced.
