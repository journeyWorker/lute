# Quest lifecycle

A quest-kind artifact (`kind: "quest"`) carries `quest` and `on` records that
are **declaration data**, not sequential steps. The engine derives the whole
lifecycle from them; the author never writes `quest.<id>.state` (dsl §5.4). The
grounding here is `ir.rs::{QuestCmd, ObjectiveEntry, OnCmd, AcceptCmd, CelPair}`
and the proposal specs 0.2.0 §5–§6, 0.4.0 §4.6, 0.21.0 §7a, 0.22.0 §7, and
0.24.0 §2.

## The state machine

`quest.<id>.state` is the fixed lifecycle enum with values `unset` → `active` →
(`complete` | `failed`). Transitions are engine-derived, pure predicates over
state — keeping the lifecycle **total**:

```
unset ──start true / accept──▶ active ──all required objectives done──▶ complete
                                  │       (complete="any": one of them)
                                  └──── fail true / required by missed ─▶ failed
```

### Why a quest failed — `quest.<id>.failedBy`

On every `→ failed` transition the engine records the reason in the reserved
read-only path `quest.<id>.failedBy` (dsl 0.24.0 §2), an enum that reads
`unset` until the quest fails:

| value        | the quest failed because |
| ------------ | ------------------------ |
| `fail`       | its `fail` predicate held — authored, or synthesized from its required children (§Subquests) |
| `by`         | a required objective missed its `by` deadline |
| `until`      | a required objective missed its `until` deadline |
| `cascade`    | its parent ended while it was still `active` (§Subquests) |
| `superseded` | its `complete="any"` parent completed through another alternative (§Subquests) |

When a missed objective and `fail` fail the quest in the same settle, the
objective's kind wins (it is the more specific cause). Each objective's own
failure is readable too: `quest.<id>.objectives.<oid>.failed` is `false`
until the objective fails (§Objectives) and `true` from then on. Content may
read both paths anywhere a condition is legal — an epilogue that tells a
superseded alternative from a failed one — but never writes
(`E-QUEST-RESERVED-WRITE`) nor declares them (`E-QUEST-RESERVED-DECL`). The
engine derives them, so they are not rows of the artifact's `state` table;
an engine reads an unwritten `failedBy` as `unset` and an unwritten `failed`
as `false`. A run-tier quest's reset clears both.

### Activation — `start`

`QuestCmd.start` is an optional `{raw, expr}` predicate (`CelPair`):

- **absent** → the quest is *accept-driven* (below): it stays `unset` until it
  is accepted (or, for a referenced subquest, until its parent activates —
  unless it declares `activate="accept"`, §Subquests);
- **decides true** → activate (`state = active`) and fire the `questActive`
  handlers;
- **decides false** → the quest **never activates** (a clean compile guarantees
  this is not provably-always-false: `E-QUEST-UNREACHABLE`,
  `crates/lute-check/src/reachability.rs`);
- **unknown** → the quest is unknown; its objectives are unknown.

A quest with **no `start`** is *accept-driven*: an external accept (the CLI
`--accept` in `lute trace`, an engine "accept quest" action in production, or
an `accept` record in a scene) activates it. A quest that carries a `start`
predicate needs no accept (`E-TRACE-ACCEPT` guards the mismatch).

### Accepting from a scene — the `accept` record

`::accept{quest="<id>"}` (dsl 0.21.0 §7a.3) lowers to a record in the scene's
command stream, typically inside a choice branch:

```ts
type AcceptCmd = {
  kind: "accept"; addr: string; quest: string;
  applies?: "nextRun"          // dsl 0.24.0 §2 — ::accept{… at="nextRun"}
  /* + Stamp */
};
```

