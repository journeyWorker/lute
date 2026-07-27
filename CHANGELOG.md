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

## [Unreleased]

## [0.8.0] - 2026-07-27

The **adoption release**. Every item here traces to a concrete gap found while
assessing Lute against a real, large game catalog — 777 authored scenes /
73,847 command rows / 583 quests / 3,104 condition rows
([`docs/adoption/oshiz-assessment.md`](docs/adoption/oshiz-assessment.md) §10).
Specs: [`scenario-dsl/0.8.0.md`](docs/proposals/scenario-dsl/0.8.0.md) and
[`plugin-system/0.0.2.md`](docs/proposals/plugin-system/0.0.2.md).

All three version axes advance to `0.8.0`; the IR JSON schema is renamed
`schemas/lute-ir-0.7.schema.json` → [`schemas/lute-ir-0.8.schema.json`](schemas/lute-ir-0.8.schema.json).
A document stamped `luteVersion: "0.7.0"` fires the pre-existing
`W-LUTE-VERSION-STALE`; restamping is the only edit a 0.7.0-clean document
needs (see *Changed* for the one exception).

### Fixed

- **`addr` field width no longer overflows** — the index segment was fixed at
  4 digits, so a shot with 100+ records emitted `001-11500` beside `001-1400`
  and **lexicographic ordering silently diverged from execution order**; an
  engine that ordered or range-checked addresses as strings would rewind into
  already-played content. This was hit in production by the `tactus` pilot and
  was invisible to the conformance suite, whose fixtures were all 4-digit.
  Both segments are now padded to a width computed from the document and
  **uniform across the whole artifact**, so *lexicographic order over `addr`
  equals execution order* is a guarantee an engine may rely on. The fold counts
  only addresses actually emitted, so a document whose every shot emits fewer
  than 100 addresses is byte-identical to 0.7.0.

### Added

- **`::end{reason?}`** — the ninth `lute.core` directive and a new IR command
  kind `end`: terminate the walk, carrying an optional free-form reason the host
  may surface. Content after an `::end` in the same straight-line body is
  reported `W-CODE-AFTER-END`. New conformance fixture `conformance/end-reason/`.
  Termination is control flow, so it is core rather than a plugin directive —
  a plugin record is opaque to reachability analysis and could not be proven to
  terminate.
- **`after:` gains `active("questId")`** — the prerequisite profile admitted
  `visited` and `completed`, but the quest lifecycle is
  `unset → active → complete|failed`, so it could express two of three
  observable states. Graph semantics match `completed` (reachability, cycles);
  the state envelope is strictly weaker. `lute scenario` reports the edge kind
  in `text`, `json` (`kinds`), and `dot` (`active` renders dashed).
- **`quest.<id>.activatedAt`** — a reserved `narrativeTime` slot the engine
  stamps at the `unset → active` transition. `validAt(rel, t)` existed since
  0.3.0 but had **no author-writable `t`**; this is it. Readable in CEL,
  never author-declarable (`E-QUEST-RESERVED-DECL`) or writable
  (`E-QUEST-RESERVED-WRITE`), and exempt from `E-MAYBE-UNSET` because a
  maybe-unset verdict on it would be undischargeable.
- **`Artifact.shots`** — authored `## ` headings now survive compilation.
  0.6.0 made shot headings free text and lowering discarded them, so a
  compile → decompile round trip lost every section title; headings were the
  only authored structure with no other IR carrier.
- **Localization round trip** — `lute loc import <file>…` canonicalizes
  `loc export` output into a `lineId`-keyed locale bundle, and
  `lute compile --locales <bundle.json>` merges it into `LineCmd.texts` and the
  choice/hub option `labels`. `text`/`label` stay the source-language string, so
  a 0.7 consumer is unaffected. A missing `(lineId, locale)` pair is
  `W-L10N-MISSING`, promotable with `--deny`. A malformed bundle is
  `E-LOCALE-BUNDLE`.
- **`lute compile --all --project <dir> -o <dir>`** — project-wide compile
  emitting one artifact per document plus `project.index.json`, whose
  `entities`/`enums`/`relations`/`seedFacts`/`rules`/`prereqEdges` are the
  deterministic **union** across every document. The runtime contract already
  required engines to compute that union; until now every adopter re-implemented
  it. All-or-nothing: one failing document writes no output.
- **`identity:` templates** — `lute.project.yaml` can now shape `lineId` and
  `voiceKey` (`{prefix}`, `{speaker}`, `{code}`), so a catalog with an existing
  identity convention can be migrated. Defaults reproduce 0.7.0 byte-for-byte;
  an unknown token is `E-IDENTITY-TEMPLATE`.
- **Plugin `stampAttrs`** — a plugin may declare **cross-cutting** attributes
  admissible on every directive *and* on content lines, landing flattened in the
  record's stamp. Engines routinely carry per-record metadata orthogonal to the
  record kind (analytics tags, bonus hooks); 0.0.1 could declare attributes only
  per-directive. `stampAttrs` participates in `capabilityVersion`.
