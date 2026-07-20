# Bridge protocol

A **bridge** is a typed call from the narrative into a host-provided service —
a minigame, a save operation, a store transaction, anything the engine
implements natively. Lute never performs the call (design decision D1); it
compiles a plugin directive that *references* a bridge into a `plugin` command
carrying the fully-resolved call and its result bindings. The engine makes the
call and applies the effects.

Grounding: `ir.rs::{OtherCmd, Effect, EffectSource}` (the compiled form),
`crates/lute-compile/src/lower.rs::resolve_effect` (the resolution), and
`crates/lute-manifest/src/schema.rs::{DirectiveDecl, BridgeRef,
BridgeCapability, DirectiveEffects, WriteDecl, WriteValue}` (the plugin
declaration).

## The compiled form

A plugin directive with a bridge lowers to a `Command::Other`, serialized as
`kind: "plugin"`:

```json
{
  "kind": "plugin",
  "addr": "001-0700",
  "tag": "minigame",
  "fields": { "id": "bianca_service_01", "kind": "rhythm",
              "resultKey": "service01", "sync": true },
  "effects": [
    { "path": "scene.minigame.service01.score",   "from": { "bridgeResult": "score" } },
    { "path": "scene.minigame.service01.rank",    "from": { "bridgeResult": "rank" } },
    { "path": "scene.minigame.service01.cleared", "from": { "bridgeResult": "cleared" } }
  ]
}
```

- `tag` — the authored plugin directive tag (`OtherCmd.tag`).
- `fields` — the resolved directive attrs, typed via the manifest `AttrDecl`s
  (`OtherCmd.fields`, a string→JSON map). This is where a `wait` attr lands (see
  below).
- `effects` — the resolved state-write bindings (`OtherCmd.effects`); **absent**
  when the directive declares none.

The **which** service/operation this `tag` invokes is a property of the plugin
manifest the artifact was compiled against, not of the record itself: a
directive declares `bridge: { service, operation }` (`BridgeRef`), and the
engine's plugin implementation is keyed on `(service, operation)`
(`BridgeCapability`). The `capabilityVersion` envelope stamp pins the snapshot
so the engine can refuse a mismatched plugin set.

## Typed calls and returns

A `BridgeCapability` (manifest) declares:

- `service`, `operation` — the call identity, unique across installed plugins;
- `result: Field[]` — the **typed return shape**: the named, typed fields the
  bridge's result object carries;
- `replay` — an optional replay policy string.

So a bridge call is typed on both sides: the directive's `fields` are the typed
arguments, and `result` is the typed return. The engine implements the
operation, receives the arguments in `fields`, and returns a result object whose
keys match the declared `result` fields.

### Blocking — `wait`

Whether the engine **blocks** on the call is carried by the directive's `wait`
attr in `fields` (default `true` in the manifest attr decl). `wait: true`
means the walk suspends until the bridge returns and its effects are applied;
`wait: false` means fire-and-continue (the effects apply when the result
arrives). The exact scheduling of a non-blocking bridge is engine policy; the
DSL only records the author's intent.

## State effects

After the call returns, the engine applies each `Effect` in order — a resolved
binding of *where a value lands* to *where it comes from* (IR A12). The
`fromAttr` path templates were already substituted at compile time (e.g.
`resultKey="service01"` produced `scene.minigame.service01.*`), so the runtime
needs **no manifest lookup and no per-plugin knowledge** to apply them.

`Effect.from` is one of (`EffectSource`, untagged):

| shape | meaning |
| ----- | ------- |
| `{ "bridgeResult": "<key>" }` | read the named key off the bridge's returned result object and write it to `Effect.path`. |
| `{ "op": "<op>", "by": <number>}` | a state mutation op (e.g. `"increment"`) applied to `Effect.path` — no bridge value read. |
| `<literal>` (bare bool / number / string) | write this literal value to `Effect.path`. |

`Effect.path` is a fully-resolved dotted state path (scope + segments), so it
lands in one of the state tiers described in
[state-lifecycle.md](./state-lifecycle.md). A bridge that declares no effects
(`effects` absent) is a pure call with no state landing site.

## Contract summary

1. Match the record's `tag` (via its manifest `bridge` ref) to your
   `(service, operation)` implementation; refuse if `capabilityVersion` does
   not match your plugin snapshot.
2. Invoke the operation with the typed `fields` as arguments; if `wait` is
   true, suspend the walk until it returns.
3. On return, apply each `Effect`: `bridgeResult` reads a key off the result,
   `op`/`by` mutates, a literal writes a constant — each to its resolved
   `path`.
4. Ignore unknown fields on the record (forward compatibility, per the
   [execution model](./execution-model.md) version policy).
