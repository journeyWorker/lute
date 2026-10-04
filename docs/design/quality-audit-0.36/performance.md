---
status: Accepted
---

# Quality audit after 0.36 - performance


## 1. `crates/lute-model/src/project.rs:109-160; crates/lute-model/src/reconcile.rs:228`

- evidence: Release `lute test docs/examples/games/monster-league`: 21.40s wall, 63.30s user, 116 pass. macOS `sample` captured `ProjectModel::build_impl` 407/5283 samples (~7.7%) and `reconcile_collected` 327+59+7 direct samples (~7.4%), with reconcile dominating the inclusive chain. The chain repeatedly reaches `defassign::uses_of -> parse_uses -> lute_cel::parse_slot -> cel_parser`; `Document::clone` also appears below reconcile.
- why: The test runner pays project assembly/reconciliation repeatedly instead of treating the project as immutable and varying only scenario state. Repeated CEL parsing and AST cloning multiply CPU and allocations.
- refactor: Build one immutable ProjectModel/ExecProject per project batch; expose cheap per-test state snapshots. Preserve parsed CEL handles/arena through checking and execution, and pass borrowed documents/Cow instead of cloning. Expected gain is potentially multi-fold for the 116-test batch; bench cold-load plus project-resolution is already 3.37s for 108 tests, while analysis is only 0.276s.
- blast radius: Medium-high: lute-model APIs, CLI test runner, play runner, CEL/check interfaces, targeted integration tests.

## 2. `crates/lute-cli/src/play/mod.rs:493-500; crates/lute-cli/src/play/project.rs:259`

- evidence: `lute play` spine script: 10.61s wall/11.76s user for 144 steps. `--dump-conditions` took 12.43s/13.24s and emitted 36,861 JSONL records. The implementation documents and invokes whole-project compilation for play.
- why: Whole-project compilation is repeated across play invocations and play-bearing tests. Condition dumping adds ~17% wall time, likely from per-evaluation JSON serialization and synchronous file output.
- refactor: Cache immutable compiled ExecProject at the command/test-batch boundary. For dumping, use a buffered writer and a compact event sink; avoid formatting/serializing unless the flag is present, and consider aggregating records before one write while retaining JSONL semantics.
- blast radius: Medium: play project construction, testcmd orchestration, condition sink/output code.

## 3. `crates/lute-cli/src/differential.rs:1568-1583`

- evidence: nextest: `differential_trace_vs_run` took 59.770s (suite 3971 tests, 94.217s total). Test enumerates corpus then calls `run_cases` over every case.
- why: Differential corpus likely repeats check/model build and trace/run setup per case; this is the single slowest workspace test by 2x over the next test.
- refactor: Load/check each project once, cache immutable model and compiled execution artifacts, and feed cases through lightweight state-only runners. Add per-phase counters to ensure no hidden rebuild.
- blast radius: Medium: differential harness and model/runtime setup.

## 4. `crates/lute-cli/tests/edit_tasks.rs:174-243,245-263`

- evidence: nextest: edit_task_06_host_result 28.148s, edit_task_05_component_insert 24.515s, edit_task_11_move_scene 23.081s, edit_task_08_merge_area 20.474s. `run_task` performs validation, revision, intended/forbidden diff, trap loop; macro creates 12 independent tests.
- why: Each isolated test appears to repeat expensive fixture copy/model validation despite a shared immutable baseline being possible; trap checks multiply full validation.
- refactor: Prepare fixture/model once per test process, make each dry-run operate on an isolated in-memory snapshot or cheap temp overlay, and cache common baseline revision/diagnostics. Keep mutation isolation for correctness.
- blast radius: Medium: edit task harness and fixture setup.

## 5. `crates/lute-cli/tests/examples_check.rs:209-217 and related corpus tests`

- evidence: nextest: corpus_check_project_is_clean_end_to_end 15.666s; corpus_halsin_relational_objective_not_dead 15.416s; envelope_never_newly_errors_a_clean_standalone_scene 15.344s; no_false_positive_episode_dup... 15.064s. The first explicitly calls `check_project(&examples_dir())` over the whole shipped corpus; neighboring tests independently analyze overlapping corpus roots.
- why: Repeated whole-corpus walks, parses, reconciliation and diagnostics make integration tests serially expensive and obscure which phase regressed.
- refactor: Create a shared corpus fixture analysis once per test binary/module, then assert multiple invariants over retained structured results. If process isolation is required, add a dedicated benchmark/cache layer rather than repeating full checks.
- blast radius: Low-medium: examples_check harness and helper result ownership.

## measurements

```json
{
  "check_project": "4.65s wall, 4.55s user",
  "compile_all": "4.15s wall, 4.05s user; 134 documents",
  "diff_self": "5.73s wall, 8.75s user",
  "bench_large": "cold-load 1825ms; project-resolution 1544ms; analysis 275.9ms/2 iterations; serialization 207.2ms/19 iterations; playback 1109ms",
  "nextest": "3971 passed, 94.217s summary"
}
```
