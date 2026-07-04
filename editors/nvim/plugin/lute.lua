-- Lute (.lute) editor support for Neovim: filetype association + lute-lsp autostart.
--
-- Drop-in: put this file where Neovim's 'runtimepath' can see it, e.g. either
--   * add `<repo>/editors/nvim` to 'runtimepath' (also surfaces the tree-sitter
--     queries under editors/nvim/queries — see editors/nvim/README.md), or
--   * copy it to ~/.config/nvim/plugin/lute.lua.
--
-- Language intelligence (diagnostics, hover, completion, go-to-definition,
-- references, folding, symbols, semantic-token highlighting) comes from the
-- `lute-lsp` stdio server, which must be on PATH (see editors/README.md for the
-- `cargo install --path crates/lute-lsp` prerequisite). When `lute-lsp` is not
-- installed the plugin degrades gracefully: `.lute` files still get the `lute`
-- filetype (and tree-sitter highlighting if configured), just no LSP.

if vim.g.loaded_lute then
  return
end
vim.g.loaded_lute = true

-- 1. Associate the `.lute` extension with the `lute` filetype.
vim.filetype.add({ extension = { lute = "lute" } })

-- Project root markers, most-specific first (matches .omp/lsp.json + the plan).
local root_markers = { "lute.project.yaml", ".git" }

-- 2. On entering a `lute` buffer, launch the `lute-lsp` stdio server. Guarded so
--    nothing happens when the binary is absent. vim.lsp.start reuses an existing
--    client with the same name + root_dir, so one server serves the whole project.
local group = vim.api.nvim_create_augroup("lute_lsp", { clear = true })
vim.api.nvim_create_autocmd("FileType", {
  group = group,
  pattern = "lute",
  desc = "Start lute-lsp for Lute buffers",
  callback = function(args)
    if vim.fn.executable("lute-lsp") ~= 1 then
      return
    end
    local fname = vim.api.nvim_buf_get_name(args.buf)
    local start = fname ~= "" and vim.fs.dirname(fname) or vim.fn.getcwd()
    local marker = vim.fs.find(root_markers, { upward = true, path = start })[1]
    local root_dir = marker and vim.fs.dirname(marker) or start
    vim.lsp.start({
      name = "lute-lsp",
      cmd = { "lute-lsp" },
      root_dir = root_dir,
    })
  end,
})
