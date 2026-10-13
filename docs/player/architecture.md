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
type IrRecord = Ir.Command            // from @lute-lang/runtime (generated from schemas/lute-ir-0.41.schema.json)

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
  readonly blocking: boolean          // lines, sequences and wait:true cues block the batch
  readonly stage: boolean             // affects persistent stage (actor/bg/cg/video/music): settled at an await
  readonly assets: ReadonlyArray<AssetKey>
  readonly execute: (ctx: CueContext) => Effect.Effect<void>   // normal play; completes when the effect finished
  readonly complete: (ctx: CueContext) => Effect.Effect<void>  // skip: snap to the end state
  readonly describe: () => string
}

interface CueFactory {
  /** One output batch → cues (records in order + presentation events). Unknown record kind = defect. */
  readonly fromOutput: (output: Output, locale: string, readLines: ReadonlySet<string>) => Effect.Effect<ReadonlyArray<Cue>>
}
```

`LineCue.execute` = `presenter.showLine(view, "normal")` in parallel with the
voice (`audio.play("voice", key)` when `bindings.voice` resolves a file);
completes when the text is fully shown — releasing the line is the
director's job (see `LineGate`). `LineView.speakerName` is the index `cast`
name (`bundle.index.cast`, empty for narration); `emotion` / `variant` carry
the line command's values.

**One cue per event (normative).** `CueFactory.fromOutput` returns exactly
one cue per output event, in event order. A host MAY wrap the factory relying
on it.

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
`PluginCall { tag, attrs, document, position }`) at the record's place and
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
   * (skip can be toggled mid-batch). Completes when every blocking cue ran and
   * every stage cue settled; one-shot cues may still be running (they are
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
  readonly snapshot: Effect.Effect<SaveRuntime>                        // runtime snapshot + log
  readonly restore: (save: SaveRuntime) => Effect.Effect<InputPoint, SaveIncompatible | RuntimeRejection>
  readonly backlog: SubscriptionRef<ReadonlyArray<BacklogEntry>>
}
```

History keeps the last `settings.rollbackDepth` (default 100) input points.
The input log starts with `{ seed }` (the `seed` option) or `{ snapshot }`
(the runtime session's save when the player session is built). `begin`
returns the runtime session's `started` output while the player session has
accepted no input since it was built; afterwards it sends `newRun`. Either is
followed by the bound `flow.newGame` raise, the events concatenated in order
with the last await (so the first `begin` presents `started` before the
raise). A session that waits where `newRun` is refused (halted, at a choice
or a bridge) is first restored to the snapshot captured when the player
session was built. Backlog names come from the `cast` option (the index
`cast`).

### stores (`stores/`)

`SaveStore` (slots: `quick`, `auto`, `1..24`; list with metadata and
thumbnails; save/load/delete) and `ProfileStore` (read lines, settings) over
`effect/persistence/KeyValueStore`; formats per spec §6.7 with Effect Schemas
and version checks (`SaveIncompatible`).

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
  the stage's files, `presenter.restore(stage)`, restart the stage music;
  re-present the await (`load.done`, accepted from `title`, `ended` and
  `halted`).
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
metadata. Before `Presenter.restore(stage)` on load, rollback or `restore`,
the director ensures `stageFiles(bindings, stage)` so bound background,
sprites, CG/video, and the music `stageMusic` restarts are available. Cues and
restored stages name files with one rule: an asset id is its `assets` entry,
or the id itself as a file relative to `assetsBase` (`assetFile`); the host's
loader and audio port resolve files against `assetsBase`.

## Package checks

From each package directory, `bun run typecheck` is the package typecheck.
For the 0.42 release it invokes `bun check`; `bun run build` remains the
declaration-emitting path through `scripts/build.ts` and `tsc`.

CI pins Bun 1.4.3 and runs `scripts/compare-typecheck.sh <package-dir>` once
during 0.42.0 to compare normalized Bun and `tsc --noEmit` diagnostics.
Identical non-empty diagnostic sets may accompany identical command failures; a
failed command with no extracted diagnostics fails the comparison. This
temporary comparison is removed after 0.42.0.
