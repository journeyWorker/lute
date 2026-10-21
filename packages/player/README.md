# @lute-lang/player

The renderer-free Lute 0.48 visual-novel player core. It drives a compiled Lute bundle through the runtime, turns runtime records into scheduled cues, and provides interaction state, saves, read history, backlog, auto/skip, rollback, and host bridges. Rendering and audio are supplied through ports, so the same core can run behind PixiJS, a native renderer, or a test presenter.

## Install

```sh
bun add @lute-lang/player@0.48.1 @lute-lang/runtime@0.48.1 effect@4.0.2
# npm install @lute-lang/player@0.48.1 @lute-lang/runtime@0.48.1 effect@4.0.2
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

Second play-throughs and locales (0.47):

- **Lineage.** Each time the story presents `ended`, the player stores the runtime snapshot in its profile as the `lineage` (at once, not debounced): the user tier (`user.*`, user-tier facts, `once: user` spends) and the ended run that `newRun` turns into `prev.run.*`. New game at `ended` sends `newRun`. In a player started without `snapshot` / `session`, New game that cannot send `newRun` where the session waits (after a reload, or a run abandoned through the menu) restores the lineage instead of the start point and sends `newRun` — so a second play-through survives a reload; a host `seed` is not used while a lineage exists, and a lineage the runtime refuses is dropped. Older profiles keep their read marks and settings. Leaving the ending (`toTitle` at `ended`) asks no confirm.
- **Locale.** `settings.locale` picks the text: lines, choice labels, branch/hub prompts and the backlog read the record's translation (`lute compile --locale-bundle`) and fall back to source text; `setSettings` applies to the next text. The `Settings` screen lists the bundle's source locale then its compiled locales, and a new profile starts at the source locale. Custom cue factories take the locale per call: `fromOutput(output, readLines, locale)`, `choiceView(await, locale)`.

## Hosting recipes (0.44)

Three ways a host composes the player, each checked against the published types (names such as `bundle`, `startMinigame` or `storeCheckpoint` stand for your own values). The imports they share:

```ts
import { Effect, Layer } from "effect"
import { BrowserKeyValueStore } from "@effect/platform-browser"
import { Cues, Director, LutePlayer, Scheduler, Session, Stores, type SaveRuntime } from "@lute-lang/player"
import { BundleSource, InputLog, Session as RuntimeSession } from "@lute-lang/runtime"
import * as RuntimeBrowser from "@lute-lang/runtime/browser"
```

### Composing `Director.make` yourself

`LutePlayer.make` builds the director for you. A host that composes it by hand passes the compiled `bundle` (required since 0.42: the director builds the default cue factory from it), the player session, stores and scheduler, and — optionally — `host` and a `cueFactory` wrapper:

```ts
const director = Effect.gen(function* () {
  return yield* Director.make({
    bundle,                                    // the compiled bundle value
    bindings, presenter, audio, assets,
    session: yield* Session.PlayerSession,     // Session.layer({ bindings, … }) over the runtime session
    profile: yield* Stores.ProfileStore,       // Stores.profileLayer({ project })
    saves: yield* Stores.SaveStore,            // Stores.layer({ project })
    scheduler: yield* Scheduler.make(),
    host: { plugin: { minigame: (call) => startMinigame(call.attrs) } },
    cueFactory: (base) => ({
      ...base,
      fromOutput: (output, readLines, locale) =>
        base.fromOutput(output, readLines, locale).pipe(Effect.tap((cues) => Effect.log(`${cues.length} cues`)))
    })
  })
})
```

- `host.plugin` reaches the default cue factory; you do not wire it a second time.
- A `cueFactory` **function** wraps that default exactly once (keep one cue per output event, in order).
- A pre-composed factory **value** must come from `Cues.make({ bundle, bindings, plugin })` for `host.plugin` to be merged into it (the host's handler wins a duplicate tag); any other value together with `host.plugin` makes `Director.make` die rather than drop the handlers.

### `LutePlayer.make` over your own runtime session

When the game owns the runtime session (it queries it, raises occasions, keeps its input log), build that session first and hand it to the player. The player needs the **same** `BundleSource` (it reads the artifacts for cues) and a `KeyValueStore` for saves and the profile:

```ts
const bundleSource = BundleSource.value(bundleValue)   // or BundleSource.directory(root) / BundleSource.url(base)
const runtimeSession = RuntimeSession.layer({ seed: { derive: true } }).pipe(   // or { snapshot }
  Layer.provide(Layer.mergeAll(RuntimeBrowser.runtime(wasmUrl), bundleSource, InputLog.memory))
)

