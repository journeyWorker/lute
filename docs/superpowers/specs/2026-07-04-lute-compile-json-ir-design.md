# Lute Compiler — Typed JSON IR Codegen (v0.0.1)

- **Date:** 2026-07-04 · **Status:** Draft (design, pre-implementation; revised after reviewer +
  identifier-model cleanup).
- **What:** the missing final stage — lower a *checked* `.lute` document to a typed **JSON**
  command-record artifact the runtime engine plays.
- **SoT:** [`scenario-dsl/0.0.1.md`](../../proposals/scenario-dsl/0.0.1.md) (language);
  [`architecture.md`](../../architecture.md) (compiler passes, auto-injection).
- **Reference (old, superseded):** the bard TS compiler
  (`packages/lute-core/…/inline/{parser,generator}.ts` → `StoryScript[]` = `idola_script_commands`).
  Consulted for the *lowering approach* + the row-addressing/`next`-linking convention; its
  all-string eevee-table schema is **not** the target.

---

## 1. Goal & gap

Everything upstream exists in the Rust toolchain (parse, check, CEL-slot resolution,
stage-resolution preview, timeline resolution). `check()` deliberately stops short:
`lute-check/src/check.rs` says its resolved view is *"the compiler's best-effort read … **WITHOUT
final flat-record codegen (scoped out of this plan)**."* This spec closes that gap and adds a
`lute compile` CLI emitting JSON.

## 2. Reuse vs build

| Concern | Where | Status |
|---|---|---|
| Parse → AST · CEL slot parse/scan | `lute-syntax`, `lute-cel` (`scan_refs`/`parse_slot`) | reuse (parse/validation aids) |
| Static validation + **folded** state schema + defs/component tables | `lute-check` (`check()`, `Ctx`, `meta.rs`, `component_import.rs`) | reuse (gate + inputs) |
| Auto-injection reducer (`StageState`, named rules, `InjectedCommand`+`Provenance`) | `lute-check/src/inject.rs` (`lower_node`) | reuse per-node core; drive from a **new CFG walker** (D9) |
| Timeline scheduling math (omitted-`at` cursor, sort, barrier) | `lute-check/src/timeline.rs` | reuse math (its `ResolvedRow` is a debug preview, not records) |
| Capability snapshot / project resolution | `lute-manifest`, `lute-check` | reuse |
| Per-speaker `code` counter | `lute-check/src/tag.rs` | reuse for `lineId` derivation |
| **Typed record IR + serialization** | — | **build** |
| **Component + persist AST normalization** (D8) | — | **build** |
| **`@ref`/`@fn(args)`/`$` → inline-CEL expander** (D4) | — | **build** (NOT `substitute_dsl_tokens`) |
| **Direct lowering + CFG + branch/match flattening** | — | **build** |
| **CFG-aware stage resolution (fork/join)** (D9) | — | **build** |
| **`addr` + `lineId` assignment** | — | **build** |
| **`lute compile` CLI** | `lute-cli` | **build** |

> **Correction (reviewer):** `lute-cel::substitute_dsl_tokens` is a private, length-preserving
> *parser-prep* helper (blanks `@`/`$` so `cel-parser` accepts a slot); it does **not** expand defs
> or bind params. Real `@ref` expansion is new (D4).

## 3. Decisions (normative)

- **D1 — New typed JSON IR** *(user)*. Not the legacy all-string `idola_script_commands`. A
  discriminated-union record schema fit for the current language incl. its new constructs.
- **D2 — Flat, ordered sequence** (DSL §3 inv. 2, §11.5). The artifact is a **flat array**;
  branching flattens to CEL-guarded records + jump targets + convergence (§7). Nested-tree output
  would violate §3.2 → out.
- **D3 — Tagged records.** `kind` discriminator + typed camelCase fields; only relevant fields
  present. CEL is a first-class string field.
- **D4 — Compile-time `@ref`/`$` expansion (new expander).** Per §8.1 a def ref is a compile-time
  macro. A new expander consumes the merged inline+imported+plugin defs table (`lute-check::Ctx`),
  binds `@fn(args)` params positionally with checked types, **parenthesizes** each substituted body,
  expands **recursively (cycle-guarded)**, then substitutes `$` with the enclosing `<match>` subject.
  Output CEL is free of `@`/`$`; the artifact carries no `defs` table.
- **D5 — Thin resolution.** Emit `assetId` as authored (asset→file-path is the engine/asset layer;
  ids are checker-validated). Text is **monolingual** with a stable `lineId`; translation sidecar
  merge is external tooling (§12). Asset-path resolution + translation merge are **deferred** (§9).
- **D6 — Compile gates on a clean check.** Any `Error` diagnostic aborts codegen (exit 1). This lets
  codegen *rely* on invariants the checker proved (declared paths, `<match>` exhaustiveness incl.
  `unset`, acyclic components, `@ref` arity/type, timeline one-writer-per-track, and **unique `<choice id>` within a branch** — the last is a new `lute-check` diagnostic `E-CHOICE-DUP` (§11.1) this gate requires).
