# Changelog

All notable changes to the Lute **toolchain** are documented here. The format
is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

Lute tracks three independent version axes; this file covers only the first:

- **Toolchain** — this changelog. The version of the CLI, checker, compiler,
  LSP, and npm launcher that ship together, stamped from the Cargo workspace
  (`CARGO_PKG_VERSION`) and printed by `lute version`.
- **Language** — currently `0.38.0`, the grammar and semantics the checker
  enforces. Its history lives in the versioned spec stack under
  [`docs/proposals/scenario-dsl/`](docs/proposals/scenario-dsl), not here.
- **IR** — the compiled JSON artifact schema, stamped as `irVersion` in every
  artifact (currently `0.38.0`) and gated on by consuming engines.




Every release holds all three axes **aligned** at one visible number, so a
release presents one number and nobody has to reconcile three. Alignment is a
presentation guarantee, not a claim that every axis changed substantively — this
changelog is where you learn which ones did. `0.10.1` was the no-op case, both
language and IR; `0.10.2` inverted it — the IR earned the move and the language
did not. `0.11.0` is a third shape: the **toolchain** is the one that earns it
this time (a new scheduling layer, a new command, and a bug class closed in the
shared reference runner), while both **language** and **IR** are content
no-ops — language `0.11.0` is byte-for-byte `0.10.2` semantics
([`0.11.0.md`](docs/proposals/scenario-dsl/0.11.0.md)), and the IR carries no
shape or content change at all. The one cost this release does not get to
skip: the IR's `major.minor` still moves, `0.10` → `0.11`, purely because
alignment re-aligns every visible number on every release — so an engine gated
on IR `0.10` **must widen its gate to `0.11`** even though there is nothing new
to read once it does, and `schemas/lute-ir-0.10.schema.json` is renamed to
[`schemas/lute-ir-0.11.schema.json`](schemas/lute-ir-0.11.schema.json) (body
unchanged) under the same precedent `0.7.0` set for a minor move with no shape
change.
See [`docs/versioning.md`](docs/versioning.md) for the full policy and the axes
table.

## [0.38.0]

Version stamps aligned; details follow in later slices.


## [0.37.0] - 2026-10-07

**Surface cleanup** (spec [`0.37.0.md`](docs/proposals/scenario-dsl/0.37.0.md);
plan [`2026-10-07-lute-0.37.0-surface.md`](docs/superpowers/plans/2026-10-07-lute-0.37.0-surface.md)).
A clean pre-1.0 breaking cutover of language and IR: every renamed spelling is
diagnosed and none is accepted as an alias. Run `lute fix` for the lossless
rewrites, then hand-migrate the items listed under Migration.

### Syntax

- Renamed directives: `::auto` → `::actor`, `::cut` → `::cg`, `::next` →
  `::jump`, `::mark{id=…}` → `::label{name=…}`.
- Renamed attributes: music `action` → `playback`; video/cg `action` →
  `display`; choice `label` → `text`; `<match on>` → `<match subject>`.
- `##` headings are **sections** (were shots) and take an optional
  `{#stable-id}` suffix; `<reward>` takes an optional `id`.
- Inline text modifiers after a content line's second colon: core
  `:pause{s=…}` and `:speed[text]{rate=…}`, plus span modifiers named by the
  project `textStyle` domain; `\:` `\[` `\]` `\{` `\}` `\\` escapes.
- New `::sequence{name=…}` (domain `sequence`, `wait` defaults to `true`);
  `::actor` gains `emotion` and `costume`; `::camera` takes `focus` /
  `framing` / `move` / `transition` domain members.
- Removed: camera `zoom`/`moveX`/`moveY`/`shake`/`reset`/`easing`, cg `full`
  (use `layout`), music `track` and sfx `name` (use `assetId`/`sound`), line
  `id` (use a preceding `::label`), project `sequence:`, reserved `<scene>`,
  body `# title` (use frontmatter `title:`). Author keys are lowerCamelCase.

### Semantics

- Staging vocabulary comes from project-declared closed domains: `anchor`,
  `action` (its `exits` is the only actor-exit rule), `emotion`, `framing`,
  `cameraMove`, `transition`, `cgLayout`, `musicPlayback` (renamed from
  `musicAction` everywhere, no alias), `costume`, `sequence`, `textStyle`.
- Mono POV rule: a `mono` line's speaker must be the effective POV
  (`pov`, falling back to project `defaults.pov`) or listed in the new
  defaultable `monoSpeakers`; component lines are checked at every `::use`
  site with the caller's context.
- Every line, narration and mono included, carries a `voiceKey`; the
  project-wide duplicate check compares modifier-stripped text.
- Section and reward ids are unique per document / per quest and become the
  stable identity keys when present; `::sequence` is recorded, not simulated,
  by the reference runner.
- Locale merge requires each translation to carry the source's modifier
  multiset.

### IR

- Kinds/fields renamed: `sprite` → `actor`, `cut` → `cg`, `background` → `bg`,
  roles `offscreen`/`voiceover`/`monologue` → `os`/`vo`/`mono`, `shots` →
  `sections` (optional `id`), `addr` → `position`, `capabilityVersion` →
  `capabilitySnapshot`, `recordKey` → `selectionKey`, option `label` → `text`,
  `vfxType` → `type`, music `action` → `playback`, cg/video `action` →
  `display` (always emitted, default `show`).
- Every command carries `family` (`content`, `staging`, `state`, `control`,
  `declaration`, `plugin`); flattened timing fields move into one `timing`
  object (`wait`, `duration`, `delay`, `at` in seconds, `timeline` ordinal).
- Lines carry `segments` / `localeSegments` when modified; `texts[locale]` is
  always the plain derivation. New `sequence` record; `actor.emotion` /
  `costume` are emitted; `RewardEntry.id`.
- Removed: `provenance.injected`, `track`, sfx `name`, numeric camera fields.
  Schema renamed to `schemas/lute-ir-0.37.schema.json`; the reference runner
  rejects removed fields with `E-IR-REMOVED-FIELD`.

### Plugin

- Plugin commands are Rust `Command::Plugin` (was `Command::Other`), still
  serialized `kind: "plugin"`, `family: "plugin"`. Plugin declarations using
  domain `musicAction` must rename it to `musicPlayback`.

### CLI

- `lute fix` rewrites the lossless forms: renamed directives and attributes,
  `<match on>`, choice `label`, line `id="x"` → preceding
  `::label{name="x"}`, and provably unique lowerCamel key edits; it never
  invents modifiers. Hand migration (diagnosed, never guessed): camera numeric
  fields, cg `full`, music `track`, sfx `name`, missing POV / `monoSpeakers`,
  project `sequence:`, `<scene>`, body `# title`.
- `lute play --json`, trace, `lute context --json`, the project index and
  bridge snapshots emit `position`, `capabilitySnapshot` and the new
  kind/family names with no alias.
- LSP: sections, `::jump`/`::label`, `::actor` and inline modifiers.

### Diagnostics

- New: `E-RENAMED-DIRECTIVE`, `E-RENAMED-ATTR`, `E-RENAMED-TAG-ATTR`,
  `E-REMOVED-ATTR`, `E-REMOVED-PROJECT-KEY`, `E-REMOVED-TAG`, `E-INERT-TITLE`,
  `E-AUTHOR-CASE`, `E-SECTION-DUP`, `E-SECTION-SUFFIX`, `E-REWARD-DUP`,
  `E-CAMERA-EMPTY`, `E-CAMERA-REMOVED`, `E-CG-LAYOUT`, `E-MONO-POV`,
  `E-MONO-NO-POV`, `E-TEXT-MODIFIER`, `E-TEXT-ESCAPE`, `E-L10N-MODIFIERS`,
  `E-IR-REMOVED-FIELD`.
- Renamed: `E-CONTENT-OUTSIDE-SHOT` → `E-CONTENT-OUTSIDE-SECTION`,
  `E-NEXT-BACKWARD` → `E-JUMP-BACKWARD`, `E-NEXT-UNDEFINED` →
  `E-JUMP-UNDEFINED`, `E-MARK-DUP` → `E-LABEL-DUP`, `W-CODE-AFTER-NEXT` →
  `W-CODE-AFTER-JUMP`; `E-TITLE-PLACEMENT` is replaced by `E-INERT-TITLE`.
- `E-DUP-VOICEKEY` now covers every line role.

### Identity

- Section `{#id}` and reward `id` are stable identity metadata; sections and
  rewards without one keep a position / declaration-index fallback reported as
  non-stable. `position` is regenerated on reorder and is never a durable id.

### Tooling

- tree-sitter grammar: sections and `{#id}`, inline modifiers,
  `::actor`/`::cg`/`::sequence`/`::jump`/`::label`, `capabilitySnapshot`
  stamp. Dogfood games, examples, conformance corpus (new 0.37 fixtures),
  docs, handbook and website EN/KO migrated to the 0.37 surface.

## [0.36.6] - 2026-10-06

### Syntax

None.

### Semantics

None.

### Plugin

None.

### CLI

None.

### Diagnostics

None.

### Identity

None.

### IR

Version moves to 0.36.6; no shape or content change — a no-op for consumers.

### Tooling

Performance release. Apart from version stamps, outputs are byte-identical to
0.36.5 over 343 CLI commands on all 14 games: the 0.36.5 matrix plus `lute
test` on single test files with and without `--project`, `lute test
--coverage`, and `lute trace --project`.

- **One project model per command.** `lute test` built the same project's
  model five times: for the tests' analysis, the gate verdicts, the quest ids,
  the producer set, and the play compile. `lute trace` built it up to four
  times. A per-command `lute_model::ModelMemo` builds each (root, options)
  once: two builds for `lute test` (checked, and compiled for quests and
  plays), and each consumer still reports a failed build as before.
  Monster-league, release, median of 5 interleaved runs against 0.36.5:

  | command | wall 0.36.5 | wall 0.36.6 | wall ratio | CPU ratio |
  |---|---|---|---|---|
  | `lute test` (project) | 6.25 s | 4.10 s | 0.66 | 0.76 |
  | `lute test` one test file | 1.16 s | 0.66 s | 0.57 | 0.54 |

## [0.36.5] - 2026-10-06

### Syntax

None.

### Semantics

None.

### Plugin

None.

### CLI

None.

### Diagnostics

None.

### Identity

None.

### IR

Version moves to 0.36.5; no shape or content change — a no-op for consumers.

### Tooling

Performance release. Apart from version stamps, outputs are byte-identical to
0.36.4. The identity check covered 290 CLI commands on all 14 games, the same
matrix as 0.36.4.

Monster-league, release, median of 5 interleaved runs against 0.36.4. The
machine was loaded (load average 30–100), so CPU time (user + sys) is the
figure to read:

| command | CPU 0.36.4 | CPU 0.36.5 | CPU ratio | wall ratio |
|---|---|---|---|---|
| `lute check-project` | 2.55 s | 1.98 s | 0.78 | 0.88 |
| `lute play` spine (144 steps) | 4.61 s | 3.46 s | 0.75 | 0.75 |
| `lute test` | 23.13 s | 17.40 s | 0.75 | 0.64 |
| play project assembly (`calendar --until 1`) | 2.82 s | 2.28 s | 0.81 | 0.94 |

- **Benchmark covers `lute play`.** lute-bench gains `play-assembly` (the
  project `lute play`/`lute test` compile for play) and `play-replay` (every
  `*.play.yaml` of the tier project over one compiled project). The existing
  `playback` phase times the standalone trace runner, which shares none of
  `lute play`'s session code. `lute-cli` now builds a library (`lute_cli::run`;
  `lute_cli::bench` is the harness's entry, not a stable API).
- **Benchmark gate.** `scripts/bench-ab.py` skips, with a note, a phase the
  merge-base binary predates; `scripts/compare-bench.py` reports a head-only
  cell instead of failing and still fails when the head loses one. The
  `performance-approved` waiver reads the label and the PR body when the job
  runs, so a label added after the PR opened counts on a re-run.
- **Datalog closure reused across walks.** A playthrough builds a fresh
  machine per walk, and about 75% of them start from exactly the previous
  walk's world. `Store::derive` keeps the last closure per thread with the
  schema, base facts and state it came from, and returns it on an exact
  match (doubles compared by bit pattern).
- **Walks start from the carried world.** A resumed machine no longer copies
  the declared defaults and re-reads the artifact's seed facts only to
  replace them with the carried world.
- **Command streams shared.** Each project document's commands are decoded
  once per compiled project and shared by every machine over it; a command
  runs by reference instead of being copied first.
- **Schema imports read once per file.** The import cache was keyed by the
  importing document's directory, so the same schema file was parsed and
  validated again for each directory that imports it (monster-league: 190
  reads of 10 files). Each file is now read once per run.

## [0.36.4] - 2026-10-06

### Syntax

None.

### Semantics

None.

### Plugin

None.

### CLI

None.

### Diagnostics

None.

### Identity

None.

### IR

Version moves to 0.36.4; no shape or content change — a no-op for consumers.

### Tooling

Performance release. Apart from version stamps, outputs are byte-identical to
0.36.3. Covered: 290 CLI commands on all 14 games (check-project text/json,
test text/json, every play with `--json --dump-conditions`, `context`,
`trace`, `calendar` text/json/csv, `compile --all` file sets, standalone
`run`). Each binary ran on a corpus stamped at its own version.

Monster-league, release, median of 5 interleaved runs against 0.36.3
(wall / CPU):

| command | 0.36.3 | 0.36.4 | ratio |
|---|---|---|---|
| `lute check-project` | 2.03 s | 1.02 s | 0.50 / 0.54 |
| `lute play` spine (144 steps) | 5.95 s | 2.89 s | 0.49 / 0.51 |
| `lute test` | 12.65 s | 6.23 s | 0.49 / 0.44 |
| play project assembly (`calendar --until 1`) | 1.95 s | 1.33 s | 0.68 / 0.60 |

Profiling (macOS `sample`) found three redundant computations:

- **CEL parsed ~35 times per fragment.** One project check ran cel-parser
  48,183 times over 1,377 distinct texts, about half of the checker's busy
  CPU. `lute-cel` now keeps one parse per thread, keyed by the prepared text
  and holding at most 4,096 fragments of ≤ 1 KiB each. Spans and arena
  handles are still computed per call.
- **A quadratic objective pass.** W-OBJECTIVE-STRANDED / W-SLOT-CONTENTION
  rescanned every document for each objective × beat pair to collect that
  beat's `::set` paths; that was most of project reconciliation. Those paths
  are now computed once per beat.
- **State copied on every evaluation.** Every guard evaluation during
  playback cloned the store's whole state map into an `EffectiveState` and
  dropped it again, which was most of the per-step cost. The new
  `EffectiveState::over` borrows the map instead.

## [0.36.3] - 2026-10-06

### Syntax

None.

### Semantics

None.

### Plugin

None.

### CLI

- **Fixed:** without `--project`, `lute trace`, `lute context` and
  `lute compile-stream` again name the nearest project relative to the
  working directory (`lute: note: using project proj (nearest
  lute.project.yaml) …`, `.` when it is the working directory), as 0.36.1
  did. 0.36.2 printed the canonical absolute path. The `lute-load` crate
  split had pointed those three commands at a second, absolute-path
  `discover_project`; that copy is removed, so a single helper prints the
  note for every command. `crates/lute-cli/tests/project_note.rs` pins the
  note text for all three.

### Diagnostics

None.

### Identity

None.

### IR

Version moves to 0.36.3; no shape or content change — a no-op for consumers.

### Tooling

- **Correction to 0.36.2:** its byte-identity claim was false for the note
  above. The 0.36.2 matrix never ran `trace`/`context`/`compile-stream`
  without `--project`. The 0.36.3 matrix adds those commands, `lute
  calendar` (text, `--json`, `--csv`), and standalone `lute run` on each
  game's compiled artifacts. Each binary runs against a corpus stamped at
  its own version. Apart from version stamps, every row is byte-identical
  to 0.36.1.
- `lute-load`'s public API drops `discover_project`, which no other code
  called.
- `play/calendar/mod.rs` moves its per-cell evaluation (`columns`,
  `evaluate`, `cell_facts`) to `calendar/cells.rs` (841 → 635 lines).
- The public-API CI job reads the `api-change` label when it runs, via the
  API, instead of from the event payload. A label added by `gh pr create
  --label`, or added after the PR opens, now counts on a re-run. README now
  states that the gate compares item paths only.

## [0.36.2] - 2026-10-06

### Syntax

None.

### Semantics

None.

### Plugin

None.

### CLI

None.

### Diagnostics

None.

### Identity

None.

### IR

Version moves to 0.36.2; no shape or content change — a no-op for consumers.

### Tooling

No behavior change: apart from version stamps, outputs are byte-identical to
0.36.1 across 148 CLI commands (check-project text/json, test text/json, every
play with `--json --dump-conditions` on all 14 games, `lute test
docs/examples`, and conformance roots), compile --all (440 files), and 28
`lute context` outputs. nextest passes 3987/3987.

- **C1 ProjectDoc:** project passes read one shared `ProjectDoc` (path, AST,
  typed frontmatter); the remaining frontmatter re-parses and Document clones
  in project assembly are gone. Identity reads keep the authored YAML.
  Regression tests are in `crates/lute-check/tests/project_doc_equivalence.rs`.
- **C2 playback:** the read-set guard cache was measured and not shipped (see
  the 0.36.2 follow-up plan's C2 outcome). Instead, `StoreSchema` is decoded
  once per `ExecProject`: state declarations, rules and vocabulary are no
  longer re-decoded from artifact JSON on every evaluator. CLI wall time versus
  v0.36.1 (release, idle, interleaved x3): `lute play` monster-league spine
  12.3 s → 10.8 s (0.88x), `lute test` monster-league 30.3 s → 27.3 s
  (0.90x). `lute-bench` large tier: all phases within 2% of base.
- **C3 cel_types gap:** keep the current skip (closed profile); tests pin it.
- **C4 public API:** `scripts/check-public-api.py` plus a CI job compares
  rustdoc item lists at the merge base and head. A removal fails unless the PR
  has the `api-change` label.
- **C5 wave-2 splits:** `lute-manifest` `schema/`, `lute-check`
  `reachability/` and `project_check/`, `lute-lsp` `backend/`, `lute-cli`
  `play/calendar/` and `codes/`, and `lute-trace` `trace/`. No file exceeds
  about 800 lines in those modules except data-only `codes/registry.rs`.
- **C6 crate split:** new crates `lute-semantic` and `lute-load`, split from
  `lute-model` per `docs/design/lute-model-crate-split.md` (kept).
  `lute-lsp` and `lute-resolve` no longer depend on `lute-model` or
  `lute-compile`. The public-API gate lists exactly the 32 moved items, making
  this an `api-change` PR. Rebuilding `lute-lsp` after touching `lute-model`:
  15 s → 0 s.

## [0.36.1] - 2026-10-05

### Syntax

None.

### Semantics

None.

### Plugin

None.

### CLI

None.

### Diagnostics

None.

### Identity

None.

### IR

Version moves to 0.36.1; no shape or content change — a no-op for consumers.

### Tooling

No behavior change: apart from the version stamps (`lute version`, artifact
`irVersion`), outputs are byte-identical to 0.36.0 across `lute test
docs/examples`, check-project and compile --all on all 14 games, single-file
checks of every corpus `.lute`, context, diff/patch on every edit task, and
play plus condition dumps. `lute test` assembles each project once
(monster-league release 17.9 s → 15.0 s); project compile reuses the parsed
pre-splice document; CEL path-use parse cache; Datalog join precheck and no
widened-artifact clone per play machine; typed diff/patch model with patch
split into decode/stage/apply/preserve; typed manifest/schema/expand error
enums; exhaustive `lute_cel::walk` for context-free CEL passes; frontmatter
parsed once into TypedMeta for folded-aware passes; module splits (parser,
testcmd, beats, cast, meta, cel_resolve, connectivity); shared CLI test
support; new tests for project compile == standalone compile over every root,
createFile-exists refusal, duplicate frontmatter blocks, and
check-doc-snippets resolving diagnostic-code constants. `lute-bench` A/B versus
0.36.0: every cell 0.97–1.01.


## [0.36.0] - 2026-10-05

**Identity, migration, performance** (phase 5 of
[`architecture-direction.md`](docs/design/architecture-direction.md); spec
[`0.36.0.md`](docs/proposals/scenario-dsl/0.36.0.md)). A clean pre-1.0
cutover: identities that engines persist no longer depend on position once a
project is tagged, renames are declared instead of inferred, and performance is
gated in CI.

### Syntax

- `::use{component="…" instance="…"}` (and `<beat use="…" instance="…">`)
  declares a component instance key: `[A-Za-z][A-Za-z0-9_-]{0,63}`, unique per
  component within its immediate expansion owner. `instance` is an identity
  attribute, never a component parameter.
- Migration: `lute tag <project>` back-fills `instance="use-NNN"` on every
  untagged use (per document and component) alongside missing line codes;
  authored keys are never rewritten, a second run writes nothing. No `lute fix`
  rule is needed: untagged uses still compile.

### Semantics

- Component-expanded identities use `{component}#{instance}` instead of the
  ordinal `{component}#{n}`: inserting a `::use` before another renumbers
  nothing. An untagged use keeps the ordinal as a fallback marked
  `stable: false`.
- `lute.project.yaml` `identity.requireStable: true` makes untagged uses and
  uncoded lines warnings; `identity.renames` declares old → new canonical node
  keys. An entry renames the node and everything the semantic graph contains
  under it; sources must be gone, destinations present, no chains or cycles.

### IR

- `identityRenames` (expanded, sorted `{from, to}` pairs) on the execution IR
  and the project index, omitted when empty; non-empty requires semantic id
  `lute.identity.renames/1`. Engines apply it to saves before reading them and
  refuse the whole migration on an unequal destination collision.
- Component-expanded `lineId`/`voiceKey` and `Source.scope` carry the instance
  key. Schema renamed to `schemas/lute-ir-0.36.schema.json`.

### Plugin

None.

### CLI

- `lute diff` emits `renamed` rows (`from`, `to`, `declarationLocation`) for
  ledger-mapped nodes and marks unmapped save-shaped removed/added pairs
  `unmappedIdentity: true`; `lute patch` `preserve.ids` accepts either side of
  a mapping.
- `lute tag` back-fills component instances; `--force` still never rewrites an
  explicit instance and, under `codesLocked`, refuses the line retag but writes
  missing instances.
- `lute context --target` and LSP hover show identity source, stability and
  component scope.

### Diagnostics

- New: `E-COMPONENT-INSTANCE-INVALID`, `E-COMPONENT-INSTANCE-DUPLICATE`,
  `W-COMPONENT-INSTANCE-UNTAGGED`, `W-LINE-CODE-UNTAGGED` (both warnings only
  under `identity.requireStable`), `E-RENAME-LEDGER`,
  `E-RENAME-LEDGER-STALE`, `E-RENAME-LEDGER-CYCLE`.

### Identity

- Every authorable node kind's identity source, stability and persistence is
  tabulated in [`docs/handbook/core.md`](docs/handbook/core.md). All fourteen
  dogfood games set `requireStable` and are fully tagged; host-line ids are
  unchanged by the migration, component-expanded ids move `#n` → `#use-NNN`.

### Tooling

- `crates/lute-bench`: in-process benchmarks (cold load, project resolution,
  analysis, serialization, playback) over ledger / drowned-crown /
  monster-league; the `Benchmarks` workflow compares the PR against its merge
  base on one runner (7 interleaved samples, fails on median ratio > 1.10;
  `performance-approved` label + PR-body `tier/phase` note to accept).
- Bounded, fixed-seed property tests: formatter idempotence, IR serialize/
  deserialize round trip, parser never panics.
- Every doc under `docs/proposals`, `docs/design`, `docs/superpowers` carries
  a status (Draft/Accepted/Implemented/Superseded/Rejected), and release
  sections carry the seven class headings; both enforced by
  `scripts/check-docs-consistency.py`.
- Faster tests and CI, no behavior change: the edit-task suite no longer keeps
  copies of dogfood games (1,632 files removed) and runs its twelve tasks as
  parallel tests against one model per game, with two tests still applying a
  reference patch for real through `lute patch`; the trace-vs-run
  differential builds each project once; `ProjectModel::graph()` is cached;
  `--dump-conditions` uses a condition index built once (monster-league play
  100 s → 30 s, byte-identical dumps); dev profile `opt-level = 1`; CI runs
  the Rust suites under cargo-nextest beside a separate doc/corpus gate job
  (60 min → under 7 min), and the conformance harness runs once per PR push.

## [0.35.0] - 2026-10-04

### Added

- `lute fmt` provides deterministic, lossless formatting; revisions and
  semantic `lute diff` make changes reviewable.
- The resolver crate powers task context (`lute context --target/--at`) and
  bounded AI authoring inspection.
- `lute patch` applies revision-checked, atomic edits with preserve claims and
  refusal codes. The 12-task `conformance/edit-tasks/` suite exercises the
  edit loop.

### Changed

- The corpus is formatted and CI runs `lute fmt --check`.

### Fixed

- `impact` now lists component-expanded lines of affected beats with graph
  owner attribution.
- Improved semantic diff performance.

## [0.34.0] - 2026-10-03

This release is a clean-cut, breaking change in the **source, toolchain, and
analysis/JSON contracts**. Project commands use one project model; analysis
results expose explicit evidence levels; and manifests may declare constraints.

### Added

- The shared project model, `lute impact` dependency-closure query, and
  `lute constraints` report for `reachable`, `completable`, `speaksOnlyWhen`,
  and `noSingleSlotProgress` declarations.
- Evidence levels `proven`, `witnessed`, `bounded`, `heuristic`, and `unknown`
  across diagnostics and analysis command results, with bounded scopes,
  witnesses, and counterexamples where applicable.

### Changed

- `check-project`, `scenario`, `beats`, `calendar`, `play`, and `test` share
  one project snapshot; `check-project` reports constraint violations only.
- Human bounded/heuristic/unknown wording is explicitly scoped, including
  `W-OBJECTIVE-STRANDED` and `W-SLOT-CONTENTION`; “not found within bounds” is
  no longer described as “no path exists”.
- Project checks reuse prepared analysis state, reducing repeated loading and
  fact-analysis work while preserving results.

### Breaking

- JSON diagnostics and command verdicts gain additive `evidence` fields and
  bounded results gain `scope`; consumers must accept the new contract.
- `lute.project.yaml` now validates `constraints:` declarations strictly.
  There is no legacy loader or compatibility output mode.

## [0.33.0] - 2026-10-03

**Domain modules and semantic negotiation.**

This release is a breaking change in the **IR and engine contract**: the
compiler now declares the engine-visible semantic obligations of each artifact,
and playback engines must negotiate those obligations before executing anything.
The source language remains unchanged; this is a clean pre-1.0 cutover.

### Changed

- The execution-IR envelope gains compiler-derived, sorted, duplicate-free
  `requiredSemantics` immediately after `capabilityVersion`. The project index
  carries the sorted union of its document artifacts; authors cannot hand-edit
  or reorder either field.
- The pre-1.0 exact major.minor IR gate runs first. `lute.engine.yaml` semantic
  negotiation runs second, so an engine accepts only its IR line and every
  required semantic id it supports.
- `lute run --engine <FILE>` and `lute play --engine <FILE>` refuse an
  unsupported semantic id before playback (exit 2,
  `E-ENGINE-SEMANTICS`). `check --engine` and `check-project --engine` perform
  author-time checks instead and report source spans.
- `lute.core/1` is the complete baseline. Enums used only for state typing
  remain core; `lute.knowledge.facts/1` is collected only for relational
  vocabulary, fact deltas, fact queries, or fact metadata.

### Added

- The 0.33 semantic-id registry covers core, staging, timeline, quest
  lifecycle/rewards, clock/cadence/seasons, occasion selection/gates,
  knowledge facts/rules/temporal, and lore.
- Engine matrices use the YAML `engine`, `irVersion`, and unique `supportedIds`
  fields, with optional descriptive `version` and `description`. The built-in
  `reference` matrix supports every current id.
- Diagnostics `E-ENGINE-MATRIX`, `E-ENGINE-IR-VERSION`,
  `E-ENGINE-SEMANTICS`, `E-CHECK-ENGINE-SEMANTICS`, and
  `E-SEMANTICS-UNKNOWN`.
- Conformance fixtures exercise every registry trigger, semantic negotiation,
  malformed matrices, unsupported IR lines, and refusal-before-playback.
- The IR schema line moves to `0.33.0`; consuming engines must use the
  exact-minor pre-1.0 gate.

### Migration

- Recompile and re-record every artifact, project index, and conformance
  fixture. Do not provide `requiredSemantics` in source or edit it in JSON.
- Add `lute.engine.yaml` to each engine integration, declare the exact `0.33`
  IR line and every semantic id implemented by that engine, and refuse missing
  ids before opening a playback session.
- A 0.32 engine refuses 0.33 artifacts at the exact-minor gate. Plugin and
  bridge compatibility remains the exact `capabilityVersion` snapshot; no
  plugin semantic id is added.


## [0.32.0] - 2026-10-02

**Standard CEL, owned state, and versioned execution IR.**

This release has breaking changes in three classes: **syntax** (fact functions
use list-form calls and `number` becomes `int`/`double`), **semantics** (closed
standard CEL profile, strict numeric typing, activation and error policy), and
**IR** (the artifact is now named execution IR and carries typed `expr`,
`celEnv`, engine ownership, and grant identity).

### Migration

- Run `lute fix` to rewrite legacy fact calls, `isSet`, and related forms.
- Retype numeric declarations: integral counters to `int`; fractional,
  division-derived, and other non-integral declarations to `double`.
- The corpus declarations retyped to `double` are recorded here: **none**;
  `grep 'type: double' docs/examples` finds no declarations beyond those
  already typed as double in the release corpus.
- CLI fact inputs (`--fact`, `--axis holds(<fact>)`, and mock `facts:`) retain
  ground-fact notation; only CEL condition calls change.

### Changed

- Conditions are standard CEL from source to IR (closed profile, spec
  `docs/proposals/scenario-dsl/0.32.0.md` §1–§4): fact queries are
  `holds('rel', ['a', '_'])`, `count`, `countDistinct('rel', [...], col)`,
  `validAt('rel', [...], t)`; presence is `has(a.b)` or `'k' in a.b`; numeric
  types are `int` and `double`, and mixed `int`/`double` operations are
  rejected. Inside a `"`-delimited attribute, CEL strings use single quotes.
- Every IR condition slot is `{cel, expr, authored?}`; `expr` covers the full
  profile with typed literals. The IR carries `celEnv` (roots and host-function
  signatures), `owner: "engine"` on engine-owned state, and an explicit `is`
  field on `<when is=…>` match arms.
- Activation and error policy are normative: reserved quest and lore-entry
  paths are always present with their reserved defaults; an erroring condition
  is not satisfied; an erroring `::set` value halts the walk.
- `lute run` refuses an execution IR whose major.minor differs from the
  toolchain's while the IR is pre-1.0 (exit 2); from 1.0 the gate is MAJOR
  only.
- `W-QUEST-STATE-ISSET` is renamed `W-QUEST-STATE-HAS`.
- Quest re-instantiation (new run, `rearm`, season reset) increments a
  save-wide quest instance number, persisted in saves as `questInstances`.

### Added

- New diagnostics `E-FACT-QUERY` (list-form fact-query shape) and
  `E-RUN-OWNED-WRITE` (`lute run` refuses an IR that writes an engine-owned
  path or a reserved relation); `E-CEL-PROFILE`, `E-CEL-TYPE` and
  `E-STATE-DECL` cover the removed forms and the numeric rules.
- Grant events carry quest `instance` and reward `index`;
  `(quest, instance, objective, index)` is unique within a save.
- `lute run` and `lute play` accept `--dump-conditions <file>`: one JSONL record
  per condition evaluation with the activation it read, the facts and visited
  set, and Lute's result.
- Conformance: the corpus covers every command kind, grant identity, owned-write
  and stale-minor refusals, and CEL profile cases. CI replays it plus every
  example play with `--dump-conditions` and re-evaluates each record with
  `@bufbuild/cel`, `cel-go` and an `expr` walker; all must agree with Lute.

## [0.31.0] - 2026-09-29

**Declared beat clock movement and schedule diagnostics.**

`advances: slot`, `advances: day`, or `advances: <n>` lets a scene, lore entry,
or bundle beat declare that presenting it moves the project clock. `lute play`
performs the move with the same quest settlement and clock raises as an explicit
`advance:` step. A duplicate explicit step is retained for compatibility but
emits a note; remove it so the declaration remains the source of truth.

### Added

- `W-OBJECTIVE-STRANDED` warns when a required objective's only completing beats
  have clock-bounded windows but the objective has no `until=` or `by=` failure
  deadline. Run-tier quests report this as informational because a new run
  retries them.
- `W-SLOT-CONTENTION` warns when two required run-tier objectives in one quest
  can only complete at the same single clock position and each completing beat
  advances the clock.
- `lute play` includes a lifecycle beat raised by an `advances:` beat in the
  presented list and transcript, including when the advancing beat ends through
  a choice branch.

### Compatibility and migration

- Existing content without `advances` is unchanged.
- Migrate plugin `spendsSlot: true` to `advances: slot`; migrate
  `spendsSlots: n` to `advances: n`. The engine, not the plugin, moves time.

## [0.30.0] - 2026-09-28

**Names as written, paths like JavaScript.**

A minor release that settles what 0.29 and 0.29.1 left split. Every name a
condition reaches by path, index or fact argument — scene, beat, entry, quest,
objective, branch, hub, choice and mark ids, document id segments, `share`
keys, seasons, relations, kinds and members, what a plugin declares, targets
and categories — is a name: letters, digits, `_` or `-`, not starting with
`-`, so `lamp-out`, `zero-coke-001` and `001` are written as the engine
already spells them. A name that is not an identifier is reached in a
condition the way JavaScript reaches it, with a quoted index
(`quest["zero-coke-001"].state`) or a quoted fact argument
(`holds(at("lab-b2"))`), and every surface — the checker, `lute trace`,
`lute test`, `lute play`, `lute context`, `lute calendar` and the language
server — reads both spellings as one name. Only a name read bare as `@name`
(defs, def params, component and template params) stays an identifier. The
language and the IR both earn the move (the IR adds no field; its path fields
may now carry such names); see
[`docs/proposals/scenario-dsl/0.30.0.md`](docs/proposals/scenario-dsl/0.30.0.md)
and [`docs/versioning.md`](docs/versioning.md).

### Added

- `lute trace --state`, the `state:` of a mock, test or play, `expect.state` and `lute calendar --axis` take a state path in either spelling: `--state 'run.visits["lab-b2"]=1'` seeds the same path as `--state run.visits.lab-b2=1`. A fact seed or expectation may quote an argument, `facts: ['at("lab-b2")']`.
- `lute trace`, `lute test` and `lute play` evaluate a quoted index into a state map (`quest["zero-coke-001"].state`, `run.visits['lab-b2']`) and a quoted fact argument (`holds(at("lab-b2"))`) as the dotted path and the bare name, including inside a rule body's `cel(…)`. A refusal names a path the way a condition writes it (`` `run.visits["lab-b2"]` is 0 ``) and a `--fact` hint quotes such an argument.
- `lute context`'s outline and `lute calendar`'s table show a member that is not an identifier as a condition reaches it (`run.visits["lab-b2"]: number`); the language server's hover, go-to-definition and references resolve a bracket path in a condition or a `{{…}}`.
- `lute check` reads a quoted index as the path segment it names everywhere a condition is written — a guard, a `<match on>` subject, a `::set` / `::assert` / `::retract` target (`::set{run.visits["lab-b2"] += 1}`), a `{{…}}` interpolation, a fact-query argument (`holds(at("lab-b2"))`, `count(…)`), a Datalog rule's body, head and comparison (`at(X, "lab-b2")`, `"lab-b2" = X`), and a component or template argument bound into a family index or fact. Both spellings of a name check as one path: declared-ness, types, literal domains and definite assignment are the same. A member reached per kind member (`run.visits[occasion.target]`, a rule guard's `run.visits[P]`) is judged under the spelling that member needs.

### Changed

- One name rule: every name an author writes — scene, beat, entry, quest, objective, branch, hub, choice and mark ids, document id segments, `share` keys, seasons, relations, enum and entity kinds and members, the occasions, events and enums a plugin declares, targets and categories — is letters, digits, `_` or `-`, not starting with `-` (a dotted id joins names with `.`). `lamp-out`, `zero-coke-001` and `001` are names. A `.`, whitespace, a quote or any other character is an error at the name under that slot's code, and the message lists the allowed characters instead of a camelCase rename. A name read bare like a JavaScript variable — a def, a def's param, a component or template param (`@name`) — stays an identifier (a letter or `_`, then letters, digits or `_`); the fault says it is read bare and names the identifier spelling. 0.29.1's split between Lute-declared names and engine-owned ids is gone: a target on a kind that lists its members (`npc.old-man`) and a category follow the same rule.
- The published JSON Schemas follow the name rule: the name patterns in `schemas/lute-ir-0.30.schema.json` (seasons and `season:<name>`, occasions, `share` keys, `series`, document ids) and the `stateWrites` / `factWrites` patterns in `schemas/lute.project.json` accept a name that has `-` or starts with a digit.

### Compatibility

- Names 0.29 refused for containing `-` or starting with a digit are accepted in every slot; the reserved-name table (`E-RESERVED-NAME`) is unchanged.
- The artifact keeps its shape. Path fields (`set` targets, `state` entries, an `expr` node's `path` / `isSet` / `has`, a `{{…}}` placeholder's `path`) carry the canonical dotted path — `run.visits.lab-b2`; a segment may contain `-` or start with a digit and never contains `.`. A condition's `raw` ships as written, so it may reach a member with a quoted index, which CEL over nested maps reads as the dotted member; fact arguments in `assert` / `retract` and seed facts are the bare names.
- **Restamp `luteVersion:`.** A document or `defaults:` stamped with an older version draws `W-LUTE-VERSION-STALE`, which names the stamp to write (`luteVersion: "0.30.0"`); bump the stamp, or `--deny-warnings` fails the project.
- **Schema file renamed; the IR adds no field.** The version strings move to `0.30.0` and `schemas/lute-ir-0.29.schema.json` is renamed to [`schemas/lute-ir-0.30.schema.json`](schemas/lute-ir-0.30.schema.json) (`$id` updated, name patterns widened). Engines gate on MAJOR, so nothing widens.

## [0.29.1] - 2026-09-28

**Engine-owned ids keep their spelling.**

A bug-fix patch on the `0.29` line: no new syntax and no IR shape change. The
0.29 identifier rule stays for every name Lute declares and stops reaching the
ids an engine owns — a `target` whose members no `entities:` kind lists and an
entry's `category` accept `-` again, as they did in 0.28.1; artifacts differ
only in the version strings. See [`docs/versioning.md`](docs/versioning.md)
for what each axis earned.

### Fixed

- The 0.29 identifier rule no longer reaches ids the engine owns. A name Lute declares is an identifier; an id the engine owns is written as the engine spells it, the way a CEL map key is string data. A `target` on an `<entry>`, `<beat>`, `<objective>`, `<on>` or scene `target:` whose members no `entities:` kind lists (no occasion, an occasion declared `target: true`, an `open:` kind) and an entry's `category` (engine vocabulary) accept `-` again, as in 0.28.1: `<entry id="key" target="item.rusty-key">` checks clean instead of `E-ENTRY-ATTR`, and the target message describes the shape it checks. A target on a kind that lists its `members:` names a declared member, so `npc.old-man` stays `E-BEAT-ATTR` (outside the domain).

## [0.29.0] - 2026-09-28

**One identifier rule, intent you can state, games as examples.**

A minor release that settles what 0.28 left open the way that leaves the
author one rule to remember. Every name an author writes — scene, beat, entry,
quest, objective, branch, hub, choice and mark ids, document id segments,
`share` keys, seasons, relations, defs, kinds and members, component params,
and what a plugin declares — is one identifier: a letter, then letters, digits
or `_`; a `-` is an error at the name naming the camelCase spelling, where
0.28 refused it only where CEL reads the name. A warning that can describe an
intended design has a spelling that states the intent — there is still no
generic suppression — and `terminal:` gains the long form
`{ when, persists: true }` for an ending that outlives runs on purpose.
`lute check-project` prints causes first, `W-CHAPTER-STALL` stays specific to
`chapters:`, and `lute new scene --occasion` writes the `on:` it was asked
for. The fourteen dogfood games ship as warning-free examples under
`docs/examples/games/`, each with a README, and CI checks them. The language
and the IR both earn the move (the IR adds the optional `terminalPersists`);
see
[`docs/proposals/scenario-dsl/0.29.0.md`](docs/proposals/scenario-dsl/0.29.0.md)
and [`docs/versioning.md`](docs/versioning.md).

### Added

- `terminal:` long form `{ when: "<condition>", persists: true }` states that an ending outlives runs on purpose (a roguelike's permanent ending): `W-TERMINAL-PERSISTENT` is silent, and `lute play` says the game is over for good (the game-over note, the note on a `newRun` after it, and the refusal of a later raise) instead of offering `newRun`. The long form's keys are closed (`E-META-VALUE` names the key a slip meant); `persists` is `true` or `false`. `persists: true` on a condition that reads only run state is `E-META-VALUE`. The IR (artifact and `project.index.json`) carries `terminalPersists: true` beside `terminal`; `lute context` shows it.

### Changed

- `W-TERMINAL-PERSISTENT` names the long form as the way to say the ending persists on purpose. `W-CHAPTER-STALL` stays specific to `chapters:`: a hand-written `after:` outside a chain never warns (now pinned by a test and documented).
- One identifier rule: every name an author writes — scene, beat, entry, quest, objective, branch, hub, choice and mark ids, document id segments, `share` keys, seasons, relations, defs and def params, enum and entity kinds and members, component params, and the occasions, events, enums and defs a plugin declares — is a letter, then letters, digits or `_` (a dotted id joins them with `.`). A `-` or any other character is an error at the name under that slot's code (`E-META-ID`, `E-BEAT-ATTR`, `E-ENTRY-ATTR`, `E-PATH-IDENT`, `E-SEASON-DECL`, `E-COMPONENT-PARSE`, `E-PLUGIN-PARSE`), naming the camelCase spelling (`lamp-duty` → `lampDuty`).
- `lute check-project` prints causes first: rows at `lute.project.yaml`, then plugins, then schemas, then each document in path order, then the project-wide rows.
- `lute new scene --occasion O` always writes `on: O`; when `O` carries a `chapters:` chain it adds how to make the scene a chapter instead (list it in the chain, drop `on:`).

### Compatibility

- Additive: `terminal:` long form; IR `terminalPersists` (omitted when false) in `schemas/lute-ir-0.29.schema.json`; `schemas/lute.schema.json` accepts both `terminal:` forms.
- A name with `-` that 0.28 accepted (`share=` keys, document ids, branch/hub/choice/mark ids, enum and entity members, plugin occasions/events/enums) is now an error; rename it to the camelCase spelling the message gives. The scaffolded and example vocabularies are renamed: `action` members `fade-in-up`, `fade-in-slow`, `slide-in-left`, `walk-in`, `pose-turn`, `pose-lean`, `fade-out`, `fade-out-down`, `fade-out-slow` are now `fadeInUp`, `fadeInSlow`, `slideInLeft`, `walkIn`, `poseTurn`, `poseLean`, `fadeOut`, `fadeOutDown`, `fadeOutSlow`, and `musicAction`'s `fade-out` is `fadeOut` — an engine keyed on the old strings maps the new ones.
- `check-project` output order changes (causes first); `--json` `project_diagnostics` follows the same order.
- `lute new scene --occasion` on a chained occasion writes `on:`.
- **Restamp `luteVersion:`.** A document or `defaults:` stamped with an older version draws `W-LUTE-VERSION-STALE`, which names the stamp to write (`luteVersion: "0.29.0"`); bump the stamp, or `--deny-warnings` fails the project.
- **Schema file renamed; the IR is additive.** The version strings move to `0.29.0` and `schemas/lute-ir-0.28.schema.json` is renamed to [`schemas/lute-ir-0.29.schema.json`](schemas/lute-ir-0.29.schema.json) (`$id` updated). The one new field, `terminalPersists` on the artifact and `project.index.json`, is optional and appears only when every `terminal:` declaration says `persists: true`. Engines gate on MAJOR, so nothing widens.

## [0.28.1] - 2026-09-27

**Leftovers from the sixth dogfood round.**

A bug-fix patch on the `0.28` line: no language change and no IR shape change.
Diagnostics name one cause where they named several, and `lute play`,
`lute lore` and `lute trace` answers are put right; artifacts differ only in
the version strings. See [`docs/versioning.md`](docs/versioning.md) for what
each axis earned.

### Fixed

- A state path written with a `-` in a name (`quest.lamp-duty.state`) is one
  `E-PATH-IDENT` naming the path, how CEL reads it (`quest.lamp - duty.state`)
  and the camelCase spelling, instead of `E-CEL-PROFILE`, `E-CEL-TYPE` and
  `E-UNDECLARED` about the subtraction it parses as. In `check-project`, a
  read of a quest or entry id with a `-` is not reported beside the id's own
  `E-PATH-IDENT`, as for an id with a `.`, so the cause is the one report.
- A rule guard comparing a rule variable with a path typed by an enum that
  lists some of the variable's kind's members (`onRoute(S) :- suitor(S),
  cel("run.route == S")` with `route: [none, alone, ren, …]`) is accepted and
  holds for the shared members, as the same comparison in a beat's `when`
  does; only a kind or enum sharing no member is `E-FACT-DOMAIN`.
- A `chapters:` chain on an occasion no plugin declares is not applied, as a
  malformed chain is not: `lute beats` no longer lists its scenes under the
  misspelt occasion.
- A gated line whose `when` a clock that ends can never reach
  (`when="run.hour == 'h05'"`, `when="clock.index == 9"`) says why in its
  `E-ARM-DEAD`, as a `<match>` arm and a beat's `when` already did.
- A misspelt `done=` on an `<objective>` (`complete=`, `doen=`) is one
  `E-UNKNOWN-ATTR` naming `done`, without an `E-OBJECTIVE-MISSING-DONE` beside
  it; a relation's misspelt `teir:` is one `E-RELATION-DOMAIN` naming `tier`,
  without a `W-RELATION-TIER-IMPLICIT` beside it.
- An entry id with a `-` (`file-walter`) names the one-word spelling
  (`fileWalter`), as a quest id does.
- `day:` written twice in a schema's `clock:` says the day count is `days:`.
- A `::set` the engine refuses (`::set{scene.choices.door = true}`) is one
  report, the refusal, without an `E-SET-TYPE` about its value beside it.
- Yarn's `$oil` in a condition is one `E-CEL-PROFILE` naming the path, without
  an `E-DOLLAR-OUTSIDE-MATCH` at the same column.
- A choice or member named `true`/`false` is refused once, where it is named;
  the `is="true"` that meant it is not also `E-WHEN-LITERAL-DOMAIN`.
- A scene with a `title:` and no `id:` (an Ink knot or Yarn node name) is told
  the id its title spells (`title: Lamp_Room` → `id: lampRoom`), at the
  `title:` line.
- In `lute play`, an `::end` in the step that ends the game prints
  `(this presentation ends)` without `the play goes on`, since the game-over
  note follows it.
- The Ink/Yarn guide maps Yarn's `#line:` id to a content line's `code=` (it
  said `id=`, which is a jump target), and the Korean guide carries the
  0.28.0 corrections: a choice id is not a jump target, and `<return>` runs
  when the last `once` choice empties the hub, which the Choices & hubs page
  now says too.
- `W-BEAT-PRIORITY-TIE` with a scene whose priority `chapters:` derived says
  so, and asks for a different `priority` on the scene that wrote its own.
- `<reward target="keepsake.skyStone">` says to write the bare member
  (`target="skyStone"`); a reward kind whose `target: { entity: … }` names a
  kind the document's schemas lack says the kind is missing (with a
  did-you-mean), not to import a schema that is already imported.
- `<entry target="kind:K">` on a `select: sequence` occasion raised for no
  target says to write `for="kind:K"`, as a `<beat>` does.
- A scene the retired `sequence:` (or a rejected chain) lists gets one
  `E-BEAT-ATTR` naming all its beat keys, not one per key.
- `lute lore` lists the facts a `<beat use>` template's directive call
  asserts (`gave(maud, kettleLid)` from `::gift{from=@who …}`), and `lute lore`
  / `lute scenario knowledge` name such a producer once, as the component at
  its use.
- `lute trace --occasion O@T` refuses a target outside `O`'s domain, and one
  other than a fixed-target beat answers; `departure@npc.maud` for a `for=`
  entry on an untargeted occasion says to write `departure@maud`.

## [0.28.0] - 2026-09-27

**One rule per slot, one name per idea.**

A large minor release from a sixth dogfood round: a whole-language review of
eleven games plus four new personas (a novice, an Ink / Yarn writer, a
live-ops designer and a confusability audit). Every accepting place checks the
same way — every condition slot runs the same typed checks, every YAML surface
refuses unknown keys with a located did-you-mean, reserved words are refused
where they are declared and flag attributes are written bare. One name per
idea: the manifest's `sequence:` is `chapters:`, `<quest after=>` /
`<reward on=>` / `<objective when=>` are `follows=` / `outcome=` /
`visibleWhen=`, `expect.offered` is `expect.options` and `expect.exit` is
`expect.end`, each old spelling an error naming the new one, and messages call
a `::next` target a *mark*. `spentBy` latches, `occasion.target` can be written,
`select: sequence` re-judges each beat at its turn, and the language fills its
gaps: `clock.day` / `clock.slot` / `clock.ended` and `raiseAtStart:`,
season-tier relations, `outsideRun:` occasions, a hub's `<return>` block,
label forms and number words. Text written in another language's markup
warns, and messages put the cause first, once. The language and the IR both
earn the move (the IR renames three quest-layer fields and is otherwise
additive); see
[`docs/proposals/scenario-dsl/0.28.0.md`](docs/proposals/scenario-dsl/0.28.0.md)
and [`docs/versioning.md`](docs/versioning.md).

### Added

- The clock reads by name: `clock.day` and `clock.slot` are read-only aliases
  of the clock's day and slot paths, and a finite clock (`last:` / `days:`)
  has `clock.ended` — `false` until the `advance:` that ends the clock, true
  from that advance's settle (so `by="clock.ended"` fails a deadline at the
  moment the clock ends, and `terminal: "clock.ended"` ends the game there,
  before the last `dayEnd` is raised — which the calendar shows `not
  raised`, and a beat only that raise would take is `W-BEAT-UNRAISED`)
  until a `newRun` starts a run-tier clock over. All three are `owner:
  engine`, typed (`clock.slot` is the slot enum) and in `lute context`. A
  `lute play` step checks the moment with `expect: { clock: { ended: true } }`
  (a usage error on a clock that never ends).
- A relation's `tier:` and the manifest's `defaults.questTier` accept
  `season:<name>`, like a quest's `tier=` and a beat's `once:`. A
  season-tier relation's facts go back to the seed facts each time the
  season opens (the play's `season … opens` line names those relations); an
  undeclared season is `E-SEASON-DECL`.
- An occasion may declare `outsideRun: true` (a title screen, a gallery
  between runs): the engine raises it even after the project's `terminal:`
  holds, and the checker does not judge its beats under `!terminal`. The IR
  lists such occasions in `outsideRun`.
- Text that is another language's markup warns instead of shipping silently:
  `W-TEXT-SINGLE-BRACE` for a single-brace `{run.oil}`, `{@def}`, Yarn
  `{$oil}`, Ink conditional text `{run.oil > 0: …}` and alternatives
  `{~Fog|Mist}` in line text or a choice label (interpolation is `{{…}}`;
  conditional text is a guarded line or a `<match>`); `W-TEXT-COMMENT-LIKE`
  for a ` // note` or a trailing Ink `# tag` inside text; and
  `W-TEXT-BRACKET-LABEL` for a choice `label="[Go inside]"`, whose brackets
  show on the button.
- `W-SEASON-UNGATED`: a beat with `once: season:<name>` (scene, `<entry>`,
  `<beat>`) or a `tier="season:<name>"` quest whose `when` / `start` does not
  imply the season's `live` condition warns, naming the condition to add —
  `once` only sets how long the beat stays spent, so such a beat played while
  the season had never opened.
- `E-BEAT-ID-DUP`: an `<entry>` and a `<beat>` of one lore document with the
  same id is an error at the later one — the beat's canonical
  `<document id>.<id>` was also the entry's alias, so `visited()` and a play's
  `expect.winner` named both. A repeated `<beat id>` moves to this code too
  (it was `E-BEAT-ATTR`).
- A hub can say something each time the player comes back to it: a
  `<return>` block inside `<hub>` runs after every non-`exit` option's arm,
  before the options are judged and shown again (never before the first menu,
  never after an `exit`). It is checked like an option body, compiles to its
  own segment named by the hub record's `return` field, and runs the same in
  `trace`, `run`, `play` and `test`. A second `<return>`, one outside a hub
  or inside an option is `E-LOGIC-CONTENT`; an attribute on it is
  `E-UNKNOWN-ATTR`.
- Label forms and number words. A kind's `labels:` entry may be `{ text, start, indefinite }`
  besides a string; `{{…:start}}` renders the `start` form (else the text capitalized) and
  `{{…:indefinite}}` the `indefinite` form (else `a` / `an` by the first letter, then the text).
  `:capitalize` upper-cases the first letter of any text placeholder, `:cardinalWord` spells
  `one` … `twenty` (digits above), and in a `plural(…)` form `#word` / `#Word` is the number as a
  word. The IR placeholder carries the new `format` values (also on `occasionTarget` and
  `reserved` placeholders), and `entities[]` / `state[]` entries carry `labelForms`; `lute play`
  and `lute trace` render them alike.
- `W-QUEST-REARM-CONSTANT`: a `rearm=` that folds to a constant never turns from false to true,
  so the quest never rearms.
- `W-BRANCH-ID-SHARED` (`lute check-project`): two documents declare a `<branch>` or `<hub>`
  with the same id, so one `choose:` key in a play or test answers both menus.
- A type may name the clock's enums: `{ domain: clock.slot }` and
  `{ domain: clock.weekdayLabel }`. A component that shows the weekday takes a
  param of that type instead of copying the labels into an enum, and its
  `<match>` arms and literal arguments are checked against the clock's labels.
  A component param of any closed `{ domain: K }` is now judged over K's
  members too (it was treated as open text: a misspelt `<when is>` passed).
- `occasion.target` can be written, not only read, in a beat or entry that targets a kind or runs for each member of one: `::set{run.count[occasion.target] += 1}`, `::assert{caught(occasion.target)}` / `::retract`, a directive attribute typed by an entity kind or domain (`::haul{fish=occasion.target}`, its declared effects included) and a component argument (`::use{component="reaction" who=occasion.target}`, a `speaker` param speaking as the member). Each write is checked once per member, and `lute play` writes only the member the beat ran for. Outside such a beat the write is one `E-UNDECLARED`. In the artifact a `set` path keeps `F[occasion.target]`, an `assert`/`retract` argument and a plugin field keep `occasion.target` for the engine to bind; a component argument expands to one `match` arm per member. A `::set` path indexed by a literal (`run.count[cod]`) is one `E-SET-SHAPE` naming `run.count.cod`, instead of three errors and a did-you-mean that made no sense.
- A relation declared without `tier:` warns, `W-RELATION-TIER-IMPLICIT`: its
  facts silently reset every run. Write `tier: run` to keep that, or the tier
  the facts should outlive.
- An enum and an entity kind with the same name are `E-DOMAIN-NAME-CLASH`,
  whether declared in one document or merged from several schemas.
- A clock may declare `raiseAtStart: true`: the engine raises the clock's
  slot occasion (and a `raise:` map's `dayStart`) itself where a run starts,
  where no `advance:` stops. `W-BEAT-UNRAISED`, `W-CHAPTER-STALL` and `lute
  calendar --axis clock` then count the run's first position as raised; the
  artifact's `clock` carries the key (omitted when `false`). `lute play` is
  unchanged: a script still plays that raise with an `occasion:` step, which
  without the key notes that the clock does not raise it there. Without the
  key, `W-BEAT-UNRAISED` no longer says a beat that holds only where the run
  starts never plays: it says the clock does not raise the occasion there,
  and to declare `raiseAtStart: true` if the engine raises it when a run
  starts, or else to answer an occasion raised where the beat holds.

### Changed

- Messages and docs call a `::next` target a **mark** (the target is declared
  with `::mark{id}` or a content line's `id=`), no longer a "label", which
  kept meaning `<choice label>`, `labels:` and a play step's `label:` too.
  `E-NEXT-UNDEFINED` now reads ``::next` targets `x`, which no `::mark` or
  line `id=` in this document declares`` and names the document's closest mark
  when one is near (``did you mean `ending`?``); `E-NEXT-BACKWARD` and
  `E-MARK-DUP` say "mark" too.
- The `lute init` project and the docs' examples name their place occasion
  `townVisit` (scenes `town.welcome`, `town.morning`, `town.dayEnd` under
  `scenes/town/`) instead of `hubVisit` / `hub.*`, which read as the `<hub>`
  element.
- A test's menu-choice expectation is `expect.options: { <branch or hub id>:
  [option ids] }`, the name a play step already uses; in both files `options`
  are a branch/hub's menu choices and a play's `offered` lists beat
  candidates. A test's top-level quest list is only `accepts:` (the `accept:`
  spelling is gone, in `--mock` files too). A key written in the wrong file
  says which file reads it: a test's `payload:` or `winner:` names the play
  step, a play's `accepts:` or `file:` names the `*.test.yaml`, and a play's
  `expect.offered: { br: [a] }` map says menu choices are `options:`.
- `::camera`'s pan attributes are `moveX` / `moveY`, camelCase like every
  other core attribute. An unknown directive attribute now suggests the
  nearest declared one (`move-x` → ``did you mean `moveX`?``).
- Did-you-mean is one helper everywhere: it ignores case and `_`/`-`
  (`kind="gold"` → `GOLD`, `changed_on` → `changedOn`, `kind: Scene` →
  `scene`, `tier: Run` → `run`) and knows neighbouring words that are not
  typos (`complete=` → `done=`, `questTier=` → `tier=`, `test=` → `when=`,
  occasion `when:` → `raisedWhen:`, `rearm` ↔ `spentBy`, `<else>` →
  `<otherwise>`). `E-UNKNOWN-ATTR` lists the element's attributes, an unknown
  content-line attribute lists the line's, `E-UNKNOWN-EVENT` names the events
  and the nearest one (`questCompleted` → `questComplete`), and a relation's
  unknown key names the relation keys (`derived` → `derive`).
- `<else>` / `<default>` inside a `<match>` is one error naming `<otherwise>`;
  its body and close tag no longer add two more.
- A target that is not an id no longer quotes a grammar
  (`Ident ("." Segment)*`): the message says what an id looks like, and an
  `<objective>` or `<on>` handed `target="kind:crew"` says it takes one
  target — kind targets are for beats and entries.
- The `share` without `once` message lists every spending period (`week`
  and `season:<name>` were missing).
- `check-project` leaves a beat the per-file check rejected (an error in its
  declaration, or a template use whose header is faulty) out of
  `W-BEAT-PRIORITY-TIE` and `W-BEAT-SHADOWED`: the error is the one report.
  A tie's reason comes from what the author wrote (`when`, `once`,
  `spentBy`, `after:`), never from a fact the beat's own body asserts.
- Three quest-layer attributes are renamed so each word keeps the meaning it
  has elsewhere: `<quest after=>` is `<quest follows=>` (quest-graph metadata;
  it never gates the quest — to wait, write `start="visited('…')"`),
  `<reward on=>` is `<reward outcome=>`, and `<objective when=>` is
  `<objective visibleWhen=>` (it only hides the objective; it never gates
  `done`). The IR follows: `ObjectiveEntry.visibleWhen`, `RewardEntry.outcome`,
  and a quest's `prereqEdges` row carries `follows` instead of `after`.
- The seam — the project's `terminal:` and an occasion's `raisedWhen` gate —
  is decided when the occasion is raised. A `judge: before` judgement that
  makes `terminal:` hold no longer closes the occasion's own beats (the
  epilogue plays; the game is over after the step).
- A `select: sequence` raise judges each beat again just before its turn,
  once an earlier beat of the raise has played: a beat whose `when` an
  earlier beat made false is skipped (and one it made true plays). `lute play`
  marks such a candidate "(judged at its turn, after an earlier beat of this
  raise)".
- A `for="kind:<kind>"` beat is spent per member: its `once` (run, user, day,
  slot, week, season) counts each member's presentation separately. A `for`
  entry's writes apply on each member's first read in the run (the engine
  keeps `entry.<id>.readFor.<member>`; `entry.<id>.read` holds once any member
  was read).
- `check-project --wip` reports a dead guard it spares as the warning `W-WIP` instead of printing the error code at warning severity (`warning [E-ARM-DEAD]`); the message names the code the guard has without the flag (`… — \`E-ENTRY-UNREACHABLE\` without \`--wip\`: …`). The diagnostics reference documents the rendering.
- An unrecognized body line written in Ink or Yarn says what Lute writes
  instead: `-> knot` names `::next{to}`/`<hub>`/occasions (`-> END` names
  `::end`), `~ x = …`, `VAR`, `CONST`, `<<set>>` and `<<declare>>` name
  `::set{…}` and `state:`, `* [..]`/`+ [..]` and Yarn `-> option` name
  `<choice>` in a `<branch>`/`<hub>` (`once` for `*`), and `- gather`,
  `=== knot ===`, `= stitch`, `<<if>>`, `<<jump>>` and Yarn headers name their
  Lute forms; prose with no speaker says "a content line needs a speaker". The
  "cannot span multiple physical lines" note appears only for a line that
  continues the one above. `::goto`/`::jump`/`::divert` (and other misspelled
  directives) get a did-you-mean, `::greet{…}` for a component names
  `::use{component="greet"}`, a `::next` target that is a heading, a choice
  or a menu id says so, and a backward `::next` names `<hub>` for loops.
- In text, a kind's `labels:` entry wins over a cast `name:`:
  `{{occasion.target}}` and a component argument typed `{ entity: K }` /
  `{ domain: K }` render the label (a speaker head keeps the cast name).
  `W-LABEL-CAST-SHADOWED` is retired: the label it said was never shown is
  now shown.

- `spentBy` latches: a beat is spent by its condition instead of by being presented, and once the condition has held (in the settled world after any settle — never mid-settle, so a `start="true"` quest is already `active` when the latch first reads it — or when the beat is judged; per member for a kind or `for=` beat) the beat stays spent for its `once` period even if the condition turns false again. `once` beside `spentBy` is now legal and sets that period (`run` unless written; `user`, `day`, `slot`, `week`, `season:<name>`); a presentation of a `spentBy` beat spends nothing. `lute play` / `lute calendar` / `lute test` name the latch (``spentBy: `run.solved` held — spent this run``) and a kind beat's member (``… holds for cod``); a scene or bundle `spentBy` beat's IR `once` is its period (`run` when unwritten) instead of `none`. `W-BEAT-SPENT-AT-START` judges the condition at the start of play — every state path at its declared default, the seeds, no scene visited, a quest `active` when its `start` holds there (`start="true"`) and `unset` otherwise, no entry read, a quest in another document included — so `spentBy: "!run.balloonUp"` is caught, and its hint names the `when: "!(…)"` rewrite (with `once: false` to repeat) for a beat meant to play while something does not hold; a `spentBy` whose literal comparison is already reported draws neither spentBy verdict. New warning `W-SPENT-BY-REVERSIBLE` (`check-project`): a `spentBy` beat with no `once` written whose condition can turn false again after it has held — it reads a fact some `::retract` / `::assert` or a directive's declared effect undoes, a season's state, facts or quest, or a quest `rearm` returns to `unset` — names the retract (or season, or rearm) and the rewrite: `once: false` + `when: "!(…)"` to judge it afresh, `once: season:<name>` for a season, `once: run` to keep the latch on purpose.
- `<entry once="false">` is accepted as the omission's meaning (a repeatable entry beat); a bare `<entry once>` / `<beat once>` names the accepted values; a quoted frontmatter scalar where a bool or number is meant (`once: "false"`, `priority: "10"`, `also: "true"`) says "the quoted string … — write it unquoted".
- The manifest's `sequence: { occasion, scenes }` is now **`chapters:`**, a list of chains `[{ on, scenes }]` — one chain per occasion, so a project can chain several. A malformed chain is reported (`E-CHAPTERS`, located at its line) and not applied; the other chains still are, and a scene a rejected chain lists is told so instead of "list the scene in `chapters:`". A chain on an occasion raised for a target requires every listed scene to declare `target:` (one without would play for every target). A listed id that names a bundle beat, a lore entry or a document says so; a bare last segment suggests the scene (`c4s1` → `main.c4s1`). `W-SEQUENCE-STALL` is now `W-CHAPTER-STALL` and fires only on a `when:` that can stay false for good — one over state the story may never set, or over a clock window that closes (no raise of the chain's occasion meets it, or none after the scene before it may have played, as for a slot on the last day of a clock that ends); a clock condition a later raise still meets only delays the chain, and one over other `owner: engine` state is the engine's to make true — judged on the next scene's effective `after:` whoever wrote it, with advice that changes the outcome. `W-SEQUENCE-ORDER` is `W-CHAPTER-ORDER`. A derived key's diagnostics point at the scene's `id:` (never into the body) and say "(written by `chapters:` in lute.project.yaml)"; play/trace reasons credit an `after:` to `chapters:` only when the chain wrote it, never by comparing text.
- `lute new` names the file after the id and keeps the case typed (`isles.harborNight` → `id: isles.harborNight`; `The Epilogue` → `theEpilogue.lute`); quest and lore document ids get no `quest.`/`lore.` prefix; `--on` is now `--occasion`; an occasion raised for a target requires `--target`; a scene for a chain's occasion says how to list it, in the words of that occasion's `select:`.
- A frontmatter key that belongs to another layer names that layer (`rearm`/`tier` → the `<quest>` attribute, `raisedWhen`/`select` → an occasion's declaration, `sequence`/`chapters`/`questTier` → lute.project.yaml, `terminal`/`clock`/`seasons`/`cast` → a schema, `use` → `uses:` or `<beat use>`, `tags` → `extra:`); a scene's `occasion:`/`event:` says the key is `on:`, once, without a "without `on:`" error per beat key.
- An attribute that another construct or layer owns names where it lives: `<entry occasion=>`/`<beat event=>`/`<objective occasion=>` say the attribute is `on=`, `<on occasion=>` says `event=`, `<quest on=>` points at `<objective on=>` and `<on event=>`, `<beat rearm=>`/`<beat tier=>` say they are `<quest>` attributes (a beat comes back with `once=` or `spentBy=`), and `<quest spentBy=>` says `spentBy` is a beat attribute. A beat or handler that wrote its occasion under another name gets that one error, not a second "names no occasion" / "has no `event`". A play step's `on:` says the step key is `occasion:`, and `for:` says it is `target:`.
- `lute play` and `lute test` report every usage error of a play script at once, one per line, in the order they were written (a step spliced in by `include:` at its `include:` line); each list entry (`facts:`, `visited:`, `presented:`, `expect.facts`, …) is located at the entry. `lute play --json` always carries `end` (`complete`, `terminal`, `incomplete` or `error`); `lute test` compares a `*.test.yaml` walk's `end` too (`terminal` when the project's `terminal:` holds where it ended).
- `lute beats` shows every ladder's gate in its header (`sluice — select: first · raisedWhen: run.depth >= 3`, then `· gate never holds` when it never does), a `for` beat as `sendoff.word (for kind:bonded)` (`--json` rows carry `for` and `forKind`), an unspent `once` as `none` in text and JSON alike, and `--occasion talk@npc.mira` says to write `--occasion talk --target npc.mira`. `lute calendar` names the member each `for` presentation is for (`days.note for npc.mira`, text, CSV and JSON), and `--occasion talk@npc.maud` where `npc.maud` is a target of `talk` says to use `--target` (`@` there names axes).
- `lute scenario reach --endings` also lists the beats whose writes can make the project's `terminal:` hold (`ends: writes `user.crowned`, which `terminal: user.crowned` reads`), and carries an occasion's `raisedWhen` on a `gate:` line with what it needs, so a gated beat without `when` no longer reads "always holds".
- `lute lore` substitutes a rule's variables in its `cel()` premise (`cel("run.aff.ren >= 3")`, as `lute scenario knowledge` does) and names where each rule is declared (`(rule at world.schema.yaml:8)`; `scenario knowledge` prints it after the rule); its first section is "Entries and beats by target", a scene beat is labelled `scene`, a `for` beat shows `(for kind:<kind>)`, and a fact a component asserts is listed where it is used, its params bound (`components: keepsake (via scene `talk.sefa.gift`)`), instead of missing (`lute lore`) or as a nameless `scene` matching every fact (`scenario knowledge`).
- `lute calendar` and `lute test` name themselves when the project does not compile (`lute calendar: 1 of 2 document(s) failed to compile; refusing to evaluate the calendar`), not `lute play`.
- Plugin load errors are located and plainly worded: every `E-PLUGIN-*`
  line from `plugin.yaml` or an export file leads with `path:line:column:`,
  and no serde or Rust wording reaches the author (`data did not match any
  variant of untagged enum OccasionTarget`, `invalid type: sequence,
  expected a map`, `struct AttrDecl`, `String("lantern-fest")`). An occasion
  `target:` names its bad key and the key meant (`kind`/`domain` →
  `entity`, `member` → `members`); a top-level key of the wrong shape says
  what it holds (`occasions:` is a map, `events:` a list); a YAML slip gets
  the plain sentence with its fix. `E-PLUGIN-RESERVED-NAME` points at the
  declaration's file and line and says what the name already is.
- A season may map straight to its condition: `seasons: { harvest:
  "@harvestLive" }` is `{ live: "@harvestLive" }`. A season named
  `lantern-fest` says why a name is an identifier and offers `lanternFest`.
- Beat templates: a `<beat use=…>` that writes its own `when=` still replaces the template's `when:`, and now says so — `W-TEMPLATE-OVERRIDE` at the use's `when=` names the template, the dropped condition, the arguments only that condition read, and both ways to keep it (write both conditions, or conjoin a template param such as `@only`).
- Beat template headers accept `F[@param]`, the spelling a component body uses: `when: "user.bond[@who] >= @need"` reads `user.bond.<who>`. The dot form `user.bond.@who` still works in a header and gets `W-TEMPLATE-DOT-PARAM` pointing at the bracket form.
- A beat template header may declare `also: true` (every use rides along after the winner), and a header `title: "{{@who}}"` renders the argument — a `speaker`'s cast name — instead of copying `{{isolde}}` into the artifact.
- A misspelt engine path names the shapes its namespace holds and the one
  meant: `quest.q1.status` → `quest.q1.state`, `entry.e1.seen` →
  `entry.e1.read`, `objectives.o.complete` → `objectives.o.done`.
- A refused CEL-profile call says what to write instead: a bare relation atom
  → `holds(knows(x))`, `completed(q)` in a condition →
  `quest.q.state == 'complete'`, `$oil` → `run.oil`, an unquoted
  `visited(a.b)` → `visited('a.b')`. An `after:` outside its profile quotes the
  subexpression as written.
- A `per:` may name a kind another schema declares; it expands against the
  merged kinds. A path declared both as a value and as a prefix of other rows
  (`run.lanterns` and `run.lanterns.gold`) is `E-STATE-DECL`.
- `lute play`, `lute test` and `lute trace` parse each CEL condition once and
  reuse it: a 176-step Lantern Academy play runs in about half the time 0.27
  took.

### Fixed

- `lute.project.yaml` no longer drops an unknown key silently (`terminal:`, `sequnce:`, `defualts:`, a profile or `identity:` typo): each is `E-MANIFEST-KEY` at its line, with a did-you-mean or the layer that owns it, and the documents are not checked under an invalid manifest. A manifest that does not parse or lacks `defaultProfile:` is one located `E-MANIFEST` in plain words. `E-DEFAULTS-KEY` is located too.
- A `state:` row with an unknown key (`defualt:`, `ownr:`, `reserved:`, `tier:`) or an inline enum `default:` that is no member is `E-STATE-DECL` at the key; an `enums:` long form with an unknown key or no `members:` is `E-META-VALUE` at the key. Engine-owned namespaces (`scene.choices.*`, `scene.visited.*`, `occasion.*`, `quest.<x>` and `season.<x>` without a field) cannot be declared (`E-STATE-NAMESPACE`, at the path), and `::set` of `scene.choices.*`, `scene.visited.*` or `occasion.*` is `E-QUEST-RESERVED-WRITE` alone, without an `E-UNDECLARED` beside it. A `lute play` `engine:` or `newRun:` write to one of them is refused before anything plays, naming what the script writes instead: the `occasion:` step's `target:`/`payload:`, `choose:`, or top-level `visited:`.
- A frontmatter opened with `---` and never closed is one `E-META-PARSE` naming the line to close it after (with a fix), not seven unrelated errors; a frontmatter value holding `: ` gets the quoted fix instead of YAML's "mapping values are not allowed". An unknown frontmatter key (`E-META-UNKNOWN-KEY`) is reported at the key, not at `1:1`.
- A flag attribute given a value no longer turns silently false. Every flag —
  `<choice once>`, `<choice exit>`, `<objective optional>`, `<beat also>` —
  reads through one reader in the parser, the checker and the compiler: bare,
  `="true"` and `="false"` mean what they say (`optional="true"` is optional,
  `exit="true"` is an exit and no longer draws `E-HUB-NO-EXIT`), and any other
  value is the new `E-FLAG-VALUE` at the attribute, naming the bare form. A
  beat/entry period on a choice (`<choice once="run">`) says a choice's `once`
  means once per hub visit. `<beat also="maybe">` moves from `E-BEAT-ATTR` to
  `E-FLAG-VALUE`.
- On a finite clock that starts and ends on the same day, the slot path (and
  `clock.slot`) only holds the slots the clock reaches: `run.hour == 'h05'`
  past `last: { day: 1, slot: h03 }` is `E-BEAT-UNREACHABLE`, a `<when
  is="h05">` arm is `E-ARM-DEAD`, and a `<match>` needs no arm for it. The
  reasons name the clock's end, as a dead `clock.index` guard's now does too.
- A clock whose day path outlives the run (`user.*`, `app.*`) keeps its
  `once: day` / `once: slot` / `once: week` spends and its end across a
  `newRun`: they used to be cleared, so a beat came back the same week. The
  play's new-run step says so (``the clock is kept: its day `user.dive`
  outlives the run, …``).
- A clock's day path defaulting below day 1 is `E-CLOCK-DECL` (it used to
  play a day 0). `last: { days: N }` says `days:` is the clock's own key;
  `week.labels` written as a map says it is a list, one label per weekday.
- `::set` of the clock's day or slot path says to move the clock with an
  `advance:` step (an `engine:` write moves it without raising anything).
- `W-QUEST-TIER-IMPLICIT` suggests `tier="season:<name>"` for a quest whose
  conditions read one season's state, instead of `tier="run"`.
- A diagnostic's column counts characters, not UTF-8 bytes, on every surface:
  `lute check`, `check-project`, `compile`, `lint`, `--json` `span.column` and
  `lute scenario … reach` `causes[].column` used to put an error after Korean
  text or an em dash several columns too far right (a line of 72 characters
  reported column 82). YAML files already counted characters; the language
  server keeps UTF-16. The diagnostics reference states the unit.
- `E-WHEN-LITERAL-DOMAIN` and `E-UNSET-LITERAL` on a condition point at the
  offending quoted literal (`'rne'` in `when="run.route == 'rne' && …"`), not
  at the start of the attribute value.
- The language server attaches `relatedInformation` to a diagnostic about an
  imported schema's or plugin file's declaration (a schema `terminal:` fault,
  say): the file and its range, where the diagnostic used to sit at the
  importer's `1:0` with no pointer. While that file is open in the editor it
  shows the fault too, pointing back at the importer.
- `lute trace` and `lute test` halt incomplete when a hub's scripted
  `choose:` list runs out while the hub is still open, as `lute play` does;
  they used to leave the hub and pass on a path no player can take. The
  unresolved line names the longer list (`choose: { askCook: [scullery,
  <oven|leave>] }`); `lute play`'s halt says the hub is still open after its
  scripted picks.
- A lore test that presents no entry (judging `eligible:` by id) asserts
  `facts:` / `notFacts:` on the seeded world — the mock's facts, the
  project's seeds and what the rules derive — instead of an empty one, where
  every fact "did not hold" (a 0.27.0 regression). A lore test may now carry
  `facts:` / `notFacts:` alone.
- `E-OCCASION-GATE` names what the gate read: a family read by the raised
  member says ``since `user.bond.ines` is 0`` (it said `user.bond` was unset),
  a negated fact gate names the fact that holds (``since `fell(isolde)`
  holds``), and a gate over `occasion.payload.*` says to raise it with a
  payload that satisfies it (`payload:` on this step). The gate is quoted as
  written (`@stageReleased`), not expanded; the IR's `gates[].raisedWhen`,
  `terminal` and `seasons[].live` carry that text as `authored` beside the
  expanded `raw`. A refused raise's step says `(not raised: gate false)`
  instead of `(no candidates)` (`--json`: `notRaised`).
- A refused scripted pick whose guard reads a derived fact suggests mocking
  the base premises its rules miss (`facts: ["recruited(wren)"]`), not the
  derived conclusion.
- Spec section numbers and ticket ids no longer reach a reader: `--help` of every command is free of `(dsl … §…)`, `(spec §5)`, `(T6)`, `(T3-20)` and `round-5` notes, and diagnostics drop the citation forms the output filter missed (`(Appendix C)` on `E-DATALOG-PARSE`, dated designs such as `(dsl 2026-08-31 §4)`, `(plugin §7)`, and prose-embedded ones like `since dsl 0.26.0 §4`); `--json` still carries them in `spec`.
- A `kind: lore` document accepts `pov:` in its frontmatter (its `<beat>` bundles are scene bodies) instead of `E-META-UNKNOWN-KEY`.
- Schema diagnostics point at the key at fault: an entity-kind label for a non-member (`musicRom:`), an `enums:` label for a non-member (now with a did-you-mean), an unknown entity-kind key (`lables:`), and each `seasons:` entry problem are reported at that key instead of the kind, enum or `seasons:` line; a `labels:` problem beside an `add:` is no longer reported at `1:1`, and the entity-kind shape error states that `labels:` may sit beside `add:`.
- `E-ENTITY-KIND-CLASH` is reported where the clash was made — the `add:` (or this document, or the later schema) that lists the id a second time — and names both places with their lines.
- `check-project` over a whole project (a `lute.project.yaml` root) reports a `quest.<id>…` read no quest defines as `E-QUEST-REF-UNKNOWN`, with a did-you-mean; when `<id>` is a quest document's `id:` it names the quest that document declares.
- A quest or objective id with a `.` (`isolation.hush`) is `E-PATH-IDENT` where it is declared, instead of every read failing `E-UNDECLARED`.
- A kind read only by a `for="kind:<kind>"` beat (scene `for:`, `<beat for>`,
  `<entry for>`) no longer draws `W-DOMAIN-UNREAD`; it is read like
  `target="kind:<kind>"`.

- Comparisons are typed at check time (`E-CEL-TYPE`): `visited('gallery') > 2`, `run.oil == true`, `run.day == 'monday'`, `clock.weekday == 'Wednesday'`, `run.hour >= 'h03'` on an enum, a number or string operand of `&&`/`||`/`!`, and arithmetic on a bool used to check clean and then halt play or stay silently false. Each message names the fix (`clock.weekdayLabel == 'Wednesday'` or `clock.weekday == 2`; `run.hour in ['h03', 'h04']`; `visited()` is a bool, not a count). A mistyped comparison is the one report — the guard it makes false is no longer also `E-ARM-DEAD`/`E-BEAT-UNREACHABLE`. `<when is="<=3">` on a number subject is `E-WHEN-LITERAL-DOMAIN` naming `..3`.
- Every condition slot runs the literal-domain checks a `when` gets: quest `rearm=`, an entry's `when=` (now `E-WHEN-LITERAL-DOMAIN` with a did-you-mean instead of only `E-ENTRY-UNREACHABLE`), `spentBy`, a season's `live:`, a rule `cel()` guard, and an occasion's `raisedWhen:` comparing `occasion.target` with a typo or a prefixed member (`'place.village'` → did you mean `'village'`?). A beat whose `when` is false only because of such a literal gets the literal error alone. `terminal:` and a season's `live:` reading `occasion.*` are one `E-UNDECLARED` (they are judged outside any occasion) instead of per-document errors.
- A path a def body reads is checked where the def is used: `@a` whose body reads an undeclared `run.nmae` is `E-UNDECLARED` naming the def, with a did-you-mean, instead of checking clean and halting play.
- A def whose body is one indexed family read (`bondNow: "user.bond[occasion.target]"`) takes the family's type instead of `E-DEF-DECL … cannot be inferred`.
- `lute context` now says what the checker enforces. `quest.<id>.state` lists `unset`; `reservedQuestPaths` lists every reserved path of every quest in the project (`state`, `failedBy`, `activatedAt`, `objectives.<o>.done` / `.failed`), not only those the document reads; a new `enginePaths` section names every engine-owned shape (`entry.<id>.read` / `everRead`, `occasion.target`, `occasion.payload.<field>`, `scene.choices.*`, `scene.visited.*`, `clock.*`, `prev.*`), and `scene.choices.*` rows are marked `owner: engine`. `occasion.payload.<field>` shows on its occasion, no longer as state. `builtinDirectives` adds `::body`, `::next{to=…}`, `::mark`, `::end`, `::clear` and `::accept{… at="nextRun"}`, and a `directiveAttrs` section lists `when=`, `duration=`/`delay=`/`wait=` and `at=` (a `<track>` clip only). Directives show attribute types and `effects:` (writes, asserts, retracts); occasions show `judge:` and `outsideRun`; a `speaker` param prints `speaker`; a component with a `beat:` header is marked a template. Quest, objective and reward keys print in attribute spelling (`start="<condition>"`) from the checker's own lists (`follows=`, `visibleWhen=`, `outcome=`), and `chapters:` chains say what they derive.
- `lute trace` frames a component expansion once, named: `-- component bondStory begin --` … `-- component bondStory end --`, with a template's `::body` marked `-- body --` … `-- body end --` inside it — no empty begin/end pair, and a beat ending in a `::use` closes its frame. A plugin call reads `::gift{from="maud" item="kettleLid"}` instead of a bare `<gift>`, and its declared effects say `(effect of ::gift)`, as `lute play` does.
- `lute trace` and `lute test --coverage` call a line `when=` guard a guard: ``guard `holds(owned(nyx))`: skipped``, and a coverage row ``guard `…` (file:line:col): taken; never skipped`` instead of ``match … 1/2 arm(s) executed [otherwise]``. A `<match>` expanded from a component `::use` is its own coverage row at the component's `file:line:column`, naming the use (`card#1 in scenes/s.lute`), instead of merging into the host construct at the same position. Coverage paths are relative to the directory `lute test` runs in (no absolute project paths), a test that resolved no document prints no empty `()`, each JSON unit carries `doc` and `local`, and the JSON `notPresentedByPlay` no longer repeats the units already `untested`.
- `{{n:plural("One morning"|"# mornings")}}` printed its quotes; quoted forms and a `,`
  separator (`plural(coin,coins)`) are now `E-PLURAL-FORM`, whose message shows the bare
  `|`-separated rewrite. An unknown hint names the nearest one and lists every hint.
- `<quest repeatable="true">` (or `repeat=`, `once=`, …) now points at `rearm="<condition>"`.
- A subquest's `rearm=` was accepted though the child, once its parent ended, stayed `unset`
  for good; it is now `E-SUBQUEST-REARM` (per document and across documents).
- A play's `visited:` seed now spends that scene's `once: user` (and its `share` key), as a presentation does: a save cannot hold a visited scene that is still unspent.
- An atom with unbalanced parentheses no longer blames YAML's comma split when it is a lone quoted entry (`"empty(ada"`); the quoting hint appears only when the next entry closes it.
- Reserved names are refused where they are declared (`E-RESERVED-NAME`),
  each message naming what the word already means and a name to use instead.
  They used to be accepted there and misread where they were used: an entity
  member or def named like a state root (`clock`, `run`) read as state in
  `holds(found(clock))` / `@run`; an enum or entity member `unset`, `true`,
  `false`, `null` or `_` could never be matched; a number, boolean or `null`
  in a member list vanished silently; a scene, beat, entry or choice id
  `none` shadowed play's `pick: none` / `winner: none`, and a choice id
  `true` drew a self-contradictory `E-WHEN-LITERAL-DOMAIN`; a CEL keyword in a
  state path or quest/objective/entry/branch/hub id (`quest.in.state`) could
  not be parsed; a season named `run`, `user`, `week`, `day` … sat beside the
  `once` period of the same name; a cast entry for `narrator` was never shown
  and gated every narrated line. Relations also refuse `completed`/`active`
  (the `after:` calls) and `cel`/`not` (rule words), and a plugin occasion
  named like an engine lifecycle event (`questComplete`) or a play step
  (`newRun`, `end`, `advance`) is `E-PLUGIN-RESERVED-NAME`. The one list
  (`lute_manifest::reserved`) is what `lute --explain E-RESERVED-NAME` prints
  and the new reference page *Reserved names* shows.
- A plugin directive may omit `attrs:` (a directive without attributes
  failed the whole plugin with `missing field attrs`).
- `lute play` (and `lute test`/`lute calendar`) print a plugin load error
  once, not once per document, and then refuse with the usual line instead
  of exiting 1 without a word.
- One mistake in one template argument is one report: a header key that reads a missing param or an argument the use-site checks refuse is not derived, so a missing `need` no longer adds `E-UNDECLARED-REF @need`, `need="lots"` no longer adds `E-CEL-PROFILE`, a missing `season` no longer adds `E-BEAT-ATTR`/`E-SEASON-DECL` for `once: "season:@season"`, and a mistyped `speaker`/member argument no longer adds target and fact errors. `E-BAD-ENUM` now carries a did-you-mean, and names a template use `<beat use="…">` instead of `::use{…}`.
- A condition a template header derived is reported at the use with the header key it came from ("template `x`'s `beat.when: …`") and its location in the component file; the entity-indexed `E-UNDECLARED` message lists the reads that work (`F.<member>`, `F[occasion.target]`, `F[@param]`, a rule variable).
- A component used only through `<beat use=…>` counts as used: `lute check` on it no longer reports `W-COMPONENT-UNVERIFIED`.
- `<entry use=…>` is one `E-TEMPLATE` ("beat templates apply to `<beat>` only") instead of an `E-UNKNOWN-ATTR` per attribute.

- A `<match>` inside a hub option's own arm that needs `scene.visited.<hub>.<option>` (or `scene.choices.<menu>`) to be anything but the value that pick records is `E-ARM-DEAD`: the record is set when the choice is picked, before its arm runs. A `<match>` right after a `<branch>` that always picks (an unguarded choice, no `timeout`) no longer demands an `unset` arm for `scene.choices.<branch>`; where the record can still be unset, one `E-MAYBE-UNSET` names the branch and why.
- `occasion.payload.<field>` is readable in an `<objective on="O">`'s `done` / `until` / body and in an `<on event="O">` handler of a declared occasion `O`; a read in an objective's `by` / `visibleWhen` says the payload is bound only while its occasion is raised.
- After the game is over, a `terminal:` that reads state a new run keeps (`visited(…)`, `user.*`, …) is refused with "it still holds after a new run: it reads …" instead of advising `newRun: true`.
- `lute doctor` compares the `lute-lsp` an npm/bun launcher on `PATH` starts with the one beside `lute`, byte for byte, instead of trusting the version the launcher reports; when it cannot tell which binary the launcher starts, the line says `(compared by reported version only)`.
- `lute scenario … reach --endings` reads an ending whose `when` never holds because of a literal (`run.route == 'rne'`) as `never holds — caused by E-WHEN-LITERAL-DOMAIN at …` again, and fails the command, now that the literal error is that `when`'s one report.
- An illegal `once` / `tier` value that names a declared season (`once="neap"`, `once: season.neap`, a near miss, a relation `tier:`) now suggests `season:<name>`, and a quest `tier="Run"` suggests `run`; a scene whose legacy `season:` key holds a declared season's name is an `E-SEASON-DECL` error saying to write `once: season:<name>` and/or the season's `live:` as `when:`, and `defaults: { season: <name> }` / `defaults: { questTier: <name> }` say the same; an undeclared `once: season:<x>` is reported at the value, not column 1.
- A document that writes its own `components:` or `uses:` still replaces `defaults.components` / `defaults.uses`, but the errors that causes now say so: `<beat use>` of a component only the defaults import, and an unknown relation, kind, def, state path or cast member only a default schema declares, name the replaced list and the file to add.
- `visited('<id>')` on a quest id says `<id>` is a quest and to write `quest.<id>.state == 'complete'` (`completed('<id>')` in an `after:`); a label beside an `add:` for a member the `add:` does not bring says labels there cover only the added members, instead of "not one of its members"; the `beats` and `investigation` templates name their quest documents `lamp.quests` / `case.quests`, so the document id no longer reads like a `quest.<id>` path.
- `lute test --json` rows carry `end` — `complete`, `terminal`, `incomplete` or `error`, the values `expect.end` names (`null` when nothing was walked); `exit` stays the exit class.
- Test and play hints name canonical ids. The ineligible-walk hint suggests `eligible: { ending.lit: false }`, not the bare `lit`. The mock hint is an instruction ("add `visited: [keeper]` to the mocks"). A bare id that is the last segment of exactly one known dotted id gets a did-you-mean everywhere suggestions are made (`winner: edith` → `isolation.edith`). A test's `file:` that resolves from a directory above the test says so: "`file:` is relative to the test file; did you mean `../scenes/garden.lute`?".
- Play scripts: an `include:` may carry a `label:`, which labels every step it splices in that has no label of its own. A step's own `choose:` key that none of its presentations presented gets the same "never used" note an `include:`'s does. A halt at a choice with no scripted decision names the ended `include:` that carried that decision unused.
- `lute test` prints a halted play's halt first (`halted: …`) and folds the end-of-play misses it caused into one line.
- `occasion@target` spellings: a play step's `occasion: talk@npc.mira` says to write `occasion: talk` + `target: npc.mira`. `lute trace --target t` is refused with the `--occasion <occasion>@t` spelling trace takes. A map entry in a test's or mock's `occasions:` names the joined string (`talk@npc.mira`). `lute compile --all <DIR>` takes the project directory positionally.
- One cause, one report. A schema import whose YAML does not parse is its importers' only report (one `clock: { day: 1 }` duplicate key gave 198 errors, now 1). A rejected `clock:` no longer adds `once: day`/`week` "declares no `clock:`" errors or `clock.*` reads. A state row that was itself reported (a `default:` outside its enum, an unknown row key, a refused member) is not judged again at its reads: no `E-UNDECLARED`, `E-MAYBE-UNSET`, `E-STATE-MAYBE-UNAVAILABLE`, `E-NONEXHAUSTIVE`, `E-UNSET-UNCOVERED` or literal-domain error follows it. A `<match>` with an `unset` arm that misses a member is `E-NONEXHAUSTIVE` alone, without `E-STATE-MAYBE-UNAVAILABLE`. Content before the first `##` (or between lore and quest blocks) is one `E-CONTENT-OUTSIDE-SHOT` naming the lines it covers, not one per line and per close tag. On one position the cause prints before what it causes, and a schema's own reports print in line order.
- `lute play`: a scripted choice refused because its guard misses a fact names what in the project asserts that fact (the scene or entry and the choice option) and what the play chose there, or says nothing asserts it, instead of suggesting a mock.
- `lute calendar`: a cell whose occasion's `raisedWhen` is false reads `gate false` instead of `-` (CSV `notes`, JSON `gated: true`).
- `lute play`, `lute calendar`, `lute run`, `lute test` and `lute trace` usage errors and play halt messages no longer carry spec section citations; a play usage error inside `lute test` prints one `error:` line per error.
- **A raise without its declared payload is caught**: a `lute play` step raising an occasion whose `payload:` declares a field it leaves out is a usage error naming the field (`add \`payload: { seconds: <number> }\` to the step`); `lute trace` / `lute test` halt incomplete on a line reading an unseeded `occasion.payload.<field>` (supply `--state occasion.payload.<field>=…`) instead of printing the placeholder raw; a clock `raise` naming an occasion with a payload is `E-CLOCK-DECL`.
- **Vacuous test assertions are refused**: a quest test raising a mocked occasion the engine would not raise (its `raisedWhen` is false, or `terminal:` holds) is refused with `E-OCCASION-GATE`; a `transcriptContains` / `transcriptLacks` needle shaped like a report line (`quest wire -> failed`, `wire.sent failed (by)`, `set run.x = 1`, `→ h`) is `E-TEST-NEEDLE` in tests and plays; `eligible: { <id>: { false: <reason> } }` names the premise that must close it (`once`, `after`, `spentBy`, `when`, `gate`, `terminal`).
- **`failedBy: subquest`**: a quest failed by a required subquest's failure now reads `quest.<id>.failedBy == 'subquest'` (was `fail`) and prints `quest X -> failed (subquest)`.
- **`lute test` reads a quest's reserved paths before any write** as `lute play` does: `quest.<id>.objectives.<o>.failed` / `.done` read `false`, `quest.<id>.state` / `.failedBy` read `unset`.
- A name a plugin package declares twice (`E-PLUGIN-DUP-ID`) or two plugins declare (`E-PLUGIN-DUP-ACROSS`) is reported at the later declaration's file and line, naming the earlier one; the first declaration is used and `check-project` still checks every document. A state path two imports declare (`E-USES-DUP-STATE`, and the other `E-USES-DUP-*`) is reported at the later import in `uses:` order with project-relative paths, and no longer drags in errors that follow from the conflicting type.
- `lute lint` (and the LSP's opt-in lint) reads each scene with the keys the manifest's `chapters:` derives, as `check-project` does: a scene that gets its `on:` from a chain is a beat, so `L-SHOT-STARTS-WITH-BACKGROUND` and `L-SCENE-LENGTH-SPREAD` no longer treat it as a linear scene. That includes a scene that continues the one before it in the same raise of a `select: sequence` occasion, which plays on the stage that scene set and needs no `::bg` of its own.
- `<when is="dawn,dusk">` whose comma-separated parts are each members says to separate alternatives with `|` (`is="dawn|dusk"`) instead of only "not a member".
- `E-HUB-NO-EXIT` on a hub with a choice named `exit` that lacks the flag points at that choice: "choice `exit` is not an exit — add the `exit` flag".
- `W-CAST-ABSENT` treats a beat's or entry's occasion gate (`raisedWhen:`) and `!terminal` as guards, like its `when`: a line on a `shower` beat raised only while Sol is on the roof no longer warns that Sol may be absent.
- Editor hovers and the project-resolution errors the LSP publishes no longer cite spec sections.
- A `target="kind:<kind>"` on an occasion raised for no target says to write `for="kind:<kind>"` instead when the occasion is `select: sequence` (in a scene, `for: "kind:<kind>"`), and why not otherwise; the beat's reads of `occasion.target` are no longer reported again as undeclared. A `for=` naming a kind without its `kind:` prefix gets a did-you-mean, and a scene's `for:` messages use the frontmatter spelling.
- A scene or beat `when: once` / `when: always` (Yarn's spelling) says that how often a beat plays is its `once` key, with its values, instead of calling `once` an unknown name.
- A `for` beat's members spent for the same reason print as one `lute play` line (`✗ id for ren, mika … — reason`), a closed season window is named as the last one, and a season opening again makes every member of a `for` beat with `once: season:<name>` eligible again (before, only a beat without `for` was).
- `lute new` refuses a name whose id another scene, quest or lore document already declares (exit 2, nothing written), naming that file — document ids are one namespace, now that quest and lore ids get no `quest.`/`lore.` prefix; a lore entry listed by its document-qualified id (`codex.page`) in `chapters:` is named a lore entry, as its bare id already was.
- A component with a `beat:` header writes its body without a `## ` heading, like a bundle beat:
  its content no longer draws `E-CONTENT-OUTSIDE-SHOT`, and `<beat use>` expands it (`::body`
  included). An ordinary component still needs the heading.
- A component may pass its own `speaker` param on to a nested component (`::use{component="inner"
  who=@who}`): no more "must be a literal cast id", "`@who` is an expression" or a false "`run.aff`
  is not a `per:` state family here". The host judges the id where it binds it, through the
  nested component (`who="narrator"` for a `run.aff[@who]` write is `E-COMPONENT-ARG` at the host's
  `::use`).
- A component body that reads a def (`@narrator{when="@late"}`) is one `E-COMPONENT-STATE` at the
  read, naming the param to declare (`late: { type: bool, default: "@late" }`). The component's own
  check reports it (it used to pass), and a host's `::use` gives the same report, so
  `check-project` shows it once instead of "`@late` is not a declared def" per use. `lute beats`
  notes a template use whose `when=` replaces its template's `when:` — ``(replaces template
  `gift`'s `when: …`)`` beside the guard, and `replacesTemplateWhen` in `--json`.
- A lore document with kind beats of different kinds (`target="kind:villager"` beside
  `target="kind:fish"`, or `for=`) types `occasion.target` in each beat by that beat's own kind:
  a `<match on="occasion.target">` in the fish beat needs arms only for fish (it used to demand
  `mara` and `tomas` too), and `occasion.target == 'ada'` in a place beat is an
  `E-WHEN-LITERAL-DOMAIN` naming only the places, no longer an `E-BEAT-UNREACHABLE` blaming the
  occasion's gate.
- A kind or `for=` beat's `when` is judged once for each member it runs for. A member it can
  never hold for (`holds(at(sol, occasion.target))` where no rule puts Sol on the deck) needs no
  `<match on="occasion.target">` arm, and an arm only for such members is `E-ARM-DEAD`; a beat
  whose `when` holds for none of its members is `E-BEAT-UNREACHABLE` (an entry,
  `E-ENTRY-UNREACHABLE`) naming the members, the way a beat for one member already was. Before,
  a `for=` beat was never reported and a kind beat only once a `terminal:` was declared, with a
  reason naming that `terminal:`.
- `lute play`: an `advance:` that passes positions without raising the
  clock's `slot` occasion (raised once, where the clock stops) says so in a
  note naming them by day and how many beats answer the occasion —
  `` passed day 1 (Mon) afternoon, night without raising `slotStart` (2 beats
  answer it; …) ``; `--json` lists them under `advance.passed`. An `advance:`
  step's `presented:` may be keyed by occasion (`presented: { dayStart: [a],
  slotStart: [b] }`), judging only the named raises; a miss names the
  occasion, and a key the clock does not raise is a usage error with a
  did-you-mean. `lute calendar --axis clock` follows the clock's raise map:
  `dayStart` only at a day's first slot and never on the day the run starts,
  `dayEnd` only at a day's last slot, the slot occasion anywhere but where
  the run starts; every other cell reads `not raised` (`notRaised` in
  `--json`) with no eligible or shadowed verdicts, and a beat only such cells
  would take is listed as never eligible. `--occasion dayEnd@clock.day`
  holds the slot at the day's last.
- The checker reads a guard's clock conditions together: a conjunction of
  the day path, the slot path, `clock.index`, `clock.day`, `clock.slot` and
  `clock.weekday` that no clock position satisfies is provably false
  (`E-BEAT-UNREACHABLE`, `E-ENTRY-UNREACHABLE`, `E-ARM-DEAD`,
  `W-DEADLINE-NEVER`), naming why — ``no clock position has `run.day == 3`,
  `run.slot == 'afternoon'` and `clock.index > 7` ``. On a finite clock of
  several days the slot path is narrowed on the last day: ``the clock ends
  at its last position, so on day 2 `run.hour` only holds h23, h00, h01,
  h02``. New `W-DEADLINE-BEFORE-WINDOW`: an objective whose `done` can only
  hold once its `by=` deadline already does (a `visited('<beat>')` whose
  beat's `when`, directly or through a derived relation's `cel()` rules,
  opens after the deadline) fails before it can be done; the warning names
  the first position `done` can hold and where the deadline already holds.
- An occasion `payload:` field type that is unknown (`nubmer`), incomplete
  (`enum` without members) or takes more than one value (`list`, `record`,
  `map`) is `E-PLUGIN-PARSE` at the field, naming the forms a payload field
  takes (`bool`, `number`, `string`, `{ enum: […] }`, `{ domain: K }`,
  `{ entity: K }`) with a did-you-mean, instead of serde's `unit variant,
  where newtype variant is expected`.
- A relation `tier:` must be exactly `scene`, `run`, `user`, `app`, `quest` or
  `season:<name>`. `tier: season.lanterns` or `tier: run.x` used to pass and
  ship a tier no engine resets; each is now `E-RELATION-DOMAIN` at the `tier:`
  key with a did-you-mean (`season:lanterns`).
- A state path typed `{ domain: K }` or `{ entity: K }` whose K nothing
  declares is `E-DOMAIN-UNKNOWN` with a did-you-mean, and a `default:` that is
  not one of a closed K's members is `E-STATE-DECL`. Both used to pass, and
  `lute play` ran in a state no `<match>` arm covered.
- A `<when is=…>` arm in a `<match>` with no `on=` is `E-MATCH-NO-SUBJECT`
  at the `is` value: there is no subject to compare it against. It used to
  check clean, compile to a match with an empty subject, and leave every
  trace and play undecided at that match with no reason given. The message
  names the declared path the literal belongs to (`add on="run.route"`), or,
  for a value that is a condition (`is="run.lamps >= 3"`), says to write
  `test=`. Lowering refuses such an arm too. A `<match>` with no `on` whose
  arms are all `test=` is unchanged.
- A bundle `<beat also>` on a `select: all` or `select: sequence` occasion is
  `E-BEAT-ATTR`, as a scene's `also: true` already was; so is a beat
  template header's `also: true` beside a header `on:` naming such an
  occasion, reported once at the header.
- `lute trace --occasion <occasion>@<target>` binds `occasion.target` for the presented beat, entry or scene that answers the occasion for a kind's members (`target="kind:K"`, `for="kind:K"`), as the engine's raise does; a target outside the kind is `E-TRACE-MOCK-TYPE`. Without a member, a `{{occasion.target}}` line (or `{{run.x[occasion.target]}}`) stops the walk incomplete instead of printing the marker raw under `trace complete`, and a `::set`/`::assert`/plugin write through it reads ``write `caught(occasion.target)` (::assert write)`` instead of an empty `match` with `arms 0/0`. `lute trace` prints a staging directive as authored (`::sfx{id="bell"}`), as `lute play` does, not a bare `<sfx>`; a `<match>` without `on=` prints `<match>`. `lute play` echoes a directive's number and `occasion.target` arguments unquoted (`::haul{fish=occasion.target n=2}`), and a quest reward granted on failure reads `(outcome="failed")` in play, run and trace text.
- A beat on an occasion the clock raises whose `when` holds only where the
  clock does not raise it — a `dayStart` beat for the day the run starts, a
  `dayEnd` beat for a slot before the day's last — passed `check-project`
  and never played. It is now `W-BEAT-UNRAISED` at its `on`, naming where
  its `when` holds and where the clock raises the occasion.
- A `lute play` step refused because the occasion's `raisedWhen` reads the
  clock (`run.hour == 'h06'`) no longer advises an `engine:` write, which
  jumps the clock without raising anything on the way: it advises an
  `advance:` to that moment.
- `lute calendar` on a beat whose `when` reads the raise's payload says how
  to vary it (`--axis occasion.payload.<field>=<value>,…`) instead of only
  leaving every cell undecided.
- `lute trace`'s coverage summary counts distinct hub options and `<match>` arms: a hub picked in a loop read `choices 5/3`, and a `<match>` run again kept only its last arm (`arms 1/3`).
- `E-TEST-KEY` in a `*.test.yaml` is reported at `file:line:col`, and its `--json` row names the test file instead of `""`.
- A plugin that fails to load prints `path:line:col: error [E-PLUGIN-KEY] …` like every other diagnostic, instead of `lute: E-PLUGIN-KEY: path:line:col: …`.
- A diagnostic with no position (`lute trace --choose` naming an unknown id, a bad `--state` mock flag) prints `file: error [CODE] …` instead of a made-up `file:0:0`.
- `lute trace --choose` naming a selection that cannot be followed says so, instead of blaming "invalid mock input" when no mock was given.
- `--on` on `lute trace`, `lute beats` and `lute calendar` is refused with a pointer to `--occasion`, instead of clap's `-- --on` tip.
- `lute new scene --occasion` no longer writes a `priority:` below an always-eligible fallback on that occasion (which started the stub as `W-BEAT-SHADOWED`): it goes above the fallback, below the other beats where there is room. Its comment lists every `once:` value.
- `check-project --wip` reports a beat dead for want of an unwritten producer once, not a second time judged under the project's `terminal:`.
- Missing files read `no such file or directory` without the platform's `(os error 2)`; an `E-DEFAULTS-KEY` path holding `a, b` says to write one path per list item.
- Did-you-mean is no longer offered for a one-character value (`default: c` against `[a, b]`).
- `lute compile --help` and the `--all` usage error name the positional project directory (`--all <DIR>`), which works without `--project`.
- A frontmatter or schema that is not valid YAML is worded for what it is (`the frontmatter` / `this file`), without the YAML library's sentence repeated or its `at line N column M`; two keys on one line, a key indented under a value, and a key written twice (located at the second) each get their own fix. `E-USES-PARSE` names the schema by file name, not its absolute path, and a mock of a document whose schema does not parse is not also reported.
- `lute context` writes directive effects and permissions readably
  (`writes run.salvage += 1`, `permissions: unrestricted`) instead of raw IR
  JSON.
- `lute play` prints a step's game-over note after the quest settle that
  caused it, and a `terminal:` that reads state a new run keeps says so
  instead of offering `newRun`.
- `lute scenario reach --endings` no longer names an ending beat as the writer
  of its own condition, lists a directive call as a fact producer only when its
  arguments match (JSON `assertedBy`), and follows a `terminal:` alternative
  over quest state.
- A literal a def compares against the wrong domain names the def
  (`in @onRen: 'rne' is not a member …`), and a path read through a def nested
  in another names the inner def that reads it.
- `lute-lsp` no longer checks a `.yaml` file as a `.lute` document (every line `E-UNCLASSIFIED`): a `*.schema.yaml` anywhere in the project is analysed as the declaration it is, like one under `schema/` or `catalog/`, and any other YAML (a play, a test, the manifest) shows only what other documents report in it.
- A misspelt `<beat>` key on a `<beat use=…>` template use names the key it misses: ``component `talker` has no parameter `priorty` — did you mean the `<beat>` key `priority`?``.
- A `bool` `@def` passed to an enum component param is one `E-COMPONENT-ARG` naming what the def produces (``argument `glass=@lastDay` … is `bool`, which does not fit `enum[steady, guttering]` ``), not two reports at the same column.
- `E-STATE-MAYBE-UNAVAILABLE` on an `owner: engine` path no longer says to add an `after:` naming a scene that sets it — no scene can; it advises an `isSet(…)` guard or a schema `default:`.

### Compatibility

- **Stricter manifest and state rows.** `lute.project.yaml`, `state:` rows and `enums:` long forms now refuse unknown keys; state paths under `scene.choices.`, `scene.visited.`, `occasion.` and a bare `quest.<x>`/`season.<x>` are no longer declarable.
- **Reserved names are refused where they are declared.** A quest, objective,
  entry, branch or hub id, a state path segment, an entity or enum member, a
  relation, season, def or choice id named after a CEL keyword (`return`, `in`,
  `if`, …), a CEL literal (`true`, `null`), a state root (`run`, `clock`, …),
  `unset` or `none` is now `E-RESERVED-NAME`, naming a replacement. Such an id
  compiled before but could never be read back (`quest.q.objectives.return`
  does not parse). Rename it; `lute --explain E-RESERVED-NAME` lists the table.
- **`::next` diagnostics say "mark".** Tools matching the old
  `E-NEXT-UNDEFINED` / `E-NEXT-BACKWARD` / `E-MARK-DUP` message text ("undefined
  label", "targets label", "label `x` is already declared") must match the new
  wording; the codes are unchanged. Authored files need no change.
- **`lute init` writes `townVisit`.** A new project's occasion is `townVisit`
  and its scenes `town.*` in `scenes/town/`; existing projects are unaffected.
- **Flag values are checked.** `optional="yes"`, `exit="yes"`,
  `<choice once="run">` (or `user`/`day`/`week`/`slot`/`season:…`) and
  `also="yes"` used to compile as `false`; they are now `E-FLAG-VALUE` errors —
  write the flag bare, or `="false"` to turn it off. `optional="true"` and
  `exit="true"`, previously read as false, now read as true.
- **Test `expect.offered` is `expect.options`.** `offered:` in a test's
  `expect:` is `E-TEST-KEY` naming `options`; rename the key (the value is
  unchanged). `lute test --json` reports those expectations with kind
  `options`, and a miss line reads `options <id>: expected …`.
- **Test and mock `accept:` is gone.** Write `accepts:`; `accept:` is
  `E-TEST-KEY` in a test and `E-TRACE-MOCK-PARSE` in a mock, both naming
  `accepts`. A play's `engine: { accept: [...] }` is unchanged.
- **`::camera{move-x move-y}` is `moveX` / `moveY`.** The kebab spellings
  are `E-UNKNOWN-ATTR` naming the new ones; the IR fields (`moveX` / `moveY`)
  are unchanged. The core capability version changes with the vocabulary.
- **Stricter clock declarations.** A clock day path with a default below 1
  (or not a whole number) is now `E-CLOCK-DECL`; give it `default: 1`.
- **Finite-clock slot narrowing.** On a clock that ends on its first day, a
  guard or `<when is>` naming a slot after the last one is now an error
  (`E-BEAT-UNREACHABLE` / `E-ARM-DEAD`), and such a slot's arm can be
  dropped from an exhaustive `<match>`.
- **Columns count characters.** A tool that reads `line:column` or `--json`
  `span.column` as a byte offset must count characters instead; columns on
  lines that are all ASCII are unchanged.
- **Quest attributes renamed.** `<quest after=>`, `<reward on=>` and
  `<objective when=>` are `E-UNKNOWN-ATTR` errors whose message names the new
  spelling; write `follows=`, `outcome=` and `visibleWhen=`. Engines reading the
  IR read `visibleWhen` on an objective entry, `outcome` on a reward entry, and
  `follows` (not `after`) on a quest's `prereqEdges` row.
- **Hub scripts that run out fail.** A `lute test` whose hub `choose:` list
  ends before the hub's `exit` option now fails incomplete; add the remaining
  picks.
- **`select: sequence` re-judges at each turn**, and **`for` beats spend
  `once` per member**: a sequence can present fewer (or more) beats than in
  0.27 when an earlier beat changes a later beat's `when`.
- `check-project --wip`: the downgraded dead-guard diagnostics now carry code `W-WIP` (was the `E-` code with `severity: warning`); a tool filtering `--wip` output by `E-ARM-DEAD`/`E-BEAT-UNREACHABLE`/`E-ENTRY-UNREACHABLE`/`E-OBJECTIVE-UNSATISFIABLE` reads the named code from the message, or matches `W-WIP`.
- **Unknown quest reads are errors over a whole project.** `check-project` on a directory with a `lute.project.yaml` now fails on a `quest.<id>.state` (or `.objectives.<oid>.done`) read no quest in the project defines (`E-QUEST-REF-UNKNOWN`, was `W-QUEST-REF-UNKNOWN`); a walk without a manifest keeps the warning.
- **Quest and objective ids must be one name.** An id that is not `[A-Za-z_][A-Za-z0-9_]*` — notably one with a `.` — is now `E-PATH-IDENT` at its declaration.
- **New text warnings.** `W-TEXT-SINGLE-BRACE`, `W-TEXT-COMMENT-LIKE` and
  `W-TEXT-BRACKET-LABEL` can appear on documents that checked clean in 0.27
  (`--deny` turns them into errors).
- **Kind labels win in text.** An engine rendering an `occasionTarget`
  placeholder renders `entities[].labels[member]` first, then the cast
  `name:`, then the id. `W-LABEL-CAST-SHADOWED` no longer exists.
- **One id per lore declaration.** An `<entry>` and a `<beat>` sharing an id in
  one document is now `E-BEAT-ID-DUP`; rename one. A duplicate `<beat id>` is
  reported as `E-BEAT-ID-DUP` instead of `E-BEAT-ATTR`.
- **New warning `W-SEASON-UNGATED`** on season-spent beats and season-tier
  quests not gated on the season's `live` condition.
- **`sequence:` is now `chapters:`.** Rewrite `sequence: { occasion: X, scenes: [...] }` as `chapters: [{ on: X, scenes: [...] }]`; the old key is `E-CHAPTERS` naming the new one and is not applied. `E-SEQUENCE`, `W-SEQUENCE-ORDER`, `W-SEQUENCE-STALL` are now `E-CHAPTERS`, `W-CHAPTER-ORDER`, `W-CHAPTER-STALL`. A chain on a targeted occasion listing a scene without `target:` is now an error.
- **`lute new --on` is now `--occasion`** (`--on` is refused naming it); new files are named after their id, and quest/lore document ids lose the `quest.`/`lore.` prefix.

- **Typed comparisons and literal checks in every condition slot.** Conditions that compare a bool, a number and a string, order enums or strings, or compare an enum or `occasion.target` with a non-member in `rearm=`, `spentBy`, an entry `when=`, a season `live:`, a rule `cel()` or a `raisedWhen:` gate used to check clean; they are now `E-CEL-TYPE` / `E-WHEN-LITERAL-DOMAIN` errors. A def reading an undeclared path is `E-UNDECLARED` at its use.

- `spentBy` changed meaning: it latches (see Changed). A beat relying on the condition turning false again to become eligible again must use `once: false` + `when: "!(…)"` instead (`W-SPENT-BY-REVERSIBLE` points at one whose fact is retracted, or whose quest rearms); a weekly reset writes `once: week` beside `spentBy`, and a condition over one season's state `once: season:<name>`. `once: false` or a `share` key beside `spentBy` is `E-BEAT-ATTR`; `once` with any other value beside `spentBy` is no longer an error. The IR `once` of a `spentBy` scene or bundle beat is its period (`run` unless written), not `none`: an engine must not spend a `spentBy` beat on presentation.
- `lute trace --json`: a guard's decision is `"construct": "guard"` with outcome `taken`/`skipped` (was a `match` with `arm 1`/`otherwise`); component boundary steps carry `component` and the new `body`/`bodyEnd` boundaries; set/assert/retract steps from a plugin call carry `effectOf`, the call step `call`. `lute test --coverage` paths are relative to the working directory, and its JSON `notPresentedByPlay` lists only tested beats.
- A malformed `plural(…)` hint is `E-PLURAL-FORM` (was `E-CEL-PROFILE`); a subquest's `rearm=`
  is `E-SUBQUEST-REARM`; a constant `rearm=` warns (`W-QUEST-REARM-CONSTANT`). Kind label
  entries are a string or `{ text, start, indefinite }` (any other key is
  `E-ENTITY-KIND-SHAPE`).
- Play and test `expect.exit` is now `expect.end: complete | terminal | incomplete | error` (`terminal`: every step played and the project's `terminal:` holds; `complete` no longer passes a play that should have ended the game). The old key is an error naming the new one.
- `lute scenario reach --endings --format json`: each need's `writers` are records `{ kind, id, file, how, via, text }` (`how`: `set`, `choice into`, `directive` with `directive`, or `reward` with `reward`; `via`: the `::use`d component), not pre-rendered strings; endings carry `runsEnd`, `writesTerminal` and `gate`, and each root `terminal`. `lute lore --json`: a scene beat row's `kind` is `scene` (was `beat`), fact rows carry `components`, rows carry `for`.
- **Reserved names are one code.** `E-CHOICE-ID-RESERVED` (a choice id
  `unset`) and `E-RELATION-RESERVED-NAME` are now `E-RESERVED-NAME`, which
  also refuses the names listed on the *Reserved names* page — a project with
  an entity member `clock`, an enum member `unset`, a season `week`, a cast
  `narrator` or an occasion `end` must rename it (the message suggests one).
- `plugin.yaml` rejects unknown keys (`dependencies:` → did you mean
  `depends`) and a `kind:` other than `capability`, as `E-PLUGIN-KEY`; an
  unknown key in an export file is `E-PLUGIN-KEY` too (was
  `E-PLUGIN-PARSE`).
- Plugin export keys are camelCase: `rewardkinds`/`assetkinds`/`stampattrs`
  are now `rewardKinds`/`assetKinds`/`stampAttrs`; the old spelling is
  `E-PLUGIN-UNKNOWN-EXPORT` naming the new one.
- A plugin directive named like a core statement (`set`, `assert`,
  `retract`, `accept`, `use`, `body`), a core block tag (`match`, `branch`,
  `hub`, `choice`, `when`, `otherwise`, `entry`, `beat`, …) or a `lute.core`
  directive (`end`, `mark`, `bg`, …), and a plugin occasion named like a
  lifecycle event (`questComplete`) are `E-PLUGIN-RESERVED-NAME` (a core
  directive name was `E-PLUGIN-DUP-ACROSS`).
- A component param named `component` or `when`, and a beat template param named like a `<beat>` header key (`id use on target for title priority once share after when spentBy also`), is now `E-TEMPLATE` at its declaration: no use could ever pass it (the attribute set the use's own key). Rename it (e.g. `title` → `heading`).
- `W-ENTRY-WRITE-REREAD` covers every entry that can be read again in a run and writes: lookup entries, `once` shorter than the run (`day`, `slot`, `week`, `season:<n>`), `spentBy` entries and `for=` entries without `once: run|user`. A write guarded by `!entry.<id>.read` is intentional and not warned. New warning `W-TERMINAL-PERSISTENT` for a `terminal:` reading state a new run keeps. `E-UNSET-UNCOVERED` no longer fires for `scene.*` subjects; a maybe-unset `scene.choices.*` read is one `E-MAYBE-UNSET`.
- **`is=` needs a `<match on>` subject; `also` needs a `select: first`
  occasion everywhere.** A `<when is=…>` arm in a `<match>` with no `on=` is
  now `E-MATCH-NO-SUBJECT` (add `on=`, or write `test=`), and a bundle
  `<beat also>` or template header `also: true` on a `select: all` /
  `sequence` occasion is now `E-BEAT-ATTR` (remove `also`). Both passed the
  check before and never ran as written.
- **Restamp `luteVersion:`.** A document or `defaults:` stamped with an
  older version draws `W-LUTE-VERSION-STALE`, which names the stamp to
  write (`luteVersion: "0.28.0"`); bump the stamp, or `--deny-warnings`
  fails the project.
- **Schema file renamed; three IR fields renamed, the rest additive.** The
  version strings move to `0.28.0` and `schemas/lute-ir-0.27.schema.json` is
  renamed to
  [`schemas/lute-ir-0.28.schema.json`](schemas/lute-ir-0.28.schema.json)
  (`$id` updated). An engine reading quests reads `ObjectiveEntry.visibleWhen`
  (was `when`), `RewardEntry.outcome` (was `on`) and a quest's `prereqEdges`
  row `follows` (was `after`; scene and bundle-beat rows keep `after`), and
  must not spend a `spentBy` beat on presentation: its `once` is now its
  period (`run` unless written), not `none`. Every new field is optional and
  appears only when the source uses the feature: `outsideRun` on the artifact
  and `project.index.json`, `HubCmd.return`, `clock.raiseAtStart`,
  `labelForms` on `entities[]` and `state[]` entries, `authored` on a seam
  condition, the placeholder formats `"cardinalWord"`, `"capitalize"`,
  `"start"` and `"indefinite"` (also on `reserved` / `occasionTarget`
  placeholders), and the relation tier `"season:<name>"`; an
  `occasion.target` write keeps `F[occasion.target]` / `occasion.target` for
  the engine to bind. `lute.core` moves (`::camera` `moveX` / `moveY`), so
  `capabilityVersion` moves with it. Engines gate on MAJOR, so nothing
  widens; the tree-sitter grammar admits a hub's `<return>` block and treats
  `visibleWhen` values as CEL.

## [0.27.0] - 2026-09-27

**One runtime, seasons, templates.**

A large minor release from a fifth dogfood round. `lute play`, `lute run`,
`lute trace`, `lute test` and the playground execute the compiled IR on one
walker, and a differential test holds every example, conformance fixture and
test fixture to one transcript under both drivers. Occasions bind members
(`occasion.target` as a ground term, `for="kind:<kind>"`, occasion
`payload:`); the engine seam gains `raisedWhen:` gates, a `terminal:` state, a
finite clock and directive `asserts` / `retracts`; cadence gains `once: week`,
quest `rearm=`, `seasons:` and `spentBy:`; components become beat templates,
the manifest's `sequence:` chains scenes into chapters, and kinds carry
display `labels:` beside the `plural` hint. The checker says no where it used
to stay silent (`E-SET-SHAPE`, member-checked `{ domain: K }` paths,
`E-ATTR-QUOTE`, `W-QUEST-TIER-IMPLICIT`), and diagnostics speak plain
language, their spec sections moved to `--json` `spec` and
`lute --explain <CODE>`. The language and the IR both earn the move (the IR
additively); see
[`docs/proposals/scenario-dsl/0.27.0.md`](docs/proposals/scenario-dsl/0.27.0.md)
and [`docs/versioning.md`](docs/versioning.md).

### Added

- **Beat templates** (dsl 0.27.0 §6, round-5 T2-9): a component may declare a
  `beat:` header (`on`, `target`, `for`, `title`, `priority`, `once`,
  `share`, `after`, `when`, `spentBy`, each may name `@param`s). In a bundle
  document, `<beat use="bondStory" id="ariaR2" hero="aria" rank="r2">…</beat>`
  takes every header key it does not write itself from the template (an
  `after:` that comes out empty is left out; one that is a bare id means
  `visited("<id>")`), runs the template's body first and then its own; a
  top-level `::body` in the template places the use's body instead. The other
  attributes are the template's params, checked like `::use` arguments.
  `<beat use="trainer" id="r3Joey" who="joey"/>` is a one-line beat. Uses
  desugar into ordinary beats before any check, so `lute beats`, the scenario
  graph, play, trace and the compiled artifact see plain beats. New
  `E-TEMPLATE` for misuse (unknown template, a component without `beat:`, a
  malformed header, `::body` outside a template).
- **`sequence:` in `lute.project.yaml`** (dsl 0.27.0 §8, round-5 T2-13, D-1):
  `sequence: { occasion: chapter, scenes: [prologue, counter, kitchen] }`
  gives each listed scene `on: chapter`, `after: visited("<previous>")` and a
  descending `priority:` (30, 20, 10, …) unless the scene writes the key
  itself. New `E-SEQUENCE` for a malformed block, an id listed twice, an id no
  scene declares (with a did-you-mean) or a listed scene whose own `on:`
  answers another occasion. "Connect scenes into a story" teaches it first,
  and `docs/examples/connect-scenes` uses it.
- `include:` takes `repeat: n` and its own `choose:` / `bridges:` (round-5
  T3-22): `- include: steps/term.steps.yaml` with `repeat: 5` splices the
  file five times; `choose:` / `bridges:` on the include script every step it
  splices in over the script's own, key by key and tag by tag, consumed from
  their start in each repetition and dropped when the segment ends — so a
  multi-term play states each term's choices beside that term instead of as
  one positional list for the whole script.
- `lute play --quiet` leaves out the candidates that were not eligible at each
  raise (round-5 T3-16). Without it, five or more `when: false` candidates at
  one raise print as one count line (`✗ 8 beats — when: false: a, b, c, …`);
  `--json` still lists every candidate.
- Members bound by occasions (dsl 0.27.0 §3, round-5 T2-1/T2-2/T2-10):
  - In a kind beat, `occasion.target` may be a fact-query argument
    (`holds(owned(occasion.target))`) and a `per:` family index
    (`user.bond[occasion.target]`). The checker checks the condition once per
    member of the kind (a member outside the relation's kind is
    `E-FACT-DOMAIN`, a family of another kind `E-UNDECLARED`, naming the
    member); play, trace, test and run substitute the bound member. A trace
    without the member names `--state occasion.target=<…>` with the traced
    beat's own members, once.
  - `for="kind:<kind>"` on an entry or bundle beat (`for: "kind:<kind>"` in a
    scene's frontmatter) of an untargeted `select: sequence` occasion
    presents the beat once per member whose `when` holds, in member order,
    binding `occasion.target`; `once` spends the beat as a whole. A `for`
    beside `target`, on a targeted or non-sequence occasion, or naming an
    unknown or `open:` kind is `E-BEAT-ATTR`. `lute play` lists one candidate
    per member (`✓ g.bday for aria`, `--json` `for`); IR `forKind: { kind,
    members }` on the scene `meta.beat`, `entry` / `beat` records and index
    beat rows.
  - A rule variable bound by a positive atom may be compared (`==` / `!=`)
    with a `{ domain: <kind> }` state path in a rule `cel()` guard
    (`close(R) :- adjacent(R, S), cel("run.stalker == S")`); the rule compiles
    grounded, one instance per member.
  - An occasion MAY declare `payload: { copies: number }`: beats answering it
    read `occasion.payload.copies` (typed; `E-UNDECLARED` in a beat of another
    occasion). A `lute play` `occasion:` step gives `payload: { copies: 2 }`
    (an undeclared field is a usage error naming the declared ones); the
    values last that one raise. Trace and test mock them as state.
- Entity-kind display labels (dsl 0.27.0 §7, T2-8): an `entities:` kind MAY
  declare `labels: { <member>: "<display text>" }` (also beside an `add:`
  list, for the members it adds). `{{occasion.target}}` in a kind beat renders
  the member's label (a cast `name:` still wins for cast ids), and so does a
  `{ domain: <kind> }` state path; a sub-kind takes its parent's labels for
  its members and the parent its sub-kinds'. `lute play`, `lute trace` and
  `lute run` render them; the IR `entities[]` entry carries `labels` and the
  path's `state[]` entry the same map. A label for an id the kind does not
  have (with a did-you-mean), a non-text label, and `labels:` on an `open:`
  kind are `E-ENTITY-KIND-SHAPE`.
- Plural hint (dsl 0.27.0 §7, T2-12): `{{run.lamps:plural(lamp|lamps)}}`
  renders the first form when the number is 1 and the second otherwise; a `#`
  in a form is the number (`{{n:plural(# lamp|# lamps)}}` → `3 lamps`). The IR
  placeholder carries `"format": "plural"` and `"forms"` so an engine can
  localize the count. A plural without exactly two forms, and `(…)` on
  `:ordinal`, are `E-CEL-PROFILE`; a plural of a non-number is `E-REF-TYPE`.
- Diagnostics JSON (`--json`, the wasm build) carries a plain `message`
  without the spec citation; the cited sections move to a new optional `spec`
  array (T3-17).
- Engine seam (dsl 0.27.0 §4, T2-3/T2-4): an occasion MAY declare
  `raisedWhen: "<condition>"` (it may read `occasion.target`) and a schema
  MAY declare `terminal: "<condition>"`. The checker judges every beat of the
  occasion under its gate and `!terminal` (`E-BEAT-UNREACHABLE` /
  `E-ENTRY-UNREACHABLE` name the gate or the terminal state, with the fact
  envelope's reasons), checks both texts like any condition, and counts their
  reads for `W-RELATION-UNREAD`. The IR artifact and `lute compile --all`
  index carry `gates` and `terminal`. `lute play` refuses a step raising a
  gated occasion while its gate is false, and any `occasion:` / `advance:`
  step once the terminal condition holds (`E-OCCASION-GATE`, new). A play
  whose last step leaves the game over ends ``── end: terminal — `terminal:
  <condition>` holds`` (`--json`: root `"end": "terminal"`), the step that
  ended it carries a note, and `newRun: true` plays on. A clock raise
  (`raise.slot`, `dayStart`, `dayEnd`) whose gate is false is not made and
  the step notes it. `lute beats` marks a ladder whose gate can never hold,
  judged under the fact envelope (`· gate never holds: …` / `· gate never
  holds for room.office: …`; `--json` `raisedWhen`, `gateNeverHolds`,
  `gateNeverHoldsFor`). `terminal:` is a schema key only (not the manifest's).
- Directive facts (dsl 0.27.0 §4, T2-11): a plugin directive MAY declare
  `effects: { asserts: ["holding(@item)"], retracts: ["holding(_)"] }` (every
  list optional, `writes` included; each `@attr` is the call's attribute or
  its declared default). A malformed pattern or an `@attr` the directive does
  not declare is `E-PLUGIN-PARSE` at load; the relation, arity and argument
  kinds are checked at each call like an `::assert` of the fact (a
  `reserved: true` relation MAY be written this way — the engine's own
  write). The `plugin` IR record carries the resolved `retracts` / `asserts`
  (`{ relation, args }`, omitted when empty); `lute play`, `lute test`,
  `lute trace` and `lute run` apply them after the call's writes (retracts
  first) through the one assert/retract path, recorded with `effectOf` (play:
  `assert holding(brassKey)  (effect of ::give)`). The fact envelope,
  `W-RELATION-UNREAD`, `E-FACT-EXCLUSIVE`, presence, `lute scenario --facts`
  / `knowledge` and `lute lore` count them as `::assert`s of the call.
- A lore `<entry>` body MAY call a plugin directive whose only behaviour is
  its declared effects (no bridge, result slot, layer or lowering); like the
  entry's own `::set`, its effects apply on the first read only (a re-read
  records them `skipped`).
- A `lute play` `occasion:` step MAY carry `engine:` writes: they land (and
  the quests settle) before the raise, as their own record of the step.
- `once: week` (dsl 0.27.0 §5, T2-6): scene beats, bundle beats
  (`once="week"`) and entries spend until the next clock week starts, when
  `clock.weekday` returns to `week.first`. It needs the clock's `week:`
  (else `E-BEAT-ATTR`, like `once: day` without a clock). IR `once: "week"`.
- Quest `rearm="<condition>"` (dsl 0.27.0 §5, T2-7a): each time the
  condition goes false→true (watched at every quest settle, the first one
  being the baseline) the quest returns to `unset` — objectives undone,
  `failedBy` cleared, deadlines forgotten — and can be started or accepted
  again. `lute play` prints `quest <id> -> unset (rearmed; was <status>)`; IR
  `QuestCmd.rearm`.
- Seasons (dsl 0.27.0 §5, T2-7b): a schema MAY declare
  `seasons: { harvest: { live: "@harvestLive" } }`. Each season is a state
  tier — `season.<name>.*` paths, the read-only `prev.season.<name>.*` (the
  last window's final values), `once: season:<name>` beats and
  `<quest tier="season:<name>">` quests — reset each time `live` goes
  false→true; seasons overlap freely. `lute play` prints
  `season <name> opens — season.<name>.* reset to defaults; last window: …`
  / `closes`. The artifact and `project.index.json` carry
  `seasons: [{ name, live }]`. A malformed or conflicting declaration or an
  undeclared season is `E-SEASON-DECL` (new); a write to `prev.season.*` is
  `E-QUEST-RESERVED-WRITE`, as for `prev.run.*`.
- `spentBy: "<condition>"` (dsl 0.27.0 §5, T3-25) on scene beats, bundle
  beats and entries, instead of `once`: the beat repeats until the condition
  holds (a retryable puzzle: `spentBy: "holds(solved(valves))"`); both on one
  beat is `E-BEAT-ATTR`. `lute play`, `lute calendar` and `lute test`'s
  `eligible:` give the reason ``spentBy: `<condition>` holds``. IR `spentBy` on
  `meta.beat`, `beat` and `entry` records and index beat rows.
- Editors: `lute-lsp` completes and documents `rearm=`, `spentBy=` and the
  `once` values `week` / `season:<name>` and quest `tier="season:<name>"`;
  the tree-sitter grammar highlights `rearm` / `spentBy` values as CEL.
- Docs: a writer page, **Connect scenes into a story**
  (`getting-started/connect-scenes`, with a Korean mirror), that chains scenes
  into a story `lute play` plays without an engine: one made-up occasion
  answered by every scene with `on:`, `after:` for the order, descending
  `priority:`, `when:` for branching endings, a play script and a scene test.
  Its files are the new `docs/examples/connect-scenes/` project.
- **`W-QUEST-TIER-IMPLICIT`**: a `<quest>` with no `tier=` (and no
  `defaults.questTier`) whose conditions read only run state — `run.*` paths,
  run-tier relations or quests — is user-tier by default and outlives the run;
  the warning asks for an explicit `tier="run"` / `tier="user"` or a
  `defaults.questTier`. The `lute init` templates and the `docs/examples`
  quests now write `tier="run"`.
- **`lute calendar` axes over a whole `per:` family**: `--axis run.aff.*=6,7`
  gives every member of the family the cell's value, and
  `--axis 'run.aff[run.route]=6,7'` only the member the `--axis run.route`
  value names in each cell (the other members keep their seed or default) —
  route × own affection is one axis pair instead of one axis per member and a
  long `--where`. The indexing axis must be an axis of the same calendar whose
  values are members of the family's kind; a bare `--axis run.aff=…` is a usage
  error naming the three forms (it said `did you mean run.day?`), and a second
  axis over a member the family axis sets is refused. Labels, `--json` and
  `--csv` name the axis as written.
- **A finite clock** (dsl 0.27.0 §4, T2-5): a schema's `clock:` MAY declare
  its last position, `last: { day: 1, slot: h05 }` (`slot` omitted: that
  day's last slot), or `days: N` for the last slot of day N. An advance whose
  destination lies past it walks to the last position (raising `dayEnd` /
  `dayStart` at every midnight on the way), raises the last day's `dayEnd`
  once — never `raise.slot` — and ends the clock: a one-night clock no longer
  rolls into `night 2, h23` and re-raises its hour occasion. `lute play`
  prints `· the clock ends (its last position)` on that advance (`--json`:
  `advance.ended: true`); any later `advance:` — or one starting past the end
  after an `engine:` write moved the day — is the new usage error
  `E-CLOCK-END` (exit 1, in `lute test` play files too), until a `newRun`
  starts the clock over. The checker ranges `clock.index` and the day path
  over the whole numbers up to the end, so a `when` needing a later position
  (`run.night == 2`, `clock.index >= 9`) is `E-BEAT-UNREACHABLE` /
  `E-ENTRY-UNREACHABLE` / `E-ARM-DEAD`, naming the clock's end, and a
  `<match>` over either path is exhaustive once every value in range is
  covered. `lute calendar --axis clock` stops at the last position. Both
  keys, `days: 0`, a `last.slot` outside `slots`, or defaults already past
  the end is `E-CLOCK-DECL`. IR: `clock.last` / `clock.days`, verbatim and
  omitted when not declared. The same whole-number reasoning now decides
  comparisons over `clock.weekday` (`clock.weekday > 5 && clock.weekday < 6`
  is false, `clock.weekday <= 6` on a 7-day week true).
- `lute scenario <dir> reach --endings[=<occasion>]` (T3-20, round-5 OT-F11): one row per ending — every beat answering the occasion, or bare, every beat whose content can run `::end` — with its `after:` verdict, the `when` verdict `check-project` reaches (`E-BEAT-UNREACHABLE`/`E-ENTRY-UNREACHABLE`: never holds; `W-BEAT-SHADOWED`: never wins), and for a `when` nothing refutes the state paths and facts it reads with who produces them (`nothing writes it` only when no authored, component, plugin-effect or reward write can reach the path). Ends with `N ending(s): A reachable, B unreachable, C unknown` and the hint that a play presenting the ending is the proof. `--format json` supported; exit 0.

### Changed

- Diagnostics speak plain language (dsl 0.27.0 §9, T3-17): every message —
  `check`, `check-project`, the editor, and the notes, halts and refusals of
  `play`, `test` and `trace` — drops its `(dsl … §…)` citations; the
  sections move to the `--json` `spec` field and to `lute --explain <CODE>`,
  which prints a code's grade, one-sentence meaning, spec sections and its
  section of the new [diagnostics reference](https://lute-lang.vercel.app/reference/diagnostics/)
  (the editor's code link points there too). One registry
  (`crates/lute-cli/src/codes.rs`) now backs `--explain`, the `--deny`
  universe and the reference page. Structural parse errors name what is
  open: a close for an enclosing block reports the unclosed tag, its line and
  the close that ended it; a `</tag>` closing nothing names the block that is
  open and is skipped; a `<choice>`, `<when>`, `<otherwise>`, `<track>` or
  `<reward>` outside its parent is one `E-LOGIC-CONTENT`, parsed and dropped
  without an `E-UNCLOSED-TAG` cascade.
- Play usage errors are located (round-5 T3-13): a step's error names the
  file, line and column of the key it is about — in the `include:`d steps file
  when the step came from one, `(included from plays/p.play.yaml:12:5)` — with
  did-you-mean for occasion names, step keys, top-level keys and include keys.
  A step `expect:` naming a beat no project beat has (`winner`, `offered`,
  `notOffered`, `presented`) or a clock position the clock does not have
  (`clock: { weekday: Sundy }`, an undeclared `slot`) is a usage error before
  the play runs, not a miss; an occasion step with no `target:` whose every
  beat has one says so. A test's (or `--mock`'s) `choose:` naming an unknown
  branch or choice id is located at its line in the yaml, with did-you-mean.
- Selection expectations on an `advance:` step have one contract (round-5
  T3-8): `presented` is every beat the step presented — each midnight's
  `dayEnd` / `dayStart`, then the slot occasion — and its miss tags each with
  the raise that presented it (`r.closeLine (dayEnd at day 3 night)`);
  `winner` / `offered` / `notOffered` judge the step's last raise, where the
  clock stops (the slot occasion, else the last midnight's). The miss places
  the step as `advance day → dailyReset`. A clock that raises only
  `dayStart` / `dayEnd` now admits them (and `choose:`); only `pick` still
  needs a `raise.slot` occasion.
- A `transcriptContains` miss's nearest line (round-5 T3-16, SG-F15) is chosen
  for the needle's first line no presented line contains: a line of that
  line's speaker first, then one said in the step where the needle's other
  lines were said, then the one the needle needs the fewest edits to occur in.
  A short line is no longer "near" a long needle because it has little text
  to differ in (`@pim: Wish well!` for a missions line).
- `lute init` (the `minimal` template): the starter scene names itself with
  `id: opening` instead of `character:`/`season:`/`episode:`, the template adds
  `tests/opening.test.yaml` (one passing scenario test, `file: ../scenes/…`),
  and the README and "Next steps" list `lute test` and `lute doctor`.
- A refused scripted pick names the false premise (round-5 T3-12): `lute trace
  --choose` / a test's `choose:` on an option whose guard decided false quotes
  the guard and every read it is false over, with the mock that changes it —
  ``its guard `holds(found(receipt))` decided false: `found(receipt)` does not
  hold (mock `--fact "found(receipt)"`)`` (`facts: [...]` / `state: { … }` in a
  `*.test.yaml`; a `visited(…)` or quest-state read notes that a single-file
  trace does not know earlier scenes); a spent hub `once` option says ``it is
  `once` and already taken in this visit of hub `h` ``. `lute play`'s
  `E-TRACE-CHOICE` appends the same premise in its script's key spelling.
- `lute test`: an `expect.eligible: { id: true }` miss names the false premise
  as the implicit miss does — ``eligible ember: expected true, got false — its
  `after="visited('ren.confession')"` is false — mock `visited:
  [ren.confession]` `` (round-5 T3-12); a bundle beat's `after=` names its
  formula and the `visited:` / `quests:` entries it needs, an entry's or beat's
  false `when` its condition. `lute trace` and `lute test` judge a presented
  scene, entry or bundle beat by the session's one eligibility rule (`once`,
  `after`, `spentBy`, `when` — what `lute play` and `lute calendar` select by)
  instead of their own copy of it, so a bundle beat's `spentBy` and `share`
  now count in a test too.
- Docs: the writer path teaches `id:` identity from the first frontmatter, adds
  a "Quotes and YAML for writers" primer, a "Pin your story with tests" part and
  `lute doctor` for an editor that disagrees with the terminal
  (`first-scene`, `learning-paths`, `installation`, Korean mirrors). The
  first-scene transcripts are regenerated from the tool, and
  `docs/examples/episodes` uses `id:`. Test-file examples in the CLI reference
  name their scene relative to `tests/` (`../scenes/…`).
- Docs: the play, CLI and cheatsheet pages (and their Korean mirrors) state
  the 0.27 transcript-needle rule — a needle's attribute block is judged, so a
  `transcriptLacks` needle with a block holds when the words are said another
  way — and the `include:` step's `repeat` / `choose` / `bridges` (which leave
  the script's own `choose:` list where it was). The cheatsheet's manifest
  block shows `sequence:` and tells it apart from `select: sequence`; the CLI
  reference and `first-scene` document `lute --explain <CODE>`; the plugin
  page's `give` example is an inventory bag (a `holding(_)` retract makes one
  slot); the `reach --endings` example and the `first-scene` envelope and
  `lute context` transcripts are regenerated from the tool.
- Docs: **Build an investigation** needs only the `lute` command. It builds the
  project in the reader's own folder with `lute …` commands (no `cargo run`, no
  repository), shows every file whole (pinned to `docs/examples/investigation`),
  adds a scenario-test step, regenerates every transcript from the tool (trace
  prints shot headings; seed facts and rules load by default), and says that
  `lute init --template investigation` is a different, larger whodunit.
  `docs/examples/investigation` uses `id:` identity with the same keys, and its
  optional objective reads the derived `points` relation.
- `lute check-project` reports a fault in an imported schema once, at the
  schema's own line, ending `(imported by N documents)`; the importers are
  judged on their own content. It used to head the fault at the first
  importer's `1:1` with `(+N more callers)` and the schema line nested under
  it. A component body fault is reported in the component file at its line,
  and a document that never `::use`s a component no longer carries that
  component's body diagnostics.
- At equal priority a sub-kind's beat outranks its parent kind's (member >
  sub-kind > kind, by strict member-set inclusion) in `lute check-project`,
  `lute beats` and `lute play`; overlapping unrelated kinds keep file order.
- `W-BEAT-PRIORITY-TIE` is one warning per group of beats that tie one
  another, anchored at the group's first beat, naming each beat and each
  distinct reason once. It was one warning per beat, repeating every earlier
  pair.
- `lute new scene --on <occasion>` writes a `priority:` 10 below the lowest
  beat already on that occasion, so successive stubs rank in creation order
  instead of tying.
- A beat's or entry's `when:` narrows a `<match>` subject through
  `a == 'x' || a == 'y'`, `a in ['x', 'y']` and `!(a in […])` as it does
  through one `==`: arms the guard rules out are `E-ARM-DEAD`, and
  exhaustiveness asks only for the values left. A `||` with a side that leaves
  the subject free narrows nothing.
- `E-STATE-MAYBE-UNAVAILABLE` names the fix: an `after:` naming a scene that
  sets the path, or an `isSet(…)` guard. A single-file `E-MAYBE-UNSET` on a
  `run.*` / `user.*` path the file never sets, in a scene with `after:` or
  `on:`, says a single file cannot see the scenes before it and that
  `lute check-project` decides reads ordered by `after:`.
- Did-you-mean suggestions need at most one edit per three letters (minimum
  one), and swapping two adjacent letters counts as one edit: `lable` still
  suggests `label`; `the` and `oven` no longer suggest `when`.
- `lute context` shows declared param defaults in component signatures
  (`keeper: string = "The inspector"`, `glass: enum[…] = steady`, a `@def`
  default by name) and as a `default` key in the JSON surface; each builtin
  directive line shows its optional `when="<condition>"` guard.
- `lute new quest` and `lute new lore` write the same `uses:` as
  `lute new scene` and a Title Case `title:` (`the-cellar` → `The Cellar`);
  the quest scaffold's objective reads a scene of the project instead of a new
  document-local counter.
- `lute trace` shows a line's delivery as written (`@wren{mono}`,
  `@wren{vo emotion="sad"}`), as `lute play` does; `--json` line steps carry
  an optional `delivery`.
- A stale language server points at `lute doctor`: `lute-lsp` shows a
  one-time message when a document or project `luteVersion` is newer than the
  server, the newer-stamp `W-LUTE-VERSION-STALE` names `lute doctor`, and the
  VS Code extension also compares the server with the project's
  `defaults: luteVersion` and, when nothing is stamped, with the `lute` on
  `PATH`.
- A scene test's `expect.quests` may name a quest another document of the
  project declares: its state is its seed, `active` when the scene accepts it,
  else `unset`. Only an id no document declares fails, with did-you-mean.
- `lute test --coverage` works at beat granularity (T3-20, round-5 OT-F11): each bundle beat (`<document id>.<beat id>`) and lore entry is its own unit (scene and quest documents stay one unit each), so `lore/endings/ren.lute: ember` is listed when no test or play presents that ending although its sibling is. The options a play picks now count in the branch/hub rows under the same `{file}:{id}` row as traced paths (the header now reads `plays count toward what they presented and the choices they picked, not match arms`), and a new last section lists the beats no play presented. `--json`: `coverage.untested` is now a list of `{file, id, kind}` units (was document paths), plus `coverage.notPresentedByPlay`.

### Fixed

- `lute play` and `lute test` refuse a `facts:` / `notFacts:` expectation atom that names an undeclared relation, the wrong arity, or a non-member argument (with a did-you-mean), and a `transcriptContains` / `transcriptLacks` needle whose `@speaker` is not in the project's cast. Such an expectation used to hold vacuously. It is a usage error in `lute play` (exit 2) and an invalid test in `lute test` (`E-TRACE-MOCK-FACT` / `E-TEST-NEEDLE`), located at the entry. A needle refusal now carries its `file:line:col` too.
- A seed or `engine:` fact that names a non-member now reads ``… names `sorn`, which is not a member of `suitor` — did you mean `soren`? (…)``, and a wrong arity names the relation's signature.
- `W-QUEST-TIER-IMPLICIT` no longer misses a run-reading quest whose quest tree has a sibling that reads nothing (`done="true"`). A quest that reads nothing takes no side, and it is named among the quests that must change together.
- `lute doctor` counts the scenes whose `on:` comes from the manifest's `sequence:` (and template-expanded beats) in its occasion tally.
- `lute trace` heads a beat or entry the engine would not raise with that reason (its occasion's `raisedWhen` is false, or `terminal:` holds), and a false `when` names itself with what it read. It used to say `` `when` is false `` in every case.
- `lute play` and `lute test` report a schema or plugin fault every importing document shares once, at its own line (as `check-project` does), and then refuse. They used to repeat it once per document.
- A beat template whose `when:` / `spentBy:` names a `@name` that is no param and no def is reported once, at the header key in the component, with a did-you-mean over the params and defs (`` `@onyl` is not a declared param or def — did you mean `@only`? ``). It used to be reported at every `<beat use=…>`, with no suggestion; the uses now derive no condition for that key and say nothing about it.
- `lute init` no longer writes spec citations (`(dsl §9)`) into the scaffolded schema comments; `lute new schema` likewise.
- `lute explain E-FOO` (a diagnostic code, any case) answers ``did you mean `lute --explain E-FOO`?`` (exit 2) instead of clap suggesting `play`.
- `lute test` prints a state miss's numbers and booleans bare and strings quoted (`state run.clues: expected 3, got 2`); it used to quote every value (`expected "3"`).
- New warning `W-SEQUENCE-ORDER`: on a `select: sequence` occasion, a scene listed in the project's `sequence:` that writes its own `priority:` out of the listed order is reported at that key. It used to reorder the chain silently.
- `::set{ add 1 to run.cluesFound }` is one `E-SET-SHAPE` naming the shape (`` `::set` takes `<path> <op> <value>`, e.g. `run.cluesFound += 1` ``) with no invented operator guess (it suggested `add = 1 to run.cluesFound`), and no cascading `E-UNDECLARED` / `E-CEL-PARSE`.

- The seam in every runtime (0.27 prerelease). `lute test` and `lute trace` judge a beat whose
  occasion has a `raisedWhen` gate, or a project with `terminal:`, by the same rule `lute play`
  refuses a raise by: `eligible: true` on a beat the engine would not raise under the mocks used
  to pass and now misses, naming the gate and the reads it is false over (or the holding
  `terminal:`); `lute test --json` adds `notRaised` to that expectation, and `lute calendar` shows
  the same reason. `E-OCCASION-GATE` in `lute play` is located at the step as written and names the
  false reads (a derived fact with the rule premise it misses); a skipped clock raise names them
  too. A dead gate's `E-BEAT-UNREACHABLE` names the rule premise nothing produces
  (`holding(office)`), not only the fact it would derive. An `include:` item whose own `choose:`
  key none of its steps presented, or whose `bridges:` tag no call took, over all its
  repetitions, gets a note naming them. Play and test values meet the member check: a play or
  save seed, an `engine:` / `newRun` write, a play `expect.state`, a test `state:` seed and a test
  `expect.state` value outside a `{ domain: K }` / enum path's members is refused with a
  did-you-mean (`run.route: rne` → `ren`), and so is an occasion payload field typed
  `{ domain: K }` naming an undeclared K (`E-DOMAIN-UNKNOWN`, at the plugin line) or a play payload
  value outside it. A refused mock or test seed (`state:`, `quests:`, `entriesRead:`) is located at
  its key in the mock or test file, not at the traced document's `0:0`.

- Seam and cadence checks close their silent holes (0.27 prerelease). A literal in `terminal:` or
  `raisedWhen:` is member-checked like one in a `when` (`E-WHEN-LITERAL-DOMAIN` with a
  did-you-mean, not 69 unreachable verdicts); a gate or effect fault is reported once, at the
  plugin's declaration, and `E-PLUGIN-PARSE` names its file; a `spentBy` that already holds at the
  start makes the beat unreachable like `when: false`; a `by=` deadline a finite clock never reaches
  is `W-DEADLINE-NEVER` (documented in clock.md, with how to write a deadline at the last
  position); a season fault in a schema is one error at the schema's line instead of a copy at
  `1:1` of every document; a bool slot (`live:`, `rearm=`, `start=`, `when`, …) holding a bare
  number/string/enum path is `E-REF-TYPE` with a compare example (`user.day > 0`,
  `run.route == 'aria'`); a def reading `occasion.target` used outside a kind beat is
  `E-UNDECLARED` naming the def, not a play that halts "incomplete".
- `W-BEAT-PRIORITY-TIE` follows a chain of `after:`s (r3 after r2 after a `once: user` r1 never
  ties r1); a negated gate over a fact that holds at every point (a seed nothing retracts) is
  "gate never holds" in `check-project` and `lute beats`; `W-ENTRY-WRITE-REREAD` also fires for an
  effect directive's `writes`/`retracts` in a repeatable entry; `lute scenario --facts` draws the
  edge through an occasion's `raisedWhen` (`::give` → `holding(brassKey)` → `canEnter(office)` →
  `room.office`); `E-CLOCK-DECL` sits at the offending clock key, gives a did-you-mean for an
  unknown key (`lats` → `last`) and says "must be a whole number" instead of serde's `expected u32`.
- Diagnostics name their fix (0.27 prerelease). `E-MATCH-RELATION-SUBJECT` shows the working form,
  a `<match>` with no `on` whose arms test the query (`<when test="@badgeCount >= 1">`), now
  documented in branch-match-when; `E-INTO-VALUE` quotes the value, names the path's members and
  points at `value=`; a `<when is>` literal, an unknown `::use` argument (`intor` → `intro`, which is
  then not also "required"), an argument of the wrong type (named with its value) and an undeclared
  `@def` (`@isNigth` → `@isNight`) get a did-you-mean; once a `::use` argument is refused, the
  writes of an `effects: true` component no longer repeat that fault at the use (one typo was three
  errors). `W-QUEST-TIER-IMPLICIT` also flags a parent whose subquests are run-looking and names the
  quests of the tree that must change together, so following it no longer produces
  `E-QUEST-TIER-MIX`; `clock.*` over a `run.*` day counts as run state, `visited()` (kept by a new
  run) does not. A refused `lute test` trace prints a warning as `warning`, not `error`, under the
  document's folded path; coverage labels a match with no `on` "match with no subject".

- Beat templates and `sequence:` (0.27 prerelease). A fault in a template's `beat:` header value
  that no `@param` changes (`once: sometimes`, an unknown `on:`) is reported once at the header key
  in the component, not at every use; a use of a faulty or unknown template no longer adds "names
  no occasion"; a header `@name` no param declares is not derived (no `E-CONN-PROFILE` per use);
  one argument judged twice at a use is one report; a long target-domain list is capped; a bundle
  beat's local id in `visited()`/`prev=` suggests its canonical `<document id>.<id>`; an optional
  condition param left empty or `true` drops its conjunct (no `&& (true)`). `sequence:` on a
  `select: sequence` occasion derives only `on:` and `priority:`, so the listed scenes play in one
  raise; `E-SEQUENCE` shape errors are located, suggest the key, and no longer stop
  `check-project` from checking the documents; an unknown `sequence.occasion` is `E-SEQUENCE`; a
  bad listed id no longer cascades into `E-CONN-UNKNOWN-NODE` in the next scene; a listed scene
  with its own `when:` that stalls the chain is `W-SEQUENCE-STALL`; `lute play`/`lute test` name a
  derived `after:` and say `sequence:` wrote it; `lute new scene --on <sequence occasion>` writes
  no `on:`/`priority:` and says to list the id.

- `lute trace`, `lute compile`, `lute compile-stream` and `lute context` on a
  single file resolve it against the nearest `lute.project.yaml`, as `lute
  check` does (and say so on stderr); `--project` still wins. A scene that
  inherits `defaults.uses` checked `ok` and then `trace` refused it with
  `E-UNDECLARED` and "run `lute check` first". Only an explicit `--project`
  gates on the reconciled project verdict.
- A `transcriptContains` / `transcriptLacks` needle whose attribute block
  names something no transcript line shows (`emotoin=`, `when=`, `code=`) or a
  value outside its domain (`emotion="sadd"`) is refused with a did-you-mean
  before anything plays: a usage error (exit 2) in `lute play`, a failing
  `E-TEST-NEEDLE` test in `lute test`. It made `transcriptLacks` hold although
  the line was said.
- `lute play`: an `occasion:` step's `engine:` write is part of the step — its
  `expect:` judges the raise and the world after it (it was judged against
  the write, "step 1 at engine, repetition 1 … actual none"), and the end
  counts the script's steps; `--json` marks the write's object
  `"beforeRaise": true`. Every `choose:` of a play — top level, an
  `include:`'s, a step's — is checked before the play runs: an unknown
  branch/hub id or option is a usage error (exit 2) located at the key, with
  did-you-mean; an `include:`'s `bridges:` error is located at its own
  `bridges:` key. A halted play reports the step expectations it never
  reached as one miss (`expected steps 24, 25, … to run (45 expectations),
  actual not reached — the play halted with an error at step 23`). An
  `advance:` refused after the game is over prints where the clock stands
  in its header. `expect` `winner` / `offered` / `notOffered` / `presented`
  accept `<id> for <member>` for a `for` beat (a bare id still names every
  member). `--quiet` leaves out a directive effect's `set` that only
  restates the bridge answer printed on its call's line.
- A non-ASCII character outside a CEL string literal — `≥`, a curly quote
  pasted from a word processor, Hangul, a full-width `＝`, an em dash — in a
  `when`, a frontmatter `when:` or a `::set` crashed `lute check`,
  `check-project` and `lute-lsp` (FS-F1, new in the 0.27 prerelease). It is
  `E-CEL-PARSE` again, as in 0.26.
- `E-SET-SHAPE` gives the right fix for more shapes (FS-F3): `run.x: 4` →
  `run.x = 4` (no second `E-CEL-PARSE`), `run.x++` / `run.x--` →
  `run.x += 1` / `-= 1`, `run.x =+ 1` → `+= 1`, `run.x-1` → `run.x -= 1`
  (one error, not three), and a bare `run.x` says "found nothing — write
  `run.x = <value>`". `run.x-=1` without spaces parses as `-=`. The message
  lists `*=`.
- An arm left open before its next sibling — `<choice>` with no
  `</choice>` before the next `<choice>`, likewise `<when>`/`<otherwise>` —
  is one `E-UNCLOSED-TAG` naming the next arm's line; the next arm no longer
  reads as a `<choice>` "outside a `<branch>`" (FS-F15). A frontmatter
  quote left open on its line and `key:value` with no space after the colon
  are located at the slip (not the next line) and name the fix.
- A kind whose members come from its `subsetOf:` sub-kinds lists them in
  declaration order — its own members, then each sub-kind's in the order
  the sub-kinds are declared (file by file across schemas) — not by sub-kind
  name (G-8). `for="kind:…"` presentations, the artifact's `forKind.members`
  and target domains, and `lute trace`'s mock hint all follow it.
- A kind `labels:` entry for a cast member with a `name:` is now the warning
  `W-LABEL-CAST-SHADOWED` at the label (G-16): text renders the cast name,
  so the label was accepted and never shown.
- A state path typed `{ entity: K }` renders the kind's `labels:` in
  `{{…}}` like a `{ domain: K }` path (OT-F-4); both carry `state[].labels`.
- `{{user.bond[occasion.target]}}` interpolates the raised member's family
  path in a kind or `for` beat (G-9). It is checked per member like a guard
  and compiles to a `ref` placeholder whose `expr` is the read.
- One mistake that every member of a kind beat hits alike is reported once,
  with `occasion.target` in the member's place and the members listed, not
  once per member (G-12).
- Message texts caught up with 0.27: the `occasion.target` scope error names
  `for=`; an undeclared `occasion.payload.<field>` says payload fields come
  from the answered occasion; a write to `prev.season.*` explains the season
  mirror; a directive an entry cannot call says why (no `effects:`, a bridge,
  a layer, …) instead of "entries admit no directives" (HW27-06);
  `E-BEAT-ATTR` for a beat key without `on:` suggests `sequence:`; a scene
  with no identity gets one `E-META-MISSING` asking for `id:` instead of three
  naming `character`/`season`/`episode`; `lute init`/`lute new` comments no
  longer cite spec sections (FS-F14).
- `lute --explain` and `--deny` suggest the nearest code for a typo, and the
  diagnostics reference links each `Spec:` citation to its proposal (FS-F11).
- A long `advance:` settles the quests at every clock position it crosses
  (G-3), not only where it stops: seasons open and close, rearms fire and
  quests start, complete and fail (`by`) in the world of each crossed
  position, raised there or not. On a clock that raises no `dayStart` /
  `dayEnd`, a season window inside one advance used to open and close
  without ever starting its quests or failing their deadlines, and a rearm
  looked as if it fired at the arrival. `lute play` prints each crossed
  position that moved a quest as the clock's `set` records of the move
  there followed by that settle (`--json`: entries of `advance.quests`, the
  `set` records under `document: ""`). `advance: day` still skips the rest
  of the day. The docs now warn that a `rearm` with a `done` over state the
  reset keeps (`run.barley >= 10`) completes again at once (G-14), and the
  quests page's rearm example reads a weekly counter.
- `W-BEAT-PRIORITY-TIE` no longer lists a beat that can never be presented —
  its `when` provably false (a finite clock's range included), or dead under
  its occasion's `raisedWhen` gate or the project's `terminal:` — as one that
  "can be eligible at once"; its unreachable verdict says why.
- `lute play`: a plugin call that declares no effect (a fire-and-forget
  engine action such as `::unlockCg`) again prints `(plugin call, not
  invoked)` and carries its JSON `note`; 0.27's effect records had dropped it
  together with the note of calls whose effects the transcript now shows.
- `W-CAST-ABSENT`: a quest's `questComplete` handler assumes the quest's
  completion — one of its required objectives' `done` holds — so a line by a
  speaker whose `present:` every required objective implies needs no guard of
  its own (a write in an objective body or an earlier `questComplete` handler
  still cancels the assumption). `questActive` / `questFailed` handlers are
  unchanged.
- **`lute trace`, `lute test` and `trace_source` execute the compiled IR**
  on the same walker `lute run` and `lute play` use (runtime unification,
  wave 2); the AST walker is gone. Behaviour the two runtimes used to
  disagree on now has one rule:
  - a plugin directive's declared effects (literal, `op`, `fromAttr`,
    `bridgeResult`) apply in trace too and show as `set` steps; `lute play`
    prints each as `set run.sanity = 9  (effect of ::fright)` and drops the
    "plugin call, not invoked" note for a call with only declared effects;
    `lute run --json` `set` records carry `effectOf` (T1-3, D1);
  - exclusivity is checked after every write, so a `::set` that makes a
    derived exclusive pair hold refuses right there (`E-FACT-EXCLUSIVE`) in
    trace, run and play (T1-7, D3);
  - a derived relation whose rule reads state is re-derived after a `::set`
    in play and run (D4);
  - an unbound `occasion.target` in a kind-target beat is undecided: trace
    exits incomplete naming `--state occasion.target=<member|…>` (T1-9);
  - an unanswered bridge result is unknown in every runtime (D7); hub
    `once` is remembered in `scene.visited.<hub>.<option>` (D8); quests
    settle in one order everywhere, a pending objective is reported once
    per trace, and a quest whose objectives are all optional completes
    (D9); an unmocked `quest.<id>.state` reads `unset` in run and play too
    (D17); `{{path}}` labels read the artifact `labels` with the `prev.`
    fallback, and `{{occasion.target}}` the cast `name:` (D12);
  - the engine writes `entry.<id>.everRead` after every read in every
    runtime (D11); `lute run` output still omits it unless the artifact
    declares it.
  Trace's final state now lists every declared path with its default (e.g.
  an unreached `scene.choices.<branch>` as `unset`), and its `said` form is
  the canonical `@speaker{…}: text` line.
- **`transcriptContains` / `transcriptLacks` judge line attributes**
  (round-5 T1-11): `lute play`, play files in `lute test` and scene tests
  now match needles against one canonical transcript form,
  `@speaker{delivery}: text` — the head `lute play` prints (role flag, then
  the line's attributes). A needle line with an attribute block
  (`@sol{emotion="sad"}: …`) matches only a line carrying those attributes;
  one without a block matches whatever the line carries. A
  `transcriptContains` miss quotes the nearest real line (a line with the
  same text but other attributes first, then the needle's speaker's lines),
  and a `transcriptLacks` miss quotes the line that matched
  (`present (line: "…")`), never the needle. A `transcriptLacks` needle
  naming attributes the line does not carry now holds (it failed in 0.26).
- **`lute test` gates a document on its project's verdict**, as `lute trace
  --project` and `lute play` do (round-5 `test-project-envelope`): a scene
  whose read is `E-MAYBE-UNSET` alone but settled by the project's `after:`
  order no longer refuses under `lute test` while `lute trace --project`
  walks it. The project is `--project`, else the nearest
  `lute.project.yaml`; a document outside it keeps the standalone check.

- `::set{ path … }` without an assignment operator is **`E-SET-SHAPE`**
  naming `=` / `+=` / `-=` / `*=` and the write you likely meant
  (`::set{ run.clues - 1 }` → "did you mean `run.clues -= 1`?"). It used to
  check clean and compile to `run.clues = 1`, eating the operator. A param as a
  dotted segment (`run.aff.@who`) is the same error, pointing at
  `run.aff[@who]`.
- A state path typed `{ domain: K }` or `{ entity: K }` (an enum, or a closed
  entity kind) is member-checked everywhere a literal meets it — `::set`
  (`E-SET-TYPE`, with did-you-mean), `==` / `!=` / `in` and `<when is>`
  (`E-WHEN-LITERAL-DOMAIN`), `into=` values (`E-INTO-VALUE`) — and a `<match>`
  over it is exhaustive over K (`E-NONEXHAUSTIVE` names the missing members),
  exactly like an inline `{ enum: […] }` path. The labelled long form of an enum
  no longer loses member checking.
- A plugin directive's `effects.writes` value is validated when the plugin
  loads: a bool/number/string literal, `{ fromBridgeResult: <field> }`,
  `{ fromAttr: <attr> }`, or `{ op: increment|decrement, by: <number> |
  { fromAttr: <attr> } }`. Any other shape — a misspelt key, another op, a
  list, a non-number `by` — is `E-PLUGIN-PARSE` naming the four; so is a
  `fromAttr` naming an attribute the directive does not declare, or a `by:`
  `fromAttr` whose attribute is not `type: number`. `{ fromAttr }` used to load
  clean and write nothing; it is now resolved at compile from the call's
  attribute (or its declared `default:`; a call with neither writes nothing),
  so `lute play` and the artifact apply it. `lute trace` / `lute test` apply a
  directive's literal and `op` writes with the runtime unification later in
  0.27.
- `per: K` declares the members K's `subsetOf:` sub-kinds add (one level or
  more), as relation arguments and beat targets already did: `user.bond.sefa`
  under `per: bonded` with `confidant: { subsetOf: bonded, members: [sefa] }`
  is declared, and a map `default:` may name it.
- A component param's `default: "@def"` is judged for definite assignment at
  each `::use` that omits the argument (`E-MAYBE-UNSET … read through @def` on
  the `::use` line), like the argument it stands for.
- An entity kind with a key other than `members:` / `open:` / `add:` /
  `subsetOf:` / `labels:` is `E-ENTITY-KIND-SHAPE` with did-you-mean
  (`membrs:` → `members`); it was silently ignored.
- `<match on="@def">` whose def expands to a fact query (`holds(…)`,
  `count(…)`, directly or through another def) is `E-MATCH-RELATION-SUBJECT`,
  as the inline `<match on="holds(…)">` is; it used to check clean and
  exhaustive. Both messages now say where the query goes instead: a `when=`
  guard on the line, choice or `::set`, or an arm's `<when test>`.
- `lute scenario knowledge` instantiates a rule the way the checker does: a
  premise over an entity kind (`routeOpen(S) :- suitor(S), not locked(S)`)
  ranges over the kind's members, so `not locked(ren)` reads "holds unless
  defeated — defeated when locked(ren) is asserted by …" instead of "cannot
  be defeated". `lute lore`'s Derived section lists those derivations too.
- Curly quotes from a word processor around an attribute value
  (`label=“Open the oven”`, `‘…’`, or a straight value closed by `”`) are one
  `E-ATTR-QUOTE` at the quote, telling you to retype it as `"`. Each word used
  to be its own `E-UNKNOWN-ATTR` with a bogus did-you-mean.
- `E-META-PARSE` (frontmatter and bare schema YAML) points at the real file
  line and column instead of `1:1`, without serde_yaml's line number (one
  short). A quote nested in a same-quoted value shows the corrected line; a tab
  in the indentation says YAML indents with spaces.
- `W-CODE-AFTER-END` / `W-CODE-AFTER-NEXT` no longer flag a `::mark` or `id=`
  line a `::next{to=…}` jumps to: only the content between the terminator and
  the next jump target is dead, and a mark nothing targets still warns.
- `W-BEAT-PRIORITY-TIE` no longer ties a `once: user` beat with a beat whose
  `after:` waits on `visited()` of it: the `after:` premise is part of
  eligibility.
- `lute beats` gives each ladder cell its own verdict: a beat shadowed on one
  target's ladder reads `shadowed by <id>` there (JSON: additive
  `shadowedBy`) even when it wins elsewhere, and a kind beat's row shows its
  `kind:<kind>` target.
- `lute doctor`'s `lute-lsp beside lute` no longer fails a bun/npm global
  install: the package's `lsp-bin.js` launcher on `PATH` is compared by the
  version it reports; two native binaries are still compared byte for byte.
- A def whose body calls a typed parameter def
  (`harvestOriginal: "@onDays(8, 14)"`) or names another def takes that def's
  type (defs settle in dependency order) instead of
  `E-DEF-DECL … cannot be inferred`.
- A component's bad literal param default is reported once, at its `params:`
  entry, with did-you-mean, not at every `::use`; a `@def` default of the
  wrong type names both types.
- A param as a dotted segment in a condition (`when="run.aff.@who > 1"`) is
  `E-CEL-PARSE` pointing at `run.aff[@who]`; it used to be read as
  `run.aff.who`.
- A use written only in a comment (`/* … */`, a line-leading `//`, a YAML
  `#`) is no longer a read for `W-RELATION-UNREAD`, `W-DEF-UNUSED`,
  `W-DOMAIN-UNREAD` or the bridge result fields a `bridges:` answer must give.
  `docs/examples/investigation` had been clean only through such a comment.
- `lute play`'s note that a hand-raised clock occasion "runs twice" appears
  only when a later `advance:` in the script actually crosses that moment.
- `lute play` / `lute run` take the `<match>` arm the author meant when an
  `is` arm's subject or `test` has no portable `expr` — a `<match on="@def">`
  whose def reads `visited('…')`, or `<when is="active"
  test="count(carrying(_)) >= 3">`. The compiled arm's `test` now carries the
  whole condition as CEL (`(visited('find')) == true`,
  `quest.lantern.state == 'active' && (count(carrying(_)) >= 3)`); it used to
  be empty (the match fell to `<otherwise>`) or the `test` alone (the `is`
  pattern was ignored). An arm that would lower to neither is
  `E-COMPILE-INTERNAL` instead of an arm no engine can take.

- `lute test`: an `eligible: true` miss names the conjunct of the `when` that is false and what it read (`its `when` is false because `run.aff.ren >= 7` is false (`run.aff.ren` is 6) — the whole `when`: …`; a negated fact that holds says `seeded by `facts:`` when the test seeded it), and it is printed before the state and fact misses it causes; an `eligible:` key naming nothing gets a did-you-mean (OT-F-10).
- `lute calendar`: `run.aff[run.route]` no longer refuses a route axis with values outside the family (`hotaru`, `alone`): at such a value the tied axis sets nothing, one cell stands for all its values, and a note says so; only an axis naming no member at all is refused. A misspelt indexing axis gets a did-you-mean and the axis list no longer names the bad axis itself (OT-F-12).
- `lute scenario reach --endings`: exits 1 when an ending is unreachable; a never-holds row cites the error inside its `when` (`caused by E-WHEN-LITERAL-DOMAIN at …`, JSON `causes`); bare `--endings` in a game with no `::end` points at `--endings=<occasion>`; `--format` is accepted after the sub-view (OT-F-13).
- `lute test --coverage`: an untested beat is no longer listed a second time under "no play presents" (that list now holds only the beats a test traces but no play presents), and a file traced from `tests/` is spelled one way (`./scenes/a.lute`, not `./tests/../scenes/a.lute`) (OT-F-14).

### Compatibility

- **`::set` without an assignment operator is `E-SET-SHAPE`.**
  `::set{ run.clues - 1 }` used to check clean and compile to
  `run.clues = 1`; it is now an error naming `=` / `+=` / `-=` / `*=` and the
  write you likely meant. A param as a dotted segment (`run.aff.@who`) is the
  same error, pointing at `run.aff[@who]`.
- **`{ domain: K }` / `{ entity: K }` paths are member-checked.** A literal
  that meets such a path outside K — in `::set` (`E-SET-TYPE`), `==` / `!=` /
  `in` and `<when is>` (`E-WHEN-LITERAL-DOMAIN`), or `into=` (`E-INTO-VALUE`)
  — is an error, and a `<match>` over the path must cover K
  (`E-NONEXHAUSTIVE` names the missing members), as for an inline
  `{ enum: […] }` path. The same member check meets play and save seeds,
  `engine:` writes, test `state:` seeds and `expect.state` values.
- **`<match on="@def">` over a fact query is `E-MATCH-RELATION-SUBJECT`.** A
  def expanding to `holds(…)` / `count(…)` used to check clean and
  exhaustive as a match subject. Test the query where it goes instead: a
  `when=` guard on the line, choice or `::set`, or an arm's `<when test>` in a
  `<match>` with no `on` (`<when test="@badgeCount >= 1">`).
- **Curly-quoted attribute values are `E-ATTR-QUOTE`.** `label=“Open”` (or
  `‘…’`, or a straight value closed by `”`) is one error at the quote; retype
  it as `"`. Each word used to be its own `E-UNKNOWN-ATTR`.
- **`W-QUEST-TIER-IMPLICIT` can appear in a clean project.** A `<quest>`
  with no `tier=` (and no `defaults.questTier`) whose conditions read only
  run state warns; write `tier="run"` / `tier="user"` or set
  `defaults.questTier`. With `--deny-warnings` it fails the project. The
  other new advisories — `W-SEQUENCE-ORDER`, `W-SEQUENCE-STALL`,
  `W-LABEL-CAST-SHADOWED`, `W-DEADLINE-NEVER` — fire only on 0.27 syntax.
- **`lute test` gates on the project's verdict.** A document is judged under
  `--project`, else the nearest `lute.project.yaml`, as `lute trace
  --project` and `lute play` do; a document outside a project keeps the
  standalone check. A test refused standalone may now run, and one the
  project refuses now fails.
- **`lute test` judges `raisedWhen` and `terminal:`.** `eligible: true` on a
  beat the engine would not raise under the test's mocks — its occasion's
  gate is false, or `terminal:` holds — used to pass and now misses, naming
  the gate and its false reads (`--json` adds `notRaised`).
- **Transcript needles judge their attribute blocks and are validated.** A
  needle written `@sol{emotion="sad"}: …` matches only a line carrying those
  attributes, so a `transcriptLacks` needle with a block now holds when the
  words are said another way (it failed in 0.26), and one naming something
  no line shows (`emotoin=`, a value outside its domain, an unknown
  `@speaker`) is refused before anything plays.
- **Play and test scripts are validated before they run.** Every `choose:`
  (top level, an `include:`'s, a step's) naming an unknown branch, hub or
  option, an `expect` naming a beat no project beat has or a clock position
  the clock does not have, a `facts:` / `notFacts:` atom with an undeclared
  relation, wrong arity or non-member argument, and an out-of-domain state
  value are usage errors in `lute play` (exit 2) and invalid tests in
  `lute test`, located at their line; they used to be misses or to hold
  vacuously.
- **`lute trace`, `compile`, `compile-stream` and `context` apply the
  nearest `lute.project.yaml`** to a single file, as `lute check` does (and
  say so on stderr); `--project` still wins. Output for a file inside a
  project now reflects its plugins, schemas and defaults.
- **One runtime.** `lute trace` / `lute test` execute the compiled IR on the
  walker `lute run` and `lute play` use, so behaviour the runtimes disagreed
  on has one answer: a plugin directive's declared effects apply in trace,
  exclusivity is checked after every write, an unanswered bridge result is
  unknown everywhere, quests settle in one order, and trace's final state
  lists every declared path with its default. A test that passed on the old
  trace walker's answer may now fail.
- **Diagnostics JSON: plain `message`, new `spec`.** Every message drops its
  `(dsl … §…)` citations; the cited sections move to an optional `spec`
  array in `--json` (and the wasm build). Tooling that parsed citations out
  of `message` should read `spec`, or `lute --explain <CODE>`.
- **`lute test --coverage --json`: `coverage.untested` changed shape.** It
  is a list of `{file, id, kind}` units at beat granularity (was document
  paths), beside the new `coverage.notPresentedByPlay`.
- **`lute play` transcripts change.** A directive's declared effects print
  as their own `set` / `assert` / `retract` lines (`set run.sanity = 9
  (effect of ::fright)`), and five or more `when: false` candidates at one
  raise fold into one count line (`✗ 8 beats — when: false: a, b, c, …`;
  `--quiet` leaves them out, `--json` still lists every candidate). Scripts
  and tools that diff transcripts see the new lines.
- **Restamp `luteVersion:`.** A document or `defaults:` stamped with an
  older version draws `W-LUTE-VERSION-STALE`, which names the stamp to
  write (`luteVersion: "0.27.0"`); bump the stamp, or `--deny-warnings`
  fails the project.
- **Schema file renamed; additive IR.** The version strings move to `0.27.0`
  and `schemas/lute-ir-0.26.schema.json` is renamed to
  [`schemas/lute-ir-0.27.schema.json`](schemas/lute-ir-0.27.schema.json)
  (`$id` updated). Every new field is optional and appears only when the
  source uses the feature: `gates`, `terminal` and `seasons` on the artifact
  and `project.index.json`, `clock.last` / `clock.days`, `forKind` and
  `spentBy` on `BeatIr` / `EntryCmd` / `BeatCmd` / index beat rows, the
  `once` values `"week"` / `"season:<name>"`, `QuestCmd.rearm` and the quest
  tier `"season:<name>"`, a `plugin` record's resolved `asserts` /
  `retracts`, `labels` on `entities[]` and `state[]` entries, and the
  placeholder format `"plural"` with its `forms`. A beat template,
  `sequence:` and `occasion.target` as a ground term lower to plain beats
  and conditions, so none adds a field. A `<match>` arm's `test` now carries
  the whole arm condition as CEL (an `is` arm over a subject with no portable
  `expr` used to lower to an empty or partial `test`). `lute.core` does not
  move, so neither does `capabilityVersion`. Engines gate on MAJOR, so
  nothing widens; the tree-sitter grammar admits a self-closing template use
  and treats `rearm` / `spentBy` values as CEL.

## [0.26.0] - 2026-09-26

**Scale and many authors.**

A minor release from a fourth dogfood round: a lead and four area writers
building one monster-collecting RPG in parallel. The fact analysis and
`lute test` load a project once and `lute test` runs in parallel; state, kinds,
content ids, rewards and display names are held consistent across the files
several authors write (`E-STATE-DECL-CONFLICT`, kinds assembled with `add:`,
`defaults.uses` globs, entity-typed directive attributes, `E-REWARD-TARGET`,
`W-DISPLAY-NAME-DUP`); components reach parity with direct text and gain
`@@who:`, param defaults and their own result slots; directives take `when=`;
one beat may answer a whole kind; a rule body may count; and `lute trace` /
`lute test` agree with `lute play` on `::next`, handler accepts and ineligible
beats. The language and the IR both earn the move (the IR additively); see
[`docs/proposals/scenario-dsl/0.26.0.md`](docs/proposals/scenario-dsl/0.26.0.md)
and [`docs/versioning.md`](docs/versioning.md).

### Added

- Kind targets at run time (dsl 0.26.0 §5): a `target="kind:<kind>"` beat
  compiles with `targetKind: { kind, prefix, members }` on its scene
  `meta.beat`, `entry` / `beat` record and `ProjectIndex.beats` row. `lute
  play` and `lute calendar` offer it for every `<prefix>.<member>` raise,
  rank it after the member-specific beats of its priority, and bind
  `occasion.target` (the member id) in its `when`, guards and text; the
  value never outlives the presentation. `lute beats` shows a `kind:<kind>`
  ladder for the members no beat names on its own, and `--target` accepts
  any member. `lute test` reads a kind beat's member from
  `state: { occasion.target: … }`. A kind target counts as a read of the kind
  (no `W-DOMAIN-UNREAD`), and `<match on="occasion.target">` needs no
  `unset` arm.
- Counts in rule bodies (dsl 0.26.0 §6): `count(R(…)) <op> n` and
  `countDistinct(R(…), V…) <op> n` (`>=`, `>`, `<=`, `<`, `==`, `!=`) are
  rule-body literals. A variable bound by another literal is read (the
  count is per binding); any other ranges over the facts. The IR carries
  them as `{ kind: "count", atom, distinct?, op, n }`; `lute trace`, `test`,
  `play` and `run` evaluate them one stratum above the counted relation. A
  count over a relation that depends on the rule's own head is the new
  `E-RULE-AGGREGATE-CYCLE`.
- `E-STATE-DECL-CONFLICT` (dsl 0.26.0 §2.1): `check-project` compares every
  frontmatter `state:` declaration of one path (inline, or imported from a
  schema) and names both files and lines when `type`, `default`, `per` or
  `owner` disagree (`scene.*` paths are scene-local and exempt). `lute play`
  refuses a project whose documents declare one path with two types.
- `defaults.uses` entries may be globs (`schema/areas/*.schema.yaml`,
  `**`), expanded in path order; `defaults.questTier: run | user` sets the
  `tier=` of every `<quest>` that writes none (dsl 0.26.0 §2.4).

- `lute play`: `advance: { to: <slot> }` / `advance: { to: { weekday, slot } }`
  moves the clock forward to the next such position after the current one
  (never backward, never zero steps: already there, the next one); a step
  `expect: { clock: { weekday, slot, day } }` judges where the clock stands
  (dsl 0.26.0 §7, T2-5).
- `lute play`: `engine: { accept: [quest ids] }` accepts an accept-driven
  (e.g. `accept="external"`) quest mid-play, printed `quest <id> accepted
  (engine)` (T2-9).
- `lute play` / `lute test`: an entry may be named `<document id>.<entry id>`
  in `pick:`, `entriesRead:`, step `winner` / `offered` / `notOffered` /
  `presented`, a test's `entry:` / `entries:` and `expect.eligible` keys, and
  `lute trace --entry` (dsl 0.26.0 §8, T3-10).
- `W-ENTRY-WRITE-REREAD` (dsl 0.26.0 §8, T3-4): an entry beat (`on=`) without
  `once` whose body has `::set` / `::retract` — re-presented by every raise,
  but its writes apply on the first read in a run only. `::assert` is exempt:
  the fact holds for the rest of the run either way.
- `lute scenario --facts` (dsl 0.26.0 §8, T3-11) draws fact-producer edges
  (`scene(mid.gameCorner) -> scene(east.ashTowerLens) [hasItem(spectralLens)]`,
  `…, via canPass(x)` through the rules) and layers the graph over them where
  they close no cycle; `--format json` carries `factEdges`, `--format dot`
  dotted edges.
- `lute beats` shows `covered by <id>` for a fallback an earlier, never-spent
  beat whose `when` it implies always beats (`--json`: `coveredBy`) (T3-3).
- `lute doctor --strict` exits `1` when any check fails (`✗`) (T3-12).
- `lute-lsp` notices when its own binary was replaced and publishes one
  `lute-lsp-stale` "stale server, restart" diagnostic (naming its version)
  instead of an older build's results; checker-backed requests answer nothing
  (T3-12).
- Component params take `default:` (`won: { type: bool, default: "@wonFight" }`,
  a literal or a `@def` resolved in the host); an omitted argument takes it,
  judged at the `::use` like the argument it stands for, though not yet for
  definite assignment (fixed in 0.27) (dsl 0.26.0 §3.3).
- A component body may read the result slots of its own plugin directives
  (`<match on="scene.battle.fight.won">`, `{{scene.battle.fight.turns}}`);
  they are not ambient state (dsl 0.26.0 §3.3).
- `@@who:` in a component body speaks as the cast member the `speaker` param
  `who` names at each `::use` (dsl 0.26.0 §3.2): the compiled line's
  `speaker` is that member, and its emotions (`E-BAD-ENUM`), `present:`
  (`W-CAST-ABSENT`, once per member and guard) and stage exits
  (`W-STAGE-ABSENT`) are judged at the `::use`. `@@x` for a param that is no
  `speaker` param, for no param, or outside a component is `E-COMPONENT-ARG`.
- `when="<condition>"` on `::use`, `::accept`, `::assert`, `::retract` and
  plugin passthrough (and bridge) directives, with the meaning of
  `::set{… when=}` (dsl 0.26.0 §4): compiled to a one-arm match (a guarded
  `::use` runs its whole expansion or none of it), never a definite write,
  fact or accept for the checker, and a `::use`'s argument reads are judged
  under its guard. A builtin-lowered directive (`::auto`, `::bg`, `::end`,
  `::mark`, a plugin `lower:` record) refuses `when=` (`E-UNKNOWN-ATTR`), as
  does a `<track>` clip (`E-TIMELINE-CONTENT`). `lute play` shows a skipped one
  as `skip ::give{item="potion"} — when: false` (`skip assert …`,
  `skip ::use{component="eff" n="5"} …`), like a skipped guarded `::set`;
  trace and test apply the same writes, facts and accepts as play.
- Entity kinds assembled across schema imports (dsl 0.26.0 §2.3):
  `entities: { person: { add: [grannyWren, oldSalt] } }` adds members to the
  kind exactly one import declares. An `add:` without that declaration (with
  a did-you-mean), onto an `open:` kind, or beside `members:`/`open:`, and a
  member listed by two files (or re-added over the declaration), are
  `E-ENTITY-KIND-SHAPE` naming both files, reported once at the schema line.
- `check`: a beat or entry may target a whole kind, `target="kind:bugMon"`
  (a closed kind within the occasion's `{ prefix, entity }` domain, else
  `E-BEAT-ATTR` with a did-you-mean); its `when`, guards and text read the
  raised member as `occasion.target`, typed by the kind (engine-owned,
  always assigned). A read of `occasion.target` in a beat that does not
  target a kind is `E-UNDECLARED`. At equal priority a kind beat ranks after
  the other candidates (the beat naming the member outranks it) — no
  `W-BEAT-PRIORITY-TIE` between them; `W-BEAT-SHADOWED` judges it per member
  (dsl 0.26.0 §5).
- Directive attributes may be typed by a project entity kind (dsl 0.26.0
  §2.5): `item: { type: { entity: bagItem } }`. A non-member is `E-BAD-ENUM`
  with a did-you-mean; an undeclared kind is `E-DOMAIN-UNKNOWN`.
- `E-REWARD-TARGET` (dsl 0.26.0 §2.5): a reward kind's `target:` contract is
  checked. `{ entity: <kind> }` or `{ provider: <name> }` validates the
  `target=` value (did-you-mean for entities; a stale provider snapshot only
  warns); `required: true` rejects a reward with no `target=`. A contract
  naming both `provider:` and `entity:` fails the plugin load.
- `lute refs <dir> --attr <directive>.<attr>` / `--reward <KIND>` (text or
  `--json`, dsl 0.26.0 §2.5): every value with the documents and lines using
  it; a reward without a target is listed as `(no target)`.
- `W-DISPLAY-NAME-DUP` (dsl 0.26.0 §2.8), advisory in `check-project` and
  `lute lint`: two cast entries with the same `name:`, a cast name equal to a
  `::use{… name="…"}` display string, or two such strings for different
  speakers (`who=`). `lute lint --deny W-DISPLAY-NAME-DUP` accepts the code.

### Changed

- A `subsetOf:` sub-kind's members are members of its parent, except for
  `per:` families (fixed in 0.27): a trainer
  listed in `trainer` (⊂ `person`) no longer needs a second line in
  `person` (restating it stays legal). The former "not a member of its
  parent" `E-ENTITY-KIND-SHAPE` is gone (dsl 0.26.0 §2.3).
- `E-ENTITY-KIND-SHAPE` also reports a member listed twice in one entity
  kind or enum, at the second position with both lines (dsl 0.26.0 §2.2).
- Sub-kind `E-ENTITY-KIND-SHAPE`, `E-USES-DUP-STATE`, `E-USES-DUP-DEF`,
  `E-USES-DUP-RELATION` and peer `E-KIND-NAME-CLASH` are reported once at the
  schema line and folded (`(+N more callers)`) instead of at every importing
  document's `1:1` (dsl 0.26.0 §2.7).
- `E-FACT-DOMAIN` for a non-member (including one reached through a `::use`
  argument) carries a did-you-mean; the `E-ENTITY-KIND-CLASH` hint offers
  renaming when the two kinds are different things (dsl 0.26.0 §8).
- trace/test follow a taken `::next` to its label instead of ending the walk
  there; the transcript shows `<next -> label>` (T1-4).
- trace/test apply an `::accept` in a quest `<on>` handler to a quest of the
  same document, as play does (T1-5).
- `lute test`: a test that walks an entry, bundle beat or scene whose `when`
  is false under its mocks fails unless it asserts `eligible:`; with
  `eligible:` asserted the ineligible body is not walked; `eligible:` works on
  scene beats (`on:`) (`when`, `after:`, spent `once: user`) (T1-7).
- `lute test` / `lute trace --project`: `accepts:` resolves quests project-wide;
  a quest document may seed its own `quest.<id>.*` (the walk starts the quest
  there) (T3-5).
- `transcriptContains` / `transcriptLacks` (test and play) drop line
  attributes from the needle; a `transcriptContains` miss names the nearest
  presented line (T3-6).
- `W-BEAT-PRIORITY-TIE` normalizes negations (De Morgan, `!(x < 1)` as
  `x >= 1`) and compares the conditions' alternatives, reads the fact
  envelope's must set at either beat's `when` slot, treats a fact only a
  beat's own unplayed `once: run`/`user` presentation asserts as absent while
  it is eligible, and says why two `when`s are not exclusive (a flag that
  outlives `once: run`, a fact asserted elsewhere or `tier: user`, the paths
  each reads) (dsl 0.26.0 §8, T3-2).
- `E-STATE-DECL-CONFLICT` does not report a declaration that refines one it
  `extends:` (an overridden default).
- `check-project --wip` checks a spine before the areas exist (dsl 0.26.0
  §2.6, T2-7): a relation only a component `::assert` with an unbound
  `@param` writes (`hasBadge(@badge)`, no `::use` yet) counts as unproduced
  for specific arguments, so `E-OBJECTIVE-UNSATISFIABLE` / `E-BEAT-UNREACHABLE`
  caused only by it are warnings, and a required `<objective quest=…>` whose
  child is dead only for that reason is a warning too; each message says so.
  A relation with no such component producer whose producers never match
  stays an error.
- `lute scenario knowledge` traces a rule's `count(…)` / `countDistinct(…)`
  premise with the producers of the facts it counts beneath it (dsl 0.26.0
  §6).
- A string a guard compares with a path whose values are a closed set of
  strings — an enum path, `occasion.target`, a quest's `state` /
  `failedBy`, `scene.choices.<id>`, an enum param, `$` — must be one of
  them: `==` or `!=` on either side, or an `in [...]` element, outside the
  domain is `E-WHEN-LITERAL-DOMAIN` (the code a foreign `<when is>` literal
  gets) with a did-you-mean (`` `'actve'` is not a member of
  `quest.q.state`'s domain [active, complete, failed, unset] — did you mean
  `'active'`? ``). It owns the dead-guard error such a comparison used to
  raise (`E-ARM-DEAD` on a line, choice, arm or guarded directive), as
  `E-UNSET-LITERAL` does, and fires for `!=`, which never reached it
  (dsl 0.26.0).

### Fixed

- 0.26 docs pass (tool contradictions): the T1-7 ineligible-presentation
  failure only suggests mocks the loader accepts — a `quests:` seed of a
  quest a scene's `after:` / a bundle beat's `after=` names is legal (under a
  resolved project, any project quest's status), and trace/test judge an
  entry's spent `once` (`once="user"` by `entriesRead: { user: [id] }`,
  `once="run"` by `entriesRead: { run: [id] }`, which also sets `everRead` as
  a play save does), naming the mock in the failure; `entriesRead:` is legal
  for any entry the document declares. An imported rule's
  `E-RULE-AGGREGATE-CYCLE` / `E-DATALOG-UNSTRATIFIED` is reported at the
  schema line, folded across importers, and by `lute check <schema>.yaml`.
  `lute trace` / `lute test` render `{{occasion.target}}` by the cast
  `name:` like `lute play`. `W-DISPLAY-NAME-DUP` for cast entries nobody
  speaks as is anchored at the cast entry (plugin export or schema), not at
  the project's first document. A plugin directive attribute named `when` is
  `E-PLUGIN-PARSE` at load (`when=` is the core directive condition).
- 0.26 prerelease review (Monster League): a `{ entity: K }` / `{ domain: K }`
  directive attribute reached through a component param (or a param typed
  that way) is checked at the `::use` argument, with a did-you-mean (N1);
  `lute refs --attr` lists values passed through components at the `::use`
  line, `via component <name>` (N2); the T1-7 ineligible-presentation failure
  names the false premise (`its \`after: visited("a")\` is false — mock
  \`visited: [a]\``) and drops the stale "walk below" note (N3); a duplicate
  `add:` member is anchored at the member's own line and names both files and
  lines (N4); `E-DEF-DECL` for an imported def is reported once at the schema
  line, folded, and `visited(…)` / `validAt(…)` infer `bool` (N5); cast
  `sharedName: true` exempts an intended role name from `W-DISPLAY-NAME-DUP`
  (N6); a `defaults.uses` glob over a missing directory matches nothing
  instead of failing the project (N7); `{{occasion.target}}` compiles to
  `{"kind": "occasionTarget", "entityKind": K}` and `lute play` renders the
  cast display name (N8); a malformed component `params:` message shows the
  `{ type, default }` long form, and `docs/runtime/state-lifecycle.md` says
  which occasions `advance: { to }` raises (N9).
- A `::use` declares in its host the result slots of the plugin directives
  its component body holds (bound to the use's arguments): the check, the
  compiled `state` table, trace mocks and `lute play` see them. `lute play`
  types a bridge answer by that slot, else by the capability's `result:`
  shape, and refuses an untyped answer instead of storing `"true"` as a
  string (dsl 0.26.0 §3.1, T1-2).
- A `{{…}}` inside a string component argument keeps its placeholder record
  after expansion; `as=@who` over a `speaker` param and `{{@p}}` inside a
  line attribute string render as in the text (the cast display name)
  (dsl 0.26.0 §3.1, T1-6).
- `lute play`: `engine: { accept: [q] }` on a quest that is already active,
  complete or failed prints `note: quest q is already active — engine
  accept ignored` (with its status; JSON `{"kind": "acceptIgnored", …}`)
  instead of a second `quest q accepted (engine)`, and changes nothing
  (dsl 0.26.0 §7).

### Performance

- The fact analysis no longer grows with the product of beats and static
  facts (dsl 0.26.0 §1, T2-1). A seed's stability (`key:` displacement) is
  a lookup instead of a scan of every produced fact — the scan made the must
  walk quadratic in the seed count and was the bulk of the cost; must sets
  share the root's seeds instead of copying them per slot; the derived
  closure is prepared once per root (the seeds' closure included), extended
  by semi-naive rounds for a slot whose facts differ, shared by every slot of
  the same shape, and computed only for a slot something reads. Output is
  unchanged. Monster League (818 beats, 211 static facts): `check-project`
  6.1 s → ≈1.2 s, a play load ≈12 s → ≈1.5 s.
- `lute test` collects each project's producer set / quest ids once and
  compiles a play project once for all its plays, then runs the tests, and
  then the plays, in parallel (`RAYON_NUM_THREADS` respected), reporting in
  the existing order with each one's stderr replayed in that order. A play
  project's compile diagnostics are printed once rather than once per play.

### Compatibility

- **A test that walks an ineligible beat now fails.** A `*.test.yaml` test
  whose entry, bundle beat or scene has a `when` (or `after:`, or a spent
  `once: user`) that is false under its mocks used to walk the body anyway;
  it now fails and names the false premise. Assert `eligible: false` (the
  body is then not walked) or fix the mocks so the premise holds (T1-7).
- **`::next` is followed by trace and test.** A taken `::next` used to end
  the `lute trace` / `lute test` walk (reported `complete`); the walk now
  continues at the label, so expectations after a jump are judged and a test
  that passed only because the walk stopped there may fail (T1-4). An
  `::accept` in a quest `<on>` handler is applied, as play does (T1-5).
- **A `transcriptLacks` needle with line attributes can now fail.**
  `transcriptContains` / `transcriptLacks` drop line attributes from the
  needle, so `transcriptLacks: ["@fixer{mono}: She remembered."]`, which
  matched no line before, now matches that line when it plays, and a test
  or play that passed on it fails (T3-6).
- **Restamp `luteVersion:`.** A document or `defaults:` stamped with an
  older version draws `W-LUTE-VERSION-STALE`, which names the stamp to
  write (`luteVersion: "0.26.0"`); bump the stamp, or `--deny-warnings`
  fails the project.
- **`E-STATE-DECL-CONFLICT` can redden a project.** Two frontmatter `state:`
  declarations of one path — inline or imported — that disagree on `type`,
  `default`, `per` or `owner` are now an error naming both files and lines;
  a project that declared a path twice with different shapes must pick one.
- **Duplicate members are an error.** A member listed twice in one entity
  kind or enum, or added to a kind by two files, is `E-ENTITY-KIND-SHAPE`.
- **`E-WHEN-LITERAL-DOMAIN` covers `!=` and `in`.** A compared string outside
  a closed domain (an enum path, `occasion.target`, a quest's `state` /
  `failedBy`, `scene.choices.<id>`, an enum param, `$`) is now reported on
  `!=` and as an `in [...]` element too, and replaces the `E-ARM-DEAD` a
  non-member `==` literal used to cause. Tooling keyed on `E-ARM-DEAD` for
  such a literal sees the new code.
- **Untyped bridge answers are refused.** `lute play` types a bridge answer by
  its result slot, else by the capability's `result:` shape, and refuses one
  it cannot type instead of storing it as a string.
- **`when` is reserved on directives.** `::use`, `::accept`, `::assert`,
  `::retract` and plugin passthrough (and bridge) directives take
  `when="…"`; a plugin manifest that declares a directive attribute named
  `when` is rejected at load (`E-PLUGIN-PARSE`), and every core staging
  directive (and a plugin `lower:` record) refuses `when=` (`E-UNKNOWN-ATTR`).
- **New advisories.** `W-ENTRY-WRITE-REREAD` (a repeatable entry beat whose
  body sets state or retracts a fact) and `W-DISPLAY-NAME-DUP` (two cast
  entries or speaker display names exactly equal; `sharedName: true`
  exempts a role name) can appear in a previously clean `check-project`; with
  `--deny-warnings` they fail it.
- **`defaults.uses` takes globs.** An entry containing `*`, `?` or `**` is
  expanded in path order, and one that matches nothing — an empty or
  missing directory — imports nothing; only a literal path must exist. A
  literal file name containing those characters is now read as a glob.
- **Schema file renamed; additive IR.** The version strings move to `0.26.0`
  and `schemas/lute-ir-0.25.schema.json` is renamed to
  [`schemas/lute-ir-0.26.schema.json`](schemas/lute-ir-0.26.schema.json)
  (`$id` updated). Every new field is optional and appears only when the
  source uses the feature: `targetKind` on `BeatIr` / `EntryCmd` / `BeatCmd` /
  index beat rows, the placeholder kind `"occasionTarget"` (with its
  `entityKind`), and the rule-body literal `"count"`. A directive's `when=`
  lowers to a one-arm match and a component param's `default:` to its value,
  so neither adds a field. `lute.core` does not move, so neither does
  `capabilityVersion`. Engines gate on MAJOR, so nothing widens; the
  tree-sitter grammar is unchanged.

## [0.25.1] - 2026-09-26

**Faster project commands.**

A performance patch on the `0.25` line: no language change and no IR shape
change. Project commands scale linearly in the document count and use every
core; output, diagnostics and exit codes are byte-identical. See
[`docs/versioning.md`](docs/versioning.md) for what each axis earned.

### Performance

- **Span offsets are O(line), not O(file).** Every source span computed its
  file-relative UTF-16 offsets by rescanning the text from the start of the
  file, so the spans of one large document cost time quadratic in its size.
  They now come from a per-line prefix table built with the line index. On a
  synthetic project of 1600 scenes plus one 1600-entry lore file (10-core
  Apple M1 Pro), `check-project` falls from 18.4 s to 2.7 s.
- **Project commands scale linearly and use every core.** `check-project`,
  `lore`, `scenario`, `beats`, `test`, `play`, `doctor`, `loc` and
  `compile --all` no longer recompute the inputs every document of a project
  shares. One per-run memo (no on-disk cache) loads each
  `lute.project.yaml`, provider catalog and activated capability snapshot
  (plugin load, assembly, `capabilityVersion` hash) once per project root and
  `(profile, plugins)`, and resolves each `uses:`/`extends:` and
  `components:` import DAG once per importing directory — the schema YAML used
  to be re-parsed for every scene. Each document is parsed once instead of
  three times, and the per-document check, the project compile pass, the
  `compile --all` builds, `lore`'s parse and the independent project-wide
  passes run in parallel (rayon; `RAYON_NUM_THREADS` is honored), folded back
  in walk order so output, diagnostics and exit codes are byte-identical.
  Several per-document `find`-by-path scans in the project passes, quadratic
  in the document count, are now map lookups. On synthetic projects of N
  scenes plus one N-entry lore file (10-core Apple M1 Pro, median of 5;
  "before" is the preceding commit, span-offset fix included):

  | N scenes | command         | before | now    | now, 1 thread |
  |----------|-----------------|--------|--------|---------------|
  | 200      | `check-project` | 251 ms | 115 ms | 151 ms        |
  | 200      | `lore`          | 212 ms | 96 ms  | 116 ms        |
  | 800      | `check-project` | 1.11 s | 360 ms | 526 ms        |
  | 800      | `lore`          | 960 ms | 270 ms | 368 ms        |
  | 1600     | `check-project` | 2.73 s | 676 ms | 1.03 s        |
  | 1600     | `lore`          | 2.34 s | 511 ms | 702 ms        |

  The wasm playground build is unaffected: the parallelism lives in the CLI
  crate only.

## [0.25.0] - 2026-09-25

**Exclusion, shared spends, graph edges.**

A minor release closing the items the 0.24.0 pre-release review left open —
each a workaround a round-3 author still carried. A relation may declare the
relations it never holds together with, and the checker, `lute play` and
`lute trace` hold content to it; beats that tell one event in different places
share one spend; a bundle beat's `after=`, a quest's subquests and its `start`
reads become scenario-graph edges; a quest may be accepted outside the script;
a reserved relation may say when the engine changes it; and a bridge answer
need not invent fields nobody reads. The language and the IR both earn the move
(the IR additively); see
[`docs/proposals/scenario-dsl/0.25.0.md`](docs/proposals/scenario-dsl/0.25.0.md)
and [`docs/versioning.md`](docs/versioning.md).

### Added

- `{{run.day:ordinalWord}}`: an ordinal-word format hint beside `:ordinal`
  (dsl 0.25.0 §8). The IR placeholder carries `format: "ordinalWord"` for the
  engine to localize; `lute play` / `lute run` / `lute trace` render the
  English words `first` … `twentieth` and fall back to `:ordinal` digits
  (`21st`) above; the checker admits it wherever `:ordinal` is admitted.
- **Shared spends** (dsl 0.25.0 §2, SU N9): a scene (`share:`), bundle
  `<beat share=…>` or `<entry share=…>` beside a written `once` names a
  project-wide key; presenting any beat of the key (an entry: reading it)
  spends every beat of it for the `once` period. `E-BEAT-ATTR` for a
  malformed key, `share` without a written spending `once` (absent or
  `false`), and (`check-project`) beats of one key with different `once`s.
  `lute play` / `lute calendar` spend the key together (`✗ … once: day —
  share: solWarm already spent today by talks.solWarmRadio`), `lute beats`
  shows the key in its `once` column (`share` in `--json`), the presence
  ladder and `W-BEAT-PRIORITY-TIE` read the whole key's flags. IR:
  `BeatIr.share`, `EntryCmd.share`, `BeatCmd.share`, `IndexBeat.share`
  (absent when unauthored).
- **Bundle beat `after=`** (dsl 0.25.0 §3, LH N9): `<beat after="…">` has a
  scene `after:`'s meaning — an eligibility conjunct (`lute play`,
  `lute calendar`, `lute trace --beat`: `not eligible: \`after\` prerequisite
  not satisfied`) and a scenario edge (`E-CONN-PROFILE`,
  `E-CONN-UNKNOWN-NODE`, cycles, envelopes as for a scene; `scenario reach`
  prints the `after:` formula and its referenced nodes). IR:
  `BeatCmd.after` plus a `prereqEdges` row keyed by the beat's canonical id.
  `lute scenario` lists a scene or bundle beat whose `when` has a
  `visited()` conjunct but no `after` as unanchored, with the `after` to
  write (`unanchoredHints` in `--format json`).
- **Exclusive relations** (dsl 0.25.0 §1, LH F17): a relation may declare
  `excludes: [other, …]` — the relations it never holds together with on the
  same arguments. The declaration is symmetric, and each partner must be a
  declared relation with the same argument kinds (else `E-RELATION-DECL`).
  `check-project` reads `holds(A(x)) && holds(B(x))` as false (a dead guard,
  `E-ARM-DEAD` / `E-BEAT-UNREACHABLE` naming the exclusion) and
  `!holds(B(x))` as following from `holds(A(x))` — in the same guard, an
  enclosing one, or anything else on every route — so such a guard is
  `W-FACT-GUARANTEED`, and cast presence uses it too. A guard's
  `holds(dead(x))` counts inside its region even when `dead` is
  engine-`reserved` (crown M1). An `::assert{A(x)}`
  where `B(x)` holds on every route to it is the new **`E-FACT-EXCLUSIVE`**,
  and a rule that derives `A` from `B` on the head's own arguments
  (`dark(X) :- lit(X)` with `dark` excluding `lit`) is the new
  **`E-RULE-EXCLUSIVE`**, reported at the rule (LH N17). Where both are
  only possible, `lute play` reports
  `✗ exclusive: fell(elias) and seenAfter(elias) both hold` at the write
  that made them hold — even when a later write of the same presentation
  undoes it (ER C3) — or, for an `engine:` write, at the step, and halts
  (exit 1); `lute trace` / `lute test` record the same `✗ exclusive` line at
  the write and refuse the walk there (`E-FACT-EXCLUSIVE`, exit 1). Seeded
  facts are checked before anything runs (LH N16): a trace `--fact` / mock
  or test `facts:` that already breaks an exclusion refuses the trace, and
  a play script's `facts:` halts the play before step 1. Derived relations
  are covered through their derivations. A relation in an exclusion is read
  by it — no `W-RELATION-UNREAD` (ER C4). The IR's
  `RelationEntry` carries `excludes` (the symmetric closure, sorted; absent
  when empty).
- **Presence after engine events** (dsl 0.25.0 §6): an engine-`reserved`
  relation may name the occasions on which the engine changes it —
  `fell: { args: [companion], reserved: true, changedOn: [battleEnd] }`.
  Cast `assume: true` then no longer reads it as unchanged in a unit
  presented on one of those occasions (a scene / entry / bundle beat `on`,
  a quest `<on event>` handler or `on=` objective body on it) nor, in
  `check-project`, in any after-descendant of such a unit over the scenario
  graph's `after:` / `after=` / `[start]` edges, nor under a guard (a unit's
  `when`, a choice / arm / handler / line `when`) that needs a fact of it —
  `holds(fell(isolde))`, `count(fell(_)) >= 1`, or a derived relation every
  rule of which needs one (ER C2): `W-CAST-ABSENT` reports the post-battle
  line again and says `assume: true` does not cover `fell` there. Without
  `changedOn` nothing changes. `changedOn` on a relation that
  is not `reserved: true`, or naming an undeclared occasion (with a
  did-you-mean), is `E-RELATION-DECL`.
- **Quest graph edges** (dsl 0.25.0 §4, ER F17 / N13): `lute scenario`
  draws `quest(parent) -> quest(child) [subquest]` for every nested quest,
  so a parent with children is no longer listed as unanchored, and a quest
  without `after=` is anchored by its top-level `start` conjuncts that read
  `visited('…')`, `entry.X.everRead` or `quest.Y.state == '…'` (not
  `unset`) — `[start]` edges; an `||` of such reads is one anchor with
  several sources. A `start`-driven quest no longer needs a copy of its
  `start` in `after=`. The sources join the graph (an entry `X` as the new
  node `entry(X)`); `reach` lists a quest's anchors (`anchors` in
  `--format json`). An explicit `after=` still replaces the `start` and
  `::accept` anchors; the subquest edge stays. Anchors never prove a quest
  unreachable, and one that would close a cycle is not drawn.
- **`<quest accept="external">`** (dsl 0.25.0 §5, LH N5): the quest is
  accepted outside the script (a quest board, a menu, a UI). It silences
  `W-QUEST-NEVER-ACCEPTED`; beside `start` it is `E-ATTR-TYPE`, and on a
  subquest child that activates with its parent it is `E-ACCEPT-TARGET`
  (declare `activate="accept"` too). IR: `QuestCmd.accept: "external"`.

### Changed

- Bridge answers (dsl 0.25.0 §7): a bridge result field no content reads may
  be left out of a `lute play` / `lute trace` / scenario-test / `mocks/*.yaml`
  `bridges:` answer, and every unanswered-call hint lists only the fields
  content reads. A field content reads is still required
  (`E-TRACE-MOCK-TYPE`). `lute play` no longer halts at a plugin call none of
  whose result slots content reads.
- `W-QUEST-NEVER-ACCEPTED` (dsl 0.25.0 §5, LH N5): a `*.test.yaml` or
  `mocks/*.yaml` `accepts:` mock no longer counts as an acceptance — a mock
  proves a test, not the game. A quest only a mock accepts still warns, the
  message names the mock file, and the hint offers `accept="external"`.
- `lute scenario knowledge` (dsl 0.25.0 §9, LH N6): a defeater lists every
  derivation route of the defeating fact (`⇐ … / ⇐ …`), not only the first.
- The `lute scenario` note on undrawn references now says a quest's edges
  come from its `after`, subquest tree, `start` conjuncts and `::accept`s.
  A quest's `visited()` read outside its top-level `start` conjuncts (in
  `fail`, or an objective's `done` / `when` / `by` / `until`) is noted as
  `reads visited('…') in its objective watch done — a condition read, not an
  anchor` and no longer suggests declaring `after`: copying such a read into
  `after=` replaced the quest's real anchor (its `::accept`) with a backwards
  edge (summer S1). `--format json` gives the read's `slot`.
- `W-LUTE-VERSION-STALE` for a stamp inherited from the manifest's
  `defaults: luteVersion` (LH N18): `check-project` reports it once, at the
  manifest's `luteVersion:` line, with the number of documents inheriting
  it, instead of once per document at `1:1`; a single-file `lute check`
  says the stamp comes from the manifest's `defaults:`.

### Fixed

- A `share` without a written `once` (or with `once` `false`) is reported
  once — the per-file `E-BEAT-ATTR` "without `once`" — and no longer also as
  a project-wide `E-BEAT-ATTR` claiming the beat declares a different
  `once: run` than its key (dsl 0.25.0 §2, summer S2). Such a beat joins
  no key.
- An undeclared `<match on>` subject is no longer also `E-NONEXHAUSTIVE`
  (dsl 0.25.0 §9, SU N8): its read is reported once — `E-UNDECLARED`, or,
  while a schema import is broken, the import error (a `clock:` that
  swallowed the state entry left only a misleading `E-NONEXHAUSTIVE`).
- `W-CAST-ABSENT` in `check-project` no longer counts an effects
  component's `::assert{rel(@param)}` as a producer of every member: a
  component's writes produce facts only at its `::use` sites, with the bound
  arguments (ER C1). An unused `joins` component no longer made a
  character's pre-recruitment lines warn.

### Compatibility

- **A test mock no longer accepts a quest for the checker.** A `*.test.yaml`
  or `mocks/*.yaml` `accepts:` entry used to silence
  `W-QUEST-NEVER-ACCEPTED`; it no longer does, since a mock proves a test, not
  the game. A quest the game accepts outside the script (a quest board, a
  menu, a UI) declares `<quest accept="external">`; the warning's message
  names the mock and its hint offers the attribute.
- **Exclusive violations halt play and trace.** Once a relation declares
  `excludes:`, a write that makes both sides hold stops `lute play` at the
  step (exit 1) and refuses the `lute trace` / `lute test` walk
  (`E-FACT-EXCLUSIVE`, exit 1); seeded facts that already break an exclusion
  refuse the run before it starts. Projects that declare no `excludes:` are
  unaffected.
- **`E-RULE-EXCLUSIVE` can redden a project.** A rule that derives a relation
  from one it excludes, on the head's own arguments (`dark(X) :- lit(X)`
  with `dark` excluding `lit`), is now an error at the rule. It only fires
  once an `excludes:` is declared that the rules contradict.
- **`W-LUTE-VERSION-STALE` for an inherited stamp is reported once.** A stale
  `luteVersion` inherited from the manifest's `defaults:` is reported by
  `check-project` once, at the manifest's `luteVersion:` line, with the
  number of documents inheriting it, instead of once per document at `1:1`.
  Tooling that counted or located these warnings per document sees one.
- **Bridge answers may be lighter, and fewer plays halt.** A `bridges:` answer
  may omit result fields no content reads (a field content reads is still
  required, `E-TRACE-MOCK-TYPE`), and `lute play` no longer halts at a plugin
  call none of whose result slots content reads — a play that used to stop
  there now continues.
- **Schema file renamed; additive IR.** The version strings move to `0.25.0`
  and `schemas/lute-ir-0.24.schema.json` is renamed to
  [`schemas/lute-ir-0.25.schema.json`](schemas/lute-ir-0.25.schema.json)
  (`$id` updated). Every new field is optional and appears only when the
  source uses the feature: `RelationEntry.excludes`, `share` on `BeatIr` /
  `EntryCmd` / `BeatCmd` / index beat rows, `BeatCmd.after` with its
  `prereqEdges` row, `QuestCmd.accept` (`"external"`), and the placeholder
  format `"ordinalWord"`. `lute.core` does not move, so neither does
  `capabilityVersion`. Engines gate on MAJOR, so nothing widens; the
  tree-sitter grammar is unchanged.
- **A large rustfmt-only reformat.** Commit `a1b6ae1` reformatted the Rust
  sources with `rustfmt`; it changes no behavior, but a downstream patch
  against the crates may need rebasing.

## [0.24.0] - 2026-09-25

**Clocks, quest structure, parties.**

A minor release from a third dogfood round over four games — a mystery, a
roguelike, a day-clock visual novel and a party RPG. A schema may declare a
clock that beats, `lute play` and `lute calendar` read; quests gain accepted
subquests, alternatives, place-bound deadlines and a reason they failed;
entities gain sub-kinds and per-member state; a cast may say when a speaker
is present; components may write state; and plugin calls are answered by
`bridges:` in play, test and trace. Every silent wrong answer that round found
is fixed here (there is no separate 0.23.2). The language and the IR both earn
the move (the IR additively); see
[`docs/proposals/scenario-dsl/0.24.0.md`](docs/proposals/scenario-dsl/0.24.0.md)
and [`docs/versioning.md`](docs/versioning.md).

### Added

- **`::clear`** (dsl 0.24.0 §4, T3-17): a core staging leaf, no attributes —
  every character on stage exits (those on stage on only some path to it
  too, as at a `::bg`), while the background and music stay. It compiles to
  one `sprite` `exit` record per character (provenance `by: stage-clear`),
  no record of its own and no new IR kind. A later line by a cleared
  character without a re-show is `W-STAGE-ABSENT` naming the `::clear`;
  `lute play` prints `::clear` where it ran and `lute trace` records it as
  an exit.
- **Interpolation format hint `{{x:ordinal}}`** (dsl 0.24.0 §4, T3-18):
  `{{user.deaths:ordinal}}` renders `1st` `2nd` `3rd` `4th` … `11th` `12th`
  `13th` … `21st` `22nd` … `101st` `111th`. The IR placeholder gains
  `format: "ordinal"` (omitted without a hint, so existing artifacts are
  byte-identical); `lute run`, `lute play`, `lute trace` and `lute test`
  render English ordinals, and a number with no ordinal (a fraction, a
  negative) renders unchanged. `ordinal` is the only hint (another is
  `E-CEL-PROFILE`), and it needs a number (`E-REF-TYPE` on a string, enum
  or bool path/def, or on `userName`).
- **`_` in rule bodies, and `countDistinct`** (dsl 0.24 T3-9). A `_` in a
  rule body atom is a fresh anonymous variable (`testified(W) :- seen(W, _)`);
  under `not` it is existential (`not seen(W, _)`: no such tuple at all). It
  stays an error in a rule head and in a comparison. The new CEL function
  `countDistinct(<pattern>, <Var>)` counts the distinct values of the pattern
  position `<Var>` names — `countDistinct(sawAt(W, _, _, _), W) >= 3` counts
  witnesses, where `count(sawAt(_, _, _, _))` counts tuples. It is decided by
  `check-project`'s fact envelope, evaluated by trace/test/play, and, like
  `count`, forbidden in a rule guard.
- **Entity sub-kinds and entity-indexed state** (dsl 0.24.0 §3, T2-6/T2-7).
  `entities: { companion: { subsetOf: person, members: [isolde, corvin] } }`
  declares a sub-kind: every member must also be a member of the parent
  (else `E-ENTITY-KIND-SHAPE` naming the outsiders; an undeclared or `open:`
  parent, an `open:` sub-kind or a `subsetOf:` loop is the same code), it is
  legal wherever a kind is, and sharing members with its parent (or a sibling
  sub-kind) is no longer `E-ENTITY-KIND-CLASH`. A state path declared
  `{ type: number, default: 0, per: companion }` declares `run.approval.<m>`
  for every member `m` of a closed kind declared in the same document (an
  `open:`, unknown or malformed kind is `E-STATE-DECL`). A rule `cel()`
  guard may read `run.approval[P]` for a variable `P` bound by a positive
  atom that ranges it over the index kind or a sub-kind (`E-FACT-DOMAIN`
  otherwise; unbound is `E-DATALOG-UNSAFE`, a non-indexed family
  `E-UNDECLARED`); anywhere else a member is named, and reading the family
  says so. Such a rule is compiled **grounded** — one IR rule per member,
  `raw` suffixed `[P = isolde]` — so IR guards stay CEL over ground terms and
  `lute trace`/`play`/`run` evaluate the same instances.
- **Cast presence and per-speaker emotions** (dsl 0.24.0 §4, T2-8/T3-17). A
  cast entry — a plugin `cast/*.yaml` export or a schema's `cast:` — may
  declare `present: "<condition>"` and `emotions: [...]`
  (`isolde: { name: Isolde, present: "holds(inParty(isolde))", emotions:
  [calm, fierce] }`). In a schema, a `present:` that does not parse is
  `E-CEL-PARSE` and one outside the CEL profile `E-CEL-PROFILE`, and an
  `emotions:` member outside the schema's own `emotion` enum is
  `E-BAD-ENUM`, each at the entry's key. A plugin cast without the new keys
  keeps its `capabilityVersion` stamp.
- **`W-CAST-ABSENT`** (new warning, dsl 0.24.0 §4): a line by a speaker
  whose cast declares `present:`, where the guards around the line do not
  imply it — its own `when=`, enclosing `<choice when>` (branch and hub),
  `<match>` arms (the arm's `is=`/`test`, and that no earlier arm matched),
  `<on when>`, `<objective done>`, and the scene beat's `when:` or the
  entry's / bundle beat's `when=`, `@def`s expanded. A guard stops counting
  once a `::set`, `::assert`/`::retract` or `::accept` that may change it
  runs between it and the line. `check-project` also counts facts that hold
  on every route to the line (asserted earlier on every path, or a seed
  nothing retracts), so `::assert{inParty(isolde)}` then `@isolde: …` is
  clean there; a single-file `check` cannot see those and says so. The
  message names the condition and the guard to add
  (`@isolde{when="holds(inParty(isolde))"}`). `--deny W-CAST-ABSENT` works.
- **`emotion=` outside the speaker's `emotions:` is `E-BAD-ENUM`** (dsl
  0.24.0 §4): on a content line by that speaker, or a directive whose
  literal `character=` names them. A value the `emotion` enum itself rejects
  keeps its one existing error.
- **Components that write state: `effects: true`** (dsl 0.24.0 §4, T2-9).
  A component file declaring `effects: true` may `::set` / `::assert` /
  `::retract` (and use a state-writing plugin directive or another effects
  component). Each write is checked at every `::use` against the host's
  schema — undeclared path, `::set` type, `owner: engine`, relation
  vocabulary and domain, permissions — and reported at the `::use`. The
  writes compile into the host's commands where the `::use` sits, and a
  param-scoped `<match>` keeps only the arm the argument selects. Fact
  analyses in `check-project` see them there too. Guards and match subjects
  in the body still may not read state (`E-COMPONENT-STATE`), and a
  presentational component may not `::use` an effects component
  (`E-COMPONENT-BODY`). Without `effects: true` a write is still
  `E-COMPONENT-BODY`.
- **`speaker` component params** (dsl 0.24.0 §4): `params: { who: speaker }`
  takes a cast id. `{{@who}}` renders the member's `name` (the id when it has
  none), while attributes and `<match on="@who">` see the id; the match
  ranges over the declared cast plus `narrator`, so `<when is="isolde">`
  arms can each `::set{run.approval.isolde += @delta}`. An id outside the
  cast is `E-CAST-UNKNOWN` with a did-you-mean. Without a declared cast any
  identifier is accepted. A def argument is `E-COMPONENT-ARG`: the name is
  chosen when the component expands.
- **`E-RELATION-RESERVED-NAME`** (dsl 0.24 T3-8): a relation named like a CEL
  call, macro or keyword (`has`, `holds`, `count`, `isSet`, `now`, …) is an
  error at its declaration; a query over one (`holds(has(lamp))`) now says
  why it cannot parse.
- **`W-RELATION-UNREAD` and `W-DEF-UNUSED`** (dsl 0.24.0 §7, T3-16):
  `check-project` advisories, once per project at the declaration (the
  schema file line, or the document's own frontmatter key). A declared,
  non-reserved relation that is asserted, seeded or derived but that no
  condition queries (`holds` / `count` / `countDistinct`), no rule body uses,
  and no def reads records facts that change nothing; a declared `@def` that
  no content, other def, or rule guard references is dead. Play scripts and
  tests are not reads.
- **`lute scenario knowledge` covers every guard slot** (dsl 0.24.0 T3-1).
  Line `when=`, `<choice when>`, `<when>` arm tests, `::next`/`::set`
  `when`, `<on when>`, reward `when`, quest `start`/`fail` and objective
  `until` join beat, entry and objective guards, grouped per document with
  their source line. `--for` takes a scene (every guard in it),
  `quest:<id>`, and `<scene>#<branch>.<choice>` for one choice (was: exit 2
  on a scene without a fact-guarded frontmatter `when`). Constants the
  positive premises force are propagated into a negated premise
  (`not liar(hollis)`, not `not liar(_)`); a negation reads "holds unless
  defeated" with one "defeated when X is asserted by …" line per defeater,
  or "always holds (nothing produces X …) — cannot be defeated" (was
  `NO PRODUCER`, which read as a warning). A derived atom's rules print once
  per report; later mentions say "traced above under …". JSON elements gain
  `for` (their handles) and `line`; a `!holds(X)` read is listed as `!X`.
- **Provenance in `--explain`, `lute lore` and the envelope** (T3-2). An
  asserted leaf names who asserted it and when (`asserted by entry
  `keeperLog2`, step 4`, `engine step 7`; JSON `assertedBy`), and an absent
  negated premise some rule could conclude is expanded one level (each rule
  with the premise that keeps it from firing; JSON `attempts`). `lute lore`
  shows each entry's and beat's `when` and, when a relation is derived, a
  Derived section: every conclusion the rules can reach, its rule instances,
  the evidence it rests on and the conditions it gates (JSON `derived`).
  `lute scenario envelope`'s Facts rows use the knowledge wording
  (`seed facts …; reserved — the engine asserts it; derived by 1 rule`;
  was `asserted by: facts: seed`, silent on `reserved`).

- **Bridge answers in play, test and trace** (dsl 0.24.0 §5, T2-10). A play
  script takes `bridges: { <tag>: [ {<field>: value}, … ] }` at the top level
  (consumed in call order across the play) and per step (consumed first by
  that step's calls; answers the step leaves unconsumed fail it). Each answer
  gives exactly the `bridgeResult` fields the call's effects read, typed by
  the result slot — an unknown tag, a stray or missing field, or a misfit
  value is a usage error (exit 2) at script load. Answers land in `scene.*`
  result slots (a `state:` seed of `scene.*` stays refused), and the
  transcript shows `(bridge answered: passed=true, margin=3)`. `*.test.yaml`
  and trace mocks take the same key (`E-TRACE-MOCK-UNDECLARED` /
  `E-TRACE-MOCK-TYPE`, also in `check-project`'s `mocks/*.yaml` pass), and
  `lute run --mock` answers calls from it (docs/runtime/bridge-protocol.md).

- **Enum member display labels** (dsl 0.24.0 §1, T2-1). A long-form enum
  (a schema's `enums:` or a plugin's `enums/*.yaml`) takes
  `labels: { <member>: <text> }`, and `{{path}}` of a state path typed
  against it (`run.wd: { type: { domain: weekday } }`) renders the label —
  `Today is Sunday.`, not `Today is sun.`; a member without a label renders
  its id. `lute play`/`lute run`, `lute trace` and `lute test` agree. The IR
  state entry gains `labels` (omitted when none is declared, so existing
  artifacts and `capabilityVersion` stamps are unchanged). A label for a
  non-member is the new `E-ENUM-LABEL-NOT-MEMBER`; a non-string label in a
  schema document is `E-META-VALUE`. A state path typed `{ domain: X }` now
  counts as reading `X`, so it no longer draws `W-DOMAIN-UNREAD`.

- **Integer `%` joins the CEL profile** (dsl 0.24.0 §1, T2-1). `run.day % 7
  == 0` and a def `wd: "run.day % 7"` (typed `number` by inference) are
  clean; both operands must be integers, so a non-number operand (`'a' % 2`,
  `run.flag % 2`) or a fractional literal (`run.day % 2.5`) is the new
  `E-CEL-TYPE`. `%` was `E-CEL-PROFILE`. The IR `expr` gains the binary op
  `%`. Trace, test, play and the condition decider evaluate it as the
  truncated integer remainder (`-7 % 3 == -1`); a fractional value or a zero
  divisor is unknown (docs/runtime/cel-and-facts.md).
- **`lute calendar` gains `visited()` axes, per-occasion axes, a presence
  grid and a never-presented list** (T3-13). `--axis
  "visited('rock.arrival')=true,false"` puts a scene or bundle-beat id in or
  out of the cell's visited set, so an `after: visited(…)` beat can be read
  both ways. `--occasion dayEnd@run.day` varies only `run.day` for `dayEnd`
  (evaluated where the other axes are at their first value, or at
  `@run.day,run.slot=night`; blank / absent elsewhere) — a once-a-day
  occasion no longer repeats in every slot row. `--facts at` (repeatable)
  prints who is where in every cell (a row per first argument, a column per
  cell; a per-cell `facts` map in `--json`, a `facts:at` column in `--csv`).
  The report adds the beats eligible in some cell but presented in none,
  with what was presented over them (`neverPresented` / `beatenBy` in
  `--json`, a second table in `--csv`).
- **A declared clock** (dsl 0.24.0 §1, T2-1). A schema MAY declare
  `clock: { day, slot, slots, raise?, week? { length, first, labels } }` over
  two `owner: engine` state paths; a malformed clock, an undeclared /
  mistyped / content-owned `day` or `slot`, `slots` that are not the slot
  enum's members, an unknown `raise` occasion, or a second clock is the new
  `E-CLOCK-DECL`. Content reads the reserved, read-only `clock.index`
  (`(day-1)*len(slots)+slotIndex`), `clock.weekday` and `clock.weekdayLabel`
  (with a `week:`) in any condition or interpolation; `::set` of a `clock.*`
  path is `E-QUEST-RESERVED-WRITE`. Scene and bundle beats take `once: day` /
  `once: slot`, entries `once="day"|"slot"` — spent until the clock's day /
  slot changes (`lute play` prints `once: day — already presented today`);
  without a clock they are `E-BEAT-ATTR`. The artifact and project index
  carry the declaration as `clock` (omitted without one); `lute run` / `lute
  play` / `lute trace` derive the `clock.*` values from the live day / slot.
- **`lute play` advances the clock: `advance:` and `include:` steps** (dsl
  0.24.0 §1, T2-1). `advance: slot`, `advance: <n>` or `advance: day` moves
  a declared clock forward in one step — writes its day / slot paths
  (wrapping slots into the next day), settles the quests (a `by` the new
  time passes fails there), then raises the clock's `raise` occasion as an
  `occasion:` step would, taking its `pick` / `choose` and selection
  `expect:` — the step's `presented:` lists every beat the step presented,
  each `dayEnd` / `dayStart` raise's then the final raise's, in order
  (summer R2, lighthouse N15), while `winner:`, `offered:` and `notOffered:`
  judge the final raise, where the clock stops. The transcript prints `advance slot: day 1 (Mon) night → day 2
  (Tue) morning`; `--json` carries `advance: { by, from, to, writes, quests
  }` beside the raised occasion's fields. An `advance:` step may carry the
  `engine:` writes of the same moment (`advance: day` + `engine: { state: {
  run.leg: 3 } }`): they land where the clock arrives — after every
  `dayEnd` / `dayStart` the advance raises on the way (that evening's
  `dayEnd` still reads the day it closes, ember R3), before the final
  settle and raise — and one settle follows both (a write to the clock's
  own day / slot path there is a usage error). An `occasion:` step raising
  the clock's own `dayEnd` / `dayStart` prints a note: the next `advance:`
  across that midnight raises it again (summer R1; `--json`: the step's
  `notes`).
  An `engine:` step that moves `clock.index` backward is a usage error (exit
  2). `- include: <file>`
  splices another file's steps in place (relative to the including file;
  a cycle is a usage error). `lute calendar --axis clock[=d1..d2]` expands
  to every slot of those days, in clock order (bare `clock`: one week).
- **`::set{… when="…"}` — a guarded write** (dsl 0.24.0 §1, T2-1).
  `::set{run.aff.wren += 1 when="run.warmed.wren < run.day"}` writes only
  when the condition holds, replacing the `<match>`/`<when>` ritual around a
  single write. The guard is checked like a line `when=` (bool, `$` out of
  scope, `E-ARM-DEAD` when provably false) and is never a definite
  assignment, so a later read of an undefaulted path stays `E-MAYBE-UNSET`.
  No IR change: it compiles to the same one-arm `match` a gated line does;
  `lute play` prints a skipped write as `skip set <path> … — when: false`.
  Inside a `<track>` a guarded `::set` is `E-TIMELINE-CONTENT`. Any other
  attribute in a `::set` body is still expression text, and its
  `E-CEL-PARSE` now names `when=` as the one attribute there is.
- **Schemas and editor support cover 0.24.** `schemas/lute-ir-0.24.schema.json`
  documents every additive 0.24 IR field: the artifact's `clock`
  (`$defs/clockDecl`, the same shape as `project.index.json`'s `clock`),
  `once: "day"|"slot"` on scene beats, entries, bundle beats and index beat
  rows, `ObjectiveEntry.until`, `QuestCmd.activate`/`complete`,
  `OnCmd.target`, `AcceptCmd.applies`, placeholder `format`,
  `StateEntry.labels` and CEL `%`; the artifacts `lute compile` emits for them
  validate against it. `schemas/lute.schema.json` accepts a schema's `clock:`
  and a cast entry's `present`/`emotions`. The LSP offers and documents
  `<objective until>`, `<quest activate complete>`, `<on target>`,
  `::accept{at}`, and `once` `day`/`slot` on entries and bundle beats.
- **Subquests taken up in dialogue: `<quest activate="accept">`** (dsl
  0.24.0 §2, round 3 F6/F7). A child quest declaring `activate="accept"` does
  not activate with its parent: it waits for an `::accept` (or an `accepts:`
  mock) and activates only while its parent is `active` — an accept while
  the parent is not active is spent without effect, and says so: `lute
  trace` / `lute test` note `accept of `c` spent: its parent quest `p` is
  never active …` for an `accepts:` mock, and `lute play` prints `note:
  accept of quest c spent — its parent quest p is not active yet …` (`--json`:
  an `acceptSpent` record) (ember N15). `activate="accept"`
  beside `start` is `E-ATTR-TYPE`, and `::accept` of a child that activates
  with its parent is now `E-ACCEPT-TARGET` naming the parent (it was a silent
  no-op). `lute trace --accept` takes such a child. IR: `QuestCmd.activate`
  (omitted by default).
- **Alternatives: `<quest complete="any">`** (dsl 0.24.0 §2, F6). The quest
  completes when any one required objective — a child quest or a plain
  objective — is done; its other still-`active` children then fail with
  `failedBy: superseded` (a child nobody accepted stays `unset`), so the
  author no longer copies the cascade into every sibling's `fail=`. Its
  synthesized `fail` is the conjunction over every required objective (a
  child's `quest.<c>.state == 'failed'`, any other's
  `quest.<q>.objectives.<o>.failed`): one failed alternative leaves the
  quest open. Default `complete="all"`. IR: `QuestCmd.complete` (omitted by
  default).
- **Why a quest failed: `quest.<id>.failedBy` and
  `quest.<id>.objectives.<o>.failed`** (dsl 0.24.0 §2, F20). Reserved
  read-only paths: `failedBy` reads `unset` until the quest fails, then
  `fail`, `by`, `until`, `cascade` or `superseded`; `failed` reads `true`
  once the objective's `by`/`until` failed it. Any condition or
  interpolation may read them (an epilogue that tells a superseded
  alternative from a missed deadline); writing is `E-QUEST-RESERVED-WRITE`,
  declaring `E-QUEST-RESERVED-DECL`. `lute run`/`play`/`trace`/`test` agree,
  a run-tier quest's `newRun` reset clears both, and `lute run --json`'s
  failing `quest` record carries `failedBy`. The paths are typed by shape and
  never enter the IR `state` table.
- **`<on event="E" target="…">`** (dsl 0.24.0 §2): the handler runs only when
  the same-named occasion is raised for that target — never for a plain
  `event:` step, another target or a lifecycle transition (`lute play` /
  `lute run`, and `lute trace` / `lute test` `occasions: [E@target]`). The
  target is checked like an `<objective on target>`'s (`E-BEAT-ATTR`: a
  quoted dotted id, a targeted occasion named `E`, inside its domain; never
  on a lifecycle event). IR: `OnCmd.target`.
- **Occasion `judge: before`** (dsl 0.24.0 §2, F20). An occasion declared
  `judge: before` judges its `on=` objectives and settles the quests before
  its beats are decided and presented, so an epilogue on it reads how its
  quests ended. Only the judging moves: the `<on>` handler bodies the raise
  answers — the same-named `<on event>` handlers and the `questComplete` /
  `questFailed` handlers of the quests it settles — run after the beats, so
  their narration follows the scene (each handler's `when` is decided where
  it fired). `lute play` prints those quest transitions right under the step
  header, before the candidates, and `--json` gives them as the step's
  `judgedBefore` (the step's `quests` are the ones after, with the handler
  bodies). The default `after` keeps the 0.21 order and every existing
  `capabilityVersion` stamp.
- **`::accept{quest="…" at="nextRun"}`** (dsl 0.24.0 §2): the acceptance is
  queued and applies right after the next `newRun` reset, so a run-tier
  quest taken at a hub between runs survives the reset and activates in the
  new run's first settle. `lute play` prints `quest X accepted (queued:
  applies after the next newRun)` where it ran and `quest X accepted (queued
  at="nextRun")` under the new run (`--json`: the `newRun` step's
  `accepted`); `lute trace` marks the accept step `nextRun: true`. Any other
  `at` is `E-ACCEPT-TARGET`. IR: `AcceptCmd.applies`.
- **`W-QUEST-NEVER-ACCEPTED`** (new warning, dsl 0.24.0 §2, F8):
  `check-project` flags an accept-driven quest — no `start`, a root or an
  `activate="accept"` child — that no `::accept` in the project names and no
  `accepts:` mock (`mocks/*.yaml`, `*.test.yaml`) reaches. `--deny` works.
- **Bundle beats as `after:` predecessors, and accept anchors** (dsl 0.24.0
  §2, T2-13). `after: visited('<doc>.<beat>')` on a scene or quest resolves a
  bundle beat (it was `E-CONN-UNKNOWN-NODE` "is a bundle beat, not a scene").
  An accept-driven quest without `after=` is anchored at every scene, bundle
  beat and quest body that `::accept`s it: `lute scenario` draws an `accept`
  edge, and the quest leaves the unanchored list. An unresolvable quest
  `after=` now suggests dropping it rather than using `when`.
- **`W-DEADLINE-BEFORE-DONE`** (new warning, dsl 0.24.0 §2.1, LH N2): an
  `on=` objective with `by=` and no `until=` whose `done` provably implies
  `by` (`done="run.v == 'fell'" by="run.v != 'undecided'"`). `by` is judged
  at every settle and `done` only at the occasion's raise, so `by` fails the
  objective at the settle it comes true, before `done` is ever judged
  (unless both happen in the step that raises the occasion) — content
  written for 0.23.1's raise-only `by` can never complete. Anchored at `by=`,
  the message suggests `until="…"` with the same condition. Only a proven
  implication warns; `check` and `check-project` both report it, and
  `--deny` works.
- **Per-member defaults on a `per:` family** (dsl 0.24.0 §3, ER N1):
  `run.rep: { type: number, default: { _: 0, company: 1, dusk: -1 }, per: faction }`
  gives each member its own default; `_` is the fallback for members the map
  does not name. A key that is no member, a value that is not a scalar of the
  path's type, a member with neither its own value nor `_`, a map `default:`
  on a path without `per:`, and a list `default:` are `E-STATE-DECL` (a map
  default was silently accepted and yielded no default at all). `check`,
  `trace`, `run`, and `play` read the per-member values.
- **Component params in fact atoms** (dsl 0.24.0 §4, CR N5): an
  `effects: true` component may write `::assert{gifted(@who, @item)}` /
  `::retract{…}`. Each `::use` binds the params to its arguments and the host
  checks the bound atom there (membership, arity, `E-ARM-DEAD` routes); an
  argument that is not a constant (an entity/enum member id, `true`, `false`)
  is `E-COMPONENT-ARG` naming the atom. A `@param` in a fact outside a
  component body is `E-FACT-DOMAIN` (it was `E-DATALOG-PARSE`).

- **Day-granular clocks and day-boundary raises** (dsl 0.24.0 §1, round-3
  LH N7, SU N7). `clock:` may omit `slot:` / `slots:` (declared together):
  the clock counts whole days, a position reads `day 3`, `clock.index` is
  `day - 1`. `raise:` may be a map `{ slot, dayStart, dayEnd }` (a scalar is
  still the `slot` occasion): an advance raises `dayEnd` at every midnight
  it crosses, at the day's last slot with the day not yet advanced
  (`advance: <n>` walks there — it never skips a day's close; `advance: day`
  closes the day where the clock stands), then `dayStart` at the next day's
  first slot, then `slot` once where it stops. The transcript shows each as
  `── step N · day 1 (Mon) night · dayEnd`; `--json` lists them under
  `advance.days`.
- `lute calendar --occasion O@clock.day[,clock.slot=<slot>]` (or the
  clock's own paths, `O@run.day,run.slot=night`) with `--axis clock`
  evaluates `O` once per day at one slot (round-3 SU N3).
- `lute doctor` compares the `lute-lsp` beside the running `lute` with the
  one on `PATH` (`lute-lsp beside lute`): another build first on `PATH` —
  same version or not — fails with the fix (round-3 SU N10).
- **Cast `assume: true`** (dsl 0.24.0 §4, round-3 ER N5): beside `present:`,
  presence reads every negated `holds` of an engine-`reserved` relation in
  `present` (directly, or through a rule such as `inParty(P) :- …, not
  fell(P)`) as true. Without it a `present` over a reserved relation is
  satisfied only by a guard at each line or beat, since the checker does not
  know when the engine writes the fact; with it, a line after the engine
  event needs its own guard.

### Changed

- **An entry's `target=` on an untargeted occasion is metadata** (dsl 0.24.0
  §6, T2-12). `<entry on="keepsakes" target="item.compass">` on an occasion
  declared without `target:` is no longer `E-BEAT-ATTR`: the target says what
  the entry is about, and the entry answers every raise of the occasion
  (`lute play`, `lute calendar`, `lute beats` and the beat advisories treat it
  as untargeted; docs/runtime/beats-and-occasions.md states the engine rule).
  A scene's or bundle beat's `target` there is still `E-BEAT-ATTR`.
- **`by=` is judged at every settle, `on=` or not** (dsl 0.24.0 §2.1, T2-4).
  0.23.1 judged an `on=` objective's deadline only when its occasion was
  raised, so a player who never went back escaped it; now the settle after
  the deadline comes true fails the objective wherever the player is (`done`
  still wins a tie in the same settle — and in the step that raises an
  `on=` objective's occasion, whose presentations and settles judge that
  objective's `by` only after the raise judged its `done`, so a hearing that
  files the right verdict completes it). The old place-bound rule is the new
  `until="…"`: judged only when the objective's occasion (and `target`) is
  raised, after `done`. `until` without `on` is `E-BEAT-ATTR`. The IR's
  `ObjectiveEntry` gains `until` (omitted when absent). `lute run` / `lute
  play` print `<quest>.<objective> failed (until)` for an `until` failure
  (was always `failed (by)`), and the objective record carries `"failedBy":
  "by" | "until"`.

- **`lute trace` and `lute beats` print a def reference as the author wrote
  it** (T3-12). A `<match on="@weekday">` header reads `<match @weekday>`
  (was `<match (run.day == 1 ? 'mon' : …)>`), an arm or choice guard
  `(@atLeast(3))`, the coverage summary `arms 1/2 (@weekday @12:1)`, and a
  beat's `when` column `@runsAtLeast(2) && @firstDay`. The new `--expand` flag
  on both prints the expansion instead. `--json` keeps the expansion in
  `id`/`guard`/`label`/`when` and adds the author's text as `authoredId`/
  `authoredGuard`/`authoredLabel` (trace, only when an expansion changed it)
  and `whenAuthored` (beats).
- **An atomic `@def` argument is substituted without parentheses** (T3-12):
  `@atLeast(2)` over `user.runs >= n` expands to `(user.runs >= 2)`, not
  `(user.runs >= (2))`. A path, number, plain string literal or already
  parenthesized group splices bare; anything else (`a + 1`, `-2`, a call)
  keeps its parentheses. The compiled `expr` is unchanged; the IR `raw` text
  of a def called with such an argument is shorter.

- **`lute init --template investigation` is rebuilt on the `beats` skeleton**
  (T3-14). The old template still wrote dsl-0.3 `character:`/`season:`/
  `episode:` frontmatter with no `defaults:`, plugin, plays, tests or negation.
  It is now a small whodunit: a `case.occasions` plugin raising `arrive`, the
  targeted `examine` (`item.<evidence>`) and `interview` (`npc.<suspect>`) and
  `accuse`; evidence lore that `::assert`s what is on record; derived rules
  with stratified negation (a suspect is `cleared` by an alibi unless evidence
  contradicts it, the `culprit` is implicated and not cleared); an
  accept-driven quest; an accusation whose choices are guarded by
  `holds(culprit(…))`; `plays/the-case.play.yaml` with `expect:` and a scenario
  test. `check-project`, `test` and `play` pass as scaffolded, and its README
  shares the `beats` commands.
- **`lute new --dir` inside a project but not at its root is refused** (exit
  2, nothing written) with the spelling to use instead: `` `--dir` names the
  project; did you mean `lute new scene talk/tavi-shell`? `` (plus `--dir
  <root>` when the root is not the current directory). It used to write to
  `<root>/scenes/<name>.lute` without a word.
- **A dotted `lute new` name keeps its dots as the id**: `lute new scene
  isolde.night` writes `scenes/isolde.night.lute` with `id: isolde.night`
  (was `isoldeNight`); quest and lore document ids follow (`quest.a.b`). `-`
  still camel-cases within a segment.
- **`lute new quest` scaffolds an accept-driven stub** (no `start`, with a
  comment naming the `::accept{quest="…"}` that begins it); the new `--start`
  flag restores the auto-starting `start="true"` form.
- **A plugin directive's `lower:` is optional** (T3-7): absent means the
  generic `kind: "plugin"` passthrough, as documented — it used to be
  `E-PLUGIN-PARSE missing field lower`. A `{ kind: builtin, name: X }` must
  name a hook the core registers (`autoStage`, `cameraTransform`,
  `clearStage`, `end`, `mark`, `next`); any other name is `E-PLUGIN-PARSE`
  with a did-you-mean and
  "omit `lower:` for the generic passthrough" (it used to lower as a silent
  passthrough). The shipped examples and the plugin bridge guide that named
  unregistered hooks (`bridgeMinigame`, `mgart`, `bridgeServe`,
  `bridgeHostPanel`) drop `lower:` — a bridge directive binds its call with
  `bridge: { service, operation }` alone; their compiled records are
  unchanged (their capability stamp moves).
- **Plugin export files reject unknown keys** (T1-3). Every declaration a
  plugin exports (directives and their attrs/state/effects/bridge, state
  shapes and templates, providers, bridge capabilities, defs, enums, events,
  frontmatter, asset kinds, stamp attrs, reward kinds, occasions, lints) is
  `E-PLUGIN-PARSE` on a key it does not know, with a did-you-mean:
  `report: { selct: all }` says ``unknown field `selct` … did you mean
  `select`?`` instead of loading as `select: first`. When the mapping holds a
  key with no value — an unquoted flow-map description split at its comma
  (`{ description: Pick one, the player picks one }`) — the error says to
  quote it and spells the quoted form. A `state/` file may now hold both
  `stateShapes:` and `stateTemplates:` (the second used to be dropped).
- **`capabilityVersion` moves for every document**: `lute.core` gains
  `::clear`, so the core capability stamp changes. The conformance fixtures
  are re-recorded (only the stamp moves in `artifact.json`; `quest-subquest`'s
  transcript gains `failedBy` on each failing `quest` record and the
  `quest.<id>.failedBy` state paths, and the fixture table lists it).
- **`lute play` and `lute run` name why a quest failed**: `quest X -> failed (by)`,
  `(until)`, `(fail)`, `(cascade)` or `(superseded)` (dsl 0.24.0 §2; was
  `quest X -> failed`). `lute trace`'s decision for a quest failed from above
  reads `superseded from quest.P` for a `complete="any"` parent's untaken
  alternative (still `cascade from quest.P` otherwise).
- **A bridge result is answered by `bridges:`, not a `scene.*` state seed**
  (dsl 0.24.0 §5, ember N8). Migration: a 0.23.1 `*.test.yaml` or trace mock
  that seeded a plugin call's result slot — `state: { scene.check.guards.passed:
  true }` — no longer decides the guard; the call's answer is read from
  `bridges:` only, so the walk stops unresolved with a `bridges:` hint.
  Replace the seed with `bridges: { check: [ { passed: true, margin: 3 } ] }`
  (one answer per call, in call order, every field the result shape
  requires). `lute play` refuses a `scene.*` seed outright (exit 2).

- `advance: day` is documented to land on the next day's first slot from
  any slot, and a clock's `slot` raise fires once per advance, never at the
  slots it passes (round-3 LH N7).

### Fixed

- **A component can index a `per:` family by its param** (ER N6).
  `::set{run.approval[@who] += 1}` (and a `run.approval[@who]` read in the
  component's CEL) binds to `run.approval.isolde` at each `::use`; an
  argument that is not a member of the family's kind is `E-COMPONENT-ARG` at
  the `::use`, naming the kind's members. It was `E-UNDECLARED` plus a
  misleading `E-CEL-PARSE`.
- **A plugin directive lowered by a builtin hook runs that builtin.**
  `lower: { kind: builtin, name: clearStage }` compiled to a `kind: plugin`
  passthrough and cleared nothing; `compile` and `trace` now treat the
  directive as the core directive the hook belongs to (`clearStage` →
  `::clear`, `autoStage` → `::auto`, `cameraTransform` → `::camera`, `end`,
  `mark`, `next`).
- **`::clear` rejects timing attributes.** `::clear{duration="0.5"
  wait="true"}` checked clean and the keys were dropped; `duration`, `delay`
  and `wait` are now `E-UNKNOWN-ATTR` on `::clear`, which takes no attributes.
- **A cast `present:` on an undeclared path is `E-UNDECLARED`** (plugin or
  schema cast), reported at the member's line in each document that does not
  declare the path, instead of a `W-CAST-ABSENT` suggesting that same
  undeclared guard.
- **Unresolved components get a did-you-mean** (CR F5). A `components:`
  import that does not resolve names the project's component file with that
  name (or the nearest one), spelled from the importing document
  (`did you mean ../../components/gauge.component.lute?`), and says a
  document's own `components:` resolves against its directory while
  `defaults: components:` resolves against lute.project.yaml's. A `::use` of
  an undeclared component names the nearest declared one.
- **`lute new` from a subdirectory no longer blames `--dir`** (CR N7). With
  no `--dir`, the refusal says the project was taken from the current
  directory, names it, and says to run from the project root or pass
  `--dir <root>`. With `--dir`, it names the directory it resolved to.
- **Exhaustiveness honors a beat's `when:`** (LH N1). `E-NONEXHAUSTIVE` now
  reads the same narrowed domain `E-ARM-DEAD` and `W-OTHERWISE-DEAD` do: under
  `when: "run.verdict != 'undecided'"` a `<match on="run.verdict">` without an
  `undecided` arm is exhaustive, so deleting the dead arm is clean. A body
  that writes the subject keeps the whole domain. `W-OTHERWISE-DEAD` names
  "the domain left by the body's `when` guard" when the guard did the
  covering.
- **`clock.weekday` and `clock.weekdayLabel` are typed** (SU N1, dsl 0.24.0
  §1). `clock.weekday` is the whole numbers `0..length-1`: `is="0"` …
  `is="6"` (or ranges) is exhaustive with no `<otherwise>`, a gap is named
  (`` `6` is not covered ``), `is="7"` is `E-WHEN-LITERAL-DOMAIN`, and
  `clock.weekday == 7` is a dead guard. `clock.weekdayLabel` is the enum of
  `week.labels`, so label arms are checked like any enum (`is="Sundy"`,
  `== 'Sundy'`).
- **`W-DOMAIN-UNREAD` sees kind reads and lands on the schema** (CR N1, ER
  N10). A kind used as a `per:` index, a sub-kind's `subsetOf:` parent, and a
  kind atom in a rule body or a `holds(…)` condition now count as reads. The
  warning is reported at the declaration's line in the schema that declares
  it (or the document's own `entities:` / `enums:` key), not at `1:1` of the
  first importer.
- **An entity kind in a rule body now derives at runtime.** `inParty(P) :-
  companion(P), …` with `companion` an entity kind (dsl 0.3.0 §3.1's unary
  domain predicate) passed `check` but derived nothing in `lute run`/`play`/
  `trace`/`test` — the evaluator looked for `companion(…)` facts, and none are
  ever asserted under a kind's name. A kind atom is now a membership test over
  the kind's members (IR `entities`), in the join, under `not`, and in
  `--explain` (`companion(isolde) — entity kind `companion``);
  docs/runtime/cel-and-facts.md states the rule for engines.
- **A `@def` in a rule `cel()` guard works** (T1-1). `litA(lamp) :-
  cel("@firstDay")` passed `check` and then never derived: the guard was
  evaluated unexpanded, read undecided, and the rule was silently dropped.
  Rule guards now expand defs (with arguments) against the project's def
  table, so check, trace, test and play all read the expanded body; the
  expanded body still passes the rule-guard firewall. An undefined def, a
  wrong argument count or `$` in a rule guard is the new
  `E-RULE-GUARD-DEF`. A rule guard that is still undecided at play time no
  longer reads as "no such fact": a guard querying that relation is unknown
  and play halts there, naming what would decide it.
- **A maybe-unset read through a `@def` is reported** (T1-4). A def is
  checked where it is used: `{{@lastF}}`, `when="@lastF > 30"`, a
  `::use` argument `fathoms=@lastF`, a `::set` right-hand side and a
  `<match on="@def">` subject all read what the def body reads, so
  `lastF: "prev.run.depth * 10"` used unguarded is `E-MAYBE-UNSET` at the use,
  naming the path and the def. A guard inside a def body counts too.
- **Guards narrow consistently** (T1-7). A scene beat's `when:`, a bundle
  `<beat when>` and an entry `when=` hold throughout their body: its
  `isSet(…)` guards prove the body's reads and `<match>` subjects (no
  `E-MAYBE-UNSET` / `E-UNSET-UNCOVERED`), and a `<match>` arm whose every
  value the guard rules out is `E-ARM-DEAD` (`when: "run.outcome ==
  'diving'"` makes `<when is="died">` dead unless the body writes
  `run.outcome`). Inside one expression a presence guard proves the reads its
  short-circuit protects: `run.a == 1 || (isSet(run.o) && run.o == 'won')`,
  `isSet(run.o) ? run.o == 'won' : false` and `!isSet(run.o) || run.o ==
  'won'` are clean — the proof never reaches the guarded body. And
  `prev.run.*` is one snapshot: once any `prev.run.<p>` is known present,
  every `prev.run.<q>` whose `run.<q>` has a `default` is too.
- **`<match on="@def">` has a domain** (T1-5). A def whose body is one state
  path (`wd2: "run.wd"`) matches like that path; any other def takes its
  declared or inferred type (`type: { enum: [a, b, c] }` → its members). An
  exhaustive def match is no longer `E-NONEXHAUSTIVE`, and a typo arm is
  `E-WHEN-LITERAL-DOMAIN`. `E-NONEXHAUSTIVE` now names the uncovered members
  (`` `b`, `c` are not covered ``).
- **Schema problems are reported once, at the schema's line** (T3-6). A
  diagnostic about an imported declaration (`W-DERIVE-NO-RULES`,
  `E-ENTITY-KIND-CLASH`, a rule's `E-DATALOG-UNSAFE`, …) used to repeat at
  `1:1` — or at an unrelated line — of every importing document. It now
  names the schema file, carries the declaration's own line, and
  `check-project` folds the importers' copies into one (`+N more callers`).
  A relation whose only rule failed to parse no longer also draws
  `W-DERIVE-NO-RULES` or emptiness verdicts (`E-OBJECTIVE-UNSATISFIABLE`, a
  dead guard) — the parse error is the one report.
- **`W-BEAT-SHADOWED` catches an untargeted beat beaten on every target**
  (T1-8): on an occasion whose target domain is closed, an untargeted beat
  that, at every `<prefix>.<member>`, loses to an earlier always-eligible,
  never-spent beat for that member is reported, naming the shadower per
  target.
- **`W-BEAT-PRIORITY-TIE` no longer fires on beats `once` or a rule schedule
  keeps apart** (T3-3): a beat's `once` joins its eligibility (an entry's
  `once="user"` needs `!entry.<id>.everRead`, `once="run"`
  `!entry.<id>.read`; a scene's or bundle beat's `once: user`
  `!visited('<id>')`), so `tName once="user"` and `tDeath … &&
  entry.tName.everRead` are exclusive; and `holds(at(sol, radio))` of a
  derived atom whose rules are all ground `cel()`-only schedules stands for
  those guards, so beats on two schedule slots no longer tie.
- **`W-BEAT-ONCE-RUN-USER` sees user-tier relations and quests** (T3-4): a
  defaulted `once: run` beat gated only on `holds` / `count` of a `tier:
  user` relation, or on `quest.<id>.*` of a user-tier quest, now warns like
  one gated on `user.*`.
- **A scene beat's frontmatter `when:` has its quest and entry ids checked**
  (T1-6): `when: "quest.lampOot.state == 'active'"` is
  `W-QUEST-REF-UNKNOWN` (and `entry.tomasOyl.everRead` `W-ENTRY-REF-UNKNOWN`)
  anchored at the id in the frontmatter; both codes add a did-you-mean.
- **A component `{{@param}}` bound to a def whose name is a different length
  renders whole in `lute trace` and `lute test`** (T1-12). `::use{component=
  "gauge" fathoms=@bondTimesTen}` traced as `The gauge shows 20Ten}}
  fathoms` (the rebound interpolation kept the param token's span), so a
  `transcriptContains` that `lute play` passed failed in `lute test`. Every
  interpolation of a bound component line now carries its span in the
  rewritten text.

- **An unanswered bridge no longer walks the default arm** (T1-14). `lute
  play` walked a `<match>` over a plugin call's result slot to its default
  arm (printing it) and only then halted; it now halts AT the call (exit 3),
  naming the tag and the `bridges:` answer to give. `lute trace`/`lute test`
  read the state-shape default (`passed: false`) and completed at exit 0; an
  unmocked result slot now reads unknown, so a guard over it halts the trace
  incomplete with a `bridges: { check: [ { passed: <bool>, margin: <number> } ] }` hint.

- **A plugin that failed to load is no longer reported as not installed**
  (T3-7). When a package's `plugin.yaml` parsed but an export did not, the
  follow-up `E-PLUGIN-MISSING-ACTIVE` says ``plugin `x` … failed to load (see
  E-PLUGIN-PARSE above)`` rather than telling the author to install it.
- **`::auto{character}` and `::camera{focus}` are checked against the cast**
  (dsl 0.24.0 §4, T1-10). With a cast declared, a staged character outside it
  is `E-CAST-UNKNOWN` with a did-you-mean, like a speaker — timeline clips and
  match/choice bodies included. `::auto{character="marra"}` used to pass while
  `@marra:` did not.
- **XML character references in attribute values are decoded** (T1-11).
  `label="&quot;Quoted&quot;"` used to ship the entity text literally; a
  quoted value now decodes `&quot;` `&apos;` `&amp;` `&lt;` `&gt;` and
  `&#NN;` / `&#xHH;` (one pass, so `&amp;quot;` is `&quot;`). Any other `&`
  — a bare `&`, `&&`, `&nbsp;` — stays literal. `\"` still works.
- **A guard that assumes an entry was read knows what the entry asserted**
  (dsl 0.24.0 §6, T2-14). Under `entry.X.read` (or `== true`) the fact
  analysis adds the facts X's body asserts on every route (and nothing
  retracts) to the must set, so a redundant `holds(…)` under it is
  `W-FACT-GUARANTEED` and its negation a dead guard. `entry.X.everRead` adds
  only the `tier: user` / `tier: app` ones, which a new run keeps.
- **`W-QUEST-HANDLER-DEAD` also names a handler that can only fire on a
  completed quest** (T3-11). An `<on event="E" when="G">` whose `G` contains
  every required objective's `done` (any one under `complete="any"`), after
  `@def` expansion, never runs: the quest settles complete as soon as `G`
  holds, and an event reaches only an active quest's handlers. `lute check`
  reports it too; the lifecycle events and quests with an `on=` or `quest=`
  objective are exempt.
- **An unsupported `lute calendar --axis` lists the axis kinds** (T3-13): a
  declared state path, `quest.<id>.state`, `quest.<id>.objectives.<oid>.done`,
  `holds(<fact>)`, `visited('<id>')` — with a did-you-mean for a mistyped
  state path. A derived atom reads once: `` `trusted(ada)` is derived by rules
  and cannot be asserted `` (was `` `trusted(ada)` `trusted` is derived… ``;
  play's `facts:` / `engine:` seeds say it the same way).
- **`transcriptContains` / `transcriptLacks` match presented lines only, in
  one form** (T1-2). `lute play` and `lute test` both match against the
  content lines that actually played, one `@speaker: text` per line — no
  step headers, candidates, staging, notes or `skip @x "…" — when: false`
  lines (a guarded line that never played used to satisfy
  `transcriptContains`). A scene test used to match the trace's `@x  text`
  rendering and a play `@x: text`; `"@narrator: Always shown."` now works in
  both.
- **`lute test`'s `offered:` sees a hub's options** (T1-13): every choice
  eligible at each visit, unioned, like a branch's (was always `[]`).
- **`eligible:` in `lute test`** (T3-5): an `eligible:` expectation silences
  the "not eligible under these mocks" note it answers (the note now names
  the map form, `eligible: { <id>: false }`); a map key naming an entry or
  bundle beat of the file the test did not present is judged alone under the
  same mocks, and a lore test may carry a map-form `eligible:` without
  presenting anything. New `expect.accepts: [quest ids]` asserts the quests
  a scene's `::accept` took (as a set).
- **Play harness details** (T3-10, T3-11): a step `expect.options: {
  <branch or hub>: [ids] }` asserts the options offered in that step (as a
  set); a `select: all` occasion with nothing eligible needs no `pick:` (it
  passes as `pick: none`; a non-empty list without one halts naming the
  offered beats); a `newRun` prints each `prev.run.*` value it snapshotted
  (`--json`: `prevRun`) and a note for an accept-driven run-tier quest it
  resets while active with no objective done or failed (`resetUnjudged`; a
  `start=` quest, which `at="nextRun"` cannot take, gets none); an atom YAML split at
  its comma (`facts: [heard(tavi, regent)]`) says to quote it; an `<on
  event>` handler of a quest that already settled prints `<on event=E> of
  quest Q skipped — quest complete` instead of vanishing.
- **`lute trace --project` settles quest existence** (T3-15): a foreign
  quest the project declares no longer gets "existence is unverified"; one
  it does not declare says so, with a did-you-mean.
- **`lute test --coverage` counts what an `advance:` step presented**
  (lighthouse N3). The occasion a clock's `raise:` fires after an `advance:`
  step presents beats exactly as an `occasion:` step does, but coverage read
  only `occasion:` steps, so a scene a play reached through `advance: day`
  was listed as an untested document. The header now says what a play feeds:
  `coverage over N traced path(s) and M play(s) (plays count toward
  documents presented only, not branches or arms):` — the branch/hub and arm
  rows come from traced paths alone.
- **The unanswered-bridge hint is an answer the loader accepts** (ember-road
  N7). `lute trace`/`lute test` hinted `bridges: { check: [ { passed: <value>
  } ] }`, and supplying exactly that was refused for lacking `margin`. The
  hint now lists every field the call's result shape requires, each with a
  type placeholder — `{ passed: <bool>, margin: <number> }` (an enum is
  `<one of: a|b>`) — and `lute play`'s halt spells its answer the same way,
  as does its load-time refusal of a `bridges:` answer missing a field.
  A `bridges:` error in a `*.test.yaml`, a `trace --mock` file or a
  `mocks/*.yaml` is anchored at the offending tag key, answer or field key in
  that file (`tests/t.test.yaml:6:21`), not at `<document>:0:0`; the JSON
  diagnostic carries `"provenance": "mock"`. An answer missing a field is now
  `E-TRACE-MOCK-TYPE` (was `E-TRACE-MOCK-UNDECLARED`), and its message shows
  the whole typed answer.
- **`lute test --coverage` names a `<match>` over a `@def` as authored**
  (ember N14): ``match `@wrenWithUs` (…)``, as `lute trace` prints it, not
  the def's expansion ``match `(holds(inParty(wren)))` ``.

- `lute scenario knowledge` reads an entity-kind atom in a rule body as a
  membership premise (`suitor(sol) — entity kind suitor; sol is a
  member`), not an "undeclared relation", and lists the rule's `cel()`
  premise with the member it reads (`cel("run.aff.sol >= 3") — state
  condition on run.aff.sol, decided at run time`); `--format json` marks
  kind premises `entityKind` and lists `cel` premises (round-3 CR N2, SU N6,
  ER N9).
- `lute beats` lists bundle beats with `once="day"` / `once="slot"`
  (round-3 SU N2).
- A `clock:` whose `day` / `slot` path is undeclared, mistyped or not
  `owner: engine`, or whose `raise` names an undeclared occasion, is
  `E-CLOCK-DECL` from `lute check <schema>.yaml` at the `clock:` line (it
  said `ok`; the occasions are the enclosing project's, when there is one).
  `check-project` reports it once, attributed to the schema's `clock:`
  line, instead of at `1:1` of every importing document.
- `lute check <schema>.yaml` reports everything an importer's check would
  about the schema — `E-ENUM-LABEL-NOT-MEMBER`, `E-ENTITY-KIND-SHAPE`,
  `E-ENTITY-KIND-CLASH`, `E-FACT-DOMAIN`, the clock — at the schema's own
  lines (it said `ok`). In `check-project` an imported enum's
  `E-ENUM-LABEL-NOT-MEMBER` is folded into one report at the schema's line
  like the others, instead of `1:1` of every importer.
- `E-RULE-GUARD-DEF` (and a malformed rule) in a document's own `rules:` is
  reported at the rule's line, not `1:1`, when the rule quotes its
  `cel("…")` guard.
- The `E-BEAT-ATTR` hint for an entry's `once="day"` / `once="slot"` without
  a clock suggests `run` / `user` or omitting `once`, not `false` (which an
  entry refuses).
- A trace / test mock that seeds a derived `clock.*` path is
  `E-TRACE-MOCK-UNDECLARED`, naming the clock's day / slot paths to seed; it
  was accepted and contradicted them.
- `lute calendar` says `undecided (…)` and `lost to an undecided cell (…)`,
  naming the beat whose `when` is unknown, where it printed `? over quiet` /
  `lost to ?`.
- **`W-CAST-ABSENT` is precise** (dsl 0.24.0 §4; round-3 SU N4/N5, ER
  N3/N4, CR N4, LH N4). Measured on the four reviewed games, with `present:`
  declared as the reviewers had it and their workaround guards removed:
  Drowned Crown 12 → 0, Summer Station 5 → 0, Skerry Rock 1 → 1 (Ada's
  handwritten log line, which needs `{vo}`), Ember Road 11 → 9 (companion
  lines before the battle whose `present:` reads the engine-reserved `fell`;
  `assume: true` clears them). Every real catch the reviewers fixed is still
  reported.
  - An `::assert`/`::retract` voids only a guard atom it can falsify on that
    path: same relation with unifiable arguments, or through a rule where the
    written relation is a premise, moving it the wrong way. Asserting
    `recruited(tomas)` keeps `holds(inParty(mara))`, and so does asserting a
    positive premise. Each `&&` conjunct of a guard stands alone.
  - A write in one `<choice>` or `<match>` arm no longer reaches its
    siblings. A guard survives the branch when it survives every arm.
  - A guard's `holds(A)` over a derived relation implies its rules' bodies
    (`holds(inParty(mara))` gives `!holds(departed(mara))`). A relation
    whose rules are ground once bound, such as a `cel()`-only schedule, reads
    as the disjunction of those bodies, as `W-BEAT-PRIORITY-TIE` does. A
    `::set` of a path such a rule reads voids the guard.
  - A quest `<on>` handler assumes the quest's state at that event and the
    `start` conjuncts that stay true (`entry.<id>.everRead`, `visited(…)`).
    An entry's body assumes its own `everRead`/`read`. Under
    `check-project`, a beat assumes that every always-eligible `once` beat
    ranked above it on the same ladder has been spent.
  - A `{vo}` line is exempt. An `{os}` line is still checked, because the
    speaker is in the scene, just out of frame. Two of Drowned Crown's real
    catches were `{os}` lines.
- **`W-CAST-ABSENT` sees a fact only its own beat produces** (round-3 ER
  re-verify R1). Under `check-project`, a beat, entry or bundle beat
  presented at most once per run starts with `!holds(F)` for every ground
  fact `F` that only it asserts. No other unit of the root, component
  documents included, can assert `F`, and no seed names it. The fact's
  relation is `tier: run`, or `tier: user` in a `once: user` beat; it is
  neither derived nor engine-`reserved`. The body's own `::assert{F}` ends
  the assumption on that path. So a `present:` such as
  `holds(inParty(wren)) || !holds(recruited(wren))` is satisfied before and
  beside the only choice that recruits her, even when another scene can
  make her depart or `fell` is reserved.

### Compatibility

- **Arms kept only to satisfy the checker are now `E-ARM-DEAD`.** A beat's
  `when:`, a bundle `<beat when>` and an entry `when=` now narrow their whole
  body, and exhaustiveness reads the same narrowed domain. The 0.23 workaround
  — a `<when is="undecided">` (or `<otherwise>`) arm the beat's `when` rules
  out, kept so `E-NONEXHAUSTIVE` would pass — is now `E-ARM-DEAD` (or
  `W-OTHERWISE-DEAD`). Delete the dead arm; the match stays exhaustive. A body
  that writes the subject keeps the whole domain.
- **`by=` is judged at every settle; the old rule is `until=`.** 0.23.1 judged
  an `on=` objective's `by` only when its occasion was raised. Now it fails the
  objective at the first settle where it holds, wherever the player is. Content
  written for the raise-only rule — a `by` that `done` implies, such as
  `done="run.v == 'fell'" by="run.v != 'undecided'"` — can never complete and
  is flagged `W-DEADLINE-BEFORE-DONE`; replace `by=` with `until=` (same
  condition), which is judged only at the occasion's raise, after `done`.
  `lute run` / `lute play` print `failed (until)` for it, and failing
  `quest` records carry `failedBy`.
- **Bridge results come from `bridges:`, not `scene.*` seeds.** A `*.test.yaml`
  or trace mock that seeded a plugin call's result slot (`state: {
  scene.check.guards.passed: true }`) no longer decides the guard: the walk
  stops unresolved with a `bridges:` hint, and `lute play` refuses the seed
  (exit 2). Replace it with `bridges: { check: [ { passed: true, margin: 3 }
  ] }` — one answer per call, every field the result shape requires.
- **Plugin `lower:` is optional, and the `builtin` workaround is rejected.** A
  directive without `lower:` is the generic `kind: "plugin"` passthrough (it
  was `E-PLUGIN-PARSE missing field lower`, and `schemas/lute.plugin.json`
  required it too). A `lower: { kind: builtin, name:
  X }` naming a hook the core does not register (`bridgeMinigame`, `mgart`, …)
  — the old way to satisfy that error — is now `E-PLUGIN-PARSE`; drop `lower:`
  (a bridge directive binds its call with `bridge:` alone; the compiled
  records do not change). A registered hook now runs its builtin:
  `clearStage` clears the stage as `::clear` does, where it used to compile to
  a passthrough. Plugin export files also reject unknown keys
  (`E-PLUGIN-PARSE` with a did-you-mean), so a misspelled key that loaded as
  its default now fails.
- **Cast presence is a new warning.** A cast entry that declares `present:`
  makes every line by that speaker whose guards do not imply it
  `W-CAST-ABSENT` (`--deny W-CAST-ABSENT` works). Projects whose cast has no
  `present:` are unaffected. Guard the line (`@isolde{when="…"}`), or declare
  `assume: true` when `present` reads an engine-reserved relation whose
  negation holds until the engine writes it.
- **`judge: before` moves when quests settle, not when handlers run.** On an
  occasion declared `judge: before`, its `on=` objectives are judged and the
  quests settled before its beats are chosen, so an epilogue reads the ending;
  the `<on>` handler bodies of that raise still run after the beats. The
  default `after` keeps the 0.21 order, and only a snapshot that declares
  `judge: before` changes `capabilityVersion` for it.
- **`capabilityVersion` moves for every document.** `lute.core` gains
  `::clear`, so every capability stamp changes; engines or build caches keyed
  on the stamp see new values. Compiled records are otherwise unchanged for
  documents that use none of the new syntax.
- **Schema file renamed; additive IR.** The version strings move to `0.24.0`
  and `schemas/lute-ir-0.23.schema.json` is renamed to
  [`schemas/lute-ir-0.24.schema.json`](schemas/lute-ir-0.24.schema.json)
  (`$id` updated). Every new field is optional and appears only when the
  source uses the feature: `clock`, `once: "day"|"slot"`,
  `ObjectiveEntry.until`, `QuestCmd.activate` / `complete`, `OnCmd.target`,
  `AcceptCmd.applies`, placeholder `format`, `StateEntry.labels`, and the CEL
  op `%`. Engines gate on MAJOR, so nothing widens; the tree-sitter grammar is
  unchanged.
- **Documents that were already wrong can redden.** A relation named like a
  CEL call (`has`, `holds`, `count`, …) is `E-RELATION-RESERVED-NAME`; an
  `::accept` of a child that activates with its parent is `E-ACCEPT-TARGET`
  (it was a silent no-op); a map `default:` without `per:` is `E-STATE-DECL`
  (it yielded no default); a maybe-unset read through a `@def` is
  `E-MAYBE-UNSET` at the use; a `@def` a rule guard cannot expand is
  `E-RULE-GUARD-DEF` (the rule used to be dropped silently).

## [0.23.1] - 2026-09-25

**Trace, test and play agree.**

A patch on the `0.23` line from a second dogfood round over three games. Where
`lute trace`, `lute test` and `lute play` answered one question three ways — a
deadline judged before its objective, a reward credit only play applied, an
occasion that never reached its quest's event handlers, a `::end` that stopped
the whole playthrough — they now give one answer, the one the runtime contract
specifies; the checker closes a few more gaps (`E-QUEST-TIER-MIX`, match-arm
narrowing, negation over stable derived facts). No syntax is added and the IR
shape does not move. See [`docs/versioning.md`](docs/versioning.md) for what
each axis earned.

### Changed

- `E-QUEST-TIER-MIX`: a subquest whose `tier` differs from its parent's is an
  error naming both quests and tiers (`check` when both share a document,
  `check-project` across documents) — a mixed tree locks for good.
- An `on=` objective's `by` deadline is judged only when its occasion is
  raised, after its `done`; in every settle `done` is judged before `by`.
  `lute trace`, `lute run` and `lute play` agree.
- `lute trace` / `lute test` apply a reward kind's `credits:` and walk objective
  completion bodies exactly as `lute play` does; play prints a whole credit as
  `= 1`, not `= 1.0`.
- `*.test.yaml`: `expect.facts` / `notFacts` (after derivation), `expect.eligible`
  for a presented entry/beat (and a note when one is presented though
  ineligible), `beat: <id>` presents a bundle beat; `E-TRACE-ENTRY` names the
  document's beats.
- `lute test` resolves scenario tests against the nearest `lute.project.yaml`
  (announced on stderr) when `--project` is absent, accepts one `*.test.yaml` /
  `*.play.yaml` file, and reports a test whose `file:` is missing as one
  `E-TEST-FILE` failure instead of aborting the suite.
- Play step `expect:` gains `quests`, `state`, `facts`, `notFacts`, judged right
  after that step settles, on any step kind; state misses quote both sides.
- `lute play`: a `::end` ends only the presentation (or quest handler) it runs
  in — the playthrough goes on with the next step. A new step `end: true` ends
  the playthrough (exit 0); later steps print as skipped and `--json` lists them
  under `skipped`. Scripts that relied on `::end` stopping the play add
  `- end: true` after that step.
- Raising an occasion also fires a same-named declared world event: every
  active quest's `<on event>` handlers run, then the occasion judges its `on=`
  objectives — in `lute play`, `lute run` and `lute trace` alike.
- `lute play` prints staging as authored (`::bg{…}`, `::auto{…}`,
  `::vfx{type=…}`, a plugin directive by its own name) and drops the bare
  `beat` line; `--ir` prints the lowered records, injected ones included. A
  `newRun` names the `prev.run.*` snapshot it took and lists only the run-tier
  quests it actually reset (with their previous status).
- `lute calendar --script` replays the script's steps before evaluating the
  grid (`--until <step number | label>` stops before that step); `--axis` is
  optional; `quest.<id>.state=…` seeds the quest's status (it was silently
  overwritten); `holds(<fact>)=true,false` asserts / retracts a base fact;
  another `quest.*` path or a derived fact is a usage error; `--where <cel>`
  drops cells where the condition does not hold; a targeted occasion gets one
  column per target its beats name (or per `--target`), not every member of
  its domain. `--json` gains `from`, `pruned`, `anyTarget`; a cell's `note` is
  now `notes` (CSV column too), and the settle moving an axis value is noted.

### Fixed

- **Trace/test seeds read by a beat `when:`** — a `quests:` / `state:` seed of `quest.<id>.state` is admitted when the only read is the scene's frontmatter `when:` (trace already judged it); the refusal for an unread quest now names `quests:` instead of `--state`.
- **Occasion target `members:`** — `target: { prefix, entity, members: [..] }` narrows a domain to a subset of an entity kind; a domain without `members` keeps its `capabilityVersion`.
- **Decider** — a disjunction of numeric comparisons on one path that covers the number line decides true. `cel()` rule guards stay opaque to the decider.
- **`lute tag` no longer renames a shipped component lineId** (ashen N7): an
  untagged component line now takes the code `lute tag` would write into the
  component file (source order, per speaker) before a param-scoped `<match>`
  folds, so a `::use` with a literal argument and one with a `@def` argument
  mint the same code for the same line. **Migration:** a component whose
  untagged lines sit in a non-first `<match>` arm compiles to new `lineId`s
  (the ones `lute tag` persists); re-export localization / voice manifests.
- **Bundle beats are scenario nodes** (lamplight N8, ashen N9): `lute scenario`
  draws every bundle beat as an edgeless entry node `beat(<doc>.<beat>)` (text,
  JSON `kind: "beat"`, DOT `shape=note`); `scenario reach` / `envelope` accept
  its canonical id bare or as `beat:<id>`, and `reach` prints its occasion,
  target and `when`.
- **`lute lore` lists beats** (lamplight N8): bundle beats and scene beats
  appear under their target, labelled `beat` (JSON `kind: "beat"`, `on`), and a
  fact a bundle beat asserts is credited under `beats` (new JSON array), not
  `entries`; `revealedBy` gains `beats`.
- **Match arms narrow their subject** (ashen N3): inside `<when is="x">` (no
  `unset` alternative) the subject is set, and once an arm takes every unset
  value (`is="unset"` with no `test`) later arms and `<otherwise>` see it set —
  no `E-MAYBE-UNSET` for those reads.
- **`W-BEAT-ONCE-RUN-USER` fires only on a defaulted `once`** (ashen N1): an
  authored `once: run` (and an entry's `once="run"`) acknowledges a per-run
  beat; `prev.run.*` counts as run history, not user state. The message adds
  "write `once: run` if it should replay every run".
- **Negation over a stably derived fact** (lamplight F9): the stable-fact set
  closes under rules whose every premise is stable, so `not alibi(crane)` with
  `alibi` derived from seeds nothing removes never holds; an impossible derived
  fact now names the defeating fact (lamplight N16) instead of "no rule".
- **`--wip` grades dead choices, arms, gated lines and `::next`** (lamplight
  N11) like dead entries/beats/objectives.
- **Mis-nested `<entry>` / `<beat>` / `<quest>`** (lamplight F23): one
  `E-UNCLOSED-TAG` "entries cannot nest; `<id>` opened at line N is still
  open", the nested block parsed as a sibling (no cascade); content outside a
  block in a lore/quest document no longer advises a `## ` heading.
- **Stage tracking across partial arms** (seven F7): a character on stage in
  only some arms of a `<match>` / `<branch>` is auto-hidden at the next `::bg`
  (compiled hide) and warned when speaking without a re-show;
  `W-STAGE-ABSENT` names the exit / `::bg` line, and a redundant exit after a
  `::bg` says "this exit does nothing. Move it before the `::bg`, or delete
  it" (lamplight N15).
- **One YAML slip no longer kills guards in other files** (seven F3): when any
  document of a root has an unparseable frontmatter, `check-project` decides
  no fact query impossible.
- **`lute scenario knowledge` names what defeats a negation** (lamplight N10):
  under a negated premise, the rule body is instantiated against the may set
  and each fact that can make it false is listed — `can be defeated by
  alibi(solt, tunnel) ⇐ seen(solt, corridor, tunnel) [entry `mirelaMatch` …],
  away(corridor) [seed]`. Producers are labelled `scene `<key>`` and bundle
  beats by canonical id `beat `<doc>.<beat>``; JSON `assertedBy` labels change
  the same way.
- **`L-EMOTION-DISTRIBUTION` judges each bundle beat alone** (ashen N6): runs
  and streaks are measured per linear unit — a document's shots and quest
  bodies together, each lore `<beat>` on its own — and lore entries are not
  measured, so a bundle's independent beats no longer read as one scene.
- **`lute doctor` checks running `lute-lsp` servers** (seven F27): a new
  `running lute-lsp` line (`runningLanguageServers` in `--json`, Unix) lists
  each running server and fails, advising an editor restart, when one was
  started before its binary was replaced or its binary reports another
  version.
- **`lute doctor` no longer flags a freshly started `lute-lsp` as replaced**
  (macOS): `ps` reports a process's elapsed time as whole seconds counted
  from the second it started, up to a second more than it has run, so a
  server launched within a second of its binary being written read as
  "started before its binary was replaced". The check now allows that second
  of slack.
- **Docs: `pov` is descriptive** (ashen N8): the pages claimed the `pov`
  speaker compiles to a reserved player role; no such IR role exists and
  `pov` does not reach the artifact. The dialogue, frontmatter and cheatsheet
  pages (and `llms-full.txt`) now say so.

### Compatibility

- **Untagged component lines may change `lineId` once.** An untagged line in
  a component now takes the code `lute tag` would write into the component
  file (source order, per speaker) before a param-scoped `<match>` folds, so
  a component whose untagged lines sit in a non-first `<match>` arm compiles
  to new `lineId`s / `voiceKey`s — the ones `lute tag` persists, so they do
  not move again. Re-export localization and voice manifests once; tagged
  lines and lines outside components are unchanged.
- **`::end` in `lute play` ends only its presentation.** It used to stop the
  playthrough; now the presentation (or quest handler) it runs in ends and
  the play goes on with the next step. A script that relied on `::end`
  stopping the play adds a step `- end: true` after that step; later steps
  print as skipped and `--json` lists them under `skipped`.
- **A raised occasion fires the same-named world event.** When a world event
  of the occasion's name is declared, raising the occasion runs every active
  quest's `<on event>` handlers for it, once, before its `on=` objectives are
  judged — in `lute play`, `lute run`, `lute trace` and the runtime contract.
  A handler that ran only on an explicit `event:` now also runs whenever the
  same-named occasion is raised; an engine that fired that event itself at
  the raise stops doing so, or the handler runs twice.
- **Trace and test apply reward credits and objective bodies.** `lute trace`
  and `lute test` now add a reward kind's `credits:` amount and walk
  objective completion bodies as `lute play` does, so a test that expected
  the state without them now fails; update its `expect:`.
- **`E-QUEST-TIER-MIX` rejects mixed-tier quest trees.** A subquest whose
  `tier` differs from its parent's used to check clean and then lock at
  runtime; it is now an error (`check` within one document, `check-project`
  across documents). Give the child its parent's tier.
- **An `on=` objective's `by` waits for its occasion.** Its deadline is judged
  only when the occasion is raised, right after its `done`, so a beat that
  answers the occasion can no longer lose to a deadline judged earlier in the
  same settle.
- **`lute calendar --json`:** a cell's `note` is now `notes` (the CSV column
  too), and the output gains `from`, `pruned` and `anyTarget`.
- **IR shape unchanged.** The version strings move to `0.23.1`;
  [`schemas/lute-ir-0.23.schema.json`](schemas/lute-ir-0.23.schema.json)
  keeps its name and `$id`, and engines, gating on MAJOR, widen nothing.
  `capabilityVersion` moves only for a snapshot whose occasion target domain
  declares `members:`; the tree-sitter grammar is unchanged.

### Known limitations

- `lute play` coverage counts presented documents only, not branch choices or match arms (trace keys arms by source position; play records compiled addresses).

## [0.23.0] - 2026-09-25

**Author overviews and time.**

Once a story is chosen by occasions rather than read top to bottom, a writer
needs to see who can say what, when. This release adds three overviews — a
beat ladder, a calendar over state, and a knowledge map — plus deadlines and
targeted objectives, occasions that compose a routine with an event, several
scene-like beats in one lore file, a checked cast, rewards that credit state,
the previous run's values, and a condition decider that finds contradictions
the checker used to miss. The language and the IR both earn the move (the IR
additively); see
[`docs/proposals/scenario-dsl/0.23.0.md`](docs/proposals/scenario-dsl/0.23.0.md)
and [`docs/versioning.md`](docs/versioning.md).

### Changed

- **A sharper condition decider** (dsl 0.23.0 §9): `decide` now reasons per
  path across `&&`/`||` operands (state paths, `$`, component params,
  `holds(P)`/`visited(id)`/`count(P)`). A conjunction whose operands are false
  for every value of one path decides false (`run.n > 5 && run.n < 3`,
  `run.slot == 'a' && run.slot == 'b'`, `x && !x`); a disjunction whose cases
  cover a path's domain decides true. `unset` counts as a value, and an
  ordering that errs on it is neither true nor false, so every verdict is the
  expression's actual value. `E-BEAT-UNREACHABLE`, `E-ENTRY-UNREACHABLE`,
  `E-ARM-DEAD`, `E-OBJECTIVE-UNSATISFIABLE`, `W-BEAT-PRIORITY-TIE` and the
  compile-time fold all benefit. In the may set, a negated rule atom over a
  seed nothing retracts or displaces is false, so `not alibi(crane)` with a
  canon alibi no longer keeps a derived fact possible.

### Added

- **Overviews** (dsl 0.23.0 §1, §11): `project.index.json` beat rows carry
  `when` (expanded) and `title` (additive, omitted when absent). `lute beats
  <dir> [--occasion] [--target] [--json]` prints each occasion/target's beat
  ladder in selection order with priority, `once`, `after`, `when` and the
  `check-project` verdicts (unreachable, shadowed, tied, once-run-user).
  `lute calendar <dir> --axis path=1..7 --axis path=a,b [--occasion]
  [--target] [--script save.play.yaml] [--json|--csv]` evaluates play's own
  eligibility at every cell of the axes' product from the save (quests
  settled per cell): winner, shadowed eligible beats, undecided cells, and
  the beats never eligible anywhere. `lute scenario <dir> knowledge [--for
  <node>]` traces every fact-guarded beat/entry/objective to the atoms it
  queries and their producers through rules (asserting documents, seed
  facts, reserved, or none). The scenario graph notes the
  `completed()`/`active()`/`visited()` references it does not draw because a
  quest declares no `after=`. `lute play` marks an already-read entry
  candidate `read`.
- **Deadlines** (dsl 0.23.0 §2): `<objective by="<condition>">` — a condition
  slot like `done`. The first time `by` holds while the objective is not
  done, the objective fails and is never judged again; a failed required
  objective fails its quest (`failed` rewards, `questFailed`, cascade). `done`
  is judged first, so a deadline never fails a done objective. IR:
  `ObjectiveEntry.by` (additive). `lute trace` records the objective decision
  `failed`; `lute run` / `lute play` print `failed (by)`.
- **Objective targets** (dsl 0.23.0 §2): `<objective on="talk"
  target="npc.maud">` is judged only when the occasion is raised for that
  target, checked like a beat target (`E-BEAT-ATTR`). IR:
  `ObjectiveEntry.target` (additive). `lute trace` / `lute run` raise for a
  target with `occasions: [talk@npc.maud]` / `--occasion talk@npc.maud`; a
  `lute play` step judges objectives for its `target:`.
- **Composing occasions** (dsl 0.23.0 §3): an occasion declared `select:
  sequence` presents every eligible beat in selection order; a scene beat's
  `also: true` rides along after a `select: first` winner and never replaces
  it (IR `BeatIr.also`, additive; `E-BEAT-ATTR` when not a bool, on an entry,
  or on a `select: all` / `sequence` occasion). `W-BEAT-SHADOWED` and
  `W-BEAT-PRIORITY-TIE` ignore `also` beats. `lute play` presents the whole
  list with a quest settle after each beat (`--json`: `presented`, then
  `then`), rejects `pick:` on a `sequence` occasion, and gains the step
  expectation `presented: [ids]`. The `sequence` value changes the
  capability stamp only for snapshots that declare it.
- **Beat bundles** (dsl 0.23.0 §4): a `kind: lore` document may hold
  scene-like `<beat id on target when priority once also title>` blocks with
  a scene body (lines, branches, hubs, matches, directives). A beat's
  canonical id is `<document id>.<beat id>` (the document needs `id:`); it is
  checked like a scene beat (`E-BEAT-ATTR`, `E-OCCASION-UNKNOWN`,
  `E-BEAT-UNREACHABLE` per file and under the fact envelope,
  `W-BEAT-SHADOWED`, `W-BEAT-PRIORITY-TIE`, `W-BEAT-ONCE-RUN-USER`), is
  spent by presentation (`once` defaults to `run`), and `visited('<doc>.<beat>')`
  reads it in any condition (an `after:` still names only scenes and quests).
  A beat id sharing a scene's id is `E-CONN-EPISODE-ID-DUP`. IR: a new `beat`
  command heads each beat's addressing unit in the lore artifact (units in
  source order; an entry's or beat's body segment runs to the next `entry` or
  `beat` record), and `project.index.json` beat rows gain kind `bundle`
  (additive). `lute play` presents bundle beats; `lute trace --beat <id>` and
  `lute run --beat <id>` present one (new `E-TRACE-BEAT`). Tree-sitter, LSP
  (completion, symbols, folding, tokens), `lute tag`/`fix`/`loc`/`context`/
  `doctor`, lint metrics and `lute lore` cover beat bodies.
- **Hub prompts** (dsl 0.23.0 §4): `<hub prompt="…">` attaches the question
  shown with the hub's options (IR `HubCmd.prompt`, additive; an empty prompt
  is `E-BRANCH-PROMPT`). `lute run` / `lute play` print it; LSP completes it.
- **Components with sentences** (dsl 0.23.0 §5): a component `string` param
  may be interpolated (`{{@p}}`); a literal `::use` argument is substituted
  into the line at expansion, so each call site ships its own sentence under
  its own component-scoped `lineId`. Binding such a param to a `@def` ref is
  `E-REF-TYPE` at the argument.
- **Previous run** (dsl 0.23.0 §6): `prev.run.<path>` reads the value every
  declared `run.<path>` had when the previous run ended — typed like its run
  path, maybe-unset before the first run ends, read-only
  (`E-QUEST-RESERVED-WRITE`). `lute play` snapshots it at `newRun`; play
  `state:` seeds and trace mocks may set it.
- **Cast** (dsl 0.23.0 §7): a plugin `cast` export or a schema document's
  `cast: { <id>: { name } }` declares the speakers; once declared, any other
  speaker is `E-CAST-UNKNOWN` with a did-you-mean. `lute context` lists the
  cast and LSP speaker completion offers it.
- **Rewards that credit state** (dsl 0.23.0 §8): `rewardKinds.<kind>.credits`
  names a state path; the IR stamps it on each reward (`RewardEntry.credits`,
  additive), `lute run` / `lute play` add the scalar amount there on grant,
  and a handler `::set` of the same path is `W-REWARD-DOUBLE-CREDIT`.
- **`lute check-project --wip`** (dsl 0.23.0 §10): `E-ENTRY-UNREACHABLE`,
  `E-BEAT-UNREACHABLE` and `E-OBJECTIVE-UNSATISFIABLE` become warnings when
  the guard is dead only because a relation has no producer at all yet (no
  seed, assert, rule, or reserved declaration); a relation that has producers
  but never matches stays an error.

### Fixed

- **`lute run` / `lute play` evaluate structured `isSet` / `has` nodes.** The
  reference runner read the lowered `{isSet}` / `{has}` expression nodes as
  unknown, so a compiled gated line or match arm shaped like
  `isSet(x) && …` never matched. It now evaluates them.
- **`schemas/lute.plugin.json`** lists the `rewardkinds`, `occasions` and
  `cast` exports the loader accepts.

### Compatibility

- **Contradictions the checker used to miss are now errors.** The sharper
  decider reports conditions that can never hold — `run.n > 5 && run.n < 3`,
  `run.slot == 'a' && run.slot == 'b'`, `x && !x`, a negated rule atom over a
  seed nothing retracts — as `E-BEAT-UNREACHABLE`, `E-ENTRY-UNREACHABLE`,
  `E-ARM-DEAD` or `E-OBJECTIVE-UNSATISFIABLE`, and folds provably true/false
  guards at compile. A project that checked clean on 0.22 may redden where a
  guard was already dead; fix the condition. While content is still being
  written, `check-project --wip` downgrades the cases caused by a relation
  with no producer at all.
- **Documents that were already wrong can redden.** Once a project declares
  a cast, an unknown speaker is `E-CAST-UNKNOWN`; a project without a cast
  is unaffected. A reward whose kind declares `credits` beside a handler
  `::set` of the same path warns `W-REWARD-DOUBLE-CREDIT`. Declaring a
  `prev.*` path is `E-STATE-NAMESPACE`.
- **Additive IR.** The version strings move to `0.23.0` and
  `schemas/lute-ir-0.22.schema.json` is renamed to
  [`schemas/lute-ir-0.23.schema.json`](schemas/lute-ir-0.23.schema.json),
  gaining the `cmdBeat` command, `sceneBeat.also`, `objectiveEntry.by` /
  `target`, `cmdHub.prompt`, `rewardEntry.credits`, and `indexBeat` kind
  `bundle` with `when` / `title`. Every new field is optional and every new
  record appears only when the source uses the feature, so artifacts that use
  none of it compile byte-identically apart from the version strings.
  Engines gate on MAJOR, so nothing widens; an engine that predates bundle
  beats rejects a lore artifact carrying a `beat` record (unknown `kind`),
  which is the intended hard error. `prev.run.*` is not an IR state row: the
  engine snapshots `run.*` at run end.
- **`capabilityVersion` moves only when used.** An occasion declared
  `select: sequence`, a non-empty plugin `cast`, and a reward kind with
  `credits` change the capability stamp only for snapshots that declare
  them. The tree-sitter grammar gains `<beat>` in lore documents.

## [0.22.0] - 2026-09-25

**A reference player that can stand in for the engine.**

Three dogfood games each shipped a fake engine inside their content because
`lute play` could not write the state the engine owns, start from a save,
assert anything, or vary decisions per step. This release closes those gaps,
gives run boundaries a lifecycle, lets trace and test apply the project's
Datalog rules, and gives occasion targets a vocabulary. It also carries the
two identity changes 0.21.1 deferred because they alter compiled output. The
language and the IR both earn the move; see
[`docs/proposals/scenario-dsl/0.22.0.md`](docs/proposals/scenario-dsl/0.22.0.md)
and [`docs/versioning.md`](docs/versioning.md).

### Changed

- **`lute trace`, `lute test` and `lute play` derive by default** (dsl 0.22.0
  §6, D-B). Trace and test now load the project's seed `facts:` and apply its
  Datalog rules (stratified negation) over the mocked and asserted facts, so a
  rule-derived fact satisfies a guard, `done` or `start` without mocking the
  conclusion, and "derived false because a negated premise holds" is
  testable. A mocked derived atom is still accepted — it is a seed like any
  other. Trace, test and the reference runner (`lute run` / `lute play`) now
  share one evaluator, `lute_trace::datalog`, so they cannot disagree about
  what a project's rules conclude. A rule guard over undecided trace state
  leaves the conclusion unknown and names the state path that would decide it.
- `derive: false` (mock / test / play-script key) and `--no-derive` (`lute
  trace`, `lute test`, `lute play`; the flag wins over the key) restore the
  0.21 model: seeds are not loaded, an unmocked derived atom is unknown, and a
  note names each derived relation read. **Migration:** a test that relied on
  an unmocked derived atom being unknown (exit 3), or on a seeded relation
  reading empty, now sees the derived / seeded answer; pin `derive: false` to
  keep the old verdict.
- **A lore document is testable, so `lute test --coverage` lists an untested
  one** (dsl 0.22.0 §5). It was left out of the untested set because no test
  could target it; a test now names the entries it presents.
- **Breaking: the default `voiceKey` is `{prefix}.{speaker}-{code}`** (dsl
  0.22.0 §11, D-E). The 0.21 default `{speaker}-{code}` repeated across
  documents, so every scene's `@ann{code="0010"}` landed on one voice asset
  (0.21.1 made that `E-DUP-VOICEKEY`). A project that recorded audio against
  the old keys pins `identity: { voiceKey: "{speaker}-{code}" }` in
  `lute.project.yaml` to keep them; a project that already pinned the prefixed
  template (as `lute init` and the example projects do) is unchanged.
- **Breaking: component lines have their own `lineId`/`voiceKey` scope** (dsl
  0.22.0 §11, T1-10). A line expanded from a component is addressed
  `{prefix}.{component}#{n}.{speaker}_{code}`, where `n` counts the host's
  `::use`s of that component (1-based, document order; a nested `::use`
  counts within its enclosing expansion and adds another segment). Each
  expansion back-fills its own untagged codes, so a component line's code no
  longer depends on the host lines before it, and host lines after a `::use`
  keep the codes `lute tag` gives them. Two uses of a tagged component, or a
  component line sharing a code with a host line, now compile clean with
  distinct ids instead of `E-DUP-LINE-CODE`; `lute loc export` emits the same
  ids. **Migration:** re-export localization / voice manifests for documents
  that `::use` components.
- **`W-STAGE-ABSENT` follows paths** (dsl 0.22.0 §12, T1-15). `lute check`
  folds each `<branch>`/`<hub>` choice and `<match>` arm from the stage state
  at the fork and joins the arms at convergence, so an exit in one arm no
  longer warns on a line in its sibling. After the convergence a character is
  on stage only if every arm left them there; one taken off on any arm warns
  when staged again without a re-show. A `::bg` scene change's auto-hide now
  records the hidden characters as exited, so a later line by one of them
  warns (it used to check clean), and a scene change no longer forgets an
  earlier declared exit. `lute compile` uses the same join.
- **Lint defaults stop assuming a linear VN** (dsl 0.22.0 §13, T3-3).
  `L-SHOT-STARTS-WITH-BACKGROUND`, `L-DIALOGUE-RATIO` and
  `L-SCENE-LENGTH-SPREAD` judge only linear scenes: beats (scenes with
  `on:`), components, quests and lore no longer fire them. Rules see the new
  `scene.kind` / `shot.kind` (`scene`, `beat`, `component`, `quest`, `lore`).
  A shot's `firstStagingTag` and `scene.directives` count staging directives
  only — `::accept`, `::use`, `::end`, `::mark`, `::next` are skipped — and
  numbers in lint messages are rounded to two decimals.
- `lute doctor` says "no pinned provider snapshots" instead of calling a
  project with plugins "core-only", and looks for them in the project's
  `catalogDir:` (default `catalog/`) — the directory `check` actually reads.
- `lute init`'s `minimal`/`investigation` manifests drop the `identity:`
  pin: the 0.22.0 default `voiceKey` already carries `{prefix}`.

### Added

- `lute play --explain <atom>` (repeatable): after the play, prints the
  derivation tree of a ground atom — the rule used and each premise's own
  support (seed fact, asserted, or derived in turn), negated premises shown
  `(absent)` — or, when it does not hold, every rule that could conclude it
  with its failing premises (a missing premise explained in turn, a present
  negated premise, a false comparison or guard). `--json` carries the same
  tree.
- **Play-script assertions** (dsl 0.22.0 §4). A play step may carry
  `expect: { winner, offered, notOffered }` (`winner: none` when the occasion
  passed; `offered` is a subset of the eligible beats, order-insensitive) and
  the script a top-level `expect: { exit, quests, state, facts, notFacts,
  transcriptContains, transcriptLacks }` judging the end of the play (`state`
  compares effective values, typed; `facts` after derivation). A miss names
  the step, its `label:` and the actual value, and `lute play` exits 1; an
  unknown `expect:` key is a usage error listing the legal keys.
- **`lute test` runs every `*.play.yaml` that carries an `expect:`**
  alongside `*.test.yaml` (PASS/FAIL lines, `--json` entries with
  `"kind": "play"` and `misses`). A play that halts fails unless its
  top-level `expect:` declares the exit. `--coverage` counts every document a
  play presented (`coverage over N traced path(s) and M play(s)`, JSON
  `coverage.plays`).
- **Scenario-test expectations** (dsl 0.22.0 §5): `transcriptLacks: [...]`,
  `offered: { <choice id>: [opts] }` (the exact set of options the walk
  offered at that branch/hub, across its presentations), and `entry: <id>` /
  `entries: [ids]` for a lore file — the entries are presented in order with
  the read flags set between them, so a repeated id is a re-read. A lore
  test without either is still `E-TEST-LORE`, which now says how to name
  them.
- **Save-history seeds in trace mocks and tests** (dsl 0.22.0 §3):
  `quests: { <id>: unset | active | complete | failed }` and
  `entriesRead: { run: [ids], user: [ids] }` (`entry.<id>.read` /
  `entry.<id>.everRead`). They seed the reserved state paths they spell and
  follow those paths' mock rules — the document must read the path.
- **`owner: engine`** (dsl 0.22.0 §1.2): a `state:` declaration may carry
  `owner: engine`; a content `::set` of that path (or a field under it) is
  the new error `E-ENGINE-OWNED-WRITE`. Reads are unrestricted; any other
  `owner:` value is `E-STATE-DECL`.
- **Run boundaries** (dsl 0.22.0 §7): `<quest tier="run">` (IR
  `QuestCmd.tier: "run"`, omitted for the default `user`; a quest document may
  hold several quests, so the tier rides on each quest record), entry beats
  accept `once="run" | "user"` (IR `EntryCmd.once`, and `ProjectIndex.beats`
  entry rows carry it; absent = repeatable), and the reserved user-tier
  `entry.<id>.everRead` flag is readable everywhere `entry.<id>.read` is
  (`W-ENTRY-REF-UNKNOWN` resolves it too). A bad `tier` is `E-ATTR-TYPE`; a
  bad `once`, or `once` without `on`, is `E-BEAT-ATTR`. New `check-project`
  warning `W-QUEST-HANDLER-DEAD`: `<on event="questFailed">` on a quest with
  no `fail`, no required subquest objective, and no parent quest.
- **Occasion target domains** (dsl 0.22.0 §8): an occasion's `target:` may be
  `{ prefix, entity }`; a scene or entry beat target must then be
  `<prefix>.<member>` of that `entities:` kind (`E-BEAT-ATTR` with a
  did-you-mean; any member of an `open:` kind). `target: true` keeps its
  shape-only meaning and its `capabilityVersion`. `lute_check::occasion_target_ok`
  is the shared rule `lute play` uses for step targets.
- **Beat selection advisories** (dsl 0.22.0 §13, `check-project`):
  `W-BEAT-PRIORITY-TIE` — beats on one `select: first` occasion (targets
  absent or equal) with equal priority whose `when`s are not provably
  exclusive, so file order picks the winner; `W-BEAT-ONCE-RUN-USER` — a
  `once: run` beat whose `when` reads only user-tier state and so replays
  every run. `W-BEAT-SHADOWED` now treats an entry with `once` as spendable.
- **`engine:` play steps** (dsl 0.22.0 §1.1, D-A): `- engine: { state: {…},
  facts: […], retract: […] }` writes what the engine owns — declared state
  as a literal or `{ add: <number> }`, ground atoms of any declared base
  relation, reserved ones included — checked against the declared types,
  entity members and arities before anything plays (exit 2). The step
  presents nothing and raises no occasion; the quest lifecycle settles after
  it, so a write can complete or fail a quest on the spot. `newRun` also
  takes `{ state, facts }`, applied after the reset as the new run's seed,
  and a new run now settles the quest lifecycle too. The fake-engine scenes,
  occasions and plugins the dogfood games shipped are no longer needed.
- **Per-step `choose:`** (dsl 0.22.0 §2): an occasion step's own map
  replaces the script's `choose:` key by key for that presentation; a
  step-local decision list starts fresh and leaves the script-wide list's
  consumption untouched.
- **Play scripts start from a save** (dsl 0.22.0 §3): top-level `visited:`,
  `presented: { run, user }` (spent `once` beats; presented scenes count as
  visited), `quests:` and `entriesRead: { run, user }`. An id the project
  does not declare is a usage error with a did-you-mean, and a `state:` seed
  that does not fit its path's declared type now is one too.
- **Run boundaries in `lute play`** (dsl 0.22.0 §7): a `newRun` returns
  `<quest tier="run">` quests to `unset` with their objectives undone; a
  first read sets `entry.<id>.everRead`, which no `newRun` resets; an entry
  beat with `once="run"` / `once="user"` is not eligible once its read flag /
  `everRead` is set.
- **`event:` play steps** (dsl 0.22.0 §9) fire a declared world event: the
  `<on event>` handlers of active quests run, as trace `events:` fires them.
  An `event:` naming an occasion, or an `occasion:` naming a world event,
  says which step key to use.
- **`pick: none`** (dsl 0.22.0 §10) closes a `select: all` list: nothing is
  presented or spent, and `on=` objectives are still judged.
- Play steps take `label:` (printed in the step header, carried in `--json`)
  and `repeat: <n>` (the step runs `n` times; each repetition is its own
  step record). A play step's target must lie in its occasion's target
  domain (dsl 0.22.0 §8) — a usage error with a did-you-mean otherwise.
- **`lute init --template beats`** (dsl 0.22.0 §13, T3-1): an occasions
  plugin, `defaults: { luteVersion, uses }`, a `world.schema.yaml` with an
  `owner: engine` clock and shorthand defs, beats with `id:`, a quest, entry
  beats, a play script with `engine:` steps and `expect:`, and scenario tests
  — `check-project`, `test` and `play` pass as scaffolded.
- **`lute new scene <name> --on <occasion> [--target <target>]`** writes a
  beat, checking the occasion and target against the project (a did-you-mean
  and exit 2 otherwise, leaving no file). Every `lute new` document now has an
  `id:` (scenes no longer get the `character`/`season`/`episode` triple),
  omits what the manifest's `defaults:` supplies, lands under the enclosing
  project's root, and `/` in a name nests it in a subfolder. Outside a
  project `lute new` says so, and refuses `--on`.
- **`lute context`** (T3-4) adds `defs` (type, params, body), relation
  `tier` and `reserved`, `owner: engine` on state paths, component
  signatures, occasion target domains and descriptions, the built-in
  directives (`::set`, `::assert`, `::retract`, `::accept`, `::use`), and
  every scene, quest and entry id in the `--project` (JSON: `defs`,
  `builtinDirectives`, `ids`).
- **`lute doctor`** (T3-5) reports the active plugins, every declared
  occasion with the number of beats answering it, the play scripts and
  scenario tests, and whether the `lute-lsp` on `PATH` is this toolchain's
  version. `lute-lsp --version` prints `lute-lsp <version>`.
- **`lute tag` / `lute fix` accept a directory** (T3-14): every `.lute` file
  under it, recursively and in sorted order, each line naming its file, then
  a summary. A refused or unreadable file is reported and the walk goes on;
  the exit code is the worst outcome.
- `lute-lsp` completes and documents on hover `<quest tier>` and
  `<entry once>`.

### Compatibility

- **Compiled identity changes (breaking).** Two changes alter the
  `lineId` / `voiceKey` strings `lute compile` emits: the default `voiceKey`
  is now `{prefix}.{speaker}-{code}`, and a line expanded from a component is
  addressed `{prefix}.{component}#{n}.{speaker}_{code}`. A project on the
  0.21 default `voiceKey` that recorded audio against it pins
  `identity: { voiceKey: "{speaker}-{code}" }` in `lute.project.yaml` to keep
  its keys; a project that already pinned `{prefix}.{speaker}-{code}` sees no
  voice-key change. Documents that `::use` components get new component-line
  ids either way — re-export localization and voice manifests for them. There
  is no `lute fix` rewrite: the pin is the migration.
- **Trace and test results can change.** Derivation is on by default, so a
  test that relied on an unmocked derived atom being unknown (exit 3), or on a
  seeded relation reading empty, now sees the derived / seeded answer. Pin
  `derive: false` in the test or mock, or pass `--no-derive`, to keep the
  0.21 verdict. `W-STAGE-ABSENT` follows paths, so a scene that warned on a
  sibling arm checks clean, and a line after a `::bg` auto-hide now warns;
  lint defaults stop firing linear-VN rules on beats, components, quests and
  lore.
- **Quest persistence is unchanged.** `<quest tier>` defaults to `user`, so
  every existing quest keeps its status across runs; only a quest that opts
  into `tier="run"` resets. An entry without `once` stays repeatable, and an
  occasion with `target: true` keeps its shape-only meaning and its
  `capabilityVersion`.
- **Documents that were already wrong can redden.** `E-ENGINE-OWNED-WRITE`
  fires only on a new `owner: engine` declaration; a beat target outside an
  occasion's new target domain is `E-BEAT-ATTR`; `check-project` adds the
  warnings `W-QUEST-HANDLER-DEAD`, `W-BEAT-PRIORITY-TIE` and
  `W-BEAT-ONCE-RUN-USER`. A play script whose `state:` seed does not fit its
  declared type, or that names an unknown id, is now a usage error (exit 2).
- **Additive IR.** The version strings move to `0.22.0` and
  `schemas/lute-ir-0.21.schema.json` is renamed to
  [`schemas/lute-ir-0.22.schema.json`](schemas/lute-ir-0.22.schema.json),
  gaining the optional `cmdQuest.tier` and `entryCmd.once` (also on the
  `ProjectIndex.beats` entry rows). Engines gate on MAJOR, so nothing widens;
  an engine without run tiers ignores both fields. `capabilityVersion` moves
  only for a project whose occasions declare a target domain; the tree-sitter
  grammar is unchanged.

## [0.21.1] - 2026-09-25

**No silent wrong answers.**

A patch on the `0.21` line. An audit of the toolchain found checks that
accepted a defect and then shipped the wrong thing — a passing test that never
reached its expectations, a `{{@def}}` an engine could only print as a marker,
a directive attribute dropped from the IR, two scenes' lines landing on one
voice asset. Each now reports the defect where it is made. No syntax is added;
the language's static semantics tighten, and the IR gains one optional field.
See [`docs/versioning.md`](docs/versioning.md) for what each axis earned.

### Added

- **New `E-DUP-VOICEKEY`.** The default `voiceKey` template
  `{speaker}-{code}` has no `{prefix}`, so lines of different scenes landed on
  one voice asset without a word. `check-project` and `compile --all` now
  refuse a key carried by lines with different text, naming each line; set
  `identity.voiceKey: "{prefix}.{speaker}-{code}"` (the default is unchanged
  until 0.22.0). `lute init` projects, the `docs/examples` projects and the
  first-scene tutorial now pin it (T1-9).
- **New `E-CAPABILITY-MISMATCH` at `check-project`.** A project whose documents
  resolve two capability snapshots passed `check-project` and was then refused
  by `compile --all` and `play`; `check-project` now runs the same
  single-snapshot gate, with the same message. `docs/examples/showcase` was
  such a project; its three scenes now share one set of scene-local options
  (T1-11).
- **IR — `expr` on a `ref` placeholder.** The referenced def body, inlined as
  a `{raw, expr}` CEL pair, so an engine renders `{{@def}}` by evaluating it
  like any other CEL slot (see *A `{{@def}}` renders its value* below).
  Optional in [`schemas/lute-ir-0.21.schema.json`](schemas/lute-ir-0.21.schema.json),
  which keeps its name and `$id`.

### Fixed

- **`lute test`: an incomplete trace fails.** A walk halted by an unknown guard
  reported `{"exit":"incomplete","passed":true}` whenever the test did not
  mention `exit:` — the expectations it never reached were never checked. It now
  fails unless the test opts in with `expect: { exit: incomplete }` (T1-13).
- **`lute test`: a lore document cannot be a test subject.** A test naming a
  lore file walked nothing and passed; it now fails with `E-TEST-LORE` and points
  at `lute trace <file> --entry <id>` (T1-13).
- **`lute test --coverage` measures the project.** The untested set was taken
  from the directory the tests live in, so `lute test tests --coverage` always
  said every document was tested. It now walks `--project`, else the nearest
  `lute.project.yaml` (T1-13).
- **`lute test`: `expect.state` compares the effective value.** A path the walk
  never wrote read "never written" even when its declared `default:` or the
  test's own `state:` seed was exactly the expected value. The comparison now
  uses trace's own read order: write, then seed, then default (T2-5).
- **`lute trace` names a beat scene whose `when` does not hold.** A scene traced
  under state where its frontmatter `when:` is false (or undecided) read as a
  plain `complete`; the trace now opens with a `beat \`when\`` note, and `lute
  test` shows it on the test line (T1-13).
- **`lute trace`: a choice forced past an unknown guard counts as unresolved.**
  It was only a `(forced)` suffix; the summary now counts it and names the atoms
  that would decide it, and `--json` carries `forcedUnknown`. The exit code is
  unchanged (T1-13).
- **`W-TRACE-MOCK-UNPRODUCIBLE` consults the project.** It judged a mocked fact
  against the traced document's own asserts, so every clue a sibling scene
  establishes was "not producible". With `--project`, or a `lute.project.yaml`
  above the file, it now uses the project's reachability-gated producer set;
  without a project the note says it judged this document only (T1-14).
- **`lute test` failures say why the walk stopped.** The unresolved guards and
  the `state:`/`facts:` entries that would decide them are printed, and `--json`
  carries `unresolved` (with `atoms` and `supply`) per test (T3-11).
- **No panic on a closed pipe.** `lute scenario`, `lute test`, `lute lore` and
  `lute trace` write their report once through the same EPIPE-safe path
  `compile` uses, so `… | head` exits instead of panicking (T3-15).
- **`lute play`: a `quest.<id>.state` seed is the quest's status.** It landed in
  state only, so the start settle re-registered the quest as `unset` and every
  quest-gated beat read the wrong status. The seed now registers the quest with
  that lifecycle status; an undeclared quest id or a value outside
  `unset|active|complete|failed` is a usage error (exit 2) (T1-1).
- **`lute play`: `::end` settles the step first.** A beat that ended the
  playthrough skipped the quest advance and the occasion's `<objective on>`
  judging, and still exited 0. The step's lifecycle now settles, then the walk
  stops (T1-2).
- **`lute play` holds the script to what is offered.** Forcing a spent `once`
  hub option was silently skipped; it now halts with `E-TRACE-CHOICE`, as `lute
  trace` does. A `choose:` list of two or more decisions for a `<branch>` is
  consumed one per presentation, in order, across the playthrough (also in
  `lute run`) instead of repeating its first entry; running out halts
  incomplete and says so. A single decision still answers every presentation
  (T1-8).
- **`lute play`: an ineligible `choose:` exits 1**, like an ineligible `pick:`;
  it was 2, the usage-error code (T3-19).
- **`lute play`: a `target: true` occasion raised without `target:` is a usage
  error** (exit 2); it played `(no candidates)` at exit 0 (T2-9).
- **A component `{{@param}}` renders the bound argument.** Every `::use`
  expansion shipped the same `"Outside: {{@weather}}."` with a `ref`
  placeholder naming a param that no longer exists, so `lute run`/`lute play`
  printed the marker. The param's literal is now substituted into each
  expansion's text (`Outside: grey.` / `Outside: still.`); a param bound to a
  caller-side def stays a `ref` placeholder naming that def (T1-3).
- **A `{{@def}}` renders its value.** The artifact has no defs table, so an
  engine could only print `{{@twice}}`. A `ref` placeholder now carries the def
  body inlined as `expr` (`{raw, expr}`; optional in
  `schemas/lute-ir-0.21.schema.json`), and `lute run`/`lute play` evaluate it.
  A def that cannot be inlined into one expression (an expansion cycle, a body
  that reads `$`) is the new error `E-INTERP-DEF` at `lute check` (T1-3).
- **A def in a directive attribute is never dropped.** `::camera{zoom=@closeUp}`
  compiled with no `zoom` at all, and `::bg{time=@slotNow}` shipped the CEL
  source `"(run.slot)"` as the time. A def that folds to a constant is now
  written as that literal (`zoom: 1.3`) and checked like an authored one
  (`E-BAD-ENUM`, `E-ATTR-TYPE`); a state-dependent def — directly, or as a
  `::use` arg the component puts into an attribute — is the new error
  `E-ATTR-DEF-DYNAMIC` (T1-4).
- **`lute check`: `quest.<id>.state` is an always-assigned lifecycle enum.**
  Every read was `E-MAYBE-UNSET` with a message about `::set`, and `== 'unset'`
  added `E-UNSET-LITERAL` — while play, trace and `lute test` all treat `unset`
  as the state before activation. Reads and `== 'unset'` are now clean, `<when
  is="unset">` names that member (and compiles to `== "unset"`, which fires;
  `!isSet(…)` never did), `$ == null` no longer counts as covering it, and
  `isSet(quest.<id>.state)` — always true — is the new warning
  `W-QUEST-STATE-ISSET` (T1-1).
- **`lute check`: a `@def` argument to an enum component param is checked.** A
  string def was compatible with every enum, so `depth=@pick` with a body that
  could produce a non-member passed and matched no arm. Every value the body
  can produce must be a member, or it is `E-COMPONENT-ARG` (T1-5).
- **`lute check`: def bodies get the CEL profile gate.** The `@name` use site
  is exempt as a macro and nothing looked at the body, so `%`, `size()` and
  even unparseable CEL passed. A body is now `E-CEL-PROFILE`/`E-CEL-PARSE` at
  its own key; the `E-DEF-DECL` hint writes `type: <bool|number|enum>` instead
  of guessing `bool` (T1-6).
- **`<quest>`, `<objective>` and `<on>` close their attributes.** An invented
  or misspelt key (`fial=`, `optinal`, `target=`) was accepted and dropped from
  the IR; it is now `E-UNKNOWN-ATTR` with a did-you-mean, as every other logic
  tag already was. LSP completion offers exactly the permitted keys (T1-7).
- **Attribute values: `\"` is a quote, `'…'` is an error.** A `\"` inside a
  quoted value kept its backslash in the label; it is now stored as `"` (other
  escapes still reach CEL untouched). A single-quoted value (`label='"Hi."'`)
  silently kept its quotes; it is now `E-ATTR-QUOTE` (T1-16).
- **Component lines keep distinct `lineId`s after expansion.** A tagged line
  in a component `::use`d twice, or sharing its `(speaker, code)` with a line
  of the host scene (typically after `lute tag`), compiled to two records with
  one `lineId`/`voiceKey`. The expanded stream is now checked: `E-DUP-LINE-CODE`
  at the `::use`, from `lute check`, `check-project` and every compile (T1-10).
- **`<when is=… test=…>` covers only what both prove.** An arm with both was
  counted as covering its whole `is=` set, so a later arm on the same member
  drew a false `W-OVERLAP-ARMS`, and a match missing members was taken as
  exhaustive (no `E-NONEXHAUSTIVE`) while the runtime matched no arm. A guard
  the checker cannot decide now covers nothing (T1-12).
- **`W-LUTE-VERSION-STALE` compares versions as numbers** and, for a stamp
  newer than the toolchain, says to upgrade the toolchain rather than restamp
  (T3-6).
- **Component-body diagnostics point at the `::use`.** They were anchored at
  the host's frontmatter (1:1) with the component's absolute path; they now
  land on the first `::use` that brings the body in and name the component
  relative to the project. `{{p}}` for a declared param now says to write
  `{{@p}}`, and a line whose whole text is `@name` for a def or param draws the
  new warning `W-TEXT-LOOKS-LIKE-REF` (it ships the literal text) (T3-7).
- **`E-META-PARSE` stops that document's checks.** An unparseable frontmatter
  was followed by a dozen errors from checking the body against an empty
  environment, and by `E-CONN-UNKNOWN-NODE` in every other file that named the
  broken scene (T3-8).

### Changed

- **`check-project` runs the compile.** Every document that checks clean is
  compiled under its project's `identity:`, so compile-stage errors (such as
  `E-DUP-LINE-CODE` and `E-DUP-VOICEKEY`) fail `check-project` instead of only
  `compile` (T1-9, T1-10).
- **`lute check <file>` uses the project the file is in.** Without `--project`
  it checked the file with no manifest — no `defaults: uses:`, no profile — and
  reported `E-UNDECLARED`/`E-DOMAIN-UNKNOWN` for paths the project declares. It
  now applies the nearest `lute.project.yaml` and says so on stderr (`note:
  using project …`) (T3-9).
- **`lute scenario envelope`: Possible lists only what is not Guaranteed.**
  Possible is a superset of Guaranteed, so every guaranteed path was printed
  twice. The quest envelope's separate `Possible \ Guaranteed` inventory is now
  that same Possible table (T3-15).
- **`lute play` transcripts read like the source.** Lines keep their delivery
  (`@wren{mono}:`, `as=`), a `when=`-guarded line shows as itself or as
  `skip @maud "…" — when: false` instead of `match -> arm 1`/`otherwise`,
  compiler-injected staging (preloads, pose resets, `::bg` auto-hides) is left
  out, and menus mark options not offered (`piano✗`, `table(spent)`). `--json`
  line records carry `role`, `lineId`, `voiceKey`, `as` and `emotion`, and
  menu records `spent`/`ineligible`. `lute run`'s transcript (the conformance
  contract) is unchanged (T1-8, T3-2).

### Compatibility

- **Multi-scene projects on the default `voiceKey` template now fail.** The
  default `{speaker}-{code}` carries no scene prefix and every scene numbers
  its lines independently, so in practice every project with two or more
  scenes gets `E-DUP-VOICEKEY` from `check-project` and `compile --all` until
  its `lute.project.yaml` pins
  `identity: { voiceKey: "{prefix}.{speaker}-{code}" }`. Pinning renames
  every voice asset key, so re-export voice-asset manifests afterwards.
- **Incomplete traces fail tests.** A `lute test` case whose walk halts on a
  guard it cannot decide used to pass whenever it did not mention `exit:`; it
  now fails and names the `state:` / `facts:` entries that would decide the
  guard. Supply them, or write `expect: { exit: incomplete }` when stopping
  there is the point of the test. A test whose `file:` is a lore document
  fails with `E-TEST-LORE`.
- **Documents that were already wrong can redden.** New errors
  (`E-ATTR-DEF-DYNAMIC`, `E-INTERP-DEF`, `E-ATTR-QUOTE`), attribute closure on
  `<quest>` / `<objective>` / `<on>` (`E-UNKNOWN-ATTR`), the CEL profile gate
  on def bodies (`E-CEL-PROFILE` / `E-CEL-PARSE`), enum checking of `@def`
  component args (`E-COMPONENT-ARG`), `E-DUP-LINE-CODE` over expanded
  components, `E-NONEXHAUSTIVE` for an `is=` + `test=` arm that was wrongly
  counted as covering, and `check-project`'s compile and single-snapshot gate
  (`E-CAPABILITY-MISMATCH`) each fire only where the artifact was already
  wrong. `quest.<id>.state` reads lose their false `E-MAYBE-UNSET` /
  `E-UNSET-LITERAL`.
- **Additive IR.** The version strings move to `0.21.1`; the one shape change
  is the optional `placeholder.expr`, so `0.21.0` artifacts stay valid against
  the `0.21` schema and engines, gating on MAJOR, widen nothing. Some
  artifacts change content: a `<when is="unset">` arm on a quest state
  compiles to `== "unset"`, a `\"` in an attribute value is stored as `"`, and
  a def in a directive attribute that folds to a constant is written as that
  literal. `capabilityVersion` does not move; the tree-sitter grammar is
  unchanged.

## [0.21.0] - 2026-09-24

**Beats and occasions: story selection without a clock.**

The `0.11.0` schedule layer modelled one kind of game — a visual novel on a
day clock, every scene a placement at a tick on a lane. Most narrative games
do not advance by clock: at some moment (a hub visit, entering a room,
talking to an NPC, a new day, the start of a run) they pick one of the story
pieces whose conditions hold. An audit of the schedule found the rest: quest
progress could not gate a placement (`completed()`/`active()` read an empty
set), placement order was a second source of truth beside `after:`, and
recurring events were forbidden. `0.21.0` replaces it. The engine raises
**occasions**; a scene or lore entry becomes a **beat** by naming the occasion
it answers, with a `when`, a `priority`, and a repetition policy; Lute defines
which beats are eligible and which one wins. Spec:
[`docs/proposals/scenario-dsl/0.21.0.md`](docs/proposals/scenario-dsl/0.21.0.md);
engine contract:
[`docs/runtime/beats-and-occasions.md`](docs/runtime/beats-and-occasions.md).

### Added

- **Plugins — `occasions:` export** — a plugin manifest may declare the
  occasions its engine raises: `select: first` (default — present the single
  winner) or `select: all` (offer every eligible beat and let the player
  pick), `target: true` for an occasion raised *for* something (`talk` →
  `npc.achilles`), and an optional `description`. Folded into the capability
  snapshot as a guarded, sorted section (the `rewardKinds` precedent), so a
  project without it keeps its `capabilityVersion`. With no declaring plugin,
  occasion names are shape-only.
- **Language — scene beats** — scene frontmatter `on:` (the occasion),
  `target:` (a dotted id), `when:` (a CEL condition over `run`/`user`/`app`
  state, `quest.*`, `entry.<id>.read`, and fact queries — never the scene's
  own `scene.*`), `priority:` (integer, default `0`, higher wins), and
  `once: run | user | false` (default `run`). A beat is eligible when its
  `after:` and `when` both hold and its `once` is unspent. `when` joins the
  CEL-slot registry like a quest `start`. The keys are scene-only and never
  defaultable.
- **Language — entry beats** — `<entry on="…" priority="…">` beside the
  existing `when` / `target`. Entries have no `once`; an entry heard once
  guards on its own `entry.<id>.read`.
- **Diagnostics** — `E-BEAT-ATTR` (a malformed `on` / `target` / `priority` /
  `once`, beat keys without `on`, or a `target` on an untargeted occasion),
  `E-OCCASION-UNKNOWN` (an occasion no resolved plugin declares, once some
  plugin declares occasions), `E-BEAT-UNREACHABLE` (a scene beat whose `when`
  provably never holds — scalar conditions per file, fact conditions in
  `check-project` through the `0.20.0` fact envelope), and the `check-project`
  warning `W-BEAT-SHADOWED` (a `select: first` beat that can never win because
  an earlier-ordered, always-eligible, never-spent beat on the same occasion
  and target always does).
- **IR** — `SceneMeta.beat` (`{on, target?, when?, priority, once}`, `once` as
  `"run"` / `"user"` / `"none"`), `EntryCmd.on` / `priority`, and
  `ProjectIndex.beats` (every scene and entry beat in selection-tiebreak
  order: document path, then declaration order). All omitted when absent.
- **CLI — `lute play <dir> --script <play.yaml> [--json]`** — rebuilt on
  occasions. A script lists `steps:` (`{occasion, target?, pick?}` and
  `{newRun: true}` boundaries) plus `state:` / `facts:` seeds and `choose:`
  branch decisions in the trace-mock grammar. Each step prints every candidate
  beat with its verdict (`once`, `after:`, `when`), presents the winner — or
  the step's `pick` on a `select: all` occasion — through the same reference
  runner as `lute run`, and then advances every quest lifecycle. `newRun`
  resets `run.*` state, run-tier facts, and `once: run` spending. Exit `0`
  complete, `1` a failed project compile or an ineligible `pick`, `2` a usage
  error (malformed script, unknown occasion), `3` an incomplete walk.
- **LSP** — hover and completion for the `<entry>` `on=` / `priority=`
  attributes.
- **Docs** — [`docs/runtime/beats-and-occasions.md`](docs/runtime/beats-and-occasions.md)
  (candidates, eligibility, spending, selection order, presentation), and the
  website's *Beats* language page and *Playing a story* tooling page.
- **Language — quests meet scenes and occasions (dsl 0.21.0 §7a)** —
  `visited('<scene id>')` is legal in every condition slot (quest `start` /
  `fail`, objective `done`, beat and entry `when`, content-line and branch
  `when=`), so a scene advances a quest by being played, without a relay
  flag; an unknown id is `E-CONN-UNKNOWN-NODE`. `<objective on="<occasion>">`
  judges the objective only when that occasion is raised — the missing
  end-of-run check point. `::accept{quest="<id>"}` accepts an accept-driven
  quest from a scene; `E-ACCEPT-TARGET` rejects a missing, unknown, or
  `start=`-gated target. IR: `ObjectiveEntry.on`, command
  `{kind: "accept", quest}`; `visited()` slots carry `raw` only, like `holds()`.
- **CLI** — `lute trace` / `lute run --occasion <name>` (repeatable) and the
  mock / test keys `visited:` and `occasions:`; `lute test`
  `expect.quests: {<id>: unset | active | complete | failed}`; `lute play`
  occasion steps judge `on=` objectives, and an occasion only objectives
  reference is a legal step.
- **Language — def shorthand and `E-DEF-DECL` (dsl 0.21.0 §7b)** — a def
  may be written as its CEL body alone, `vesnaHasPaper: "holds(knows(vesna,
  manifest))"`, and the long form's `type:` is optional: an absent type is
  inferred from the body by the same closed procedure that types a `::set`
  (a body it cannot type asks for the long form; an explicit type must agree
  with the body's). `E-DEF-DECL` rejects every other shape — a non-string,
  non-mapping value, a mapping without a string `cel:`, a bad `type:`, an
  unknown key, `params:` without `type:` — inline and in an imported schema.
  This closes a gate hole: a def whose body `check` could not see (a bare
  string, before) passed `check` and then failed `compile`/`trace` with
  `E-COMPILE-EXPAND … (gate should have caught this)`. The published
  `lute.schema.json` def shape now matches (`cel` required; `min`/`max`/
  `values` — never read by the checker — removed).

### Changed

- **Start-less quests are accept-driven in `lute run` / `lute play`** — an
  unreferenced quest with no `start` stays `unset` until a mock `accepts:`
  entry or an `accept` record names it; it used to activate at walk start.
  This matches `lute trace` (dsl 0.4.0 §4.4) and `quest-lifecycle.md`, whose
  contradictory "activates at the start of the walk" line is corrected.
  Subquest children are unchanged.
- **`lute scenario` lists bare quests** — a quest without `after=` appears as
  `unanchored` (text, `roots[].unanchored` in JSON, a dashed DOT node) instead
  of vanishing; `scenario reach` on one reads `Unanchored — …` (token
  `unanchored`, formerly `reachable`).
- **`lute run` transcript** gains `accept` and `occasion` records; `lute trace`
  JSON gains the `accept` step.
- **`lute play` drives quest lifecycles** — after each presentation it
  advances every quest exactly as `lute run` does for a quest artifact
  (resuming carried status rather than restarting), so a later `when` over
  `quest.*` and an `after: completed(…)` see real progress. The
  schedule-era player never advanced quests.
- **`lute context`** lists the project's declared occasions (`occasions` in
  `--json`, with each occasion's `select` and `target`) beside the other
  vocabulary.

### Removed

- **`schedule.yaml`** — the project-file layer, its loader and route-space
  sweep, and the clock / lane / placement model. No project used it; a
  day-clock story is a `dayStart` occasion plus `when` over `run.day`.
- **Schedule-driven `lute play`** — the `--state`, `--fact`, `--choose`,
  `--auto`, `--lanes`, `--steps`, and `--coverage` flags go with it; seeds and
  decisions now live in the play script. The reference runner's `--auto first`
  hub fallback (auto-selecting the first eligible option once a scripted
  sequence ran out) is gone: an unscripted hub halts the walk incomplete.
- **Every schedule diagnostic** — `E-SCHED-AT-PARSE`, `E-SCHED-BUCKET-DUP`,
  `E-SCHED-CLOCK-OVERFLOW`, `E-SCHED-CLOCK-STRUCTURE`,
  `E-SCHED-CURSOR-DYNAMIC`, `E-SCHED-DOC-MISSING`, `E-SCHED-DOC-PATH`,
  `E-SCHED-EVENT-DUP`, `E-SCHED-GUARD-PARSE`, `E-SCHED-LANE-UNKNOWN`,
  `E-SCHED-SIZE-INVALID`, `E-SCHED-USER-OVERLAP`, `E-SCHED-VARIANT-AMBIG`,
  `E-SCHED-VARIANT-FORM`, `E-SCHED-VARIANT-GAP`, `W-SCHED-DOC-UNPLACED`,
  `W-SCHED-IDLE`, and `W-SCHED-ROUTESPACE-CAP`.
- **`docs/schedule-and-play.md`** and the website's *Schedule & play* pages
  (en + ko), replaced by *Playing a story*.

### Compatibility

- Additive IR: scene and lore artifacts without beats compile
  byte-identically apart from the version strings; the IR restamps to
  `0.21.0` and the schema file renames to
  [`schemas/lute-ir-0.21.schema.json`](schemas/lute-ir-0.21.schema.json),
  gaining `sceneBeat`, the entry fields, and the `indexBeat` row. Engines gate
  on MAJOR, so nothing widens; an engine without beat support ignores the new
  fields and reaches every scene by explicit flow.
- A project carrying a `schedule.yaml` keeps checking and compiling (the file
  is simply no longer read); scripts invoking `lute play` with the removed
  flags must move to `--script`.
- `capabilityVersion` moves only for a project that installs an
  `occasions:`-declaring plugin; the tree-sitter grammar is unchanged.
- `E-DEF-DECL` can redden a def that compiled before only when that def was
  already wrong: an unknown key was silently ignored, and a `type:` that
  disagrees with its body's decidable type (`type: string` over the CEL
  number `0010`) mistyped every `@ref` to it. A type-less long-form def, which
  used to be unchecked at its `@ref` sites, is now typed by inference, so an
  existing misuse of it can surface as `E-REF-TYPE`.

## [0.20.0] - 2026-09-24

> **First published toolchain since `0.17.2`.** The `0.18.0` and `0.19.0`
> entries below were never published as packages; their language and IR
> changes (range patterns in `<when is>`, `W-WHEN-TEST-LITERAL`, lore entries,
> document bundles) ship in this release together with fact envelopes.

**Fact envelopes: `check-project` decides relational guards.**

An author who writes a line that presumes knowledge guards it —
`@eris{when="holds(knows(player, project_lumen))"}` — and until now the checker
never looked at that guard. A relational query was always *undecided*: the
producibility walk asked only whether a relation **name** was asserted
somewhere, and only for quest `start`/`fail`, objective `done`, and entry
`when`. A line guard on a fact nothing ever asserts — a typo'd argument, a cut
scene, a lore entry never written — checked clean, while
`W-UNPROVEN-RELATIONAL` marked every other relational gate "not proven", noise
on exactly the guards that were fine. `0.20.0` computes, for every
`holds(…)` / `count(…)` in every guard slot, whether the queried facts are
**impossible**, **guaranteed**, or **possible** there. Spec:
[`docs/proposals/scenario-dsl/0.20.0.md`](docs/proposals/scenario-dsl/0.20.0.md).

### Added

- **Language — the may set** — a project-wide, argument-level
  over-approximation of every ground fact that can be live: `facts:` seeds,
  every `::assert` in a document not proven unreachable (scenes, quest bodies,
  lore entries), every fact of a reserved relation, and the rules' closure
  over them (negated atoms and rule-body CEL guards read as satisfiable).
- **Language — the must set** — a path-sensitive under-approximation of the
  facts live on every route to a slot: a forward must-dataflow within each
  document (branch/match joins intersect, a hub is a greatest fixpoint,
  `::end`/`::next` route their set), propagated across the `after:` graph
  with the scalar envelope's recursion (`visited(A)` contributes `A`'s exit,
  `&&` unions, `||` intersects). Only monotone facts cross a document
  boundary — no matching `::retract`, no `key:` conflict, not reserved or
  engine-open, not `tier: scene`/`tier: quest`. Guards are assumptions inside
  their regions; quest and entry bodies start from the seeds plus their own
  guards. `count(P)` is decided over the interval `[|Must ∩ P|, |May ∩ P|]`.
- **Checker — verdict plumbing** — the verdict is substituted into `decide`,
  so every slot that already reports a provably dead or always-true condition
  now does so for relational ones, with messages that name the fact and the
  reason (the assert site, seed, rule, or enclosing guard). Project-level
  only: single-file `lute check` leaves relational queries undecided.
- **Diagnostics — `E-ENTRY-UNREACHABLE`** — a lore entry `when` that provably
  never holds (the one guard slot without a dead-code diagnostic); it also
  fires for a scalar-decidable `when` in single-file `lute check`.
- **Diagnostics — `W-FACT-GUARANTEED`** — a relational query inside a guard
  (line `when=`, `<choice when>`, `<when test>`, entry `when`) that holds on
  every route to it: the condition is redundant. Quest `start`/`fail` and
  objective `done` are predicates, not guards, and are not flagged.
- **CLI — `lute scenario <dir> envelope <node>`** prints a *Guaranteed facts*
  table (each fact with what establishes it) beside the scalar tables;
  `--format json` carries it as `envelope.guaranteedFacts`
  (`[{fact, establishedBy}]`).

### Changed

- **Dead relational guards reuse each slot's code** — `E-ARM-DEAD`
  (`<when test>`, `<choice when>`, content-line `when=`, `::next` guard),
  `E-OBJECTIVE-UNSATISFIABLE`, `E-QUEST-UNREACHABLE`, `W-OBJECTIVE-HIDDEN`.
- **Examples** — the four guards `W-FACT-GUARANTEED` found in `docs/examples`
  are removed (haven `purser.lute` `listTheMass`; investigation
  `crime-scene.lute`'s derived `points(blake)` line and `interview.lute`'s two
  hub choices), with their tests, READMEs, and the website tutorial updated;
  the `lute init --template investigation` scaffold drops the same two
  guards. `check-project docs/examples` reports no warnings.

### Removed

- **`W-UNPROVEN-RELATIONAL`** — its premise, that relational gates are
  unanalyzable, no longer holds, and a *possible* gate is the normal state of
  a guard. Following the `W-INJECT-CONFLICT` (0.10.0) precedent the code
  leaves the deny registry: `--deny W-UNPROVEN-RELATIONAL` is a usage error
  (exit `2`).

### Compatibility

- No grammar or IR change: every 0.19.x document parses and compiles
  identically; the IR restamps to `0.20.0` and the schema file renames to
  [`schemas/lute-ir-0.20.schema.json`](schemas/lute-ir-0.20.schema.json)
  (`$id` and title only). Engines gate on MAJOR, so nothing widens.
- `check-project` may report new errors on guards that could never hold —
  already dead at runtime; the diagnostic is new, the bug is not — and new
  `W-FACT-GUARANTEED` warnings on redundant guards.
- Pipelines passing `--deny W-UNPROVEN-RELATIONAL` must drop it.
- `capabilityVersion` is unchanged; the tree-sitter grammar is unchanged.

## [0.19.0] - 2026-09-24

**Lore entries: content the engine looks up instead of plays.**

Scenes and quests put a story on a **time axis** — what happens, in what
order, under which conditions. Much of a game's story sits in **space and
objects** instead: the torn page in the lab, the inscription on a door, the
key whose description changes after the fire, the line an NPC mutters as you
walk past. The player finds these in any order and reads them again, and the
only way to write one was a scene per note — scheduled, sequential, played
once. `0.19.0` adds a third document kind, `kind: lore`, whose `<entry>`
declarations say **what** the text is, **when** it is eligible, and **what**
reading it changes; the engine still decides **where** it lives — which item
spawns where, which panel shows the codex, when an NPC barks. Spec:
[`docs/proposals/scenario-dsl/0.19.0.md`](docs/proposals/scenario-dsl/0.19.0.md);
engine contract: [`docs/runtime/lore-entries.md`](docs/runtime/lore-entries.md).

### Added

- **Language — `kind: lore` and `<entry>`** — a lore document takes the
  quest-document frontmatter keys and a body of one or more top-level
  `<entry>` declarations and nothing else (a `# ` heading, `## ` shot,
  `<quest>`, or loose content is `E-GRAMMAR-NOT-ADMITTED`). `<entry>`
  carries `id` (required, project-unique), `target` (a dotted id such as
  `item.rusty_key`, shape-only), `category` (an ident such as `note` or
  `bark`, shape-only), `title` (localized like a quest title), `series` /
  `order`, and a `when` eligibility guard that joins the CEL-slot registry.
  Several entries may share a `target`.
- **Language — entry bodies** — content lines, `<match>` (recursively),
  `::set`, `::assert`, and `::retract` only; `<branch>`, `<hub>`,
  `<timeline>`, `<on>`, `<objective>`, and every `::` directive are
  `E-GRAMMAR-NOT-ADMITTED`. Each entry is its own lineId / voiceKey / code
  scope, as each quest is. Revealing knowledge is the ordinary `::assert`
  against the project's relations, checked by the usual arity/domain/tier
  rules, and scenes react through `holds(…)`.
- **Language — `entry.<id>.read`** — a reserved, engine-written `bool`
  (default `false`, run tier) readable from any CEL slot in any document
  kind: series gating (`when="entry.scientistLog1.read"`), a scene guard, a
  quest objective, a `<match on>` subject. Writing it is rejected, as
  writing `quest.*` is.
- **Runtime contract — first-read effects** — presenting an entry runs its
  body against live state; `::set` / `::assert` / `::retract` apply only
  while `entry.<id>.read` is `false`, after which the engine sets it.
  Re-reading shows the (possibly different) text and changes nothing else.
- **Language — document bundles** — a quest or lore document MAY declare a
  document `id:` (the scene `id:` shape) naming the file as a bundle
  (`haven.purserLedger`, `haven.mainChain`); it becomes `meta.id` and the
  document's `ProjectIndex` key. A lore document MAY declare `series:`,
  making every entry one series ordered by position in the file (1-based);
  a non-ident value is `E-META-VALUE`. Per-entry `series=` / `order=` stay
  for series spanning files.
- **Diagnostics** — `E-ENTRY-ATTR` (attribute shape, `order` without
  `series`, or a per-entry `series=` / `order=` in a document declaring
  `series:`), `E-ENTRY-ID-DUP` (per document in `check`, project-wide in
  `check-project`), `E-ENTRY-SERIES-ORDER` (a duplicate resolved
  `(series, order)`), and `W-ENTRY-REF-UNKNOWN` (`check-project`: an
  `entry.<id>.read` naming an id no document declares). `E-META-ID` now
  covers a quest or lore document's `id:`; `E-CONN-EPISODE-ID-DUP` covers
  quest and lore document ids, which share one namespace with scene ids,
  and its message says "document id". `E-UNKNOWN-KIND` admits `lore`.
- **IR** — artifact `kind: "lore"` with `LoreMeta` (`id?`, `title?`,
  `series?`, `contentLang?`, `extra?`, `plugin?`); a new `entry` command
  record (`addr`, `id`, `target?`, `category?`, `title?`, `titleLineId?`,
  the resolved `series?` / `order?`, `when?`, and `body`, the address of its
  body segment in the `OnCmd.body` convention); optional `QuestMeta.id`;
  `ProjectIndex.entries` (one row per entry, document order, omitted when
  empty), and a quest or lore document with an authored `id:` is indexed
  under it. The schema renames to
  [`schemas/lute-ir-0.19.schema.json`](schemas/lute-ir-0.19.schema.json)
  and gains `loreMeta`, `entryCmd`, `questMeta.id`, and `indexEntry`.
- **CLI — `lute trace <doc> --entry <id>`** presents one entry against
  mocked state: its lines, the `<match>` arm taken, and the effects a first
  read applies — or skips, when the mock seeds `entry.<id>.read: true`.
  Required for a lore document; `E-TRACE-ENTRY` on a non-lore document or
  an unknown id. **`lute run <artifact> --entry <id>`** does the same over
  a compiled lore artifact (exit `2` without it, or on another kind).
- **CLI — `lute lore <dir> [--json]`** — the world-narrative map: entries
  grouped by `target` and by `series` (in `order`), and for every asserted
  ground fact whether lore entries, scenes/quests, or both reveal it.
- **CLI — `lute new lore <name>`** scaffolds `lore/<name>.lute` with one
  `<entry>`.
- **Editors and tooling** — tree-sitter gains a top-level `entry`
  production with fold, highlight, and tag queries (nvim mirrors); the LSP
  covers `<entry>` in document symbols, folding, semantic highlighting, and
  attribute completion and hover. `lute tag` and `lute loc` walk entry
  bodies (each entry its own identity scope), and `lute compile --all`
  writes `ProjectIndex.entries`; `lute lint` excludes lore documents from
  scene metrics and counts their lines as translatable content.
- **Conformance — `lore-entry` and `lore-entry-reread`** — one entry
  presented on its first read (effects applied) and on a re-read (effects
  skipped); the harness passes the entry id from each fixture's
  `entry.txt`.
- **Examples — `docs/examples/haven/lore/`** — a `series:` bundle (the
  purser's ledger) whose pages reveal facts with `::assert` and gate on
  `entry.<id>.read`, a place inscription selecting text by state, and an
  NPC bark gated on `holds(knows(…))`.

### Changed

- **`lute new quest`** now writes a namespaced document `id:`
  (`quest.<ident>`), as `lute new lore` does (`lore.<ident>`).
- **`W-META-LEGACY` is scene-only** — it fires for a scene identity key
  beside an authored `id:`; on a quest or lore document those keys are
  already `E-META-UNKNOWN-KEY`, so the new document `id:` never draws it.

### Compatibility

- Every 0.18.x document is a valid 0.19.0 document: `kind: lore` was
  `E-UNKNOWN-KIND`, `entry.*` paths were undeclared, and `id:` in a quest
  document was `E-META-UNKNOWN-KEY`.
- IR: additive. Scene artifacts, and quest artifacts without an authored
  `id:`, are byte-identical apart from the version strings, and a project
  without lore writes no `entries` key. Engines gate on MAJOR, so a
  scene/quest consumer is unaffected; one without lore support rejects
  `kind: "lore"` as it rejects any unknown artifact kind.
- `capabilityVersion` is unchanged (no core vocabulary is added); the
  tree-sitter grammar regenerates.

## [0.18.0] - 2026-09-24

**Numeric thresholds get a pattern form; literal guards move onto `is=`.**

`<when is="…">` has always been the arm form the checker can reason about —
exhaustiveness, typo detection against the subject's domain, dead-arm
proofs — while `test="…"` is an opaque CEL guard. But numeric thresholds
(`$ >= 2`) had no pattern form, and the examples taught `test="$ == 'gold'"`
for plain literal comparisons, so the most common arm shape paid CEL's
quoting cost and the checker saw less than it could. `0.18.0` adds inclusive
numeric ranges to `is=`, gives `number` subjects real interval coverage, and
ships a warning plus a mechanical `lute fix` that moves literal comparisons
onto `is=`. Spec:
[`docs/proposals/scenario-dsl/0.18.0.md`](docs/proposals/scenario-dsl/0.18.0.md).

### Added

- **Language — range literals in `<when is>`** — `N..M`, `N..`, `..M`, both
  bounds inclusive, signed decimal bounds (`-3..-1`, `0.5..1.5`), freely
  mixed with other literals (`is="..0 | 10.."`). An alternative containing
  `..` is always a range, never an enum member. Lowers to the existing
  `>=` / `<=` / `&&` operators in `MatchArm.expr`.
- **Language — number-domain coverage** — a subject declared `number`
  (schema decl or component param) is covered by the union of its arms'
  intervals over the reals. `E-NONEXHAUSTIVE` names the first uncovered gap
  (`..0` + `1..` leaves `(0, 1)`); `..0` + `0..` is exhaustive without
  `<otherwise>`. `E-ARM-DEAD` catches an arm inside earlier unguarded
  coverage (`1..5` then `2..4`), `W-OTHERWISE-DEAD` a redundant
  `<otherwise>`. A partially overlapping range does not warn — `3..` then
  `1..` is the descending-threshold cascade; a covered point literal still
  warns `W-OVERLAP-ARMS`.
- **Language — `E-WHEN-RANGE`** — a malformed (`..`, `a..b`, `1...2`,
  `1..2..3`) or empty (`3..1`) range literal, anchored at the literal.
- **Language — `W-WHEN-TEST-LITERAL`** — a `<when>` without `is=` whose
  `test` is exactly `$ == L`, `$ in [L, …]`, `$ >= N`, `$ <= N`, or a
  `$ >= A && $ <= B` pair (either operand order). The warning carries a
  `migrate` fixit, so the LSP offers it as a quick fix and **`lute fix`
  applies it** (`test="$ in ['silver', 'bronze']"` →
  `is="silver|bronze"`, `test="$ >= 2"` → `is="2.."`). Only literals that
  round-trip through the `is=` classifier are rewritten — `'1'`, `'true'`,
  `'a b'` stay guards. The guard form remains valid.
- **Conformance — `match-range`** — a numeric subject selecting a range arm
  on its inclusive upper bound.
- **Editors** — the tree-sitter `when_literal` token lexes every range
  shape; LSP hover on an `is=` value over a `number` subject names the
  number domain and the inclusive-range rule.

### Changed

- **Examples and docs use `is=` for literal arms** — every living example
  and website page was migrated with `lute fix`; the frozen proposal stack
  and historical plans are untouched. Compiled snapshots of migrated
  examples show an empty debug `MatchArm.test` for rewritten arms and an
  `==`/`||` tree where `$ in [...]` used to lower to `in`; behavior is
  identical.
- **One `is=` classifier** — the checker, compiler, component folding, and
  `lute trace` now share `lute_syntax::is_pattern`, so a literal cannot mean
  one thing statically and another at runtime. Side effect: `lute trace` and
  component folding no longer match a string subject against a
  number-looking or `true`/`false` literal, matching what compiled
  artifacts always did.
- **`lute fix` output** — reports `applied N fix(es)` / `nothing to fix`
  instead of the stale `migrated … to 0.2.2`, since it now carries rules
  from several releases.

### Compatibility

- Every 0.17.x document is a valid 0.18.0 document; no existing literal
  changes meaning. New diagnostics on existing documents are the
  `W-WHEN-TEST-LITERAL` warning and, on `number` subjects only, newly
  provable dead/overlapping point arms (`1` and `1.0` are now one point).
- IR: no shape change — the version restamps to `0.18.0` per the
  alignment rule and the schema file renames to
  `schemas/lute-ir-0.18.schema.json`. Engines gate on MAJOR, so nothing
  widens.
- `capabilityVersion` is unchanged; the tree-sitter grammar regenerates.

## [0.17.2] - 2026-09-18

**Plugin owner metadata for passthrough IR records.**

### Added

- **Plugin owner identity in `kind: "plugin"` records** — passthrough plugin
  commands now carry the resolved owning package id in the optional `plugin`
  field. Hosts can dispatch and diagnose extension records by `(plugin, tag)`.
- **Typed projection fixture** — generic presentation directives prove that
  declarative directives lower to core `background`/`sprite` records while
  host-owned directives remain typed plugin records.
- **Normative specs** — plugin-system `0.0.7` defines owner metadata and
  scenario DSL `0.17.2` records the additive IR contract.

### Compatibility

- The source grammar and static semantics are unchanged.
- `plugin` is append-only on `cmdPlugin`; consumers that ignore unknown fields
  remain compatible.
- `capabilityVersion` is unchanged. Plugin ids and versions already identify
  the active owner set.

## [0.17.1] - 2026-09-21

**Neutral public history.**

Toolchain-only alignment restamp. The repository history was rewritten so
that every example, conformance fixture, snapshot, and design note uses
neutral, self-contained names: example cast and project identifiers were
renamed byte-length-preserving (every span, column, and snapshot is
unchanged), and one internal adoption assessment that was never a normative
source (`docs/adoption/`) was dropped along with references to it. Neither the
language nor the IR earns the move; both restamp per
[`docs/versioning.md`](docs/versioning.md)'s alignment rule.

### Changed

- **Examples and fixtures use neutral identifiers** — `docs/examples/`,
  `conformance/`, and the compile snapshots carry renamed cast/project ids.
  Behavior of every command is unchanged; the rename is same-length so the
  checker's byte spans and the e2e snapshots are byte-for-byte stable.
- **IR is a pure restamp** — no field added, renamed, moved, or retyped.
  `schemas/lute-ir-0.17.schema.json` keeps its name and `$id`; a `0.17`
  engine parses a `0.17.1` artifact unchanged.
- **Language is a pure restamp** — `LUTE_LANG_VERSION` advances to `0.17.1`
  so `W-LUTE-VERSION-STALE` fires on a document stamped `0.17.0` (mechanical
  fix: restamp to `0.17.1`). Spec:
  [`docs/proposals/scenario-dsl/0.17.1.md`](docs/proposals/scenario-dsl/0.17.1.md).
- `capabilityVersion` does NOT move this release.
- Version re-alignment per [`docs/versioning.md`](docs/versioning.md):
  toolchain, language, and IR all present `0.17.1`.

### Removed

- `docs/adoption/` (internal adoption assessment; not a normative source).

## [0.17.0] - 2026-09-14

**Checked continuations and least-authority compilation.**

This release adds a checked, append-only continuation compiler and generic
capability-permission ceilings. Both features reuse the existing language and
ordinary artifact contract: streaming recompiles accepted source through the
whole-document pipeline, while permissions can only narrow the capabilities a
resolved document may use. The language and IR axes therefore move to `0.17.0`
as alignment restamps with no grammar, static-semantic, or IR-shape change.

### Added

- **Checked streaming continuation compiler** —
  `lute_compile::streaming::ContinuationCompiler` accepts a resolved, checked
  scene template and append-only ordinary Lute shot-body text, then emits a
  complete ordinary `Artifact` snapshot for each accepted top-level unit.
  Each unit passes through the existing checker, normalization, component
  expansion, stage injection, lowering, and addressing pipeline. Updates carry
  a monotonic `sequence` and `append_from`; prior commands and state entries
  must remain semantically unchanged, with `E-STREAM-PREFIX-CHANGED` rejecting
  retroactive changes. Normative contract:
  [`docs/proposals/scenario-dsl/0.17.0.md`](docs/proposals/scenario-dsl/0.17.0.md);
  implementation design:
  [`docs/superpowers/specs/2026-09-14-streaming-continuation-compiler-design.md`](docs/superpowers/specs/2026-09-14-streaming-continuation-compiler-design.md);
  runtime guide:
  [`docs/runtime/incremental-continuations.md`](docs/runtime/incremental-continuations.md).
- **`lute compile-stream` NDJSON transport** —
  `lute compile-stream <scene.lute> [--project DIR] [--providers DIR]` resolves
  the host-owned template once, reads body text from stdin, and flushes
  `start`, every accepted `update`, then `finish` or `error`. EOF finalizes a
  complete final leaf; exit `0` means successful finalization, `1` means
  syntax, semantic, or service rejection, and `2` means invocation, I/O, or
  invalid UTF-8. The lower-level
  [`lute_syntax::incremental::IncrementalContinuationParser`](crates/lute-syntax/src/incremental.rs)
  remains available for lossless syntax framing without checking or artifact
  production.
- **Generic capability permissions** — trusted `lute.project.yaml` root and
  profile policy can restrict directives, scalar-state writes, fact writes,
  bridge `service/operation` pairs, declarative rewards, and quests. Missing
  fields are unrestricted; explicit empty lists deny the category. Project,
  `global`, ancestor, selected-profile, and host layers compose
  conjunctively. `--permission-profile NAME` adds an independently trusted
  ceiling to `check`, `compile` (including all-or-nothing `--all`),
  `compile-stream`, and `context` without activating plugins or rewriting the
  source-selected profile. Non-suppressible `E-PERMISSION-*` diagnostics
  cover defaults, seed facts, plugin effects and bridges, nested/transitive
  components, quests, and rewards; the compiler rechecks policy before
  lowering. Normative contract:
  [`docs/proposals/plugin-system/0.0.6.md`](docs/proposals/plugin-system/0.0.6.md);
  security and host guide:
  [`docs/runtime/capability-permissions.md`](docs/runtime/capability-permissions.md);
  implementation design:
  [`docs/superpowers/specs/2026-09-14-capability-permissions-design.md`](docs/superpowers/specs/2026-09-14-capability-permissions-design.md);
  runnable example:
  [`docs/examples/capability-permissions/`](docs/examples/capability-permissions/).

### Changed

- **Release-axis alignment** — the toolchain, language, and IR versions all
  advance to `0.17.0` under
  [`docs/versioning.md`](docs/versioning.md). Language `0.17.0` is
  byte-for-byte `0.16.0` grammar and static semantics; IR `0.17.0` has no
  field, command, or serialization-shape change.
- **IR schema restamped per release line** —
  `schemas/lute-ir-0.16.schema.json` is renamed to
  [`schemas/lute-ir-0.17.schema.json`](schemas/lute-ir-0.17.schema.json), with
  `$id` and title updated and the schema body otherwise unchanged. The runtime
  gate remains MAJOR-only, so consuming engines require no schema migration.
- **Capability snapshot hashing includes restrictive policy** — an effective
  permission ceiling participates in `capabilityVersion`, while unrestricted
  projects retain their existing capability hash byte-for-byte.

## [0.16.0] - 2026-09-01

**Rewards become data.**

Conditions have been first-class, statically checked surface since the
scene kind shipped; rewards were only operational — plugin `::grant`
directives buried in `<on questComplete>` bodies. Execution was correct
(exactly-once, quest-scoped since 0.14.0), but the artifact carried
rewards as commands inside a handler, so nothing read "this quest's
rewards" as data: no journal preview at accept time, no balancing
extraction, no reward-shaped lint, and a conditional reward equally
invisible. `0.16.0` closes the half that was missing. `<reward/>` — a
self-closing element, direct child of `<quest>` or `<objective>`, with
`kind` / `target` / `amount` (integer scalar or the new `N..M` range
literal, negatives legal) / `when` (ordinary CEL slot) / quest-only `on`
(`complete` default, or `failed`) — lowers to pure data on the owning
records (`QuestCmd.rewards` / `ObjectiveEntry.rewards`), and the
reference runner and `lute trace` emit deterministic `grant` transcript
events at each fresh transition (spec §3 D-D). The engine still grants;
the language never rolls a range or synthesizes a `::grant`.

### Added

- **Language — declarative `<reward/>` element on `<quest>` /
  `<objective>`** — a self-closing owner field, direct child of the
  enclosing quest or objective; anywhere else, a `<reward>` with a body,
  or an unknown attribute in its closed set is rejected through the same
  per-tag closure that catches every other misplaced construct
  (`E-UNKNOWN-ATTR`, 0.10.0 §D-J). Attributes: `kind` (required
  non-empty string, the vocabulary key), `target` (optional string, the
  rewarded id — item / currency / quest / …), `amount` (optional integer
  **or** range literal `N..M` with integer bounds and `N <= M`; default
  `1`; negatives are real deductions), `when` (optional `CelString`,
  evaluated at the grant instant; joins the CEL-slot registry with
  `E-CEL-PROFILE` / `E-MAYBE-UNSET` / unset-sentinel guards / LSP hover /
  fill), and quest-level `on` (`complete` default or `failed` — which
  terminal transition grants it; on an objective-level entry `on` is
  rejected). Range amounts are declarations, not rolls: the journal
  shows "1–5", a balancer computes expectation, and the reference
  runtime never rolls — dice belong to the engine (0.0.1). Spec:
  [`docs/proposals/scenario-dsl/0.16.0.md`](docs/proposals/scenario-dsl/0.16.0.md).
  Design record: [`docs/superpowers/specs/2026-09-01-lute-reward-design.md`](docs/superpowers/specs/2026-09-01-lute-reward-design.md).
- **Language — `E-REWARD-ATTR`** — shape violation, anchored at the
  offending attribute: empty `kind`, malformed `amount` (non-integer,
  bad range, `N > M`), `on=` on an objective-level reward, or an `on`
  value outside the closed `complete` / `failed` enum.
- **Language — `E-REWARD-KIND`** — vocabulary violation when the
  resolved capability set declares any `rewardKinds:`: an unknown
  `kind`, a `target` present/absent against the kind's contract, or a
  `target` failing its provider domain. Stale snapshots degrade to the
  usual *catalog-stale* grade, never a hard error. With no
  `rewardKinds:` declared, shape checks stand alone and any kind name
  admits.
- **Manifest — `rewardKinds:` plugin export** — a map of kind id →
  declaration: optional `target` contract naming a provider domain (the
  same `providerRef` pattern directive attrs use — resolved against
  pinned snapshots, Fresh / *catalog-stale* / unknown-id) plus optional
  extra attr schema for game-specific slots. Folded into the capability
  snapshot as a guarded, sorted section: a populated vocabulary moves
  `capabilityVersion`, while the empty core section hashes
  byte-identically — no restamp for projects without a
  `rewardKinds:`-declaring plugin.
- **IR — `QuestCmd.rewards` and `ObjectiveEntry.rewards`** — new arrays
  of `RewardEntry`, `skip_serializing_if = "Vec::is_empty"`, so a
  rewardless artifact stays byte-identical to `0.15.1` output.
  `RewardEntry` serializes `kind`, optional `target`, exactly one of
  `amount` XOR (`amountMin` + `amountMax`) after amount defaulting
  (unauthored → `amount: 1`), optional `when` (`CelPair`), and `on:
  "failed"` only on a quest-level entry (`on: "complete"` is the
  omitted default; never emitted on objective entries). Range amounts
  are carried verbatim on the wire (spec D-C: never pre-rolled).
- **Runtime — deterministic `grant` transcript events** — `lute run` /
  `play` and `lute trace` emit a structured grant event at each fresh
  transition (spec §3 D-D): at `→ complete` the quest's `on="complete"`
  rewards, at `→ failed` (including cascade child-failure, 0.14.0) its
  `on="failed"` rewards, and when an objective first becomes `done`
  that objective's rewards. Each reward grants **at most once per quest
  instance** (same monotonicity as objective bodies); `when` is
  evaluated at the grant instant against the transition's own
  state/fact snapshot, a non-`true` verdict skips the reward once (not
  re-armed); order within one owner is declaration order, and when one
  step settles both an objective and its quest, objective grants
  precede quest grants — all grants precede the corresponding lifecycle
  handler body, so handler narrative can react to what was just
  granted. Range amounts pass through as
  `amountMin`/`amountMax` unchanged, keeping reference output
  byte-deterministic; the engine resolves `N..M` however it likes.
- **Grammar — tree-sitter `reward` production** — the `<reward/>`
  element joins the closed tag set (tags are enumerated), so
  editor grammars see it after regeneration (`tree-sitter generate`).

### Changed

- **Schema renamed per release line** —
  `schemas/lute-ir-0.15.schema.json` is renamed to
  `schemas/lute-ir-0.16.schema.json`;
  its content gains the `rewardEntry` definition (`kind` required,
  optional `target`, exactly one of `amount` / (`amountMin` +
  `amountMax`), optional `when`, quest-only `on: "failed"`) and the two
  `rewards` arrays on `questCmd` and `objectiveEntry`. `$id` and title
  updated to match. Under the `0.13.0` MAJOR-only gate the rename
  tracks the published release line for strict validators only; no
  engine gate widens, so a `0.15` engine parses a `0.16.0` artifact
  unchanged and simply does not ask for the added arrays.
- **`docs/runtime/quest-lifecycle.md` gains a Rewards section** — the
  when / `when=` / order / range rules the engine now grants against
  (§3 D-D), added to the normative runtime contract so a consuming
  engine has one place to read the semantics from.
- **Example corpus gains reward coverage** — quest fixtures under
  `docs/examples/` grow `<reward/>` declarations exercising the range
  literal, conditional `when=`, `on="failed"`, and objective-level
  entries; scene fixtures are untouched.
- `capabilityVersion` does NOT move this release for any project
  without a `rewardKinds:`-declaring plugin: the new snapshot section
  is guarded and its empty core value hashes byte-identically. The
  tree-sitter grammar gains one production — a grammar regeneration,
  not a capability restamp (the stamp tracks the core snapshot).
- Version re-alignment per [`docs/versioning.md`](docs/versioning.md):
  toolchain, language, and IR all present `0.16.0`.

## [0.15.1] - 2026-09-01

**LSP joins the npm distribution.**

Toolchain-only alignment restamp. The npm launcher `@lute-lang/lute`
now ships `lute-lsp` alongside `lute`: a second `bin` entry
(`packages/cli/lsp-bin.js`) resolves the platform-specific core package
and dispatches to its `lute-lsp` executable, so downstream editors can
spawn the language server through the same install the CLI already uses
(a `bunx @lute-lang/lute-lsp` route, symmetric with `bunx @lute-lang/lute`).
Every per-platform core package (`@lute-lang/lute-core-darwin-arm64`,
`@lute-lang/lute-core-linux-x64`, `@lute-lang/lute-core-win32-x64`) now
carries both binaries, and the publish/build-native workflow matrices
build `-p lute-lsp` next to `-p lute-cli`. Neither the language nor the
IR earns the move; both restamp per
[`docs/versioning.md`](docs/versioning.md)'s alignment rule.

### Changed

- **npm distribution ships `lute-lsp` alongside `lute`** — the
  `@lute-lang/lute` launcher gains a second `bin` entry, `lute-lsp`,
  backed by `packages/cli/lsp-bin.js` (the CLI launcher's
  platform-resolution logic reused, dispatching to the same
  `@lute-lang/lute-core-<platform>` package). The per-platform core
  packages now carry both `lute` and `lute-lsp` binaries; `publish.yml`
  and `build-native.yml` build both under one release matrix. No
  behavior change to either binary — this is packaging only.
- **IR is a pure restamp** — no field added, renamed, moved, or
  retyped. `schemas/lute-ir-0.15.schema.json` keeps its name and `$id`
  (the file's `0.15.x` title already covers `0.15.1`, and the schema is
  byte-identical to `0.15.0`); no engine gate widens under `0.13.0`'s
  MAJOR-only runtime contract, so a `0.15` engine parses a `0.15.1`
  artifact unchanged.
- **Language is a pure restamp** — no grammar production, no
  static-semantic rule, no diagnostic added or removed;
  `LUTE_LANG_VERSION` advances to `0.15.1` so `W-LUTE-VERSION-STALE`
  fires on a document stamped `0.15.0` (mechanical fix: restamp to
  `0.15.1`; a `0.15.0`-clean document restamped to `0.15.1` checks
  clean with no other edit). Spec:
  [`docs/proposals/scenario-dsl/0.15.1.md`](docs/proposals/scenario-dsl/0.15.1.md).
- `capabilityVersion` does NOT move this release: no `lute.core`
  export changes, no grammar production change.
- Version re-alignment per [`docs/versioning.md`](docs/versioning.md):
  toolchain, language, and IR all present `0.15.1`.

## [0.15.0] - 2026-09-01

**Scenes get to name themselves.**

The canonical scene key was the derived join `{character}.{episodeId}`,
and three required frontmatter keys had degenerated into an opaque id
authored in three pieces, two forced to be integers. `0.15.0` adds one
opt-in scene frontmatter key — `id:` — that IS the canonical scene key
wherever the derived join was consumed today, demotes the four legacy
identity keys to optional when it is present, and lands a free descriptive
`extra:` block on scene and quest roots for anything the language never
read anyway. No grammar production changes (frontmatter is one opaque
token), `capabilityVersion` does not move, and untouched corpora keep
every lineId, translation, `visited()` reference, and `schedule.yaml`
behavior — their artifacts differ from the 0.14.0 output only by the
added `meta.id` line.

### Added

- **Language — authored canonical scene id via `id:`** — one scene-only
  frontmatter key that IS the canonical scene key everywhere the derived
  `{character}.{episodeId}` join is consumed today: the lineId prefix and
  the structural choice/hub option ids, `visited()` targets (including the
  nearest-key suggestion on `E-CONN-UNKNOWN-NODE`), connectivity nodes,
  reachability, project-wide duplicate detection, `prereqEdges[].node`,
  the document key in `project.index.json`, and the runtime visited-set
  entry `lute play` records after presentation. Value is non-empty and
  matches `[A-Za-z0-9_.-]+` (`.` admits namespacing like
  `haven.s01ep01`); anything else is **`E-META-ID`**. When `id:` is
  present the four legacy identity keys (`character` / `season` /
  `episode` / `episodeId`) are no longer required and **`E-META-MISSING`**
  fires only when the defaults-merged frontmatter lacks `id:`. Authored
  and derived keys share one namespace and one project-wide uniqueness
  rule: **`E-CONN-EPISODE-ID-DUP`** keeps its code (tooling stability)
  but its message now names the *canonical scene id*, and an occurrence
  contributed by an authored `id:` anchors at that key. `id` is not
  defaultable (a manifest supplying one to many documents can only
  manufacture collisions), so it stays outside `defaults:`' closed key
  set. When absent, every site derives `{character}.{episodeId}` exactly
  as 0.14.0 — the fallback is the existing code path, so an untouched
  project's lineIds, translations, `visited()` references, and
  `schedule.yaml` behavior do not move. Spec:
  [`docs/proposals/scenario-dsl/0.15.0.md`](docs/proposals/scenario-dsl/0.15.0.md).
  Design record: [`docs/superpowers/specs/2026-09-01-lute-scene-id-design.md`](docs/superpowers/specs/2026-09-01-lute-scene-id-design.md).
- **Language — free descriptive `extra:` block on scene and quest roots**
  — one reserved frontmatter key holding an open mapping (keys free,
  values scalars or flat scalar-lists; a nested mapping or a non-scalar
  list entry is **`E-META-VALUE`**), legal on scene and quest roots and
  rejected on schema/component documents through the existing kind gate.
  `extra:` carries **no language semantics**: not readable from CEL, not
  a state path, not a template token, consulted by no checker, compiler,
  or runtime rule. It is serialized verbatim into `meta.extra` (omitted
  when empty) so external tooling — search, TMS, editors — can key on
  it, and the top level stays closed (`E-META-UNKNOWN-KEY` keeps
  catching top-level typos). `extra` joins `defaults:`' closed key set;
  0.10.0 §6.2's whole-value-per-key rule applies unchanged.
- **Language — `W-META-LEGACY`** — a document that authors `id:` and
  ALSO authors any of `character:` / `season:` / `episode:` /
  `episodeId:` in its **own** frontmatter draws one warning per key,
  anchored at that key: identity now comes from `id:`; move the value
  under `extra:` if it should stay searchable. Promotable with `--deny`.
  The pass reads the authored frontmatter, never the defaults-merged
  view: a manifest-inherited `character:` on an `id:`-carrying document
  is silent. A document with no `id:` warns nowhere; the derived path is
  fully supported and its removal is deferred to a future major.
- **IR — `SceneMeta.id` (required) and `extra` on both `SceneMeta` and
  `QuestMeta` (optional)** — `id` is always emitted as the resolved
  canonical scene key (authored `id:` verbatim, else the derived
  `{character}.{episodeId}` join), so no consumer rederives identity;
  the four legacy `SceneMeta` fields (`character`/`season`/`episode`/
  `episodeId`) demote to optional and are emitted only when the source
  supplies them; both metas gain a `BTreeMap`-backed `extra` block
  (key-sorted, omitted when empty), populated from the frontmatter
  block. Additive under `0.13.0`'s MAJOR-only gate — the only artifact
  delta on a pre-0.15 document is the added `meta.id` line, and a
  `0.14` engine parses a `0.15.0` artifact unchanged.

### Changed

- **Schema renamed per release line** — `schemas/lute-ir-0.14.schema.json`
  is renamed to [`schemas/lute-ir-0.15.schema.json`](schemas/lute-ir-0.15.schema.json);
  its `sceneMeta` now requires `id`, admits the four legacy identity
  keys as optional, and gains an `extra` object (values are scalars or
  flat scalar-lists); `questMeta` also gains `extra`. `$id` and title
  updated to match. Under the MAJOR-only gate the rename tracks the
  published release line for strict validators only; no engine gate
  widens.
- **`E-META-MISSING` narrowed and `E-CONN-EPISODE-ID-DUP` generalized**
  — the former fires only when the defaults-merged frontmatter lacks
  `id:` (so an `id:`-carrying document with no `character:`/`season:`/
  `episode:` is silent); the latter keeps its code but its message
  names the *canonical scene id*, and an occurrence contributed by an
  authored `id:` anchors at `id:` instead of `character:`.
- **Clippy 1.96 lint-debt cleanup** — doc-list indentation, identical-if
  merges, `if let` chains, and scoped `#[allow(…)]`s for
  `result_large_err` / `too_many_arguments`. No behavior change; the
  workspace is clippy-clean again on the current stable toolchain.
- `capabilityVersion` does NOT move this release: no `lute.core` export
  changes, and frontmatter admission is checker work, not a capability
  export.
- Version re-alignment per [`docs/versioning.md`](docs/versioning.md):
  toolchain, language, and IR all present `0.15.0`.

## [0.14.0] - 2026-08-31

**Quests that name their sub-quests.**

A parent objective can now name a child quest by id — `<objective id
quest="childId"/>` — and the child's completion IS that objective. The
language finally owns the vocabulary for the parent–child structure games
model constantly (BG3 "Save the Grove" → "Free Halsin"), so the checker
can catch orphaned children, the compiler can synthesize both the parent's
per-child completion tests and its per-required-child failure disjunction,
and engines can reconstruct a project-wide journal tree by unioning one
new IR field. `abandoned` is explicitly rejected as a fifth lifecycle
state (a nuance of journal copy is not worth a state-enum ripple).

### Added

- **Language — subquest support via `<objective quest=…>`** — an
  objective can now name a child quest by id (`<objective id
  quest="childId"/>`), and the child's completion is the objective.
  `quest=` and `done=` are mutually exclusive on one objective
  (`E-OBJECTIVE-QUEST-DONE`, exactly one required); every other objective
  attribute admits alongside `quest=` (`when=`, `optional`, `title=`, a
  completion body), and the body still plays exactly once — when the
  objective first becomes `done`, i.e. when the child completes — the
  natural "child resolved" journal slot. The compiler rewrites the
  objective's `done` as `quest.<child>.state == 'complete'` and the
  parent's `fail` as the disjunction of the authored predicate (if any)
  and one `quest.<c>.state == 'failed'` test per **required** child in
  document order, so the parent's derived-completion machinery and
  `fail`-before-completion precedence are unchanged and an engine
  unaware of the feature evaluates the compiled artifact correctly with
  zero code changes. The two rules a per-artifact compile cannot
  synthesize (a child does not know its parent) are documented in
  [`docs/runtime/quest-lifecycle.md`](docs/runtime/quest-lifecycle.md):
  (1) a terminal transition on a parent cascades every still-`active`
  child to `failed` (recursive; a required child cannot be `active` at
  parent `complete`, so that arm only ever fails running optionals) and
  (2) a referenced child with no `start` activates when its parent
  activates (replacing the walk-start / accept-driven default; a `start`
  predicate on a referenced child is evaluated only while the parent is
  `active`). Project shape is a **tree, not a DAG** — at most one parent
  per quest, cycles rejected, depth unbounded — guarded by three new
  project-level diagnostics `E-QUEST-REF-UNKNOWN` (same-document
  resolution at `check`, cross-document at `check-project`, matching how
  `after` targets already split), `E-QUEST-MULTI-PARENT`, and
  `E-QUEST-TREE-CYCLE` (self-reference is a length-1 cycle and, when
  parent and child share a document, `check` catches it early), plus the
  reachability extension where an `E-QUEST-UNREACHABLE` child propagates
  `E-OBJECTIVE-UNSATISFIABLE` onto its referencing objective's `quest=`
  span. Design record:
  [`docs/superpowers/specs/2026-08-31-lute-subquest-design.md`](docs/superpowers/specs/2026-08-31-lute-subquest-design.md).
  Language reference: [Quests & scenes → Subquests](https://lute-lang.vercel.app/language/quests-and-scenes/#subquests).
  Worked example: [`docs/examples/quest-subquest.lute`](docs/examples/quest-subquest.lute).
- **IR — `ObjectiveEntry.quest`** — the new field carries the referenced
  child id when authored; it is serialized only for subquest objectives
  (`skip_serializing_if = "Option::is_none"`, appended after `body`), so
  artifacts from documents without the feature are byte-identical.
  Engines reconstruct the parent→child tree by unioning the field across
  artifacts, exactly as they already union `relations`, `rules`, and
  `prereqEdges` — no new command kind, no new edge table.

### Fixed

- **`lute run` fired every quest's lifecycle handlers on every quest's
  transition.** The reference runner matched `<on>` handlers by event name
  alone, so in a multi-quest artifact one quest's `questComplete` ran EVERY
  quest's `questComplete` bodies — three completions replayed the same
  narrator line three times. Handlers now carry their enclosing quest
  (recovered from stream order — an `on` record follows its own quest
  declaration head) and the engine-derived lifecycle events
  (`questActive`/`questComplete`/`questFailed`) fire only for their own
  quest, which is what quest-lifecycle.md always said; mock `events:` (world
  events) stay unscoped. Surfaced by the subquest work — a cascading child's
  `questFailed` under the old matching would have replayed every sibling's
  failure copy — but the bug predates it and needed no subquests to trigger.
  The runner also implements the two subquest engine rules now, exactly as
  [`docs/runtime/quest-lifecycle.md`](docs/runtime/quest-lifecycle.md)
  specifies them for any engine: a referenced child activates on its
  parent's activation (its `start`, if any, is evaluated only while the
  parent is `active`; it is not accept-driven and never activates at walk
  start) and a terminal parent cascades every still-`active` child to
  `failed`, recursively, firing each child's own `questFailed`.

## [0.13.0] - 2026-08-26

**Editorial policy as configuration, and drafts that stay legible.**

Two axes earn this one. The **toolchain** gains a lint layer: `lute lint`
evaluates configurable editorial content rules — the checks a scenario team
enforces by convention (dialogue length and ratio, scene-length spread,
per-speaker emotion distribution with streak caps and a thrash floor,
variant composition, asset existence, shot staging) — as project policy in
`lute.lint.yaml` rather than as hardcoded opinion. The **language** gains
one universal frontmatter key, `codesLocked:`, which marks a document's
line codes as published identity and lets the new `lute tag --force`
renumber freely everywhere it is absent. The release also relaxes the
runtime version gate: engines now refuse on a **major** mismatch only, so
the recurring "pure restamp, but every engine must widen its gate" cost
(paid on 0.11.0 and 0.12.0 back to back) is gone.

### Added

- **Lint system** — `lute lint` evaluates configurable editorial content
  rules independently of `lute check`, with human and JSON diagnostics,
  deniable `L-*` rule codes, and LSP opt-in (`lsp: true`). `lute.lint.yaml`
  configures levels (`off`/`hint`/`info`/`warn`/`error`), thresholds,
  ignore globs, and project-local CEL rules over core-computed metric
  tables (line/shot/scene/speaker/group/project); seven core rules ship
  enabled with drafting-safe defaults. Plugins may export advisory
  `lints/*.yaml` rules (`<plugin-id>/<rule-id>`,
  [`plugin-system/0.0.5.md`](docs/proposals/plugin-system/0.0.5.md))
  **without** changing the capability snapshot or `capabilityVersion` —
  lints are advisory and never move artifact identity. Guide:
  [`docs/linting.md`](docs/linting.md).
- **Language 0.13.0 — `codesLocked:` and the guarded renumber** —
  `lute tag --force` rewrites every content line's `code` in clean document
  order (0010/0020/… per speaker per identity scope), a drafting tool for
  sequences left gappy by edits; output is indistinguishable from a fresh
  tag pass and the run is idempotent. The new universal frontmatter key
  `codesLocked:` refuses it — published codes key `lineId`/`voiceKey`, and
  renumbering them severs the localization/voice join. The guard fails
  closed (any value other than exactly `false` locks), and plain `lute tag`
  back-fill stays available under lock. Spec:
  [`docs/proposals/scenario-dsl/0.13.0.md`](docs/proposals/scenario-dsl/0.13.0.md).

### Changed

- **Version negotiation gates on MAJOR only** — the runtime contract
  ([`docs/runtime/execution-model.md`](docs/runtime/execution-model.md))
  and the reference runner (`lute run`/`play`/`test`) now refuse an
  artifact only when its `irVersion` MAJOR differs from the implemented
  line; minor and patch are compatible-by-default (append-only fields,
  ignored when unknown), and an **unknown command `kind`** remains the
  hard error that catches a genuinely newer capability. Previously a
  minor-line mismatch was refused outright, which taxed every aligned
  release — `0.11.0` and `0.12.0` both moved the gated line while
  changing nothing an engine reads. Pre-1.0 caveat: breaking IR changes
  may still land in a minor (`0.10.0`'s provenance rename); they are
  called out in this changelog and the schema, no longer fenced by the
  gate.
- **IR 0.13.0 is a pure restamp** — no field is added, renamed, moved, or
  retyped. The schema file still tracks the release line for strict
  validators, so `schemas/lute-ir-0.12.schema.json` is renamed to
  [`schemas/lute-ir-0.13.schema.json`](schemas/lute-ir-0.13.schema.json)
  (body unchanged apart from the stamp) — but under the major-only gate
  this rename no longer implies any engine edit at all.
- `capabilityVersion` does NOT move this release: plugin `lints` exports
  are excluded from the capability snapshot by design, and `lute.core`
  declares nothing new (`codesLocked` is checker frontmatter admission,
  not a capability export).
- Version re-alignment per [`docs/versioning.md`](docs/versioning.md):
  toolchain, language, and IR all present `0.13.0`.

## [0.12.0] - 2026-08-19

**Flow that names its destinations.**

The release-earning axis is the **language**: forward jumps. A document can
now label a position — `::mark{id="x"}` anywhere, or `id="x"` directly on a
content line — and move to it with `::next{to="x"}`, optionally guarded
(`::next{to="x" when="<CEL>"}`: jump when true, fall through when false).
Jumps are FORWARD-ONLY by static rule, so the walk stays a DAG and every
existing analysis (reachability, definite assignment, trace termination,
coverage) keeps its footing. Combined with `::end{reason=…}`, branches can
now leave their arm, rejoin a later trunk, re-diverge, and land on multiple
endings without nesting.

### Added

- **Language 0.12.0 — labels and forward jumps** — `::mark{id=…}` (position
  anchor, emits no record), content-line `id=…` (that line's record is the
  label; the one line attribute that is compile-time addressing rather than
  a record field), `::next{to=… when=…}`. One label namespace per document.
  New diagnostics: `E-MARK-DUP` (duplicate label, mark/line-id cross
  collisions included), `E-NEXT-UNDEFINED`, `E-NEXT-BACKWARD` (forward-only),
  `W-CODE-AFTER-NEXT` (dead nodes after an unguarded `::next`, the
  `W-CODE-AFTER-END` mirror). Guarded `::next` desugars to the same canonical
  one-arm `<match>` a guarded content line lowers to. Spec:
  [`docs/proposals/scenario-dsl/0.12.0.md`](docs/proposals/scenario-dsl/0.12.0.md).
- Timeline clips explicitly reject `::mark`/`::next` (the `::end` precedent).

### Changed

- **IR 0.12.0 is a pure restamp** — `::next` lowers to the EXISTING `jump`
  command and guarded jumps to the existing `match` record; no field is
  added, renamed, moved, or retyped. The gated `major.minor` still moves
  (`0.11` → `0.12`) purely by the alignment rule, so
  `schemas/lute-ir-0.11.schema.json` is renamed to
  [`schemas/lute-ir-0.12.schema.json`](schemas/lute-ir-0.12.schema.json)
  (body unchanged apart from the stamp) and an engine gated on IR `0.11`
  must widen its gate to `0.12` — reading no new field once it does.
- `lute.core` capability surface grows two directives (`mark`, `next`), so
  `capabilityVersion` snapshots move.
- Version re-alignment per [`docs/versioning.md`](docs/versioning.md):
  toolchain, language, and IR all present `0.12.0`.

## [0.11.1] - 2026-08-19

**A branch that asks its question out loud, and starts a clock.**

The release-earning axis is the **language**: `<branch>` gains two optional
attributes. `prompt="…"` names what the choice is ABOUT — the situation
sentence a host UI shows above the option labels — and `timeout="N"` gives the
pick a positive-integer seconds budget, for hosts that run a countdown and
emit a timeout when the reader does not choose. Both are author-optional;
every existing document is untouched.

### Added

- **Language 0.11.1 — `<branch prompt=… timeout=…>`** — two new optional
  `<branch>` attributes. The checker admits them (`E-UNKNOWN-ATTR` no longer
  fires) and validates their values: `prompt` must be a non-empty string
  (`E-BRANCH-PROMPT`), `timeout` must parse as a positive integer
  (`E-BRANCH-TIMEOUT`; `"0"` and non-numeric values are rejected at the
  attribute's own span). `<hub>` is unchanged.
- **IR 0.11.1 — `prompt` / `timeoutSec` on the choice record** — the compiled
  choice command carries the two values when authored and omits both fields
  entirely when not, so artifacts from prompt-less documents are byte-stable.
  Additive-only: the schema file stays
  [`schemas/lute-ir-0.11.schema.json`](schemas/lute-ir-0.11.schema.json) (the
  gated `major.minor` does not move) and engines already on IR `0.11` parse
  `0.11.1` artifacts unchanged.
- **`lute play` shows the ask** — a prompted branch renders as
  `▷ choice <id> "<prompt>" (<N>s): …` in playthrough transcripts.

### Changed

- Version re-alignment per [`docs/versioning.md`](docs/versioning.md):
  toolchain, language, and IR all present `0.11.1`. The toolchain and IR moves
  are consumer no-ops beyond the two optional fields above.

## [0.11.0] - 2026-08-15

**A route through the whole project, played in the order the player sees it.**
Everything below is toolchain: a new scheduling layer that places scenes on a
tick clock instead of leaving order to file position, a new command that
chains them into one reviewable transcript, and two bug fixes in the shared
reference runner that predate this release and affect `lute run` as much as
the new command. Language and IR are both content no-ops this time — see
*Changed* for the one real cost that still falls out of the alignment rule.

### Added

- **`schedule.yaml`** — a headerless, CLI-owned project file beside
  `lute.project.yaml` that places a project's scenes on a tick clock instead
  of leaving reading order to file position. A `clock:` (named buckets ×
  ticks-per-bucket × days) carries `lanes:` (`user`, single-threaded and
  guarded against overlap by default; `world`, overlap-by-design for events
  that do not wait for the player) and `placements:`, each an `event`
  occupying a `[at, at+size)` interval with one satisfiable-per-route
  `variant` (`when:` reads the same content-line CEL surface a guard already
  does) selected at play time — plus `optional:` (legal to have no
  satisfiable variant on some route), `presentation:` (execution order is
  `(presentation, resolved at, declaration index)`, decoupling *when a scene
  is presented* from *when it happens on the story clock* — a cold-open
  flashback can present first and be story-chronological last), and a
  variant-level `at:`/`size:`/`presentation:` override so the same event can
  sit at a different position per route. Static checks cover clock structure,
  malformed/dynamic `at:`, duplicate/unknown lanes and events, missing or
  escaping `doc:` paths, unsatisfiable and ambiguous route-space assignments
  (an `assume:` list lets a schedule assert an upstream contract like
  "inflow is never `none`" so a sentinel route stops producing false gaps),
  overlapping same-lane intervals, and an idle-pacing threshold — see
  [`docs/schedule-and-play.md`](docs/schedule-and-play.md) for the full key
  and diagnostic reference. Deliberately out of language scope: no `kind:`,
  no `luteVersion:`, no capability fold, no language/IR version bump — a
  future design integrates it as a real doc kind.
- **`lute play <PROJECT_DIR>`** — plays one scheduled route through a WHOLE
  project as one chained, reviewer-facing transcript: the whole gated project
  compiles once (the same declaration union `compile --all` writes,
  including quest docs, which are never placed), then walks the schedule's
  user-lane placements in presentation order, re-evaluating each event's
  guarded variants against LIVE state and threading `run.*`/`user.*`/
  `app.*`/`quest.*` state and facts across scene boundaries through `lute
  run`'s own reference evaluator (`scene.*` always resets to the entering
  scene's own declared defaults). A scene's `after:` prerequisite is
  re-checked against the visited/completed sets accumulated in presentation
  order, not file order — a cold-open scene declared `presentation: 0` can
  legitimately run before a day-one scene it is chronologically behind.
  World-lane events interleave: after each user placement, every not-yet-fired
  world placement whose start tick falls inside the segment just covered
  drains atomically, in `(at, declaration index)` order, even under
  `--lanes user` (world scenes still execute — state must not depend on
  rendering — the flag only gates the transcript). A presentation jump
  backward starts a new segment and is purely cinematic (no state rolls
  back); a world event draining inside one is flagged
  `W-SCHED-WORLD-IN-FLASHBACK`. Route selection is `--state`/`--fact` seeds,
  a `--script <route>.play.yaml` (this module's own closed grammar — `state:`/
  `facts:`/`choose:` with EVENT-QUALIFIED choice/hub ids, `kuhen-meeting/ask:
  [ask-record]` — never the trace mock parser, whose top-level key set has no
  notion of that shape), and/or ad-hoc `--choose <event>/<id>=<choiceId>`;
  `--auto first` resolves anything left unscripted, at every hub
  re-presentation, not just the first. Any guard or effect the reference
  runner genuinely cannot resolve (`now()`/`validAt`, an unresolved plugin
  `bridgeResult`) halts the walk **incomplete** naming the surface, never a
  silent unknown. Exit `0` complete, `1` a schedule/causality violation named
  by its `E-SCHED-*` code, `2` a usage/I/O failure (including the hard error
  when a project has no `schedule.yaml` at all — there is no `after:`-graph
  fallback, since sibling route files are unguarded by design), `3`
  incomplete. `--lanes user|all`, `--steps N` (partial-playback preview),
  and `--json` (a deterministic, byte-identical-for-the-same-seeds structured
  transcript) round out the surface.
- **`lute play --coverage <FILE>…`** — the review-gap detector: replays every
  named route script through the same chain executor with per-script
  transcript rendering suppressed, then reports every placement, variant, and
  hub/choice option the corpus as a whole never exercised. Exit `0` full
  coverage, `1` a gap remains, `2` a usage/I/O failure, `3` at least one
  corpus script halted before completion. Exclusive with `--script`/
  `--choose`/`--steps` — a single playthrough's own knobs do not compose with
  a corpus replay.
- **The full `E-SCHED-*`/`W-SCHED-*` diagnostic set** — fifteen static errors
  (clock structure, duplicate buckets, unknown lanes, duplicate events,
  malformed variant form, invalid size, unparseable/dynamic `at:`, clock
  overflow, a missing or path-escaping `doc:`, an unsatisfiable or ambiguous
  route assignment, an overlapping same-lane interval, and a malformed
  guard), one runtime error (an `after:` prerequisite unsatisfied in
  presentation order), and five warnings (an unplaced scene doc, an idle-gap
  above the pacing threshold, a route-space enumeration too large to sweep, a
  scene's first `::bg time=` disagreeing with its placement's bucket, and a
  world event draining inside a rewound segment).

### Fixed

- **A compiled `<when is="…">` match arm always fell through to
  `<otherwise>`, no matter which value it named.** An `is`-form arm compiles
  to an EMPTY raw `test` string plus a structured `expr` node (IR A13) — the
  executable surface an engine is meant to read — and the reference runner's
  `do_match` evaluated only `test`, so every `is` arm read as unknown and the
  match always converged on its `otherwise` branch, regardless of the actual
  state. First observed as six onboarding routes all greeting the player with
  the fallback line. `do_match` now prefers the compiled `expr` whenever one
  is present, falling back to the raw `test` only for a `test=`-form arm.
  This shipped in `lute run` (and therefore `lute trace`'s replay of a `run`)
  since `<when is=>` existed; a project relying on a `<match>`/`<when is=>`
  for its reference transcript should re-run it against this release.
- **A hub whose scripted decision sequence ran out with an eligible,
  non-`exit` option still on the table silently left the hub instead of
  halting.** `Runner::do_hub` iterated its forced-choice vector to the end
  and fell through to whatever came after, regardless of whether every
  option had actually converged — so a mock's `choose:` list one entry short
  of a full hub visit reported a clean, complete run (exit `0`) instead of
  the incomplete walk it actually was. `do_hub` now halts incomplete, naming
  the hub and its still-eligible options, exactly like an unscripted branch
  choice already did. Affects any `lute run --mock`/`lute play` walk through
  a hub with a `once`, non-`exit` option a script does not explicitly retire.

### Changed

- **All three axes read `0.11.0`, and only the toolchain earns it.** Language
  `0.11.0` is byte-for-byte `0.10.2` (== `0.10.1` == `0.10.0`) semantics
  ([`scenario-dsl/0.11.0.md`](docs/proposals/scenario-dsl/0.11.0.md)), and the
  IR carries no shape *or* content change — genuinely nothing for a consuming
  engine to read differently. What still moves is the number: `LUTE_IR_VERSION`
  reads `0.11.0` because a release re-aligns every visible axis whether or not
  its contract changed, and that number's `major.minor` component is the one
  the runtime contract gates on. `0.10.1` and `0.10.2` both stayed inside
  `0.10`, so neither cost a consuming engine anything; `0.11.0` does not get
  that shelter — an engine implementing IR `0.10` **must widen its gate to
  `0.11`** purely to keep accepting artifacts, even though the artifact it
  receives is byte-identical in shape to the one it already reads. Per the
  `0.7.0` precedent (a minor move with no shape change still renames the
  schema file, because the file tracks the gated `major.minor`, not the
  release number), `schemas/lute-ir-0.10.schema.json` is renamed to
  `schemas/lute-ir-0.11.schema.json` (`$id` updated to match, body otherwise
  identical). A document stamped `luteVersion: "0.10.2"` now draws
  `W-LUTE-VERSION-STALE`; restamping to `"0.11.0"` is the whole migration.


## [0.10.2] - 2026-08-12

**A checked value stops evaporating at compile.** One change, entirely IR and
toolchain: a plugin-owned frontmatter key the checker already validates now
reaches the compiled artifact instead of being discarded the moment a checked
document becomes something a runtime reads.

### Changed

- **A plugin-owned, checker-validated frontmatter key now reaches the compiled
  artifact.** §6.8 (plugin-system `0.0.1`) let a plugin declare a top-level
  `meta` key with a schema, and `0.0.2` §3 made the checker enforce it
  (`E-FRONTMATTER-SCHEMA`) — both stopped at validation. `SceneMeta`/
  `QuestMeta` were closed structs and `artifact_meta`/`quest_meta` never read
  a plugin-owned key out of the raw frontmatter at all, so a document could
  pass the checker on a value that then evaporated at the one step that turns
  a checked document into something a runtime reads. Both envelope types gain
  a `plugin` object (`BTreeMap<String, Value>`, skipped when empty — a
  document authoring no plugin-owned key is byte-identical to before this
  change): a key counts only when it is BOTH declared by an active plugin
  (`snapshot.frontmatter`) AND its authored value independently passes that
  declaration's schema — `lute-compile` re-derives this from the snapshot
  itself rather than trusting a caller-supplied `CheckResult`'s `ok`, so a
  value the checker would reject can never leak into the artifact. Value
  conversion reuses the existing `Literal` → JSON path every `state:`
  `default:` already serializes through; nested record/map values stay
  key-sorted, so `meta.plugin` is deterministic at every depth. See
  [`plugin-system/0.0.4.md`](docs/proposals/plugin-system/0.0.4.md).
- **All three axes read `0.10.2`, and this time the IR earned it while the
  language did not.** `LUTE_IR_VERSION` and `schemas/lute-ir-0.10.schema.json`
  (`sceneMeta`/`questMeta` each gain a `plugin` property) catch up to the
  `meta.plugin` shape change in this same release, rather than lagging it —
  the schema file keeps its name and `$id` (it tracks the gated `major.minor`,
  which does not move) but its content does. Language `0.10.2` is
  byte-for-byte `0.10.1` semantics
  ([`scenario-dsl/0.10.2.md`](docs/proposals/scenario-dsl/0.10.2.md)).
  Documents carrying `luteVersion: "0.10.1"` now draw
  `W-LUTE-VERSION-STALE`; restamping is the whole migration.

## [0.10.1] - 2026-08-10

**A plugin is not a second-class citizen.** All three entries come from one
adoption project — a visual-novel prototype consuming `0.10.0` artifacts — and
all three are the same shape: a surface that works for `lute.core` and quietly
does less, or nothing, once a plugin is involved. None of them is a language
change; see *Not in this release* for the one that is.

### Fixed

- **`lute test` could not see a project at all.** The subcommand had no
  `--project` flag and passed `None` unconditionally, so a document that reaches
  its schema through a manifest's `defaults: uses:` or its directives through a
  `profile:` failed **every** test on `E-DOMAIN-UNKNOWN` / `E-UNDECLARED` /
  `E-UNKNOWN-DIRECTIVE`, no matter what the test asserted. Its sibling commands
  — `check`, `compile`, `trace` — all resolved the same manifest correctly, so
  `lute trace <doc> --project P` walked a document `lute test P` could not load:
  one question, two tools, two answers, which is the class `0.10.0` spent itself
  closing and this one missed. `lute test` now takes `--project` with `trace`'s
  flag, resolution order and provider-catalog precedence, and a project-
  resolution `E-` diagnostic gates the exit code instead of surfacing as a test
  failure. There is still no manifest auto-discovery — omitting the flag keeps
  the previous core-only resolution exactly.
  This is **not** backlog `#19`/`T9.7`, which is about `lute test` walking the
  source rather than the artifact and the derived-relation fixpoint. That one
  changes *what* the harness walks; this one is whether it can see the manifest.
  They are independent and neither blocks the other.
- **An `assetKind` segment could declare a type that enforced nothing.**
  `AssetSegment.ty` is the same shared `Type` enum every other typed position
  uses, so every variant parsed in a segment position while
  `validate_segments` enforced four of them and accepted the rest in silence.
  Measured, one plugin, one document, two segments: a segment typed
  `{ enum: [alpha, beta] }` given `NOPE` reported `E-ASSET-SEGMENT`; a segment
  typed `{ domain: … }` given `NOPE` reported nothing. The plugin spec's closed
  `Type ::=` production (plugin-system `0.0.1` §7) never admitted `domain` in a
  segment — it was reachable through a Rust enum, not by design — so the fix is
  to **reject the declaration** rather than to invent member validation the
  grammar does not describe. New `E-PLUGIN-ASSET-SEGMENT-TYPE`, at plugin load,
  naming the kind, the segment, the declared type and the four admitted ones.
  Every other inadmissible variant is rejected with it, each for a stated
  reason: `enumFromOption` and `slotId` are scoped "attribute types only" by the
  production itself; `narrativeTime` is opaque and never author-declarable;
  `list`, `record` and `map` have no serialization into the single delimited
  token a decomposed segment is; `assetKind` inverts the relation by describing
  a whole id rather than one token within one; and `bool`, though single-token,
  would recreate the identical declared-but-unenforced hole for a new variant.
  A domain used *only* as a segment also drew a spurious `W-DOMAIN-UNREAD`;
  rejecting the declaration removes that at the source.
- **Every plugin load and resolve error printed a Rust struct.** Both
  diagnostic sites built their message with `format!("{e:?}")`, so
  `E-PLUGIN-PARSE` reached the user as `Parse { file: "…", msg: "…" }` and the
  new code above would have shipped as
  `AssetSegmentType { file: "…", kind: "…" }`. `LoadError` and `ResolveError`
  now implement `Display` — one sentence per variant, in the voice the checker's
  own diagnostics use — and both sites render it. The structured fields were
  always there; only the rendering was missing.
- **`E-PLUGIN-ASSET-SEGMENT-TYPE` anchored at a directory.** It named the
  `assetkinds/` export directory rather than the `.yaml` carrying the
  declaration, because the merge callback only received the directory.
  `read_kind` now threads the per-file path to its callers.
- **One LSP test matched a diagnostic by its `Debug` text.**
  `analyze_publishes_project_resolver_diagnostics` located its target with
  `message.contains("DependsCycle")` — a substring of a Rust struct name — and
  so broke the moment that struct gained a `Display`. It keys on the stable
  `E-DEPENDS-CYCLE` code now, which is the doctrine the rest of the repo
  already follows.

### Changed

- **All three axes read `0.10.1`, and none of them earned it.** The alignment
  rule moves every visible axis on every release whether or not its contract
  changed, and this is the first release since `0.7.0` where the honest report
  is "no-op on two of three". `schemas/lute-ir-0.10.schema.json` keeps its name
  and its `$id`: the schema file tracks the gated `major.minor`, not the release
  number, which is why `0.7.0` renamed its schema and this release does not.
  Documents carrying `luteVersion: "0.10.0"` now draw `W-LUTE-VERSION-STALE`;
  restamping is the whole migration, and a `0.10.0`-clean document restamped to
  `0.10.1` checks clean with no other edit.

### Not in this release

- **The staging reducer still dispatches on the literal source tag.** A plugin
  directive declaring `lower: { record: background }` gets none of the stage
  semantics its record implies: the core `::bg` injects a sprite exit at a scene
  change and the plugin equivalent injects nothing, so an engine consuming the
  second artifact leaves a character on stage. Two further rules diverge in two
  further directions — an injection silently dropped, and a `posReset`
  fabricated for a character no longer in the scene. It is filed rather than
  fixed because flag-driven dispatch has already been declined twice on the
  record (`plugin-system/0.0.3.md` §4, `scenario-dsl/0.9.0.md` §7) against a
  semantics vocabulary that genuinely cannot drive it — `mutatesScene` is shared
  by `::bg` and `::music`, so branching on it would make music clear the stage.
  Closing it needs a new closed flag or record-intrinsic dispatch, and either
  changes what the checker emits about a legal document, which puts it on the
  language axis. Evidence, reproductions and both remedy shapes:
  [`2026-08-10-staging-tag-dispatch.md`](docs/superpowers/notes/2026-08-10-staging-tag-dispatch.md).
  Also filed there: `lower:`'s own grammar is written closed in
  plugin-system `0.0.1` §8.2 and parses open, so a misspelled key — including
  one belonging to the sibling untagged variant — is dropped in silence.

## [0.10.0] - 2026-08-06

**The toolchain says what it knows.** Every entry below is a place where the
tool already held the answer and did not use it: it resolved a type and did not
apply it, held a permitted-attribute table and enforced one row of it, proved a
relation dead and reported that in one slot and nothing in another, computed a
layer and rendered none. Once, it said the opposite of what it knew.

`0.10.0` was scoped from a drive test: eighteen documents written *in* Lute on
purpose, producing a 111-entry findings log and a 38-issue backlog, of which
this release takes twenty-six. Specs:
[`scenario-dsl/0.10.0.md`](docs/proposals/scenario-dsl/0.10.0.md) — thirteen
language changes, six `LANG` and seven `LANG-SOFT`. `LUTE_LANG_VERSION`,
`LUTE_IR_VERSION` and the toolchain version all read `0.10.0`; the IR schema is
[`schemas/lute-ir-0.10.schema.json`](schemas/lute-ir-0.10.schema.json) and this
time the shape **moved** — see the IR bullet under *Changed*.

### Changed

- **BREAKING (IR) — `provenance.reason` is now `provenance.explanation`.** On
  the injection provenance stamp an artifact carries for every command the
  compiler synthesized:
  `{ "injected": true, "by": "auto-pose-reset", "explanation": "…" }`. The old
  name was a **collision, not a synonym**. `end.reason` is an opaque author
  token a host dispatches on — the author writes it and your engine branches on
  it. This field is human-readable English the compiler wrote to say why a
  record you did not author exists, and nothing dispatches on it. Two keys
  sharing a name with nothing else in common is exactly what a rename removes.
  An engine gated on IR `0.9` **must widen to `0.10`**, because the runtime
  contract requires refusing a newer major.minor; **the rename is the only edit
  it needs** beyond that. `provenance.injected` is retained but is now
  constant-`true` — with `W-INJECT-CONFLICT` gone nothing can construct a
  `false`, so do not read a `true` as distinguishing anything. Removing the
  field would be a second IR break and is deferred.
- **BREAKING (documents) — `::set` now checks the value it writes against the
  path it writes to.** `::set{run.shedPressure += "two"}` where the schema
  declares `{ type: number }` is `E-SET-TYPE`, at the right-hand side's own
  span. Every report is a write the runtime was already discarding: `+= "two"`
  on a number left the path at `0`. The checker had resolved the target's
  declared type all along — it used it to diagnose a *different* construct in
  the same run, on the same path, and never applied it to the write. It remains
  a proof obligation, never a guess: an expression whose type cannot be decided
  is accepted silently rather than guessed at.
- **BREAKING (documents) — an attribute the logic tags do not accept is now an
  error.** `<branch>`, `<choice>`, `<match>`, `<when>`, `<otherwise>` and
  `<hub>` all close their attribute sets, and a name outside the set is
  `E-UNKNOWN-ATTR` at the attribute's own column. It was already being
  discarded — silently, which is why a typo'd `when=` on a `<choice>` produced
  an unguarded choice and no complaint. Only `<otherwise>`'s empty set was
  enforced before, out of the same table the other five never consulted.
  `<choice>`'s set is **position-dependent**: `once` and `exit` are hub-choice
  only, so `exit` on a branch choice, which the hub reducer is the only reader
  of, no longer passes in silence. And `as=` on a `<choice>` is its own
  `E-AS-REMOVED` rather than "unknown", because it is not unknown — it was
  renamed to `into=` in `0.1.0`, `lute fix` performs the rename, and doing so
  restores the `set` record the document was losing.
- **BREAKING (documents) — a quest gate that can never open is an error.** A
  `<quest start=>` querying a relation nothing can ever produce is
  `E-QUEST-UNREACHABLE`, naming the relation and the declared routes. The
  producibility fixpoint already proved it: it reported the identical fact in
  `done=` as a project-wide error and in `start=` as nothing at all, after
  which `scenario reach` printed **Reachable** for the silent one. It fires on
  `start=` only, never on `fail=` — a `fail` that can never hold means the
  quest cannot fail, which is not a defect.
- **BREAKING (documents) — two required objectives that cannot both hold are an
  error.** `done="run.shedPressure >= 99"` and `done="run.shedPressure <= 0"`
  on one quest is `E-OBJECTIVE-CONTRADICTION`, naming both ids and the path.
  The diagnostic names both because it cannot know which one is wrong. Scoped to
  path-versus-literal scalar comparisons, and it carries the "this quest can
  never complete" consequence as a note rather than escalating to a second
  diagnostic.
- **BREAKING (mocks) — `mocks/*.yaml` requires a `file:` key, and is now
  checked.** `file:` names the document the mock previews, resolved **relative
  to the mock**; a mock without one is `E-MOCK-SUBJECT`. There is no
  subject-less mode. This is what makes the rest possible: `check-project` now
  validates every `mocks/*.yaml` it walks, so a mock seeding an undeclared path
  or naming a choice id that no longer exists is reported by the ordinary
  project check instead of only when someone happens to run `lute trace`. Mock
  diagnostics anchor at the mock file and name the offending key in the message
  — no line and column, because spanned YAML is not in scope. When
  `lute trace <doc> --mock m.yaml` disagrees with `file:`, the command line wins
  and the disagreement is the error.
- **BREAKING (`*.test.yaml`) — the key set is closed, and a test that asserts
  nothing fails.** Unknown keys were dropped at both nesting levels, so a file
  spelling `chooses:` lost its selection, `trace` auto-picked the first
  eligible arm, and the assertions written for the arm the file *names* were
  checked against the arm it excluded — green. Both levels now close with the
  same edit-distance did-you-mean four checker codes already use
  (`E-TEST-KEY`). And the verdict was `all()` over an empty vector, so a test
  with no recognised expectations reported **PASS**; that is now
  `E-TEST-NO-EXPECT`. Auto-picking a branch stays legal and stops being silent:
  every auto-picked branch is named along with the arm it took.
- **BREAKING (`lute run`) — a forced selection whose guard decided false is
  refused.** Asking for a choice arm whose `when=` evaluates false played it in
  full at exit `0` — in the drive test, a character delivered four lines from
  inside a cryopod. `lute trace` refused the same selection on the same
  document in the same project, and `lute test`, being trace-based, inherited
  the refusal: one question, three tools, two answers. The guard was already in
  the artifact as `option.when` and this walk already evaluated CEL everywhere
  else in it. Hard refusal, exit `2`, no opt-in flag — a flag to keep the old
  behaviour would re-create the disagreement under another name. Covered on both
  dispatch sites; a hub option that a prior visit's `::set` enabled still plays,
  because the hub evaluates per visit.
- **BREAKING (`loc export`) — a component's lines are exported once per call
  site, under the caller's id.** Adopting the language's only reuse mechanism
  used to remove a line from the localization pipeline with no diagnostic
  saying so: the export keyed a component's lines to the *component* file with
  `lineId` null, because `{prefix}` derives from the importing document's
  frontmatter and a component has none. Everything downstream keys on `lineId`,
  so `loc import` skipped the row at exit `0`, `lute tag` answered "already
  tagged" (the lines *do* carry a `code=`), and `compile --locales` then emitted
  `W-L10N-MISSING` for a caller-derived id that appeared in no export the
  translator ever saw — and shipped English at exit `0`. The export now
  normalizes first, the same pass `trace` and `compile` run, so each line is
  extracted once per call site with the caller's prefix and its `@params` bound.
  A new `source` field carries the component file and line so a TMS can dedupe
  identical text. `{{…}}` interpolation is deliberately left intact — that is
  what a translator must see.
- **Timeline time is integer milliseconds.** `at`, `duration` and `delay` are
  authored exactly as before, and the checker now converts each by **shifting
  the authored decimal**, never by multiplying a parsed float, so overlap and
  duration comparisons are exact. A boundary hand-off — `at="0.8"
  duration="0.4"` then `at="1.2"` — is legal, as the spec always said and
  floating-point accumulation denied; the epsilon and the shortened-duration
  workarounds authors wrote to dodge `E-CLIP-OVERLAP` can be deleted. A value
  finer than a millisecond is `E-TIME-RESOLUTION`. `E-CLIP-OVERLAP` and
  `E-TIMELINE-DURATION` print the **authored** decimal, never a reconstructed
  float. **The artifact keeps seconds** under the same names and JSON type — a
  cursor-derived `1.2` simply stops serializing as `1.2000000000000002`.
  Renaming them to milliseconds would place every effect 1000× late in an engine
  that did not notice.
- **A standalone component check no longer contradicts the project one.**
  `lute check c.component.lute --project P` said `ok` for a component that
  cannot work with *any* of its callers, while `check-project` reported the
  fault once per caller at line 1 of the wrong file and `lute trace` refused
  with "run `lute check` first" — advice that could not be followed. Four
  changes, one contract: a component-body diagnostic keeps its
  component-internal line and column as a secondary location instead of
  collapsing onto the importer's frontmatter span; identical reports across N
  callers roll up to one, with `(+N more callers)`; a malformed `params:` is
  reported as `E-COMPONENT-PARSE` on the standalone leg and the
  `E-UNDECLARED-REF` it *causes* is suppressed, so the author is no longer sent
  to `defs:` for a param they declared four lines up; and with at least one
  caller in scope the standalone leg reports what holds at **every** call site,
  anchored inside the component. A fault holding at only some sites is
  caller-specific and stays with `check-project`, where the caller is visible.
  With **no** caller in scope the verdict is `W-COMPONENT-UNVERIFIED`, not `ok`
  — refusing to claim a check it did not perform. "No caller in scope" covers
  both of its disjuncts, including the one an author actually types: `lute
  check c.component.lute` with **no** `--project` (there is no manifest
  auto-discovery). The two disjuncts do not share a message — "no project
  resolved" means the tool could not look, "no document imports this" means it
  looked and found nothing.
- **`lute check` runs the compile gate, so it stops being greener than
  `trace`.** `normalize` + `expand` run after the `check` gate in both
  `lute compile` and `lute trace`, and `lute check` ran neither: a scene whose
  `defs:` bodies form a cycle reported `ok: … (0 warning(s))` while `lute trace`
  on the same file printed `E-COMPILE-EXPAND … def expansion cycle: a -> b -> a`
  and then *"has check error(s) — run `lute check` first"*. That advice was
  unfollowable by construction for the whole `E-COMPILE-*` class. `lute check`
  now runs the same two passes, in the same order, past the same gate, and
  reports what they find; `E-COMPILE-COMPONENT`, `E-COMPILE-EXPAND`,
  `E-COMPILE-INTERNAL` and `E-WHEN-UNSET-SUBJECT` join the `--deny` universe
  accordingly.
- **A component is not a root document.** `lute trace`/`lute compile` on a
  `*.component.lute` used to fail with the expander's own internal invariant
  assertion — `` `@pressure` names no known def body (gate should have caught
  this) `` — and blame `check`, which reported that exact file `ok`. A
  component's `params:` are bound at each `::use`, so it has no standalone
  compiled form and no standalone walk; both commands now refuse the invocation
  for that reason and point at an importing document. On the `check` side the
  gate binds the params as a call site would, so a component's own body faults
  are reported while the absence of a caller is not mistaken for one.
- **`&&` narrowing runs in every CEL slot.** `<quest start|fail>` and
  `<objective done|when>` were the four slots where an intra-expression
  `x != unset && x > 3` did not discharge `E-MAYBE-UNSET`, as it already did
  everywhere else.
- **A component param may be declared `{ type: X }`.** Accepted as a synonym
  for the short form, so the long form authors reach for by analogy with
  `state:` and `defs:` no longer fails.
- **`--coverage` keys a `<match>` on its position, not on its guard text.** Six
  blocks opening `<match on="true">` across four files collapsed into one row
  reading `3/3 arm(s) executed` — the tool's only false statement, and its most
  reassuring one, certifying a set of six blocks no single traced path ever
  visited together. A `<match>` is now keyed on file plus line/column with the
  guard text riding along as a label (a `<branch>`/`<hub>` keeps its declared
  id, which is document-unique). The same run over the drive-test corpus renders
  19 match rows where it rendered 10. Coverage also used to accumulate only
  from reports that *ran*, so deleting a test made its scene invisible rather
  than untested; `--coverage` now lists every testable document no
  `*.test.yaml` names.
- **`E-CEL-PARSE` inside a `::set` body names `::set`'s own attribute surface**
  and drops the `'=' assigns; comparison is '=='` suggestion, which is advice
  for a guard and wrong for an assignment.
- **`E-MAYBE-UNSET` on `quest.<id>.state` names a remedy that exists.** It used
  to prescribe definite assignment, which is not reachable for a reserved
  quest path; it now names the two forms that do work.
- **`E-LOGIC-CONTENT` loses its attribute arm.** An attribute on `<otherwise>`
  is now `E-UNKNOWN-ATTR`. The code is unchanged for its three body-shape
  rules. This is the one message change on a construct that already enforced.
- **A nested `lute.project.yaml` is validated by every command that walks it.**
  `compile` and `compile --all` reached nested manifests and never validated
  them, so a broken one that `check` rejected compiled at exit `0`;
  `E-IDENTITY-TEMPLATE` and its siblings now fire from `compile` too and carry
  the manifest's own path, which they did not before. A nested manifest that the
  invoked root does **not** govern draws `W-PROJECT-INERT` — but only when it
  would have resolved a different capability snapshot, different identity
  templates, or a different `defaults:` block, because an unconditional
  warning fires on manifests whose presence changes nothing. Those three are
  exactly what a document resolves through its governing manifest, so a
  nested root cannot be inert in a way that matters and stay quiet: two roots
  supplying different `season:`/`character:` defaults rewrite every `lineId`
  in the inner subtree with both `check-project` and `compile --all` at exit
  `0`.

### Added

- **`defaults:` in `lute.project.yaml`.** Hoist frontmatter every document in a
  root repeats — `character`, `season`, `profile`'s neighbours, `uses:`,
  `extends:` and the rest of a closed defaultable set — into the manifest, and
  let a document override any of them. Purely additive; nothing requires it.
  Override is **whole-value per key**, never merged, so a document that names a
  key owns that key outright. A key outside the set is `E-DEFAULTS-KEY`: schema
  keys already compose through `uses:`/`extends:`, `profile` and `plugins`
  already have manifest routes, and `title`/`episodeId`/`after` are per-document
  by nature. `mode` is excluded because it is inert — a legal key nothing reads,
  and a defaultable key that changes nothing is a trap in a block whose whole
  purpose is changing many documents at once. A `uses:`/`extends:` path in
  `defaults:` resolves relative to the **manifest**; the same key in a document
  resolves relative to the **document**. The rest of the manifest stays open;
  only the `defaults:` mapping is closed.
- **`W-DOMAIN-UNREAD` — a declared domain nothing reads.** Project-wide only
  (`check-project`, never single-document `lute check`), because a domain
  declared in a shared schema is read by *some* document and warning on the
  scene that happens not to read it would be a false positive on the most
  common layout in the language.
- **`W-EXIT-INERT` and `W-STAGE-ABSENT` — stage state the reducer already
  held.** A content-line `action=` naming a member of the `action` domain's
  declared `exits:` looks like it removes the character and does not — only
  `::auto` does — so it is `W-EXIT-INERT`, and the message names both discharge
  paths: split it into the two-event form, or stop declaring that member an
  exit. A staging event on a character already removed by an explicit declared
  exit is `W-STAGE-ABSENT`, firing only after such an exit and only until a
  re-show, so a character's first line — which legitimately puts them on stage
  — never warns. Two codes rather than one, because they are different claims
  and `--deny <CODE>` must be able to separate them.
- **`E-RELATION-UNKNOWN` suggests the nearest declared relation.** State paths,
  `after:` scene keys, `::set` targets and enum members all offered a spelling
  suggestion; relation names were the one class that did not, against a
  `relations:` block that is the cheapest closed set in the language to compare
  against. In one drive-test run `run.shedPresure` got its suggestion and
  `can_hlat`, two lines up, got nothing. Deterministic tie-break; the
  entity-kind hint keeps precedence, so a name that *is* a declared kind still
  gets the categorical explanation rather than a spelling guess.
- **`E-META-UNKNOWN-KEY` suggests the nearest known frontmatter key.**
- **`lute trace` renders the exit, the ending, and the heading.** An exit was
  invisible: an entrance and an exit are the same construct with the same
  attribute names, and the entire difference is which value appears in the
  `action` domain's declared `exits:` — `trace` printed both as `<auto>`, and
  now prints `<auto exit>`, read from the resolved domain, never inferred. A
  terminator was unlabelled: `reason` is `::end`'s entire payload, the only
  thing distinguishing it from falling off the end of the document, and a
  project with several endings previewed them all as an identical `<end>`;
  it now prints `<end reason=bridge-reached>`. `TraceReport` gains `disposition`
  and `endReason` as additive keys, so a harness can finally tell a terminated
  walk from a spent one.
- **`scenario envelope` reports the computed layer, not the declared one.** The
  join existed and was rendered nowhere: the assembly pass built per-scene write
  sets and per-quest completion writes, handed them to propagation, and dropped
  them. Inverting that names the **writers** of every path — the edge nobody
  could draw. Scoped to graph ancestors rather than project-wide, because a
  project-wide list renders identically at every node and would name a scene
  eight scenes downstream as a writer in an earlier scene's pre-entry envelope.
  Both halves are reported and each is labelled with what is actually known:
  writers on a declared route reaching the node, and writers whose write is not
  provably before it.

### Fixed

- **A refused test prints the diagnostics it is holding.** A `choose:` naming a
  deleted choice id, one naming a deleted branch id, and a `state:` naming a
  deleted path all produced the same single line, `trace refused: invalid mock
  input`. The harness was holding the diagnostic vector, *inspecting the codes
  in it* to pick between two canned strings, and then discarding it. On a
  31-file suite that is the difference between a one-second fix and a bisect.
  Flag spellings are rewritten to key spellings on the way out, because a
  `*.test.yaml` cannot use `--choose`.
- **A malformed imported state declaration is named instead of counted.**
  `E-USES-PARSE` reported `(1 issue(s))` and nothing else — the author's total
  information about a four-word mistake in a schema. It now carries the
  import's own diagnostics as `related`, positioned against the imported file,
  through the renderer that already walks `related`; the count is computed from
  that same vector so the two cannot disagree. And `lute check world.schema.yaml`
  — the obvious next command — used to parse the YAML schema **as a scene** and
  tell the author to add `kind: scene`, which destroys it; a `.yaml`/`.yml`
  target now takes the same schema lift it gets when reached through `uses:`.
- **A content-line enum error names the line, not an invented directive.** The
  enum-member check hardcoded the `::` directive sigil and content lines passed
  it the *speaker*, so every content-line enum error named a `::narrator` that
  exists in no document, no grammar, and no `lute context` listing. Directives
  render `::auto`; content lines render `@narrator`. `E-ATTR-TYPE` had the same
  defect through the same call path and is fixed with it. Separately,
  `scenario envelope` on a quest annotated an author-facing table with an
  internal task label and a Rust function name; the distinction it draws is real
  and kept, in the author's vocabulary.
- **Nine false documentation statements, rewritten from what the binary
  prints.** Among them: `when=` was described as unqualified sugar for a one-arm
  `<match>`, when a relational guard is legal on a line and
  `E-MATCH-RELATION-SUBJECT` as a subject — where the guard queries facts the
  line form is the *only* form; "quest documents additionally use
  `::assert`/`::retract`", which a scene in the corpus disproves; and
  `{ type: enum, values: [...] }`, copied into three files, which is
  `E-STATE-DECL`. A gap you can see costs a workaround; a false sentence costs
  rounds you do not know you are spending.
- **Three documentation silences broken**, each anchored in runnable output
  rather than written from a plan: a worked `identity:` block naming the two
  templates *as* the defaults a project gets without declaring them and stating
  the two id classes they do not govern; what a content line's `action=`
  actually does (it sets the pose and marks the speaker dirty, so the next plain
  line gets an injected pose reset) and that an entrance and an exit are
  `::auto`; and that a quest's `after` is an **attribute** on `<quest>`, said on
  the page that owns both document kinds.
- **The `--deny` code registry documented its own guard in the wrong place.**
  Three documents named `crates/lute-cli/tests/deny.rs`, following a doc comment
  in `main.rs`; the drift guard is a unit test inside `main.rs` itself. The
  comment and all three documents are corrected — the misdirection had already
  misled two independent readers.

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

### Removed

- **`W-INJECT-CONFLICT`.** The first removal in this series, and the case that
  gives the release its second clause: **the toolchain said the opposite of what
  it knew.** The warning fired on `anchor="center"` where `center` is the
  declared default — and *only* there. Writing a different anchor was silent;
  writing none was silent. The one authored shape it complained about was
  **agreement**. The injecting rule only injects in the no-anchor arm, so a real
  conflict is structurally impossible; narrowing the code to "and the values
  differ" makes it unsatisfiable, which is why this is a removal and not a
  narrowing.

  **The information it carried is dropped, not migrated.** It was the only
  record that an author wrote what a rule would have injected, and there is no
  `injected: false` provenance surface to fall back on — no such surface has
  ever existed, and building one would plant a spurious anchor record in the
  artifact. An earlier draft of the spec claimed otherwise; that claim is
  retracted. If you were consuming this warning, it is gone and nothing replaces
  it.

  `--deny W-INJECT-CONFLICT` is now a usage error, exit `2`, because the code
  left the deniable registry with it.

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
assessing Lute against a real, large game catalog (hundreds of authored
scenes, tens of thousands of command rows; the assessment itself is not
included in this repo).
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

[0.13.0]: https://github.com/journeyWorker/lute/releases/tag/v0.13.0
[0.12.0]: https://github.com/journeyWorker/lute/releases/tag/v0.12.0
[0.11.1]: https://github.com/journeyWorker/lute/releases/tag/v0.11.1
[0.7.0]: https://github.com/journeyWorker/lute/releases/tag/v0.7.0
[0.2.0]: https://github.com/journeyWorker/lute/releases/tag/v0.2.0
[0.1.0]: https://github.com/journeyWorker/lute/releases/tag/v0.1.0
