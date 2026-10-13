# @lute-lang/player

The renderer-free Lute 0.41 visual-novel player core. It drives a compiled Lute bundle through the runtime, turns runtime records into scheduled cues, and provides interaction state, saves, read history, backlog, auto/skip, rollback, and host bridges. Rendering and audio are supplied through ports, so the same core can run behind PixiJS, a native renderer, or a test presenter.

## Install

```sh
bun add @lute-lang/player@0.41.1 @lute-lang/runtime@0.41.1 effect@4.0.2
# npm install @lute-lang/player@0.41.1 @lute-lang/runtime@0.41.1 effect@4.0.2
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

## API and guides

- [Core architecture](https://github.com/journeyWorker/lute/blob/v0.41.1/docs/player/architecture.md)
- [Bindings reference](https://github.com/journeyWorker/lute/blob/v0.41.1/docs/player/bindings.md)
- [Player overview](https://lute-lang.vercel.app/player/overview/)
- [Custom hosts](https://lute-lang.vercel.app/player/custom-hosts/)
- [Runtime event contract](https://github.com/journeyWorker/lute/blob/v0.41.1/docs/runtime/event-contract.md)

Source and releases: [github.com/journeyWorker/lute](https://github.com/journeyWorker/lute).
