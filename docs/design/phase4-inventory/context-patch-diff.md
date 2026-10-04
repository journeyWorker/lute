# Phase 4 inventory — context-patch-diff

## summary

Read-only inventory of the existing context, LSP resolution, identities, revision/caching, rewrite/diff infrastructure, semantic-model substrate, and Report 03 Appendix B’s 12 tasks. Phase 4 is not implemented as a patch/apply/semantic-diff loop: the repository has context, check/compile/replay/impact primitives and source rewrites, but no task-scoped context schema, patch command, base-revision enforcement, preserve contract, or semantic-diff implementation.

## files

```json
[
  {
    "path": "crates/lute-cli/src/cmd_context.rs",
    "description": "Implements whole-file project-resolved authoring context in human and JSON forms."
  },
  {
    "path": "crates/lute-cli/src/context.rs",
    "description": "Extends and renders context sections such as reserved quest paths, engine paths, occasions, relational vocabulary, and components."
  },
  {
    "path": "crates/lute-lsp/src/features/mod.rs",
    "description": "Defines byte-offset cursor resolution and shared state/import/definition lookup behavior for LSP features."
  },
  {
    "path": "crates/lute-lsp/src/features/completion.rs",
    "description": "Completion candidates for directives, attributes, enums, defs, state paths, choices, and cast speakers."
  },
  {
    "path": "crates/lute-lsp/src/features/hover.rs",
    "description": "Hover resolution for capability declarations, defs, state paths, CEL host functions, and refs."
  },
  {
    "path": "crates/lute-lsp/src/features/nav.rs",
    "description": "Definition/reference navigation for @refs, state paths, choice paths, and interpolation paths."
  },
  {
    "path": "crates/lute-compile/src/address.rs",
    "description": "Regenerates addr and assigns lineId/voiceKey from code, identity templates, and component scope."
  },
  {
    "path": "crates/lute-compile/src/ir.rs",
    "description": "Execution IR structures including capabilityVersion, requiredSemantics, addr, lineId, voiceKey, source/provenance, and metadata."
  },
  {
    "path": "crates/lute-compile/src/index.rs",
    "description": "ProjectIndex union of artifacts, vocabularies, requiredSemantics, and document rows."
  },
  {
    "path": "crates/lute-cli/src/rewrite.rs",
    "description": "In-place lute tag and lute fix implementation; both are mechanical source rewrites, not general patches."
  },
  {
    "path": "crates/lute-cli/src/differential.rs",
    "description": "Existing trace-versus-compiled-run differential oracle over declared runtime observables."
  },
  {
    "path": "crates/lute-model/src/project.rs",
    "description": "ProjectModel assembly: parse, resolve, check, reconcile, compile, source maps, and project index."
  },
  {
    "path": "crates/lute-model/src/graph.rs",
    "description": "Deterministic semantic graph and NodeKey/NodeKind definitions."
  },
  {
    "path": "crates/lute-model/src/impact.rs",
    "description": "Reverse-impact query API with evidence, reasons, source locations, lineId, and speaker."
  },
  {
    "path": "crates/lute-model/src/cache.rs",
    "description": "Per-invocation memoization keyed by project/provider/profile/plugin inputs; no source revision key."
  },
  {
    "path": "reports/lute_report_03_bundle/lute_report_03.md",
    "description": "Appendix B experiment design and the verbatim-ish twelve-task suite."
  },
  {
    "path": "docs/design/architecture-direction.md",
    "description": "D8 identity, D10 AI edit-loop requirements, and phase-4 exit criterion."
  }
]
```

## architecture

Current substrate is a pipeline, not an edit-loop API: source .lute/project declarations -> build_input/fold/check/fold_env -> optional compile_mapped ExecutionIr + SourceMap -> ProjectModel reconciliation/index -> graph/impact/scenario/constraints; LSP separately resolves a cursor into AST/CEL symbols using the same capability/import data but returns source-local spans. Existing replay is trace versus compiled IR run, while existing source mutation is limited to tag/fix/scaffolding. The architecture direction explicitly requires task-scoped context, position queries equivalent to expectedType/visibleSymbols, base-revision atomic patches with preserve, and semantic diff, but those phase-4 surfaces are not present in the inspected CLI/model/LSP code.

## report

## 1. Phase-4 requirements and current boundary

