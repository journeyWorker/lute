---
status: Accepted
---

# Phase 5 inventory - migration

## summary

Migration/breaking-change inventory completed: releases 0.32–0.34 classify some breaks, but 0.30/0.31/0.35 lack the D6 class taxonomy; `lute fix` is a registered CLI command backed by `lute_check::fix_document` plus YAML/CEL rewrites and is tested for idempotence; `lute tag` is separate stable-code backfill/guarded force-retag tooling; no shipped `lute migrate` command exists. Version freshness and exact-minor IR gates are implemented. D11 property-test and benchmark-gate requirements are not fully evidenced, and many proposal/design/superpower documents have stale or non-canonical statuses.

## architecture

Breaking-change policy is documented in the ADR, while migrations are split between parser/checker fixits (`lute_check::fix_document`), CLI source/YAML span rewrites (`crates/lute-cli/src/rewrite.rs`), and stable identity tagging (`tag_document`/`retag_document`). Version freshness is warning-only for source stamps; artifact engines negotiate `irVersion` and semantic IDs. Documentation status is inconsistent: released/implemented scenario specs coexist with Draft labels, old design-wave labels, and many unmarked implementation plans/notes.

## 1. CHANGELOG breaking-change treatment (0.30–0.35)

| Release | Current wording / classification | Inventory finding |
|---|---|---|
| 0.35.0 | The section has Added/Changed/Fixed only; no Breaking, Compatibility, Migration, or class label (`CHANGELOG.md:57-76`). | No D6 class is stated. It is especially notable because the section claims `lute fmt`/patch/edit-loop delivery but does not classify syntax, semantic, IR, plugin, CLI, or diagnostics impact. |
| 0.34.0 | Explicitly calls itself a clean-cut breaking change in “source, toolchain, and analysis/JSON contracts” (`CHANGELOG.md:79-84`); a `### Breaking` subsection identifies JSON evidence/scope changes and strict project constraints/no legacy loader (`CHANGELOG.md:103-108`). | Broad contract labels, but not the D6 taxonomy. “source/toolchain/analysis/JSON” is not the required syntax / semantic / IR / plugin list, and CLI/diagnostics are not separately classified. |
| 0.33.0 | Explicitly says breaking in “IR and engine contract,” source unchanged (`CHANGELOG.md:111-118`). The migration section covers recompilation, engine matrices, exact `0.33` line, and refusal behavior (`CHANGELOG.md:153-162`). | IR/engine is clear, but “engine contract” is not one of D6’s four exact class labels; no explicit syntax/semantic/plugin/CLI/diagnostics classification table. |
| 0.32.0 | Explicitly has three classes: syntax, semantics, and IR (`CHANGELOG.md:165-176`); migration says run `lute fix`, retype numeric declarations, and update CLI facts only where applicable (`CHANGELOG.md:177-184`). | This is the closest current example to D6. It omits plugin, CLI, and diagnostics as explicit classes, even though the section also describes runtime refusal and a renamed warning (`CHANGELOG.md:202-223`). |
| 0.31.0 | Describes clock movement and schedule diagnostics, then has “Compatibility and migration”; migration is plugin `spendsSlot(s)` → `advances` (`CHANGELOG.md:224-251`). | No explicit breaking label or class. The plugin migration is not called “plugin breaking”; diagnostics are described functionally, not classified. |
| 0.30.0 | Calls the change a minor language/IR move and describes name/path behavior (`CHANGELOG.md:253-273`). Compatibility gives old-name acceptance, restamping `luteVersion`, and schema rename (`CHANGELOG.md:287-292`). | No explicit Breaking/Migration class list. The actual source-language and IR/version-stamp changes are not labeled syntax/semantic/IR. |

The ADR requirement is direct: all structural changes are clean cutovers, each corpus migration lands in the same change, and each break is classed in CHANGELOG as **syntax, semantic, IR, or plugin** (`docs/design/architecture-direction.md:158-164`). Thus 0.32 partially satisfies the requested taxonomy; 0.33–0.34 use broader labels; 0.30/0.31/0.35 do not provide the required class list. No sampled section has a distinct normative **CLI** or **diagnostics** class, although the user’s requested inventory asks whether those axes are tracked separately.

## 2. `lute fix`, `lute tag`, and `lute migrate`

### `lute fix`: registration and traversal

