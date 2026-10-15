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
      fromOutput: (output, readLines) =>
        base.fromOutput(output, readLines).pipe(Effect.tap((cues) => Effect.log(`${cues.length} cues`)))
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
| Restores | the story where it was | the story and the player's log and backlog; read marks added after the checkpoint are removed, rollback history is cleared, the `auto` slot is rewritten, stage and music are rebuilt, and the await is presented again |

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


### Resuming a fresh player-web mount

The player-web Promise facade exposes the same save Effects as the scoped
handle. Save the `SaveRuntime`, then start a fresh mount from its runtime
snapshot and restore the complete save:

```ts
import { mountPlayer } from "@lute-lang/player-web"
import type { SaveRuntime } from "@lute-lang/player"

const mountOptions = {
  bindings: { url: "/player.bindings.yaml" },
  bundleUrl: "/bundle/",
  wasmUrl: "/wasm/lute_runtime_wasm_bg.wasm"
} as const
const player = await mountPlayer(document.querySelector("#player")!, mountOptions)
const save: SaveRuntime = await player.snapshot()
await player.unmount()

const fresh = await mountPlayer(document.querySelector("#player")!, {
  ...mountOptions,
  snapshot: save.snapshot
})
await fresh.restore(save)
await fresh.send({ type: "raiseOccasion", occasion: "ambient", target: "npc.guide" })
const bundle = fresh.bundle
await fresh.unmount()
```

`mountPlayer` also provides `program` directly and rejects `send`, `snapshot`,
and `restore` with the same failures as the underlying Effects. Its `bundle`
is the bundle loaded by that player, so hosts can inspect `index.documents`
and `artifacts` without loading a second copy.

## Background barks during a conversation (0.45)

A host can show an ambient line while the story waits at a choice or a bridge,
without a second presentation (the runtime keeps one open presentation, and
concurrent presentations are not planned). The read-only queries —
`candidates`, `evaluate`, `def`, `gate`, `view` on `player.program` — answer at
every await and never change the state, so the host can pick a bark while the
conversation is suspended, then read its lines from the bundle:

```ts
import { Effect, Schema, SubscriptionRef } from "effect"
import type { LutePlayerHandle } from "@lute-lang/player"
import type { Bundle, Contract } from "@lute-lang/runtime"

interface AmbientLine { readonly text: string; readonly speaker?: string; readonly voiceKey: string; readonly lineId: string }
declare const showAmbient: (lines: ReadonlyArray<AmbientLine>) => Effect.Effect<void>   // the host's own UI

// The record fields the pattern reads; other fields are ignored.
const Artifact = Schema.Struct({
  commands: Schema.Array(Schema.Struct({
    kind: Schema.String,
    position: Schema.String,
    id: Schema.optionalKey(Schema.String),
    body: Schema.optionalKey(Schema.String),
    speaker: Schema.optionalKey(Schema.String),
    text: Schema.optionalKey(Schema.String),
    texts: Schema.optionalKey(Schema.Record(Schema.String, Schema.String)),
    voiceKey: Schema.optionalKey(Schema.String),
    lineId: Schema.optionalKey(Schema.String),
    placeholders: Schema.optionalKey(Schema.Unknown)
  }))
})

/** A static entry's lines, or undefined when the bark must be raised instead. */
const staticBarkLines = (bundle: Bundle.Bundle, candidate: Contract.Candidate, locale?: string): ReadonlyArray<AmbientLine> | undefined => {
  if (candidate.kind !== "entry" || candidate.verdict.verdict !== "eligible") return undefined
  const document = bundle.index.documents.find((item) => item.path === candidate.document)
  if (document === undefined) return undefined
  const { commands } = Schema.decodeUnknownSync(Artifact)(bundle.artifacts[document.artifact])
  const head = commands.find((record) => record.kind === "entry" && record.id === candidate.id)
  const start = commands.findIndex((record) => record.position === head?.body)
  if (start < 0) return undefined
  const next = commands.findIndex((record, index) => index > start && (record.kind === "entry" || record.kind === "beat"))
  const body = commands.slice(start, next < 0 ? commands.length : next)
  const lines: Array<AmbientLine> = []
  for (const record of body) {
    if (record.kind !== "line" || record.placeholders !== undefined) return undefined
    if (record.text === undefined || record.voiceKey === undefined || record.lineId === undefined) return undefined
    const speaker = record.speaker === undefined ? undefined : bundle.index.cast?.[record.speaker]
    lines.push({
      text: (locale === undefined ? undefined : record.texts?.[locale]) ?? record.text,
      voiceKey: record.voiceKey,
      lineId: record.lineId,
      ...(speaker === undefined ? {} : { speaker })
    })
  }
  return lines
}

// While the story waits at a choice or a bridge:
const ambientBark = (player: LutePlayerHandle, bundle: Bundle.Bundle) => Effect.gen(function* () {
  const { state } = yield* SubscriptionRef.get(player.session.current)
  const candidates = yield* player.program.candidates(state, "ambient", "npc.guide")
  for (const candidate of candidates) {
    const lines = staticBarkLines(bundle, candidate)
    if (lines !== undefined) return yield* showAmbient(lines)
  }
})
```