The accepted direction says source and project declarations remain authoritative; semantic edits select by meaning and apply to source, preserving comments/layout rather than regenerating source from IR (`docs/design/architecture-direction.md:69-76`). D10 requires context to evolve from a whole-file capability surface to task-scoped context containing target node, declared reads/writes, direct references, affected conditions, related tests, source revision, and explicit exclusions/dynamic-or-retrieval-only labels; position queries `expectedType` and `visibleSymbols` must come from the same resolution as LSP; patches must carry base revision, target nodes, and preserve declarations; application must be atomic, stale/ambiguous targets must be rejected, and preserve must be checked by semantic diff (`docs/design/architecture-direction.md:225-241`). Phase 4’s exit is specifically the 12-task Appendix B suite over the dogfood corpus, recording validity, unintended semantic diff, and preserved-ID changes (`docs/design/architecture-direction.md:358-362`).

## 2. `lute context`: inputs, outputs, and what it does not do

`run_context(file, json, providers, project, permission_profile)` discovers or accepts a project root, calls `build_input`, reports project diagnostics, gates capability-resolution errors, parses/folds the document, collects actual branch/hub choice paths, collects referenced reserved quest paths, and extends the surface before serializing either JSON or a human outline (`crates/lute-cli/src/cmd_context.rs:29-98`). It is explicitly a capability query rather than validation and emits even when document diagnostics exist; success is 0, unreadable/serialization output failure is 2, and capability-resolution failure is 1 (`crates/lute-cli/src/cmd_context.rs:29-98`). The CLI exposes only `file`, `--json`, `--providers`, `--project`, and `--permission-profile`; there are no task, target-node, revision, related-test, or include/exclude arguments (`crates/lute-cli/src/cli.rs:262-281`).

The machine surface is deterministic: maps are BTreeMaps and arrays have defined stable ordering, with directives sorted by name, state paths by path, components by name, and declaration-order attrs/params (`crates/lute-cli/src/cmd_context.rs:103-115`). Top-level JSON includes `capabilityVersion`, `permissions`, `directives`, `bridges`, `rewardKinds`, `cast`, `enums`, `assetKinds`, `providers`, `stateSchema`, `components`, `entities`, `relations`, `facts`, `rules`, `projectEnums`, and `deliveryFlags` (`crates/lute-cli/src/cmd_context.rs:393-453`). `capabilityVersion` is the resolved capability snapshot stamp (`crates/lute-cli/src/cmd_context.rs:393-395`).

Directive entries include name, optional layer, attrs, semantics, and nonempty declared effects (`crates/lute-cli/src/cmd_context.rs:138-166`). Attr entries include name, type, required, optional domain, and optional default (`crates/lute-cli/src/cmd_context.rs:143-152`). State entries include path, type, namespace, optional owner, default, and domain; reserved quest paths and occasion paths are deliberately omitted from `stateSchema`, while engine-owned paths are marked owner `engine` (`crates/lute-cli/src/cmd_context.rs:198-225`). Components include name, typed/defaulted params, optional effects, and optional beat-template metadata (`crates/lute-cli/src/cmd_context.rs:238-285`). Relations expose name, arity, args, derive, tier where applicable, and reserved (`crates/lute-cli/src/cmd_context.rs:333-347`).

The human outline is intentionally compact: it prints capabilityVersion, directive names/attr keys/semantic flags, enum members, state paths, reserved quest paths, relational vocabulary, fixed delivery flags, and components (`crates/lute-cli/src/cmd_context.rs:638-645`; `crates/lute-cli/src/cmd_context.rs:663-665`; `crates/lute-cli/src/cmd_context.rs:837-864`). It is not task-scoped and currently contains no source revision, target identity, read/write closure, affected conditions, related tests, or explicit omitted-context list.

## 3. LSP position queries and symbol resolution

The LSP feature layer consists of four pure cursor-byte-offset functions: hover, completion, definition, and references (`crates/lute-lsp/src/features/mod.rs:1-12`). The public LSP boundary converts UTF-16 LSP positions to byte offsets with one `TextIndex`; nav returns byte spans and the backend converts them back to LSP ranges (`crates/lute-lsp/src/features/mod.rs:14-28`). There are no functions literally named `expectedType` or `visibleSymbols`; their closest existing equivalents are capability-backed completion/hover and checker-side expected-type construction.

