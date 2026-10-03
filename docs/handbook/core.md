# Core

**Semantic ids:** `lute.core/1` (baseline), plus the CEL and state rules below.

## Constructs and rules

- **Project/document declarations.** Frontmatter (`kind`, `id`, project/schema declarations) resolves before document checking. `meta.id` is the canonical document key; declarations lower into execution-IR envelopes and command arrays. Diagnostics include unknown kinds, duplicate ids, unresolved references and admission errors.
- **State tiers and ownership.** `scene`, `run`, `user`, `app`, `season:<name>` and reserved engine paths have declared types, defaults and reset scopes. Content may write only content-owned paths; engine-owned paths are readable and engine-settled. Invalid types, defaults, or ownership writes are diagnostics (`E-STATE-DECL`, `E-RUN-OWNED-WRITE`).
- **CEL conditions.** Every condition/value slot is standard CEL in the closed 0.32 profile; `@def` and match `$` expand at compile time. `has()` tests presence. Errors make guards false; erroneous sets abort without a partial write. See [DSL §1–4](../proposals/scenario-dsl/0.32.0.md#1-conditions-are-standard-cel).
- **Execution IR and capabilities.** The compiler emits `cel` plus typed `expr`, `celEnv`, and sorted `requiredSemantics`; engines check capabilities before playback. Unknown required ids refuse loading, while plugin records remain `kind: "plugin"` and use `capabilityVersion`.
- **Identity and addressing.** `addr` is positional; author ids, `lineId`, and `voiceKey` are stable joins. Source maps survive lowering. Duplicate or colliding ids are checker errors.
## Project model and evidence (0.34.0)

All project commands load through one project model per nearest manifest root.
The model is the shared snapshot for project checking, graph queries, impact,
constraints, calendar, play, and test; this is a loading contract, not a
runtime behavior change.

Analysis results use the evidence vocabulary `proven`, `witnessed`, `bounded`,
`heuristic`, and `unknown`. Diagnostics may serialize optional `evidence` and
`scope`; bounded diagnostics include a non-empty scope. Human output scopes
bounded and heuristic claims and never turns “not found within bounds” into
“no path exists”.

Project manifests may declare typed `constraints:`. The `reachable`,
`completable`, `speaksOnlyWhen`, and `noSingleSlotProgress` kinds produce
`holds`, `violated`, or `unknown` results; `lute constraints` reports every
result, while `check-project` reports violations only.

## Evaluation and lowering

Project/schema resolution → CEL/type checking → component/plugin normalization → lowering/addressing → optional locale merge → engine interpretation. The execution order is the command-array order except timeline scheduling and explicit control-flow targets.

## Diagnostics

Use the structured checker verdicts *impossible*, *guaranteed*, *possible*; `possible` means undecided, not false. Bounded analyses state their bounds; no bounded failure proves impossibility.

## Example

```yaml
kind: scene
id: foyer
```

## History

[DSL 0.32 §4](../proposals/scenario-dsl/0.32.0.md#4-activation-and-evaluation), [§5](../proposals/scenario-dsl/0.32.0.md#5-engine-owned-state-in-the-execution-ir), [§7](../proposals/scenario-dsl/0.32.0.md#7-execution-ir-naming-versions-stability); [architecture D1–D5](../design/architecture-direction.md#d1-product-boundary-engines-execute).
