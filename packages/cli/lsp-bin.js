#!/usr/bin/env node
// `lute-lsp` entry: same launcher (platform detection, binary resolution,
// argv/exit-code passthrough) as `bin.js`, aimed at the LSP binary the
// platform package ships alongside `lute`.
process.env.LUTE_LAUNCHER_BINARY = "lute-lsp";
await import("./dist/index.js");
