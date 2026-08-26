# Versioning policy

Lute is versioned along several **independent** axes. A single "version" number
would conflate things that move at different speeds — the grammar an author
writes against, the binary they install, and the artifact schema an engine
consumes are separate contracts. This document names each axis, says which
change bumps which, and states the pre-1.0 breaking-change policy.

## Axes

| Axis | Where it lives | Current | What a bump means |
|---|---|---|---|
| **Toolchain** | Cargo workspace version (`CARGO_PKG_VERSION`); `lute version` | `0.13.0` | A release of the CLI, checker, compiler, and LSP shipping together, and the npm launcher that distributes them. Tracked in [`CHANGELOG.md`](../CHANGELOG.md). |
| **Language** | [`lute_check::LUTE_LANG_VERSION`](../crates/lute-check/src/lib.rs); `luteVersion:` frontmatter | `0.13.0` | A change to the grammar or static semantics the checker enforces. History is the versioned spec stack under [`docs/proposals/scenario-dsl/`](proposals/scenario-dsl/). |
| **IR** | `irVersion` field of every compiled artifact ([`lute_compile::LUTE_IR_VERSION`](../crates/lute-compile/src/lib.rs)) | `0.13.0` | A change to the compiled JSON artifact schema ([`schemas/lute-ir-0.13.schema.json`](../schemas/lute-ir-0.13.schema.json)). Consuming engines gate parsing on its MAJOR (0.13.0; previously major.minor). |
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

**`0.10.1` aligns all three axes at `0.10.1`, and none of them earned it.** That
is the alignment rule working as written rather than a defect in it: the release
is two toolchain fixes — `lute test` gains the `--project` resolution its
sibling commands already had, and an `assetKind` segment declared with a `Type`
outside the closed segment production is rejected at plugin load instead of
enforcing nothing — and the language and IR numbers move because the rule says
they move. Language `0.10.1` is byte-for-byte `0.10.0` semantics
([`proposals/scenario-dsl/0.10.1.md`](proposals/scenario-dsl/0.10.1.md), written
for the same reason `0.7.0`'s was: a language version absent from the spec stack
cannot be told apart from one nobody recorded).

**This one costs a consuming engine nothing.** The runtime contract gates on
`irVersion` **major.minor**, and `0.10.1` shares `0.10` with `0.10.0`, so an
engine that accepts a `0.10.0` artifact accepts a `0.10.1` artifact with no
edit. For the same reason `schemas/lute-ir-0.10.schema.json` keeps its name and
its `$id` — the schema file tracks the gated `major.minor`, not the release
number, which is why `0.7.0` renamed its schema and this release does not.

**`0.10.2` aligns all three axes at `0.10.2`, and this time the IR is the one
that earned it — the language did not.** Inverse of `0.10.1`: the release adds
`meta.plugin` to the compiled artifact envelope (`SceneMeta`/`QuestMeta`), a
checker-validated, plugin-owned frontmatter key folded into the artifact
instead of discarded at compile time (`crates/lute-compile`, plugin-system
[`plugin-system/0.0.4.md`](proposals/plugin-system/0.0.4.md)). Language
`0.10.2` is byte-for-byte `0.10.1` (== `0.10.0`) semantics
([`proposals/scenario-dsl/0.10.2.md`](proposals/scenario-dsl/0.10.2.md)).

**Additive, so this one costs a consuming engine nothing either — but a
strict-schema validator gains an admitted field.** The runtime contract still
gates on `irVersion` **major.minor**, and `0.10.2` shares `0.10` with
`0.10.0`/`0.10.1`, so an engine that accepts a `0.10.0`/`0.10.1` artifact
accepts a `0.10.2` artifact with no edit — `plugin` is a new object key a
permissive JSON reader simply does not ask for.
`schemas/lute-ir-0.10.schema.json` keeps its name and its `$id` for the same
reason `0.10.1`'s did (the file tracks the gated `major.minor`, not the
release number), but its CONTENT does move this release: `sceneMeta` and
`questMeta` each gain a `plugin` property, so a consumer that validates
strictly against the published schema (rather than merely parsing
permissively) now admits the field instead of rejecting it under the schema's
`additionalProperties: false`.

