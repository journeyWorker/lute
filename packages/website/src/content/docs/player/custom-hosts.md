---
title: Custom player hosts
description: Build a native or test host for the renderer-free Lute player core with Presenter, AudioPort, AssetLoader, and PlayerHost.
---

Use `@lute-lang/player` directly when PixiJS is not the right renderer. The core has four seams: `Presenter` renders cues and screens, `AudioPort` plays bound sounds, `AssetLoader` loads bound files, and `PlayerHost` handles host-specific bridges and idle policy.

## Compose the core

```ts
import { Effect, Layer } from "effect"
import { LutePlayer } from "@lute-lang/player"
import * as RuntimeBun from "@lute-lang/runtime/bun"

const player = LutePlayer.make({
  bindings: bindingsYaml,
  presenter: myPresenter,
  audio: myAudio,
  assets: myAssets,
  host: {
    bridge: { "save-score": (request) => sendToNativeHost(request) },
    idle: (await_) => Effect.succeed(undefined)
  }
})

const requirements = Layer.mergeAll(
  RuntimeBun.runtime,
  RuntimeBun.bundle("dist/bundle"),
  RuntimeBun.inputLog("my-game"),
  myKeyValueStore
)
```

`LutePlayerOptions` requires `bindings`, `presenter`, `audio`, and `assets`; it accepts optional `host` and `seed`. Bindings can be decoded data or YAML text. Provide runtime, bundle, input-log, and key-value-store services before running `LutePlayer.make` in a scope.

## Port contracts

`Presenter` implements `stage(cue, mode)`, `showLine(line, mode)`, `revealLine`, `clearLine`, `restore(stage)`, `showChoice(menu, remaining)`, `hideChoice`, `showScreen(screen)`, `hideScreen`, `setUiHidden`, `applySettings`, `capture`, and an `actions` stream. A staging or line effect completes when the visual effect or typewriter completes. `mode: "skip"` must snap to the end state. `restore` is used on load, rollback, and presentation start; restore the runtime's stage directly rather than replaying old records.

`AudioPort` implements `play(channel, sound)`, `stop(channel, fade)`, and `setVolumes`. Channels are `music`, `sfx`, and `voice`. `AudioPlay.file` is already resolved from bindings; `loop`, `volume` (0–1), and `fade` are explicit. `AssetLoader.ensure(files)` keeps files loaded for the open scope and reports progress through its stream; missing files fail with `AssetMissing`.

The public `PlayerAction` union is the device-independent input surface: advance, auto/skip, rollback, backlog/menu/save/load/settings, quick save/load, UI visibility, choice, idle, voice replay, and settings actions. A native host can emit these actions from any device.

## Bridges

A bridge call suspends the runtime. Add a handler keyed by the bridge tag:

```ts
const host = {
  bridge: {
    "battle-result": (request) =>
      Effect.tryPromise(() => nativeBattle(request.fields)).pipe(
        Effect.map((fields) => ({ won: fields.won, score: fields.score }))
      )
  }
}
```

The handler receives the typed `BridgeAwait` (`request`, `tag`, `document`, `position`, and field declarations) and returns an Effect of a record. The director sends the result back as `bridgeResult`, then continues cue playback. If no handler exists, the player emits a bridge-unhandled failure rather than inventing an answer.

## Idle flow

`flow.newGame` raises the configured occasion after the runtime begins. At `idle`, `flow.idle` may raise an input automatically, or expose a menu of labelled occasions. `PlayerHost.idle(await_)` runs first and may return an `Input` to override the bindings flow. Returning `undefined` delegates to `flow.idle`: an input is sent immediately, while a menu becomes an `Idle` screen and waits for `pickIdle`.

## Saves and profile data

The core stores saves through Effect's `KeyValueStore`. Slots are `quick`, `auto`, and numbered slots `1..24`. A save has this shape:

```json
{
  "saveVersion": "0.40",
  "project": "<bundle fingerprint>",
  "savedAt": "<ISO timestamp>",
  "label": "Chapter 1",
  "runtime": { "snapshot": "<runtime snapshot>", "log": [], "backlog": [] },
  "thumbnail": "data:image/png;base64,..."
}
```

The runtime snapshot includes `world.stage` and `continuation.stage` where applicable; the player adds its deterministic input log and backlog. Profiles use `profileVersion: "0.40"`, the project fingerprint, `readLines`, and settings. Storage is namespaced by the project fingerprint, so unrelated bundles do not share saves. A different project or save format returns `SaveIncompatible`; malformed data returns `SaveCorrupt`.

## Rollback and history

The session retains the last `settings.rollbackDepth` input points (default 100). Each point contains runtime state, await, stage, backlog length, and log length. `rollback` interrupts current cue fibers, restores the selected point's stage through `Presenter.restore`, restarts its music, and resumes from that await. If no earlier point remains, the core returns `NothingToRollback`. It never serializes an XState actor snapshot as a save.

## Read history and backlog

Each line has a stable `lineId`. The profile's `readLines` set marks lines after they are shown. `LineView.read` controls read-aware skip behavior; the default read-only skip stops on unread lines, choices, bridges, and idle. Backlog entries retain line id, voice key, speaker name, text, and whether voice is available. Hosts can render the `Backlog` screen and replay voice with `replayVoice`.

For a complete architecture contract, see [`docs/player/architecture.md`](https://github.com/journeyWorker/lute/blob/main/docs/player/architecture.md). For browser implementations, see [the web architecture](/player/overview/).

## Choice views and restored assets

`Presenter.showChoice` receives a `ChoiceView` with the authored prompt and
localized option text, plus runtime verdict, timeout, default, once, and exit
metadata. On load and rollback, the director ensures the restored stage's
bound files before calling `Presenter.restore`.
