# Architecture direction after the three independent reviews

- **Status:** Accepted, 2026-10-01.
- **Inputs:** three independent reviews under `reports/`:
  `lute-deep-report.md` ("Deep"), `Lute_2026_Clean_Room_Architecture_Thesis.md`
  ("Thesis"), `lute_report_03_bundle/lute_report_03.md` ("03"). Thesis and 03
  are pinned to `57af7e6` (0.31.0); Deep read `main` at the same time but
  audited docs and layout only, not code.
- **Scope:** product boundary, source of truth, representations, extension
  model, compatibility, identity, diagnostics, AI workflow, toolchain. Each
  decision below is binding for future specs; a spec that contradicts one
  amends this document first.

## 0. Summary

Lute is a **contract-first authoring toolchain**: humans and AI write `.lute`
plus project declarations; the toolchain checks, analyzes and compiles them to
an **execution IR**; engines — including the one we will build — interpret
that IR. Lute owns the *meaning* of what it emits (evaluation order,
required semantics, conformance cases) and a reference executor that pins that
meaning. It does not own a shipped runtime, a VM, or play-time AI.

The next investments, in dependency order: lock the engine contract
(engine-owned state and grant identity in the artifact, a portable
condition contract, an extended conformance corpus) → a concept model that
draws domain-module boundaries and the semantic-id negotiation built on them
→ a project semantic model with impact analysis and author constraints → an
AI edit loop over it → identity, migration and performance hygiene.

The only consumer today is the dogfood corpus, so every change below is a
clean cutover (D6).

## 1. What the reviews agree on

All three reviews, independently:

- keep the Rust core and the dedicated DSL; no rewrite to Lua, GDScript or
  TypeScript;
- Helm contributes packaging, profiles and typed values — never text
  templating;
- Lua/Luau only behind a declared bridge, never inside the core grammar;
- JSX/XML only as optional alternate frontends, never canonical;
- TypeScript is the tooling/SDK boundary and never holds a second copy of
  the semantics;
- separate source, semantic model and execution IR; the current flat artifact
  stays as the execution IR;
- `addr` is position, not identity; provenance and source maps survive to
  the end;
- external results are recorded and replayed;
- AI gets compiler-produced, project-resolved context and structured
  diagnostics, not a spec pasted into a prompt.

These are settled and are not re-litigated below.

## 2. Decisions

### D1. Product boundary: Lute emits contracts; engines execute

- Engines interpret the execution IR. Lute ships **no VM, no embeddable
  runtime, no effect router, no runtime model-provider layer** (Deep's
  runtime half is rejected — §3).
- The reference executor (`lute run` / `play` / `trace` / `test`, the single
  walker of `runtime-unification.md`) stays. Its role is **semantic oracle and
  conformance reference**, not a product runtime.
- Lute still **defines execution meaning**: evaluation order (deadline vs.
  occasion vs. scene, grant order — already partly in
  `runtime/quest-lifecycle.md`), expression profile and function signatures,
  missing-value behavior. Engine neutrality means the meaning is written down,
  not left to each engine.
