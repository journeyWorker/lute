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