When the walk reaches it, the engine activates quest `quest` **if its state is
`unset`** — stamping `activatedAt` and firing `questActive` exactly as for any
other activation — and ignores the record otherwise (an active, complete, or
failed quest is left alone). An `activate="accept"` child (§Subquests)
activates only if its parent is `active` at that settle; otherwise the accept
is spent without effect. `check-project` guarantees the target is an
accept-driven quest of the project (`E-ACCEPT-TARGET`): a quest without
`start` that is either no one's subquest or declares `activate="accept"` —
so an engine never sees an `accept` for a quest with a `start` predicate, nor
for a child that activates with its parent. An accept-driven quest that no
`::accept` in the project names is `W-QUEST-NEVER-ACCEPTED`.

**Accepting for the next run — `applies: "nextRun"`** (dsl 0.24.0 §2).
`::accept{quest="<id>" at="nextRun"}` queues the acceptance instead: the
engine keeps the id and applies it right after the next run-start reset
(§Run-tier quests), so the new run's first settle activates the quest. This
is the hub-between-runs pattern: a run-tier bounty taken at a hub after the
run ended would otherwise be activated in the ending run and reset by the
next one. Queued ids are applied once; the record's state check (`unset`)
happens at application time.

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
evaluation instant). A **required** objective that misses its `by` or `until`
deadline (§Objectives below) fails its quest the same way, at the same point of the
evaluation instant: the same `failed` transition, `on="failed"` rewards,
`questFailed` handlers, and downward cascade to children. A `complete="any"`
quest is the exception: one missed alternative leaves the others open, and
the quest fails only through its `fail` predicate (§Subquests synthesizes it
for the case where every required objective has failed). The reason lands in
`quest.<id>.failedBy` (§The state machine).

### Completion — derived from objectives

Completion is **not authored**. When **all non-`optional` objectives are
`done`**, the quest transitions to `complete` and fires `questComplete`. The
compiler emits no control flow for this — `objectives` is a declaration table
inlined in the `quest` record (analogous to `HubCmd.options`), and the engine
derives the transition.

`QuestCmd.complete` (dsl 0.24.0 §2) is `"any"` for a `<quest
complete="any">` and absent for the default `all`: such a quest completes
when **any one** required objective is `done` — typically one subquest
among alternatives ("open the bridge by parley, by bribe, or by force"). Its
other still-`active` children then fail with `failedBy: superseded`
(§Subquests).

### Run-tier quests — `tier`

`QuestCmd.tier` (dsl 0.22.0 §7) is `"run"` for a `<quest tier="run">` and
absent for the default `user` tier. A quest document may declare several
quests, so the tier rides on each `quest` record, not on `QuestMeta`.

- **`user`** (absent) — the status persists across runs: a quest that reached
  `complete` or `failed` stays there for the rest of the save. Every quest
  before 0.22.0 behaved this way.
- **`run`** — when a run starts, the engine returns the quest to `unset`:
  `quest.<id>.state = unset`, every `quest.<id>.objectives.<oid>.done =
  false`, and `quest.<id>.activatedAt`, `quest.<id>.failedBy` and every
  `quest.<id>.objectives.<oid>.failed` cleared. The reset itself fires no
  handler. The quest is then a fresh instance, exactly as a repeatable quest's
  re-instantiation (§Activation instant above): at the next evaluation
  instant a `start` that holds activates it again (stamping `activatedAt` and
  firing `questActive`), an accept-driven one stays `unset` until it is
  accepted, and its objective bodies, rewards, and lifecycle handlers fire
  again on the new run's transitions.

A subquest's tier must equal its parent's (`E-QUEST-TIER-MIX`, 0.23.1): a
run-tier parent's end would cascade into a user-tier child that never resets,
and a user-tier parent that has ended never re-activates run-tier children
once they reset.