- **D7 — Placement.** New crate **`lute-compile`** (deps: `lute-syntax`, `lute-cel`, `lute-manifest`,
  `lute-check`) + a `lute compile <file> [--json] [--project <dir>] [-o <out>]` subcommand. DAG
  `…-check ← lute-compile ← lute-cli` is acyclic (reviewer-confirmed).
- **D8 — Components + persist are AST-normalized *before* lowering.** `::use` expansion and
  `<choice persist=…>` sugar rewrite the **node tree** (real `Node`s) so they participate in `@ref`
  expansion, lowering, auto-injection, and lookahead. Expanding after lowering (reviewer Critical)
  strands them.
- **D9 — Stage resolution is control-flow-aware (fork/join).** Auto-injection threads over a **CFG**,
  not a linearized stream: clone `StageState` at each branch/match entry, lower each arm
  independently with only CFG-reachable lookahead, **join** arm-exit states at convergence (§7.3).
  Linearizing mutually-exclusive arms (reviewer Critical) leaks one arm's stage into the next.

## 4. Output schema

### 4.1 Envelope

```jsonc
{
  "lute": "0.0.1",
  "meta": { "character": "bianca", "season": 1, "episode": 2,
            "episodeId": "S01EP02", "title": "Behold the Performance of All-Purpose Bianca" },
  "state": [                             // RESOLVED + FOLDED schema (D6), not author-only
    { "path": "scene.affect.bianca",  "type": "number", "default": 0 },
    { "path": "scene.flags.saw_beam", "type": "bool",   "default": false },
    { "path": "scene.choices.number", "type": "enum", "domain": ["blunt","soft","unset"],
      "default": "unset", "provenance": "branch:number" }   // implicit, from <branch id> (§11.1)
  ],
  "commands": [ /* flat ordered array, §4.4 */ ]
}
```

`state` is the *folded* runtime schema `check()` already builds — author `state:` + **implicit**
`scene.choices.<branchId>` (domain = choice ids + `unset`, §11.1) + plugin slots — so the engine can
init/type all paths. `defs` are absent (D4).

### 4.2 Identifiers — three, one job each

