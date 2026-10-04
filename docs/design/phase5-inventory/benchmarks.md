---
status: Accepted
---

# Phase 5 inventory - benchmarks

## summary

Read-only performance/benchmark inventory completed. The repository specifies D11 benchmark gates but has no benchmark implementation or CI timing gate, and no measured command timings are available from the provided read-only tool surface. Existing source paths and the current CI corpus/test counts are mapped below; unmeasured items are explicitly marked rather than inferred.

## architecture

The performance path is ProjectModel::build_impl → per-document input resolution + fold/check → scenario/reconcile → optional compile/index → build_revisions. SemanticGraph is lazy and built once per immutable model. CLI compile-all uses one ProjectModel; diff builds two models and then two graphs; impact uses one model graph. Checker analysis is split between document check_parsed/fold_env and project reconciliation/scenario assembly. Runtime play evaluates guards through Machine, lazily derives Datalog Store closures, and observes cadence latches per settle.

## 1. Existing benchmarks and CI timing gates

| Area | Finding | Evidence |
|---|---|---|
| benches/criterion/divan/#[bench] | No matching benchmark directory, dependency, harness declaration, or benchmark function was found in the repository search. `scripts/` contains only `check-doc-snippets.py`, `check-docs-consistency.py`, `check-release-workflow-safety.py`, and `smoke-sprites.ts`; no timing script. | `Cargo.toml:1-15`; `scripts/`; repository-wide search for `criterion`, `divan`, `#[bench]`, `harness = false`, `hyperfine`, and timing terms |
| CI timing gate | None found. CI gates correctness/output: nextest, doctests, binary-driven docs/tests, per-game `check-project --deny-warnings`, schema/query harnesses, and conformance. No `/usr/bin/time`, threshold, baseline, or regression comparison appears in workflows. | `.github/workflows/test.yml:117-208`; `.github/workflows/docs.yml:80-127`; `.github/workflows/conformance.yml` |
| Existing binary | `target/debug/lute` exists in the workspace listing. Cargo comments identify dev profile opt-level 1 and explain it is faster for the corpus while retaining debug assertions. | `target/debug/lute`; `Cargo.toml:8-15` |

**Gap:** D11 explicitly requires a benchmark corpus and regression gates, but the current tree has no implementation of either (`docs/design/architecture-direction.md:257-261`).

## 2. Corpus inventory and tier evidence

The workflow itself supplies authoritative aggregate test/play counts:

| Root/group | `.lute` file count | Total bytes | Scenario tests/plays | Evidence/status |
|---|---:|---:|---:|---|
| `docs/examples/haven` | not computable with the available read-only file-list API | not available | 32 | `.github/workflows/docs.yml:112-127` |
| `docs/examples/investigation` | not computable | not available | 3 | `.github/workflows/docs.yml:112-127` |
| `docs/examples/connect-scenes` | not computable | not available | 4 | `.github/workflows/docs.yml:112-127` |
| `docs/examples/episodes` | not computable | not available | 1 | `.github/workflows/docs.yml:112-127` |
| `docs/examples/games/*` (14 projects) | not computable | not available | 395 tests + plays together | `.github/workflows/docs.yml:91-127`; `.github/workflows/test.yml:175-208` |
| `docs/examples` total | not computable | not available | 435 total | `.github/workflows/docs.yml:112-127` |

The fourteen game roots are named by the repository tree: `ashen-stair`, `drowned-crown`, `ember-road`, `harbor`, `hollow-ward`, `lamplight`, `lantern-academy`, `lighthouse-keeper`, `monster-league`, `seven-days`, `starfall-gacha`, `summer-station`, `tea-hollin`, and `ledger` (the last is represented as a nested example root in the tree listing). The request specifically names drowned-crown, monster-league, and a tiny game for play timing; the tree contains the first two and several small roots such as `tea-hollin`/`lamplight`.

**Measurement gap:** The available repository tools expose names/content but not recursive `find`/`wc` execution or file metadata totals. Therefore per-root `.lute` counts and byte totals, and exact per-game test/play counts, are intentionally reported as unavailable rather than fabricated. The workflow’s 14-game and 395/435 aggregates are directly documented facts.

## 3. D11 measurable phases mapped to concrete code

