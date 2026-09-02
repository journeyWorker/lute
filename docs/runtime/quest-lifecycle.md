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

### Activation instant — `quest.<id>.activatedAt`

On the `unset → active` transition the engine MUST stamp
`quest.<id>.activatedAt` with the current narrative time (dsl 0.8.0 §5,
[state-lifecycle.md](./state-lifecycle.md)). It is the anchor authors pass as
`validAt(rel(args), t)`'s second argument. `validAt` is a point-in-time query —
true iff the fact was valid *at* `t` (dsl 0.3.0 §3.2, `established ≤ t <
invalidated`) — so an `*_AFTER`-style gate is the conjunction "true now, and
not yet true at activation": `holds(R) && !validAt(R, quest.<id>.activatedAt)`.
Content may read the stamp
but never writes it (`E-QUEST-RESERVED-WRITE`), and never declares it
(`E-QUEST-RESERVED-DECL`). For a repeatable quest the engine re-stamps it on
each re-instantiation, alongside clearing the instance's other scratch fields.

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

## Subquests

An `<objective quest="c"/>` names a child quest whose completion is the
objective. The mechanism is two compiler-synthesized surfaces plus two
engine-derived rules; the state machine, `activatedAt` stamp, and `<on>`
handler contract are all untouched. The design record and the diagnostic
set are in
[`docs/superpowers/specs/2026-08-31-lute-subquest-design.md`](../superpowers/specs/2026-08-31-lute-subquest-design.md).

### Synthesized surfaces (transparent to the engine)

For every `<objective id="oid" quest="c"/>` the compiler synthesizes:

- `ObjectiveEntry.done = { raw: "quest.c.state == 'complete'", expr: … }`.
  The field stays the required, always-present `CelPair`, so an engine
  unaware of subquests evaluates a subquest objective the same way it
  evaluates any other — one predicate over `quest.<id>.state` (dsl §5.4).
  Derived quest completion ("all non-`optional` objectives `done`") is
  unchanged; marking the objective `optional` decouples the child from the
  parent's completion in both directions.
- The parent quest's effective `QuestCmd.fail` becomes the disjunction of
  the authored predicate (if any) and one `quest.<c>.state == 'failed'`
  test per **required** subquest child, in document order:

  ```
  <authoredFail> || quest.c1.state == 'failed' || quest.c2.state == 'failed'
  ```

  An `optional` child contributes nothing. `fail`'s precedence over derived
  completion (dsl 0.2 §6.3) is unchanged, so a required child failing
  resolves the parent to `failed` at the next evaluation instant even if
  the remaining objectives could otherwise complete.

`ObjectiveEntry` also grows a `quest: Option<String>` field carrying the
referenced child id — omitted for authored `done=` objectives
(`skip_serializing_if = "Option::is_none"`, byte-stable for artifacts
without the feature). The field is what the two engine rules below key on;
unioned across artifacts exactly as `relations`/`rules`/`prereqEdges`
already are, it reconstructs the project-wide parent→child tree.

### Downward failure cascade (engine-derived)

When a quest transitions to a terminal state (`failed` or `complete`),
every child of that quest still `active` transitions to `failed` and fires
its `questFailed` handlers.

The reference points parent → child, so a child compiled in its own
document does not know which parent (if any) owns it; the cascade cannot
be synthesized per-artifact and must be an engine rule. Notes:

- A **required** child cannot be `active` when its parent completes — its
  `complete` is part of the parent's derived completion — so the
  parent-`complete` arm of this rule only ever fails still-running
  **optional** children.
- The cascade is recursive: a cascaded `failed` transition is itself a
  terminal transition, so its own live children are cascaded in turn.
- `abandoned` is deliberately not a fifth lifecycle state. Reusing
  `failed` keeps the enum, its match exhaustiveness, and every consumer
  contract (diagnostics, IR, engine) untouched; journal copy that wants to
  say "abandoned" reads the parent's own terminal transition to distinguish
  the cases.

### Activation of referenced children (engine-derived)

Being referenced refines a child's activation — the natural consequence of
"child = big objective" (an objective is evaluated only while its enclosing
quest is `active`):

- **child with no `start`** → the child activates when its parent
  activates, replacing the walk-start / accept-driven default. An
  unreferenced quest with no `start` keeps today's semantics exactly.
- **child with a `start` predicate** → the predicate is evaluated only
  while the parent is `active`; the effective gate is the conjunction
  "parent is `active` && `start` holds".

Same union-derived tree as the cascade rule above. `quest.<child>.activatedAt`
stamping is unchanged: the stamp fires at the `unset → active` transition
whatever gate produced it, and the reserved-path guards
(`E-QUEST-RESERVED-DECL`, `E-QUEST-RESERVED-WRITE`) still hold.

