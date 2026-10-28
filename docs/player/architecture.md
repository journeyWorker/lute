# `@lute-lang/player` — architecture

Normative behaviour: `docs/proposals/scenario-dsl/0.40.0.md` §6–§7 and
`0.41.0.md` §7 (host surface). This document fixes the module boundaries and
the TypeScript contracts between them, so the modules are built and tested
independently and composed by the director.

```text
                    PlayerAction (input/)            host: PlayerHost
                          │                                │
                ┌─────────▼─────────┐  MachineEvent  ┌────▼──────────────┐
                │  machine/         │◄───────────────│  director/        │── ports ──► Presenter
                │  playerMachine    │                │  (Effect service) │            AudioPort
                │  (XState v6,      │ DirectorCommand│  owns the actor,  │            AssetLoader
                │   pure)           │───────────────►│  timers, fibers   │
                └───────────────────┘   (enq.emit)   └──┬─────┬──────┬───┘
                                                         │     │      │
                                         ┌───────────────▼┐ ┌──▼────┐ ┌▼─────────┐
                                         │ session/       │ │ cues/ │ │ stores/  │
                                         │ PlayerSession  │ │ +     │ │ saves,   │
                                         │ runtime, log,  │ │ sched-│ │ profile  │
                                         │ rollback       │ │ uler/ │ │          │
                                         └────────────────┘ └───────┘ └──────────┘
```

## Principles

- **The runtime owns the story** (world, stage, outcomes, timed-choice result).
  The player never decides an outcome; it sends inputs.
- **The machine owns interaction state and nothing else.** It is pure: no
  timers, no I/O, no Effect. Every side effect is an emitted
  `DirectorCommand`; every outside fact arrives as a `MachineEvent`. It is
  tested by sending events to an actor and asserting state + emitted commands.
- **The director owns time and effects.** It runs the actor, executes
  commands through ports on Effect fibers against `Clock` (auto delay,
  countdowns, voice waits), and feeds results back as events. All timers are
  `Clock`-based so `TestClock` drives every test.
- **Cues are commands** (GoF command): immutable objects built from records;
  the scheduler only knows `execute` / `complete`.
- **Saves are domain data** (runtime snapshot + player data), never actor
  snapshots.

## Module contracts

All types live in `src/model.ts` unless noted; modules import only `model.ts`,
`ports/`, and the runtime package.

### records → cues (`cues/`)

```ts
/** Every record the runtime emits, decoded with the generated IR schemas. */
type IrRecord = Ir.Command            // from @lute-lang/runtime (generated from schemas/lute-ir-0.53.schema.json)

interface CueContext {
  readonly presenter: Presenter
  readonly audio: AudioPort
  readonly bindings: Bindings
  readonly settings: Settings
}

interface Cue {
  readonly id: number                 // position in its batch
  readonly kind: CueKind              // "line" | staging kinds | "barrier" | "choiceRecord" | "state" | "plugin" | "presentation" | …
  readonly timing: CueTiming          // normalized: wait, delay, duration, at, timeline (all resolved defaults)
  readonly blocking: boolean          // lines, sequences, presentation and wait:true cues block the batch
  readonly stage: boolean             // affects persistent stage (actor/bg/cg/video/music): settled at an await
  readonly assets: ReadonlyArray<AssetKey>
  readonly execute: (ctx: CueContext) => Effect.Effect<void>   // normal play; completes when the effect finished
  readonly complete: (ctx: CueContext) => Effect.Effect<void>  // skip: snap to the end state
  readonly describe: () => string
}

interface CueFactory {
  /** One output batch → cues (records in order + presentation events), line text in `locale`. Unknown record kind = defect. */
  readonly fromOutput: (output: Output, readLines: ReadonlySet<string>, locale: string) => Effect.Effect<ReadonlyArray<Cue>>
  /** Authored branch/hub prompt and option labels for a runtime await, in `locale`. */
  readonly choiceView: (await_: ChoiceAwait, locale: string) => ChoiceView
}
```

`LineCue.execute` = `presenter.showLine(view, "normal")` in parallel with the
voice (`audio.play("voice", key)` when `bindings.voice` resolves a file);
completes when the text is fully shown — releasing the line is the
director's job (see `LineGate`). `LineView.speakerName` is the index `cast`
name (`bundle.index.cast`, empty for narration); `emotion` / `variant` carry
the line command's values.

The cue of a runtime `presentation` event clears the dialogue window
(`presenter.clearLine`) and waits by default (0.47 §2.7): it runs in order with
the batch, so it can never wipe the presentation's first line.

