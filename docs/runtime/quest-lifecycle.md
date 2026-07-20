# Quest lifecycle

A quest-kind artifact (`kind: "quest"`) carries `quest` and `on` records that
are **declaration data**, not sequential steps. The engine derives the whole
lifecycle from them; the author never writes `quest.<id>.state` (dsl §5.4). The
grounding here is `ir.rs::{QuestCmd, ObjectiveEntry, OnCmd, CelPair}` and the
proposal specs 0.2.0 §5–§6 and 0.4.0 §4.6.

## The state machine

`quest.<id>.state` is the fixed lifecycle enum with values `unset` → `active` →
(`complete` | `failed`). Transitions are engine-derived, pure predicates over
state — keeping the lifecycle **total**:

```
unset ──start true / accept──▶ active ──all required objectives done──▶ complete
                                  │
                                  └────────── fail true ─────────────▶ failed
```

### Activation — `start`

`QuestCmd.start` is an optional `{raw, expr}` predicate (`CelPair`):

- **absent** → the quest activates at the start of the walk;
- **decides true** → activate (`state = active`) and fire the `questActive`
  handlers;
- **decides false** → the quest **never activates** (a clean compile guarantees
  this is not provably-always-false: `E-QUEST-UNREACHABLE`,
  `crates/lute-check/src/reachability.rs`);
- **unknown** → the quest is unknown; its objectives are unknown.

A quest with **no `start`** is *accept-driven*: an external accept (the CLI
`--accept` in `lute trace`, an engine "accept quest" action in production)
activates it. A quest that carries a `start` predicate needs no accept
(`E-TRACE-ACCEPT` guards the mismatch).

### Failure — `fail`, before completion

`QuestCmd.fail` is an optional predicate evaluated **before** derived
completion (dsl 0.2 §6.3 precedence): if `fail` decides true at any evaluation
instant, an activated instance transitions to `failed` and fires `questFailed`
— even if its objectives would otherwise complete. A `fail` that decides true
unconditionally is `E-QUEST-UNREACHABLE` (the quest fails at the first
evaluation instant).

### Completion — derived from objectives

Completion is **not authored**. When **all non-`optional` objectives are
`done`**, the quest transitions to `complete` and fires `questComplete`. The
compiler emits no control flow for this — `objectives` is a declaration table
inlined in the `quest` record (analogous to `HubCmd.options`), and the engine
derives the transition.

## Objectives

Each `ObjectiveEntry` in `QuestCmd.objectives`:

| field         | meaning |
| ------------- | ------- |
| `id`          | the objective id; recorded at `quest.<id>.objectives.<oid>.done`. |
| `done`        | a `{raw, expr}` completion predicate over state (**required** field). |
| `when`        | an optional `{raw, expr}` **visibility** gate — it gates whether the objective is *shown/tracked*, **not** the completion obligation (dsl §6.3). |
| `optional`    | `bool` (always present). A non-`optional` objective is *required*: it must be `done` for the quest to complete. |
| `title` / `titleLineId` | present only when authored; `titleLineId` is `{questId}.{objectiveId}` for localization. |
| `body`        | **always present**; the `addr` of the objective's completion-body segment, or `null` when the body is empty. |

**Monotonic completion (dsl §6.3).** Once an objective's `done` predicate holds,
it stays recorded (`quest.<id>.objectives.<oid>.done = true`); a completed
objective does not un-complete. Because completion is monotonic, the
objective's **body segment plays exactly once** — when `done` first holds. The
body is a forward-only segment (ends by falling through / a forward converge —
no backward jump); an empty-body objective has `body: null` and emits no
segment.

A required objective whose `when` visibility gate is provably false is
`W-OBJECTIVE-HIDDEN` (a warning, not an error — `done` is evaluated
independently of visibility, so completion may still be reachable). A required
objective whose `done` is provably false is `E-OBJECTIVE-UNSATISFIABLE`; mark
such an objective `optional` if that is intended.

## Re-evaluation cadence

After **activation** and after **every event**, the engine (0.4.0 §4.6):

1. re-evaluates each objective's `done` predicate (monotonic — once `true`,
   recorded);
2. evaluates `fail` **before** derived completion (§6.3 precedence);
3. fires each lifecycle transition's handlers **once**.

Event handlers see a **pre-event snapshot** of state and facts (a clone taken
before the event, dsl 0.2 §4.2); matching arms then run in document order,
applying their writes to live state.

## `<on>` handlers

An `OnCmd` is an independent event-condition-action record (not part of the
quest's declaration table):

- `event` — the event name it responds to. The engine-derived lifecycle events
  are `questActive` / `questComplete` / `questFailed` — these are fired by the
  engine on the transitions above, **never** by a user (`E-TRACE-EVENT` guards
  hand-firing them). Other event names are capability/world events the host
  raises.
- `when` — an optional `{raw, expr}` guard, evaluated against the pre-event
  snapshot.
- `body` — the `addr` of the action segment (a line, `::set`, `::assert` /
  `::retract`, etc.) the engine plays when the event fires and `when` holds.

## Cross-document reachability is out of scope for one artifact

A quest's `after` prerequisite (dsl §2.4) appears in the artifact only as raw
text under `prereqEdges` (`node`, `after`) — **unresolved and unvalidated**. A
single `compile` has no project root to resolve `visited(...)` / `completed(...)`
targets against. An engine reconstructs the project-wide prerequisite graph by
**unioning `prereqEdges` across every document's artifact**, exactly as it
unions `relations`/`rules`. The static reachability proof lives in
`check-project` / `lute scenario`, and even there it is **conservative under the
declared `after` routes** — never a claim about every runtime path.