* The CLI enum registers `Fix { path }` with help text covering legacy line/choice syntax, literal `<when>` comparisons, and 0.31 CEL fact-query/presence calls; a directory recursively processes `.lute`, `.yaml`, and `.yml` files (`crates/lute-cli/src/cli.rs:252-261`).
* `run_fix` is the in-place driver: it walks directory targets in sorted recursive order, invokes `fix_file`, writes only changed text, counts fixes, and returns the worst outcome (`crates/lute-cli/src/rewrite.rs:211-280`). `fix_file` first calls `lute_check::fix_document`, then applies `.lute` or YAML CEL rewrites (`crates/lute-cli/src/rewrite.rs:268-281`).
* `lute_check::fix_document` is the source codemod core. It collects checker/parser fixits whose `kind == "migrate"`, applies them back-to-front, re-parses, and only then runs structural AST rewrites (`crates/lute-check/src/fix.rs:82-126`).

### Rewrites and version/feature keys

| Rewrite | Keyed by | Registration/mechanism | Idempotence/evidence |
|---|---|---|---|
| `:line[speaker]{…}: text` → `@speaker{…}: text` | Removed 0.0.1 bracket form; current 0.2.2 sigil | Parser emits `E-UNCLASSIFIED` with a `migrate` fixit; `fix_document` applies all `migrate` fixits (`crates/lute-syntax/src/parser.rs:1217-1248`; `crates/lute-check/src/fix.rs:86-104`). | The old form disappears; unit tests cover attrs/no attrs (`crates/lute-check/src/fix.rs:517-539`). |
| Any legacy leading `:` content sigil → `@` | 0.2.2 / DSL §7.1 | Parser emits `E-LEGACY-CONTENT-SIGIL` with a one-byte `migrate` fixit (`crates/lute-syntax/src/parser.rs:1257-1270`); same phase-1 collector applies it. | A second parse sees `@`; mixed-document test asserts second run is unchanged (`crates/lute-check/src/fix.rs:634-653`). |
| Choice/hub-choice `as=` → `into=` | 0.1.0 rename, documented by 0.10.0 | Phase-2 AST walk collects choices in scene, quest, lore entry, and bundle beat bodies; rewrites only choice keys (`crates/lute-check/src/fix.rs:113-184`). CLI docs call it a mechanical migration (`crates/lute-cli/src/cli.rs:252-258`). | Test coverage includes branch and hub choices and nested quest/on/objective cases (`crates/lute-check/src/fix.rs:541-575,657-664`; `crates/lute-check/tests/choice_persist.rs:404-406`). Content-line `as=` is intentionally not touched (`crates/lute-check/src/fix.rs:1-18`). |
| `persist="run"` deletion when the same choice has `into=` | DSL 0.6.0 §2.3 | Phase-2 choice walk deletes only this meaning-preserving shape; other values/missing `into` remain manual (`crates/lute-check/src/fix.rs:187-205`; `crates/lute-check/src/fix.rs:39-41`). | Idempotent because the key is removed; checker/LSP and CLI byte-equivalence is tested (`crates/lute-check/tests/choice_persist.rs:404-406,543-546`). |
| `## Shot N.` / `## Scene N.` prefix strip when a title follows | DSL 0.6.0 §3.4 | Phase-2 shot scan (`crates/lute-check/src/fix.rs:211-218`; implementation continues after `:213`). | Documented as idempotent; bare heading is intentionally preserved (`crates/lute-check/src/fix.rs:42-51,208-210`). |
| `<when test="…">` literal `$` comparisons → `<when is="…">` | DSL 0.18.0 §3 and `W-WHEN-TEST-LITERAL` | Shared classifier/rewrite list is used by checker fixit and `fix_document` (`crates/lute-check/src/fix.rs:52-62`; `crates/lute-check/src/check/pipeline.rs:897-902`). | Tests cover single/multiple forms and second-run no-op (`crates/lute-check/tests/when_test_literal.rs:95-112,212-216`). |
| Legacy CEL calls: `holds`, `count`, `countDistinct`, `validAt`, `isSet` | 0.31 → standard CEL in 0.32 | `rewrite_cel_with_quote` parses valid slots, finds target call spans, and rewrites to list-form calls/`has` (`crates/lute-cli/src/rewrite.rs:282-319`); YAML scalars are decoded/spliced and comments/layout preserved (`crates/lute-cli/src/rewrite.rs:879-917`). | Unit tests cover quoted args, nested calls, distinct/validAt, presence, unmatched shapes, second run, YAML comments and quote safety (`crates/lute-cli/src/rewrite.rs:944-1043`). |

