# Phase 3 inventory — project-model

## summary

Project state is not built by one canonical snapshot today. `build_input` is the shared per-document resolver for manifest/defaults, provider catalog, plugin capability snapshot, schema imports, components, and identity; `check` then folds/checks one document. Project-oriented commands either reuse `collect_project_inputs`/`collect_project_docs` and its `DocGroup` (`Document` + `FoldedEnv`), or independently compile every document into `ExecutionIr` and assemble `ExecProject`. `run` bypasses source/project loading entirely and consumes one JSON artifact. The LSP has no project-wide semantic cache: it caches open text and diagnostics, while reloading manifest/providers/snapshot/imports/components on each analysis.

## files

```json
[
  {
    "path": "crates/lute-cli/src/input.rs",
    "description": "Shared `build_input`/`assemble_input`: project, defaults, providers, snapshot, imports, components, CheckInput, and BuiltInput."
  },
  {
    "path": "crates/lute-cli/src/input_cache.rs",
    "description": "Per-invocation memoization of manifests, providers, snapshots, plugin origins, and import cache."
  },
  {
    "path": "crates/lute-cli/src/project/mod.rs",
    "description": "Recursive `.lute` collection, nearest-root grouping, per-file check/fold, `DocGroup` and `ByRoot`."
  },
  {
    "path": "crates/lute-cli/src/cmd_check.rs",
    "description": "Single-file `check` path and compile-gate diagnostics."
  },
  {
    "path": "crates/lute-cli/src/cmd_check_project.rs",
    "description": "Project check/reconciliation and project compile pass."
  },
  {
    "path": "crates/lute-cli/src/cmd_scenario/mod.rs",
    "description": "Scenario root assembly and graph/reach/envelope/knowledge outputs."
  },
  {
    "path": "crates/lute-cli/src/beats_cmd.rs",
    "description": "Beats report over collected docs/folded environments and reconciled verdicts."
  },
  {
    "path": "crates/lute-cli/src/play/project.rs",
    "description": "Whole-project compile for play/calendar/test and in-memory `ExecProject` construction."
  },
  {
    "path": "crates/lute-cli/src/play/mod.rs",
    "description": "Play script load, project load, planning, session execution, and output."
  },
  {
    "path": "crates/lute-cli/src/play/calendar.rs",
    "description": "Calendar loads the same `ExecProject`, resolves axes, and evaluates independent cells."
  },
  {
    "path": "crates/lute-cli/src/runner.rs",
    "description": "`run` reads one execution-IR JSON and constructs a runtime machine; no source project load."
  },
  {
    "path": "crates/lute-lsp/src/backend.rs",
    "description": "LSP open-document/diagnostic caches and per-analysis manifest/provider/snapshot/import/component resolution."
  },
  {
    "path": "crates/lute-compile/src/index.rs",
    "description": "`ProjectIndex`, `IndexInput`, and `build_index` union/consistency structures."
  },
  {
    "path": "crates/lute-trace/src/exec/session/project.rs",
    "description": "`ExecProject` fields and assembly from compiled artifacts."
  }
]
```

## architecture

There are four materially different state shapes: (1) source analysis `CheckInput` + parsed `Document` + `FoldedEnv`/`CheckResult`; (2) project analysis `ByRoot`/`RootScenario`; (3) compiled project runtime `ExecutionIr` map + `ProjectIndex` + `ExecProject`; and (4) LSP per-document snapshots/diagnostics. `ProjectIndex` is canonical only for compiled-project consumers (`compile --all`, play/calendar/runtime); check-project performs selected index checks but does not retain/emit a `ProjectIndex`.

## report

## Shared source/project loading

`build_input(file, providers, project, permission_profile)` reads the file and delegates to `assemble_input`; the latter loads the project through the invocation's `InputCache`, applies manifest defaults before lifting frontmatter, resolves the capability snapshot from profile/plugins, applies permission restrictions, resolves `uses:`/`extends:` schemas and `components:` relative to the document directory, and returns `BuiltInput { input: CheckInput, meta, defaults, identity, resolve_error, resolve_blocks, project_diags }` (`crates/lute-cli/src/input.rs:99-110,145-232`). `CheckInput` contains source text/URI, capability snapshot, providers, `Mode::Ci`, `SchemaImports`, `ComponentSet`, and defaults (`crates/lute-cli/src/input.rs:216-231`). The resolver's `InputCache` memoizes `load_project`, providers, snapshots keyed by root/profile/plugins, plugin origins, and import-cache values once per CLI invocation; it does not persist across commands (`crates/lute-cli/src/input_cache.rs:1-24,31-57,59-111`).

