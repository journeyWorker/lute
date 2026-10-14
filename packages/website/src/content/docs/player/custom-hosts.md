---
title: Custom player hosts
description: Build a native or test host for the renderer-free Lute player core with Presenter, AudioPort, AssetLoader, and PlayerHost.
---

Use `@lute-lang/player` directly when PixiJS is not the right renderer. The core has four seams: `Presenter` renders cues and screens, `AudioPort` plays bound sounds, `AssetLoader` loads bound files, and `PlayerHost` handles host-specific bridges, plugin records and idle policy.

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
    bridge: { "save-score": (request) => sendToNativeHost(request).pipe(Effect.map((fields) => ({ fields }))) },
    plugin: { well: (call) => game.drawFromWell(call.attrs) },
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
`LutePlayerOptions` requires `bindings`, `presenter`, `audio`, and `assets`; it accepts optional `host`, `cueFactory`, `linePacing`, and `autosave`, plus one start: `seed` (default `{ derive: true }`), `snapshot` (a runtime snapshot), or `session` (a runtime session the host built — the player drives it and never closes it). `cueFactory` wraps the default `CueFactoryShape` exactly once; return its `fromOutput` unchanged when you need the one-cue-per-output-event ordering. Bindings can be decoded data or YAML text. Provide runtime, bundle, input-log, and key-value-store services before running `LutePlayer.make` in a scope; with `session`, only the bundle (the same one) and the key-value store are required.

## Upgrading from 0.40

The player-session option that names speakers is `cast` (since 0.41.0). A host that composes the session itself with `Session.layer` (the `PlayerSession` layer) passes `SessionOptions.cast` — a plain record, cast id → display name, which is the bundle index's `cast` — where 0.40 took `castNames`, a `ReadonlyMap`. The old `castNamesFrom(artifacts)` helper is gone; read the names from `bundle.index.cast`:

```ts
import { Session } from "@lute-lang/player"

// 0.40: Session.layer({ bindings, castNames: Cues.castNamesFrom(artifacts) })
Session.layer({ bindings, cast: bundle.index.cast ?? {} })
```

