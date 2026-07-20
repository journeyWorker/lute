# Versioning policy

Lute is versioned along several **independent** axes. A single "version" number
would conflate things that move at different speeds — the grammar an author
writes against, the binary they install, and the artifact schema an engine
consumes are separate contracts. This document names each axis, says which
change bumps which, and states the pre-1.0 breaking-change policy.

## Axes

| Axis | Where it lives | Current | What a bump means |
|---|---|---|---|
| **Toolchain** | Cargo workspace version (`CARGO_PKG_VERSION`); `lute version` | `0.7.0` | A release of the CLI, checker, compiler, and LSP shipping together, and the npm launcher that distributes them. Tracked in [`CHANGELOG.md`](../CHANGELOG.md). |
| **Language** | [`lute_check::LUTE_LANG_VERSION`](../crates/lute-check/src/lib.rs); `luteVersion:` frontmatter | `0.7.0` | A change to the grammar or static semantics the checker enforces. History is the versioned spec stack under [`docs/proposals/scenario-dsl/`](proposals/scenario-dsl/). |
| **IR** | `irVersion` field of every compiled artifact ([`lute_compile::LUTE_IR_VERSION`](../crates/lute-compile/src/lib.rs)) | `0.7.0` | A change to the compiled JSON artifact schema ([`schemas/lute-ir-0.7.schema.json`](../schemas/lute-ir-0.7.schema.json)). Consuming engines gate parsing on it. |
| **Capability** | `capabilityVersion` in resolved provider/plugin snapshots | — | A change to the built-in `lute.core` capability surface (directives, state shapes, providers, bridge signatures) a document resolves against. |
| **Plugin** | each plugin manifest's own version | — | A change to a specific plugin's declared capabilities, independent of core. |

The **language and toolchain versions are independent**: a toolchain release
need not advance the language (e.g. a new CLI subcommand or a bug fix), and a
language delta can ship under any toolchain version. Likewise the IR version
bumps on a pure artifact-shape change even when the grammar is untouched.

**Aligned as of `0.7.0`.** Although the axes are independent, they had drifted
to different visible numbers (language/IR `0.6.1`, toolchain `0.2.0`). The
`0.7.0` release **re-aligns every axis at one number** so a single release
presents a single number and users stop reconciling three. Going forward the
policy is: a release that changes any axis re-aligns the visible numbers to that
release's number; the axes MAY still drift apart again only when an axis
genuinely does not change (e.g. a toolchain-only bug-fix release leaves the
language and IR numbers where they are). Alignment is a presentation guarantee,
not a merge of the axes — each still means exactly what its row above says.

## Which bump when

- Fix a checker/compiler/LSP bug, add a CLI flag, ship a new prebuilt target →
  **toolchain** only.
- Add or change grammar or static semantics → **language** (a new spec-stack
  delta), and usually **toolchain** (the release that carries it).
- Change the compiled artifact's shape → **IR**, and **toolchain**.
- Change the built-in core capability surface → **capability** (and whatever
  language/IR follows from it).
- Change a plugin's declared surface → that **plugin**'s version.

## Breaking-change policy (pre-1.0)

The language is **draft** (see below), so while it is pre-1.0:

- Breaking grammar or semantic changes **may** land in a minor language version
  (e.g. `0.5.x` → `0.6.0`); we do not promise grammar stability before `1.0`.
- Every breaking change ships a **migration path** via `lute fix` wherever the
  rewrite is mechanical (`lute fix` migrates a document in place — see its
  entry in `lute --help`). A change that cannot be migrated mechanically is
  called out in the spec delta and the changelog.
- The checker emits `W-LUTE-VERSION-STALE` when a document's `luteVersion:`
  stamp lags the checker's language version, so drift is visible, never silent.

## What "draft" means

The language being **draft** is a statement about the *grammar contract*, not
about implementation maturity:

- **Grammar may break.** New minor language versions may change or remove
  syntax, subject to the migration policy above.
- **The compiler is real and tested.** The checker, compiler, provider/plugin
  resolver, LSP, and CLI are implemented Rust crates with test suites — not a
  prototype or a stub.
- **Production stability is not yet guaranteed.** Because the grammar and
  artifact schema may still move before `1.0`, we do not yet promise a stable
  contract for production pipelines. Pin the toolchain version and validate
  compiled artifacts against the `irVersion` you target.

## Supported platforms

Prebuilt native binaries are distributed via the [`@lute-lang/lute`](https://www.npmjs.com/package/@lute-lang/lute)
npm launcher for:

- `darwin-arm64` (macOS, Apple Silicon)
- `linux-x64`
- `win32-x64` (Windows, x86-64)

Any other platform can build from source with `cargo install --path crates/lute-cli`.
