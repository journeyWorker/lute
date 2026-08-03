# Changelog

All notable changes to the Lute **toolchain** are documented here. The format
is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

Lute tracks three independent version axes; this file covers only the first:

- **Toolchain** — this changelog. The version of the CLI, checker, compiler,
  LSP, and npm launcher that ship together, stamped from the Cargo workspace
  (`CARGO_PKG_VERSION`) and printed by `lute version`.
- **Language** — currently `0.9.0`, the grammar and semantics the checker
  enforces. Its history lives in the versioned spec stack under
  [`docs/proposals/scenario-dsl/`](docs/proposals/scenario-dsl/), not here.
- **IR** — the compiled JSON artifact schema, stamped as `irVersion` in every
  artifact (currently `0.9.0`) and gated on by consuming engines.

Every release holds all three axes **aligned** at one visible number, so a
release presents one number and nobody has to reconcile three. `0.9.0` keeps
that alignment: the language and the toolchain both advance, and the IR number
moves with them even though **IR `0.9.0` is shape-identical to IR `0.8.0`**.
Alignment is a presentation guarantee, not a claim that every axis changed
substantively — this changelog is where you learn which ones did. See
[`docs/versioning.md`](docs/versioning.md) for the full policy and the axes
table.

## [Unreleased]

### Fixed

- **A component file checked standalone now enforces the presentational-body
  contract (dsl 0.4.0 §6.2).** `lute check some.component.lute` reported `ok`
  for a body containing `<branch>`, `<hub>`, `<timeline>`, `<on>`,
  `<objective>`, `::set`, `::assert`, or `::retract` — every one of which fails
  with `E-COMPONENT-BODY` the moment the component is reached through a
  `::use`. A component file carries no `kind:`, so it degrades to
  `DocKind::Scene` and walked through the ordinary scene `Walker`, where all of
  those constructs are legal; `walk_component_body`, which owns the
  prohibition, was reached only from `validate_components` over an *importing*
  document's component table. The standalone leg is the one a component author
  is most likely to run, and it was a false green. The component root now walks
  through the same `walk_component_body` the `::use` leg uses — one
  implementation of the contract, not two. This is the mirror of the earlier
  fix that made the standalone leg no longer too *strict* about a component
  file's own `<quest>`.

  A `<hub>` additionally draws `E-HUB-NO-EXIT` on the standalone leg only,
  because the branch-folding pre-pass runs over any root document; that
  residual is deliberate and documented in
  `crates/lute-check/tests/component_logic_block.rs`.

- **`docs/examples/components/greet.component.lute` and
  `showcase/components/stinger.component.lute` documented a rule the language
  dropped.** Both header comments stated dsl §13.4's blanket ban on logic
  blocks in a component body; 0.4.0 §6.2 has admitted a param-scoped
  `<match on="@param">` since then, which `reaction.component.lute` relies on.
  Reading the examples taught the wrong rule. `stinger`'s claim that a
  standalone check reports `E-META-MISSING` was also stale — it reports
  `E-DOMAIN-UNKNOWN`, because that file declares no `uses:` of its own.

## [0.9.0] - 2026-07-29

**Language `0.9.0` — vocabulary ownership: the core declares slots, the project
declares members.** Breaking at the language axis (pre-1.0 allowance). Specs:
[`scenario-dsl/0.9.0.md`](docs/proposals/scenario-dsl/0.9.0.md) and
[`plugin-system/0.0.3.md`](docs/proposals/plugin-system/0.0.3.md).
`LUTE_LANG_VERSION` is `0.9.0` and the toolchain ships as `0.9.0`.
**`LUTE_IR_VERSION` also moves to `0.9.0`** under the axis-alignment rule even
though the artifact shape is untouched; the IR JSON schema is
[`schemas/lute-ir-0.9.schema.json`](schemas/lute-ir-0.9.schema.json), the `0.8`
file renamed with no shape edit. **For a consuming engine the IR bump is a
no-op apart from one gate widening** — see the IR bullet under *Changed*.

### Changed

