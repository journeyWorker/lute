# Lute for Neovim

Neovim client for the Lute scenario DSL: `.lute` filetype association, `lute-lsp`
autostart, and tree-sitter syntax highlighting.

## Prerequisite

Install the language server so `lute-lsp` is on your `PATH`:

```sh
cargo install --path crates/lute-lsp   # -> ~/.cargo/bin/lute-lsp
```

(Alternatively `cargo build -p lute-lsp` and add `target/debug` to `PATH`.) See
[`../README.md`](../README.md) for the full feature list. The plugin degrades
gracefully when `lute-lsp` is absent: `.lute` files still get the `lute`
filetype and tree-sitter highlighting, just no language server.

## 1. Install the plugin

`plugin/lute.lua` registers the `.lute` → `lute` filetype and starts `lute-lsp`
(over stdio) for every Lute buffer, once per project root. Pick one:

### Option A — drop-in `runtimepath`

Add this directory to `runtimepath` early in your config. This also surfaces the
tree-sitter queries in [`queries/lute/`](queries/lute) (see step 2):

```lua
vim.opt.runtimepath:append("/path/to/lute/editors/nvim")
```

Or with a plugin manager pointed at the repo, e.g. lazy.nvim:

```lua
{ dir = "/path/to/lute/editors/nvim", ft = "lute" }
```

### Option B — copy the file

Copy `plugin/lute.lua` to `~/.config/nvim/plugin/lute.lua`.

### Option C — nvim-lspconfig snippet

If you prefer to wire the server yourself (skip `plugin/lute.lua`'s autostart),
register a custom config. On a recent nvim-lspconfig / `vim.lsp.config` setup:

```lua
vim.filetype.add({ extension = { lute = "lute" } })

vim.lsp.config("lute_lsp", {
  cmd = { "lute-lsp" },
  filetypes = { "lute" },
  root_markers = { "lute.project.yaml", ".git" },
})
vim.lsp.enable("lute_lsp")
```

On older Neovim, the classic nvim-lspconfig `configs` API works too:

```lua
local lspconfig = require("lspconfig")
local configs = require("lspconfig.configs")
if not configs.lute_lsp then
  configs.lute_lsp = {
    default_config = {
      cmd = { "lute-lsp" },
      filetypes = { "lute" },
      root_dir = lspconfig.util.root_pattern("lute.project.yaml", ".git"),
    },
  }
end
lspconfig.lute_lsp.setup({})
```

## 2. Syntax highlighting (tree-sitter)

The LSP already provides semantic-token highlighting. For full syntax
highlighting, register the tree-sitter grammar that ships in this repo under
[`tree-sitter-lute/`](../../tree-sitter-lute) with nvim-treesitter:

```lua
local parser_config = require("nvim-treesitter.parsers").get_parser_configs()
parser_config.lute = {
  install_info = {
    url = "/path/to/lute/tree-sitter-lute", -- or the git URL of this repo, with `location = "tree-sitter-lute"`
    files = { "src/parser.c", "src/scanner.c" },
  },
  filetype = "lute",
}
```

Then install and enable the parser:

```vim
:TSInstall lute
```

nvim-treesitter compiles the parser but does **not** copy the grammar's query
files. Make them available in `runtimepath` under `queries/lute/`. Two ways:

- **Option A (drop-in):** if you added `editors/nvim` to `runtimepath` in step 1,
  the queries in [`queries/lute/`](queries/lute) are already found — nothing else
  to do.
- **Option B (copy):** copy the grammar's queries (the source of truth) into your
  config:

  ```sh
  mkdir -p ~/.config/nvim/queries/lute
  cp /path/to/lute/tree-sitter-lute/queries/*.scm ~/.config/nvim/queries/lute/
  ```

The grammar and its canonical queries (`highlights.scm`, `folds.scm`, `tags.scm`)
live in [`tree-sitter-lute/`](../../tree-sitter-lute); the copies under
[`queries/lute/`](queries/lute) mirror them for the drop-in path.
