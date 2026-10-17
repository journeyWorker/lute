# Runtime event contract

**Status: 0.47.1 runtime contract.**

Runtime R2 executes a compiled bundle and returns effects as data. A **host**
—a game, player or verification server—owns rendering, pacing, bridge calls
and grant settlement. The runtime never calls those services itself.

The normative source is [Lute 0.40.0 §§3–12](../proposals/scenario-dsl/0.40.0.md#3-one-contract-definition-rust-types-generated-schemas),
as amended by [Lute 0.41.0 §5](../proposals/scenario-dsl/0.41.0.md#5-runtime-and-the-event-contract-041).
The Rust contract types in `crates/lute-runtime` are the single definition:
the committed [events schema](../../schemas/lute-events-0.47.schema.json) and
[snapshot schema](../../schemas/lute-snapshot-0.47.schema.json) are generated
from them with `schemars`. The generated schemas expose `Input`, `Seed`,
`Output`, `Event`, `Await`, `Rejected`, snapshots, and the read-only query
results. TypeScript hosts should also read the
[TypeScript runtime guide](./typescript-runtime.md).
This guide explains how a host uses them. Artifact dispatch is described in
[execution-model.md](./execution-model.md).

## Begin, step and restore

Load the immutable bundle once with `Runtime::load`: document execution IR
and `project.index.json` from `lute compile --all`. `begin(seed)` initializes
project defaults, applies the seed, settles quests and returns `(State,
Output)`. It is not an input. `restore(snapshot)` is the other way to obtain
state; it is also outside the step function.

`step(state, input)` takes state by value and returns its successor and one
output. It runs until the next **input point**, collecting every event along
the way. It does not stop at each dialogue line: pacing the returned batch is
the player's job. Clone the predecessor first if you need rollback; cloning
shares persistent world maps and copies the continuation, not the entire
world. Rejection returns the original state unchanged with `Rejected`.

The step is pure: no I/O, clock reads, randomness, threads or globals. Equal
runtime, state and input produce equal results on every platform. A host
supplies bridge answers and clock advancement explicitly.

## Seed and inputs

A seed carries the play script's top-level state, facts, `derive` (default
`true`) and save keys, not its `choose`, `bridges`, `steps` or `expect`.
This example is a `Seed`:

```json
{
  "state": [{"path": "user.endingsSeen", "value": 2}],
  "facts": ["met(mira, harbor)"],
  "derive": true,
  "save": {
    "visited": ["prologue"],
    "presented": {"user": ["mira.intro"], "run": []},
    "quests": {"findTheKey": "active"},
    "questInstances": {"findTheKey": 2},
    "entriesRead": {"user": [], "run": []}
  }
}
```

Inputs have a `type` tag. Values are JSON scalars—boolean, number or string—
resolved against the destination's declared type just as in `lute play`.
Unknown names or values that do not resolve are rejected. Arrays are applied
in written order.
The eight inputs are:

| Type | Accepted at | Effect |
|---|---|---|
| `raiseOccasion` | `idle`; `ended` only for an `outsideRun` occasion | Apply optional writes, bind payload, judge the seam and candidates, present selected beats, settle quests. |
| `choose` | Matching `awaitChoice` request | Resume with an open option. |
| `timeout` | Matching timed `awaitChoice` request | Resume exactly as choosing that menu's `default` option. |
| `bridgeResult` | Matching `awaitBridge` request | Apply result-dependent effects, then the optional `writes` (`state`, `facts`, `retract`, `accept`, recorded `effectOf` the tag with `by: "host"`), and resume. |
| `advanceClock` | `idle` | Apply optional writes and settle (if `terminal` then holds, stop: the clock stays, await `ended`), then move the clock, settling and raising the clock's occasions; optional `pick` controls the slot raise. The `clock` event follows every event of the move. |
| `hostWrite` | `idle` | Apply engine-owned writes and settle. |
| `worldEvent` | `idle` | Raise a declared world event to quest handlers. |
| `newRun` | `idle`, `ended` | Reset run-tier state and quests, apply queued `nextRun` accepts and optional writes, settle. |

An occasion can carry a target, payload, pick and writes. Every writes key is
optional: `state` mutations, ground `facts`, `retract` patterns (including
`_`) and quest `accept` ids. This is an `Input`:

```json
{
  "type": "raiseOccasion",
  "occasion": "talk",
  "target": "mira",
  "payload": {"gift": "rose"},
  "pick": {"beat": "mira.thanks"},
  "writes": {
    "state": [{"path": "run.storm", "value": "squall"}, {"path": "run.gold", "add": 5}],
    "facts": ["met(mira, harbor)"],
    "retract": ["inParty(elena, _)"],
    "accept": ["findTheKey"]
  }
}
```

`pick` may instead be `"pass"` (play's `pick: none`). `advanceClock.by` is
`"slot"`, `"day"`, an integer slot count, or a destination `{"to": …}`
holding a `weekday` and/or a `slot` (a slot alone may be written as a
string, `{"to": "dusk"}`), resolved like play's `advance: { to: … }`.
Each of the following examples is an `Input`:

```json
{"type": "advanceClock", "by": "slot", "pick": "pass", "writes": {"facts": ["met(mira, harbor)"]}}
```

```json
{"type": "advanceClock", "by": {"to": {"weekday": "sat", "slot": "dusk"}}}
```

```json
{"type": "choose", "request": 3, "option": "talk"}
```

```json
{"type": "bridgeResult", "request": 4, "fields": {"won": true, "score": 12}, "writes": {"state": [{"path": "user.wellDepth", "value": 3}], "facts": ["pulled(rope)"]}}
```

```json
{"type": "hostWrite", "writes": {"state": [{"path": "run.gold", "add": 5}]}}
```

```json
{"type": "worldEvent", "name": "harborClosed"}
```

```json
{"type": "newRun", "writes": {"accept": ["findTheKey"]}}
```

### Writes that belong to the ending (0.44)

Once the runtime answers `ended`, it accepts only `newRun` or an `outsideRun`
raise. A host write that belongs to the ending run — ending metadata, a final
score — MUST therefore ride as `writes` on the input that causes the terminal
transition, when that input carries writes: `raiseOccasion`, `advanceClock`,
`bridgeResult` or `hostWrite`, whose writes land before the transition. For
example, the dawn raise that ends a game carries the ending's name:

```json
{"type": "raiseOccasion", "occasion": "dawn", "writes": {"state": [{"path": "run.endedBy", "value": "dawnSeven"}]}}
```

`choose`, `timeout` and `worldEvent` carry no writes: an ending they cause
records its state in the story (a `::set` in the branch that ends) or in a
`hostWrite` sent before them. `newRun`'s writes seed the run it starts, where
the ended run's `run.*` values are readable as `prev.run.*`; they never write
into the ended run. A `hostWrite` after the ending is refused `E-RUNTIME-BUSY`
with the state unchanged, and its message says where the write belongs.

The host measures wall-clock time and sends `timeout`; wall-clock time never
enters the runtime. Timeout is accepted only for a timed `awaitChoice` whose
request matches. Its continuation records a choice of the menu's `default`,
so it is equivalent to `choose` with that option.

## Outputs, events and awaits

A wire `Output` has `eventVersion`, ordered `events` and one `await`:

```json
{"eventVersion": "0.47.1", "events": [], "await": {"type": "idle"}}
```

Consume events in execution order before acting on the await. Events are:

| Type | Fields | Host interpretation |
|---|---|---|
| `record` | `document`, `record` | A machine execution record: lines, staging with resolved timing, state/fact effects, grants, entries, decisions or plugins. |
| `presentation` | `occasion?`, `target?`, `beat`, `document`, `kind` | A presentation starts. |
| `presentationEnd` | `beat`, `document`, `reason?` | The walk finishes or encounters `end`. |
| `quest` | The `QuestAdvance` fields used by play's `quests[]` | A quest or objective changes status in a settle. |
| `clock` | `from`, `to`, `passed?` | The in-game clock moves. |
`Event.record.record` is decoded by the generated `Ir.Command` schema for
command-shaped records; supporting IR definitions are exported from
`@lute-lang/runtime` as `Ir`.

Since 0.42 every staging record (`bg`, `music`, `sfx`, `vfx`, `actor`,
`camera`, `cg`, `video`, `sequence`) carries `attrs`, the command's resolved
fields: authored values and lowered defaults, absent optional fields omitted,
always an object, never `kind`, `position`, `timing`, provenance or plugin
`stampAttrs`. A host reads media fields from `attrs`, not from the artifact.

| kind | `attrs` |
| --- | --- |
| `bg` | `location?`, `time?`, `assetId?` |
| `music` | `playback?`, `stop?`, `mood?`, `volume?`, `assetId?` |
| `sfx` | `sound?`, `assetId?` |
| `vfx` | `type`, `label?`, `transition?` |
| `actor` | `character`, `anchor?`, `action?`, `exit?`, `emotion?`, `costume?`, `posReset?`, `preload?` |
| `camera` | `focus?`, `framing?`, `move?`, `transition?` |
| `cg` | `assetId`, `display`, `layout?` |
| `video` | `assetId`, `display` |
| `sequence` | `name` |

`record.record` preserves the `lute run --json` execution-record shape plus
play-only line `delivery` and menu markers that run strips. A `line` record's
delivery fields are `role`, `lineId`, `voiceKey`, `as`, `emotion`, `segments`,
`texts` and `localeSegments`, each present when the line command has it.
`text` and `segments` have their `{{…}}` markers replaced by the rendered
values; `texts` and `localeSegments` (dsl 0.47.0 §2.2) are the command's
translations with each marker replaced by the value the source rendered for
the marker with the same referent — the text inside `{{ }}`, trimmed
(translators may reorder markers; a marker the source does not have stays as
written). The rendered record holds decoded text: the IR's escaped-interpolation
encoding (lute 0.47.1, see [execution-model.md](./execution-model.md)) is
read, not shown, and `\{{` in a translation stays a literal `{{`, as in the
source. Grant records
carry `quest`, `instance`, optional `objective`, `index`, `reward` and the
machine's remaining fields; `instance` is the settlement key. The host
settles the grant, not the runtime.

The read-only queries are part of the 0.40 contract (spec §4.1), not an
implementation-only report:

| Query | Result shape |
| `candidates(occasion, target?)` | `Candidate[]`, in selection order. Each candidate is `{ id, kind, document, priority, verdict, read, also, forMember?, rejudged }`; `forMember` is omitted for non-kind candidates. |
| `eligibility(beat, member?)` | `Candidate` or `null`. |
| `clock()` | `ClockAt` or `null`, where `ClockAt` is `{ day, slot }`. |
| `terminal()` | `boolean`. |
| `view(withFacts)` | `WorldView`, `{ state: Record<string, Value>, facts?: string[], quests: Record<string, string>, clock?: ClockView }`; `facts` is included only when requested, and `clock` is nullable when no position exists. |
| `evaluate(expr)` (0.41) | `Evaluation` `{ value: Value, unresolved?: string[] }` or a rejection. `expr` is an IR `exprNode` — the `expr` of any `{cel, expr}` pair the IR carries (an objective's `visibleWhen.expr`, a beat's `when.expr`). |
| `def(name, args?)` (0.41) | `Evaluation` or a rejection, for a project def the bundle carries in `project.index.json` `defs`; `args` is one JSON scalar per declared parameter (absent = `[]`), bound to its `param` nodes. Arity or type misfits (an `enum`/closed `domain`/`entity` argument must be a member) are `E-RUNTIME-INPUT`. |
| `gate(occasion, target?)` (0.41) | `Gate` `{ open: bool, unresolved?: string[] }`: whether a `raiseOccasion` with no writes would pass the seam now — `raisedWhen` false, or `terminal` holding for a non-`outsideRun` occasion, is `{open: false}`; an undecided seam lists its unresolved reads. An undeclared occasion is `E-RUNTIME-INPUT`. |

### Queries at every await; background barks (0.45)

The read-only queries `candidates`, `evaluate`, `def`, `gate` and `view` answer
at every await (`idle`, `awaitChoice`, `awaitBridge` and `ended`) and MUST NOT
change state. On a TypeScript `Session`, a query during a running input reads
the last committed state. While a choice or bridge is pending, inputs are
unchanged: only that await's answer is accepted.

A host MAY use `candidates(occasion, target?)` while waiting at `awaitChoice`
or `awaitBridge` to show a background bark outside the presentation: take an
eligible `kind: "entry"` candidate, find its head in the document's artifact
(`index.documents[].path == document`), and read the records from the head's
`body` position up to the next `entry`/`beat` head. When every one is a `line`
without `placeholders`, the bark is static: its lines give `text` (or
`texts[locale]`), `speaker` (display name `index.cast[speaker]`), `voiceKey`
and `lineId`, and showing them needs no step. A static bark shown this way
records nothing: it does not mark the entry read, spend `once`, or apply
writes. A bark that must record, or whose body is not static, MUST be raised
as its occasion after the story returns to `idle`. The runtime keeps one open
presentation; concurrent presentations are not planned.

`evaluate` and `def` (spec 0.41.0 §5.1) evaluate a condition over the state
exactly as a walk's guard does: the effective state (written, seeded, then
declared default), the fact store with derivation under the run's `derive`
mode (`holds`, `count`, `countDistinct`), `visited(...)`, and `now()` from the
clock paths. Evaluation is three-valued: `value` is `bool`, `int`, `double`,
`str`, or `unknown`, and an `unknown` value lists in `unresolved` why (an
unset state path, an undetermined derived fact, `validAt(...)`). An
`occasion.target` read outside a raise is unknown. A state suspended at an
await is evaluated over the world from before its input, as `view` is. The
value is never `error`: an evaluation error (division by zero, `int()` of a
non-number) is the rejection `E-RUNTIME-EVAL`. An `expr` with a node outside
the `exprNode` schema (a `param` node is admitted only in an index def), a
`def` name the bundle does not carry, and `def` arguments that miss the
declared parameters are `E-RUNTIME-INPUT`. No query changes state.

`Candidate.verdict` is a tagged `Verdict`: `{ verdict: "eligible" }`,
`{ verdict: "ineligible", premise }`, or `{ verdict: "unknown", why }`.
An ineligible `Premise` is tagged on `premise` and has one of these shapes:
`spent { once?, reason }`, `after { raw, unmet: Atom[], chapters }`,
`spentBy { raw }`, `when { raw }`, `gate { occasion, raw, reads:
GuardRead[] }`, or `terminal { occasion, raw }`. `ClockView` carries
`{ day, slot?, weekday?, weekdayLabel?, last?, ended? }`. These are generated
definitions in `lute-events-0.47.schema.json` and in
`packages/runtime/src/generated/Contract.ts`; hosts must decode them rather
than treating query JSON as untyped data.
`stage()` returns the persistent presentation stage:

```json
{
  "bg": {"location": "foyer", "time": "night", "assetId": "bg.foyer"},
  "music": {"playback": "theme", "mood": "calm", "volume": "0.8", "assetId": "theme"},
  "actors": [{"character": "guide", "anchor": "left", "emotion": "happy", "costume": "coat"}],
  "cg": null,
  "video": null
}
```

Staging records reduce this value in command order before emission. A new
scene, entry or bundle presentation resets it to empty; quest objective and
`<on>` bodies do not reset it. The stage is serialized in `world.stage` and
survives snapshot/restore. A state suspended at `awaitChoice` or
`awaitBridge` keeps the world from before its input, so the stage at that
await is serialized as `continuation.stage`, and `stage()` returns it.

| Await type | Fields | What the host can send next |
|---|---|---|
| `awaitChoice` | `request`, `menu` | `choose` with this request and an open option. |
| `awaitBridge` | `request`, `tag`, `document`, `position`, `attrs` (the call's IR `fields` — authored attributes, `@ref`s folded, omitted ones at their declared `default`; never the stamp's `wait`/`duration`/`delay` or an undeclared cross-cutting `stampAttrs` attribute — with `occasion.target` bound; `{}` when none), `results` (each result field content reads → its declared type from index `bridgeResults`) | Perform the bridge call outside the runtime, then send `bridgeResult` (optionally with `writes`). Non-blocking plugin records are write-free: answer them with `hostWrite` at the next `idle`, or declare the call a bridge. |
| `idle` | — | An idle input from the input table. |
| `ended` | `reason: "terminal"` | `newRun` or an `outsideRun` occasion. |
| `halted` | `kind`, `message`, optional `site` | None; restore an earlier snapshot to continue. |

 A choice menu carries `construct` (`branch` or `hub`), `id`, `document`,
`position`, `presentation`, optional `prompt`, `timeout` and `default`, and
ordered options with `id`, `verdict` (`open`, `closed` or `spent`), `exit` and
`once`. Hubs omit `timeout` and `default`.
An undecided guard never becomes an `unknown` menu verdict: it halts as
`incomplete`. Request ids are unsigned integers, incremented for each await
and never reused within a state lineage. Restoring preserves the pending id.

A choice or bridge call may occur anywhere an input reaches: nested menus,
lore entries, quest bodies, handlers, every beat of a sequence raise, the
raises of a clock cascade. The runtime resumes a suspended input by
deterministic replay: it keeps the world from before the input, the input
and the answers given so far, re-runs the input with those answers, and
stops at the next unanswered await. Each step returns only the events
produced since the previous await, so a host sees every event once.

## Rejections and halts

A `Rejected` value is separate from an output and changes nothing:

```json
{"code": "E-RUNTIME-REQUEST", "message": "The request does not match the pending await."}
```

The example message illustrates the shape; actual messages retain play's
human refusal text. Codes distinguish:

- `E-RUNTIME-BUSY`: an input the current await does not accept; presentations
  cannot interleave.
- `E-RUNTIME-REQUEST`: a choice or bridge request id is not the pending one.
- `E-RUNTIME-TIMEOUT`: `timeout` was sent while the pending await is not a
timed choice (a mismatched request remains `E-RUNTIME-REQUEST`).
- `E-RUNTIME-OPTION`: a missing or non-open choice option.
- `E-RUNTIME-BRIDGE-SHAPE`: a missing field content reads, or a value that does
  not fit its `results` type (`int` an integer within ±2^53, `double` a
  number, `bool` a boolean, `enum` a member string, otherwise a string).
- `E-RUNTIME-INPUT`: malformed input/seed, unknown path, relation, quest,
  occasion, event or beat, or a value incompatible with its declared type
  (an `int` state path takes, as `value` or `add`, an integer within ±2^53);
  for `evaluate`/`def`, a malformed `exprNode` (a `param` node included), a
  def the bundle lacks, or a `def` argument fault; `gate` of an undeclared
  occasion; `bridgeResult.writes` that do not resolve or write the clock's
  `day`/`slot` path.
- `E-RUNTIME-EVAL` (0.41): an `evaluate`/`def` condition evaluates to an
  error.
- `E-OCCASION-GATE` (0.41): a `raiseOccasion` whose `raisedWhen` is false, or
  raised while `terminal` holds and not `outsideRun`, is rejected with the
  state unchanged (its own writes and payload included); the message is play's
  refusal text. An undecided gate still halts `incomplete`. Existing
  pick/write/clock refusals keep play's refusals.
- `E-RUNTIME-HALTED`: any input after a halt.

A halt ends the state lineage. `incomplete` means the walk needed a value no
input can supply—an unseeded path, unbound `occasion.target`, or unresolved
`now()` / `validAt(…)` in R1. `error` means a failure inside the walk rather
than a rejected input, such as `E-FACT-EXCLUSIVE`. These correspond to play's
exit 3 and exit 1 respectively. Do not retry a halted state with more inputs;
restore an earlier snapshot.

## State, snapshots and replay

State includes persistent and per-run world data (not `scene.*`), the
continuation and pending request when suspended, the request counter and the
current await kind. Play script cursors, bridge queues and report scaffolding
are host data, not runtime state.

A snapshot serializes state with `snapshotVersion`, a bundle `project`
fingerprint, await, request, world and continuation. This guide does not
specify the internal world or continuation wire layout. Restore rejects a different
bundle fingerprint (`E-RUNTIME-SNAPSHOT-PROJECT`) or snapshot major.minor
(`E-RUNTIME-SNAPSHOT-VERSION`); content-version save migration is not R1.
For every reachable state and input, stepping a restored snapshot must equal
stepping the original state.

Snapshots exist only at input points. Saving while displaying a batch means
saving the preceding input-point snapshot plus the player's cursor into that
batch. Readable authored cross-run data such as endings and gallery unlocks
belongs in `user.*` and travels in the snapshot; per-line read-skip history
is a player profile and never enters runtime state.

The Event axis versions input, output and snapshot JSON and is aligned with
the other axes at `0.47.1`. Hosts gate on exact major.minor before 1.0.
Outputs carry `eventVersion`; snapshots carry the equal `snapshotVersion`.
The input log is `(seed or snapshot, inputs)`. Replaying it against the same
bundle must reproduce every serialized output byte-for-byte, including
bridge results recorded as inputs rather than rerunning external services.
`lute play --events` exposes the reference JSON Lines stream: first seed and
output, then input and output pairs, each stamped with `eventVersion`.

Deterministic JSON sorts object keys by code point at every level (execution
records included) and formats numbers as `lute run` does; never rely on key
order. Independent runtimes must also reproduce these orders:

1. Candidates start in index beat order, expand `forKind` in member order,
   then sort by descending priority, member/sub-kind/kind precedence and
   original index order. `first`, `sequence` and `all` consume that order.
2. Quest documents settle in path order to a fixpoint; an earlier document's
   update is visible to later documents in the same pass.
3. Quest rewards grant in declaration order.
4. Menus keep document option order; hub presentations number from zero per visit.
5. Datalog rules run in IR order within a stratum to a fixpoint; facts form an
   ordered set in lexical rendered-fact order.
6. Seasons and cadence use plan order and declared season-path order.
7. Queued accepts use queue order; world-event handlers use declaration order.
8. Bridge effects use IR effect order.
9. Visible state-map iteration (snapshots and `view`) is lexical by path.