The core documentation explicitly calls `fix_document` “idempotent, deterministic, total” (`crates/lute-check/src/fix.rs:82-85`), and the CLI has an end-to-end second-run assertion (`crates/lute-cli/tests/cli.rs:996-1030`). There is no version-dispatch registry/table: rewrites are hard-coded by historical DSL comments and diagnostic/parser fixit kinds, not selected by a `--from`/`--to` version argument. The only explicit CLI argument is `path` (`crates/lute-cli/src/cli.rs:252-261`).

### `lute tag`

* Normal mode calls `lute_check::tag_document`; it backfills missing per-speaker `code`, writes only if additions occur, and reports “already tagged” without rewriting (`crates/lute-cli/src/rewrite.rs:122-129,196-208`).
* `tag_document` refuses to rewrite a structurally broken document and otherwise adds stable codes (`crates/lute-check/src/tag.rs:23-28`). Its identity contract is `code` → `lineId`/`voiceKey`; force mode is separate (`crates/lute-check/src/tag.rs:173-212`).
* `lute tag --force` calls `retag_document`, renumbers every line in document order with per-speaker counters, and refuses when `codesLocked` is present or structural errors exist (`crates/lute-cli/src/rewrite.rs:131-189`; `crates/lute-check/src/tag.rs:198-218`). Locking is fail-closed: only exactly boolean `false` unlocks; malformed/other values lock (`crates/lute-check/src/tag.rs:338-352`).
* Both normal and force paths have CLI/core tests proving second-run no-op/idempotence (`crates/lute-cli/tests/tag.rs:1-58`; `crates/lute-check/src/tag.rs:505-510`).
* D8 still identifies the unresolved identity issue: component `{component}#{n}` is ordinal and inserting an earlier `::use` renumbers it; phase 5 is expected to supply stable component instance keys (`docs/design/architecture-direction.md:187-201`). The current tag command only stabilizes line codes; it does not solve component-instance identity.

### Any `lute migrate` notion

No shipped CLI command or CLI enum variant named `Migrate` was found; the command inventory has `Tag` and `Fix` but not `Migrate` (`crates/lute-cli/src/cli.rs:238-261`). Existing implementation/docs consistently call the tool `lute fix` and its edits “migrations” (`crates/lute-cli/src/cli.rs:252-258`; `crates/lute-check/src/fix.rs:1-10`). A conceptual `lute migrate --from 0.31 --to 0.40` appears in the deep architecture report only (`reports/lute-deep-report.md:2405-2408`), and a later report sketch lists `lute migrate` as a proposed inspection/migration surface (`reports/lute-deep-report.md:2701-2703,2827-2829`). That is proposal/report material, not an implemented command or ADR decision.

## 3. Version gates and stamps

### Language/frontmatter

* `lute_check::LUTE_LANG_VERSION` is `0.35.0`; checker compares a present frontmatter stamp to this constant for freshness (`crates/lute-check/src/lib.rs:70-79`).
* `luteVersion` is lifted from frontmatter and is explicitly freshness-only, not capability-validated (`crates/lute-check/src/meta.rs:158-164`).
* `W-LUTE-VERSION-STALE` fires only when a present stamp differs from current language version; absent/current stamps are clean, newer stamps are not stale; the warning names the current stamp to write (`crates/lute-check/src/check/version.rs:14-25,64-75`).
* Project `defaults.luteVersion` is inherited and folded to one manifest-level warning rather than one warning per document (`crates/lute-cli/src/cmd_check_project.rs:435-478`).

### IR/engine gate