The project walker `collect_project_inputs` recursively finds sorted/deduplicated `.lute` files, resolves each file's nearest root unless `single_root`, builds each input, desugars, splices component effects, runs `fold_env`, and runs `check_parsed` in parallel. It returns aligned file results, `ByRoot`, and inputs (`crates/lute-cli/src/project/mod.rs:34-49,89-116,128-230`). `DocGroup = Vec<(PathBuf, Document, FoldedEnv)>`; `ByRoot = BTreeMap<PathBuf, DocGroup>` (`crates/lute-cli/src/project/mod.rs:79-87`).

## Command table

| Command | Loader / project-building function | In-memory structures held | Checker/compiler/index work | Output |
|---|---|---|---|---|
| `check` | `run_check` discovers an explicit or nearest project, then calls `build_input`; source: `crates/lute-cli/src/cmd_check.rs:528-575`. | `BuiltInput`; destructured `CheckInput` and project identity; local parsed/check result. `CheckInput` is the only durable input handed to the checker. | `lute_check::check(&input)`; if clean, `compile_gate_diags` runs normalize/expand diagnostics; optional engine semantic diagnostics compile a mapped artifact (`crates/lute-cli/src/cmd_check.rs:580-608`). No project index and no project-wide document set. | One `CheckResult` human/JSON diagnostics and exit status. It reports project-resolution diagnostics separately (`crates/lute-cli/src/cmd_check.rs:556-577`). |
| `check-project` | `run_check_project` validates manifests, then calls `collect_project_inputs(dir, providers, false)`; `crates/lute-cli/src/cmd_check_project.rs:32-72`. | Per file: `(PathBuf, CheckResult)` plus `CheckInput`; per root: `DocGroup`; later project diagnostics and reconciliation data. | `reconcile_collected` performs project-wide quest/connectivity/fact/envelope analysis; `project_compile_pass` compiles clean non-components and checks capability snapshot mismatches and voice-key collisions. It creates `IndexInput` slices but only calls `capability_mismatches`/`voice_key_collisions`, not `build_index` (`crates/lute-cli/src/cmd_check_project.rs:72-91,403-530`). | Per-file plus project-wide diagnostics, human/JSON; no serialized `ProjectIndex`. |
| `context` | `run_context` uses nearest/explicit project then `build_input`; parses/folds again locally (`crates/lute-cli/src/cmd_context.rs:30-62`). | `CheckInput`, parsed `Document`, `FoldedEnv`; branch-path set; referenced reserved quest-path set; JSON `surface` assembled from capability snapshot/schema/relations and project metadata (`crates/lute-cli/src/cmd_context.rs:62-88`). | No `check`; no compiler and no project index. `fold_env` is used as a total structural schema query even when document diagnostics exist (`crates/lute-cli/src/cmd_context.rs:28-29,62-64`). | Authoring surface: capabilityVersion, permissions, directives, bridges/reward kinds/cast, stateSchema, components, relational vocabulary, defs, built-ins, beat/quest keys, project clock/terminal/seasons/chapters, and IDs for scenes/quests/entries/bundle beats. JSON is deterministic; human mode is an outline (`crates/lute-cli/src/cmd_context.rs:393-441`; `crates/lute-cli/src/context.rs:1-9,646-709`). |
| `scenario` | `run_scenario` calls `collect_project_docs`, which is the same `collect_project_inputs` result narrowed to `(file_results, ByRoot)` (`crates/lute-cli/src/cmd_scenario/mod.rs:466-505`; `crates/lute-cli/src/project/mod.rs:89-127`). | For each root, `assemble_root_scenario` creates `RootScenario`: plain docs, scene/quest/beat key sets, connectivity graph/fixpoint reachability, ambiguity/unreachable sets, envelope defaults, per-document effects, relational vocabulary, scene Must facts, and read maps (`crates/lute-cli/src/cmd_scenario/mod.rs:79-157,131-219`). | Reuses folded/check results but independently derives the scenario graph/fixpoint and project semantic views; no compiler IR and no `ProjectIndex`. | Graph (`--facts` optional), node reachability, envelope, knowledge, or endings report; all text/format variants are projections of `ByRoot` plus `RootScenario` (`crates/lute-cli/src/cmd_scenario/mod.rs:467-505`). |
| `beats` | `run_beats` calls `collect_project_docs`; then `reconcile_collected` (`crates/lute-cli/src/beats_cmd.rs:223-247`). | `ByRoot` docs/folded envs; reconciled `CheckResult`s/project diagnostics; per-root `ProjectBeat` list, occasion declarations, fact environments, selection ladders/cells. | Uses `lute_check::project_beats` and `in_selection_order`, static verdict diagnostics, `always_eligible`, `coverers`, and fact-environment checks; no compile and no `ProjectIndex` (`crates/lute-cli/src/beats_cmd.rs:247-292`). | Per-root ladder of beats by occasion/target/selection order, priority, authored `when`, verdicts, gate/shadow/coverage information; human or JSON (`crates/lute-cli/src/beats_cmd.rs:490-610`). |
| `calendar` | `run_calendar` parses optional play script, then calls `compile_project(dir, CALENDAR, reference matrix)` (`crates/lute-cli/src/play/calendar.rs:1171-1228`). | `ExecProject`; resolved axes (`Axis` with `Apply` mode and values); per-cell cloned/seeded `World`, evaluated candidate/presentation/fact results. Axis modes include state, quest, fact, visited, clock, family, tied family (`crates/lute-cli/src/play/calendar.rs:84-127,183-220`). | `compile_project` reuses `reconciled_project_results`, calls `build_input_with` per non-component document, `compile_with_check`, collects occasions/events/bridge types/cast names, then `ExecProject::assemble`; assembly invokes `build_index` (`crates/lute-cli/src/play/project.rs:75-193`; `crates/lute-trace/src/exec/session/project.rs:108-148`). Calendar then evaluates every independent grid cell with play eligibility and optional `--where`/facts (`crates/lute-cli/src/play/calendar.rs:1-35,1171-1238`). | Grid/table, JSON or CSV of eligibility/winners/shadows and optional facts; also reports never-eligible and eligible-but-never-presented beats (module docs `crates/lute-cli/src/play/calendar.rs:1-35`). |
| `overview` | There is no `overview` CLI subcommand in the command enum/dispatch. The repository's “overview” is an integration-test grouping of `calendar`, `beats`, `scenario knowledge`, graph notes, and play transcript behavior (`crates/lute-cli/tests/overview.rs:1-8`). | No distinct overview state. Each constituent command uses its own structures above. | None beyond the constituent command. | No `lute overview` output; users invoke the constituent reports. |
| `trace` | `run_trace` uses the same nearest/explicit project discovery and `build_input` as `check`/`compile`, then merges mock file and CLI flags (`crates/lute-cli/src/cmd_trace.rs:35-60`; module contract `crates/lute-cli/src/cmd_trace.rs:16-23`). | `CheckInput`; parsed/folded document for lore/usage checks; `MockSet`; optional project gate/reconciled verdict only when explicit `--project`; `TraceReport` and runtime walk state inside `lute_trace`. | `trace_with_check` (or `trace_entries_with_check`/beat path) performs check gate, normalization/expansion and source AST walk; it does not compile an `ExecutionIr` or build a `ProjectIndex` for the normal document trace (`crates/lute-cli/src/cmd_trace.rs:16-23,245-258`). | Human or JSON deterministic decision transcript/report, with refusal/incomplete/completed exit tiers. |
| `play` | `play::load` parses the script and calls `compile_play_project`; `compile_project` performs manifest validation, reconciled project results, per-document `build_input_with`, `compile_with_check`, and `ExecProject::assemble` (`crates/lute-cli/src/play/mod.rs:86-115`; `crates/lute-cli/src/play/project.rs:48-193`). | `Loaded { PlayScript, ExecProject, plan: Vec<Step>, World }`; `ExecProject` stores artifacts, authored directive map, `ProjectIndex`, occasions, state table/domains, rules/seeds, quests/objectives, entries, entity kinds, bridge reads, eval artifact, producers, cadence, and related unions (`crates/lute-trace/src/exec/session/project.rs:18-106`). | `ExecProject::assemble` calls `build_index` over every compiled artifact (`crates/lute-trace/src/exec/session/project.rs:108-148`); `execute` runs `Session::settle` and script steps (`crates/lute-cli/src/play/run.rs:72-108`). | Play transcript/human or JSON outcome; optional IR, condition dump, explain output, expectations. |
| `run` | `runner::run_artifact` reads one file, parses JSON, loads/negotiates engine matrix and IR version; it does not call `build_input`, `load_project`, schema import resolution, or plugin loading (`crates/lute-cli/src/runner.rs:78-126`). | One generic `serde_json::Value` artifact, `MockSet`, and a `Machine<RunDriver>` created from that artifact; optional entry/beat selection and occasion list (`crates/lute-cli/src/runner.rs:70-103,780-784`). | Runtime validates engine/IR envelope, owned writes, command array, then runs the compiled artifact. No source checker/compiler/project index is built at this command (`crates/lute-cli/src/runner.rs:103-150`). | Runtime transcript/report (human/JSON), optionally condition dump. |
| `lsp` | LSP `Backend::analyze` does not use CLI `build_input`; it parses current open text, calls `snapshot_for`, `imports_for`, and `components_for`, then constructs `CheckInput` with `Mode::Author` and calls `check` (`crates/lute-lsp/src/backend.rs:228-285`). `snapshot_for` discovers the nearest manifest, calls `load_project`, `project_providers`, and `resolve_document_snapshot` (`crates/lute-lsp/src/backend.rs:735-791`). | Cache: `docs: DashMap<Uri, DocumentSnapshot>` (full text + LSP version), `diagnostics: DashMap<Uri, Vec<Diagnostic>>` (original fixits/covered data), `published: DashMap<Uri,(Vec<LspDiagnostic>,Option<i32>)>`, and `imported: DashMap<PathBuf, Vec<(Uri,LspDiagnostic)>>`; no cached `ProjectConfig`, providers, snapshot, `Document`, `FoldedEnv`, project index, or compiled artifact (`crates/lute-lsp/src/backend.rs:44-94`). | Per analysis: parse/fold only as needed by check/features; `Mode::Author`; optional lint reloads manifest and lint config. `defaults_for` also reloads the project manifest; imports/components are resolved each call and degrade best-effort on failures (`crates/lute-lsp/src/backend.rs:665-733`). No project-wide check/index. | Published LSP diagnostics, related/foreign diagnostics, and editor feature responses; code actions read cached original diagnostics. |

