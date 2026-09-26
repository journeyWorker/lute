---
title: Runtime contract
description: What a game engine must implement to run a compiled Lute artifact — the envelope, version negotiation and the IR 0.10.0 provenance-field rename, the addr width invariant, and the dispatcher loop over the twenty-two scene and quest command kinds.
---

Lute is a total, side-effect-free compiler. `lute compile <file>` checks a
`.lute` document and lowers it to a JSON artifact — and then stops. It runs
**no CEL, no Datalog fixpoint, keeps no fact store, fires no bridge**. Every
behavior lives on the far side of the artifact, in the **engine**. This page is
the condensed runtime contract; the full, source-grounded specification is in
[`docs/runtime/`](https://github.com/journeyWorker/lute/tree/main/docs/runtime)
and the machine-checkable shape is
[`schemas/lute-ir-0.26.schema.json`](https://github.com/journeyWorker/lute/blob/main/schemas/lute-ir-0.26.schema.json)
(JSON Schema draft 2020-12).

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
| Lower every CEL guard **whose text is inside the closed §8.4 profile** to a portable `expr` AST; leave the rest as raw CEL. | **Evaluate** guards against live state. |
| Emit facts, `assert`/`retract` deltas, and Datalog rules as **data**; prove the rules are stratified and safe. | **Compute the minimal model** (least fixpoint) over the fact store. |
| Emit quests, objectives, and `<on>` handlers as **declarations**. | **Derive** the quest lifecycle from `start`/`fail`/objective completion. |
| Resolve plugin bridge calls and their state-write bindings. | **Make the call** and apply the effects. |
| Schedule timeline clips and prove no write races. | Replay the schedule (or run tracks concurrently) and honor the barrier. |

`holds()` and `count()` are inside the §8.4 CEL profile that authors may write
and are deliberately **absent** from the `expr` AST, so a guard that queries
facts reaches the engine as its raw CEL text alone (`option.when`, `arm.test`,
`set.value`) with no `expr` sibling — and an engine MUST therefore have a CEL
evaluator, not merely an AST walker. `lute run`'s module doc says the same:
it resolves every slot from the raw CEL "including the `holds`/`count`
fact-query functions the structured `expr` AST deliberately omits".
The same holds for `visited('<scene id>')` (dsl 0.21.0): legal in every condition slot, true once
that scene has been presented in this save — the visited set the engine already keeps for
`after:` — and carried as raw CEL alone.

The through-line: Lute proves *shape and structure*; the engine supplies
*evaluation and effect*. Lute's static analyses are also honest about their
limits — reachability is conservative under the declared `after:` routes,
relational fact gates yield **Unknown** verdicts behind a human-review
boundary, and `lute trace` walks one deterministic mock-driven path, never a
proof of all paths.

## The envelope

Every artifact opens with a fixed envelope (the `Artifact` struct in
`crates/lute-compile/src/ir.rs`):

| field | meaning |
| ----- | ------- |
| `kind` | `"scene"` \| `"quest"` — read first; selects `meta`'s shape. |
| `lute` | language-version pin (informational for the runtime). |
| `irVersion` | the IR schema version you **gate on**. |
| `capabilityVersion` | a plugin-snapshot hash; refuse a mismatch. |
| `meta` | scene meta (`character`/`season`/`episode`/`episodeId`) or quest meta. |
| `state` | the folded init/type table. |
| `entities` / `enums` / `relations` / `seedFacts` / `rules` | the declared vocabulary (omitted when empty). |
| `commands` | the flat, ordered, addressed command stream. |
| `prereqEdges` | advisory raw `after` prerequisite edges (omitted when empty). |
| `shots` | authored `## ` shot headings, `{shot, heading}` (omitted when empty). |

One artifact is produced per document. A project's engine **unions** the
`relations` / `rules` / `seedFacts` / `entities` / `enums` / `prereqEdges`
across every artifact it loads, exactly as it concatenates the command streams.

`enums` is the one field whose **content** moved at language `0.9.0` while the
schema stood still. It has always carried the domains an author declares in an
`enums:` block; since content-vocabulary members became the project's to
declare, those domains — `emotion`, `action`, `anchor`, `mood`, `volume`,
`musicAction`, `vfxType` — arrive through the same array. A project declaring
them inline or through `uses:`/`extends:` emits them; a project whose members
come from a plugin's `enums` export emits **no** `enums` at all, because a
plugin vocabulary is capability surface (folded into `capabilityVersion`), not
per-document data. Either way this is data an engine already unions, so nothing
new is required of it — the `enums` move added, renamed, and moved no field.
`irVersion` reads `0.10.2`. The shape *does* change at `0.10.2`, in one place
unrelated to `enums`: a plugin-owned, checker-validated frontmatter key now
reaches the artifact (`meta.plugin`). See
[What IR 0.10.2 changed](#what-ir-0102-changed). Members carrying
compiler semantics (`action`'s
`exits:`, `anchor`'s `default:`) are resolved away at compile time and never
serialized: an engine needs no member semantics at runtime.

## Version negotiation

Gate on `irVersion` by **MAJOR only** (since `0.13.0`):

- **Accept** any artifact whose `irVersion` major you implement.
- **Refuse** one from a newer major — minor and PATCH versions are compatible by
  default within that major and never gate.
- **Ignore unknown object fields** — optional fields are added append-only
  within a major line, so a newer minor or PATCH artifact still loads on an
  older engine.
- **Treat an unknown command `kind` as an error** — a new command kind is a
  real capability you cannot fake.

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
  changes `capabilityVersion`; `true` and `false` keep their 0.21 stamp.
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
{ "injected": true, "by": "auto-pose-reset",
  "explanation": "pre-loading `vesna`'s first emotion `level` seen ahead of the entrance" }
```

The old name was a collision, not a synonym. `end.reason` is an **opaque author
token you dispatch on** — the author writes it, you branch on it. This field is
**human-readable English the compiler wrote** to explain why a record it
synthesized exists, and nothing dispatches on it. Two keys with the same name
and nothing else in common is exactly the trap a renamed field removes, and
`explanation` reads correctly beside `by`.

Nothing else in the shape moves: no field added, no field retyped, no new
command `kind`, no changed constraint. `Provenance.injected` is **retained but
is now constant-`true`** — with `W-INJECT-CONFLICT` removed nothing can
construct a `false`, so do not read a `true` as distinguishing anything.
Removing the field would be a second IR break and is deferred.

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
serializing as `1.2000000000000002`. And `capabilityVersion` changes —
`W-INJECT-CONFLICT` left the code set and eleven codes joined it.

### What IR 0.9.0 changed

*History, retained for anyone still on the `0.9` line.* **Nothing in the shape.**
IR `0.9.0` was shape-identical to IR `0.8.0` — no field added, renamed, moved,
or retyped, no new command `kind`. The number moved only because Lute's
[versioning policy](https://github.com/journeyWorker/lute/blob/main/docs/versioning.md)
re-aligns every visible axis number on every release, and widening the gate to
accept `0.9` was the entire migration. What *did* move was artifact **content**:
`enums` began carrying the project's content-vocabulary domains (see
[the envelope](#the-envelope)) and `capabilityVersion` changed because the core's
vocabulary emptied — both new values in fields that already existed.

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
- **`shots` and the locale maps are append-only optional fields**, so by the
  ignore-unknown-fields rule a 0.7 engine still loads a 0.8 artifact that
  carries no `end` record.
- **The `addr` width invariant is new**, and it is the one change that can
  alter bytes in an artifact you already consume. See below.

## Addressing

Every executable record carries an `addr`, a position string
`"{shot}-{(index + 1) * 100}"` (e.g. `"001-0300"`). It is **regenerated on
every compile** — a position, not an identity. The stable content joins are
`lineId` / `voiceKey`.

Both segments are zero-padded to a width computed from the document — at least
`3` for the shot and `4` for the index, wider when the document needs it — and
that width is **uniform across the whole artifact**. The guarantee that follows
is the one you can rely on:

> Within one artifact, every emitted `addr` has the same length; therefore
> **lexicographic order over `addr` equals execution order.**

A document whose every shot emits fewer than 100 addresses, with fewer than
1000 shots, is byte-identical to what 0.7.0 produced — the widths only grow
past their minimums when the artifact actually needs them.

**If you may load artifacts built by a 0.7-or-earlier toolchain, compare `addr`
segment-wise numerically, never as a plain string.** Before 0.8.0 the index
segment was fixed at 4 digits, so a shot with 100+ records emitted `001-11500`
beside `001-1400` and string comparison reported `"001-11500" < "001-1400"` —
an engine ordering or range-checking addresses lexicographically would rewind
into already-played content.

## Streaming snapshot replacement

[`lute compile-stream`](/tooling/continuation-compiler/) emits a full ordinary
artifact snapshot for every accepted continuation unit. It does not define a
second IR or a patch language. `appendFrom` is the previous snapshot's command
count and only identifies the newly appended array region; it is not a runtime
PC and does not request execution of every record in that region.

When a snapshot arrives, replace the immutable program image and rebuild the
`addr -> command index` map. Retain the **numeric** command cursor, current
state values, facts, selected control-flow stack, and host-owned effect/
idempotency records. Initialize only newly declared state slots. Never reapply
an existing default or seed fact merely because the artifact object was
replaced.

Rebuilding address lookup is required because appending commands can widen
uniform address padding across the complete artifact. The compiler compares
typed addresses and control targets by numeric `(shot, index)` meaning, so this
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
`target`/`otherwise`/`converge` — are all [addrs](#addressing). Walk with a
program counter over an `addr → index` map, dispatching on `kind`:

```ts
// Every CEL slot carries its verbatim source under its own key — `option.when`,
// `arm.test`, `set.value` — and the lowered `expr` AST ONLY when that CEL is
// inside the closed §8.4 profile. A relational fact query or a `visited()` read
// carries raw text alone.
const evalSlot = (raw, expr, state, facts) =>
  expr !== undefined ? evalExpr(expr, state) : evalCel(raw, state, facts);

const index = new Map(artifact.commands.map((c, i) => [c.addr, i]));
let pc = 0;
while (pc < artifact.commands.length) {
  const cmd = artifact.commands[pc];
  let next: string | null = null; // null ⇒ fall through to pc + 1

  switch (cmd.kind) {
    // content & staging
    case "line":       present(cmd, state); break; // substitute cmd.placeholders
    case "background": case "music": case "sfx": case "vfx":
    case "sprite":     case "camera": case "cut": case "video":
      stage(cmd); break;

    // state & facts
    case "set":     writeState(state, cmd.path, cmd.op, evalSlot(cmd.value, cmd.expr, state, facts)); break;
    case "assert":  facts.assert(cmd.relation, cmd.args); break;
    case "retract": facts.retract(cmd.relation, cmd.args); break;

    // control flow
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

    // quest declarations & plugin bridges
    case "quest":   registerQuest(cmd); break;
    case "on":      registerHandler(cmd); break;
    case "accept":  acceptQuest(cmd.quest); break; // activates it iff still `unset`
    case "plugin":  callBridgeAndApplyEffects(cmd, state); break;

    default: throw new UnknownCommandKind(cmd.kind); // version gate: hard error
  }

  pc = next === null ? pc + 1 : index.get(next)!;
}
```

The full scene and quest command set is twenty-two kinds: `line`, `background`,
`music`, `sfx`, `vfx`, `sprite`, `camera`, `cut`, `video`, `set`, `assert`,
`retract`, `choice`, `match`, `hub`, `jump`, `end`, `barrier`, `quest`, `on`,
`accept`, `plugin`. A lore artifact adds the `entry` declaration head, which
the engine looks up rather than plays ([Lore entries](/language/lore-entries/)).

`accept` (dsl 0.21.0) is the scene-side `::accept{quest}`: the engine activates
that accept-driven quest if its state is still `unset` and ignores the record
otherwise ([Quests & scenes](/language/quests-and-scenes/#quests-meet-scenes-and-occasions)).

`end` carries an optional free-form `reason` — an author string
(`"completed"`, an ending id) Lute assigns no meaning to and the host MAY
surface. Terminating on it is identical to running off the end of `commands`,
except the reason is available.

## Localized text

A `line` record's `text` and a choice/hub option's `label` are always the
**source language** (`contentLang`). When the artifact was built with
[`lute compile --locales`](/tooling/cli/#--locales--merge-a-translation-bundle),
the record also carries a `texts` map (option: `labels`), locale tag →
translated string, keyed on the record's `lineId`:

```json
{
  "kind": "line",
  "addr": "001-0100",
  "role": "narration",
  "speaker": "narrator",
  "text": "Welcome to your new Lute project.",
  "lineId": "narrator.s01ep01.narrator_0010",
  "texts": { "ja-JP": "Lute プロジェクトへようこそ。" }
}
```

Both maps are omitted when empty, so an artifact compiled without `--locales`
is byte-identical to before, and a consumer that ignores them keeps rendering
the source language. Present a locale by looking it up in `texts` and falling
back to `text` — the compiler warns at build time (`W-L10N-MISSING`) about
exactly those gaps, so a complete bundle leaves nothing to fall back to.

## The runtime docs

Each surface has its own contract document under
[`docs/runtime/`](https://github.com/journeyWorker/lute/tree/main/docs/runtime):

- **[incremental-continuations.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/incremental-continuations.md)** — checked continuation compilation, full-snapshot replacement, cursor/state retention, stream finalization, and prefix stability.
- **[execution-model.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/execution-model.md)** — the artifact shape, version gate, addressing, and the dispatcher loop.
- **[state-lifecycle.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/state-lifecycle.md)** — the `scene`/`run`/`user`/`app`/`quest.<id>` tiers, initialization, and reset boundaries.
- **[cel-and-facts.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/cel-and-facts.md)** — evaluating the `expr` AST, the fact store's assert/retract deltas, and the stratified least-fixpoint the engine computes.
- **[quest-lifecycle.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/quest-lifecycle.md)** — `start`/`fail` precedence, required vs. optional objectives, monotone completion, run-tier quests, and lifecycle events.
- **[beats-and-occasions.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/beats-and-occasions.md)** — occasions and their target domains, beat candidates, eligibility and `once` spending, and the selection order.
- **[lore-entries.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/lore-entries.md)** — entry eligibility, presentation, first-read effects, and the `read` / `everRead` flags.
- **[timeline-semantics.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/timeline-semantics.md)** — the local clock, per-track cursors, barriers, and the one-writer-per-target invariant the checker guarantees.
- **[bridge-protocol.md](https://github.com/journeyWorker/lute/blob/main/docs/runtime/bridge-protocol.md)** — typed bridge calls, return shapes, `wait`, and resolved state effects.