`LutePlayer.make` fills `cast` from the bundle for you. The full list of 0.41 migration steps is in the [CHANGELOG](https://github.com/journeyWorker/lute/blob/v0.41.1/CHANGELOG.md#0411).

## The handle

`LutePlayer.make` returns `actions`, `view`, `send`, `snapshot`, `restore`, `shutdown`, `session`, and `program`:

- `snapshot` is an `Effect<SaveRuntime, Errors.RuntimeRejection>` checkpoint. It takes the player session's lock, so it is linearized with `send` and `restore`; called while a batch is presenting, it does not wait for the presentation and returns the last committed runtime snapshot, accepted-input log, and backlog (each backlog entry includes its `voiceKey`). It does not capture presenter overlays, timers, profile state, or renderer fibers; pass the result directly to `restore(save)`.
- `session` is the runtime session in use and `program` its `session.program`. Read `current`, subscribe to `events`, and run `view` / `evaluate` / `def` / `gate` / `candidates` without loading the bundle a second time.
- `send(input)` hands the runtime an input while no batch runs and the story waits at `idle` or `ended` (at `ended`, only `newRun` or an outside-run raise is accepted). An input that ends the run carries host writes on `raiseOccasion`, `advanceClock`, `bridgeResult`, or `hostWrite`; `choose`, `timeout`, and `worldEvent` carry none, so write before them or author the write. `newRun` writes seed the next run and the ended run's `run.*` is readable as `prev.run.*`. A `hostWrite` after the ending fails `RuntimeBusy` with a message saying the run has ended. Elsewhere it fails with `DirectorBusy { flow, story }` and changes nothing; a runtime rejection fails it with the runtime's error and the director stays at the same await.
`send` completes when the input was accepted and its batch started.

## Port contracts

`Presenter` implements `stage(cue, mode)`, `showLine(line, mode)`, `revealLine`, `clearLine`, `restore(stage)`, `showChoice(menu, remaining)`, `hideChoice`, `showScreen(screen)`, `hideScreen`, `setUiHidden`, `applySettings`, `capture`, and an `actions` stream. A staging or line effect completes when the visual effect or typewriter completes. `mode: "skip"` must snap to the end state. `restore` is used on load, rollback, and presentation start; restore the runtime's stage directly rather than replaying old records.

A `StageCue.Vfx` carries an `EffectSpec`: a catalogue effect (`flash`, `shake`, …) or `effect: "custom"` with a `name` — a `{ effect: custom }` vfx binding (its `name` or the binding key) or a vfx type with no binding. Branch on `cue.effect.effect === "custom"` and read `cue.effect.name`. `LineView.speakerName` is the cast name from the bundle index (empty for narration); `emotion` and `variant` carry the line's own values.

`AudioPort` implements `play(channel, sound)`, `stop(channel, fade)`, and `setVolumes`. Channels are `music`, `sfx`, and `voice`. `AudioPlay.file` is already resolved from bindings; `loop`, `volume` (0–1), `fade` and `restart` are explicit; a music `volume` is the gain `bindings.volumes` maps the record's member to (no member: 1; a member without a gain: 0). Music records follow `playback`: `start` plays from the start (`restart: true`); `stop` / `fadeOut` stop (fade `duration`, `fadeOut` defaulting to 1 s); any other value changes to the file, and playing the file that already plays without `restart` only changes its volume. `AssetLoader.ensure(files)` keeps files loaded for the open scope and reports progress through its stream; missing files fail with `AssetMissing`.

The public `PlayerAction` union is the device-independent input surface: advance, auto/skip, rollback, backlog/menu/save/load/settings, quick save/load, UI visibility, choice, idle, send, voice replay, and settings actions. A native host can emit these actions from any device.

## Line pacing

By default the director paces lines: `showLine` completes when the text is fully shown, `advance` reveals a typing line and releases a shown one. A presenter that types, pages and takes the confirm itself has two options:

- `linePacing: "presenter"`: `showLine` completes when the reader released the line. `advance` then never reveals or releases a line and the director runs no auto timer for lines (the presenter reads `PlayerView.mode`). `showLine(line, "skip")` still completes at once.
- Keep director pacing and set `Presenter.confirmsLines: true`: an `advance` that arrives while the line is typing is kept and applied as the release when the line is shown, instead of revealing it.

## Bridges

A bridge call suspends the runtime. Add a handler keyed by the bridge tag:

```ts
const host = {
  bridge: {
    "battle-result": (request) =>
      Effect.tryPromise(() => nativeBattle(request.attrs)).pipe(
        Effect.map((result) => ({
          fields: { won: result.won, score: result.score },
          writes: { state: [{ path: "run.gold", value: result.gold }] }
        }))
      )
  }
}
```

The handler receives the typed `BridgeAwait` (`request`, `tag`, `document`, `position`, the call's `attrs` and the typed `results` it must answer) and returns a `BridgeAnswer { fields, writes? }`. The director sends `bridgeResult { request, fields, writes? }`, then continues cue playback; the writes land before the lines that follow. The handler runs on a fiber of its own, so menus keep working while the host plays. If no handler exists, the player emits a bridge-unhandled failure rather than inventing an answer.

## Plugin records

A non-blocking plugin directive reaches the host through `PlayerHost.plugin`, keyed by tag. The record becomes a waiting cue: the handler runs with a `PluginCall { tag, attrs, document, position }` at the record's place in the batch, the batch continues when it completes, and it runs in skip mode too (a game action is not presentation). Records of calls answered through a bridge are never handed to `plugin`. The cue factory returns exactly one cue per output event, in order, so a host may also wrap it.

`Director.make` accepts the same `bundle`, `bindings`, optional cue-factory
wrapper, and `host` options, so a hand-composed director does not need a
second plugin-wiring call. If a pre-composed `Cues.make({ plugin })` and
`host.plugin` both handle a tag, the host handler wins; unique tags from both
maps remain available and the duplicate runs only once. A pre-composed factory
not built by `Cues.make` cannot take `host.plugin`: `Director.make` dies
rather than drop the handlers — pass a wrapper instead.

## Idle flow

`flow.newGame` raises the configured occasion after the runtime begins. The first new game presents the session's start (`started`) followed by that raise; a later new game from the title starts a new run (`newRun`, keeping `user.*` progress) instead of replaying the start. At `idle`, `flow.idle` may raise an input automatically, or expose a menu of labelled occasions. `PlayerHost.idle(await_)` runs first, on a fiber of its own, and may return an `Input` to override the bindings flow. Returning `undefined` delegates to `flow.idle`: an input is sent immediately, while a menu becomes an `Idle` screen and waits for `pickIdle`. A host that drives the story from its own game (a map, a HUD) returns `undefined` and calls `send` when the game acts; a `send` interrupts a pending `idle`.

## Autosave

`autosave` controls the `auto` slot: omitted, it is written at each presentation start and at every choice; `"presentation"` or `"choice"` writes only there; `false` never writes it (no snapshot, no `capture`). Quick saves and slot saves are unaffected.

## Saves and profile data

The core stores saves through Effect's `KeyValueStore`. Slots are `quick`, `auto`, and numbered slots `1..24`. A save has this shape:

```json
{
  "saveVersion": "the release line",
  "project": "<bundle fingerprint>",
  "savedAt": "<ISO timestamp>",
  "label": "Chapter 1",
  "runtime": { "snapshot": "<runtime snapshot>", "log": [], "backlog": [] },
  "thumbnail": "data:image/png;base64,..."
}
```

The runtime snapshot includes `world.stage` and `continuation.stage` where applicable; the player adds its deterministic input log and backlog. Profiles use `profileVersion: "0.41"`, the project fingerprint, `readLines`, and settings. Storage is namespaced by the project fingerprint, so unrelated bundles do not share saves. A different project or save format returns `SaveIncompatible`; malformed data returns `SaveCorrupt`.

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