## Where work is duplicated or diverges

1. **Source resolution is shared but not universally used.** CLI `check`, `context`, `trace`, single-file compile, and project collection use `build_input`; the CLI explicitly documents that `trace` resolves identically to `check`/compile (`crates/lute-cli/src/cmd_trace.rs:16-23`). LSP has a parallel implementation that calls the same manifest snapshot/import/component primitives but reconstructs `CheckInput` itself and uses `Mode::Author` rather than CLI `Mode::Ci` (`crates/lute-lsp/src/backend.rs:228-285`; `crates/lute-cli/src/input.rs:216-231`). `run` bypasses all source loaders.
2. **Manifest/root defaults differ by command.** Single-file commands discover the nearest manifest above the file (`crates/lute-cli/src/cmd_check.rs:548-556`). `check-project` and `scenario` use each file's nearest root bounded by the walk root (`crates/lute-cli/src/project/mod.rs:56-77,89-116`). Play/calendar force the supplied directory as one project root through `compile_project` and `build_input_with(..., Some(project_dir), ...)` (`crates/lute-cli/src/play/project.rs:75-97,132-148`), so nested subprojects are not treated as independent roots there. Explicit `--project` on single-file compile/trace gates against a reconciled project, while omitted project uses only the single-file gate (the trace contract says this explicitly at `crates/lute-cli/src/cmd_trace.rs:16-23`; compile's corresponding gate is `crates/lute-cli/src/cmd_compile.rs:152-155`).
3. **Document sets differ.** `check-project`/`scenario` recursively collect all `.lute` files and retain components in `ByRoot` for project checks/reports (`crates/lute-cli/src/project/mod.rs:89-116`). Play/calendar compile all non-component documents and skip component files as runtime artifacts (`crates/lute-cli/src/play/project.rs:95-100,132-148`). `trace` and `check` are single-document source operations. `run` receives exactly one compiled artifact.
4. **Project index is split.** `build_index` defines `ProjectIndex` with documents, requiredSemantics, vocabulary, entries, beats, clock, gates, terminal, seasons, and outside-run data (`crates/lute-compile/src/index.rs:178-239`). Play/calendar build and retain it inside `ExecProject` (`crates/lute-trace/src/exec/session/project.rs:18-36,108-148`). `compile --all` writes an index, but the requested command set does not include compile; `check-project` only invokes index helper checks over temporary artifacts and drops them (`crates/lute-cli/src/cmd_check_project.rs:403-530`). Scenario/beats operate on AST/folded structures, not compiled index rows.
5. **Checker/compiler duplication.** `check` calls `check`, then potentially compile-gate checks; `check-project` first checks/folds all documents, reconciles project analysis, then recompiles clean documents for compile-stage diagnostics; play/calendar repeat project reconciliation and compilation to build `ExecProject` (`crates/lute-cli/src/cmd_check.rs:580-608`; `crates/lute-cli/src/cmd_check_project.rs:72-91,403-530`; `crates/lute-cli/src/play/project.rs:75-193`). The per-invocation `InputCache` removes repeated manifest/provider/snapshot/import computation within one invocation, but it does not unify the subsequent AST/project-model/IR passes (`crates/lute-cli/src/input_cache.rs:1-10`).
6. **`context` is intentionally not a validation report.** It emits even if document diagnostics exist, because it builds from the folded structural schema. Its data source is the resolved capability snapshot plus folded state/relations, imports/components, document defs and implicit choice paths, and project metadata; it is not a list of current checker diagnostics or a semantic dependency/impact graph (`crates/lute-cli/src/cmd_context.rs:19-29,62-88`; `crates/lute-cli/src/context.rs:1-9`).
7. **LSP caching is document/result caching, not project-state caching.** Open text/version, original diagnostics, published wire diagnostics, and foreign related diagnostics are cached. Manifest/defaults/provider/snapshot/imports/components are re-read/re-resolved per analysis; the lint module explicitly states there is no watched-files channel and project-derived state is reloaded on each analysis (`crates/lute-lsp/src/lint.rs:1-10`; loader code `crates/lute-lsp/src/backend.rs:665-791`).
8. **No `overview` implementation exists.** The only repository use is `crates/lute-cli/tests/overview.rs`, whose module comment defines overview as a test grouping around `calendar`, `beats`, `scenario knowledge`, graph notes, and play transcript behavior (`crates/lute-cli/tests/overview.rs:1-8`).

