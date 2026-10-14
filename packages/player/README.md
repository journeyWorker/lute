# @lute-lang/player

The renderer-free Lute 0.43 visual-novel player core. It drives a compiled Lute bundle through the runtime, turns runtime records into scheduled cues, and provides interaction state, saves, read history, backlog, auto/skip, rollback, and host bridges. Rendering and audio are supplied through ports, so the same core can run behind PixiJS, a native renderer, or a test presenter.

## Install

```sh
bun add @lute-lang/player@0.43.0 @lute-lang/runtime@0.43.0 effect@4.0.2
# npm install @lute-lang/player@0.43.0 @lute-lang/runtime@0.43.0 effect@4.0.2
```

`@lute-lang/runtime` (the same release version) and `effect@4.0.2` are **peer dependencies**: the host installs them once, so the player, the runtime and the host share one `effect` and one runtime. For unreleased local builds, depend on packed tarballs (`scripts/pack-local.sh`), not `file:` directory links — see "Local dogfooding" in `docs/runtime/typescript-runtime.md`.

## Minimal usage

Provide a decoded `Bindings` value (or the text of `player.bindings.yaml`), a `Presenter`, `AudioPort`, `AssetLoader`, and the runtime requirements (`@lute-lang/runtime/bun` or `/browser`, a bundle source, an input log, and a key-value store):

```ts
import { Effect } from "effect"
import { LutePlayer } from "@lute-lang/player"

const program = LutePlayer.make({
  bindings: bindingsYaml,
  presenter,
  audio,
  assets,
  host: {
    bridge: { "my-plugin": (request) => answerPlugin(request).pipe(Effect.map((fields) => ({ fields }))) },
    plugin: { well: (call) => game.well(call.attrs) }
  },
  seed: { derive: true }          // or `snapshot`, or the host's runtime `session`
})

// Run `program` in a scope with the runtime, bundle, input-log, and store layers.
const handle = await Effect.runPromise(Effect.scoped(program.pipe(Effect.provide(requirements))))
await Effect.runPromise(handle.actions({ type: "newGame" }))
```

The handle:

- `view` is a live `SubscriptionRef<PlayerView>`; `actions` accepts `PlayerAction`; `shutdown` closes the player.
- `send(input)` hands the runtime an input at `idle` or `ended` (`DirectorBusy` elsewhere); it completes when the input's batch started.
- `restore(save)` continues from a host checkpoint (`PlayerSession.snapshot`): the stage, backlog, read history and `auto` slot follow the save and its await is presented again.
- `session` and `program` are the runtime session in use and its program, for `current`, `events` and queries (`view`, `evaluate`, `def`, `gate`, `candidates`).

Options beyond the ports: `host` (`bridge` answers `{ fields, writes? }`, `plugin` runs a game action at a `plugin` record's place, `idle` overrides the idle flow — each on a fiber of its own), `linePacing: "presenter"` (the presenter types, pages and takes the confirm), `autosave` (`false`, `"presentation"`, `"choice"`; default both). A `Presenter` may set `confirmsLines: true` to have an `advance` that arrives while a line types kept as its release.

## Hosting recipes

### Compose `Director.make` directly

`Director.make` requires `bundle`, `session`, `bindings`, `presenter`, `audio`, `assets`, `profile`, `saves`, and `scheduler` (plus the host's scope). Its default cue factory receives `host.plugin`:

```ts
const director = Director.make({
  bundle, session, bindings, presenter, audio, assets, profile, saves, scheduler,
  host: { plugin: { sparkle: call => sparkle(call.attrs) } },
  cueFactory: (base) => ({ ...base, fromOutput: output => myOutput(base.fromOutput(output)) })
})
```

`host.plugin` is merged into the default factory, and a wrapper receives that factory. If supplying a pre-composed factory, create it with `Cues.make({ bundle, bindings })`; an arbitrary factory cannot merge `host.plugin` and `make` dies.

### External runtime session

Build the runtime session yourself, then pass the same bundle source and a key-value store to the player:

```ts
const source = BundleSource.value(bundle) // or BundleSource.directory(root) / BundleSource.url(base)
const session = Session.layer({ seed }).pipe(
  Layer.provide(Layer.mergeAll(LuteRuntime.layer, source, InputLog.memory))
)
const handle = LutePlayer.make({
  session, bindings, presenter, audio, assets, host, cueFactory
}).pipe(Effect.provide(Layer.mergeAll(source, Stores.memory)))
```

Use `InputLog.keyValueStore("game-log")` instead of `InputLog.memory` for persistence, and use `BrowserKeyValueStore.layerLocalStorage` in browsers or a host-supplied `KeyValueStore` elsewhere. Both `make` overloads require `Scope`; the `session` overload additionally requires `BundleSource` and `KeyValueStore`, while seed/snapshot construction may also receive `LuteRuntime` and `InputLog`.

### Runtime snapshots and player saves

`Contract.Snapshot` is produced and consumed by the runtime and restores runtime state only. `SaveRuntime` is produced by `handle.snapshot`, consumed by `handle.restore`, and restores the runtime snapshot plus player log and backlog. On a running player: `const save = await handle.snapshot; store(save); await handle.restore(save)`. For a fresh player use `LutePlayer.make({ snapshot: save.snapshot, ... })`, leave `boot` by running `actions({ type: "newGame" })`, then call `restore(save)`. The bundle fingerprint must match or `SaveIncompatible` is raised.

## API and guides

- [Core architecture](https://github.com/journeyWorker/lute/blob/v0.43.0/docs/player/architecture.md)
- [Bindings reference](https://github.com/journeyWorker/lute/blob/v0.43.0/docs/player/bindings.md)
- [Player overview](https://lute-lang.vercel.app/player/overview/)
- [Custom hosts](https://lute-lang.vercel.app/player/custom-hosts/)
- [Runtime event contract](https://github.com/journeyWorker/lute/blob/v0.43.0/docs/runtime/event-contract.md)

Source and releases: [github.com/journeyWorker/lute](https://github.com/journeyWorker/lute).