Cursor resolution walks shots, quest bodies, lore entries, and beat bundles, descending into directive attrs, CEL slots, set paths, interpolations, match patterns, event values, construct attrs, and speaker positions (`crates/lute-lsp/src/features/mod.rs:146-223`). A `Cursor::Cel` carries the AST `CelSlot` and match-subject flag; `Cursor::SetPath` carries a state target; `Cursor::Interp` carries path/ref/reserved interpolation data (`crates/lute-lsp/src/features/mod.rs:95-145`).

Hover parses typed frontmatter and merges imports, then resolves directive declarations, attr schemas, asset segments, enum values, CEL host functions, refs, state paths, choice paths, interpolation refs, and fixed construct attrs (`crates/lute-lsp/src/features/hover.rs:30-120`). Completion offers directive names, attr keys, state paths, defs, enum values, match choices, event names, and cast/speaker candidates; completion dispatch uses the shared cursor resolver (`crates/lute-lsp/src/features/completion.rs:27-104`).

Definition navigation maps state paths to a state declaration or branch declaration, @refs to a local `defs:` declaration, and interpolation paths/refs similarly (`crates/lute-lsp/src/features/nav.rs:40-89`). Imported defs have no local declaration span, so definition degrades to None rather than inventing a location; references still find in-document use sites (`crates/lute-lsp/src/features/mod.rs:30-45`; `crates/lute-lsp/src/features/nav.rs:30-38`). References scan every @ref/state-path use, including `::set` targets and CEL occurrences, and can optionally include the declaration span (`crates/lute-lsp/src/features/nav.rs:91-100`).

The checker has an expected-type analogue for CEL slots: the backend constructs `ExpectedType::Ty` from the YAML declaration and passes it into `check_cel_slot` (`crates/lute-lsp/src/backend.rs:647-650`). This is diagnostic/type checking, not a standalone query endpoint. Completion is the practical visible-symbols analogue, but it is context-specific: for example `Cursor::SetPath` returns state-path items, while interpolation interiors intentionally have no completion (`crates/lute-lsp/src/features/completion.rs:43-104`).

Import precedence is deliberately shared with checking: imported state overwrites inline state on collisions, while inline defs win over imported defs; plugin defs are also visible (`crates/lute-lsp/src/features/mod.rs:30-45`; `crates/lute-lsp/src/features/mod.rs:65-93`). Thus the existing LSP resolver is a useful source of truth for future position-query behavior, but it is document-local and span-oriented, not project task-context oriented.

## 4. Identity inventory and stability

### Document and construct identities

`meta.id` is the canonical scene/document identity in the current model; if absent, scenes use the derived character/episode identity. The compiler stamps that resolved id into every scene line identity prefix (`crates/lute-compile/src/lib.rs:495-510`). `SceneMeta.id` is always present in the artifact, authored or derived; legacy character/season/episode/episodeId fields are optional/compatibility metadata (`crates/lute-compile/src/ir.rs:353-378`). Quest/lore document IDs are authored `id:` values when present and are also ProjectIndex keys (`crates/lute-compile/src/ir.rs:619-627`; `crates/lute-compile/src/index.rs:53-67`).

The architecture inventory names author-declared IDs for scene, beat, entry, quest, objective, branch, hub, choice, mark, `share=` keys, season, relation, def, enum/entity members, component/template params, and plugin occasions/events; `addr` remains a build-local position (`docs/design/architecture-direction.md:190-203`). The semantic graph currently covers Project, Document, Scene, Beat, Shot, Line, Choice, Quest, Objective, Reward, Entry, Occasion, Relation, State, Def, Component, Expanded, Fact, Clock, and Engine node kinds (`crates/lute-model/src/graph.rs:9-58`).

### Lines, voices, and tagging

A line’s authored per-speaker `code` feeds stable `lineId` and `voiceKey`; `lute tag` back-fills missing codes and the compiler back-fills only the not-yet-tagged remainder deterministically (`crates/lute-compile/src/address.rs:1-14`; `crates/lute-compile/src/ir.rs:975-984`). `LineCmd` serializes `lineId` and optional `voiceKey`; authored/back-filled `code` is intentionally not serialized (`crates/lute-compile/src/ir.rs:953-984`). Locale joins use `lineId`, never `addr`, because `addr` is regenerated position (`crates/lute-compile/src/locale.rs:1-18`).

