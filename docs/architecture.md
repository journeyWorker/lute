# Lute — Architecture (compiler · AST · validation · LSP)

**Status:** draft / forward-looking. Not yet implemented. Captures a design session that
converged the *next* shape of the inline scenario authoring format, the AST it should parse
to, and the LSP that AST enables. The current parser lives at
`packages/lute-core/src/modules/scenario/inline/parser.ts`; the runtime target is the existing
flat command-record format (`idola_script_commands`).

> This document is the **implementation architecture**. The **language** is specified formally and
> versioned as a proposal: [`proposals/scenario-dsl/0.0.1.md`](proposals/scenario-dsl/0.0.1.md)
> (grammar + normative semantics). The **plugin / extensibility system** is specified normatively in
> [`proposals/plugin-system/0.0.1.md`](proposals/plugin-system/0.0.1.md) (human overview:
> [`plugin-system.md`](plugin-system.md)). Use the two proposals as the SoT and this doc for the
> AST/compiler/LSP architecture.

## Why

Moving the player runtime to Flutter unlocks two things the current format can't express:
**camera direction** (ordered, timed transforms) and **conditional branching on remembered
state**. This draft adds exactly those, plus the static-validation machinery an LLM/human
co-authored format needs — *without renaming or replacing existing directives, and without
embedding a Turing-complete scripting language.*

Two load-bearing constraints from the session:

1. **Reuse the SoT, add on top.** The existing directives (`::bg`, `::music`, `::sfx`,
   `::auto`, …) and the compiled `idola_script_commands` format are the source of truth. New
   capability is *layered on*, never an alias/rename. (Two false starts here came from
   designing in the abstract instead of reading the parser and the compiled output first.)
2. **Total, not Turing-complete.** Conditions are [CEL](https://cel.dev) (terminating,
   side-effect-free). Everything desugars to flat command records + CEL strings at compile
   time. **Litmus:** the first construct that *cannot* desugar to data is the signal to stop
   extending the bespoke DSL and embed a sandboxed interpreter (e.g. QuickJS) instead.

## System layers & boundaries (DSL · compiler · engine)

Three distinct systems, each with its own source of truth, tooling, and execution time. Do not
conflate them — most design mistakes come from reasoning about one in the terms of another.

| Layer | Who acts | Job | When | SoT / tooling |
|---|---|---|---|---|
| **DSL** (authoring surface) | human / AI *writes* | `:line`/`::auto`/`<branch>`/`<timeline>`/CEL — expressiveness, readability, static validatability | author time | `.lute` text · parser → AST · LSP · tree-sitter |
| **compiler** (`lute`) | the build *transforms* | AST → engine format: lowering, **auto-injection** (stage resolution), `@ref` expansion, asset binding, validation | **build time, once** | AST → `idola_script_commands` · `generator` · `harp` |
| **engine** (Flutter runtime) | the player *runs* | walk flat records, render, timing (`wait`/`delay`), evaluate CEL against runtime state (player choices) | **play time** | `idola_script_commands` + save-state |

**The load-bearing boundary:** the engine never sees the DSL — only the compiled flat records.
"Everything desugars to flat records + CEL strings" *is* the compiler↔engine contract. CEL
straddles the boundary: `@ref` macros expand at **compile** time; the inlined CEL string is
evaluated at **runtime** by the engine.

### Auto-injection is a deterministic compile-time GC, not runtime GC

The compiler's implicit insertion (auto-show a speaker not on stage, reposition existing
sprites, `posReset` a dirty pose, auto-hide on exit/scene-change) is **lifetime management of
stage entities** — GC-*like in spirit* (you don't write the cleanup; it's inferred from a
stage-state model), but mechanically a **deterministic, inspectable, build-time insertion pass**,
closer to RAII/lifetime-inference than a runtime collector. The stage is the heap; show = alloc,
hide = free, "hide whoever's no longer speaking" = collecting an unreachable entity. GC's failure
modes map to the checks this needs: *leak* (never auto-hidden) and *use-after-free* (auto-hidden
then spoken) are caught by determinism + provenance (`{injected, by, reason}`) + LSP-visible
resolved view + conflict warnings. See **Compiler — stateful resolution** below.

### Implementation language (open)