- **Declarative lowering is implemented** — `lower: { record, fields }` parsed
  since 0.0.1 but `lute-compile` never matched on it, so *every* plugin
  directive became `kind: "plugin"`. A directive may now lower to one of the
  eight non-control-flow staging kinds, with `fromAttr`/literal field bindings
  validated at assembly (`E-LOWER-RECORD-UNKNOWN`, `E-LOWER-RECORD-FIELD`). The
  emitted record inherits the target kind's `wait` default, so it is
  indistinguishable from the core directive an engine dispatches identically.
- **Browser playground** — the website ships a fully client-side
  [Try Lute](https://lute-lang.vercel.app/playground/) page: a new `lute-wasm`
  crate compiles the checker, compiler, and tracer to WebAssembly (2.3 MB,
  committed at `packages/website/public/playground/pkg/` so the site build stays
  Rust-free), exposing `check_source` / `compile_source` / `trace_source` /
  `version`. Live diagnostics with click-to-seek, an on-demand compiled-IR view,
  a mock-driven trace transcript, and three embedded checker-clean examples.
  Scope: one self-contained document, core profile (no `uses:` or plugins).
- **LSP stale-binary version guard** — `lute-lsp` advertises the language
  version it implements (`lute_check::LUTE_LANG_VERSION`) as the LSP
  `serverInfo.version`, and the VS Code extension warns once when the running
  server is strictly older than a document's frontmatter `luteVersion:` target.
  A stale server silently mis-analyzes newer grammar and cannot self-detect it
  (its own `W-LUTE-VERSION-STALE` compares against the version it was built at),
  so the client-side comparison is the only reliable signal. Toggle with
  `lute.versionCheck` (default on). Diagnostics remain a byte-for-byte
  reprojection of the CLI (`crates/lute-lsp/tests/divergence.rs`).

### Changed

- **Author `state:` is scalar — enforced** (`E-STATE-COLLECTION`). Three sources
  disagreed: the normative text said scalar-only, the shape validator accepted
  the full `Type` union (so `type: { list: string }` silently passed), and
  `docs/runtime/state-lifecycle.md` documented `list<…>`/`map<…>`/`record` as
  valid. All three now agree — collection-shaped `StateEntry` types reach the
  artifact only through a plugin `state_shapes` expansion. **This is the one
  case where a 0.7.0-clean document may newly fail**; collections were always
  meant to be modelled as `relations:` (0.3.0 §3).
- **`E-`-severity capability-resolution diagnostics now gate the exit code.**
  Project/plugin resolution errors print on the `lute:` channel rather than the
  per-document diagnostic list, and were previously advisory — so a new
  `E-PLUGIN-OPTION-TYPE` would have printed and passed. They now fail
  `check`/`check-project`/`compile`/`test`, matching the binary-severity rule
  (`E-` gates). The forced-single-root reconciliation scan behind
  `compile --project` is exempt: a sibling belonging to a nested subproject
  legitimately mis-resolves under a forced root, and that is not the target
  document's fault.
  Because that text is now the whole of what a failing author sees, every
  `AssembleError` renders prose: the five variants that previously fell back to
  a Rust `Debug` form (`E-PLUGIN-MISSING-ACTIVE`, `E-PLUGIN-DUP-ACROSS` /
  `E-DOMAIN-DUP`, `E-PLUGIN-RESERVED-NAME`, `E-STATE-SHAPE-CYCLE`,
  `E-PLUGIN-UNKNOWN-ASSETKIND`) now say what went wrong and what to do. Codes
  are unchanged.
- **Plugin option validation** (spec Appendix C1) — activation rejects an
  unknown option name (`E-PLUGIN-OPTION-UNKNOWN`) and a value that fails its
  declared type (`E-PLUGIN-OPTION-TYPE`).
- **Plugin frontmatter value validation** (C2) — a plugin-owned frontmatter key
  is now checked against its declared schema, not merely admitted
  (`E-FRONTMATTER-SCHEMA`).
- **Reserved stamp-attribute names** (C4, widened) — a plugin declaring `at`,
  `duration`, `delay`, `wait`, `timeline`, `provenance`, or `source` as an
  attribute is rejected at assembly with `E-PLUGIN-RESERVED-STAMP-ATTR`, on both
  the `stampAttrs` and per-directive surfaces.
- **`lute init` / `lute new` stamp the current language version** instead of a
  hardcoded literal, so scaffolds cannot go stale on a version bump again.

### Not done, deliberately

Recorded so they are not re-proposed — each was rejected on measured evidence,
see [`scenario-dsl/0.8.0.md`](docs/proposals/scenario-dsl/0.8.0.md) §10:
Datalog aggregation (`sum`), any Datalog surface extension, author-declarable
collection state, and a per-record `label` field. Plugin spec item **C3**
(the `wait="false"` stale-default bridge read) remains open: it needs a
dominance analysis the checker does not perform, and is deferred rather than
half-shipped.

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
