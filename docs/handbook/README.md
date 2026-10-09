# Lute current-rules handbook

This is the normative, current-rules view of Lute as of 0.38.0. It includes
the phase-2 domain-module and semantic-negotiation contract; the 0.32
execution-IR rules remain the base contract and are linked where relevant.
The 0.34 project-model, evidence, impact, and constraints documentation remains
part of the historical stack; the 0.35 AI edit-loop contract is current.
This handbook is intentionally not a release history. The versioned
[scenario DSL proposals](../proposals/scenario-dsl/) explain why and when a
rule was introduced; each page links only to proposal files present in this
repository under **History**. The [runtime documentation](../runtime/) is the
detailed engine contract and host obligations.

## Module pages

- [Core](core.md) — baseline declarations, state, CEL, IR and identity.
- [Narrative](narrative.md) — scenes, lines, choices, hubs, matches and components.
- [Staging](staging.md) — staging vocabulary, actors, camera, media, sequences and stage lifetime.
- [Timeline](timeline.md) — tracks, clips, timing and barriers.
- [Quest](quest.md) — lifecycle, objectives, accepts, deadlines, handlers and rewards.
- [Time](time.md) — clock, cadence, seasons and resets.
- [Occasions](occasions.md) — gates, terminal state, eligibility and selection.
- [Knowledge](knowledge.md) — vocabulary, facts, temporal queries and Datalog rules.
- [Lore](lore.md) — entries, disclosure and read state.

## Shared rules

Lute source plus project declarations are authoritative. Compilation lowers checked source to the **execution IR**; engines interpret that IR and must refuse an artifact before playback when its `requiredSemantics` are unsupported. Authors cannot override required semantics. Every command record carries `kind`, `family` (`content`, `staging`, `state`, `control`, `declaration` or `plugin`) and `position`; `position` is build-local, while declared ids, `meta.id`, `lineId` and `voiceKey` are the stable joins described by [execution addressing](../runtime/execution-model.md#addressing).

Conditions use the closed standard-CEL profile in [DSL 0.32 §1](../proposals/scenario-dsl/0.32.0.md#1-conditions-are-standard-cel). A condition error is not satisfied; an erroneous `::set` halts without a partial write. State owned by the engine is readable but not content-writable.

## Genre guidance

Genre guidance is advice for authors, not an engine obligation. A project may
recommend that only significant actions consume time (the 0.31 convention),
but a checker must not turn that recommendation into a runtime guarantee.

## Execution contract

The host advances the clock according to declared `advance`/`advances`
operations and settles at the documented points. A checker may report schedule
consequences, but the host owns time advancement.