**One cue per event (normative).** `CueFactory.fromOutput` returns exactly
one cue per output event, in event order. A host MAY wrap the factory relying
on it.

**Locale per call (0.47).** The director passes the current
`settings.locale` to every `fromOutput` and `choiceView` call (custom
factories and `cueFactory` wrappers receive it and pass it on). A locale
switch while a batch plays calls `fromOutput` again for that batch with the new
locale; each line cue still to start takes the cue at its index from the
latest call, so the next line shown reads the new locale. A line reads
`localeSegments[locale]`, else `texts[locale]`, else its source segments /
text — all from the runtime's `line` record, whose `{{…}}` markers the runtime
already replaced with their values (a translation's marker takes the value the
source rendered for the same marker text, 0.47); the IR command's templates are
never shown. An option label reads `texts[locale]`, else its source text; a branch/hub
prompt reads the record's `promptTexts[locale]`, else its source `prompt`. A bundle's
locales come from `bundleLocales(bundle)`: the **source locale** is the
index's `contentLang`, else the `contentLang` of the first artifact (in
`index.documents` order) whose meta declares one, else `"en"`; the **compiled
locales** are the keys of every command's `texts` and `promptTexts` map, every choice/hub option's `texts` and every `plugin` command's `localized.<attr>.texts` (0.51; a `fields` or `stamps` map, or a `text` attribute named `texts`, is never read as one). The `Settings` screen model's `locales` is the source locale
followed by the compiled locales (sorted, without duplicates); a saved locale
the bundle does not carry reads source text and Settings shows the source
locale selected. `LutePlayer.make` starts a new profile at the source locale.

**Music** (`MusicCue`) reads resolved media fields from the runtime record's
`attrs` (`playback`, `volume`, `stop`, `mood`, `assetId`) rather than reopening
the artifact. Volume is an absolute binding member: an omitted member gives
gain `1`; an unresolved or malformed member gives `0`, and
`bindings.volumes` gains are clamped defensively to `[0, 1]`.
`stop: true` or `playback: stop` stops (fade `duration ?? 0`); `fadeOut`
stops over `duration ?? 1` s; `start` plays the file from its start
(`AudioPlay.restart: true`); anything else changes to the file — the file
already playing only changes volume. After a load, rollback or `restore` the stage's
music is silent when the stage has none or its `playback` is `stop` /
`fadeOut` (`stageMusic`).

**Vfx** (`StageCue.Vfx.effect`): a catalogue binding gives its effect; a
`{ effect: custom, name?, duration? }` binding gives
`{ effect: "custom", name: name ?? key, params: {} }` with its `duration` as
the cue's; a type with no binding gives `{ effect: "custom", name: type,
params: {} }`. The presenter decides what a custom effect plays.

**Plugin records.** A `plugin` record whose tag has a `PlayerHost.plugin`
handler is a waiting cue: the scheduler runs the handler (with a
`PluginCall { tag, attrs, stamps?, raises?, localized?, document, position }`;
`raises` are the record's declared raises and `localized` each `text`
attribute in the current locale, 0.51) at the record's place and
continues when it completes, in skip mode too. Without a handler, and for a
record carrying `answered` (a call answered through `awaitBridge`) or an
effect record sharing the plugin's position, it stays bookkeeping.

### scheduler (`scheduler/`)

```ts
interface LineGate {                 // provided by the director per batch
  /** Called when a line is fully shown; resolves when the reader (or auto/skip) releases it. */
  readonly waitRelease: (line: LineView) => Effect.Effect<void>
}

interface CueScheduler {
  /**
   * Plays one batch (spec §6.3 rules 1–6). `mode` is read at each cue start
   * (skip can be toggled mid-batch). Completes when every blocking cue ran,
   * every stage cue settled and every forked cue (each non-waiting cue and
   * timeline clip) has started — its `delay`/`at` elapsed and its `execute`
   * begun (skip: `complete` begun; a cue still waiting when skip begins is
   * completed at once; a forked cue whose fiber exits before starting also
   * releases the wait); started one-shot cues may still be running (they are
   * forked into `scope`). Interrupting the returned effect interrupts all
   * cue fibers of the batch.
   */
  readonly play: (
    cues: ReadonlyArray<Cue>,
    ctx: CueContext,
    mode: SubscriptionRef<PlayMode>,
    gate: LineGate,
  ) => Effect.Effect<void, never, Scope>
  readonly events: Stream<SchedulerEvent>   // cueStarted / cueDone / lineShown, for the backlog and tests
}
```

### machine (`machine/`)

```ts
type MachineEvent =
  | PlayerAction                                          // from input/
  | { type: "boot.ready"; hasContinue: boolean }
  | { type: "batch.started" } | { type: "batch.settled"; await: Await }
  | { type: "line.shown"; read: boolean; hasVoice: boolean }
  | { type: "voice.ended" } | { type: "auto.elapsed" }
  | { type: "countdown.tick"; remaining: number } | { type: "countdown.expired" }
  | { type: "bridge.answered" } | { type: "bridge.unhandled"; tag: string }
  | { type: "load.done"; await: Await } | { type: "save.done" } | { type: "failed"; message: string }