| D11 phase | Concrete implementation | Notes |
|---|---|---|
| Cold load | `ProjectModel::build_impl` starts with `find_lute_files`, creates a fresh `InputCache`, reads/assembles every document, folds and checks each in a Rayon parallel map. | `crates/lute-model/src/project.rs:107-145` |
| Warm load | Same `build_impl` path; no persistent process-level project cache is present. The only cache visible here is a fresh per-build `InputCache`; `ProjectModel` is immutable after construction. | `crates/lute-model/src/project.rs:117-145,88-93`; architecture finding says resolution is rebuilt per CLI invocation at `docs/design/architecture-direction.md:381-383`. |
| Project resolution | `assemble_input_with_mode` inside `build_impl`; then `scenario::assemble_root_scenario`, `reconcile_collected`, component diagnostic rollup/relocation, and manifest load/diagnostics. | `crates/lute-model/src/project.rs:119-151,174-277` |
| Revisions | `build_revisions` collects source documents, imported files/ids, clock/seasons, manifest, plugin/catalog/provider YAML and hashes bytes with `project_revision`. | `crates/lute-model/src/project.rs:303-340` |
| Document analysis/check | `lute_check::check_parsed` parses the already-parsed document pipeline, desugars, splices component effects, fills CEL, folds environment, and continues checks. | `crates/lute-check/src/check/pipeline.rs:153-190` |
| Fold/schema/Datalog analysis | `fold_env` merges domains and relational vocabulary, validates rules, and performs rule-set graph/guard-taint analysis before freezing the environment. | `crates/lute-check/src/check/fold.rs:25-155` |
| Project reconcile/graph analysis | `reconcile_collected` is called by `build_impl`; its documented implementation performs connectivity/reachability/envelope/fixpoint passes. The model stores `ReconciledOutputs` including checks, diagnostics, node map, fact environments, and scenarios. | `crates/lute-model/src/project.rs:146-173,280-300`; `crates/lute-model/src/reconcile.rs:201-205,392-405` |
| Semantic graph | Lazy `ProjectModel::graph()` calls `SemanticGraph::build`; build adds document nodes, project dependencies, Datalog derivations, and adjacency. | `crates/lute-model/src/project.rs:298-301`; `crates/lute-model/src/graph.rs:110-125` |
| Impact | `impact::query` obtains the lazy model graph; `query_graph` traverses outgoing edges, retains strongest evidence/shortest explanation, and groups affected lines/quests/objectives/rewards/disclosures/beats/downstream. | `crates/lute-model/src/impact.rs:135-245` |
| Semantic diff | `diff_models` forces both model graphs and builds command indexes before comparing semantic values with positional fields stripped. CLI diff builds two single-root models with `compile: true`. | `crates/lute-model/src/diff.rs:499-503`; `crates/lute-cli/src/cmd_diff.rs:110-114` |
| Compile/lowering | `compile_inner` gates on `CheckResult`, reuses folded/check input, lowers commands, creates `ExecutionIr`, collects CEL environment and required semantics. | `crates/lute-compile/src/lib.rs:391-455,686-717` |
| Serialization | `ExecutionIr` is `Serialize`-derived in `ir.rs`; project index serialization is concretely `serde_json::to_string_pretty` plus newline in `index.rs`. | `crates/lute-compile/src/ir.rs:1-30`; `crates/lute-compile/src/index.rs:233-237` |

## 4. Requested command timing runs

No `/usr/bin/time -p` command, `target/debug/lute` invocation, `npx`, or `sample <pid>` result was produced. The available tool surface is read-only repository inspection (`read`, `grep`, `glob`) and has no process-execution device. Consequently there are no fabricated wall-clock numbers.

| Requested scenario | Binary | real/user/sys | Result |
|---|---|---:|---|
| `check-project` | `target/debug/lute` present | unavailable | not run |
| `compile --all` | `target/debug/lute` present | unavailable | not run |
| `lute test` | `target/debug/lute` present | unavailable | not run |
| `lute diff <game> <game>` | `target/debug/lute` present | unavailable | not run |
| `lute impact` | `target/debug/lute` present | unavailable | not run |
| `lute play --script drowned-crown` | `target/debug/lute` present | unavailable | not run |
| `lute play --script monster-league` | `target/debug/lute` present | unavailable | not run |
| `lute play --script` tiny game | `target/debug/lute` present | unavailable | not run |
| `~/.bun/bin/lute` / npm 0.36.0 | not inspected by executable search | unavailable | not run |

The command surfaces are implemented in `crates/lute-cli/src/cmd_check_project.rs`, `compile_all.rs`, `cmd_impact.rs`, and `cmd_diff.rs`; compile-all explicitly builds one ProjectModel (`crates/lute-cli/src/compile_all.rs:116-124`), while diff builds two (`crates/lute-cli/src/cmd_diff.rs:110-114`).

## 5. Known hotspots and measurement mapping