`lute tag` is idempotent for already tagged documents and refuses forced renumbering when `codesLocked:` is present or structural errors exist; forced renumbering would sever localization/voice joins (`crates/lute-cli/src/rewrite.rs:105-189`). The CLI describes the same identity contract: `codesLocked` protects published `lineId`/`voiceKey` identity (`crates/lute-cli/src/cli.rs:238-256`).

### addr and component identity

`addr` is assigned after lowering/expansion and is dense, ordered, and regenerated per compile; width is calculated across the whole artifact (`crates/lute-compile/src/address.rs:54-81`; `crates/lute-compile/src/address.rs:177-250`). It is therefore not stable across inserted/deleted/reordered records. Labels and control-flow targets are resolved to addrs only in the final addressing pass (`crates/lute-compile/src/address.rs:84-109`).

Component-expanded line identity includes a component scope under `{component}#{n}`; the scope is carried as provenance/internal data and lineId/voiceKey are minted under that scope (`crates/lute-compile/src/ir.rs:750-755`; `crates/lute-compile/src/normalize.rs:39-50`). Existing tests confirm two uses receive distinct `lampsOut#1` and `lampsOut#2` line IDs/voice keys (`crates/lute-compile/src/tests/component_string_param.rs:58-95`). However, the architecture direction explicitly identifies `{component}#{n}` as ordinal and says inserting an earlier `::use` renumbers it; stable component-instance keys remain a phase-5 gap (`docs/design/architecture-direction.md:196-203`).

### Graph NodeKey

`NodeKey` is a `(NodeKind, String)` pair with canonical `kind:key` serialization (`crates/lute-model/src/graph.rs:60-76`). Graph keys are semantically named for many constructs: document IDs or fallback relative paths, scene IDs, quest IDs, objective `quest.objective`, reward keys, state paths, facts, relations, components, and expansions (`crates/lute-model/src/graph.rs:157-215`; `crates/lute-model/src/graph.rs:365-368`). The graph does not add a source-generated universal node ID. Its stability depends on the underlying key: authored IDs/state/relation/def names are comparatively stable until renamed; fallback path/heading keys and ordinal reward keys are edit-sensitive; spans and file paths are provenance, not identity. The graph’s own `node()` uses first insertion (`or_insert`) for duplicate NodeKeys, so key collisions do not create a second identity (`crates/lute-model/src/graph.rs:117-124`).

## 5. Revision, hash, and cache notions

There is no source revision/hash field in `ProjectModel::ModelDocument`; it stores path, CheckInput, AST, folded env, diagnostics, optional ExecutionIr, optional SourceMap, resolution flags, and project diagnostics (`crates/lute-model/src/project.rs:58-78`). The LSP tracks an open-document version and stamps diagnostics with it, but that is an LSP transport version, not a persisted content revision (`crates/lute-lsp/src/backend.rs:50-51`; `crates/lute-lsp/src/backend.rs:319-321`).

The strongest existing content-derived hash is `capabilityVersion`: the resolved capability snapshot’s version/content stamp, emitted by context and artifacts (`crates/lute-cli/src/cmd_context.rs:393-395`; `crates/lute-compile/src/ir.rs:25-30`). Conformance treats it as a content hash of the resolved capability snapshot and recompiles fixtures to detect stale stamps (`crates/lute-cli/tests/conformance.rs:213-216`). It identifies the resolved plugin/core capability surface, not the source document revision.

`InputCache` memoizes loaded projects by root path, providers by explicit/project key, resolved snapshots by `(root, profile, plugins map)`, plugin origins by directory path, and imports through `ImportCache` (`crates/lute-model/src/cache.rs:14-31`). These are per-invocation resolution caches and are not source revision keys. The cache keys do not include document bytes, mtime, Git commit, or source hash (`crates/lute-model/src/cache.rs:14-31`).

## 6. Existing patch/apply/diff/rewrite infrastructure