The reset belongs to the run boundary (`state-lifecycle.md`), beside the
`run.*` reset, and precedes the new run's first evaluation. `lute play`'s
`newRun` step performs exactly this: it resets run-tier state, run-tier facts
and run-tier quests, applies the step's seed and the acceptances queued with
`at="nextRun"`, then settles the lifecycle.

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
| `on`          | present only when authored (dsl 0.21.0 §7a.2): the occasion at which `done` is judged — see below. |
| `target`      | present only when authored, always beside `on` (dsl 0.23.0 §2): the objective is judged only when `on` is raised for this target — see below. |
| `by`          | present only when authored (dsl 0.23.0 §2): a `{raw, expr}` **deadline** predicate, judged at every evaluation instant — see below. |
| `until`       | present only when authored, always beside `on` (dsl 0.24.0 §2.1): a `{raw, expr}` deadline judged only when the objective's occasion is raised — see below. |

**Monotonic completion (dsl §6.3).** Once an objective's `done` predicate holds,
it stays recorded (`quest.<id>.objectives.<oid>.done = true`); a completed
objective does not un-complete within a quest instance (only a run-tier
quest's reset at a run start clears it, §Run-tier quests above). Because
completion is monotonic, the objective's **body segment plays exactly once**
per instance — when `done` first holds. The
body is a forward-only segment (ends by falling through / a forward converge —
no backward jump); an empty-body objective has `body: null` and emits no
segment.

A required objective whose `when` visibility gate is provably false is
`W-OBJECTIVE-HIDDEN` (a warning, not an error — `done` is evaluated
independently of visibility, so completion may still be reachable). A required
objective whose `done` is provably false is `E-OBJECTIVE-UNSATISFIABLE`; mark
such an objective `optional` if that is intended.

**Objectives judged at an occasion (dsl 0.21.0 §7a.2).** An objective with
`on` is **not** evaluated continuously. The engine evaluates its `done` only
when it raises that occasion (`beats-and-occasions.md`) while the quest is
`active` — the check point for conditions that only mean something at a
moment, such as "the shed stayed calm through the run" judged at `runEnd`.
Raising the occasion evaluates every such objective of every active quest,
then settles the quest as at any evaluation instant (below); an objective
that is not `done` then simply stays open until the next time the occasion
is raised. The occasion need not be answered by any beat: an occasion that
only objectives reference is still raised by the engine at its moment.
Monotonic completion is unchanged — once recorded, `done` stays recorded
(until a run-tier quest's reset, §Run-tier quests above). An objective
without `target` is judged at every raise of its occasion, whatever the
target. An objective with `target` (dsl 0.23.0 §2) follows the beat target
rule (`beats-and-occasions.md`): it is judged only when its occasion is
raised **for that target** — `on="talk" target="npc.maud"` is judged by a
`talk` raised for `npc.maud`, never by one raised for `npc.oskar` or without
a target.

**Deadlines — `by` and `until` (dsl 0.23.0 §2, 0.24.0 §2.1).** Both are
condition slots like `done` (the same `Bool` typing, definite-assignment and
fact rules). The **first** time an objective's deadline holds while it is
neither `done` nor already failed, the objective **fails**: it is never judged
again — neither `done` nor a deadline — for the rest of the quest instance. A
failed **required** objective fails its quest (§Failure above); a failed
`optional` objective only closes itself.

- **`by` is a moment.** It is judged at **every** evaluation instant, for
  every objective, with or without `on`: "done before the fifth day" is
  `by="run.day > 5"`, and the settle after the clock passes day 5 fails it
  whether or not the objective's occasion was ever raised. (0.23.1 judged an
  `on` objective's `by` only at its raise, so a player who never went there
  escaped the deadline; 0.24.0 reverses that.) An `on` objective whose `done`
  provably implies its `by` fails at the settle `by` comes true, before its
  occasion judges `done` (unless both happen in the step that raises it);
  `lute check` warns `W-DEADLINE-BEFORE-DONE` there and suggests `until=`.
- **`until` is a place.** It requires `on` (`E-BEAT-ATTR` without it) and is
  judged only when the objective's occasion is raised for its target, right
  after its `done` — the 0.23.1 raise-only rule: the moment the occasion
  answers is both the judgement and the deadline, so a beat answering the
  occasion that writes the state `until` reads cannot fail a correct answer.

`done` wins a tie: whenever `done` is judged in the same settle as a deadline
it is judged first, so an objective whose `done` and deadline become true at
the same instant is done, not failed; once `done` is recorded no deadline is
evaluated for it again. An `on` objective's `done` is judged only at its
raise, so the engine extends the tie to the **moment that raises it**: while
an occasion is being raised — the beats it presents, their settles, and (for
a clock advance) the settle after the clock moves — the `by` of each `on`
objective that raise judges is not evaluated until the raise has judged its
`done`; then `by` is judged at the settle right after the raise (so a `by`
that holds while `done` is false there still fails it). A `by` that comes
true in any other moment, between raises, fails the objective there. An
objective's failure is readable: `quest.<id>.objectives.<oid>.failed` is `true` from then
on (dsl 0.24.0 §2), and a required one's kind lands in `quest.<id>.failedBy`
(`by` / `until`); both reset with a run-tier quest.
The reference tooling shows it: `lute trace` records an objective decision
`failed` whose guard is the deadline's text (an undecidable deadline is
reported unresolved, exit 3), and `lute run` / `lute play` print
`<quest>.<objective> failed (by)` or `failed (until)` (`"failed": true,
"failedBy": "by" | "until"` on the objective record in `--json`). To raise
an occasion for a target in `lute trace` / `lute run`, write
`<occasion>@<target>` (`occasions: [talk@npc.maud]`, `--occasion
talk@npc.maud`); a `lute play` step raises it with `target:`. A `lute play`
`advance:` step moves a declared clock (`state-lifecycle.md` §The clock) and
settles the quests, so a `by` over the clock's day fails at the advance that
passes it.

