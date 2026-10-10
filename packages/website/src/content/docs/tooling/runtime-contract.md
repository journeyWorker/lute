---
title: Runtime contract
description: What a game engine must implement to run a compiled Lute artifact — the envelope, exact-minor IR and semantic negotiation, engine capability matrix, refusal-before-playback rule, and dispatcher loop over the scene and quest command kinds.
---

Lute is a total, side-effect-free compiler. `lute compile <file>` checks a
`.lute` document and lowers it to a JSON artifact — and then stops. It runs
**no CEL, no Datalog fixpoint, keeps no fact store, fires no bridge**. Every
behavior lives on the far side of the artifact, in the **engine**. This page is
the condensed runtime contract; the full, source-grounded specification is in
[`docs/runtime/`](https://github.com/journeyWorker/lute/tree/main/docs/runtime).
The `lute.engine.yaml` matrix format is specified in
[`0.33.0.md §4`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.33.0.md#4-engine-capability-matrix).
The machine-checkable shape is
[`schemas/lute-ir-0.39.schema.json`](https://github.com/journeyWorker/lute/blob/main/schemas/lute-ir-0.39.schema.json)
(JSON Schema draft 2020-12).

## Runtime event contract and `lute-runtime`

For the resumable runtime API and its JSON wire format, see the
[runtime event contract](https://github.com/journeyWorker/lute/blob/main/docs/runtime/event-contract.md).
The `lute-runtime` crate exposes the pure integration surface: load a
`compile --all` bundle with `Runtime::load`, create a session with `begin`,
advance it with `step`, and use `snapshot`/`restore` plus the read-only
queries for host inspection. Hosts own rendering, pacing, bridge calls, and
grant settlement; the runtime returns those effects as data.

## TypeScript runtime

TypeScript hosts can use [`@lute-lang/runtime`](https://github.com/journeyWorker/lute/tree/main/docs/runtime/typescript-runtime.md), an Effect 4 API over the wasm runtime. Its Bun and browser layers load a `lute compile --all` bundle, expose scoped sessions and event streams, and provide typed rejections, snapshots, and replay. See the [full TypeScript runtime guide](https://github.com/journeyWorker/lute/blob/main/docs/runtime/typescript-runtime.md) for installation, a complete Bun session, browser serving, program queries, errors, generated contract schemas, and `Replay`.

:::caution[Permissions stop at the artifact boundary]
[Capability permissions](/tooling/capability-permissions/) reject forbidden
authored effects before compilation. They are not a runtime sandbox and do not
authorize the engine's bridge, persistence, network, filesystem, or reward
effects. Engine hosts still load only trusted artifacts and authorize every
real effect. See the
[runtime/security guide](https://github.com/journeyWorker/lute/blob/main/docs/runtime/capability-permissions.md).
:::

## What Lute does vs. what the engine does

| Lute (compile time) | Engine (runtime) |
| ------------------- | ---------------- |
| Statically check the document; refuse to emit on any error. | Trust the artifact — it compiled clean. |
| Fold the state schema into an init/type table. | Initialize state from that table; own the tier lifetimes. |
| Lower every CEL guard **whose text is inside the closed §8.4 profile** to a portable `expr` AST; leave the rest as CEL text in `cel`. | **Evaluate** guards against live state. |
| Emit facts, `assert`/`retract` deltas, and Datalog rules as **data**; prove the rules are stratified and safe. | **Compute the minimal model** (least fixpoint) over the fact store. |
| Emit quests, objectives, and `<on>` handlers as **declarations**. | **Derive** the quest lifecycle from `start`/`fail`/objective completion. |
| Resolve plugin bridge calls and their state-write bindings. | **Make the call** and apply the effects. |
| Schedule timeline clips and prove no write races. | Replay the schedule (or run tracks concurrently) and honor the barrier. |

`holds()` and `count()` are inside the §8.4 CEL profile that authors may write
and are deliberately **absent** from the `expr` AST, so a guard that queries
facts reaches the engine as its `cel` text alone (`option.when`, `arm.test`,
`set.value`) with no `expr` sibling — and an engine MUST therefore have a CEL
evaluator, not merely an AST walker. `lute run`'s module doc says the same:
it resolves every slot from the `cel` text "including the `holds`/`count`
fact-query functions the structured `expr` AST deliberately omits".
The same holds for `visited('<scene id>')` (dsl 0.21.0): legal in every condition slot, true once
that scene has been presented in this save — the visited set the engine already keeps for
`after:` — and carried as `cel` text alone.

## Identity migration (0.36.6)

Component scopes use the authored `instance` key:
`::use{component="hearthFire" instance="opening"}` produces
`hearthFire#opening`, never an ordinal. A project that publishes joins or saves
opts into `identity.requireStable: true`; this enables diagnostics for missing
component instances and line codes. `lute tag` back-fills both, preserving
existing authored keys and allocating `use-001`, `use-002`, ….

The optional `identityRenames` array in the execution IR and project index is
the engine's only rename migration input. Each item is `{from, to}` using
canonical `NodeKey` strings. `lute diff` reports `renamed` for a ledger match
and `unmappedIdentity` when no authored mapping exists; it never guesses from
text or position.

```lute check
---
kind: scene
id: runtime-identity
---
## Opening
@narrator{code="intro"}: Ready.
```

The through-line: Lute proves *shape and structure*; the engine supplies
*evaluation and effect*. Lute's static analyses are also honest about their
limits — reachability is conservative under the declared `after:` routes,
relational fact gates yield **Unknown** verdicts behind a human-review
boundary, and `lute trace` walks one deterministic mock-driven path, never a
proof of all paths.

## The envelope

Every artifact opens with a fixed envelope (the `ExecutionIr` struct in
`crates/lute-compile/src/ir.rs`):

| field | meaning |
| ----- | ------- |
| `kind` | `"scene"` \| `"quest"` — read first; selects `meta`'s shape. |
| `lute` | language-version pin (informational for the runtime). |
| `irVersion` | the IR schema version you **gate on first**. |
| `capabilitySnapshot` | the plugin capability snapshot hash; refuse a mismatch. |
| `requiredSemantics` | compiler-derived, sorted semantic ids used by the artifact; authors cannot edit this list. |
| `meta` | scene meta or quest meta. |
| `state` | the folded init/type table. |
| `entities` / `enums` / `relations` / `seedFacts` / `rules` | the declared vocabulary (omitted when empty). |
| `commands` | the flat, ordered command stream; every record carries `kind`, `family`, and `position`. |
| `prereqEdges` | advisory raw graph edges (omitted when empty). |
| `sections` | the document's `##` sections, each `{ section, heading, id? }` (omitted when none has a heading or id). |
| `seasons` / `gates` / `terminal` / `terminalPersists` | optional clock and occasion controls. |

`requiredSemantics` is the sorted union of the lowered features in one
artifact. The project index carries the same field as the sorted union of its
document artifacts, allowing `check-project`, `play`, and an engine to
negotiate before opening every document. The complete registry and trigger
mapping live in [the 0.33.0 proposal §2–§3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.33.0.md#2-semantic-id-registry);
this page intentionally does not duplicate that registry.

### Command records

Every command record has three required fields and three optional siblings:

| field | meaning |
| ----- | ------- |
| `kind` | the record kind; an unknown `kind` is a hard error. |
| `family` | the normative grouping of `kind` (below), read by dispatch before `kind`. |
| `position` | the record's build-local execution position (see [Positions](#positions)). |
| `timing` | optional `{ wait?, duration?, delay?, at?, timeline? }`; omitted when every member is absent. |
| `provenance` | optional `{ by, explanation }` on a record the compiler synthesized. |
| `source` | optional source location. |

| `family` | kinds |
| -------- | ----- |
| `content` | `line` |
| `staging` | `bg`, `music`, `sfx`, `vfx`, `actor`, `camera`, `cg`, `video`, `sequence` |
| `state` | `set`, `assert`, `retract` |
| `control` | `choice`, `match`, `hub`, `jump`, `end`, `barrier` |
| `declaration` | `quest`, `on`, `entry`, `accept`, `beat` |
| `plugin` | `plugin` |

`timing.wait` is a boolean; `duration`, `delay`, and `at` are finite,
non-negative seconds; `timeline` is the zero-based ordinal of the
`<timeline>` the record was emitted from — an ordinal, not seconds. A
`barrier` has no `timing`: its `timeline` ordinal and `at` seconds are direct
fields. Read timing only from `timing`.

Staging values are opaque members of the project's declared domains:
`actor.anchor`/`action`/`emotion`/`costume`, `camera.framing`/`move`/
`transition` (plus `focus`, a cast reference), `cg.layout`, `music.playback`,
and `sequence.name`. Lute synthesizes no numeric camera transform. `cg` and
`video` always carry the resolved `display` (`show` by default). A `sequence`
record names an external cinematic; with `timing.wait: true` the engine
finishes presenting it before advancing. An actor exit is the derived
`actor.exit: true` (from the `action` domain's `exits:`); it is omitted when
false.

A `line` carries `role` (`dialogue`, `narration`, `mono`, `os`, or `vo`),
`speaker`, `text`, `lineId`, and a `voiceKey` on every role — a join key, not a
promise that a recording exists. `text` is the plain derivation of the
authored text: inline modifiers removed, escapes decoded, `{{…}}` markers kept.
A line that uses an inline modifier also carries `segments`, the presentation
runs: `{ text, styles?, rate? }` text runs and `{ pause }` leaves (seconds).
A line without modifiers has no `segments`. See [Localized text](#localized-text)
for the locale forms.

```json
{
  "kind": "music",
  "family": "staging",
  "position": "001-0400",
  "playback": "play",
  "assetId": "theme",
  "timing": { "duration": 1.5 }
}
```

## Activation and evaluation

An engine evaluates each CEL slot against nested maps built from live state:

- every declared root and intermediate map on the path to a declared slot is
  present;
- a non-reserved slot is present only with an effective value (write, seed, or
  default), and values retain their declared CEL types;
- reserved engine paths are always present with reserved defaults: for every
  quest the IR declares, `quest.<id>.state` and
  `quest.<id>.failedBy` are `"unset"`; each objective's
  `quest.<id>.objectives.<oid>.done` and `.failed` are `false`; and every lore
  entry's `entry.<id>.read` and `.everRead` are `false`.

Use `has(path)` for presence. A `<match>` arm with `is` is satisfied for a
bare path with no effective value only when its value is exactly `"unset"`;
other `is` values are definitely false. Other arms evaluate normally, and an
evaluation error means the arm is not satisfied.

One artifact is produced per document. A project's engine **unions** the
`relations` / `rules` / `seedFacts` / `entities` / `enums` / `prereqEdges`
across every artifact it loads, exactly as it concatenates the command streams.

`enums` is the one field whose **content** moved at language `0.9.0` while the
schema stood still. It has always carried the domains an author declares in an
`enums:` block; since content-vocabulary members became the project's to
declare, those domains — `emotion`, `action`, `anchor`, `mood`, `volume`,
`musicPlayback`, `vfxType` — arrive through the same array. A project declaring
them inline or through `uses:`/`extends:` emits them; a project whose members
come from a plugin's `enums` export emits **no** `enums` at all, because a
plugin vocabulary is capability surface (folded into `capabilitySnapshot`), not
per-document data. Either way this is data an engine already unions, so nothing
new is required of it — the `enums` move added, renamed, and moved no field.
That move did not change the IR shape. (The unrelated `meta.plugin` key arrived
in the same release; see the history entry
[What IR 0.10.2 changed](#what-ir-0102-changed).) Members carrying
compiler semantics (`action`'s
`exits:`, `anchor`'s `default:`) are resolved away at compile time and never
serialized: an engine needs no member semantics at runtime.

## Version negotiation and engine integration

Every host negotiates in this order, before opening a playback session:

1. **Exact MAJOR.MINOR IR gate.** Before 1.0, every minor may break the
   execution-IR shape, so an engine pins the exact `irVersion` major.minor
   line: a `0.39` engine accepts only `0.39.*` artifacts and refuses `0.38.*`
   and `0.40.*`; patch handling is the engine's policy. From 1.0 onward, the
   released major's policy applies.
2. **Semantic capability gate.** Load the immutable `lute.engine.yaml` matrix
   and compare every artifact `requiredSemantics` id with `supportedIds`.
   `engine`, `irVersion`, and unique `supportedIds` are required; `version`
   and `description` are descriptive. The full registry is in the
  [0.33.0 proposal §2](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.33.0.md#2-semantic-id-registry).

The matrix example is:

```yaml
engine: chat-text-engine
irVersion: "0.39.0"
supportedIds:
  - lute.core/1
  - lute.quest.lifecycle/1
  - lute.quest.rewards/1
  - lute.knowledge.facts/1
  - lute.knowledge.rules/1
  - lute.knowledge.temporal/1
version: "1.4.0"
description: "Text-only chat client: no staging, timeline, or clock"
```

`lute run --engine <file>` and `lute play --engine <file>` perform both gates.
If an id is missing, they refuse with exit code **2** and
`E-ENGINE-SEMANTICS` **before playback**: no command, condition, asset, or
bridge executes. Malformed matrices use `E-ENGINE-MATRIX`; an artifact outside
the matrix's exact major.minor line (a `0.38.*` artifact against the `0.39.0`
matrix above, say) uses `E-ENGINE-IR-VERSION`. Unknown
artifact or registry ids use `E-SEMANTICS-UNKNOWN`. `lute run` refuses an
artifact that still carries a removed 0.36 field with `E-IR-REMOVED-FIELD`.

`lute check --engine <file>` and `lute check-project --engine <file>` perform
author-time negotiation without playback. Missing capabilities are reported as
`E-CHECK-ENGINE-SEMANTICS` with the lowered construct's source span, semantic
id, and engine name. The project form checks the project-index union and lists
each contributing document/span. Plugin compatibility remains the exact
`capabilitySnapshot` check and is not folded into this matrix.

The reference executor's built-in matrix is named `reference` and supports
every current semantic id.

### Match-arm shorthand semantics (dsl 0.32.0 §7)

Each match arm has `test` and `target`; an arm authored with `<when is="…">`
also has semantic IR field `is` containing the shorthand text. A `test=` arm
omits `is`. The slot's `authored` field is diagnostic-only and MUST NOT be used
to infer whether an arm came from shorthand.

When the match subject is a bare state path with no effective value, an arm
with `is` other than `"unset"` is definitely not satisfied; `is: "unset"` is
satisfied. Other arms are evaluated normally, and an evaluation error means
not satisfied.

### What IR 0.37.0 changed

**A clean surface cutover: renamed kinds and fields, one new kind, removed fields.** An
engine on the 0.36 line must refuse a 0.37 artifact, and nothing old is accepted as an
alias. To move an engine to 0.37 (dsl 0.37.0 §2.2, §5):

- **Envelope:** `capabilityVersion` is `capabilitySnapshot` (the project index and
  `lute context --json` use the same key); `shots` is `sections`, each
  `{ section, heading, id? }`, where `id` is the authored `## Heading {#id}` suffix.
- **Every record** carries the new required `family` (see
  [Command records](#command-records)); the common `addr` is `position`, with the
  same format; the flattened timing fields move into one `timing` object; and
  `provenance.injected` is removed.
- **Kinds:** `background` is `bg`, `sprite` is `actor`, `cut` is `cg`, and `sequence` is
  new. `music.action` is `music.playback`; `cg.action` and `video.action` are `display`;
  `cg.full` is the domain-valued `layout`; `vfx.vfxType` is `type`; `music.track` and
  `sfx.name` are removed. Camera's numeric `zoom`/`moveX`/`moveY`/`shake`/`reset`/`easing`
  are replaced by the domain-valued `framing`, `move`, and `transition`.
- **Lines:** roles `monologue`, `offscreen`, and `voiceover` are `mono`, `os`, and `vo`;
  every line carries `voiceKey`; a line with inline modifiers adds `segments`, and its
  translations add `localeSegments`.
- **Control:** choice and hub option `label` is `text` (its locale map is `texts`);
  `recordKey` is `selectionKey`.

The schema file is `lute-ir-0.37.schema.json`.

## IR change history

*History, retained for anyone migrating an engine from an older line.* Each
entry below describes the gate rules of its own release (some predate the
exact major.minor gate); a current engine follows
[Version negotiation](#version-negotiation-and-engine-integration) above.

### What IR 0.30.0 changed

**No field added, renamed or retyped; names may now carry `-` or open with a digit.** An engine
that loads 0.29 artifacts loads 0.30 ones, provided it reads paths and conditions as follows
(dsl 0.30.0 §3):

- **Path fields are canonical dotted strings.** A `set` command's `path`, a `state` entry's
  `path`, an `expr` node's `path` / `isSet` / `has` and a `{{…}}` placeholder's `path` join names
  with `.`: `quest.zero-coke-001.state`, `run.visits.lab-b2`,
  `run.visits.001`. A segment may contain `-` or start with a digit but never contains `.`, so
  `split('.')` recovers it — key state on that string or on its segments, never re-parse it as
  CEL.
- **Conditions ship as written.** A condition's `raw` is the author's CEL, and a name that is not
  an identifier is reached with a quoted index, as in JavaScript: `quest["zero-coke-001"].state`,
  `run.visits['lab-b2'] >= 2`. That is ordinary CEL over nested maps — `m["k"]` and `m.k` read the
  same entry — so an engine evaluating `raw` over its state maps needs nothing new. The portable
  `expr` of such a condition carries the canonical path (`{"path": "run.visits.lab-b2"}`).
- **Fact arguments are names.** `holds('at', ["lab-b2"])` in `raw` asks about the fact an
  `assert` command writes as `{"relation": "at", "args": ["lab-b2"]}`; the quotes are the
  condition's spelling, never part of the name. Seed facts and the structured terms of rule heads
  and bodies carry the bare name the same way.

The schema file renames per release line: `lute-ir-0.29.schema.json` is now
`lute-ir-0.30.schema.json`, and its name patterns (seasons, occasions, `share` keys, `series`,
document ids) accept a name with `-` or a leading digit.

### What IR 0.29.0 changed

**One optional field; nothing renamed.** There is no new command `kind` and no field is renamed,
retyped or removed, so under the MAJOR-only gate an engine that loads 0.28 artifacts loads 0.29
ones unchanged (dsl 0.29.0 §5):

- **`terminalPersists: true`** on the artifact and `project.index.json`, serialized after
  `terminal`, when every `terminal:` declaration is the long form with `persists: true`. The
  ending outlives runs on purpose (a roguelike's permanent ending): once `terminal` holds, raise
  no occasion in this run or any later one — a new run does not reopen the game (a condition that
  may persist reads state a new run keeps, so it still holds). Omitted when false.

The schema file renames per release line: `lute-ir-0.28.schema.json` is now
`lute-ir-0.29.schema.json`.

### What IR 0.28.0 changed

**Three quest-layer fields renamed; everything else additive.** There is no new command `kind`
and no field is retyped or removed, so under the MAJOR-only gate an engine that loads 0.27
artifacts loads 0.28 ones — but one that reads the renamed fields must read them under their new
names (dsl 0.28.0 §6):

- **`ObjectiveEntry.visibleWhen`** (was `when`): visibility only — show the objective while it
  holds; it never gates `done`.
- **`RewardEntry.outcome`** (was `on`): only ever `"failed"`, on a quest-level reward that grants
  on a fresh `failed` transition.
- **A quest's `prereqEdges` row carries `follows`** (was `after`): quest-graph metadata that never
  gates the quest (its `start` does). Scene and bundle-beat rows keep `after`.

New behaviour and optional fields, each omitted when unauthored:

- **`outsideRun: [occasion]`** on the artifact and `project.index.json`, name-sorted: raise these
  occasions even after `terminal` holds (a title screen, a gallery between runs); every other
  occasion stays closed once the game is over.
- **`HubCmd.return`**: the position of the hub's `<return>` segment. Run it each time a non-exit
  option's segment ends, before the hub is judged and presented again — never before the first
  menu and never after an `exit` option. Like an option target, it bounds the segment before it.
- **`clock.raiseAtStart: true`**: raise the clock's slot occasion (the string form, or the map's
  `slot`) and the map's `dayStart` yourself at the run's first position, where no advance stops.
  Omitted when `false`.
- **The reserved clock reads `clock.day` and `clock.slot`** (aliases of the clock's day and slot
  paths) and, on a finite clock, **`clock.ended`**: `false` until the advance that ends the clock,
  `true` from that advance's settle until a new run starts a run-tier clock over.
- **`labelForms: { <member>: { start?, indefinite? } }`** on `entities[]` and `state[]` entries,
  and the placeholder formats **`cardinalWord`** (`one` … `twenty`, digits above),
  **`capitalize`**, **`start`** and **`indefinite`**, also on `reserved` and `occasionTarget`
  placeholders. `capitalize` upper-cases the rendered text's first letter; `start` renders the
  member's `start` form, else capitalizes; `indefinite` renders its `indefinite` form, else `a` /
  `an` by the first letter, a space and the text. In a `plural` form, `#word` / `#Word` is the
  number as a (capitalized) cardinal word.
- **Relation `tier: "season:<name>"`**: the relation's facts go back to the seed facts each time
  that season opens.
 - **`authored`** beside `cel` on a seam condition (`gates[].raisedWhen`, `terminal`,
  `seasons[].live`) when `@def` expansion changed it: for messages only; evaluate
  `cel`.
- **`spentBy` latches**: the `once` of a `spentBy` beat is now its period (`run` unless written),
  not `"none"` — never spend such a beat on presentation (see `spentBy` below).
- **A `forKind` beat spends `once` per member**: each member's presentation counts separately, and
  a `for` entry's first-read writes apply on each member's first read in the run.
- **`occasion.target` writes**: see *Writes through `occasion.target`* below.
- **When the seam is decided**: judge `terminal` and an occasion's `raisedWhen` when the occasion is
  raised; a `judge: before` judgement that makes `terminal` hold does not close that occasion's
  own beats. A `select: sequence` raise judges each beat again just before its turn, once an
  earlier beat of the raise has played.

### What IR 0.27.0 changed

**Additive fields and new values of existing fields.** No field is renamed, retyped, or removed,
and there is no new command `kind`, so under the MAJOR-only gate an engine that loads 0.26
artifacts loads 0.27 ones. Cadence (dsl 0.27.0 §5):

- **`once: "week"`** on `BeatIr` (a scene's `meta.beat`), `BeatCmd`, `EntryCmd` and
  `project.index.json` beat rows: spent from its presentation until the next clock week starts.
  Week *n* holds the days whose `(day - 1) div week.length` is *n*, so a week starts when
  `clock.weekday` returns to `week.first`. Only emitted when the clock declares a `week:`.
- **`once: "season:<name>"`** on the same records: spent from its presentation until the season
  `<name>` opens again.
- **`spentBy: {cel, expr, authored?}`** on `BeatIr`, `BeatCmd` and `EntryCmd` (its `raw` alone on the index
  beat row): the beat is spent by this condition instead of by being presented. Observe it at
  every quest settle (per member of a kind or `for` beat, `occasion.target` bound) and when the
  beat is judged: once it has held, the beat is spent for its `once` period — `run` when `once`
  is `run` or absent (a new run clears it), `user` never, `day` / `slot` / `week` while the
  clock stays in the period it was first seen holding in, `season:<name>` until that season
  opens again — even if the condition turns false. A presentation of a `spentBy` beat spends
  nothing. Omitted when unauthored.
- **`QuestCmd.rearm: {cel, expr, authored?}`**: observe it at every quest settle of a playthrough, the first
  observation being the baseline. Each time it goes false→true, return the quest to `unset`
  (objectives not done, `failedBy` cleared, deadlines forgotten); a `start` that holds activates it
  in the same settle. Omitted when unauthored.
- **`QuestCmd.tier: "season:<name>"`**: return the quest to `unset` when the season opens again,
  as a `"run"` quest at a new run.
- **`seasons: [{ name, live: {cel, expr, authored?} }]`** on the artifact (after `clock`, `gates`,
  `terminal`) and on `project.index.json`, with `live` after `@def` expansion. When a season's
  `live` goes false→true, copy every `season.<name>.*` value into `prev.season.<name>.*`, reset
  `season.<name>.*` to its declared defaults, clear the presentation record of its
  `once: "season:<name>"` beats, and return its season-tier quests to `unset`. Seasons are
  independent and may overlap. `prev.season.*` is a read-only mirror, like `prev.run.*`.

Text (dsl 0.27.0 §7):

- **`entities[].labels: { <member>: "<text>" }`**: display text for a kind's members, with a
  sub-kind's and its ancestors' labels for shared members. Render an `occasionTarget`
  placeholder of `entityKind` K as `labels[member]` of K, else the cast `name:` when the member
  is a cast id, else the id. A `{ domain: K }` or `{ entity: K }` state path carries the same map as
  its `state[].labels`, which `path` placeholders already render. Omitted when none.
- **Placeholder `format: "plural"` with `forms: [one, other]`**: render `one` when the number is
  1, `other` otherwise, each `#` in the form replaced by the number (`{{n:plural(# lamp|# lamps)}}`
  → `3 lamps`). A localizing engine may choose its own plural categories from the two English
  forms.

The finite clock (dsl 0.27.0 §4):

- **`clock.last: { day, slot? }`** or **`clock.days: N`**, verbatim as declared (`days: N` is
  `last: { day: N }`; `slot` omitted means that day's last slot). The clock stops at that
  position: an advance whose destination lies past it moves only to the last position (raising
  `dayEnd` / `dayStart` at every midnight it crosses on the way), raises the last day's `dayEnd`
  once and no `slot` occasion, and the clock is ended — advance no further until a new run resets
  the day and slot paths. Both omitted on a clock that never ends.

The engine seam (dsl 0.27.0 §4):

- **`gates: [{ occasion, raisedWhen: { cel, expr, authored? } }]`** (top level, occasion-sorted, `@def`s
  expanded): raise `occasion` only while its gate holds, reading `occasion.target` as the member
  it is raised for (`room.office` → `office`). A raise your clock makes (`raise.slot`, `dayStart`,
  `dayEnd`) whose gate is false is simply not made; the clock still moves. The checker has judged
  every beat of the occasion under its gate, so a beat you would never present is reported to the
  author. Omitted when no occasion declares a gate.
- **`terminal: { cel, expr, authored? }`** (top level): the game is over once it holds — raise no occasion
  and advance no clock until a new run. Several schemas' declarations arrive joined by `||`.
  Omitted without one.
- **Directive fact effects**: a `kind: "plugin"` record may carry **`retracts`** and
  **`asserts`**, each `[{ relation, args }]` with the call's attributes already substituted
  (`_` in a retract matches anything). Apply them after the record's own writes, retracts first,
  through your ordinary assert/retract path (a `reserved` relation included: the write is the
  engine's own). Omitted when empty. A lore entry's effect-only directive applies them on the
  entry's first read only, like the entry's `set` records.

Members bound by occasions (dsl 0.27.0 §3):

- **`occasion.target` as a pattern argument and a family index**: in a kind beat's (and a
  `forKind` beat's) CEL, a fact-query pattern may take `occasion.target` as an argument
  (`holds('owned', [occasion.target])`) and a `per:` family may be indexed by it
  (`user.bond[occasion.target]`). Substitute the bound member: the pattern argument becomes that
  member id, and `F[occasion.target]` reads `F.<member>`. The checker has proved every member's
  instance well-typed.
- **Writes through `occasion.target`** (dsl 0.28.0 §3): in the same beats, a `set` record's `path`
  may end in `[occasion.target]` (`run.count[occasion.target]`: write `run.count.<member>`), an
  `assert` / `retract` argument may be `occasion.target`, and so may a `plugin` record's field
  value, effect path index (`effects[].path`) or fact argument (`asserts` / `retracts`). Substitute
  the bound member before applying the write, exactly as for a read. A component argument never
  reaches you this way: it is compiled to one `match` arm on `occasion.target` per member.
- **`forKind: { kind, members }`** on `BeatIr` (a scene's `meta.beat`), `BeatCmd`, `EntryCmd` and
  `project.index.json` beat rows: the beat answers an untargeted `select: sequence` occasion once
  per listed member, in list order. Judge each member's eligibility at the raise with
  `occasion.target` bound to it, and present the beat once for each eligible member, binding
  `occasion.target` while it runs. `once` spends the beat, not a member. Omitted when unauthored.
- **Occasion payloads**: an occasion declared with `payload: { <field>: <type> }` hands typed
  values to each raise. Bind `occasion.payload.<field>` to the raise's values before its beats are
  judged, and unbind them once its beats have been presented: a payload lasts one raise. The
  artifact's CEL and `path` placeholders read them like state (`occasion.payload.copies`).

### What IR 0.22.0 changed

**Two additive fields, a new reserved flag, a wider occasion declaration, and new identity
values.** No field is renamed, retyped, or removed, and there is no new command `kind`, so under
the MAJOR-only gate an engine that loads 0.21 artifacts loads 0.22 ones. An engine that ignores
the new fields plays them with 0.21 semantics: every quest persists across runs and every entry
beat repeats. To play them as written:

- **`QuestCmd.tier`** is `"run"` on a `<quest tier="run">` and omitted for the default `user`
  tier. It rides on each `quest` record, not on `meta`, because one quest document may declare
  several quests. When a run starts, return every run-tier quest to `unset` with its objectives
  not done, then settle the lifecycle as at any evaluation instant. User-tier quests keep their
  status. See
  [quest-lifecycle.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/quest-lifecycle.md).
- **`EntryCmd.once`** is `"run"` or `"user"` on an entry beat that authored it, and omitted
  otherwise (repeatable, as in 0.21). `ProjectIndex.beats` entry rows carry the same value; scene
  rows always carried `once`. A `"run"` entry beat is not eligible while `entry.<id>.read` is true,
  and a `"user"` one once `entry.<id>.everRead` is.
- **`entry.<id>.everRead`** is a new reserved, user-tier `bool` per entry, default `false`. Set it
  whenever you set `entry.<id>.read` after a first read, and never reset it when a run starts. It
  is not listed in the artifact's `state` table (`entry.<id>.read` is, with `entry:<id>`
  provenance), but content reads it like any path, in raw CEL and in the `expr` AST
  (`{ "path": "entry.<id>.everRead" }`), so keep one for every entry you load.
- **Occasion target domains.** In the capability snapshot an occasion's `target` is `false`,
  `true`, or `{ "prefix", "entity" }`. For a domain, the checker guarantees every beat target of
  that occasion is `<prefix>.<member>` of the project's entity kind `entity`, so raise the
  occasion with targets of that shape. Occasion declarations stay out of the artifact. A domain
  changes `capabilitySnapshot`; `true` and `false` keep their 0.21 stamp.
- **`owner: engine`.** A `state:` declaration may name the engine as the path's writer. The
  checker refuses a content `::set` of such a path, or of a field under it
  (`E-ENGINE-OWNED-WRITE`), so no `set` record in a clean artifact targets it: the engine writes
  it. The artifact's `state` entry for the path is unchanged (path, type, default);
  `lute context --json` reports the owner as `"owner": "engine"`.
- **Identity values move.** The default `voiceKey` template is now `{prefix}.{speaker}-{code}`
  (it was `{speaker}-{code}`). A line expanded from a component is minted under
  `{prefix}.{component}#{n}`, where `n` counts the host's `::use`s of that component from 1, so
  under the default templates its `lineId` is `{prefix}.{component}#{n}.{speaker}_{code}` and its
  `voiceKey` `{prefix}.{component}#{n}.{speaker}-{code}`; a nested `::use` adds another segment.
  The field shapes are unchanged but the values differ, so rebuild voice and localization tables
  keyed on 0.21 ids. A project with audio recorded against the old keys pins
  `identity: { voiceKey: "{speaker}-{code}" }` (see
  [Frontmatter & profiles](/language/frontmatter-and-profiles/)).

### What IR 0.10.2 changed

**One additive field — no field renamed, retyped, or removed.** `meta` (both
`sceneMeta` and `questMeta`) gains `plugin`: a plugin-owned, checker-validated
top-level frontmatter key, folded in as `{ "<key>": <json value> }`, skipped
entirely when the document authors no plugin-owned key:

```json
{ "meta": { "character": "marina", "season": 1, "episode": 2,
    "episodeId": "s01ep02", "plugin": { "cast": { "ana": { "costume": "school" } } } } }
```

A key is present only when it is both declared by an active plugin and its
authored value independently passes that plugin's declared schema — the same
predicate the checker already gates `E-FRONTMATTER-SCHEMA` on, so a value the
checker would reject can never reach `meta.plugin`. See [plugin-system
0.0.4](https://github.com/journeyWorker/lute/blob/main/docs/proposals/plugin-system/0.0.4.md)
for the normative membership rule.

**No gate widening, by the ignore-unknown-fields rule.** `0.10.2` shares major
minor `0.10` with `0.10.0`/`0.10.1`, and `plugin` is a genuinely new object
key — an engine that already accepts a `0.10.x` artifact and ignores keys it
does not read keeps working unchanged. If you validate strictly against the
published JSON Schema rather than merely parsing, `sceneMeta`/`questMeta` in
`lute-ir-0.10.schema.json` now admit `plugin` as a typed property (still
`additionalProperties: false` otherwise); the file keeps its name because the
gated major.minor did not move.

### What IR 0.10.1 changed

*History, retained for anyone still on the `0.10.1` line.* **Nothing in the
shape.** IR `0.10.1` was shape-identical to IR `0.10.0` — no field added,
renamed, moved, or retyped, no new command `kind`. The number moved only
because a release re-aligns every visible axis; widening the gate to accept
`0.10.1` cost nothing beyond what `0.10.0` already required, since both share
major.minor `0.10`.

### What IR 0.10.0 changed

*History, retained for anyone still on the `0.10.0` line.* **One field
rename — the first shape change since `0.8.0`.** The injection
provenance stamp's `reason` becomes **`explanation`**:

```json
{ "by": "auto-pose-reset",
  "explanation": "pre-loading `vesna`'s first emotion `level` seen ahead of the entrance" }
```

The old name was a collision, not a synonym. `end.reason` is an **opaque author
token you dispatch on** — the author writes it, you branch on it. This field is
**human-readable English the compiler wrote** to explain why a record it
synthesized exists, and nothing dispatches on it. Two keys with the same name
and nothing else in common is exactly the trap a renamed field removes, and
`explanation` reads correctly beside `by`.

Nothing else in the shape moves: no field added, no field retyped, no new
command `kind`, no changed constraint.

**The gate above is normative, and this bump costs you two edits rather than
one.** An engine that implements IR `0.9` **must refuse** an artifact stamped
`0.10.0`, because `0.10` is a newer major.minor. The update is:

> **Widen the gate to accept `0.10`,** and if you read the injection provenance
> stamp, **rename `reason` to `explanation`.** Nothing after that — no new
> `kind` to dispatch, no behavioural difference.

If you validate against the JSON Schema, repoint at
`lute-ir-0.10.schema.json`; the `0.9` file is retained beside it.

Artifact **content** also moves, in a way that costs nothing: clip `at`,
`duration`, `delay` and the barrier `at` are still JSON numbers in seconds, but
they are now computed from integer milliseconds, so a cursor-derived `1.2` stops
serializing as `1.2000000000000002`. And the capability snapshot changes —
`W-INJECT-CONFLICT` left the code set and eleven codes joined it.

### What IR 0.9.0 changed

*History, retained for anyone still on the `0.9` line.* **Nothing in the shape.**
IR `0.9.0` was shape-identical to IR `0.8.0` — no field added, renamed, moved,
or retyped, no new command `kind`. The number moved only because Lute's
[versioning policy](https://github.com/journeyWorker/lute/blob/main/docs/versioning.md)
re-aligns every visible axis number on every release, and widening the gate to
accept `0.9` was the entire migration. What *did* move was artifact **content**:
`enums` began carrying the project's content-vocabulary domains (see
[the envelope](#the-envelope)) and the capability snapshot changed because the
core's vocabulary emptied — both new values in fields that already existed.

### What IR 0.8.0 changed

*History, retained for anyone still on the `0.8` line.* The schema file was
renamed
`schemas/lute-ir-0.7.schema.json` → `schemas/lute-ir-0.8.schema.json` along
with the minor bump. Three deltas matter to a consumer:

- **`end` is a new command `kind`.** By the unknown-kind rule above, an engine
  implementing only IR 0.7 **must refuse** an artifact carrying one — it cannot
  fall through the record, because `end` terminates the walk and falling
  through would play content the author marked unreachable. This is why
  termination is a core kind rather than a plugin directive: a plugin directive
  lowers to `kind: "plugin"`, which an older engine would happily skip.
- **The section-heading table and the locale maps are append-only optional
  fields**, so by the ignore-unknown-fields rule a 0.7 engine still loads a 0.8
  artifact that carries no `end` record.
- **The position width invariant is new**, and it is the one change that can
  alter bytes in an artifact you already consume. See below.

## Positions

Every command record carries a `position`, a string
`"{section}-{(index + 1) * 100}"` (e.g. `"001-0300"`): the one-based `##`
section number, then the record's order within that section. It is
**regenerated on every compile** — a build-local execution position, never an
identity, a localization key, or a save key. The stable content joins are
`lineId` / `voiceKey`; a section's stable identity is its optional `id` in
`sections`.

Both segments are zero-padded to a width computed from the document — at least
`3` for the section and `4` for the index, wider when the document needs it —
and that width is **uniform across the whole artifact**. The guarantee that
follows is the one you can rely on:

> Within one artifact, every emitted `position` has the same length;
> therefore **lexicographic order over `position` equals execution order.**

The fields that carry a position are exactly: every command `position`;
`choice.converge` and `choice.options[].target`; `hub.converge`, `hub.return`,
and `hub.options[].target`; `match.converge`, `match.otherwise`, and
`match.arms[].target`; `jump.target`; `quest.objectives[].body` (`null` for an
empty body); and `on.body`, `entry.body`, and `beat.body`.

## Streaming snapshot replacement

[`lute compile-stream`](/tooling/continuation-compiler/) emits a full ordinary
artifact snapshot for every accepted continuation unit. It does not define a
second IR or a patch language. `appendFrom` is the previous snapshot's command
count and only identifies the newly appended array region; it is not a runtime
PC and does not request execution of every record in that region.

When a snapshot arrives, replace the immutable program image and rebuild the
`position -> command index` map. Retain the **numeric** command cursor, current
state values, facts, selected control-flow stack, and host-owned effect/
idempotency records. Initialize only newly declared state slots. Never reapply
an existing default or seed fact merely because the artifact object was
replaced.

Rebuilding position lookup is required because appending commands can widen
uniform position padding across the complete artifact. The compiler compares
typed positions and control targets by numeric `(section, index)` meaning, so this
formatting-only change is accepted; it rejects any semantic mutation of an
already emitted command or state entry with `E-STREAM-PREFIX-CHANGED`.

The ordinary dispatcher still chooses branch, match, hub, and jump paths. When
it reaches the current stream frontier, wait for another snapshot rather than
marking the scene complete. Completion follows a successful compiler EOF
`finish`, or an authored ordinary `end` command. EOF and `end` are distinct:
`end` can stop runtime execution before stdin closes, while EOF never
synthesizes an `end`.

## The dispatcher

The `commands` array is already in execution order. Control-flow fields —
`jump.target`, choice/hub option `target` and `converge`, match arm
`target`/`otherwise`/`converge` — are all [positions](#positions). Walk with a
program counter over a `position → index` map, dispatching on `family` and
`kind`:

```ts
// Every CEL slot carries its verbatim source under its own key — `option.when`,
// `arm.test`, `set.value` — and the lowered `expr` AST ONLY when that CEL is
// inside the closed §8.4 profile. A relational fact query or a `visited()` read
// carries raw text alone.
const evalSlot = (raw, expr, state, facts) =>
  expr !== undefined ? evalExpr(expr, state) : evalCel(raw, state, facts);

const index = new Map(artifact.commands.map((c, i) => [c.position, i]));
let pc = 0;
while (pc < artifact.commands.length) {
  const cmd = artifact.commands[pc];
  let next: string | null = null; // null ⇒ fall through to pc + 1

  switch (cmd.kind) {
    // family "content"
    case "line":       present(cmd, state); break; // placeholders; segments when present
    // family "staging": opaque domain members; honor cmd.timing
    case "bg":    case "music":  case "sfx": case "vfx":   case "actor":
    case "camera": case "cg":    case "video": case "sequence":
      stage(cmd, cmd.timing); break;

    // family "state"
    case "set":     writeState(state, cmd.path, cmd.op, evalSlot(cmd.value, cmd.expr, state, facts)); break;
    case "assert":  facts.assert(cmd.relation, cmd.args); break;
    case "retract": facts.retract(cmd.relation, cmd.args); break;

    // family "control"
    case "choice":
    case "hub": {
      const opt = pickOption(cmd, state);   // per option: evalSlot(o.when, o.expr, …)
      next = opt ? opt.target : cmd.converge; break;
    }
    case "match": {
      const arm = cmd.arms.find(a => truthy(evalSlot(a.test, a.expr, state, facts)));
      next = arm ? arm.target : (cmd.otherwise ?? cmd.converge); break;
    }
    case "jump":    next = cmd.target; break;
    case "end":     finish(cmd.reason); return;  // terminates the walk
    case "barrier": joinTimeline(cmd.timeline, cmd.at); break;

    // family "declaration" and family "plugin"
    case "quest":   registerQuest(cmd); break;
    case "on":      registerHandler(cmd); break;
    case "accept":  acceptQuest(cmd.quest); break; // activates it iff still `unset`
    case "plugin":  callBridgeAndApplyEffects(cmd, state); break;

    default: throw new UnknownCommandKind(cmd.kind); // version gate: hard error
  }

  pc = next === null ? pc + 1 : index.get(next)!;
}
```

The full scene and quest command set is twenty-three kinds: `line`, `bg`,
`music`, `sfx`, `vfx`, `actor`, `camera`, `cg`, `video`, `sequence`, `set`,
`assert`, `retract`, `choice`, `match`, `hub`, `jump`, `end`, `barrier`,
`quest`, `on`, `accept`, `plugin`. A lore artifact adds the `entry` and `beat`
declaration heads, which the engine looks up rather than plays
([Lore entries](/language/lore-entries/)).

`accept` (dsl 0.21.0) is the scene-side `::accept{quest}`: the engine activates
that accept-driven quest if its state is still `unset` and ignores the record
otherwise ([Quests & scenes](/language/quests-and-scenes/#quests-meet-scenes-and-occasions)).

`end` carries an optional free-form `reason` — an author string
(`"completed"`, an ending id) Lute assigns no meaning to and the host MAY
surface. Terminating on it is identical to running off the end of `commands`,
except the reason is available.

## Localized text

A `line` record's `text` and a choice/hub option's `text` are always the
**source language** (`contentLang`), as plain text. When the artifact was built
with
[`lute compile --locales`](/tooling/cli/#--locales--merge-a-translation-bundle),
the record also carries a `texts` map, locale tag → translated plain text,
keyed on the record's `lineId`. A line with inline modifiers carries `segments`
for the source and, per translated locale, `localeSegments` beside the plain
`texts` entry — every translated locale always has its `texts` entry, so
backlog, TTS, and search never need to flatten segments:

```json
{
  "kind": "line",
  "family": "content",
  "position": "001-0200",
  "role": "narration",
  "speaker": "narrator",
  "text": "Wait for it here it comes.",
  "lineId": "opening.narrator_0020",
  "voiceKey": "opening.narrator-0020",
  "segments": [
    { "text": "Wait for it" },
    { "pause": 0.5 },
    { "text": " " },
    { "text": "here it comes", "rate": 1.25 },
    { "text": "." }
  ],
  "texts": { "ja-JP": "待って 来るよ。" },
  "localeSegments": {
    "ja-JP": [
      { "text": "待って" },
      { "pause": 0.5 },
      { "text": " " },
      { "text": "来るよ", "rate": 1.25 },
      { "text": "。" }
    ]
  }
}
```

The locale maps are omitted when empty, so an artifact compiled without
`--locales` is byte-identical to before, and a consumer that ignores them keeps
rendering the source language. Present a locale by looking it up in `texts`
(and `localeSegments` for a modified line) and falling back to `text` — the
compiler warns at build time (`W-L10N-MISSING`) about exactly those gaps, so a
complete bundle leaves nothing to fall back to.

## The runtime docs

Each surface has its own contract document under
[`docs/runtime/`](https://github.com/journeyWorker/lute/tree/main/docs/runtime):

- **[incremental-continuations.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/incremental-continuations.md)** — checked continuation compilation, full-snapshot replacement, cursor/state retention, stream finalization, and prefix stability.
- **[execution-model.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/execution-model.md)** — the artifact shape, version gate, positions, and the dispatcher loop.
- **[state-lifecycle.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/state-lifecycle.md)** — the `scene`/`run`/`user`/`app`/`quest.<id>` tiers, initialization, and reset boundaries.
- **[cel-and-facts.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/cel-and-facts.md)** — evaluating the `expr` AST, the fact store's assert/retract deltas, and the stratified least-fixpoint the engine computes.
- **[quest-lifecycle.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/quest-lifecycle.md)** — `start`/`fail` precedence, required vs. optional objectives, monotone completion, run-tier quests, and lifecycle events.
- **[beats-and-occasions.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/beats-and-occasions.md)** — occasions and their target domains, beat candidates, eligibility and `once` spending, and the selection order.
- **[lore-entries.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/lore-entries.md)** — entry eligibility, presentation, first-read effects, and the `read` / `everRead` flags.
- **[timeline-semantics.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/timeline-semantics.md)** — the local clock, per-track cursors, barriers, and the one-writer-per-target invariant the checker guarantees.
- **[bridge-protocol.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/bridge-protocol.md)** — typed bridge calls, return shapes, `wait`, and resolved state effects.
