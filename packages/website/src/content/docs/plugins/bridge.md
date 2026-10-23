---
title: Typed bridge directives
description: How a plugin declares a typed runtime bridge, the directive that invokes it, the declared result state it writes, sync semantics, and the match over its result slot.
---

A **bridge** is a typed runtime call the engine owns — a minigame, a service, an external beat. The DSL emits only data; the engine executes the bridge; and story control-flow observes **only the declared state** a directive writes, never raw bridge output. This is what keeps the language total: a bridge call is data, not an arbitrary tool call.
A passthrough plugin directive that has no declarative `lower: { record, fields }` or named core
builtin lowering emits a generic `kind: "plugin"` record (`family: "plugin"`). It may carry an
optional `plugin` owner id resolved from capability assembly (never authored in the scene),
alongside its `tag`, typed `fields`, and any declared `effects`:

```json
{
  "kind": "plugin",
  "family": "plugin",
  "position": "001-0400",
  "tag": "host-panel",
  "plugin": "game.presentation",
  "fields": {
    "assetId": "CG.chapter_one.first_choice",
    "resultKey": "first-choice",
    "text": "First choice"
  }
}
```

Hosts dispatch these extension records by the pair **`(plugin, tag)`**. `plugin` is omitted for
core or unresolved ownership. Presentation directives that declare `lower: { record: ... }` still
lower to their stable core staging record kinds; the owner field is specific to passthrough plugin
records.


## Declaring the bridge capability

A `bridge/*.yaml` file names the service and its typed result fields. Those result fields are the *only* values a directive may write back into scene state.

```yaml
bridgeCapabilities:
  - service: minigame
    operation: play
    replay: recorded          # recorded | deterministic | none
    result:
      - { name: score,   type: int }
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
```

Every written path MUST be declared by the slot's shape. A host may keep its own option such as `sync` — a plugin never redeclares the reserved dsl timing key `wait`, which is what makes a call blocking or not (see [Sync semantics](#sync-semantics)). `at`, `duration`, `delay`, `wait`, `timeline`, `timing`, `provenance` and `source` are the eight names the core stamp owns; a plugin declaring an attribute under any of them is rejected at assembly with [`E-PLUGIN-RESERVED-STAMP-ATTR`](/plugins/manifests/).

A call blocks — waits for the host's answer — because an `effects.writes` entry reads a bridge result. A directive that declares `layer: bridge` and `bridgeCall` but has no `fromBridgeResult` write would compile to a record nothing waits for, so since 0.41.1 it is [`E-BRIDGE-NO-RESULT-WRITE`](/reference/diagnostics/#e-bridge-no-result-write) at its declaration, and the project does not compile until you add the write or remove `layer: bridge`.

## Worked example: `::minigame` and the match over its result slot

The scene writes nothing by hand. It runs the bridge, then reads the declared slot:

```lute
::minigame{kind="rhythm" id="marina_service_01" resultKey="service01" sync="true"}

<match subject="scene.minigame.service01.rank">
  <when is="gold">
    @marina{code="0030" emotion="delighted"}: Wonderful! A perfect service!
    ::set{scene.affect.marina += 2}
  </when>
  <when is="silver|bronze">
    @marina{code="0040" emotion="content"}: Not bad at all, Mr. Fixer.
    ::set{scene.affect.marina += 1}
  </when>
  <otherwise>
    @marina{code="0050" emotion="shy"}: Shall we try once more? The rhythm takes practice.
  </otherwise>
</match>
```

`rank` is a finite enum `[fail, bronze, silver, gold]`, so the `<match>` is first-match-wins with `<otherwise>` covering `fail`.

## Sync semantics

A bridge call that writes a result blocks, and its result writes are a **definite assignment after the call** (since 0.49). A *blocking bridge call* is a plugin directive call whose declaration has at least one `effects.writes` entry valued `fromBridgeResult` and whose effective core `wait` is not `false`. The checker treats each resolved result path of an unguarded blocking call as assigned from the call on, so the `<match>` above reads `scene.minigame.service01.rank` with no `E-MAYBE-UNSET`, and removing an arm from it reports `E-NONEXHAUSTIVE`. Under a `when=` guard the writes belong only to the guarded fork.

`wait` is the language's blocking key. With **`wait="false"`** the call is fire-and-continue, the writes are *not* assumed, and a following read of the result keeps `E-MAYBE-UNSET`.

A host option such as the `sync` attribute above is the plugin's own: **Lute does not read `sync`.** A directive whose host option means "fire and continue" (`sync="false"`) MUST also be written `wait="false"`, or the checker will treat its result as assigned while the host does not wait for it:

```lute
::minigame{kind="rhythm" id="marina_service_01" resultKey="service01" sync="false" wait="false"}
```

Use a blocking bridge whenever story logic branches on the result, and reserve `wait="false"` for a bridge whose outcome nothing in the same scene reads.

## What the host receives

A plugin record, a bridge request and an `awaitBridge` carry the call's `attrs` (its `fields`) and, when the directive was stamped with a cross-cutting [`stampAttrs`](/plugins/manifests/#cross-cutting-attributes-stampattrs) value, a separate `stamps` object. It is omitted when empty and never merged into `attrs`.
