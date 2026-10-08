# Runtime event contract

**Status: 0.38.0 runtime contract.**

Runtime R1 executes a compiled bundle and returns effects as data. A **host**
—a game, player or verification server—owns rendering, pacing, bridge calls
and grant settlement. The runtime never calls those services itself.

The normative source is [Lute 0.38.0 §§3–12](../proposals/scenario-dsl/0.38.0.md#3-the-step-function).
The [events schema](../../schemas/lute-events-0.38.schema.json) defines the
wire fields; its `$defs` expose `Input`, `Seed`, `Output`, `Event`, `Await`
and `Rejected`. This guide explains how a host uses them. Artifact dispatch
is described in [execution-model.md](./execution-model.md).

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
in written order. The seven inputs are:

| Type | Accepted at | Effect |
|---|---|---|
| `raiseOccasion` | `idle`; `ended` only for an `outsideRun` occasion | Apply optional writes, bind payload, judge the seam and candidates, present selected beats, settle quests. |
| `choose` | Matching `awaitChoice` request | Resume with an open option. |
| `bridgeResult` | Matching `awaitBridge` request | Apply result-dependent effects and resume. |
| `advanceClock` | `idle` | Apply optional writes, move the clock, settle and raise the clock's occasions; optional `pick` controls that raise. |
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
{"type": "bridgeResult", "request": 4, "fields": {"won": true, "score": 12}}
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

There is no dialogue `Advance` or `Timeout` input. Choice timeout seconds
are carried in the IR, but expiry policy belongs to the host: send `choose`
with the option that policy selects.

## Outputs, events and awaits

A wire `Output` has `eventVersion`, ordered `events` and one `await`:

```json
{"eventVersion": "0.38.0", "events": [], "await": {"type": "idle"}}
```

Consume events in execution order before acting on the await. Events are:

| Type | Fields | Host interpretation |
|---|---|---|
| `record` | `document`, `record` | A machine execution record: lines, staging with resolved timing, state/fact effects, grants, entries, decisions or plugins. |
| `presentation` | `occasion?`, `target?`, `beat`, `document`, `kind` | A presentation starts. |
| `presentationEnd` | `beat`, `document`, `reason?` | The walk finishes or encounters `end`. |
| `quest` | The `QuestAdvance` fields used by play's `quests[]` | A quest or objective changes status in a settle. |
| `clock` | `from`, `to`, `passed?` | The in-game clock moves. |

`record.record` preserves the `lute run --json` execution-record shape plus
play-only line `delivery` and menu markers that run strips. Grant records
carry `quest`, `instance`, optional `objective`, `index`, `reward` and the
machine's remaining fields; `instance` is the settlement key. The host
settles the grant, not the runtime.

Selection diagnostics are not events. Pure read-only queries
`candidates(occasion, target)`, `eligibility(beat, member)`, `clock()`,
`terminal()` and `view(with_facts)` provide host inspection without changing
state. CLI candidate/evidence reports are separate report capture.

| Await type | Fields | What the host can send next |
|---|---|---|
| `awaitChoice` | `request`, `menu` | `choose` with this request and an open option. |
| `awaitBridge` | `request`, `tag`, `document`, `position`, `fields` (field → declared type) | Perform the bridge call outside the runtime, then send `bridgeResult`. |
| `idle` | — | An idle input from the input table. |
| `ended` | `reason: "terminal"` | `newRun` or an `outsideRun` occasion. |
| `halted` | `kind`, `message`, optional `site` | None; restore an earlier snapshot to continue. |

A choice menu carries `construct` (`branch` or `hub`), `id`, `document`,
`position`, `presentation`, optional `prompt` and `timeout`, and ordered
options with `id`, `verdict` (`open`, `closed` or `spent`), `exit` and `once`.
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
- `E-RUNTIME-OPTION`: a missing or non-open choice option.
- `E-RUNTIME-BRIDGE-SHAPE`: a missing field content reads, or wrong value type.
- `E-RUNTIME-INPUT`: malformed input/seed, unknown path, relation, quest,
  occasion, event or beat, or a value incompatible with its declared type.
- `E-OCCASION-GATE` and existing pick/write/clock refusals: the same refusals
  as play, including a false `raisedWhen` or a held terminal condition.
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
the other axes at `0.38.0`. Hosts gate on exact major.minor before 1.0.
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