The compiler is build-time batch — **not** a latency bottleneck, so a Rust rewrite *for speed
alone* isn't justified and would add a second toolchain to a TS/Bun monorepo. Where Rust + a
CEL impl (`cel-rust`) genuinely pays is the **LSP + tree-sitter** layer (continuous, incremental,
already that ecosystem). The clean end-state, if the LSP becomes a real investment: **one Rust
core (parser + checker + CEL + lowering) shared by the CLI compiler and the LSP** (no TS/Rust
parser drift), with the **Dart engine keeping its own runtime CEL (`cel-dart`)**. CEL is what
makes this polyglot story tractable — it is spec'd with conformance tests, so `cel-rust`
(compile/LSP) and `cel-dart` (runtime) stay in sync (a payoff of choosing CEL over Lua / a
bespoke expression syntax). **Prerequisite either way: land the capability manifest first** so
the vocabulary is data both a Rust and a TS core can consume — making the implementation language
a later, swappable decision rather than a lock-in now.

## Layer model & bracket rule

Three authoring layers, distinguished by syntax so a reader can tell them apart at a glance:

| Layer | Syntax | Examples |
|---|---|---|
| **Content** | `:line[name]{attrs}: text` — speaker selects dialogue / narration (`narrator`) / monologue | dialogue, narration |
| **Staging (leaf)** | `::name{attrs}` | `::bg`, `::music`, `::sfx`, `::auto`, `::camera`, `::set` |
| **Logic / timeline (nesting)** | `<tag>…</tag>` | `<branch>`, `<choice>`, `<match>`, `<when>`, `<otherwise>`, `<timeline>`, `<track>` |

**Bracket rule — the single organizing axis is _nesting vs leaf_, not logic-vs-staging:**