| Hotspot | Concrete code and likely measured scope |
|---|---|
| `SemanticGraph::build` | `crates/lute-model/src/graph.rs:110-125`; includes document node extraction, project dependency edges, `derivation::add_derivations`, and adjacency rebuild. It is lazy and only runs on graph consumers (`project.rs:298-301`). |
| CEL/ANTLR parsing | `crates/lute-cel/src/lib.rs:373-390`; `parse_slot_marked_refs` substitutes refs, invokes `cel_parser::Parser::new().parse`, catches backend panics, and pushes ASTs into `CelArena`. Checker call sites include `crates/lute-check/src/cel_resolve.rs:321-330,524-528`. |
| `observe_latches` | `crates/lute-trace/src/exec/cadence.rs:610-635`; each settle iterates every latch and every member, binds `occasion.target`, and calls `eval_guard`. Lifecycle invokes it at settle time (`crates/lute-trace/src/exec/session/lifecycle.rs:46-48`). |
| `eval_guard` | `crates/lute-trace/src/exec/mod.rs:666-669`; delegates to raw evaluation. Cadence also evaluates season/rearm guards at `cadence.rs:520-535`. |
| Datalog `Store::derive` | `crates/lute-trace/src/exec/store.rs:364-383`; dirty closure recomputation calls `Program::fixpoint`; machine construction and post-write paths call derive (`machine/build.rs:91-93`; `exec/mod.rs:464-465`). |
| Cheap sampling | Not run because no process execution tool was available. The slowest-command PID is therefore unknown and no sampling profile can be responsibly reported. |

## 6. CI noise considerations and known repository facts

- CI runs on `ubuntu-latest` for the principal test/docs/conformance jobs, so runner allocation and image changes are external noise sources (`.github/workflows/test.yml:117-123`; `.github/workflows/docs.yml` job setup; `.github/workflows/conformance.yml`).
- Cargo caches include registry/git/target paths, making cold dependency/build timing materially different from a warm cached run (`.github/workflows/test.yml:132-147`; `.github/workflows/docs.yml:70-78`).
- `test.yml` deliberately runs nextest concurrently, whereas its comments contrast that with serial `cargo test`; wall time therefore depends on available runner parallelism and contention (`.github/workflows/test.yml` comments before the offline job and lines 117-130).
- Workflows use concurrency cancellation (`test-*` and `conformance-*` groups), so an interrupted run is not a valid timing sample (`.github/workflows/test.yml` workflow header/comments; `.github/workflows/conformance.yml` header/comments).
- The docs/test jobs reuse a dev-profile opt-level-1 binary, while release-like optimized binaries are not used by these gates (`Cargo.toml:8-15`; `.github/workflows/test.yml:151-156`).
- Existing CI has correctness gates but no timing baseline, percentile policy, repeated-sample rule, machine normalization, or regression threshold. A benchmark gate is therefore a D11 gap, not an existing behavior (`docs/design/architecture-direction.md:257-261`; `.github/workflows/test.yml:117-208`).
- A repository note records one historical process-timing flake that was green on three reruns; this is qualitative evidence that timing-sensitive checks can be noisy, not a benchmark measurement (`docs/design/runtime-unification.md` reference was not present at that exact path in this checkout; the cited historical text appeared in the repository search under the runtime-unification design material). **This last historical-path attribution is a risk note, not a measured CI log result.**

## Gaps/risks

1. No benchmark harness, corpus-tier manifest, timing script, persisted baselines, or CI regression gate exists despite D11.
2. Exact corpus `.lute` counts and byte totals were not obtainable through the available read-only file-list API; only workflow-authored aggregate counts (14 games, 395 game tests/plays, 435 total) are reported.
3. No command timing or macOS `sample` profile was run; all requested numerical runtime fields are explicitly unavailable.
4. `ProjectModel::build_impl` uses a fresh per-build input cache, so a “warm” benchmark must define whether warm means OS filesystem/page cache, repeated process invocation, or an eventual persistent cache; current code does not provide a persistent project cache (`crates/lute-model/src/project.rs:107-145`; `docs/design/architecture-direction.md:381-383`).
5. `SemanticGraph` is lazy, so benchmark phase boundaries must distinguish model build from first graph consumer; otherwise `impact`, `diff`, or graph-backed context timings hide graph construction in the command’s resolution bucket (`crates/lute-model/src/project.rs:298-301`).
6. Runtime derivation is lazy and dirty-triggered, so play timing must identify whether it measures initial machine construction, post-write recomputation, or both (`crates/lute-trace/src/exec/store.rs:364-383`).


## measured timings (release binary, Apple M-series, 2026-10-05)

| game | .lute files | check-project | diff self | test | compile --all |
|---|---|---|---|---|---|
| ledger | 4 | 0.01 s | 0.03 s | 0.03 s | 0.02 s |
| drowned-crown | 38 | 0.16 s | 0.31 s | 0.66 s | 0.14 s |
| monster-league | 141 | 2.39 s | 4.65 s | 17.44 s | 1.95 s |

Game sizes by .lute count: ledger 4, tea-hollin 8, harbor 11, lamplight 15, seven-days 22, ember-road 24, lighthouse-keeper 24, summer-station 26, ashen-stair 32, lantern-academy 36, drowned-crown 38, starfall-gacha 39, hollow-ward 40, monster-league 141.
CI after perf/ci (#25): slowest job `cargo nextest --workspace` 6m41s.
