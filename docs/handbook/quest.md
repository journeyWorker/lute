# Quest

**Semantic ids:** `lute.quest.lifecycle/1`, `lute.quest.rewards/1`.

## Constructs and rules

- **Quest declaration/lifecycle:** `<quest id title start fail follows tier activate complete accept rearm>` declares `unset → active → complete|failed`; fail and required deadline failure precede derived completion. IR is `QuestCmd`; reserved state is engine-owned.
- **Objectives:** `<objective id done title visibleWhen optional>` completes monotonically. `visibleWhen` controls presentation only, never completion. `on`/`target` judge at matching raises; `by` is continuous; `until` is raise-local and done wins the tie.
- **Subquests:** `quest="child"` synthesizes objective completion from child `complete`; terminal parents cascade active children (`cascade`, or `superseded` for `complete="any"`).
- **Accept/start:** `start` roots evaluate at settle; `::accept{quest=...}` queues activation, with `at="nextRun"` applied after reset. `follows` is graph metadata, not an activation gate.
- **Cadence/rearm:** `tier="run|season:name"` and false→true `rearm` reset scratch and preserve the save-wide instance counter.
- **Rewards/grants:** `<reward id kind target amount when outcome>` entries are ordered declaration data, not body commands. The optional `id` token (`[A-Za-z][A-Za-z0-9_-]{0,63}`) is unique within its quest (`E-REWARD-DUP`) and is the stable reward key in source maps, the model and IR `RewardEntry.id`; an untagged reward falls back to its declaration index. Fresh objective grants precede quest grants and corresponding handlers; grant identity is `(quest, instance, objective?, index)`.
- **Handlers:** `<on event when target>` uses the pre-event snapshot. With default `judge: after`, world handlers run after beat presentation and before objective judgment; with `judge: before`, objective judgment runs before beats and handlers are deferred until after beats. Lifecycle handlers run after grants.

## Evaluation and lowering

Clock/season observation → child activation → objective done/deadlines → fail reason → completion → cascades → grants → lifecycle handlers. For an occasion, default `judge: after` is presentation → world handlers → objective judgment; `judge: before` is objective judgment → beats → deferred handlers. Every crossed clock position is a settle point. Declarations lower to `QuestCmd`, `ObjectiveEntry`, `OnCmd`, `AcceptCmd`, and addressed bodies. Detailed ordering: [modules §Quest ordering](../design/modules.md#quest) (or the corresponding runtime [quest lifecycle](../runtime/quest-lifecycle.md)).

## Diagnostics

Checker enforces required `done`, Boolean CEL, legal objective attributes, target/occasion vocabulary, and quest graph/tier compatibility. Typical errors include invalid deadline combinations, writes to reserved quest paths, and impossible required objectives. Engine refusal of a tampered owned write is `E-RUN-OWNED-WRITE`.

## Example

```lute check
---
kind: quest
id: rescue
---
<quest id="rescue" title="Rescue" start="true">
  <objective id="safe" done="true" title="Safe" />
  <reward id="medal" kind="item" target="medal" amount="1" />
</quest>
```

## History

[DSL 0.37 §3.5](../proposals/scenario-dsl/0.37.0.md#35-control-and-logic-names), [DSL 0.33 §1–2](../proposals/scenario-dsl/0.33.0.md#1-domain-modules-and-evaluation-order), [DSL 0.32 §6](../proposals/scenario-dsl/0.32.0.md#6-grant-identity), [quest lifecycle](../runtime/quest-lifecycle.md).
