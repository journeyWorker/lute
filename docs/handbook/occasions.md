# Occasions

**Semantic ids:** `lute.occasions.selection/1`, `lute.occasions.gates/1`.

## Constructs and rules

- **Occasion vocabulary/raise:** manifests and clock `raise` declare capability names; hosts raise by name and optional target. Target binding supplies `occasion.target`.
- **Beat/entry eligibility:** `on`, `target`, `when`, `priority`, `once`, `share`, `spentBy`, `after`, `also` and `sequence` select candidates once per raise; priority descends, then project index order.
- **Gates/terminal:** `raisedWhen` is checked before a raise. `terminal` suppresses ordinary occasions; explicitly `outsideRun` occasions remain eligible.
- **Cadence:** `once` supports run/user/day/slot/week/season and `share` spends all members atomically. `spentBy` latches spent status for its period.

## Evaluation and lowering

Bind target → gate/terminal check → one eligibility snapshot → selection → presentation → settle before the next presentation. `judge="before"` moves objective judgment ahead of beats; default `after` follows them. See [module ordering](../design/modules.md#occasions) and [beats and occasions](../runtime/beats-and-occasions.md).

## Diagnostics

Checker validates occasion/target domains, gate Boolean CEL, cadence values, `after` reachability and priority ties; it reports impossible or stranded beats with bounded evidence. Explicit raises blocked by gates are engine-visible refusals/suppression, not successful presentations.

## Example

```yaml
occasions:
  - name: bell
```

## History

[DSL 0.33 §1–2](../proposals/scenario-dsl/0.33.0.md#1-domain-modules-and-evaluation-order), [beats runtime](../runtime/beats-and-occasions.md).
