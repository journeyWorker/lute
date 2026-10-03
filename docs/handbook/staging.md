# Staging

**Semantic id:** `lute.staging/1`.

## Constructs and rules

- **Core directives:** `::bg`, `::music`, `::sfx`, `::vfx` lower to presentation records and execute in command order. Their `wait`, `duration`, and `delay` timing belongs to `lute.staging/1`; timeline placement is `lute.timeline/1`.
- **Character staging:** `::auto{character,...}` and injected sprite records manage stage membership, action, emotion, costume, anchor and deterministic show/reposition/hide lifetime.
- **Media:** `::camera`, `::cut`, and `::video` lower to media records with typed assets. Their blocking and command-level timing belong to `lute.staging/1`.

## Evaluation and lowering

Stage resolution may inject lifetime records after source normalization; the engine executes the final command stream. Timeline directives are scheduled by [timeline](timeline.md), while plugin directives remain bridge records.

## Diagnostics

Closed directive attributes, cast/action/asset domains, media kinds and timeline admission are checked. Unknown attributes, invalid assets, and content writes in presentation-only directives are rejected.

## Example

```lute
::bg{asset="foyer"}
::music{asset="theme", wait=true}
::auto{character="guide", anchor="center"}
```

## History

[DSL 0.33 §1](../proposals/scenario-dsl/0.33.0.md#1-domain-modules-and-evaluation-order), [architecture staging table](../architecture.md#staging).
