# Changelog

All notable changes to the Lute **toolchain** are documented here. The format
is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

Lute tracks three independent version axes; this file covers only the first:

- **Toolchain** — this changelog. The version of the CLI, checker, compiler,
  LSP, and npm launcher that ship together, stamped from the Cargo workspace
  (`CARGO_PKG_VERSION`) and printed by `lute version`.
- **Language** — currently `0.7.0`, the grammar and semantics the checker
  enforces. Its history lives in the versioned spec stack under
  [`docs/proposals/scenario-dsl/`](docs/proposals/scenario-dsl/), not here.
- **IR** — the compiled JSON artifact schema, stamped as `irVersion` in every
  artifact (currently `0.7.0`) and gated on by consuming engines.

As of the `0.7.0` release, all three axes are **aligned at `0.7.0`**: language,
IR, and toolchain share one visible number to remove version confusion. They
still move **independently** in principle — a toolchain release need not advance
the language, and a language delta can land under any toolchain version — and
MAY drift apart again when a future release genuinely changes only some axes.
See [`docs/versioning.md`](docs/versioning.md) for the full policy and the axes
table.

## [0.7.0] - 2026-07-20

### Changed

- **Version unification** — every version axis is aligned at `0.7.0`. The
  language (`LUTE_LANG_VERSION`, was `0.6.1`), the IR (`LUTE_IR_VERSION`, was
  `0.6.1`), the Cargo workspace toolchain (was `0.2.0`), and all four npm
  packages (were `0.2.0`) now share one visible number. This supersedes the
  `0.2.0` toolchain release below, which shipped the same day as the last
  independently-numbered toolchain: `0.7.0` is the unified number for that work
  plus the additions here. There is **no grammar, semantic, or IR shape change**
  — language `0.7.0` is byte-for-byte `0.6.1` semantics (see
  [`docs/proposals/scenario-dsl/0.7.0.md`](docs/proposals/scenario-dsl/0.7.0.md)).
  The IR JSON schema is renamed `schemas/lute-ir-0.6.schema.json` →
  [`schemas/lute-ir-0.7.schema.json`](schemas/lute-ir-0.7.schema.json) (body
  unchanged). A document stamped `luteVersion: "0.6.1"` now fires
  `W-LUTE-VERSION-STALE`; the remedy is to restamp it `luteVersion: "0.7.0"`.

### Added

- **`lute run` reference runner** — an executable reference interpreter for
  compiled artifacts, validated against the `conformance/` fixture corpus so an
  engine has a golden oracle for artifact execution semantics.
- **`lute test` scenario tests + coverage** — a scenario test runner with
  coverage reporting over authored paths, so authors can assert reachable
  outcomes and see which regions a suite exercises.
- **`lute init` / `lute new` / `lute doctor`** — project scaffolding
  (`init`/`new`) and an environment/health diagnostic (`doctor`).
- **`lute scenario --format json|dot`** — machine-readable (`json`) and
  Graphviz (`dot`) exports of the scenario graph alongside the human view.
- **`lute loc` export/report** — localization string export and a coverage
  report over translatable content.
- **New website pages** — `getting-started/learning-paths`, a tutorial track,
  a "when to use" fit page, and the `spec/current` consolidated spec index.
- **Docs CI** — a continuous-integration workflow that runs the docs
  consistency checker and builds the website on every change.
- **VS Code extension packaging** — the editor extension is packaged and a
  `.vsix` artifact is produced as a CI build output.

## [0.2.0] - 2026-07-20

### Added

- **Runtime contract documentation** — a runtime docs set under
  [`docs/runtime/`](docs/runtime/) plus a website page at `tooling/runtime-contract`
  describing what a compiled artifact promises an engine, and the honest
  boundaries of static analysis (reachability is conservative under declared
  `after:` routes; relational gates can yield `Unknown` verdicts requiring
  human review; `lute trace` walks one deterministic mock-driven path, not a
  proof over all paths).
- **Versioned IR JSON schema** — [`schemas/lute-ir-0.6.schema.json`](schemas/lute-ir-0.6.schema.json),
  a machine-readable schema for the compiled artifact envelope, letting engines
  validate artifacts against the `irVersion` they stamp.
- **`lute version`** — prints the toolchain, language, and IR versions;
  `lute version --json` emits `{"toolchain":…,"language":…,"ir":…}` for tooling.
- **Windows x86-64 prebuilt binaries** — the npm launcher now resolves a
  native binary on `win32-x64` in addition to `darwin-arm64` and `linux-x64`.
- **Investigation RPG example** — a worked example exercising quests,
  objectives, relational state, and connectivity analysis.
- **`LICENSE`** — the project is MIT-licensed.
- **`docs/versioning.md`** — the versioning policy: the toolchain / language /
  IR / capability / plugin axes, which bumps when, and the pre-1.0 draft
  breaking-change policy.

### Changed

- **Homepage repositioning** — the README and website landing now split the
  status claim along its axes (language draft vs. implementation shipped vs.
  production stability) rather than a single blanket "implemented" claim, and
  link `LICENSE`, this changelog, and the versioning policy.

## [0.1.0]

Initial scoped npm release: the [`@lute-lang/lute`](https://www.npmjs.com/package/@lute-lang/lute)
launcher resolving `darwin-arm64` and `linux-x64` prebuilt binaries, targeting
language version `0.6.1`.

[0.7.0]: https://github.com/journeyWorker/lute/releases/tag/v0.7.0
[0.2.0]: https://github.com/journeyWorker/lute/releases/tag/v0.2.0
[0.1.0]: https://github.com/journeyWorker/lute/releases/tag/v0.1.0