type DirectorCommand =
  | { type: "session.newGame" } | { type: "session.continue" }
  | { type: "session.send"; input: Input }               // choose / timeout / raiseOccasion
  | { type: "line.reveal" } | { type: "line.release" }
  | { type: "auto.start"; afterVoice: boolean } | { type: "auto.cancel" }
  | { type: "countdown.start"; seconds: number } | { type: "countdown.pause" } | { type: "countdown.resume" } | { type: "countdown.cancel" }
  | { type: "bridge.call"; await: BridgeAwait }
  | { type: "idle.flow"; await: Await }
  | { type: "save"; slot: SlotId } | { type: "load"; slot: SlotId } | { type: "rollback" }
  | { type: "screen.show"; screen: ScreenModel } | { type: "screen.hide" }
  | { type: "ui.hide"; hidden: boolean } | { type: "settings.apply"; settings: Settings }
```

`playerMachine` (XState `6.0.0-alpha.65`, `setup({ schemas })`, inline
transition functions, `enq.emit` for commands) has the parallel regions of
spec §6.4. The machine context carries only interaction state: current
await, line state (`typing | shown`), mode, overlay stack, countdown
remaining, UI hidden, `lastLineRead`. Rules the machine owns: advance while
typing → `line.reveal`; advance while shown → `line.release`; auto on a shown
line → `auto.start` (cancelled by any input or overlay); skip read-only stops
on an unread line, at choices, at bridges, at idle; overlays pause
(`countdown.pause`, `auto.cancel`) and resume on close; a timed choice emits
`countdown.start` and on `countdown.expired` emits
`session.send {type:"timeout"}`; `choose` emits `session.send {type:"choose"}`.
`toTitle` asks the discard-progress confirm while playing, from the in-game
menu and in the error flow; with the flow at `ended` and no overlay open it
goes to the title at once (0.47 §2.5: nothing is lost).

### session (`session/`)

```ts
interface InputPoint {
  readonly state: RuntimeState         // immutable runtime state at this await (0.38)
  readonly await: Await
  readonly stage: Stage
  readonly backlogLength: number
  readonly logLength: number
}