- **BREAKING — a document must declare the content vocabulary it uses.**
  `lute.core` ships **no vocabulary members**. It declares seven *slots* —
  `emotion`, `action`, `anchor`, `mood`, `volume`, `musicAction`, `vfxType` — as
  the types of core content-line and directive attributes, and nothing more.
  Every member now comes from one of three declaration routes: an `enums:` block
  in the using document's **own frontmatter**, a project schema's `enums:`
  (imported through `uses:`/`extends:`), or a plugin's `enums` export. Using a
  slot that no source declares is `E-DOMAIN-UNKNOWN`, and the diagnostic names
  all three routes.

  Until now those six baseline vocabularies were closed lists inside
  `lute.core` that **no route could extend**: a project schema declaring
  `emotion:` got `E-DOMAIN-DUP` and had its members dropped, a project's own
  capability plugin exporting `emotion` failed whole-project resolution with
  `E-PLUGIN-DUP-ACROSS`, and `lute.core` cannot be deactivated. Measured against
  one real catalog, **20.7% of 30,861 authored `emotion` values were
  unrepresentable**.
- **`action` is now validated.** It previously carried a guard that *skipped*
  validation whenever nothing declared the domain, so 9,880 values across 53
  distinct ids received no checking at all and a typo like `step-foward`
  shipped. The guard is gone; `action` behaves exactly like `emotion`.
- **`::auto{action}` and `::music{mood}` are domain-typed**, having been free
  strings. This is why the `mood` domain had been declared-but-inert since it
  shipped.
- **An `::auto` that omits `anchor` now checks the `anchor` slot it implicitly
  reads.** The default-anchor injection reads the `anchor` domain's `default:`,
  but nothing in the document names `anchor` on that path and directive
  validation only sees AUTHORED attributes — so a project that declared `action`
  and forgot `anchor` checked clean while the anchor command 0.8.0 emitted
  unconditionally simply disappeared from the artifact. An undeclared slot is now
  `E-DOMAIN-UNKNOWN` there too, reported on the `::auto` itself, so the slot rule
  above holds for implicit reads as well as written ones. Writing an explicit
  `anchor` was, and remains, an error at the attribute.
- **A component body is checked the same way through `::use` as standalone.**
  Five whole-document passes ran only at the document root and never over an
  imported component body, so the same content checked clean inside a component
  and dirty at scene level. All five now run over component bodies: content-line
  attributes (`E-DOMAIN-UNKNOWN`, `E-BAD-ENUM`, `E-UNKNOWN-ATTR`, the delivery
  rules), duplicate line codes (`E-DUP-LINE-CODE`), reachability (`E-ARM-DEAD`,
  `W-CODE-AFTER-END`), admission of a component's unwalked top-level content
  (`E-GRAMMAR-NOT-ADMITTED`), and injection folding (`W-INJECT-CONFLICT`). Two
  of the five let real defects reach the artifact: an undeclared vocabulary
  value, and a duplicated `lineId`. A third silently **dropped** a component's
  top-level `<quest>` entirely. **A component body that used to check clean may
  now report; every such report is a defect that was already there.**