`candidates` returns beats in selection order; the helper takes an eligible
lore entry, finds its head in the document's artifact (`index.documents`), and
reads the records from the head's `body` position up to the next `entry` or
`beat` head. The bark is **static** when every one of them is a `line` without
`placeholders`; each line gives its text (`texts[locale]` when the bundle has
that locale), the speaker's display name from `index.cast` (none for
narration), `voiceKey` and `lineId`.

Showing a bark this way records nothing: no `entry.<id>.read`, no `once`
spending, no writes. A bark that must record — a first read, `once`, an
authored write such as `::set` — or whose body is dynamic (`{{…}}`
placeholders, `match`) is raised as its occasion once the story is back at
`idle`:

```ts
yield* player.send({ type: "raiseOccasion", occasion: "ambient", target: "npc.guide" })
```

See [0.45.0 §3](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.45.0.md#3-background-barks-during-a-conversation-host-pattern).

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
- `send(input)` hands the runtime an input while no batch runs and the story waits at `idle` or `ended` (at `ended`, only `newRun` or an outside-run raise is accepted). Elsewhere it fails with `DirectorBusy { flow, story }` and changes nothing; a runtime rejection fails it with the runtime's error and the director stays at the same await.
- **Writes that belong to the ending (0.44).** Once the story is at `ended`, a `hostWrite` is refused (`RuntimeBusy`, its message says so). Send ending metadata as `writes` on the input that ends the run — the `raiseOccasion`, `advanceClock`, `bridgeResult` or `hostWrite` that makes `terminal` hold (`send({ type: "raiseOccasion", occasion: "dawn", writes: { state: [{ path: "run.endedBy", value: "dawnSeven" }] } })`). `choose`, `timeout` and `worldEvent` carry no writes: when one of them ends the run, record the state in the story (a `::set` in the branch) or in a `hostWrite` sent before it. `newRun`'s writes seed the next run, where the ended run's values read as `prev.run.*`.
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
  "saveVersion": "0.45",
  "project": "<bundle fingerprint>",
  "savedAt": "<ISO timestamp>",
  "label": "Chapter 1",
  "runtime": { "snapshot": "<runtime snapshot>", "log": [], "backlog": [] },
  "thumbnail": "data:image/png;base64,..."
}
```

The runtime snapshot includes `world.stage` and `continuation.stage` where applicable; the player adds its deterministic input log and backlog. Profiles use `profileVersion` (the release line, `"0.45"`), the project fingerprint, `readLines`, and settings. Slot keys (`save:quick`, `save:auto`, `save:1`…) are not namespaced; each save carries the project fingerprint and another project's save is refused, and the profile is stored under `profile:<project>`. Give the player its own (or a prefixed) store when the game stores other data. A different project or save format returns `SaveIncompatible`; malformed data returns `SaveCorrupt`.

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