- Has children → JSX-style `<tag>…</tag>` (self-naming close, folding, nesting — the "what
  does this close?" pain only ever lived in nested constructs).
- Single-line leaf → directive `::name{attrs}` (JSX buys nothing for a childless node; `::`
  stays consistent with the existing `::bg`/`:line` family).
- Content text after `: ` is **opaque to end-of-line** — parens, `(?)`, `<`, anything is literal,
  never parsed. Every content line is a prefixed `:line[…]:` (no bare prose), so classification is trivial.

> **Worked example:** [`examples/bianca-s01ep02.lute`](examples/bianca-s01ep02.lute) — the real
> content-catalog S01EP02 in this format, with `::camera`, the finger-beam `<timeline>` (four
> tracks on one clock), and a `<branch>`/`<match>`/state callback woven in (each marked NEW/demo).

## Existing directives — reuse verbatim (do NOT rename or reinvent)

Canonical attrs per `parser.ts`. New timing attrs (below) may be *added*; existing attrs and
names stay.

| Directive | Attrs |
|---|---|
| `::bg` | `location`, `time`, `assetId` |
| `::music` | `action` = `start\|change\|stop\|resume\|fade-out`, `mood`, `volume` = `silent\|down\|normal\|up\|full`, `assetId`, `track` |
| `::sfx` | `sound` (description), `assetId`, `name` |
| `::auto` | `character`, `anchor` = `left\|center\|right`, `action` (named action-id, e.g. `fade-in-up` / `fade-out-down` / `pose-*`) — **character entrance/exit/pose lives here** |
| `::vfx` | `type` (e.g. `blackOut`), `label`, `transition` |
| `::cut` | `assetId` (`CUT.*`), `action` = `show\|hide`, `full?` |
| `::video` | `assetId` (`VID.*`), `action` = `show\|hide`, `wait?` |
| `:line[name]` | `code`, `emotion`, `variant`, `action`, `dialogMotion` |

> Mistakes this table corrects (recorded so they aren't repeated): there is no `::scene`
> (it's `::bg`); music is not `play`/`to` (it's `action`/`mood`/`volume`-enum); sfx carries
> `sound`+`assetId` separately (not a single `asset`); character staging is `::auto`+action-id
> (not a `::sprite`/`::char` with `enter`/`pose`). Music fade-out is `action="fade-out"`,
> character exit is `::auto{action="fade-out-down"}` — both already exist.

## New additions (this is the entire delta)

### 1. `::camera` — net new (no camera in current format)

`::camera{focus, zoom, move-x, move-y, shake, reset, duration, easing, delay, wait}`.
A single `::camera` with multiple attrs = **one combined transform** applied together over
`duration` (covers "push in while drifting", the common case). A sequential move (zoom *then*
pan) = two consecutive `::camera` directives.

### 2. Timing attrs + concurrency — reuse `wait`, **no `<parallel>`, no `detached`**

The engine already has a per-directive **`wait`** flag: `wait="true"` means **the script holds
until the effect completes** (blocking); absent / `wait="false"` means **non-blocking** — the
next directive/line proceeds immediately, i.e. runs concurrently. So concurrency is just
consecutive non-`wait` directives; no `<parallel>` wrapper, and **no invented `detached`** (it
was the inverse of the existing `wait`).

Verified against the SoT: `::video` defaults to `wait=true` and opts out with `wait="false"`
(`parser.ts:649`, "holds until the clip ends … non-blocking/background video"); in compiled
output only `background` (99) and `video` (6) carry `wait` — every other type is non-blocking.
**There is no global "blocking default" to set** — each directive carries its own `wait`
default. (An earlier draft asserted a global blocking-default + `detached` opt-out; both wrong.)

New timing attrs add only:
- `duration="0.6"` — transform length.
- `delay="0.3"` — offset from the directive's own slot start.
- `wait="true"` — opt into blocking when a beat needs it ("pan to her, *then* she speaks").

New `::camera` verbs each declare a `wait` default in their schema (a slow push-in may default
non-blocking so dialogue rides over it; a focus-then-speak beat sets `wait="true"`).

```
::sfx{sound="문이 노크 없이 벌컥" assetId="PLACEHOLDER_door_slam"}
::camera{shake="0.3" duration="0.2"}                          /* no wait → next runs concurrently */
::auto{character="sofia" anchor="center" action="fade-in-up"}
:line[sofia]{code="0010" emotion="delighted" variant="1" action="sway"}: 매니저. 안녕…

::camera{focus="sofia" zoom="@closeUp" duration="0.5" wait="true"}  /* holds → the line waits for the pan */
:line[sofia]{code="0020" emotion="neutral" action="lean"}: 그러니까, 매니저. 딱 한 뼘. 두 뼘.
```

### 3. `<timeline>` — multi-track choreography block (After-Effects model)

> Named `<timeline>` + `<track>` (Unity-Timeline model), not `<cutscene>`/`<lane>`: `cut` and
> `scene`/`sceneId` are already taken (`::cut`, `## Scene N.`), so `cutscene` is out. `timeline`
> is collision-free. `<track>` is effectively collision-free too: a `track=` attr exists only on
> the **legacy `::bgm` alias** (verified — used in just one un-migrated character's scenarios,
> *eris*; canonical `::music` never uses `track=` across the whole catalog). `<track>` is a tag,
> `track=` a legacy attr — different positions, and the attr is on its way out anyway. The
> existing schema stays unchanged; we simply don't reuse the word for an attr going forward.

A **bounded, non-interactive choreography unit** with its own local clock — distinct from the
dropped `<parallel>` (whose only job, concurrency, the engine's `wait` already does). The value
is **temporal scoping + unit blocking**: multiple **tracks** (camera, a character, music, vfx)
each hold time-positioned clips, all tracks play concurrently as the playhead advances, and the
whole block blocks following content until it completes.

```
<timeline duration="2.4">
  <track subject="camera">
    ::camera{focus="door" duration="1.2"}      /* at 0.0 */
    ::camera{zoom="1.3" duration="0.4"}        /* omitted at → after prev clip → 1.2 */
  </track>
  <track subject="sofia">
    ::auto{character="sofia" action="walk-in" at="0.4"}
    ::auto{character="sofia" action="pose-turn" at="1.6"}
  </track>
  <track channel="music"> ::music{action="change" mood="tense" at="0.8"} </track>
  <track channel="vfx">   ::vfx{type="whiteOut" transition="flash" at="1.6"} </track>
</timeline>
```

Locked rules:

- **`<track>` is canonical** (a flat `at=`-only form may be added later as sugar that lowers to
  tracks). The track gives authors the AE mental model and the checker a real track boundary.
- **Time = absolute, with sequential-omission sugar.** `at="1.6"` is absolute timeline time;
  an omitted `at` starts after the previous clip in that track (first = `0.0`). `at` is **never**
  a relative nudge — that ambiguity is locked out.
- **One writer per track.** Each `subject`/`channel` key appears once; duplicate track keys are
  invalid. No two `subject="camera"` tracks (they'd silently fight) — explicit `property=` tracks
  (`subject="camera" property="zoom"`) are a later addition, gated on a write-set checker.
- **Staging-only, non-interactive.** Tracks hold `::` staging leaves (+ `::set` for state marks);
  **no `:line`/prose/`<choice>`/`<branch>`/`<match>` inside** — those would make it reader-paced,
  not clock-paced. No nested timelines initially.
- **Lowering** (no new runtime concept): resolve omitted `at` per track cursor → validate
  track-local overlap → validate cross-track write conflicts → emit the **same flat command
  records** sorted by `resolvedAt` → append a final barrier at `duration` (or `max resolvedEnd`).
  Still data, non-Turing, inspectable.
- **Two views.** The nested `<track>` source is the *authoring* view; the compiler/LSP renders a
  *resolved timeline table* as the debugging view:

  ```
  0.0  camera  focus door   dur 1.2
  0.4  sofia   walk-in
  0.8  music   change tense
  1.2  camera  zoom 1.3      dur 0.4
  1.6  sofia   pose-turn  ·  vfx whiteOut
  2.4  barrier
  ```

  LSP folds per `<track>`, renders this table, and warns past `>8` tracks / `>12` clips per track /
  `>40` clips total.

### 4. Logic layer — replaces `::choice`/`:::route` with nesting JSX

```
<branch id="couch">                          # unique-in-episode; auto-records to scene.choices.couch
  <choice id="help" label="같이 옮긴다">       # id = recorded key; label = shown text
    :line[fixer]{code="0020"}: ...알겠습니다.
    ::set{scene.affect.sofia += 2}           # scene.* spans shots within THIS episode
  </choice>
  <choice id="ignore" label="모른 척한다" when="@warm">   # when = availability gate (CEL)
    :line[fixer]{code="0030"}: 제 업무 범위를 다시 확인하고 오겠습니다.
    ::set{scene.affect.sofia -= 1}
  </choice>
</branch>

<match on="scene.affect.sofia">              # state-driven branch (no player input); intra-episode
                                             # (to carry to the NEXT episode, write run.* — see 0.0.1.md §9.1)
  <when test="$ >= 3"> ... </when>           # $ = the subject; pure CEL
  <when test="$ in [1, 2]"> ... </when>
  <when test="@chose('couch', 'ignore')"> ... </when>   # subject-independent guard
  <otherwise> ... </otherwise>
</match>
```

- `<match>` arms are **first-match-wins**; harp warns on provably-overlapping arms.
- **Exhaustiveness only for finite domains** (enum, bool, branch child-ids); otherwise
  `<otherwise>` is mandatory. `unset` is a domain member.
- `::set{path <op> celExpr}` — one assignment per `::set`; ops `=` `+=` `-=` (`*=` for
  numbers); operator/type matrix (`bool` → `=` only). The compound-assignment **operator is a
  token, not a string value**, so the old `value="+2"` string-vs-increment ambiguity can't
  arise. Lint binds each `::set` against the *current* state schema, not just syntax.

### 5. Definitions & conditions

One typed `defs` table (predicates + numeric staging values + parameterized macros are the
same thing — named, typed CEL). Referenced as `@name` / `@fn(args)`; **`@` is a compile-time
macro expansion to inline CEL.** harp validates each `@ref` against its use-context type
(bool-as-number / number-as-guard = compile error). Params are typed.

```yaml
defs:
  warm:    { type: bool,   cel: "scene.affect.sofia >= 2" }
  closeUp: { type: number, cel: "scene.affect.sofia >= 5 ? 1.35 : 1.15", min: 1.0, max: 1.6 }
  chose:   { type: bool, params: { q: choiceRef, opt: choiceId }, cel: "scene.choices[q] == opt" }   # intra-episode; choices are episode-scoped (§11.1)
```

Dynamic staging args are `@symbol` references only (`::camera{zoom="@closeUp"}`) — no inline
`{js}` expressions; all attribute values are strings (or a bare `@ref`), schema-coerced by
type. This keeps staging non-Turing-complete.

### 6. State

- **Explicit namespaces named by reset boundary (lifetime):** `scene.*` (episode end — one
  `.lute` doc; survives across its shots) · `run.*` (new run — cross-episode carry within one
  attempt) · `user.*` (profile wipe — survives runs) · `app.*` (uninstall — identity-independent,
  content-read-only). One axis;
  the engine owns each backend + fires each reset. The `run`/`user`/`app` schema is a single
  imported SoT (`uses`); declarations live in `---` frontmatter (not a `:::meta` fence — there
  is no `:::` in the grammar). Full normative model: `proposals/scenario-dsl/0.0.1.md` §9.
- **Definite-assignment analysis (path-sensitive):** every read resolves to a declared default /
  dominating write / guard / def param. Reading an undeclared path = compile error
  (`E-UNDECLARED`, never null/false). Non-`scene` paths are **maybe-unset at scene entry** unless
  schema-defaulted; a dominating `::set{p=…}` or guard (`isSet()`/`has()`) proves them after.
  Compound `::set` (`+=`/`-=`/`*=`) carries an implicit read. Diagnostics distinguish
  `E-UNDECLARED` from `E-MAYBE-UNSET`.

## Compile target

Everything desugars to the existing `idola_script_commands` flat records + CEL strings.
Camera/timing directives additionally carry **resolved absolute `start`/`duration`/`writes`**
so authors can see what `delay` became (and so same-subject/same-property overlapping writes
can be flagged as errors via each directive's schema `writes[]`).

## Compiler — stateful resolution (auto-injection)

Lowering is **not** a pure 1:1 map. The compiler maintains scene state while walking the node
stream and **injects implicit commands** the author didn't write — today this is tangled mutable
flags in one big loop in `generator.ts` (two sets, `anchoredCharacters` + `dirtyCharacters`,
threaded through inline `if`s that emit `posReset`, reposition rows, and a look-ahead-emotion
sprite load). The clean structure:

1. **Explicit typed `StageState`** (`{ onStage: Map<char,{anchor,pose,emotion}>, dirty: Set, bg,
   music }`) — one value passed through, not scattered loop-local sets.
2. **Lowering as a pure reducer** — `lower(state, node, lookahead) → { state', emit: Command[] }`.
   Deterministic; testable by feeding a node + state and asserting `emit` + `state'`.
3. **Two passes.** *Pass 1* — direct lowering: each directive → its explicit record(s), pure and
   manifest-driven (data). *Pass 2* — stage resolution: fold the stream through the `StageState`
   reducer + injection ruleset, emitting the implicit commands (code). This physically separates
   "what the author wrote" from "what the compiler added."
4. **Injection rules = named, ordered, pure ruleset** (not inline `if`s), each unit-testable:
   `auto-pose-reset` (dirty & !stateful & !exit → `posReset`), `auto-anchor-on-show` (show w/o
   anchor → compute anchors + reposition existing), `entry-emotion-lookahead` (show → next
   dialogue's emotion for the sprite), `stage-bookkeeping` (show/exit/anchor → update `onStage`).
5. **Provenance on every injected command** — `{ injected: true, by: "auto-pose-reset",
   reason: "…" }` (formalizing the `comment:` strings the current code already writes). Surfaced
   in the resolved view + LSP timeline → the injection is *visible*, not silent magic; conflicts
   (author-written vs would-be-injected) become warnings.
6. **Manifest-driven, code-executed** — which directives touch stage state is declared by the
   per-directive `reads`/`writes`/`semantics` flags (`::auto` → `writes.stagePose`,
   `mayExitCharacter`, `usesAnchor`; `:line` → `reads.onStage`). The resolver is *driven by* those
   flags but its algorithm stays code (a closed-registry named hook). This is the data-vs-code
   boundary made concrete: manifest says *which* participates, code says *how* it injects.

This is the "deterministic compile-time GC for stage entities" from the system-layers box: the
named rules are the collector, provenance is the visible free-list, and determinism + conflict
warnings catch the leak / use-after-free analogues.

## AST

**Two tiers.** The parser produces a deliberately *generic* **ParseAst** (the LSP reads this —
stable across new directives); the compiler then lowers it to a **CheckedIr** with per-tag
typed commands. Keeping `Directive` generic in the ParseAst means adding a new staging verb is
schema work, not grammar/AST churn.

```
# ── ParseAst (LSP-facing; generic, stable) ──
Document
├─ Meta            { state: StateDecl[], defs: Def[] }
└─ Shot[]          { heading, span, body: Node[] }
   Node =
   │  Line         { speaker, attrs{code,emotion,variant,action,delivery,as,…}, text, span }
   │                # speaker distinguishes dialogue / narration (narrator) / monologue (player + delivery=thought)
   │  Directive    { tag, attrs: Attr[], span }          # leaf: bg/music/sfx/auto/vfx/cut/video/camera
   │  Set          { path, op, expr: CelSlot, span }     # distinct node — state mutation
   │  Branch       { id, choices: Choice[], span }
   │  Choice       { id, label, when?: CelSlot, body: Node[], span }
   │  Match         { subject: CelSlot, arms: Arm[], span }
   │  Timeline     { duration?: CelSlot|number, tracks: Track[], span }   # multi-track timeline
   Track   { key: {subject?|channel?|property?}, clips: Clip[], span }
   Clip   { node: Directive|Set, at?: number, duration?: number,
            resolvedAt, resolvedEnd, writeSet }
   Arm =
   │  When         { test: CelSlot, body: Node[], span } # `$` binds to Match.subject
   │  Otherwise    { body: Node[], span }
Attr     { key, value: string | CelSlot, span }
CelSlot  { kind: condition|attr-value|set-expr|match-subject,
           raw: string, ast?: CelAst, span, id: StableNodeId }   # @name / @fn(args) live in ast

# ── CheckedIr (compiler-facing; per-tag typed) ──
CameraCommand | AutoCommand | SfxCommand | BgCommand | MusicCommand | SetCommand | …
```

**`CelSlot` is the single biggest LSP win.** Every CEL-bearing field is a ranged child node
(not an opaque string), so: `@ref`/`$`/`scene.*` resolve *inside* the original document ranges;
incremental reparse replaces only a damaged slot, not its parent; and invalid CEL stays
attached as `ast: undefined` + diagnostics instead of poisoning the surrounding DSL tree (error
recovery). The CEL parser owns expression syntax; the DSL parser owns structure; the slot is
the seam, preserving both `raw` source slice and `ast`.

Resolution inputs the AST is checked against: the **directive/attr schemas** (per-tag,
incl. enum domains, timing/`wait`/`writes`), the **state schema** (from `Meta.state`), the
**defs table** (from `Meta.defs`), and external **character + asset registries** (character
ids, `assetId`/`CUT.*`/`VID.*` catalogs, `sound` ↔ assetId).

## Validation core — one `check()`, three surfaces

The LSP is the real investment because validation has **two live consumers**: **AI agents** author
`.lute` and must verify the instant they write (no editor — they call validation headlessly in
their write→verify loop), and **human managers** edit `.lute` in an editor and need live squiggles +
fix-its + the resolved/injection views. Both must see the *same* result.

So the contract is a checker core, **not** the LSP protocol (LSP is transport/presentation, not the
validation contract):

```ts
check(input: CheckInput): CheckResult
// CheckInput  = { text, uri, workspaceSnapshot, manifestVersion, providerSnapshotIds, mode }
// CheckResult = {
//   ok,
//   diagnostics: [{ code, severity, message,
//                   span:{byteStart,byteEnd,line,column,utf16Range},
//                   layer: content|staging|logic|cel,
//                   fixits:[{title,kind,edit,confidence}], provenance? }],
//   resolved?: { commands, timeline, injections }   // resolved view + injected-command provenance
// }
```

The **core** owns parse, type/check, CEL-slot validation, lowering preview, injection provenance,
and fix-it generation. Three surfaces wrap the *same* `check()`:

| Surface | Consumer | Wraps `check()` how |
|---|---|---|
| **editor LSP** | human managers | converts `CheckResult` → LSP diagnostics/code-actions; owns only doc-sync, routing, presentation |
| **headless API** | AI agents | calls `check()` directly (CLI / JSON-RPC); returns the structured `CheckResult` (byte spans + stable codes + machine-applicable edits) for self-correction |
| **CI / batch** | gates | same `check()` over many files |

**No divergence between agent and manager.** The LSP builds an immutable `DocumentSnapshot` and
calls the same core; incremental parsing, when added, is an optimization *behind* the same
`check(snapshot)` contract — never a second code path. Enforce with a **golden test comparing
headless output vs LSP-published diagnostics byte-for-byte** (after normalization).

## LSP feature map

The AST + schemas + registries above are exactly what each surface renders from a `CheckResult`.
For **managers (non-programmers)** the highest-value features are plain-language squiggles,
quick-fixes, hover docs, and — more than clever completion — the **resolved timeline view** and the
**injection-provenance view** ("this command was auto-injected by rule X because Y"), with
diagnostics grouped by *narrative cause*, not compiler phase. Noise to avoid: deep AST views,
generic refactors, over-rich CEL autocomplete, type-theory wording.

| Capability | Source |
|---|---|
| **Diagnostics** | parse errors + harp lint: non-exhaustive `<match>`, overlapping arms, definite-assignment (`E-UNDECLARED`/`E-MAYBE-UNSET`), `::set` schema-binding + op/type matrix, unknown directive/attr, bad enum value, undeclared `@ref`/state-path/choice-id, type-mismatched `@ref` use, wait-omission suspicion (a timed `::camera`/`::auto` move immediately followed by dialogue with no `wait` — possible unintended race), unknown `assetId`/character |
| **Hover** | directive/attr docs from schema; `@ref` → its CEL definition + type; state path → declared type/default; emotion/action/anchor enum docs; `assetId` → catalog entry |
| **Completion** | directive names; attr keys per directive schema; attr enum values (music `action`/`volume`, `anchor`, `emotion`); character ids (registry); `assetId`/`CUT.*`/`VID.*` (catalog); `@ref` names (defs); state paths; choice ids inside `<match on=>` |
| **Go-to-definition** | `@ref` → defs entry; state path → state decl; `scene.choices.<id>` → `<branch id>`; jump/`next` target → shot |
| **Find-references** | `@ref` uses; state-path reads/writes; choice id |
| **Folding** | `<…>` blocks; shots; per `<track>`; + a rendered *resolved timeline table* view for `<timeline>` |
| **Semantic tokens** | 3 layers colored distinctly (content / staging / logic); CEL sub-tokens; `@ref`s; state paths |
| **Document symbols** | shots, branches, matches |

**Architecture.** Two parsers, one grammar:

1. **tree-sitter grammar** — editor-side incremental parse for syntax highlight + folding +
   bracket matching (fast, runs on every keystroke). This is also what gives the JSX `<>`
   blocks free matched-tag/fold behavior, and what the very first "is an LSP hard?" question
   was really asking for — most of the cost is here and tree-sitter amortizes it.
2. **the `check()` core** — reuses the canonical `lute-core` parser for the authoritative AST and
   the **existing harp validators** for diagnostics (the semantic checks above are mostly already
   implemented as `harp lint`/`harp validate` rules). The LSP server is a thin adapter exposing
   `check()` over `textDocument/publishDiagnostics`; the headless agent API and CI call the same
   `check()`. Completion/hover/definition draw on the directive schemas + defs + state schema +
   asset/character registries.

The expensive part (semantic validation) already exists as `harp`; every surface is an adapter
over one `check()` core, and tree-sitter handles the editor-surface intelligence. That split is
what makes this LSP cheap relative to a general-purpose one.

**First build the daemon, not Rust.** Three surfaces strengthen the case for a clean core *API*,
not immediately for Rust. A warm **daemon / headless JSON service around the TS core** removes the
cold-start that would otherwise dominate the agent's write→check→rewrite loop — so the daemon, not
a rewrite, is the first move. Reach for a shared Rust core (`cel-rust`) only when measurements
justify it (agents spawning a checker per chunk needing sub-50ms cold-start, or visible CI
throughput limits) — see **System layers → Implementation language**.

## Extensibility — see the plugin system spec

How capability is *extended* — the capability manifest, the data↔code boundary, plugin packaging,
profile-based activation, providers, typed directives/state/bridge, and lowering hooks — is
specified normatively in [`proposals/plugin-system/0.0.1.md`](proposals/plugin-system/0.0.1.md),
with the human-facing overview in [`plugin-system.md`](plugin-system.md). The compiler/AST/validation/LSP machinery
in this document consumes the resolved **capability snapshot** that spec produces; the named
lowering hooks a plugin may target live in *Compiler — stateful resolution* above.

## Roadmap / open items

1. **`::camera` `wait` defaults** — each camera verb declares its `wait` default in schema,
   consistent with the engine's existing per-directive `wait` (verified: `video`/`bg` default
   `wait=true`, the rest non-blocking). Resolved: the marker is `wait`, there is no global
   blocking default, and `detached` is dropped.
2. `::camera` verb set + per-method schema (`types`/`timing`/`wait`/`writes`).
3. Land the new front-end (JSX logic + `::camera` + timing attrs) on the existing
   `idola_script_commands` compiler.
4. harp lint rule set: exhaustiveness, definite-assignment, `::set` schema-binding,
   wait-omission suspicion, `@ref`/asset/character resolution.
5. tree-sitter grammar → editor highlight/fold; then the LSP adapter over `lute-core` + harp.
6. Persistence backends, one per lifetime tier with its own engine-fired reset trigger: `run.*`
   (new run), `user.*` (per-identity, survives runs), `app.*` (device-global). `scene.*` needs no
   backend (in-memory, dropped at scene end). Ship order `scene` → `run` → `user`/`app`. The
   `run`/`user`/`app` schema is a single imported SoT (`uses`, 0.0.1.md §9.2).
7. `<timeline>` track resolver: omitted-`at` track cursor → track-local overlap + cross-track
   write-conflict checks → flat records sorted by `resolvedAt` + final barrier; resolved
   timeline-table renderer (shared by compiler diagnostics and the LSP).
8. Two-tier AST (`ParseAst` generic → `CheckedIr` per-tag typed) + `CelSlot` ranged CEL nodes
   for incremental reparse and CEL-internal resolution.
9. Capability manifest MVP, in order: enums + directive attr schemas → manifest → generate
   parser/checker validation tables → generate LSP completion/diagnostics → provider snapshot
   interface → narrow named lowering hooks → manifest hash checks → (last) tree-sitter
   generation. One golden test per directive (DSL → CheckedIr → records → diagnostics).
10. Refactor `generator.ts` auto-injection into the explicit `StageState` reducer + named,
    ordered injection ruleset + provenance tags (per **Compiler — stateful resolution**); golden
    test per injection rule.
11. Implementation-language decision (per **System layers**): keep the compiler TS for now; reach
    for a shared Rust core (`cel-rust`) only when the LSP is the real investment, and only after
    the manifest makes the vocabulary language-portable. Engine CEL stays `cel-dart`; track
    CEL conformance across impls.
12. `check(input) → CheckResult` core + three adapters (editor LSP / headless agent API / CI),
    with the structured `CheckResult` (byte spans, stable codes, fixits, resolved+injection views)
    and a byte-for-byte golden test (headless vs LSP-published). **Build the warm daemon around
    the TS core first** (kills agent-loop cold-start); Rust later only if measured.
13. Implement v0.0.1 block comments: `/* ... */` is body trivia, may be standalone/inline/trailing,
    is ignored inside quoted strings, does not nest, and errors if unterminated. The current
    `parser.ts` has no comment handling.

## Provenance

Design converged over an iterative review with a `codex` DSL-critic peer (via `drum swarm`,
10 rounds). Later rounds added the two-tier AST (`ParseAst` → `CheckedIr`) + `CelSlot`, the
capability-manifest extensibility architecture (data-vs-code boundary, snapshot-first providers,
narrow named lowering hooks), the `check()` core + three-surface validation (editor LSP /
headless agent / CI, daemon-first), and the
multi-track `<timeline>` block (After-Effects tracks, absolute time + sequential-omission
sugar, one-writer-per-track, lowering to flat records + a barrier). Key reversals it drove:
deleted subject-elided `is` in favor of a `$` placeholder
(pure CEL); kept fluent ergonomics but moved leaf staging back to directives (nesting-vs-leaf
rule); `::set` compound-assignment operators over string values. Human review then dropped
`<parallel>` and `detached` — the engine's existing per-directive **`wait`** flag (`wait=true`
holds, absent = non-blocking) already expresses both blocking and concurrency — and corrected
the invented `::sfx`/`::music`/`::scene`/`::char` attrs back to the real
`::bg`/`::music`/`::sfx`/`::auto` vocabulary. (The recurring failure mode: designing in the
abstract instead of reading the parser, the compiled output, and the engine first.)