**`0.11.0` aligns all three axes at `0.11.0`, and only the toolchain earns
it.** A new `schedule.yaml` project-file layer (clock, lanes, guarded
placements, static route-space checks) and a new `lute play` command that
chains a schedule into one reviewer-facing, whole-project transcript
(`--coverage` for corpus review-gap reporting), plus two bug fixes in the
shared reference runner (a compiled `<when is=…>` match arm now reads its
structured `expr` instead of always falling through to `otherwise`; a hub
whose scripted decisions run out with an eligible option remaining now halts
incomplete instead of silently converging) — all toolchain, none of it
language or IR. Language `0.11.0` is byte-for-byte `0.10.2` (== `0.10.1` ==
`0.10.0`) semantics
([`proposals/scenario-dsl/0.11.0.md`](proposals/scenario-dsl/0.11.0.md)).

**Unlike `0.10.1`/`0.10.2`, this one is not free for a consuming engine — the
IR's `major.minor` itself moves, even though its content does not.** The IR
carries no shape change AND no content change: nothing about the compiled
artifact is different from `0.10.2`. But `0.10.1` and `0.10.2` both stayed
inside `0.10`, and `0.11.0` does not — the alignment rule moves every visible
axis on every release, and this time that move crosses the `major.minor`
boundary the runtime contract actually gates on. An engine implementing IR
`0.10` **must widen its gate to `0.11`** before it will accept a `0.11.0`
artifact at all, and that widening is the *entire* cost — there is no field
to add, rename, or start reading once it does. Per the `0.7.0` precedent (a
`major.minor` move renames the schema file even with no shape change, because
the file tracks the gated `major.minor` rather than the release number),
`schemas/lute-ir-0.10.schema.json` is renamed to
`lute-ir-0.11.schema.json` — later re-stamped along the same rule and now
published as [`schemas/lute-ir-0.13.schema.json`](../schemas/lute-ir-0.13.schema.json)
(`$id` updated to match; body otherwise byte-identical to `0.10.2`'s).
`schedule.yaml` itself stays deliberately outside every one of these axes —
no `kind:`, no `luteVersion:`, no capability fold — so none of this release's
real, substantive work is what moved the IR number; the number moved because
alignment always moves it, and this file's `0.7.0`-set precedent decided long
ago that a `major.minor` move earns a rename regardless of why it happened.

**`0.13.0` aligns all three axes at `0.13.0`; the toolchain and the language
earn it, and the release retires the gate-widening tax the two previous
paragraphs kept paying.** The toolchain gains the lint layer (`lute lint`,
`lute.lint.yaml`, plugin `lints` exports —
[`plugin-system/0.0.5.md`](proposals/plugin-system/0.0.5.md) — deliberately
excluded from the capability snapshot, so `capabilityVersion` does not move)
and `lute tag --force`. The language gains one universal frontmatter key,
`codesLocked:`
([`proposals/scenario-dsl/0.13.0.md`](proposals/scenario-dsl/0.13.0.md)).
The IR is a pure restamp again — and that recurrence is the point: `0.11.0`
and `0.12.0` each forced every consuming engine to widen a `major.minor`
gate to accept artifacts nothing in which had changed. As of `0.13.0` the
[runtime contract](runtime/execution-model.md#version-negotiation) gates on
**MAJOR only**: minor and patch are compatible-by-default (fields append-only
within a major line, unknown fields ignored), and the hard error that
actually protects an engine from a newer artifact is an unknown command
`kind`, not the version number. A pre-1.0 breaking IR change (the `0.10.0`
provenance rename remains the only one) is called out in the changelog and
the schema rather than fenced by the gate. The schema file keeps its
per-release rename (`lute-ir-0.12.schema.json` →
[`lute-ir-0.13.schema.json`](../schemas/lute-ir-0.13.schema.json)) with a
revised rationale: it tracks the published release line for strict
validators, no longer the gated boundary.

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