* `LUTE_IR_VERSION` and the artifact `irVersion` stamp are `0.35.0` (`crates/lute-compile/src/lib.rs:385-390`; artifact field docs `crates/lute-compile/src/ir.rs:23-27`).
* `EngineMatrix::negotiate` reads artifact `irVersion`, requires `major_minor_match`, then checks `requiredSemantics`; an IR mismatch emits `E-ENGINE-IR-VERSION` before semantic negotiation (`crates/lute-cli/src/engine_matrix.rs:49-66`).
* `major_minor_match` accepts same MAJOR and, while MAJOR is 0, exact MINOR; PATCH may differ. From MAJOR 1 onward only MAJOR must match (`crates/lute-cli/src/engine_matrix.rs:79-82`; tests `crates/lute-cli/src/engine_matrix.rs:105-115`). The reference runner carries the same rule (`crates/lute-cli/src/runner.rs:5-8,120-134`).
* `docs/versioning.md` defines Toolchain, Language, IR, Capability, and Plugin axes (`docs/versioning.md:1-25`), says releases re-align every visible axis (`docs/versioning.md:29-47`), and documents pre-1.0 exact major.minor versus post-1.0 MAJOR-only IR gating (`docs/versioning.md:16-21`).

## 4. D6 and what the class list must cover

D6 says the clean-cutover policy exists because the only consumer was the dogfood corpus: no compatibility windows, shims, or legacy modes (`docs/design/architecture-direction.md:158-161`). Every breaking change must migrate the corpus in the same change, via `lute fix` or one-shot codemod, and CHANGELOG must class it as syntax, semantic, IR, or plugin (`docs/design/architecture-direction.md:162-164`). The policy ends at the **first external consumer**; after that, D5’s required-semantics negotiation becomes the safety mechanism (`docs/design/architecture-direction.md:164`; the external-consumer finding is reiterated at `docs/design/architecture-direction.md:392-394`).

Therefore the normative list must at minimum cover: source syntax, source/static semantics, execution IR, and plugin/capability contracts. In the requested expanded audit, CLI/tooling and diagnostics are currently observable break surfaces but are not named D6 classes; 0.34’s “toolchain and analysis/JSON contracts” is the closest substitute (`CHANGELOG.md:79-84`).

## 5. D11 hygiene and property-test/benchmark status

### Required property tests

| Requirement | Evidence found | Status |
|---|---|---|
| Formatter idempotence | Normative requirement `fmt(fmt(x)) == fmt(x)` (`docs/proposals/scenario-dsl/0.35.0.md:101-111`); phase-4 inventory says the existing repo did not yet provide the full canonical/comment-preserving formatter contract (`docs/design/phase4-inventory/formatter.md:4-5,216-219`). | Claimed by 0.35 spec, but no dedicated `proptest`/`quickcheck`-style property harness found in the repository search. Existing `lute fix`/CEL rewrite idempotence tests are example/unit tests, not formatter property tests (`crates/lute-cli/src/rewrite.rs:1014-1027`). |
| IR serialize/deserialize round-trip | D11 requires it (`docs/design/architecture-direction.md:253-255`). Search found ordinary serde/golden tests but no property-test harness or explicit serialize→deserialize arbitrary round-trip property. | Gap / not evidenced. |
| Parser never panics on arbitrary input | D11 requires arbitrary-input property/fuzz proof (`docs/proposals/scenario-dsl/0.35.0.md:103-110`). `lute-cel` catches known backend panics (`crates/lute-cel/src/lib.rs:307-344`) and has targeted malformed-input tests (`crates/lute-cel/src/lib.rs:577-582`); checker/parser modules repeatedly document total/no-panic behavior (`crates/lute-check/src/check/pipeline.rs:151-154`). | Defensive implementation plus targeted tests exist, but no arbitrary-input `proptest`, `quickcheck`, `arbitrary`, or fuzz corpus/harness was found. |

The repository-wide search did find the word `fuzz` in the new spec claim, and `panic`/`catch_unwind` defensive code/tests, but did not find `proptest`, `quickcheck`, or `arbitrary` harness usage. This is evidence of missing property infrastructure, not proof that every parser path can panic.

### Benchmark gates

D11 requires a tiny/medium/large benchmark corpus with cold/warm load, resolution, analysis, and serialization timings as CI regression gates (`docs/design/architecture-direction.md:259-261`). No Criterion benchmark target, `benches/` target, `cargo bench`, or CI comparison/failure gate was found in the scoped `.github`, Cargo, and crates search. Current CI does have ordinary docs/corpus checks (`.github/workflows/docs.yml:82-96`), and the Unreleased changelog reports CI/test performance work, but it does not describe benchmark thresholds or regression failure gates (`CHANGELOG.md:20-30`). Thus the phase-5 benchmark-gate exit is not evidenced as implemented.