There is no general patch/apply command in the CLI enum. The relevant source mutation commands are `lute tag` and `lute fix` (`crates/lute-cli/src/cli.rs:238-261`). `lute tag` performs pure-core tag/retag logic then reads/writes whole files; it is deterministic and idempotent in normal mode, while force mode renumbers codes and is guarded by `codesLocked`/structural checks (`crates/lute-cli/src/rewrite.rs:105-189`). `lute fix` applies known mechanical migrations to `.lute` and YAML files, preserving bytes/comments where possible and writing only when changes exist (`crates/lute-cli/src/cli.rs:257-261`; `crates/lute-cli/src/rewrite.rs:191-299`). It is not a target-addressed patch engine and does not take a base revision or preserve list.

The compiler has source maps (`SourceMap.by_addr`, trailing markers, quests, objectives, handlers) keyed primarily by final addr and declaration IDs (`crates/lute-compile/src/source_map.rs:19-25`; `crates/lute-compile/src/source_map.rs:191-193`). This supports diagnostics/provenance but is not an edit application protocol. The project model retains each compiled artifact and source map (`crates/lute-model/src/project.rs:63-68`).

Streaming compilation has a narrow emitted-prefix preservation check: it compares old/new immutable commands after canonicalizing only typed address slots and control targets, while arbitrary payload strings are untouched (`crates/lute-compile/src/streaming.rs:276-329`). This is a streaming safety invariant, not a general semantic diff or preserve enforcement.

The existing `differential.rs` compares trace’s AST walk and run’s compiled-IR Machine over transcript lines, state, holding facts, quest states, and exit (`crates/lute-cli/src/differential.rs:1-20`; `crates/lute-cli/src/differential.rs:21-54`). It intentionally compares declared runtime observables, not rendering. This is the closest current semantic-behavior comparator, but it compares two executions of one source, not before/after source revisions.

Conformance is the existing execution replay acceptance mechanism: fixtures carry source/artifact/mock/expected output and are compiled/replayed; artifact stamps include IR, capability, and required semantics (`conformance/README.md:1-34`; `conformance/choice-basic/artifact.json:4-8`). It can provide replay evidence for a patch candidate, but it does not calculate unintended semantic diff or preserved-ID change by itself.

`scaffold.rs` implements `lute init`/`lute new` source/project generation and documents that generated projects pass check/check-project and selected test/play flows (`crates/lute-cli/src/scaffold.rs:1-17`). It is seed generation, not patching.

## 7. What a semantic diff could observe from existing artifacts/models

### Execution IR

The execution IR envelope has `irVersion`, `capabilityVersion`, compiler-derived sorted/deduplicated `requiredSemantics`, metadata, state, entities, enums, relations, seed facts, rules, commands, and prerequisite edges (`crates/lute-compile/src/ir.rs:23-64`). The compiler stamps capabilityVersion and initializes requiredSemantics before compilation populates semantics (`crates/lute-compile/src/lib.rs:690-694`). `requiredSemantics` is compiler-derived and sorted/deduplicated; it is the natural compatibility-level semantic comparison field, unlike `addr` (`crates/lute-compile/src/ir.rs:25-30`).

A before/after semantic comparison can therefore distinguish structural/address churn from meaningful changes by comparing canonicalized command payloads, command kinds/order/control targets, state declarations/defaults, relations/facts/rules, quest/objective/reward records, choice IDs/effects/targets, lineId/voiceKey sets, host effects/bridges, prerequisite edges, `requiredSemantics`, and metadata. Existing tests already treat `addr`, source, and line identity as removable structural churn when comparing folded component content to a hand-duplicated twin (`crates/lute-compile/src/tests/component_fold.rs:144-164`). This is evidence of a local comparison convention, not an existing general diff API.

### ProjectModel and graph

`ProjectModel::build` assembles all files, resolves inputs, parses/folds/checks, reconciles project diagnostics, optionally compiles each document with source maps, and builds a `ProjectIndex` over successful artifacts (`crates/lute-model/src/project.rs:81-225`). The model exposes documents, index, reconciled outputs, and a graph builder (`crates/lute-model/src/project.rs:81-88`; `crates/lute-model/src/lib.rs:39-43`).

`SemanticGraph` is deterministic and contains nodes plus directed edges with kind, reason, file/span, and evidence (`crates/lute-model/src/graph.rs:89-105`; `crates/lute-model/src/graph.rs:126-145`). It includes dependency edges from state/CEL slots, facts, relations, quest lifecycle, components, commands, project defaults/chapters, and Datalog derivations; derivation intentionally models possible impact even when a fact is not true in the current scenario (`crates/lute-model/src/graph.rs:662-735`; `crates/lute-model/src/derivation.rs:1-8`).

