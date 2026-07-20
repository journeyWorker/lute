# Changelog

All notable changes to the Lute **toolchain** are documented here. The format
is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

Lute tracks three independent version axes; this file covers only the first:

- **Toolchain** — this changelog. The version of the CLI, checker, compiler,
  LSP, and npm launcher that ship together, stamped from the Cargo workspace
  (`CARGO_PKG_VERSION`) and printed by `lute version`.
- **Language** — currently `0.6.1`, the grammar and semantics the checker
  enforces. Its history lives in the versioned spec stack under
  [`docs/proposals/scenario-dsl/`](docs/proposals/scenario-dsl/), not here.
- **IR** — the compiled JSON artifact schema, stamped as `irVersion` in every
  artifact (currently `0.6.1`) and gated on by consuming engines.

The language and toolchain versions move **independently**: a toolchain
release need not advance the language, and a language delta can land under any
toolchain version. See [`docs/versioning.md`](docs/versioning.md) for the full
policy and the axes table.

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

[0.2.0]: https://github.com/journeyWorker/lute/releases/tag/v0.2.0
[0.1.0]: https://github.com/journeyWorker/lute/releases/tag/v0.1.0