- **IR `0.9.0` — the number moves, the shape does not.** `irVersion` is now
  `0.9.0` and the schema is
  [`schemas/lute-ir-0.9.schema.json`](schemas/lute-ir-0.9.schema.json), the
  `0.8` file renamed. **No field is added, renamed, or moved, and no command
  `kind` is new**: IR `0.9.0` is shape-identical to IR `0.8.0`, and the number
  moved only because a release re-aligns every axis. **This is a no-op for
  consumers except for one thing, which is not optional**: the
  [runtime contract](docs/runtime/execution-model.md#version-negotiation)
  requires an engine to refuse an artifact from a newer `irVersion`
  major.minor, so an engine implementing `0.8` **will reject every `0.9.0`
  artifact** until it widens its gate to accept `0.9`. Widening the gate is the
  whole migration — no parser change, no new field, no new behaviour.
- **Artifact content changes; the artifact shape does not.** A project-declared
  vocabulary — inline or imported — now reaches the compiled artifact's existing
  `enums` array, because it is project data like `entities:`/`relations:`. A
  vocabulary supplied by a plugin `enums` export does not appear there — it is
  part of `capabilityVersion`.
  `capabilityVersion` changes for every project (the core's vocabulary emptied
  and two attribute types changed).
- **`capabilityVersion` covers the member semantics, not just the members.** The
  stamp folds a plugin-exported vocabulary's `default:`/`exits:` alongside its
  member list. Those keys now decide emitted output — the injected anchor and
  `sprite.exit` — so two capability surfaces that agree on members and differ
  only there compile differently and no longer share a stamp. A surface carrying
  no vocabulary at all hashes byte-identically to before.
- **`lute new scene`** imports `vocabulary.schema.yaml` when the project has one.

### Added

- **`enums:` long form** — `{ members, default, exits }`. A bare list stays
  shorthand for `{ members: [...] }`, so every existing declaration keeps parsing
  byte-for-byte. A declaration of `action` **MUST** supply `exits:` (the members
  that exit their character) and a declaration of `anchor` **MUST** supply
  `default:` (the member used when the attribute is absent); for the other five
  slots both keys are rejected. Four new diagnostics:
  `E-ENUM-MISSING-SEMANTICS`, `E-ENUM-UNEXPECTED-SEMANTICS`,
  `E-ENUM-DEFAULT-NOT-MEMBER`, `E-ENUM-EXITS-NOT-MEMBER`. The same validator
  runs on all three routes.
- **A document's own inline `enums:`/closed `entities:` now declares vocabulary
  for that document.** `enums:` has always been legal frontmatter in any
  document, but the projection was built and then dropped before the domain
  merge, so using what you had just declared on the line above still reported
  `E-DOMAIN-UNKNOWN`. It now reaches the merge by the same path an imported
  declaration does, and surfaces in `lute context --json`'s `projectEnums`, in
  `lute doctor`'s slot report, and in LSP hover/completion. This is the only
  route open to a single-file author or the playground, which checks one
  in-memory document and can resolve no import. Precedence: inline wins over an
  imported declaration of the same slot and must re-declare a superset of its
  members (`E-EXTENDS-RELATION-SIG` otherwise); against a plugin or the core it
  is `E-DOMAIN-DUP` and the plugin wins. A component body is the one place it
  does not apply — see *Known limitation*.
- **`lute init` scaffolds a starter vocabulary** (`vocabulary.schema.yaml`)
  covering all seven slots with `exits:`/`default:` filled in, so a fresh project
  checks clean out of the box and its starter scene actually uses a slot. The
  opinionated default lives in the template, not in the compiler.
- **`lute doctor` reports vocabulary slots**, with the member semantics inline:
  `vocabulary slots declared: emotion, action (exits: …), anchor (default: …), …`.

### Removed

- **The hardcoded exit heuristic**, in *both* hand-synced copies (the checker's
  reducer and the compiler's lowerer, the second commented "mirrors … byte-for-
  byte"). Exit is now membership in the declared `exits:` list. Gated on a table
  test proving the new reading reproduces the old verdict over the full fixture
  corpus before either copy was deleted.
- **`DEFAULT_ANCHOR = "center"`** — replaced by the declared `anchor`
  `default:`. Production code now branches on **zero** domain members.
- **Two `semantics` flags with no consumer** — `isStateful` and
  `cancelsPrevious` (plugin 0.0.3 §4). The closed vocabulary goes from twelve
  flags to **ten**; no shipped plugin declared either.
- **Dead `pose` attribute reads** in the stage reducer. `pose` is not a known
  content-line attribute, so `@x{pose="…"}` was already `E-UNKNOWN-ATTR` and
  neither read was reachable.

### Migration

1. **Declare your vocabulary**, by whichever of the three routes fits. Add an
   `enums:` block to a schema your documents already import (best for a
   multi-document project), add one to a single document's own frontmatter (the
   only route open to a one-file author or the playground, which resolves no
   imports), or export `enums` from your own capability plugin. `lute init`
   scaffolds one; `lute doctor <dir>` lists which slots a project root has
   declared, and `lute check` names all three routes on the first undeclared use.
   Declare per project root — a sibling root's declaration does not reach in.
   Declaring one slot through a plugin **and** either project route in one root
   is `E-DOMAIN-DUP` (the plugin wins); declaring it both inline and in an
   imported schema is not, but the inline block must re-declare a superset of the
   imported members or it is `E-EXTENDS-RELATION-SIG`.
2. **Spell out the member semantics** — `exits:` for `action`, `default:` for
   `anchor` — and include the members the old core rejected.
3. **Restamp** `luteVersion: "0.8.0"` → `"0.9.0"` (the pre-existing
   `W-LUTE-VERSION-STALE`).
4. **Fix what the component bodies were hiding** (see *Changed*).

`conformance/` needs **zero** fixture edits: no conformance source uses any of
the seven slots.

#### Known limitation

A component body resolves its vocabulary against the **importing** document,
because neither a component's own `uses:` nor an inline `enums:` block in its
frontmatter is carried through `::use`. So a component naming a domain only *it*
declares passes a standalone `lute check` and fails through a `::use` from a
scene that does not declare or import the same vocabulary. Keep the
declaration at the project root so both reach the same one. A *component schema*
surface that carries a component's own imports into the expansion is a named
future direction, filed separately
([`scenario-dsl/0.9.0.md`](docs/proposals/scenario-dsl/0.9.0.md) §6.1).

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