`impact::query` computes a reverse closure from a typed target or fact, retains strongest evidence/explanation paths, groups results into lines/quests/objectives/rewards/disclosures/beats/downstream, and emits file/line/sourceSpan, lineId, speaker, evidence, and reasons (`crates/lute-model/src/impact.rs:9-18`; `crates/lute-model/src/impact.rs:102-138`; `crates/lute-model/src/impact.rs:169-235`). This is the strongest existing substrate for target-scoped context and affected-condition reporting, but it is a query over a freshly built model, not a before/after diff.

### requiredSemantics and project index

`ProjectIndex` is the project-level union of document rows and vocabulary: `irVersion`, one capabilityVersion, sorted requiredSemantics, documents, entities, enums, relations, seedFacts, rules, prerequisite edges, and other project axes (`crates/lute-compile/src/index.rs:53-67`; `crates/lute-compile/src/index.rs:179-183`; `crates/lute-compile/src/index.rs:541-544`). It enforces one capability snapshot across documents and reports mismatch if documents resolve different capabilityVersions (`crates/lute-compile/src/index.rs:566-570`). This can detect project-contract changes and capability mismatches, but it does not represent source revision or explain target-level preserve violations.

### requiredSemantics provenance

The semantic collector records each semantic ID and provenance `(addr, what)` while scanning command fields and CEL expressions (`crates/lute-compile/src/semantics.rs:1-9`; `crates/lute-compile/src/semantics.rs:89-107`; `crates/lute-compile/src/semantics.rs:187-211`). Provenance currently uses addr, so it is useful for reporting which emitted behavior required a semantic capability but inherits addr’s edit instability.

## 8. Appendix B’s twelve tasks, one line each, with constructs and current verification command

The report explicitly lists these twelve tasks and preservation/error targets (`reports/lute_report_03_bundle/lute_report_03.md:901-918`):

1. **New choice:** add a choice while preserving existing choice IDs and effects; touches `<branch>`/`<hub>`/choice options, line IDs, `scene.choices.*`, and control-flow targets; verify with `lute check` or `lute check-project`, then `lute compile`/`lute compile --all`, and replay with `lute test`/`lute trace`/`lute run` as applicable.
2. **Delay clue disclosure:** move a clue later while preserving an alternate progression; touches lore `<entry>`/`<beat>`, content lines/interpolations, fact assertions and guards, `when`/`visibleWhen`; verify `lute check-project`, `lute scenario`, `lute context`, then `lute trace`/`lute test` and `lute run`/differential replay.
3. **Add optional quest:** add a non-required quest without harming main-quest completion; touches `<quest>`, objectives, quest lifecycle state, accepts, prerequisites/connectivity, rewards; verify `lute check-project`, `lute scenario`, `lute constraints --run`, and `lute test`/`lute play`.
4. **Change event period:** alter an event/beat period while preserving completion/reward history; touches occasion/beat `once`/share/spentBy/season/clock scheduling, quest completion and rewards; verify `lute check-project`, `lute beats`, `lute calendar`, `lute trace`, and `lute test`/`lute play`.
5. **Insert before repeated component:** insert a repeated component use before existing instances while preserving existing instance identities; touches `::use`, component expansion, component-scoped lineId/voiceKey, ordinal `{component}#{n}` scope; verify `lute check`, `lute compile`, `lute tag`/`lute loc` identity surfaces, and replay with `lute test`/`lute trace`.
6. **Branch on host result:** add a branch on a host/bridge result while preserving the existing success path; touches plugin bridge directive, declared effects/result slots, CEL condition, choices/branches, host contract; verify `lute check --engine`, `lute check-project --engine`, `lute context`, `lute compile`, then `lute trace`/`lute run`/`lute test` with bridge mocks.
7. **Rewrite item description only:** edit prose while preserving stats, conditions, and references; touches line text, interpolation refs, locale/voice lineId joins, and any nearby item/catalog reference; verify `lute check`, `lute compile`, `lute loc export`/locale merge if used, and `lute test`/`lute trace` to ensure behavior unchanged.
8. **Merge regional manuscripts:** merge documents while preserving shared types and namespaces; touches document `id`, `uses`/schema imports, state/enums/entities/relations/rules, defs/components, duplicate declarations; verify `lute check-project`, `lute context`, `lute compile --all`, and `lute scenario`/`lute constraints`.
9. **Change NPC death condition:** alter survival/death condition while exposing every affected downstream line/quest/reward/disclosure; touches state/facts/relations, CEL guards, lines, quest lifecycle, rewards, lore disclosure; verify `lute impact <dir> state:<path>` or `fact:<relation(args)>`, `lute check-project`, `lute scenario`, `lute constraints --run`, and replay tests.
10. **Remove class/job restriction:** remove an access restriction while preserving narrative rationale and alternative conditions; touches choice/beat/quest guards, cast/entity enums, facts and disclosure conditions; verify `lute check-project`, `lute impact`, `lute scenario`, `lute trace`, and `lute test`/`lute play`.
11. **Move document/scene:** move a document/scene while preserving references, translation, and voice keys; touches file path, `meta.id`/document ID, scene keys, `lineId`/`voiceKey`, locale bundles, project index paths, refs; verify `lute check-project`, `lute compile --all`, `lute loc export`, and `lute test`/`lute trace`.
12. **Add consuming schedule action:** add a time-consuming schedule action while preserving completion before a deadline; touches clock slots, `advances`/duration, occasions/beats, quest deadlines, schedule conflicts, and constraints; verify `lute check-project`, `lute calendar`, `lute beats`, `lute constraints --run`, and `lute trace`/`lute test`/`lute play`.