**Scenes in objectives.** A `done` (like every condition slot) may read
`visited('<scene id>')` — true once that scene has been presented in this save
(`cel-and-facts.md`) — so a scene advances a quest simply by being played,
without relaying a flag.

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

  For a `complete="any"` parent (dsl 0.24.0 §2) one failed alternative must
  not fail it, so the synthesized part is instead the **conjunction** over
  every required objective, in document order — a child by failing, any
  other required objective by its reserved `failed` flag:

  ```
  <authoredFail> || (quest.c1.state == 'failed' && quest.c2.state == 'failed'
                     && quest.p.objectives.pay.failed)
  ```

  (parenthesized when it has more than one term; synthesized even when no
  required objective is a child). The parent fails — `failedBy: fail` —
  only once no alternative is left.

`ObjectiveEntry` also grows a `quest: Option<String>` field carrying the
referenced child id — omitted for authored `done=` objectives
(`skip_serializing_if = "Option::is_none"`, byte-stable for artifacts
without the feature). The field is what the two engine rules below key on;
unioned across artifacts exactly as `relations`/`rules`/`prereqEdges`
already are, it reconstructs the project-wide parent→child tree.

### Downward failure cascade (engine-derived)

When a quest transitions to a terminal state (`failed` or `complete`),
every child of that quest still `active` transitions to `failed` and fires
its `questFailed` handlers. The child's `failedBy` is `cascade` — or, when
the parent is a `complete="any"` quest that just **completed**,
`superseded` (dsl 0.24.0 §2): the alternatives the player did not take are
closed, and an epilogue can tell them from a failed attempt
(`quest.toll.failedBy == 'superseded'`). A cascade below a superseded child
is `cascade` again. A child that never activated (an `activate="accept"`
child nobody accepted) stays `unset`.

The reference points parent → child, so a child compiled in its own
document does not know which parent (if any) owns it; the cascade cannot
be synthesized per-artifact and must be an engine rule. Notes:

- A **required** child cannot be `active` when its `complete="all"` parent
  completes — its `complete` is part of the parent's derived completion — so
  that parent-`complete` arm only ever fails still-running **optional**
  children. Under `complete="any"` it fails every other running child,
  required or optional.
