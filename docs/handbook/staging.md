# Staging

**Semantic id:** `lute.staging/1`.

## Constructs and rules

- **Staging vocabulary:** staging members come from project-declared closed domains in document/schema `enums:` (short list or long form with `members`, `default`, `exits`, `labels`). Reserved staging domains are `anchor`, `action`, `emotion`, `framing`, `cameraMove`, `transition`, `cgLayout`, `musicPlayback`, `costume`, `sequence`, and `textStyle`. A non-member is `E-BAD-ENUM`; an undeclared domain is `E-DOMAIN-UNKNOWN`. The directives themselves are core; only their members are project vocabulary.
- **Scene directives:** `::bg{location?, time?, assetId?}`, `::music{playback?, mood?, volume?, assetId?}`, `::sfx{sound?, assetId?}` and `::vfx{type, label?, transition?}` lower to `bg`, `music`, `sfx` and `vfx` records and execute in command order.
- **Actors:** `::actor{character, anchor?, action?, emotion?, costume?}` and compiler-injected actor records manage stage membership, action, emotion, costume, anchor and deterministic show/reposition/hide lifetime. An omitted anchor reads the `anchor` domain default; an action listed in `action.exits` ends presence and lowers with `exit: true`.
- **Camera:** `::camera` takes at least one of `focus` (cast reference), `framing` (`framing`), `move` (`cameraMove`) or `transition` (`transition`); none is `E-CAMERA-EMPTY`. Values are opaque domain members; no numeric transform is synthesized and camera state is not persisted.
- **Media:** `::cg{assetId, display?, layout?}` and `::video{assetId, display?}` show or hide media; `display` is `show|hide` and resolves to `show`. `layout` is a `cgLayout` member. `::sequence{name}` references a declared `sequence` member and blocks by default (`wait` defaults to `true`); it is a reference to an external cinematic, not an inline one.
- **Timing:** every directive admits `wait`, `duration` and `delay`; command-level timing belongs to `lute.staging/1` and lowers into the record's `timing` object. Timeline placement is `lute.timeline/1`.

## Evaluation and lowering

Domains merge before lowering. Stage resolution may inject actor lifetime records after source normalization; the engine executes the final command stream. Every staging record carries `family: "staging"`. Timeline directives are scheduled by [timeline](timeline.md), while plugin directives remain bridge records.

## Diagnostics

Closed directive attributes, required attributes (`E-MISSING-ATTR`), cast/domain members, media kinds and timeline admission are checked. Unknown attributes, invalid assets, and content writes in presentation-only directives are rejected.

## Example

```lute check
---
kind: scene
id: foyer
title: Foyer
enums:
  anchor: {members: [default, center], default: default}
  action: {members: [enter, leave], exits: [leave]}
  framing: [closeUp, wide]
  transition: [fade]
  cgLayout: [full]
  sequence: [doorOpens]
---
## Arrival {#arrival}
::bg{location="foyer"}
::music{assetId="theme", wait=true}
::actor{character="guide", anchor="center", action="enter"}
::camera{focus="guide", framing="closeUp", transition="fade"}
@guide{code="hi"}: Welcome.
::cg{assetId="door", layout="full"}
::sequence{name="doorOpens"}
::actor{character="guide", action="leave"}
```

The sequence and the final actor lower to:

```json
[
  {"kind": "sequence", "family": "staging", "position": "001-0700", "name": "doorOpens", "timing": {"wait": true}},
  {"kind": "actor", "family": "staging", "position": "001-0800", "character": "guide", "action": "leave", "exit": true}
]
```

## History

[DSL 0.37 §3.2–3.3](../proposals/scenario-dsl/0.37.0.md#32-project-declared-staging-vocabulary), [DSL 0.33 §1](../proposals/scenario-dsl/0.33.0.md#1-domain-modules-and-evaluation-order), [architecture staging table](../architecture.md#staging).