## Gaps / risks observed

- There is no single project snapshot/API shared across `check`, `context`, `scenario`, and `beats`; they share input resolution and sometimes `DocGroup`, but derive separate project structures and passes.
- `check-project` does not retain a `ProjectIndex`; its temporary compiled artifacts are used for mismatch/collision checks only, so consumers cannot query the same compiled index it checked (`crates/lute-cli/src/cmd_check_project.rs:403-530`).
- Play/calendar's forced single-root compilation can diverge from nested-root grouping used by `check-project`/scenario (`crates/lute-cli/src/project/mod.rs:56-77`; `crates/lute-cli/src/play/project.rs:75-97`).
- The LSP can observe changes to project manifests/schemas/plugins only on a subsequent analysis, and reloads them rather than caching/invalidation-sharing; there is no project-wide cache or index (`crates/lute-lsp/src/lint.rs:1-10`; `crates/lute-lsp/src/backend.rs:44-94`).
- `context` exposes structural authoring vocabulary and IDs but no dependency/impact reasons or evidence levels; its surface is not a semantic model query (`crates/lute-cli/src/cmd_context.rs:19-29,62-88`).
- `trace` is source-AST execution and `run` is compiled-IR execution, so they have separate state construction and can only be compared through higher-level differential machinery; neither normal path exposes a shared project snapshot (`crates/lute-cli/src/cmd_trace.rs:16-23`; `crates/lute-cli/src/runner.rs:78-126`).
- The architecture target explicitly calls for one project snapshot shared by `check`, `context`, `scenario`, and `beats`, plus impact queries, constraints, and evidence levels, but the current command paths above do not implement that shared model (`docs/design/architecture-direction.md:347-358`).