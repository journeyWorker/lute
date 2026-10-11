# @lute-lang/player

The renderer-free Lute 0.40 visual-novel player core. It drives a compiled Lute bundle through the runtime, turns runtime records into scheduled cues, and provides interaction state, saves, read history, backlog, auto/skip, rollback, and host bridges. Rendering and audio are supplied through ports, so the same core can run behind PixiJS, a native renderer, or a test presenter.

## Install

```sh
bun add @lute-lang/player@0.40.0 @lute-lang/runtime@0.40.0 effect@4.0.2
# npm install @lute-lang/player@0.40.0 @lute-lang/runtime@0.40.0 effect@4.0.2
```

`effect@4.0.2` is a required peer dependency. The runtime and player package versions are released in lockstep.

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
  host: { bridge: { "my-plugin": (request) => answerPlugin(request) } },
  seed: { derive: true }
})

// Run `program` in a scope with the runtime, bundle, input-log, and store layers.
const handle = await Effect.runPromise(Effect.scoped(program.pipe(Effect.provide(requirements))))
await Effect.runPromise(handle.actions({ type: "newGame" }))
```

`handle.view` is a live `SubscriptionRef<PlayerView>`, `handle.actions` accepts `PlayerAction`, and `handle.shutdown` closes the player. A host implements `Presenter`, `AudioPort`, and `AssetLoader`; `PlayerHost` optionally answers bridge awaits and controls the idle flow.

## API and guides

- [Core architecture](../../docs/player/architecture.md)
- [Bindings reference](../../docs/player/bindings.md)
- [Player overview](/player/overview/)
- [Custom hosts](/player/custom-hosts/)
- [Runtime event contract](../../docs/runtime/event-contract.md)

Source and releases: [github.com/journeyWorker/lute](https://github.com/journeyWorker/lute).