## 6. Documentation status inventory

The ADR requires statuses from the controlled vocabulary Draft, Accepted, Implemented, Superseded, Rejected, and specifically calls out `runtime-unification.md` as stale (`docs/design/architecture-direction.md:256-258`).

### Explicitly stale or semantically mismatched markers

| File | Marker | Why stale/mismatched |
|---|---|---|
| `docs/design/runtime-unification.md:1-8` | `Status: design for wave 2 of 0.27` | ADR says its `Machine` landed, so the status should no longer describe a future wave (`docs/design/architecture-direction.md:256-258`). |
| `docs/proposals/scenario-dsl/0.0.1.md:3-4`, `0.1.0.md:3-4`, `0.2.0.md:3-4`, `0.3.0.md:3-4`, `0.10.0.md:3-4` through `0.0.21` files listed below | `Draft` | These are historical normative specs for releases that have shipped/been superseded; the marker is not one of the final states for a released record. |
| `docs/proposals/scenario-dsl/0.11.1.md:3-7` | `Draft — backfilled … This revision shipped` | Explicitly self-identifies a shipped revision while retaining Draft. |
| `docs/proposals/scenario-dsl/0.31.0.md:1-4` | `status: Draft (0.31.0, …)` | Changelog records 0.31.0 as released (`CHANGELOG.md:224-251`), so Draft is stale. |
| `docs/proposals/plugin-system/0.0.1.md:3-4` through `0.0.7.md:3-4` | `Draft` | Several say they shipped with Lute 0.17/0.17.2 (`docs/proposals/plugin-system/0.0.6.md:3-7`; `0.0.7.md:3-7`) while retaining Draft. |
| `docs/proposals/character-cast/0.0.1.md:3-4`, `design.md:3-4` | Draft / Approved design (pre-implementation) | Historical pre-implementation markers; no current Implemented/Superseded/Rejected state is recorded. |
| `docs/superpowers/specs/2026-07-04-lute-compile-json-ir-design.md:1-4` | Draft (pre-implementation) | It is an implementation-era design; current execution IR is shipped and versioned, but marker remains Draft. |
| `docs/superpowers/specs/2026-07-10-lute-data-catalog-foundation-design.md:3-7` | Approved design, pre-implementation | Current data-catalog code exists, but status is not normalized to Implemented. |
| `docs/superpowers/specs/2026-08-14-lute-schedule-and-play-design.md:1-4` | design v2 / implementation proceeds | No controlled-vocabulary final state; implementation is present in current CLI/runtime surfaces. |
| `docs/superpowers/specs/2026-08-26-lute-lint-system-design.md:1-4` | approved design, pre-implementation | `lute lint` exists in the CLI/source, but marker remains pre-implementation. |
| `docs/superpowers/specs/2026-08-31-lute-subquest-design.md:3-4`, `2026-09-01-lute-reward-design.md:3-4`, `2026-09-01-lute-scene-id-design.md:3-4` | approved design, pre-implementation | Current specs/features exist, but markers have not been promoted. |

The scenario proposal set’s current explicit markers, by file ranges, are:

* Draft: `0.0.1`, `0.1.0`, `0.2.0`, `0.3.0`, `0.4.0`, `0.5.0`, `0.5.1`, `0.5.2`, `0.6.0`, `0.6.1`, `0.7.0`, `0.8.0`, `0.9.0`, `0.10.0`, `0.10.1`, `0.10.2`, `0.11.0`, `0.11.1`, `0.12.0`, `0.13.0`, `0.14.0`, `0.15.0`, `0.15.1`, `0.16.0`, `0.17.0`, `0.17.1`, `0.17.2`, `0.18.0`, `0.19.0`, `0.20.0`, `0.21.0` (representative header evidence: `docs/proposals/scenario-dsl/0.0.1.md:3-4`; continuation headers are enumerated by repository search). The explicit transition begins at `0.22.0` Released (`docs/proposals/scenario-dsl/0.22.0.md:3-7`), continues through `0.30.0` Released (`docs/proposals/scenario-dsl/0.30.0.md:1-4`), then regresses to Draft at `0.31.0`, and is Implemented for `0.32.0`–`0.35.0` (`docs/proposals/scenario-dsl/0.32.0.md:1-8`, `0.33.0.md:1-8`, `0.34.0.md:1-8`, `0.35.0.md:1-9`).

