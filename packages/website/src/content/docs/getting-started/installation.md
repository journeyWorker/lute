---
title: Installation
description: Install the Lute CLI with bunx, a global bun install, or from Rust source, then verify the toolchain and move on to your first scene.
---

Lute ships as a single command-line tool, `lute`. It reads `.lute` scenario files and checks,
compiles, traces, and inspects them. The current language version is **0.15.0**.

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

```
$ lute version
lute toolchain 0.11.0
language      0.15.0
IR schema     0.11.0
```

Those are three independent axes, and you will see all three again elsewhere: the **toolchain**
version is this CLI, the **language** version is the grammar and semantics the checker enforces,
and the **IR schema** version is what `lute compile` stamps into every artifact as `irVersion`.
They mean different things, but a release always **re-aligns all three visible numbers** to that
release's number, so you never reconcile three
([versioning policy](https://github.com/journeyWorker/lute/blob/main/docs/versioning.md)).
At `0.11.0` only the **toolchain** substantively moved — a new `schedule.yaml` layer and
`lute play` command, plus two reference-runner fixes. The IR carries no shape or content change at
all, but its `major.minor` still moves (`0.10` → `0.11`, because a release re-aligns every visible
number whether or not its contract changed), and engines still gate on `irVersion` by
major.minor — so an engine implementing IR `0.10` must widen its gate to `0.11` before it accepts
a `0.11.0` artifact, even though nothing inside it is different.

For scripts and CI, `--json` prints the same three axes as one object:

```
$ lute version --json
{"toolchain":"0.15.0","language":"0.15.0","ir":"0.15.0"}
```

(`lute --version` also works and prints just `lute 0.11.0` — the toolchain axis alone.)

## Next

Head to [Write your first scene](/getting-started/first-scene/) to build a real `.lute` file from
an empty file, running `lute` at every step.
