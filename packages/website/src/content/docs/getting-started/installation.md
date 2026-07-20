---
title: Installation
description: Install the Lute CLI with bunx, a global bun install, or from Rust source, then verify the toolchain and move on to your first scene.
---

Lute ships as a single command-line tool, `lute`. It reads `.lute` scenario files and checks,
compiles, traces, and inspects them. The current language version is **0.7.0**.

## Quick start with `bunx`

The fastest way to run Lute without installing anything permanently is `bunx`, which fetches the
published npm package and runs its bundled native binary:

```sh
bunx @lute-lang/lute check scene.lute
```

The npm package is named `@lute-lang/lute`; the command it installs is `lute`. `bunx @lute-lang/lute <args>` and a
globally installed `lute <args>` are the same program.

## Global install

To keep `lute` on your `PATH` for everyday use, install the package globally with bun:

```sh
bun add -g @lute-lang/lute
lute check scene.lute
```

`@lute-lang/lute` is a thin launcher: it detects your platform and dispatches to a prebuilt native binary
shipped as a platform-specific optional dependency (`@lute-lang/lute-core-darwin-arm64` or
`@lute-lang/lute-core-linux-x64`). The correct one is selected automatically at install time.

## Platform support

| Platform | npm core package | Status |
|---|---|---|
| macOS (Apple silicon) | `@lute-lang/lute-core-darwin-arm64` | Supported |
| Linux (x86-64) | `@lute-lang/lute-core-linux-x64` | Supported |

On an unsupported platform the launcher exits with an actionable error naming the supported
matrix. Windows and musl-based Linux are not yet packaged — build from source instead.

## Building from source

Lute's compiler, checker, and CLI are written in Rust. If you have a Rust toolchain, install the
CLI directly from a checkout of the repository:

```sh
cargo install --path crates/lute-cli
```

This builds the `lute` binary (the crate declares `[[bin]] name = "lute"`) and places it in your
Cargo bin directory. For a throwaway local build during development, `cargo build -p lute-cli`
produces `./target/debug/lute`.

## Verify

Whichever route you took, confirm the tool is on your `PATH`:

```sh
lute --version
```

## Next

Head to [Write your first scene](/getting-started/first-scene/) to build a real `.lute` file from
an empty file, running `lute` at every step.
