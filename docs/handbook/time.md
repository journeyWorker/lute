# Time

**Semantic ids:** `lute.time.clock/1`, `lute.time.cadence/1`, `lute.time.seasons/1`.

## Constructs and rules

- **Clock declaration/derived paths:** schema `clock.day`, `slot`, `week`, `raise`, and finite bounds define engine-owned movement. `clock.index`, weekday/label and `clock.ended` are read-only derived paths.
- **Advance/calendar:** play `advance` and beat `advances` move the clock; every crossed position settles quests and raises declared occasions at explicit stops. `lute calendar` is bounded analysis, not runtime truth.
- **Run/prior mirrors:** `newRun` resets run-tier state/facts/quests, applies queued next-run accepts, then settles. `prev.run.*` and `prev.season.*` expose the prior window and are read-only.
- **Seasons:** `live` false→true opens a season, snapshots prior values, resets season state/cadence and season-tier quests; truth staying true does not reopen it.
- **State tiers/reset:** scene resets per presentation; run/user/app persist by their scopes; quest and season resets follow declarations and preserve quest instance counters.

## Evaluation and lowering

Clock movement → reset/derived-path refresh → quest settle → clock occasion raise. Intermediate day-end/day-start stops are observed. See [module ordering](../design/modules.md#time) and [state lifecycle](../runtime/state-lifecycle.md).

## Diagnostics

The checker validates clock domains, finite bounds, tier/reset compatibility and temporal argument types (`E-TEMPORAL-ARG`). Calendar reports bounded results and must label unknown/out-of-bound cases.

## Example

```yaml
clock:
  day: run.day
  slot: run.slot
  slots: [morning, night]
  raise: slot
```

## History

[DSL 0.33 §1](../proposals/scenario-dsl/0.33.0.md#1-domain-modules-and-evaluation-order), [state lifecycle](../runtime/state-lifecycle.md).