- The cascade is recursive: a cascaded `failed` transition is itself a
  terminal transition, so its own live children are cascaded in turn.
- `abandoned` is deliberately not a fifth lifecycle state. Reusing
  `failed` keeps the enum, its match exhaustiveness, and every consumer
  contract (diagnostics, IR, engine) untouched; journal copy that wants to
  say "abandoned" reads `quest.<id>.failedBy` (`cascade` / `superseded`).

### Activation of referenced children (engine-derived)

Being referenced refines a child's activation — the natural consequence of
"child = big objective" (an objective is evaluated only while its enclosing
quest is `active`):

- **child with no `start`** → the child activates when its parent
  activates, replacing the accept-driven default. An unreferenced quest
  with no `start` stays accept-driven.
- **child with `activate="accept"`** (`QuestCmd.activate`, dsl 0.24.0 §2) →
  the child does NOT activate with its parent: it waits for an accept (an
  `accept` record or the engine's accept action) and activates only while
  its parent is `active`. This is how a side path the player takes up in
  dialogue joins the journal when it is offered, not when the parent
  starts. `activate="accept"` and `start` are exclusive (`E-ATTR-TYPE`).
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
   recorded) — except an objective with `on`, which is evaluated only when
   its occasion is raised for its target (§Objectives above); raising an
   occasion is itself an evaluation instant, running steps 2–4 after those
   objectives;
2. evaluates the `by` deadline of every not-done, not-failed objective (with
   or without `on`, dsl 0.24.0 §2.1 — except, until the raise, an `on`
   objective an occasion being raised at this moment judges) and, at a
   raise, the `until` deadline of each objective judged in step 1; the first
   time one holds the objective fails (dsl 0.23.0 §2);
3. evaluates `fail` — and any required objective failed in step 2 (not for
   a `complete="any"` quest, dsl 0.24.0 §2) — **before** derived completion
   (§6.3 precedence), recording `quest.<id>.failedBy`; then derived
   completion (all required objectives, or any one under `complete="any"`),
   and the downward cascade of each terminal transition;
4. fires each lifecycle transition's handlers **once**.

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
- `target` — present only when authored (dsl 0.24.0 §2): the handler runs
  only when its event is raised as an occasion **for this target**
  (`<on event="bossDefeated" target="foe.regent">`, the beat target rule —
  `beats-and-occasions.md`); a plain world event, a raise for another target
  and the lifecycle events never run it. Without `target` a handler answers
  every raise of its event, as before.
- `body` — the `addr` of the action segment (a line, `::set`, `::assert` /
  `::retract`, etc.) the engine plays when the event fires and `when` holds.

A `questFailed` handler on a quest that can never reach `failed` never runs.
`check-project` warns `W-QUEST-HANDLER-DEAD` (dsl 0.22.0 §7) when the quest
has no authored `fail`, no required objective with a `by=` or `until=` deadline (dsl
0.23.0 §2), no required subquest objective whose child can itself fail (a
failing required child fails its parent through the synthesized `fail`), and
no parent quest (whose terminal transition would cascade-fail
it). The record is still emitted; the warning is for the author.

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

In that project-wide graph (dsl 0.24.0 §2) a bundle beat is a node too:
`visited('<lore doc id>.<beat id>')` in a scene's `after:` or a quest's
`after=` names the beat, which has no prerequisites of its own (its occasion
presents it whenever it is eligible). An accept-driven quest — a root quest
with no `start`, or a child with `activate="accept"` — that declares no
`after=` is anchored at every scene, bundle beat and quest body that
`::accept`s it, so it is drawn and reachable without repeating the accepting
scene in `after=`; a quest with an explicit `after=` keeps only its declared
edges. An anchor never proves a quest unreachable: the engine may accept it
outside any `::accept`.