The report’s exact task table says the preserved properties are, respectively: existing choice IDs/effects; alternate progress; main-quest completable; existing completion/reward history; existing component-instance identity; existing success path; stats/conditions/references; shared types/namespaces; affected downstream meaning; narrative rationale/alternative conditions; references/translation/voice keys; and deadline completion path (`reports/lute_report_03_bundle/lute_report_03.md:901-918`). The report’s required metrics include syntax validity, type/reference validity, intended semantic-change accuracy, unintended semantic diff, preserved-ID changes, repair count, tokens, tool calls, elapsed time, and human review time (`reports/lute_report_03_bundle/lute_report_03.md:920-923`).

## 9. Gaps and risks

- No task-scoped `lute context` input/output contract; current context is whole-file capability vocabulary only (`crates/lute-cli/src/cmd_context.rs:29-115`, `crates/lute-cli/src/cmd_context.rs:393-453`).
- No standalone `expectedType`/`visibleSymbols` query API; only LSP completion/hover and checker-internal expected-type plumbing (`crates/lute-lsp/src/features/mod.rs:1-45`, `crates/lute-lsp/src/backend.rs:647-650`).
- No source revision/hash model; LSP document version is transport-local, capabilityVersion hashes capabilities rather than source, and InputCache keys omit source bytes (`crates/lute-lsp/src/backend.rs:50-51`, `crates/lute-model/src/cache.rs:14-31`).
- No general patch/apply command, base-revision check, atomic multi-file patch protocol, ambiguous-target refusal, or preserve declaration enforcement (`crates/lute-cli/src/cli.rs:238-281`).
- `addr` and semantic provenance based on addr are unstable under edits (`crates/lute-compile/src/address.rs:54-81`, `crates/lute-compile/src/semantics.rs:89-107`).
- Component instance identity is explicitly ordinal and can renumber when an earlier use is inserted (`docs/design/architecture-direction.md:196-203`).
- Graph keys are stable only where their underlying authored keys are stable; fallback path/heading keys and ordinal reward keys remain edit-sensitive (`crates/lute-model/src/graph.rs:157-215`).
- Differential and conformance replay compare runtime behavior, but neither is a before/after semantic-diff or preserve checker (`crates/lute-cli/src/differential.rs:1-20`, `conformance/README.md:1-34`).
- The existing source rewrites (`tag`, `fix`) are narrowly mechanical and whole-file; treating them as a patch substrate would not supply target selection or semantic preservation (`crates/lute-cli/src/rewrite.rs:105-299`).
- Appendix B is an experiment plan, not a previously executed result; the report explicitly says its hypotheses were not yet tested (`reports/lute_report_03_bundle/lute_report_03.md:871-889`).