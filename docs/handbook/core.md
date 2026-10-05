# Core

**Semantic ids:** `lute.core/1` (baseline), plus the CEL and state rules below.

## Constructs and rules

- **Project/document declarations.** Frontmatter (`kind`, `id`, project/schema declarations) resolves before document checking. `meta.id` is the canonical document key; declarations lower into execution-IR envelopes and command arrays. Diagnostics include unknown kinds, duplicate ids, unresolved references and admission errors.
- **State tiers and ownership.** `scene`, `run`, `user`, `app`, `season:<name>` and reserved engine paths have declared types, defaults and reset scopes. Content may write only content-owned paths; engine-owned paths are readable and engine-settled. Invalid types, defaults, or ownership writes are diagnostics (`E-STATE-DECL`, `E-RUN-OWNED-WRITE`).
- **CEL conditions.** Every condition/value slot is standard CEL in the closed 0.32 profile; `@def` and match `$` expand at compile time. `has()` tests presence. Errors make guards false; erroneous sets abort without a partial write. See [DSL §1–4](../proposals/scenario-dsl/0.32.0.md#1-conditions-are-standard-cel).
- **Execution IR and capabilities.** The compiler emits `cel` plus typed `expr`, `celEnv`, and sorted `requiredSemantics`; engines check capabilities before playback. Unknown required ids refuse loading, while plugin records remain `kind: "plugin"` and use `capabilityVersion`.
- **Identity and addressing.** `addr` is positional; author ids, `lineId`, and `voiceKey` are stable joins. Source maps survive lowering. Duplicate or colliding ids are checker errors.
## Identity table (0.36.1)

`addr` is a build-local position and never a save, localization, patch, or
identity key. Stability columns are **sibling insert / file move / parent
rename / format**. `Y` means stable when the authored source remains; `N`
means it changes; `conditional` names the prerequisite. “Persisted” includes
engine saves and durable localization/voice joins.

| Node kind | Identity source | Stability (insert / move / rename / fmt) | Persisted |
|---|---|---|---|
| project/root | canonical project root | Y / N (revision) / N / Y | no |
| document | `meta.id` | Y / Y / Y if id remains / Y | document joins |
| scene | authored scene/document id | Y / Y / Y if id remains / Y | visited scene id |
| quest | authored quest `id` | Y / Y / Y if id remains / Y | `quest.<id>.*`, instances |
| objective | authored objective `id` under quest | Y / Y / N with parent / Y | completion/failure |
| reward | authored reward id, else warning-bearing owner ordinal | Y with id, else N / conditional / N with owner / Y | owner reward history |
| lore entry | authored entry `id` | Y / Y / Y if id remains / Y | `entry.<id>.read` |
| beat/occasion beat | authored beat id, document-qualified | Y / Y / N with document / Y | visited/once/share |
| shot | authored id, else warning-bearing heading fallback | Y with id / conditional / N with fallback / Y | no direct save key |
| directive | authored tag/id or warning-bearing owner ordinal | Y with id, else N / conditional / N / Y | plugin effects may persist |
| set (`::set`) | authored target path + operation | Y / Y / Y unless derived / Y | target state path |
| match (`<match>`) | authored match id or warning-bearing owner identity | Y with id, else N / conditional / N / Y | no direct save key |
| `on` wrapper | authored event qualified by owner | Y / Y / N with owner / Y | indirect event effects |
| assert | canonical relation + ground terms, owner-qualified | Y / Y / N with owner / Y | base fact |
| retract | canonical relation + ground terms, owner-qualified | Y / Y / N with owner / Y | base fact state |
| expanded graph node | source identity + complete component scope | Y if source/scope remains / conditional / N with host / Y | no direct save key |
| fact graph node | canonical relation + canonical terms | Y / Y / Y unless renamed / Y | base facts; derived recompute |
| clock graph node | authored clock declaration key | Y / Y / Y if key remains / Y | engine clock position |
| engine graph node | engine-owned contract key | conditional / conditional / conditional / Y | engine-owned state |
| branch | authored branch `id` | Y / conditional / N with parent / Y | choice state |
| choice/option | authored choice `id`, parent-qualified | Y / conditional / N with parent / Y | once/visited |
| hub | authored hub `id` | Y / conditional / N with parent / Y | option visited/once |
| mark | authored mark label | N before target / conditional / conditional / Y | no |
| line | speaker `code`, tagged by `lute tag` | Y / Y if scope remains / N with scope / Y | lineId/voiceKey joins |
| component definition | authored component name | Y / conditional / N on rename / Y | expanded joins |
| component instance (`::use`) | `instance` token; fallback warns | Y / Y if host remains / N with host / Y | expanded joins |
| template parameter | authored `params` member | Y / conditional / N on rename / Y | no direct save key |
| match arm | authored id, else warning-bearing owner ordinal | Y with id, else N / conditional / N / Y | no |
| timeline/track/clip | authored id, else warning-bearing owner ordinal | Y with id, else N / conditional / N / Y | no |
| state path | authored path | Y / Y / Y unless derived / Y | exact state path |
| relation | authored relation name + arity | Y / Y / Y unless derived / Y | base relation values |
| rule source | authored rule body/name or canonical form | Y / Y / Y unless derived / Y | derived closure recomputes |
| def | authored `defs` member | Y / conditional / Y unless derived / Y | no direct save key |
| enum/entity member | authored schema member | Y / conditional / N on rename / Y | values may persist |
| occasion/event | authored contract name | Y / Y / Y / Y | indirect effects |
| chapter member | authored manifest id/references | Y / Y / N on ref rename / Y | referenced state |
| plugin directive | authored tag and external operation key | Y / Y / Y unless renamed / Y | indirect effects |

Component invocation keys use `instance="token"` with
`[A-Za-z][A-Za-z0-9_-]{0,63}` and produce the scope segment
`{component}#{instance}`. The key is unique for the same component in its
immediate owner; nested scopes are qualified by the complete enclosing path.
Missing instance keys and missing line codes are warning-bearing positional
fallbacks until `lute tag` writes them. Normal tagging never changes an
existing key; `lute tag --force` retags line codes only and remains guarded by
`codesLocked`. Published parent/document renames use the
`identity.renames` ledger described in [DSL 0.36 §3](../proposals/scenario-dsl/0.36.0.md#3-rename-ledger-and-save-migration).

### Stable identity policy and migration

Projects that publish saves, localization, or voice joins SHOULD set
`identity.requireStable: true`. With that opt-in, every untagged component
invocation and uncoded line is diagnosed (`W-COMPONENT-INSTANCE-UNTAGGED` and
`W-LINE-CODE-UNTAGGED`); standalone files and snippets do not emit these two
warnings. Run `lute tag` to back-fill both axes. It preserves authored values,
allocates component keys as `use-001`, `use-002`, … per owner/component, writes
only changed files, and is idempotent. `lute tag --force` still retags line
codes only and never rewrites an explicit `instance`.

Published renames are explicit migration data:

```yaml
identity:
  requireStable: true
  renames:
    "quest:oldName": "quest:newName"
```

The ledger is resolved to canonical `NodeKey` strings, sorted by `from`, and
rejects stale sources, missing destinations, duplicate endpoints, chains, and
cycles. It is carried as `identityRenames` in the execution IR and project
index. Semantic diff reports a `renamed` change when the ledger matches; an
unmatched identity is reported as `unmappedIdentity`, never inferred from
position or `addr`.

```lute check
---
kind: scene
id: identity-demo
---
## Opening
@narrator{code="intro"}: The fire is ready.
```

The component key must match `[A-Za-z][A-Za-z0-9_-]{0,63}` and is unique for
the same component in its immediate owner.

### Identity history

[DSL 0.36 §1](../proposals/scenario-dsl/0.36.0.md#1-identity-contract), [§2](../proposals/scenario-dsl/0.36.0.md#2-stable-component-instance-keys), [§3](../proposals/scenario-dsl/0.36.0.md#3-rename-ledger-and-save-migration); [architecture D8](../design/architecture-direction.md#d8-identity).

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
