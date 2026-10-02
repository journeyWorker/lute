# Knowledge

**Semantic ids:** `lute.knowledge.facts/1`, `lute.knowledge.rules/1`, `lute.knowledge.temporal/1`.

## Constructs and rules

- **Vocabulary:** `entities`, `enums`, typed `relations`, tiers, keys, `reserved`, and `excludes` define closed domains and ownership. Seeds are initial ground facts.
- **Fact deltas:** `::assert` adds a ground fact; `::retract` removes matching facts. Commands are sequential and later commands see earlier deltas.
- **Queries/time:** CEL `holds`, `count`, `countDistinct`, `validAt`, and `now` query current facts or half-open validity history. `validAt` takes narrative time, not a bare literal.
- **Datalog:** `derive` relations and rules compute a deterministic stratified least fixpoint. Positive recursion is allowed; negation and aggregates require lower-stratum safety; guards use scalar CEL and cannot call fact functions.
- **Evidence/integrity:** May/Must analysis reports impossible, guaranteed or possible; runtime remains authoritative. Truth, testimony, belief and inference are ordinary distinct relations.

## Evaluation and lowering

Schema composition → relation/rule validation → command-order fact deltas → lower-stratum closure → queries. Reserved engine writes may trigger lifecycle reevaluation. See [module ordering](../design/modules.md#knowledge) and [CEL and facts](../runtime/cel-and-facts.md).

## Diagnostics

The checker rejects unknown/duplicate relations, wrong arity/domain, unsafe rules, negation cycles, aggregate cycles, derived/reserved writes and fact calls in guards (`E-DATALOG-GUARD-FACT`).

## Example

```yaml
relations:
  knows:
    args: [person, person]
rules:
  - "trusted(X) :- knows(X, Y)"
```

## History

[DSL 0.33 §1–2](../proposals/scenario-dsl/0.33.0.md#1-domain-modules-and-evaluation-order), [fact runtime](../runtime/cel-and-facts.md), [DSL 0.32 §2](../proposals/scenario-dsl/0.32.0.md#2-host-functions).
