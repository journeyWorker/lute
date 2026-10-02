# Narrative

**Semantic id:** `lute.core/1` (baseline document, line, choice, match, hub, and control-flow records). Components/templates expand at compile time and have no semantic id.

## Constructs and rules

- **Scene/shot:** `kind: scene` frontmatter and `##` shots define layout; shots emit addressed command segments in source order.
- **Line/speaker/code:** `@speaker{code,...}: text` emits a line with source text and stable `lineId`/`voiceKey`; `code` is per-speaker and may be backfilled by `lute tag`.
- **Branch/choice:** `<branch>` and `<choice>` show eligible options, record the selected id in scene state, jump to its target and converge. Guards are evaluated at the choice record.
- **Hub:** `<hub>` may be presented repeatedly; `once`, `exit`, visited state and `<return>` control revisits and convergence.
- **Match:** `<match on>` evaluates arms top-to-bottom; first match wins. `is` shorthand and `test` CEL lower to arms; an unset subject matches only `is="unset"`.
- **Components/templates:** typed `component:` files and `::use` expand before lowering; parameter and effect checks apply at every use. Engines never see `::use`.
- **Interpolation/localization:** `{{path}}`, `{{@def(args)}}` and reserved/occasion tokens resolve at presentation; locale maps join after lowering by stable line/option ids.

## Evaluation and lowering

Normalize/expand → lower and address → walk command array; control-flow targets and convergence alter the PC. Lines and guards read live state at their position. See [module ordering](../design/modules.md#narrative) and [execution model](../runtime/execution-model.md).

## Diagnostics

Checker validates speaker/cast, ids, option totality, target/converge references, CEL types, match exhaustiveness/dead arms, component cycles/arity/types, interpolation paths and locale key collisions.

## Example

```lute check
---
kind: scene
id: greeting
---
## opening
@guide{code="hello"}: Hello.
<branch id="next">
  <choice id="go" label="Go">
  </choice>
</branch>
## end
@narrator: A destination.
```

## History

[DSL 0.33 §1](../proposals/scenario-dsl/0.33.0.md#1-domain-modules-and-evaluation-order), [execution model](../runtime/execution-model.md).