interface PlayerSession {
  readonly begin: Effect.Effect<Output, RuntimeRejection>              // §7.3, below
  readonly send: (input: Input) => Effect.Effect<Output, RuntimeRejection>
  readonly current: Effect.Effect<InputPoint>
  readonly rollback: (steps: number) => Effect.Effect<InputPoint>       // fails NothingToRollback
  readonly snapshot: Effect.Effect<SaveRuntime>                        // runtime snapshot + log + backlog + pending raises
  readonly restore: (save: SaveRuntime) => Effect.Effect<InputPoint, SaveIncompatible | RuntimeRejection>
  readonly deliverRaise: Effect.Effect<Output | undefined>             // 0.51, below
  readonly backlog: SubscriptionRef<ReadonlyArray<BacklogEntry>>
}
```

History keeps the last `settings.rollbackDepth` (default 100) input points.
The input log starts with `{ seed }` (the `seed` option) or `{ snapshot }`
(the runtime session's save when the player session is built). `begin`
(0.47 §2.3):

- at `ended` (also a session restored from an ended snapshot) sends `newRun`;
  at `idle` once the session accepted an input, likewise;
- an untouched session presents the runtime session's `started` output;
- a session that cannot send `newRun` where it waits (untouched, halted, at a
  choice or a bridge) is restored to the `lineage` option's snapshot when
  there is one, then sends `newRun` (the log starts with
  `{ snapshot: <lineage> }`); a lineage the runtime refuses is dropped
  (`lineage.drop`). Without one, the session goes back to the snapshot
  captured when it was built (and presents `started`, or sends `newRun` when
  that start waits at `idle` / `ended`).

Each start is followed by the bound `flow.newGame` raise, the events
concatenated in order with the last await. Backlog names come from the `cast`
option (the index `cast`); a backlog entry also keeps the runtime `line`
record's rendered `texts` and `localeSegments`, and the director resolves them
for the current locale when the `Backlog` screen opens (`localizedEntry`).

**Pending raise queue (0.51.0 §4.6).** When an output is applied, the
`raises` of its `plugin` and `grant` records are queued in record and list
order, each tagged with its `document`, `position` and the output's input-log
index. `deliverRaise` takes the head, removes it and sends it as a
`raiseOccasion` under the session lock while the runtime waits at `idle`; a
raise the runtime rejects is logged and dropped. The director calls it on
`idle.flow` (after a batch, a load or a rollback settles at `idle`) and plays
each output as its own batch, so every raise goes before the host `idle` and
the bindings' idle flow. A raise takes no rollback checkpoint: the
checkpoint `send` keeps before a player input also keeps the queue, so one
`rollback(1)` undoes the input and every raise it caused. An accepted
`newRun`, a return to the start point and a lineage replacement replace the
queue with the start output's raises; a rejected one keeps it. Saves carry
the queue (`SaveRuntime.pendingRaises`); `restore` and `rollback` reinstate it.

### stores (`stores/`)

`SaveStore` (slots: `quick`, `auto`, `1..24`; list with metadata and
thumbnails; save/load/delete) and `ProfileStore` (read lines, settings,
lineage) over `effect/persistence/KeyValueStore`; formats per spec §6.7 with
Effect Schemas and version checks (`SaveIncompatible`). Saved backlog entries
keep `texts` / `localeSegments`. A profile of any `profileVersion` decodes:
its read marks and settings are kept, and only a profile of the current
version keeps its `lineage`. `setLineage` writes at once; settings changes are
debounced.

### Cross-run model (0.47)

What survives a reload: the **user tier** (`user.*`, user-tier facts,
`once: user` spends, entry read marks) and the **ended run's values** that
`newRun` turns into `prev.run.*` — both in the runtime snapshot, kept as the
profile's **lineage** — plus the profile's read history and settings. A run in
progress survives only through a save.

- **Written**: every time a batch settles at `ended`, the director stores
  `PlayerSession.snapshot`'s runtime snapshot with `ProfileStore.setLineage`,
  replacing the previous one (immediately; a failed write is logged).
- **Used**: `LutePlayer.make` passes the profile's lineage to the player
  session only when it starts without `snapshot` / `session` (the default or
  a host `seed`). New game that cannot send `newRun` where the session waits —
  an untouched session after a reload, or a run abandoned through the menu —
  restores it instead of the start point and sends `newRun`; a host `seed` is
  not used while a lineage exists. A lineage the runtime refuses is dropped
  and New game starts from the start point.

### director (`director/`) and the facade

`LutePlayer.make(options)` (scoped) composes: bindings decode, runtime
session, machine actor, scheduler, ports, stores. It is the only module that
creates fibers that outlive a call.

- **Start** (§7.1): `seed` (default `{ derive: true }`) or `snapshot` builds
  `Session.layer` in the player's scope; `session` drives the host's runtime
  session, which the player never closes (the host still provides the same
  bundle through `BundleSource`, and a `KeyValueStore`).
- **Handle**: `actions`, `view: SubscriptionRef<PlayerView>`, `send`,
  `snapshot`, `restore`, `shutdown`, plus `session` (the runtime session) and
  `program` (`session.program`) for `current`, `events`, `view` / `evaluate` /
  `def` / `gate` / `candidates` without a second load.
  `snapshot` is linearized with `send` and `restore` and returns the last
  committed runtime snapshot, accepted-input log and backlog.
- **`send(input)`** (§7.2–§7.3): accepted while no batch runs and the story
  waits at `idle` or `ended`; otherwise `DirectorBusy { flow, story }` and
  nothing changes. A runtime rejection fails `send` and the director stays at
  the same await. An accepted input plays as a batch (`ended` goes back to
  `playing`); `send` completes once the batch started, not when it ends. The
  `{ type: "send", input }` action does the same and logs a refusal.
- **Host hooks run on fibers of their own**: `PlayerHost.idle` and
  `PlayerHost.bridge` never block the command loop, so actions keep working
  and a handler's own `send` is refused as busy instead of deadlocking. A
  `send` or any input that leaves the await interrupts the pending hook. A
  bridge handler returns `BridgeAnswer { fields, writes? }`, sent as
  `bridgeResult`.
- **`restore(save)`** (§7.4), in every flow state but `boot`: interrupt the
  batch, countdown, auto timer and host hook; hide the choice UI, any overlay
  and the line; `PlayerSession.restore`; remove the read marks the director
  added at input points later than `save.log.length` (it journals the marks
  per input point); rewrite the `auto` slot unless `autosave: false`; ensure
  the stage's files, `presenter.restore(stage)`, begin the stage music (a
  fiber of the player scope, not awaited: `PixiAudio` holds a play until audio
  is unlocked); re-present the await (`load.done`, accepted from `title`,
  `ended` and `halted`).
- **Cue composition** (§5.1, §5.3): `LutePlayer.make` creates the default
  factory, applies `cueFactory?: (base) => base` exactly once, and preserves
  one cue per output event and event order. `Director.make` can build the same
  factory from its bundle and bindings; `host.plugin` wins duplicate tags when
  merged with a pre-composed factory.
- **Line pacing** (§7.5): `linePacing: "presenter"` — `showLine` completes
  when the reader released the line; `advance` never reveals or releases and
  no auto timer runs for lines. With the default `"director"` pacing, a
  presenter declaring `confirmsLines: true` gets an `advance` that arrives
  while the line types buffered and applied as the release at `line.shown`.
- **Autosave** (§7.9): omitted, the `auto` slot is written at a presentation
  start and at every `awaitChoice`; `"presentation"` / `"choice"`, only
  there; `false`, never (no snapshot, no `presenter.capture`).

## Testing

- `cues/`: every record kind decodes; unknown kind is a defect; line view
  resolution (index cast name, emotion/variant, locale, segments, read flag);
  the music table; vfx effects (custom, catalogue, unbound); plugin waiting
  cues; one cue per event.
- `scheduler/`: each §6.3 rule with a recording presenter and `TestClock`.
- `machine/`: pure actor tests — event sequences → state + emitted commands
  (advance/reveal/release, auto, skip stops, choice, timed choice → timeout,
  overlay pause/resume, rollback, save/load flows).
- `session/`: send/rollback/snapshot/restore round trips over a real compiled
  bundle (conformance projects).
- `stores/`: schema round trips, version refusal, slot listing.
- `director/`: end-to-end with fake ports: play `conformance/session/*`
  cases from their `inputs.jsonl` (answers by choose/bridge/timeout) and
  assert the presenter saw every record in order and the final await equals
  the runtime's; save → load reproduces stage/backlog/await; rollback;
  `send` (idle, ended, busy, rejected), host hooks on their own fibers,
  `restore` from playing/ended/halted, line pacing, autosave modes, bridge
  answers with writes.
- `LutePlayer`: start from a seed, a snapshot or a host session; the handle's
  `session` / `program`.

### Choices and restored stages

`CueFactory.choiceView(await_, locale)` resolves authored branch/hub prompt and
option text (with locale fallback) from the bundle index. Presenters receive a
`ChoiceView`, including runtime verdicts, timeout, default, once, and exit
metadata, and the plugin `stamps` authored on the `<branch>`/`<hub>` element
and on each `<choice>`, read from the IR command at the menu's address (each
absent when none is authored). Before `Presenter.restore(stage)` on load, rollback or `restore`,
the director ensures `stageFiles(bindings, stage)` so bound background,
sprites, CG/video, and the music `stageMusic` restarts are available. Cues and
restored stages name files with one rule: an asset id is its `assets` entry,
or the id itself as a file relative to `assetsBase` (`assetFile`); the host's
loader and audio port resolve files against `assetsBase`.

## Package checks

From each package directory, `bun run typecheck` is the package typecheck; it
runs `bun check`. `bun run build` remains the declaration-emitting path through
`scripts/build.ts` and `tsc`. CI pins Bun 1.4.3.

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
| Holds | runtime state: `await`, `continuation?`, `project`, `request`, `snapshotVersion`, `world` (state, facts, stage, clock) | `{ snapshot, log, backlog, pendingRaises }`: that runtime snapshot plus the player's accepted-input log, backlog and the declared raises it has not sent yet (`[]` when none) |
| Produced by | runtime `Session.save` | the player handle's `snapshot` (`DirectorHandle.snapshot`) |
| Consumed by | runtime `Session.restore`; `LutePlayer.make({ snapshot })`; `RuntimeSession.layer({ snapshot })` | the handle's `restore(save)`; save slots |
| Restores | the story where it was | the story and the player's log, backlog and pending raises (each sent once the story is idle again); read marks added after the checkpoint are removed, rollback history is cleared, the `auto` slot is rewritten, the stage is rebuilt, its music is begun without waiting for it to start, and the await is presented again |

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
