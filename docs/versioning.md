# Versioning policy

Lute is versioned along several **independent** axes. A single "version" number
would conflate things that move at different speeds — the grammar an author
writes against, the binary they install, and the artifact schema an engine
consumes are separate contracts. This document names each axis, says which
change bumps which, and states the pre-1.0 breaking-change policy.

## Axes

| Axis | Where it lives | Current | What a bump means |
|---|---|---|---|
| **Toolchain** | Cargo workspace version (`CARGO_PKG_VERSION`); `lute version` | `0.10.0` | A release of the CLI, checker, compiler, and LSP shipping together, and the npm launcher that distributes them. Tracked in [`CHANGELOG.md`](../CHANGELOG.md). |
| **Language** | [`lute_check::LUTE_LANG_VERSION`](../crates/lute-check/src/lib.rs); `luteVersion:` frontmatter | `0.10.0` | A change to the grammar or static semantics the checker enforces. History is the versioned spec stack under [`docs/proposals/scenario-dsl/`](proposals/scenario-dsl/). |
| **IR** | `irVersion` field of every compiled artifact ([`lute_compile::LUTE_IR_VERSION`](../crates/lute-compile/src/lib.rs)) | `0.10.0` | A change to the compiled JSON artifact schema ([`schemas/lute-ir-0.10.schema.json`](../schemas/lute-ir-0.10.schema.json)). Consuming engines gate parsing on it. |
| **Capability** | `capabilityVersion` in resolved provider/plugin snapshots | — | A change to the built-in `lute.core` capability surface (directives, state shapes, providers, bridge signatures) a document resolves against. |
| **Plugin** | each plugin manifest's own version | — | A change to a specific plugin's declared capabilities, independent of core. |

The axes are **independent in meaning**: a toolchain release need not advance
the language (e.g. a new CLI subcommand or a bug fix), a language delta can
ship under any toolchain version, and the IR version answers a pure
artifact-shape question the grammar never asks. What each number *means* is
the row above; what each number *reads* is the rule below.

## Alignment

**Aligned as of `0.7.0`, and on every release since.** The axes had drifted to
different visible numbers (language/IR `0.6.1`, toolchain `0.2.0`), and the
`0.7.0` release re-aligned them so a single release presents a single number
and users stop reconciling three. That is now the standing rule, and it has no
exceptions:

> **Every release re-aligns every visible axis number to that release's
> number.**

An axis that did not substantively change still moves. If a release leaves the
artifact shape untouched, the IR number becomes the release number anyway and
the [changelog](../CHANGELOG.md) says so explicitly — naming the bump a
**no-op for consumers**, so the number moved but the contract did not. "This
axis did not really change" is a fact for the changelog to record. It is never
a reason to hold a number back, and the axes are not permitted to drift apart
again.

Alignment is a **presentation guarantee, not a merge of the axes.** Each row
above still means exactly what it says, and each still bumps for its own
reason: `irVersion` remains the artifact-schema contract an engine gates on,
`luteVersion:` remains the grammar contract the checker enforces, and the
toolchain version remains the thing you pin in CI. Alignment guarantees only
that the numbers you *read* on a given release match, so "which `0.10.0`?" is
never a question. Read the changelog to learn which axes changed
substantively; read the numbers only to learn which release you are on.

`0.8.0` kept the alignment: it advanced the language (a new core directive, a
new `after:` primitive, a narrowed `state:` rule), the IR (a new `end` command
kind plus append-only fields), and the toolchain together.

**`0.10.0` aligns all three axes at `0.10.0`, and every one of them earned it.**
The language changes
([`proposals/scenario-dsl/0.10.0.md`](proposals/scenario-dsl/0.10.0.md)) restrict
what a document may be — thirteen of them, three able to redden a document that
checks clean under `0.9.0`. The toolchain release carries them plus thirteen
tooling fixes. And the IR shape **moves for the first time since `0.8.0`**:
`Provenance.reason` becomes `Provenance.explanation`
(`schemas/lute-ir-0.10.schema.json`), because the old name collided with
`end.reason` — an opaque author token a host dispatches on — while this field is
human-readable English the compiler wrote. `capabilityVersion` changes too:
`W-INJECT-CONFLICT` leaves the code set and eleven codes join it.

That has one real cost, and hiding it would be worse than paying it. The
[runtime contract](runtime/execution-model.md#version-negotiation) requires an
engine to **refuse** an artifact whose `irVersion` major.minor is newer than
the one it implements, so an engine built against IR `0.9` refuses every
`0.10.0` artifact until it widens that gate. Unlike the `0.9.0` bump, widening
is not the *entire* migration: an engine that reads the injection provenance
stamp must also rename `reason` to `explanation`. That rename is the only edit
beyond the gate.

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
