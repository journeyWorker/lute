---
title: Typed bridge directives
description: How a plugin declares a typed runtime bridge, the directive that invokes it, the declared result state it writes, sync timing, and the match over its result slot.
---

A **bridge** is a typed runtime call the engine owns — a minigame, a service, an external beat. The DSL emits only data; the engine executes the bridge; and story control-flow observes **only the declared state** a directive writes, never raw bridge output. This is what keeps the language total: a bridge call is data, not an arbitrary tool call.

## Declaring the bridge capability

A `bridge/*.yaml` file names the service and its typed result fields. Those result fields are the *only* values a directive may write back into scene state.

```yaml
bridgeCapabilities:
  - service: minigame
    operation: play
    replay: recorded          # recorded | deterministic | none
    result:
      - { name: score,   type: number }
      - { name: rank,    type: { enum: [fail, bronze, silver, gold] } }
      - { name: cleared, type: bool }
```

## The directive that invokes it

A directive binds to the capability via `bridge: { service, operation }`, opens a typed result slot with a `slotId` attr, and declares the state it `writes` — each write bound with `fromBridgeResult` (or a `{ op: increment }`):

```yaml
directives:
  - name: minigame
    layer: bridge
    attrs:
      - { name: kind,      required: true, type: { enumFromOption: allowedKinds } }
      - { name: id,        required: true, type: { providerRef: minigameId } }
      - { name: resultKey, required: true, type: { slotId: { namespace: scene.minigame } } }
      - { name: sync,      type: bool, default: true }
    semantics: [ "writes.sceneState", "bridgeCall" ]
    state:
      declares:
        - { scope: scene, path: [minigame, { fromAttr: { name: resultKey, slotType: localId } }], shape: minigameResult }
    effects:
      writes:
        - { scope: scene, path: [minigame, { fromAttr: { name: resultKey } }, rank],     value: { fromBridgeResult: rank } }
        - { scope: scene, path: [minigame, { fromAttr: { name: resultKey } }, cleared],  value: { fromBridgeResult: cleared } }
        - { scope: scene, path: [minigame, { fromAttr: { name: resultKey } }, attempts], value: { op: increment, by: 1 } }
    bridge: { service: minigame, operation: play }
    lower:  { kind: builtin, name: bridgeMinigame }
```

Every written path MUST be declared by the slot's shape. A blocking bridge uses a plugin-owned `sync` attribute — **not** the reserved dsl timing key `wait`, which (with `at` / `duration` / `delay`) is reserved across all directives and MUST NOT be redefined by a plugin.

## Worked example: `::minigame` and the match on its result slot

The scene writes nothing by hand. It runs the bridge, then reads the declared slot:

```lute
::minigame{kind="rhythm" id="bianca_service_01" resultKey="service01" sync="true"}

<match on="scene.minigame.service01.rank">
  <when test="$ == 'gold'">
    @bianca{code="0030" emotion="delighted"}: Wonderful! A perfect service!
    ::set{scene.affect.bianca += 2}
  </when>
  <when test="$ in ['silver', 'bronze']">
    @bianca{code="0040" emotion="content"}: Not bad at all, Mr. Fixer.
    ::set{scene.affect.bianca += 1}
  </when>
  <otherwise>
    @bianca{code="0050" emotion="shy"}: Shall we try once more? The rhythm takes practice.
  </otherwise>
</match>
```

`rank` is a finite enum `[fail, bronze, silver, gold]`, so the `<match>` is first-match-wins with `<otherwise>` covering `fail`.

## Sync semantics & the stale-default trap

With `sync="true"` (the default for a blocking bridge) the writes **dominate** the immediately following read — the `<match>` sees the produced outcome. With `sync="false"` the writes do *not* dominate: because result slots are typically shape-defaulted (e.g. `rank: fail`), a following read is **not** `E-MAYBE-UNSET` — it silently reads the **default**, not the outcome. The checker reports this as a distinct *result-read-before-produced* (stale-default) diagnostic. Prefer a blocking bridge whenever story logic branches on the result.