## Rewards

A `<reward kind= target= amount= when= on=/>` (dsl 0.16.0 §2) is a
**declaration**, not flow: a self-closing element legal only as a direct
child of `<quest>` or `<objective>`. It lowers to pure data — a
`RewardEntry` in `QuestCmd.rewards` or `ObjectiveEntry.rewards`, never
synthesized into a handler, command, or predicate (the exact inverse of
the subquest surfaces above; `::grant` is plugin vocabulary and the core
language must never depend on any plugin's directive existing, spec D-B).
The engine grants; the reference runtime emits a deterministic transcript
event per grant.

### When grants fire (engine-derived)

Grants ride the same transitions the lifecycle already exposes; no new
state, no new event surface:

- **objective grants** — when an objective first becomes `done` (the same
  monotonic transition its body segment plays on, §Objectives above).
- **quest `on="complete"` grants** (or `on` omitted — `complete` is the
  default) — at the `→ complete` transition.
- **quest `on="failed"` grants** — at the `→ failed` transition, including
  a parent-cascade `failed` (§Subquests above). A cascade-failed child
  grants its `on="failed"` rewards **exactly once**, on the cascaded
  transition, before its own `questFailed` handlers fire.

Each reward is granted **at most once per quest instance** — the same
monotonicity as objective bodies. A repeatable quest re-arms its rewards on
re-instantiation, alongside clearing the instance's other scratch fields.

### `when` is evaluated at the grant instant

`RewardEntry.when` is the ordinary `{raw, expr}` CEL slot (checker
profile, `E-MAYBE-UNSET`, unset-sentinel guards, LSP hover/fill). The
engine evaluates it against the same pre-transition state/fact snapshot
the triggering transition observed — the reward is skipped exactly when
`when` decides non-`true`, and it is not re-armed (a skipped grant does
not fire later even if `when` later flips true).

### Order within one owner is declaration order

Within one owner (`Quest.rewards` or `Objective.rewards`), grants fire in
document order. When one event settles both an objective and its enclosing
quest, **objective grants precede quest grants**, and all grants precede
the corresponding lifecycle handler body — so a `questComplete` handler's
narrative reads live state after every grant of that transition has
applied. The reverse order would make "you received X" a lie at the
instant it plays (spec D-D).

### Ranges are declaration data — the reference runtime never rolls

`RewardEntry.amount` is either a scalar (`amount: N`) or a range (`amountMin: N`
+ `amountMax: M`, with integer bounds and `N <= M`). A range is a
**declaration**, not a roll: journals render "N–M", balancers compute an
expectation, and the roll itself is the simulation's half (the 0.0.1 dice
contract). The reference runtime keeps output byte-deterministic by
emitting the declared shape verbatim — `lute run` / `play` / `trace`
carry `amount` or `amountMin`+`amountMax` in the grant event exactly as
authored, never a rolled sample (spec D-C).

### Grant transcript event

The reference runtime emits one deterministic event per grant, mirrored
by `lute trace` as a `Step::Grant`:

```
{ "kind": "grant",
  "quest": "<questId>",
  "objective": "<oid>"?,            // present iff an objective grant
  "reward": { …RewardEntry sans when… },
  "onFailed": true?                 // present iff the reward's on == "failed"
}
```

`objective` is present only for objective-owned rewards. `onFailed` is
present only for quest-owned `on="failed"` rewards (the default
`complete` transition omits the field). `reward` carries the wire-shape
`RewardEntry` minus `when` — a grant event only fires when `when` (if
authored) decided `true`, so re-serializing the predicate is noise. Range
bounds are the declared literals; a rolled amount NEVER appears.

### Coexistence with `<on questComplete>` + `::grant`

The `<on questComplete>` + plugin-`::grant` idiom (0.2.0 §6.5) remains
fully supported for narrative staging and engine-specific effects:
declarative rewards and handler-driven grants coexist. Double-grant
detection across the two is a **non-goal** — plugin directive semantics
are opaque to the checker.

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
targets against. Each `prereqEdges[].node` is the containing document's
**canonical scene id** — a scene's authored `id:` when it declares one, else
the derived `{character}.{episodeId}` fallback (dsl 0.15.0 §2); a quest keeps
its authored quest id. An engine reconstructs the project-wide prerequisite
graph by **unioning `prereqEdges` across every document's artifact**, exactly
as it unions `relations`/`rules`. The static reachability proof lives in
`check-project` / `lute scenario`, and even there it is **conservative under the
declared `after` routes** — never a claim about every runtime path.
