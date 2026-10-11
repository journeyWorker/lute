# `@lute-lang/player` — architecture

Normative behaviour: `docs/proposals/scenario-dsl/0.40.0.md` §6–§7. This document
fixes the module boundaries and the TypeScript contracts between them, so the
modules are built and tested independently and composed by the director.

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
type IrRecord = Ir.Command            // from @lute-lang/runtime (generated from schemas/lute-ir-0.40.schema.json)

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
director's job (see `LineGate`). `MusicCue` resolves `assetId` / `mood`
through bindings, crossfades, and stops on `stop: true`.

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
  readonly begin: Effect.Effect<Output, RuntimeRejection>              // seed + flow.newGame raise
  readonly send: (input: Input) => Effect.Effect<Output, RuntimeRejection>
  readonly current: Effect.Effect<InputPoint>
  readonly rollback: (steps: number) => Effect.Effect<InputPoint>       // fails NothingToRollback
  readonly snapshot: Effect.Effect<SaveRuntime>                        // runtime snapshot + log
  readonly restore: (save: SaveRuntime) => Effect.Effect<InputPoint, SaveIncompatible | RuntimeRejection>
  readonly backlog: SubscriptionRef<ReadonlyArray<BacklogEntry>>
}
```

History keeps the last `settings.rollbackDepth` (default 100) input points.

### stores (`stores/`)

`SaveStore` (slots: `quick`, `auto`, `1..24`; list with metadata and
thumbnails; save/load/delete) and `ProfileStore` (read lines, settings) over
`effect/persistence/KeyValueStore`; formats per spec §6.7 with Effect Schemas
and version checks (`SaveIncompatible`).

### director (`director/`) and the facade

`LutePlayer.make(options)` (scoped) composes: bindings decode, runtime
session, machine actor, scheduler, ports, stores. It is the only module that
creates fibers that outlive a call. It exposes `actions: (a: PlayerAction) =>
Effect<void>`, `view: SubscriptionRef<PlayerView>` (what screens render), and
`host` hooks.

## Testing

- `cues/`: every record kind decodes; unknown kind is a defect; line view
  resolution (cast name, locale, segments, read flag); music stop.
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
  the runtime's; save → load reproduces stage/backlog/await; rollback.