const game = Effect.scoped(Effect.gen(function* () {
  const session = yield* RuntimeSession.Session
  const player = yield* LutePlayer.make({ session, bindings, presenter, audio, assets, host })
  yield* player.actions({ type: "newGame" })
  // …
}))

Effect.runFork(game.pipe(Effect.provide(Layer.mergeAll(runtimeSession, bundleSource, Stores.memory))))
```

- `Stores.memory` keeps saves and the profile in memory (tests, previews). For saves that survive a reload use a persistent `KeyValueStore`: `BrowserKeyValueStore.layerLocalStorage` in a browser, or your own implementation on Bun/Node/native hosts. The player writes `save:quick`, `save:auto`, `save:1`… and `profile:<project>`; give it a prefixed store (`KeyValueStore.prefix`) if it shares one with the game.
- `InputLog.memory` keeps the runtime's input log in memory; `InputLog.keyValueStore("game-log")` persists it in the same store.
- Requirements: `make({ session })` needs `Scope`, `BundleSource` and `KeyValueStore`. `make({ seed })` / `make({ snapshot })` build the runtime session themselves and also need `LuteRuntime` and `InputLog`.

### Runtime snapshot versus player save

| | runtime `Contract.Snapshot` | player `SaveRuntime` |
|---|---|---|
| Holds | runtime state: `await`, `continuation?`, `project`, `request`, `snapshotVersion`, `world` (state, facts, stage, clock) | `{ snapshot, log, backlog }`: that runtime snapshot plus the player's accepted-input log and backlog |
| Produced by | runtime `Session.save` | the player handle's `snapshot` (`DirectorHandle.snapshot`) |
| Consumed by | runtime `Session.restore`; `LutePlayer.make({ snapshot })`; `RuntimeSession.layer({ snapshot })` | the handle's `restore(save)`; save slots |
| Restores | the story where it was | the story and the player's log and backlog; read marks added after the checkpoint are removed, rollback history is cleared, the `auto` slot is rewritten, the stage is rebuilt, its music is begun without waiting for it to start, and the await is presented again |

Checkpoint and restore a running player with the handle:

```ts
const save: SaveRuntime = yield* player.snapshot   // linearized with send/restore
yield* storeCheckpoint(save)
// … later, same player:
yield* player.restore(save)
```

To resume in a **fresh** player (a new page, a new process), start its runtime from the save's runtime snapshot, then restore the save to bring back the log and backlog. `LutePlayer.make` returns once the player has left `boot`, so `restore` can follow at once:

```ts
const resume = Effect.scoped(Effect.gen(function* () {
  const save = yield* loadCheckpoint
  const player = yield* LutePlayer.make({ snapshot: save.snapshot, bindings, presenter, audio, assets })
  yield* player.restore(save)
}))
```

With your own runtime session, build it with `RuntimeSession.layer({ snapshot: save.snapshot })` and pass it as `session`. The save must come from the same compiled bundle (the project fingerprint is checked); otherwise `restore` fails `SaveIncompatible`.

## API and guides

- [Core architecture](https://github.com/journeyWorker/lute/blob/v0.48.1/docs/player/architecture.md)
- [Bindings reference](https://github.com/journeyWorker/lute/blob/v0.48.1/docs/player/bindings.md)
- [Player overview](https://lute-lang.vercel.app/player/overview/)
- [Custom hosts](https://lute-lang.vercel.app/player/custom-hosts/)
- [Runtime event contract](https://github.com/journeyWorker/lute/blob/v0.48.1/docs/runtime/event-contract.md)

Source and releases: [github.com/journeyWorker/lute](https://github.com/journeyWorker/lute).