- **State ownership** stays the existing two-way split: content-written
  slots, and engine-owned slots (`owner: engine`, `state-lifecycle.md`;
  `reserved: true` relations, `cel-and-facts.md`) that content reads but
  never writes. Content changes engine-owned state only by emitting a
  declared request (bridge call, reward grant) that the engine settles.
  Whether the engine settles locally (a packaged game) or forwards to a
  server (live service) is the engine's own business and invisible to Lute;
  there is no separate "server" owner. Reward settlement and persistence
  remain engine duties (`runtime/capability-permissions.md` §"What the host
  still owns").
- `owner: engine` is today a check-time contract only and is not carried in
  the artifact. It moves into the execution IR so engines and the
  conformance corpus can enforce it.
- Author-time AI (writes source that is checked like any source) and
  play-time AI (untrusted dynamic data through an explicit bridge) are
  separate products. Lute's default path is author-time only.

### D2. Source of truth: `.lute` + project declarations

- The authoritative, editable source is `.lute` documents plus project
  declarations (`lute.project.yaml`, plugin manifests and bridge YAML, state
  schemas, catalogs). Every other representation is derived.
- Semantic edits (AI or GUI) **select** by meaning and **apply** to source
  text, then re-check. Comments, prose layout, drafts and Git diffs are
  preserved because the source is never regenerated from IR.
- No Semantic-IR-as-canonical (Thesis §6) and no YAML split of world, quest
  and narrative surfaces (Thesis §9.1). A second surface is admitted only
  when an experiment (03 Appendix B) shows a task class it does measurably
  better.

### D3. Three representations, explicitly named

| Representation | Holds | Stability |
|---|---|---|
| Source (+ lossless syntax) | prose, comments, spans, authored structure, drafts | language-versioned |
| Project semantic model | resolved references, types, reads/writes, domain structure, derived graphs, diagnostic evidence | **internal**; read-only query API first |
| Execution IR (today's artifact) | commands and declarations an engine consumes, required semantics, string/asset joins | **public, versioned contract** |

- Docs and code call the current artifact the **execution IR**.
- `docs/architecture.md` opens with an API stability table: CLI and its JSON
  outputs, execution IR, bridge protocol = public; semantic model query API =
  experimental; AST and checker internals = internal.

### D4. Extension: internal official domain modules

- Between "plugin adds vocabulary" and "edit the whole core", introduce
  **domain modules** — quest lifecycle, clock/schedule, staging/timeline,
  knowledge (facts/relations/rules), and others as they are identified.
- Each module owns: declarations and types it adds; state/facts it reads and
  writes; evaluation timing and order; what it guarantees statically and what
  it does not; its lowering into execution IR; its conformance cases.
- Modules live in the same binary. Splitting crates is optional;
  **dependency and evaluation-order documentation comes first** — a module
  boundary that cannot explain cross-module ordering (deadline → occasion →
  scene → quest state) is not a boundary.
- Plugins remain declarative vocabulary plus typed bridges. No external
  behavior SDK until module contracts are stable. When one opens: hooks run in
  fixed phases, ordering is declared (`after` / `before`, cycles are errors),
  and every transform is followed by full re-validation.
- A new genre need goes to (in order): existing modules composed → a
  module extension → a new module. A new core keyword requires showing the
  first three fail.

### D5. Compatibility: required-semantics negotiation

- Today's gate (`runtime/execution-model.md` §Version negotiation) ignores
  unknown object fields and errors only on unknown command `kind`. A
  meaning-changing optional field (e.g. `advances`, `rewards`) is therefore
  silently dropped by an older engine. This is the hole being closed.
- Scope is **core-language semantics only**. Plugin vocabulary and bridges
  stay under `capabilityVersion` (exact snapshot match) and keep the existing
  `kind: "plugin"` record routed by `(plugin, tag)`
  (`proposals/plugin-system/0.0.7.md`, `runtime/bridge-protocol.md`); there
  is no second extension representation.
- Semantic ids are owned by domain modules (D4): each module publishes one
  versioned id per engine-visible behavior it adds (illustrative:
  `lute.quest.rewards/1`, `lute.clock.advances/1`). The registry is derived
  from the module inventory, so negotiation lands after it (§5 phase 2).
- The compiler collects the ids of features an artifact actually uses into
  `requiredSemantics`. Authors cannot declare it away. Purely descriptive
  fields stay ignorable.
- Engines declare a **capability matrix** (supported ids, unsupported ids,
  policy for unknown effects and missing assets). Load succeeds only if every
  required id is supported; otherwise the engine refuses before playback.
- The four version questions stay distinct: format (how to read), semantics
  (how to execute), requirements (what this artifact needs), build identity
  (which exact artifact a save or test result belongs to).

### D6. Pre-1.0 change policy: clean cutover

- The only consumer is the dogfood corpus. Structural changes land as clean
  cutovers: no compatibility windows, shims or legacy modes.
- Each breaking change migrates the corpus in the same change (`lute fix` or
  a one-shot codemod) and is classed in the CHANGELOG as syntax, semantic, IR
  or plugin breaking.
- This policy ends at the first external consumer; D5 is what makes that
  transition safe.

### D7. Concept model, derived from Lute's own constructs

- The inventory starts from what Lute has: document kinds (scene, quest,
  lore), occasions and beats, state tiers and the clock, facts / relations /
  Datalog rules, choice / hub / match, quest lifecycle and rewards, staging
  and timelines, bridges, components. Each construct is mapped to its owning
  module (D4), its reads and writes, its evaluation point and its lowering.
- Thesis's primitive list (Entity, State, Relation, Event, Beat, Choice,
  Effect, View, Constraint, Reference) is a **lens for finding gaps and
  overlaps**, not a target vocabulary. Lute's terms win where they collide
  (`beat` keeps its existing meaning).
- Organizing units (scene, shot, timeline, directive, component, plugin,
  file) are kept distinct from the semantic units modules reason about.
- It is not a syntax change by itself.
- Truth structure (world canon, observation, testimony, belief, inference,
  disclosure policy) is expressed with existing project-declared relations,
  not new core keywords. World truth never automatically counts as player
  knowledge.

### D8. Identity

- No second identity store. Existing keys keep their roles: `meta.id`
  (canonical document key), per-speaker `code` → `lineId` / `voiceKey`
  (content joins, back-filled by `lute tag`), the author-declared ids of
  dsl 0.29.0 §1 (scene, beat, entry, quest, objective, branch, hub, choice
  and mark ids; `share=` keys; season, relation, def, enum and entity
  members; component and template params; plugin occasions and events), and
  `addr` (build-local position).
- The work is to document the identity source of **every authorable node
  kind** and close the gaps: node kinds with no stable id, and component
  instances, whose `{component}#{n}` segment is ordinal
  (`runtime/execution-model.md` §Addressing) and renumbers when an earlier
  `::use` is inserted. Those get stable instance keys.
- AI patch targeting (D10) addresses nodes through this documented
  composite, not through a new `nodeId` field.
- Move and rename are tracked (alias or ledger), never modeled as
  delete-plus-create.

### D9. Diagnostics: evidence level and author constraints

- Build on the verdicts Lute already computes: *impossible* / *guaranteed* /
  *possible* (dsl 0.20.0; sound proofs, *possible* = undecided), `trace`'s
  `unknown` (exit 3), and `calendar`'s grid (bounded over stated axes).
  Unify them into one evidence vocabulary across `check`, `scenario`,
  `beats`, `calendar`, `trace` and `play` output, adding *Witnessed* (a
  play/trace input reproduces it) and *Bounded* (checked within a stated
  scope). "No path found within bounds" is never reported as "no path
  exists".
- **Constraint** is first-class: authors declare invariants (e.g. "this
  quest is completable on every path", "no mandatory progress exists only in
  a single schedule slot", "a dead NPC never speaks"). The checker reports
  each with its evidence level.
- Genre policy (e.g. 0.31's "only significant actions consume time") is
  documented separately from execution contract ("the host advances time").
- Artistic judgment (tone, foreshadowing, over-explanation) stays outside
  the checker; AI critique may advise, never gate.

### D10. AI edit loop

- The compiler is the AI's oracle. `lute context` evolves from a whole-file
  capability surface into **task-scoped context**: target node, its declared
  reads/writes, direct references, affected conditions, related tests, source
  revision, and an explicit list of what was *not* included (dynamic or
  retrieval-only relations are labeled as such).
- Position queries (`expectedType`, `visibleSymbols`) come from the same
  resolution `lute-lsp` uses.
- Edit flow: Inspect → Plan → Patch → Check → Replay → Review. A patch names
  its base revision and target nodes and states what it must preserve
  (public ids, choice effects, host contracts); application is atomic,
  rejects stale revisions and ambiguous targets, and `preserve` is enforced
  by a **semantic diff** of before/after, not trusted from the model.
- Interface order: structured CLI JSON first, then thin adapters for
  existing harnesses. No Lute-specific chat app, session manager or general
  agent harness.

### D11. Toolchain hygiene

- **Engine conformance corpus**: extend the existing one (`conformance/`,
  ten fixtures; byte-identity replay in
  `crates/lute-cli/tests/conformance.rs`; `differential.rs`) to cover every
  command kind, every quest transition and reward grant, `owner: engine`
  refusal, and later every semantic id. It is the acceptance suite for the
  engine we build. Comparison stays over declared observables (choices
  offered, line ids presented, effects requested, quest/knowledge changes),
  not rendering.
- `lute fmt`: one canonical form, comment-preserving, idempotent.
- Property tests: formatter idempotence, IR serialize/deserialize round
  trip, parser never panics on arbitrary input.
- Proposals and design docs carry a status: Draft, Accepted, Implemented,
  Superseded, Rejected (e.g. `runtime-unification.md` still reads "design
  for wave 2" although its `Machine` landed).
- A current-rules **handbook** separate from the versioned delta history.
- A benchmark corpus (tiny / medium / large) with cold/warm load, project
  resolution, analysis and serialization timings as regression gates.

### D12. Conditions are standard CEL, from source to IR

- Authors are mostly AI, and AI already knows CEL. So conditions are
  **written** in standard CEL — "write CEL" must be a complete instruction.
  Lute-only condition sugar is removed from the source language: Prolog-style
  fact patterns with bare-name atoms (`holds(inParty(elena, _))`), the
  path-taking `isSet(path)`, and all-double numbers. The dogfood corpus is
  migrated by codemod (D6).
- Fact queries are ordinary CEL function calls over a relation name and an
  argument list (shape: `holds("inParty", ["elena", "_"])`; bound
  expressions such as `occasion.target` are ordinary list elements).
  Presence is the standard `has()` macro. The exact spelling of the wildcard
  and of entity references is settled in the phase-1 spec.
- Numeric types are CEL's **`int`** and **`double`** (`number` is removed);
  literals are typed as in CEL. The checker enforces CEL's overload rules
  (no mixed int/double arithmetic, `%` on ints only), so a checked condition
  never hits a missing overload at runtime.
- Every condition slot in the execution IR carries **both** the condition's
  CEL text and its **`expr`** JSON AST, and `expr` now covers the whole
  profile, host-bound function calls included. The engine chooses how to
  evaluate: an off-the-shelf CEL implementation (cel-go, cel-js/cel-es,
  cel-dart, a Unity/C# port, …) or an `expr` walker. Lute guarantees the two
  mean the same thing.
- The artifact carries the **environment declaration**: root variables and
  their types, and the signatures of the host-bound functions (`holds`,
  `count`, `countDistinct`, `validAt`, `visited`, …). Lute specifies their
  semantics; the engine implements them over its fact store and visited set.
- The activation rule is normative: state as nested maps, a path present
  iff it has an effective value (write → seed → default), names that are not
  identifiers reached by index. The runtime policy for a condition that
  evaluates to an error is specified once and pinned by conformance cases.
- Equivalence is proven, not assumed: CI evaluates the conformance corpus's
  conditions through Lute's evaluator, `@bufbuild/cel` (cel-es; runs
  cel-spec's official conformance data; Node is already in CI), `cel-go`
  (`cel.dev/cel-go`, the reference implementation; one `setup-go` step), and
  an `expr` walker, and requires identical results. Neither is a candidate
  for Lute's own executor, so both stay independent of it.
- To settle in the phase-1 spec: `/` on ints (CEL truncates), whether
  `count` returns `int`, the wildcard and entity-reference spelling, and
  whether the reference executor evaluates through a CEL library
  (`cel-interpreter`) instead of `lute-trace`'s evaluator, which keeps
  three-valued unknowns for `trace` preview only.

## 3. Rejected

| Proposal | Source | Why |
|---|---|---|
| Lute VM / embeddable runtime / effect router | Deep | Violates D1; engines execute and settle engine-owned state (rewards, inventory, currency), locally or via their own server. |
| Runtime model invocation as a core operation | Deep | Play-time AI is a separate product (D1); breaks static analysis into Unknown. |
| Wasm / subprocess / native runtime plugin tiers, MCP bridge in core | Deep | No runtime to plug into. Tooling-side TS runs out of process via CLI JSON (D10). |
| Execution bytecode | Deep | No runtime to optimize; engines choose their own representation. |
| Generic-language surface (`let`/`fn`/`type`/generics) | Deep | Existing state declarations and CEL cover the need; growth goes to domain modules (D4). |
| Semantic IR as canonical source | Thesis | D2. |
| YAML split of world/quest/narrative surfaces | Thesis | D2; only via measured experiment. |
| Lua/GDScript/TS as content language; JSX/XML canonical; Helm templating | all | §1. |
| Public plugin registry, marketplace, multi-engine official runtimes | Deep, Thesis | Not needed to test the core hypothesis. |

## 4. Already present — extend, do not rebuild

`lute --explain <CODE>` (stable diagnostic codes); `lute version` (toolchain
/ language / IR axes); `lute-lsp`; checker golden tests
(`crates/lute-check/tests/golden.rs`); `lute fix` and `lute tag`; the one
walker `lute_trace::exec::Machine` behind `run` / `play` / `trace` / `test`;
the conformance corpus and differential harness (D11); doc-snippet CI
(`.github/workflows/docs.yml`, `scripts/check-doc-snippets.py`); the
impossible / guaranteed / possible fact verdicts (D9); `lute context`
(shared `build_input` / `fold_env` resolution); `scenario`, `beats`,
`calendar`, `refs`, `lore`, `loc`; `replay: recorded` bridges; `owner:
engine` state and `reserved` relations (D1).

## 5. Roadmap

Phases are ordered by dependency, not calendar. Exit criteria are
observable.

1. **Engine contract.** Execution-IR naming and stability table (D3);
   `owner: engine` carried in the artifact (D1); a quest-instance identity
   on grant events (§6); standard-CEL source conditions with `int`, codemod
   of the corpus, full-profile `expr`, environment declaration (D12);
   conformance corpus extended (D11). *Exit:* the corpus covers every
   command kind, every grant and an `owner: engine` write refusal; every
   emitted condition evaluates identically under Lute, `@bufbuild/cel`,
   `cel-go` and an `expr` walker; replaying a grant with the same
   instance identity is detectable.
2. **Concept model and negotiation.** Construct inventory mapped to modules
   (D7); domain-module boundaries with evaluation-order documentation (D4);
   semantic-id registry, `requiredSemantics`, engine capability matrix (D5);
   handbook skeleton (D11). *Exit:* for any construct, the team can name its
   owning module, reads/writes, evaluation point, lowering and semantic id;
   an artifact using an id the engine does not declare is refused before
   playback; the corpus covers every id.
3. **Project semantic model.** One project snapshot shared by `check`,
   `context`, `scenario`, `beats`; impact query ("what changes if this node
   changes"); Constraint declarations; evidence levels on analyses (D9).
   *Exit:* changing an NPC's survival condition lists every affected line,
   quest, reward and disclosure, each with its reason and evidence level.
4. **AI edit loop.** `lute fmt`; task-scoped context; patch with base
   revision and `preserve`; semantic diff (D10). *Exit:* the 12-task suite of
   03 Appendix B runs end-to-end on the dogfood corpus, recording validity,
   unintended semantic diff and preserved-id changes.
5. **Identity, migration, performance.** Identity source documented per
   node kind, stable component instance keys (D8); breaking-change classes in
   CHANGELOG (D6); benchmark gates (D11). *Exit:* inserting a component use
   renumbers nothing; benchmark regressions fail CI.

## 6. Findings at HEAD that feed the roadmap

Verified against `57af7e6` during review of this document:

- Quest grant events carry quest and objective ids but **no quest-instance
  identity**; an engine cannot make grants idempotent across save/reload,
  retry and repeatable-quest runs from the event alone → phase 1.
- Relational conditions (`holds`, `count`, `visited`) are emitted as raw
  Lute-CEL text; only the closed scalar profile gets portable `expr`. Lute
  uses `cel-parser` only and evaluates with its own evaluator, whose
  semantics differ from standard CEL (pattern arguments, path-taking
  `isSet`, all-double numbers) → D12, phase 1.
- Checker goldens pin diagnostics and resolved views, not emitted IR; only
  the conformance fixtures pin artifacts → extend in phase 1.
- Project resolution is rebuilt on every CLI invocation, not cached →
  measure in phase 5 before adding any cache or daemon.

## 7. When to revisit

- Current DSL + task-scoped context already makes AI edits reliable →
  lower the priority of semantic patches.
- Domain-module separation does not reduce what a task must know → stop
  splitting.
- A TS builder or other surface beats `.lute` on the same validation and
  human-editability measures for some task class → admit it for that class.
- An external consumer appears → D6 ends; D5 becomes the compatibility
  mechanism.