Three identifiers — *position/flow*, *line identity + localization*, *voice file*. Each answers
exactly one question, so they never overlap. **There is no separate `textUnitId`:** the localization
key IS `lineId` (§12's own "composed lineId"), so that redundant name is retired. `voiceKey` stays —
voice files are found and attached by a separate build step that needs an explicit key.

| id | axis | purpose | shape |
|---|---|---|---|
| **`addr`** | *where* (position/flow) | record order **and** the jump/`target`/`converge` key | `"{shot:03}-{idx:04}"`, `idx` **+100** within each shot over all emitted records (authored + injected). Compiler-owned, deterministic; the +100 gaps leave room to hand-insert a row (`004-0650`). Exactly the legacy `{shot}-{index}` + `next` scheme. |
| **`lineId`** | *which line* (identity + localization) | the stable line id: the i18n sidecar key `lineId → { lang: text }` **and** the identity a voice file is matched to — on every `line` + every `choice.options[]` label | line: `{character}.s{s}ep{e}.{speaker}_{code}`; option label: `{character}.s{s}ep{e}.{branchId}.{choiceId}`. Assigned once, **never recomputed** — `lute tag` persists the `code` into source (the Yarn `#line:` model, §12). |
| **`voiceKey`** | *which voice file* (audio) | the voice-asset lookup/attach key — emitted on **voiced** lines (`dialogue`/`voiceover`) so a separate voice-attach build step finds and inserts the clip | aligned to `lineId`: core `{characterId}-{code}`; when the character-cast plugin lands, `{voiceBank}-{code}` with a static `costumeVoiceBank` (§7.3). **Namespace:** unique within the episode/voice package — a global consumer pairs it with `meta.episodeId`. |

> **0.1.0 note (choice-label `lineId`):** the `s{s}ep{e}` middle segment is `meta.episodeId` (default `s{season:02}ep{episode:02}`), pinned in 0.1.0 as the derivation input; this episode-prefixed choice/hub-label shape is confirmed correct — see the *DSL 0.1.0 addendum (2026-07-06)* below.

`addr` is regenerated each compile (a position); `lineId`/`voiceKey` are stable content joins that
survive text edits, insertion, and reordering. `lineId` and `voiceKey` are distinct because a voice
bank can differ from the character (cast's costume/era voice — a child/flashback bank); in the core
(cast-inactive) case `voiceKey` is just `{characterId}-{code}`, aligned to `lineId`. Unvoiced lines /
choice labels carry `lineId` (i18n) but no `voiceKey`. No dense integer id, no standalone
`code`/`shot`/`textUnitId` field; `addr` never keys content, and `lineId`/`voiceKey` never key flow.

**Which id for which task** — so neither an AI nor a human has to guess:

| task | id |
|---|---|
| join a translation string | `lineId` |
| find / attach a voice file | `voiceKey` (voiced lines) |
| control-flow jump / convergence target | `addr` |
| hand-insert a row between two rows | an `addr` in the +100 gap |
| record / read an intra-episode choice | `scene.choices.<branchId>` vs a `<choiceId>` |
| carry a choice across episodes | a named declared `run.*` fact (via `<choice persist>`) |

### 4.3 Common fields

`kind` (discriminator) + `addr` always. Where meaningful: `wait` (bool, **resolved** effective
blocking = manifest per-directive default ⊕ author override); `duration`/`delay` (s); `at` (s) +
`timeline` (int) for a `<timeline>` clip; `provenance` `{ injected:true, by, reason }` on injected
records; `source` `{ component }` on component-expanded records.

### 4.4 Record kinds

| `kind` | Source | Fields (beyond `kind`/`addr`) |
|---|---|---|
| `line` | `:line` | `role`=`dialogue`\|`narration`\|`monologue`\|`voiceover`; `speaker`; `text`; `lineId`; `voiceKey?` (voiced roles); `emotion?`, `variant?`, `action?`, `dialogMotion?`, `as?` |
| `background` | `::bg` | `location?`, `time?`, `assetId?` (`wait` default true) |
| `music` | `::music` | `action`, `mood?`, `volume?`, `assetId?`, `track?` |
| `sfx` | `::sfx` | `sound?`, `assetId?`, `name?` |
| `vfx` | `::vfx` | `vfxType`, `label?`, `transition?` |
| `sprite` | `::auto` **or** an injected command | authored: `character`, `anchor?`, `action?`, `exit?`. injected (`provenance`, separate record): one of `anchor` / `posReset:true` / (`preload:true`+`emotion`) / `exit:true` (§7.4) |
| `camera` | `::camera` | `focus?`, `zoom?`, `moveX?`, `moveY?`, `shake?`, `reset?`, `easing?` (+ `duration`/`delay`/`wait`) |
| `cut` | `::cut` | `assetId`, `action?`, `full?` (+ `wait`) |
| `video` | `::video` | `assetId`, `action?` (`wait` default true) |
| `plugin` | any non-core (plugin-provided) `::`directive | `tag` (directive name); `fields` (attr map, each value typed via the directive's manifest `AttrDecl`) — a passthrough the plugin runtime interprets; core codegen invents no per-plugin schema |
| `set` | `::set` or synthesized persist | `path`, `op`=`=`\|`+=`\|`-=`\|`*=`, `value` (CEL, expanded) |
| `choice` | `<branch>` | `branchId`, `recordKey`=`scene.choices.<branchId>`, `options`:`[{ id, label, lineId, when?:<cel>, target:<addr> }]`, `converge`:<addr> |
| `match` | `<match>` | `subject`(cel), `arms`:`[{ test:<cel>, target:<addr> }]`, `otherwise?`:<addr> (present unless finite & fully covered, §11.2), `converge`:<addr> |
| `jump` | flatten-inserted | `target`:<addr> — control-flow return to a convergence at an arm/choice-body end |
| `barrier` | `<timeline>` end | `timeline`:<int>, `at`:<barrierAt> — blocks until the timeline completes |

> `recordKey` on a `choice` is the concrete state path `scene.choices.<branchId>` (§11.1) — an alias of that path, not a separate author id to mint.

### 4.5 Worked examples

Shot 2 (`::auto`→ authored sprite; entry-emotion preload → **separate** injected sprite; `addr` gapped +100):

```jsonc
[
  { "kind":"sprite", "addr":"002-0100", "character":"bianca", "anchor":"center", "action":"fade-in-up" },
  { "kind":"sprite", "addr":"002-0200", "character":"bianca", "preload":true, "emotion":"surprised",
    "provenance":{ "injected":true, "by":"entry-emotion-lookahead",
      "reason":"pre-loading bianca's first emotion `surprised` seen ahead of the entrance" } },
  { "kind":"camera", "addr":"002-0300", "focus":"bianca", "zoom":1.1, "duration":0.5, "wait":false },
  { "kind":"line", "addr":"002-0400", "role":"narration", "speaker":"narrator",
    "text":"A hostess walked over with a menu pressed to her chest.",
    "lineId":"bianca.s01ep02.narrator_0010" },
  { "kind":"line", "addr":"002-0500", "role":"dialogue", "speaker":"bianca",
    "emotion":"surprised", "variant":0, "text":"Oh!",
    "lineId":"bianca.s01ep02.bianca_0010", "voiceKey":"bianca-0010" }
]
```

Branch (Shot 4) — flattened; `target`/`converge` are `addr`s; `soft` arm carries an in-arm `set`:

```jsonc
[
  { "kind":"choice", "addr":"004-0500", "branchId":"number", "recordKey":"scene.choices.number",
    "options":[ { "id":"blunt", "label":"Just ask, flatly", "lineId":"bianca.s01ep02.number.blunt", "target":"004-0600" },
                { "id":"soft",  "label":"Ask gently",       "lineId":"bianca.s01ep02.number.soft",  "target":"004-0800" } ],
    "converge":"004-1100" },
  { "kind":"line", "addr":"004-0600", "role":"dialogue", "speaker":"fixer", "text":"Bianca. Your number.",
    "lineId":"bianca.s01ep02.fixer_0050", "voiceKey":"fixer-0050" },
  { "kind":"jump", "addr":"004-0700", "target":"004-1100" },                       // end of `blunt`
  { "kind":"line", "addr":"004-0800", "role":"dialogue", "speaker":"fixer",
    "text":"Bianca — would you mind terribly if I had your number?", "lineId":"bianca.s01ep02.fixer_0052", "voiceKey":"fixer-0052" },
  { "kind":"set",  "addr":"004-0900", "path":"scene.affect.bianca", "op":"+=", "value":"1" },
  { "kind":"jump", "addr":"004-1000", "target":"004-1100" },                       // end of `soft`
  { "kind":"line", "addr":"004-1100", /* convergence continues */ }
]
```

Match (Shot 5) — `@fond` expanded to inline parenthesized CEL, `$` → subject:

```jsonc
{ "kind":"match", "addr":"005-0700", "subject":"scene.choices.number",
  "arms":[ { "test":"(scene.affect.bianca >= 1)", "target":"005-0800" },
           { "test":"scene.choices.number == 'blunt'", "target":"005-1000" } ],
  "otherwise":"005-1200", "converge":"005-1400" }
```

## 5. Pipeline

Pure, deterministic: `compile(document, snapshot, providers) → Result<Artifact, Vec<Diagnostic>>`.
Codegen runs only on a clean check (D6).

1. **Parse + fill CEL + check (gate).** Reuse `check()`; any `Error` → return diagnostics, no
   artifact. Yields the resolved snapshot (`wait` defaults/schemas), the folded state schema, and the
   defs + component tables.
2. **AST normalization (D8).** (a) `::use` → inline the component body `Node`s with each `@param`
   bound (recursive, acyclic per checker), tagged with `component` provenance; (b) choice `persist`
   → append a synthesized `Set` node to the persisting `<choice>` body. Output: a tree with no
   `::use`, persists as real `Set`s.
3. **CEL expansion (D4).** Every `CelSlot`: `@ref`/`@fn(args)` → inline parenthesized CEL (typed
   positional binding, recursive, cycle-guarded); then `$` → enclosing `<match>` subject.
4. **Direct lowering + CFG (Pass 1).** Each primitive node → typed record(s) (schema-driven, pure).
   `<branch>`/`<match>` → header + one arm subgraph per choice/arm + a convergence label. Nested
   branch/match: the arm's trailing `jump → outer converge` is emitted **after** the inner
   convergence (reviewer-verified sound under symbolic labels). `<timeline>` → a schedule group handled **inline in Pass 2** (below).
5. **CFG-aware stage resolution + timeline (Pass 2, D9).** Thread `inject.rs::StageState` over the
   CFG in document order: clone at each branch/match entry; lower each arm with lookahead restricted
   to **CFG-reachable** successors (never sibling arms); interleave `InjectedCommand`s as **separate**
   `sprite` records with provenance; **join** arm-exit states at convergence (§7.3). A `<timeline>` is
   handled **inline in this same walk** (not a later pass): schedule its clips via `timeline.rs` math
   (omitted-`at` cursor → absolute `at`), thread the resolved clips through the **same reducer** in
   deterministic `(at, track-order)` order — a clip MAY be a stage-changing `::auto`/`::bg` — stamp
   every emitted record with `timeline`+`at` (+`duration`), append a `barrier` at `duration` (or max
   resolved end), and carry the reducer's **post-barrier** `StageState` to the next node. Ordering is
   load-bearing: the node after a timeline is injected from the timeline's resulting stage, never
   stale pre-timeline state.
6. **Addressing + identity (final pass).** Assign `addr` (`{shot:03}-{idx:04}`, `idx += 100` per
   shot) to every record in final order; resolve every symbolic `target`/`converge` label — a
   compiler-internal temporary produced during flattening, never author-facing and never serialized
   — to the concrete `addr`; assign `lineId` (every line + option label) **and** `voiceKey` (voiced
   lines; core `{characterId}-{code}`, §4.2), back-filling a per-speaker `code` where absent (mirroring `tag.rs`).
7. **Serialize.** `serde_json` the `Artifact`; deterministic field/record order (byte-stable).

## 6. Crate layout (`lute-compile`)

- `ir.rs` — `Artifact`, `Command` (tagged, `Serialize`), field structs, `StateEntry`.
- `normalize.rs` — D8: `::use` expansion + choice-`persist` `Set` synthesis.
- `expand.rs` — D4: `@ref`/`@fn(args)`/`$` inline-CEL expander (typed binding, recursive, cycle-guarded).
- `lower.rs` — Pass-1 direct lowering.
- `cfg.rs` — CFG build + branch/match flattening (headers, arm subgraphs, `jump`, convergence).
- `stage.rs` — D9 CFG-aware reducer driver over `inject.rs` (fork / reachable lookahead / join); handles a `<timeline>` inline via `schedule.rs` and carries post-barrier state forward.
- `schedule.rs` — timeline clip scheduling (reusing `timeline.rs` math), **invoked by `stage.rs`** during the CFG walk → stamped records + `barrier`.
- `address.rs` — final `addr` assignment + label resolution + `lineId`/`code`.
- `lib.rs` — `compile(...)` orchestration + public re-exports.

`lute-cli`: a `Compile { file, json, project, out }` subcommand — thin shell (read → `compile()` →
write JSON to `-o`/stdout; exit 0 clean / 1 check-error / 2 I/O).

## 7. Control flow (flat VM contract)

Executed by a sequential VM keyed on `addr`:

1. **Flow.** Fall-through in `addr` order. `choice`: present each `option` whose `when` is true; on
   pick, record `recordKey = <optionId>`, set PC ← option `target`; never falls through (player must
   pick); each option body ends in `jump → converge`. `match`: evaluate `arms` top-down
   (first-match-wins, §11.2), PC ← first true arm's `target`, else `otherwise`; `otherwise` may be
   omitted only when the clean-check gate proved the domain finite & fully covered (incl. `unset`);
   each arm body ends in `jump → converge`. `jump`: PC ← `target`. `barrier`: block until the
   `timeline` completes, then fall through.
2. **Nesting.** A choice/arm body ending in a branch/match lays its inner convergence first, then the
   outer trailing `jump → outer converge` after it, so control always reaches the outer converge.
   Empty arm = a bare `jump → converge`.
3. **Stage join at convergence (D9).** Per on-stage character / sprite slot (`anchor`/`pose`/
   `emotion`): identical across all arms → carry forward; differing (or present in some arms only) →
   `Unknown`. After convergence, injection treats `Unknown` conservatively (a following plain line
   makes no pose assumption → no false `posReset`; a following `::auto` for an `Unknown`/absent
   character is a fresh show → anchor + preload). Never use-after-frees, never leaks an arm's stage
   into the spine; cost is occasional redundant re-anchoring. *v1 limitation (documented):* joins are
   conservative, not minimal.
4. **Injected sprites (§7.4).** `InjectKind` → `sprite` records (each `provenance.injected=true`,
   separate from authored `::auto`): `Anchor` → `{character,anchor}`; `PosReset` →
   `{character,posReset:true}`; `SpriteLoad` → `{character,preload:true,emotion}`; `Hide` →
   `{character,exit:true}`.

CEL (`when`/`test`/`subject`/`set.value`) is evaluated by the engine's runtime CEL against live
state; the compiler guarantees only that the strings are well-formed, typed, and `@`/`$`-free.

## 8. Testing

- **Golden-per-construct** (arch #9): a minimal snippet per record kind → asserted JSON. Every
  directive, `:line` role, `::set` op, branch (+`when`,+`persist`), match (+`otherwise`,
  +finite-covered omission), timeline (+barrier +a stage-changing clip), `::use`, `@ref`/`@fn`/`$`.
- **Injection goldens** (arch #10): the four rules' separate `sprite` records + provenance, incl. a
  **branch fork/join** case (arm 1 shows bianca, arm 2 speaks bianca → arm 2 still gets its own
  show/anchor/preload).
- **E2E goldens**: `bianca-s01ep02`, `showcase/`, `components/` → full-artifact snapshots
  (`cargo-insta`).
- **Determinism**: same input ⇒ byte-identical JSON. **Gate**: an `Error` doc emits no artifact.
- Tests authored by the `Tester` agent.

## 9. Scope

**In (v1):** every Appendix-A directive incl. `::camera`; `:line` (all roles, `dialogMotion`, `as`);
`::set` (all ops); `<branch>`/`<choice>` (+`when`,+`persist`/`as`/`value`, + `E-CHOICE-DUP` unique-choice-id gate in `lute-check`); `<match>` (+`otherwise`,
+finite-covered omission); `<timeline>`/`<track>` (+stage-changing clips, +barrier); `::use`;
`@ref`/`@fn(args)`/`$`; folded-state envelope; `addr` + `lineId` + `voiceKey`; CFG-aware auto-injection with
provenance; `lute compile` → JSON; goldens + e2e + determinism.

**Out (deferred):** asset-id→file-path resolution (engine/asset layer); translation sidecar merge
(external, §12); runtime CEL evaluation, incremental/streaming compile, warm daemon, engine playback.

## 10. Execution model (agent-driven)

The project's subagent-driven controller pattern (HANDOFF): this spec → reviewer (done) →
`writing-plans` plan → **serial** implementer subagents (one shared worktree/index) → **mandatory
`reviewer` per task** (verdict → `.superpowers/sdd/briefs/<task>-review.md`) → fix wave for
Critical/Important (TDD + re-review) → controller fold-in/commit/ledger → whole-branch review →
ff-merge. Branch `feat/lute-compile`. Per-task hygiene: `cargo fmt`/`clippy -D warnings`/own-crate
tests before commit; full-workspace gate at plan end. New public types live in `lute-compile`; any
`lute-check` surface widening (to expose the folded schema / defs table) → grep call sites + rebuild
`-p lute-check -p lute-lsp -p lute-cli`, keep `divergence.rs` green. No tree-sitter
`capabilityVersion` impact.

## 11. Open questions

- **`::camera` `wait` default** — no per-verb table yet (arch #1); v1 defaults non-blocking unless
  authored. Revisit when the camera-verb schema lands.
- **Cross-arm stage-join precision (§7.3)** — v1 conservative; a minimal data-flow join is a later
  refinement (correctness holds either way).
- **Reuse-input exposure** — the folded state schema + merged defs table live in `lute-check`
  internals; `lute-compile` needs read access. Prefer a small additive public accessor over
  duplicating the folding logic (one SoT).
- **Speaker-coupled ids (tradeoff).** `lineId` and `voiceKey` both derive from the per-speaker `code`
  (§12's Yarn `#line:` model — assigned once, persisted into `code`, stable across text edits), so
  changing a line's *speaker* changes both. Correct VN semantics (a recast line is a new unit). A
  future fully-opaque tag is a §12/`lute tag` derivation change, IR-schema-neutral.
- **`voiceKey` bank = characterId in v1.** The character-cast plugin (costume/era `voiceBank`) is not
  yet implemented, so `voiceKey = {characterId}-{code}`, aligned to `lineId` and unique within the
  episode/voice package. When cast lands, a voiced line whose bank ≠ character gets `{voiceBank}-{code}`
  (§7.3) — a plugin-hook change to the derivation, not the IR schema.

---

## DSL 0.1.0 addendum (2026-07-06)

Applies the language revision in [`2026-07-06-lute-dsl-0.1.0-design.md`](./2026-07-06-lute-dsl-0.1.0-design.md)
(driving [`scenario-dsl/0.1.0.md`](../../proposals/scenario-dsl/0.1.0.md)) to this IR. **The
0.0.1 body and worked examples above are unchanged** — they describe the shipped compiler. This
section is the delta a 0.1.0 compiler emits; where it conflicts with a 0.0.1 statement, this
section wins **for 0.1.0 input**. Every derivation named here has its single source of truth in the
DSL 0.1.0 design doc (section ids cited below); this addendum states only the IR-schema consequence.

### A1. `sprite.costume` + costume-aware stage join (amends §4.4, §7.3)

- Every `sprite` record — authored (`::auto`) **and** injected (§7.4) — gains an optional
  **`costume`** field: the character-cast plugin's resolved costume id for that character at that
  point. Absent when cast is inactive (the core case).
- The stage fork/join slot vector (§7.3) extends from `anchor`/`pose`/`emotion` to
  **`anchor`/`pose`/`emotion`/`costume`**. The join rule is unchanged: a slot identical across all
  arms carries forward; a slot that differs between arms, or is present in only some, becomes
  `Unknown` and is treated conservatively after convergence.
- Because costume resolution reuses this same conservative CFG join, a `costume` that is path-joined
  yet factually identical MAY still be conservatively rejected (treated `Unknown` / re-emitted) —
  documented expected behavior (character-cast errata: voiceBank/costume determinability).
- Rendering contract: the current `costume` applies to dialogue sprites and is **carried in the IR**
  on the `sprite` record; the engine does not reconstruct it from `set` records (character-cast
  errata: costume rendering contract). Its interaction with `voiceKey` is unchanged from §4.2/§11 — a
  voiced line whose cast voiceBank differs from the character gets `voiceKey = {voiceBank}-{code}` (a
  plugin-hook derivation change, not an IR-schema change).

### A2. New `hub` record kind (amends §4.4, §4.1, §7)

DSL 0.1.0 adds `<hub>` (Ink-parity revisit conversations). It lowers to one new record kind,
structurally a `choice` plus revisit flags:

```jsonc
{ "kind":"hub", "addr":"006-0100", "id":"barConvo",
  "recordKey":"scene.choices.barConvo",       // alias of that path, as on `choice` (§4.4) — not a new authored id
  "options":[
    { "id":"askName", "label":"What's your name?", "lineId":"bianca.s01ep02.barConvo.askName",
      "once":true,  "exit":false, "when":"isSet(scene.flags.curious)", "target":"006-0200" },
    { "id":"leave",   "label":"That's enough.",     "lineId":"bianca.s01ep02.barConvo.leave",
      "once":false, "exit":true,                                        "target":"006-0500" } ],
  "converge":"006-0700" }
```

- **Fields.** `id` (the hub id); `recordKey` = `scene.choices.<hubId>` (an alias of that state path,
  exactly as on a `choice`, §4.4 — not a separate id to mint); `options[]` of `{ id, label, lineId,
  once, exit, when?:<cel>, target:<addr> }` where `once`/`exit` are always-present bools and `when`
  appears only when authored; `converge:<addr>`. Each option `label` carries a `lineId` exactly like a
  `<choice>` option label (§4.2).
- **Folded envelope state (§4.1) gains two implicit slot groups per hub:**
  - `scene.choices.<hubId>` — `enum`, domain = the hub's choice ids ∪ `unset`, default `unset` (same
    shape/provenance as a `<branch>`; **hub ids and branch ids share the one per-episode
    `scene.choices.*` uniqueness domain**);
  - `scene.visited.<hubId>.<choiceId>` — `bool`, default `false`, one per hub choice, in the NEW
    reserved implicit namespace **`scene.visited.*`** (kept separate from `scene.choices.*` to avoid a
    path parent/leaf collision).
- **Flat-VM contract (§7).** On reaching the `hub` and after each **non-`exit`** arm completes, the VM
  re-presents every *eligible* option; eligible = `when` (if any) true **∧** (if `once`)
  `scene.visited.<hubId>.<choiceId>` == `false`. On a pick it records **both**
  `scene.choices.<hubId>` = the option id **and** `scene.visited.<hubId>.<choiceId>` = `true`, then
  sets PC ← option `target`. A **non-`exit`** arm's completion returns control to the hub loop head
  (a runtime property of the `hub` kind — **no backward `jump` is emitted**); an **`exit`** arm ends
  in a forward `jump → converge`, exactly like a `choice` arm. If no option is eligible at a
  presentation point, the hub auto-exits to `converge`. The clean-check gate proves `E-HUB-NO-EXIT`
  (≥1 unguarded `exit` choice, or all choices `once`), so the loop always has a terminating path.
- **§3.2 invariant preserved (D2).** The hub lowers to *finitely many* records — one header, one arm
  subgraph per choice, one convergence label — exactly as a `<branch>` does. The re-presentation
  *loop* is a runtime property that consumes one player input per cycle, so it introduces no unbounded
  compile-time computation and emits no backward jump into the record stream; the artifact stays the
  flat, ordered, "reduces to data" sequence D2/§3.2 require. Backward diverts remain a permanent
  non-goal.
- **Definite assignment.** Hub arms have no dominance relation (same join rule as `<match>` arms) —
  arm writes are may-writes at hub exit.

### A3. `line.placeholders` + `{{…}}` interpolation (amends §4.4)

DSL 0.1.0 adds `{{…}}` interpolation inside content-line `Text` (and inside `<choice>`/`<hub>` label
strings). The IR keeps `text` **verbatim as authored** — the `{{…}}` markers stay in the string,
because that string is the localization source keyed by `lineId` — and each `line` record gains:

- **`placeholders: [ … ]`** — one entry per interpolation, **in left-to-right appearance order**,
  each tagged with its `kind`: `path` (a state-path read), `ref` (a `@def` / `@fn(args)` reference),
  or `reserved` (a reserved token; only `userName` in 0.1). Each entry also carries the referent it
  substitutes (`path` / `ref` / `token`). Absent when the line has no interpolation.

```jsonc
{ "kind":"line", "addr":"007-0100", "role":"dialogue", "speaker":"bianca",
  "text":"Nice to meet you, {{userName}} — affection {{scene.affect.bianca}}.",
  "lineId":"bianca.s01ep02.bianca_0080", "voiceKey":"bianca-0080",
  "placeholders":[ { "kind":"reserved", "token":"userName" },
                   { "kind":"path", "path":"scene.affect.bianca" } ] }
```

- Purpose (1) **engine substitution** — the runtime renders each placeholder against live state per
  DSL D-B (number → shortest decimal, bool → `true`/`false`, enum → member text verbatim); the
  compiler emits only the verbatim `text` + this typed, ordered placeholder list. (2)
  **translation-fill validation** — see A6.
- A `<choice>`/`<hub>` option `label` that itself contains `{{…}}` carries the same `placeholders`
  array on that option: its text is translatable and `lineId`-keyed, so the identical set-equality
  check applies.

### A4. `meta.episodeId` as the lineId derivation input (confirms §4.2)

- Envelope `meta.episodeId` (already present in §4.1) becomes the **derivation input** for the
  episode segment of every `lineId`. Default `s{season:02}ep{episode:02}` (e.g. `s01ep02`); when
  authored/pinned, the literal `episodeId` value is used verbatim.
- 0.1.0 lineId shapes (SoT: DSL 0.1.0 §S3):
  - line: `{character}.{episodeId}.{speaker}_{code}`
  - choice/hub option label: `{character}.{episodeId}.{branchOrHubId}.{choiceId}`
- The `{episodeId}` segment is exactly the `s{s}ep{e}` middle segment already shown in §4.2 — so
  **§4.2 was already correct**: the choice-label `lineId` is episode-prefixed. 0.1.0 resolves the old
  0.0.1 §12 ↔ compile-IR §4.2 conflict **in this doc's favor**. Pinning `episodeId` makes
  translation/VO keys survive episode renumbering.

### A5. `line.role` derivation (SoT: DSL 0.1.0 §12 / X1)

- The `role` value in a `line` record (§4.4: `dialogue`|`narration`|`monologue`|`voiceover`) is
  derived from the content-line `delivery` attribute per the **DSL 0.1.0 §12 delivery→role table,
  which is the single source of truth**. This doc defines no separate rule:
  - speaker `narrator` → `narration` (a `delivery` attr on `narrator` is a static error);
  - else `delivery="thought"` → `monologue`; `delivery="voiceover"` → `voiceover`;
    `delivery="spoken"` or absent → `dialogue`.
- A non-player `delivery="thought"` is **legal** (that character's inner voice) and lowers to
  `"role":"monologue"`.

### A6. `E-L10N-PLACEHOLDER` is a compile-stage diagnostic

- Per DSL D-B's i18n contract, at build time the compiler MUST check **placeholder-set equality**
  between a line's source `text` and each translation, keyed by `lineId`, using exactly the
  `placeholders` set from A3. A mismatch (missing / extra / renamed placeholder) is
  **`E-L10N-PLACEHOLDER`**, raised at the **compile stage** (not parse/check). It is the one new
  0.1.0 diagnostic that belongs to `lute-compile`; the rest of the 0.1.0 registry delta is
  parse/check/manifest-stage.
- This does not reopen D5's deferrals (§9): asset-path resolution and translation *merge* stay
  external. Only the placeholder-set contract check is compile-owned, because it consumes the
  compiler's extracted `placeholders`.

## Artifact self-containedness addendum (2026-07-06, post-audit)

A compile-output audit (showcase `episode01.lute`, 81 records) confirmed the record shapes above
and surfaced five artifact-level gaps. Rule adopted: **the engine executes the artifact alone** —
no CEL parser, no manifest lookup, no capability guesswork at runtime.

### A7. Expression AST (`expr`) — no runtime CEL parser

Every CEL-carrying field (`match` arm `test`, `choice`/`hub` option `when`, `set` `value`)
additionally carries **`expr`**: the compiled Lute-CEL expression as a JSON tree. The string form
is retained for logs/debugging; **`expr` is authoritative for execution**.

```jsonc
// when: "(user.level >= (1))"
"expr": { "op": ">=", "l": { "path": "user.level" }, "r": { "lit": 1.0 } }
```

- Node kinds (closed, mirrors DSL 0.1.0 §8.4): `lit` (double | bool | string), `path`,
  unary `!` / `-`, binary `&& || == != < <= > >= + - * / in`, `cond` (ternary), `list`,
  `isSet(path)`, `has(path)`.
- All numeric literals serialize as doubles (Lute-CEL numeric model).
- **Why:** stock CEL evaluators implement *standard* CEL semantics — strict int/double separation,
  no `isSet` — which the Lute-CEL profile deliberately deviates from; reusing one (e.g. the
  pre-1.0 Dart `cel` package) would import wrong semantics plus a supply-chain risk. A ~10-node
  tree-walker per engine language replaces the entire parser dependency.
- **Conformance:** `lute-compile` ships **golden eval fixtures** (`expr` + state snapshot →
  expected value); any runtime evaluator that passes the fixture set conforms — a pure-Dart
  tree-walker and an FFI-wrapped Rust evaluator are interchangeable behind this contract.

### A8. `wait` fully materialized (amends §4.3)

§4.3 already defines `wait` as *resolved* (manifest default ⊕ author override), but the audit
found records omitting it (e.g. `music` carried no `wait` while `background`/`camera` did). Every
record whose directive family defines `wait` MUST carry the resolved value explicitly. The
artifact is timing-self-contained; an engine never consults manifest defaults.

### A9. Envelope hardening (amends §4.1)

- **`irVersion`** — the IR schema version, independent of `lute` (the language-version pin).
  The 0.1.0 record changes (A1–A7) bump it; engines gate parsing on it.
- **`capabilityVersion`** — the plugin-system §13 snapshot stamp. Required: without it an engine
  cannot reject an artifact compiled against a drifted bridge/directive schema (e.g. `::serve`).
- **`episodeId` normalization** — `meta.episodeId` MUST equal the lineId episode segment
  byte-for-byte (default lowercase `s{season:02}ep{episode:02}`). The audited artifact emitted
  `meta.episodeId: "S01EP01"` beside `lineId: "bianca.s01ep01.…"` — a defect to fix in the 0.1.0
  cutover.

### A10. Attr coercion in records (amends §4.4)

A manifest-declared **number** attr serializes as a JSON number, **bool** as a JSON bool — per the
DSL 0.1.0 §4.5 coercion grammar. (Audit: `camera.zoom: 1.2` vs `camera.shake: "0.4"` in one
artifact; the string form is non-conforming.)

### A11. Translation delivery = runtime string table (adjusts DSL §12 "fill")

`app.lang` is runtime state (mid-game language switching), so translated text is NOT baked into
per-language artifact copies. The artifact keeps `lineId` + source `text` (+ `placeholders`, A3);
translations ship as an **id-keyed runtime string table** (`lineId → text`) per locale, loaded by
the engine. Build time **validates** (A6 placeholder-set equality, key coverage) rather than
fills. DSL 0.1.0 §12's "fill the compile target's per-language text slots" is amended accordingly.

### A12. `plugin` records carry their effect bindings (amends §4.4)

The audited `plugin` record (`::serve`) carries only `{tag, fields, wait}` — but the manifest's
`effects.writes` (which state slots the bridge result lands in, e.g. `scene.serve.<key>.rank ←
fromBridgeResult:rank`) is NOT in the artifact. A runtime executing the artifact alone cannot know
where to write the bridge result — violating the self-containedness rule above.

Every `plugin` record with declared `effects` gains an **`effects`** field, resolved at compile
time (attr templates like `fromAttr` already substituted):

```jsonc
"effects": [
  { "path": "scene.serve.debut.rank",     "from": { "bridgeResult": "rank" } },
  { "path": "scene.serve.debut.attempts", "from": { "op": "increment", "by": 1 } }
]
```

The runtime's bridge dispatch becomes generic: call host with `{tag, fields}`, apply `effects`
to the state store from the returned result. No per-plugin knowledge in any runtime; the manifest
stays a compile-time-only input (snapshot-first, plugin-system §3.4).