### Docs lacking a status marker

The file-level search found no missing status in the normative `docs/proposals` files or the three top-level `docs/design` files (`architecture-direction.md`, `modules.md`, `runtime-unification.md` all have a marker at `:3` or `:6`). The missing-marker problem is concentrated in `docs/superpowers` implementation plans and research notes. Files observed without a `Status`/`status` header include:

| Directory | Unmarked files (each has no controlled status header) |
|---|---|
| `docs/superpowers/plans` | `2026-07-01-lute-lsp-rust.md`; `2026-07-02-lute-assetkinds.md`; `2026-07-02-lute-plugin-system.md`; `2026-07-03-feat1-property-tracks.md`; `2026-07-03-feat2-uses-extends.md`; `2026-07-03-feat3-choice-persist.md`; `2026-07-03-feat4-ctx-env-scope-split.md`; `2026-07-03-feat5-components.md`; `2026-07-03-fnd1-ast-traversal-seam.md`; `2026-07-03-fnd2-lsp-uses-context.md`; `2026-07-03-lute-checker-precision.md`; `2026-07-03-lute-plugin-defs-and-param-calls.md`; `2026-07-03-showcase-full-spec.md`; `2026-07-04-lute-compile.md`; `2026-07-06-lute-check-0.1.0-cutover.md`; `2026-07-06-lute-syntax-0.1.0-cutover.md`; `2026-07-07-lute-tooling-0.1.0.md`; `2026-07-07-lute-compile-0.2.0-quest.md`; `2026-07-09-lute-check-0.2.0-quest.md`; `2026-07-09-lute-compile-0.2.0-quest.md`; `2026-07-09-lute-editor-0.2.0-quest.md`; `2026-07-09-lute-manifest-0.2.0-events.md`; `2026-07-09-lute-syntax-0.2.0.md`; `2026-07-10-lute-0.2.1-editor-hygiene.md`; `2026-07-10-lute-data-catalog-foundation.md`; `2026-07-11-lute-0.3.0-relational-facts.md`; `2026-07-11-lute-0.4.0-writer-experience.md`; `2026-07-13-lute-connectivity-layer.md`; `2026-07-29-lute-vocabulary-ownership-design.md`; `2026-07-31-haven-prologue.md`; `2026-08-06-lute-0.10.0-00-foundation.md`; `2026-08-06-lute-0.10.0-01-lang-core.md`; `2026-08-06-lute-0.10.0-02-lang-project.md`; `2026-08-06-lute-0.10.0-03-lang-soft.md`; `2026-08-06-lute-0.10.0-04-tooling.md`; `2026-08-06-lute-0.10.0-05-release.md`; `2026-08-10-lute-0.10.0-execution-order.md`; `2026-08-31-haven-prologue.md`; `2026-09-01-lute-rewards.md`; `2026-09-01-lute-scene-id.md`; `2026-10-01-lute-0.32.0-engine-contract.md`; `2026-10-03-lute-0.35.0-edit-loop.md`. (The two exceptions with explicit status in plans are `2026-08-06-lute-0.10.0-execution-order.md:3` and the two current 0.34/0.35 plan files only carry release metadata, not a normalized `Status` key; see their headers.) |
| `docs/superpowers/specs` | `2026-07-03-components-macros-proposal.md` is marked Implemented (`:2-3`), and `2026-07-01-lute-lsp-rust-design.md` is marked Approved (`:2-4`); most other specs have a non-controlled `approved/design/Draft` marker. No specs were found wholly headerless in the status search, but many use free-form values rather than the five required values. |
| `docs/superpowers/notes` | `2026-07-31-haven-drive-test-findings.md`; `2026-07-31-lute-0.9.0-improvement-backlog.md`; `2026-08-10-staging-tag-dispatch.md` — no status header (their prose contains incidental words such as “rejected” or “implemented,” which is not a document status). |

The status scan itself returned explicit markers for `docs/design/architecture-direction.md:3`, `docs/design/modules.md:3`, and `docs/design/runtime-unification.md:6`; proposal headers such as `docs/proposals/scenario-dsl/0.30.0.md:3` and `0.32.0.md:3`; and selected superpower specs. It did not find a normalized controlled-vocabulary marker on the unlisted superpower plan/note files. These are documentation hygiene findings only; no files were edited.
