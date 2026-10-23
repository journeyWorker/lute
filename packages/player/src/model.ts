/**
 * The core's shared vocabulary (docs/player/architecture.md "Module contracts").
 * Modules import only this file, `ports/`, and `@lute-lang/runtime`.
 */
import type { Effect, Scope, Stream, SubscriptionRef } from "effect"
import type { Contract, Program } from "@lute-lang/runtime"
import type { Bindings } from "./bindings/schema.js"
import type {
  AudioPort, BacklogEntry, ChoiceView, LineView, PlayerAction, PlayMode, Presenter, ScreenModel, Settings, SlotId
} from "./ports/index.js"

// ── Runtime contract (generated in @lute-lang/runtime) ──────────────────────

export type Await = Contract.Await
export type MenuAwait = Contract.MenuAwait
export type BridgeAwait = Extract<Contract.Await, { readonly type: "awaitBridge" }>
export type ChoiceAwait = Extract<Contract.Await, { readonly type: "awaitChoice" }>
export type Input = Contract.Input
export type Output = Contract.Output
export type Event = Contract.Event
export type Snapshot = Contract.Snapshot
export type Stage = Contract.Stage
export type RuntimeState = Program.RuntimeState

// ── Bindings (decoded player.bindings.yaml; schema generated from Rust) ─────

export type { Bindings }

// ── Cues (command pattern) ───────────────────────────────────────────────────

export interface CueContext {
  readonly presenter: Presenter
  readonly audio: AudioPort
  readonly bindings: Bindings
  readonly settings: Settings
  /** Called when a voice clip ends; optional for standalone scheduler callers. */
  readonly voiceEnded?: (lineId: string) => Effect.Effect<void>
}

/** Resolved timing of a cue (record `timing` with defaults applied). */
export interface CueTiming {
  /** Seconds to wait before the cue starts (`timing.delay`). */
  readonly delay: number
  /** Seconds the effect lasts, when authored (`timing.duration`). */
  readonly duration: number | undefined
  /**
   * The batch waits for this cue to finish (`timing.wait`; sequences always).
   * A line always waits for its release; `false` lets the cues after it run
   * while it is presented, up to the next line (spec 0.49.0 §4.2).
   */
  readonly wait: boolean
  /** Timeline ordinal and start offset (`timing.timeline`, `timing.at`). */
  readonly timeline: number | undefined
  readonly at: number | undefined
}

export type CueKind =
  | "line" | "bg" | "music" | "sfx" | "vfx" | "actor" | "camera" | "cg" | "video" | "sequence" | "barrier"
  | "presentation" | "presentationEnd" | "quest" | "clock" | "plugin" | "bookkeeping"

/** One runtime record (or session event) as an executable command. */
export interface Cue {
  readonly kind: CueKind
  readonly timing: CueTiming
  /** Changes persistent stage (bg, actor, music, cg, video): settled before an await is presented. */
  readonly stage: boolean
  /** Bound files the cue needs loaded. */
  readonly files: ReadonlyArray<string>
  /** Present for line cues. */
  readonly line: LineView | undefined
  /** Play normally; completes when the effect finished. */
  readonly execute: (ctx: CueContext) => Effect.Effect<void>
  /** Snap to the end state at once (skip). */
  readonly complete: (ctx: CueContext) => Effect.Effect<void>
  /** Short text for logs and the debug overlay. */
  readonly describe: string
}

// ── Scheduler ────────────────────────────────────────────────────────────────

/** The director's hook into line pacing. */
export interface LineGate {
  /** Called when a line starts typing. */
  readonly started: (line: LineView) => Effect.Effect<void>
  /** Called after a line is fully shown; resolves when the reader, auto or skip releases it. */
  readonly release: (line: LineView) => Effect.Effect<void>
}

export type SchedulerEvent =
  | { readonly _tag: "CueStarted"; readonly index: number; readonly cue: Cue }
  | { readonly _tag: "CueDone"; readonly index: number; readonly cue: Cue }
  | { readonly _tag: "LineShown"; readonly line: LineView }

// ── Machine ↔ director ───────────────────────────────────────────────────────

export type MachineEvent =
  | PlayerAction
  /** `confirmsLines`: the presenter's {@link Presenter.confirmsLines} (spec 0.41.0 §7.5). */
  | { readonly type: "boot.ready"; readonly canContinue: boolean; readonly confirmsLines?: boolean }
  | { readonly type: "batch.started" }
  | { readonly type: "batch.settled"; readonly await: Await }
  /** A line started typing. */
  | { readonly type: "line.typing"; readonly read: boolean; readonly hasVoice: boolean }
  /** A line is fully shown. */
  | { readonly type: "line.shown"; readonly read: boolean; readonly hasVoice: boolean }
  | { readonly type: "line.released" }
  | { readonly type: "voice.ended" }
  | { readonly type: "auto.elapsed" }
  | { readonly type: "countdown.tick"; readonly remaining: number }
  | { readonly type: "countdown.expired" }
  | { readonly type: "bridge.answered" }
  | { readonly type: "bridge.unhandled"; readonly tag: string }
  | { readonly type: "load.done"; readonly await: Await }
  | { readonly type: "save.done" }
  | { readonly type: "slots.listed"; readonly mode: "save" | "load" }
  | { readonly type: "slot.occupied"; readonly slot: SlotId; readonly occupied: boolean }
  | { readonly type: "failed"; readonly message: string }

/** The interaction state kept by the progression statechart. */
export interface PlayerMachineContext {
  readonly await: Await | undefined
  readonly line: "typing" | "shown" | undefined
  readonly mode: "normal" | "auto" | "skip"
  readonly skipMode: "read" | "all"
  readonly overlayStack: ReadonlyArray<Overlay>
  readonly countdownRemaining: number | undefined
  readonly uiHidden: boolean
  readonly lastLineRead: boolean
  readonly hasVoice: boolean
  readonly autoPending: boolean
  readonly canContinue: boolean
  readonly occupiedSlots: ReadonlyArray<SlotId>
  readonly pendingConfirm: ConfirmAction | undefined
  readonly haltedMessage: string | undefined
  /** The presenter confirms lines itself: an `advance` while typing is buffered. */
  readonly confirmsLines: boolean
  /** An `advance` arrived while the line typed; applied as the release once it is shown. */
  readonly advanceBuffered: boolean
}

/** An overlay screen on top of the game. */
export type Overlay = "menu" | "backlog" | "save" | "load" | "settings" | "confirm"

/** An operation that is waiting for confirmation in an overlay. */
export type ConfirmAction =
  | { readonly type: "overwrite"; readonly slot: SlotId }
  | { readonly type: "load"; readonly slot: SlotId }
  | { readonly type: "delete"; readonly slot: SlotId }
  | { readonly type: "toTitle" }

/** A renderer-facing summary derived from a player machine snapshot. */
export interface PlayerView {
  readonly flow: "boot" | "title" | "loading" | "playing" | "ended" | "error"
  readonly story: string
  readonly mode: "normal" | "auto" | "skip"
  readonly overlay: "none" | "menu" | "backlog" | "saveLoad" | "settings" | "confirm" | "hidden"
  readonly countdownRemaining: number | undefined
  readonly uiHidden: boolean
}

export type DirectorCommand =
  | { readonly type: "session.newGame" }
  | { readonly type: "session.continue" }
  | { readonly type: "session.send"; readonly input: Input }
  | { readonly type: "line.reveal" }
  | { readonly type: "line.release" }
  | { readonly type: "auto.start"; readonly waitVoice: boolean }
  | { readonly type: "auto.cancel" }
  | { readonly type: "countdown.start"; readonly seconds: number }
  | { readonly type: "countdown.pause" }
  | { readonly type: "countdown.resume" }
  | { readonly type: "countdown.cancel" }
  | { readonly type: "choice.show"; readonly await: ChoiceAwait }
  | { readonly type: "choice.hide" }
  | { readonly type: "bridge.call"; readonly await: BridgeAwait }
  | { readonly type: "idle.flow"; readonly await: Await }
  | { readonly type: "mode.changed"; readonly mode: "normal" | "auto" | "skip" }
  | { readonly type: "save"; readonly slot: SlotId }
  | { readonly type: "load"; readonly slot: SlotId }
  | { readonly type: "delete"; readonly slot: SlotId }
  | { readonly type: "rollback" }
  | { readonly type: "screen.show"; readonly screen: ScreenModel }
  | { readonly type: "screen.hide" }
  /** Show the top overlay; the director builds its screen model from the stores. */
  | { readonly type: "overlay.show"; readonly overlay: Overlay; readonly confirm: ConfirmAction | undefined }
  | { readonly type: "overlay.hide" }
  | { readonly type: "ui.hidden"; readonly hidden: boolean }
  | { readonly type: "settings.apply"; readonly settings: Partial<Settings> }
  | { readonly type: "voice.replay"; readonly lineId: string; readonly voiceKey: string }
  | { readonly type: "toTitle" }

// ── Session ──────────────────────────────────────────────────────────────────

/** The player state at one runtime await. */
export interface InputPoint {
  readonly state: RuntimeState
  readonly await: Await
  readonly stage: Stage
  readonly backlogLength: number
  readonly logLength: number
}

/** What a save stores about the runtime (spec §6.7). */
export interface SaveRuntime {
  readonly snapshot: Snapshot
  /** The session's start (`{ seed }` or `{ snapshot }`), then every accepted input. */
  readonly log: ReadonlyArray<LogEntry>
  readonly backlog: ReadonlyArray<BacklogEntry>
}

export type LogEntry = { readonly seed: Contract.Seed } | { readonly snapshot: Snapshot } | { readonly input: Input }

// ── Module service shapes ────────────────────────────────────────────────────

export interface CueFactoryShape {
  /**
   * One runtime output → cues, in order, with line text for `locale` (a
   * record's translation, else its source text). A record kind the factory
   * does not know is a defect.
   */
  readonly fromOutput: (output: Output, readLines: ReadonlySet<string>, locale: string) => Effect.Effect<ReadonlyArray<Cue>>
  /** Resolve authored choice text for a runtime await, in `locale` (translation, else source). */
  readonly choiceView: (await_: ChoiceAwait, locale: string) => ChoiceView
}
export type CueFactoryWrapper = (base: CueFactoryShape) => CueFactoryShape
export interface CueSchedulerShape {
  readonly play: (
    cues: ReadonlyArray<Cue>,
    ctx: CueContext,
    mode: SubscriptionRef.SubscriptionRef<PlayMode>,
    gate: LineGate
  ) => Effect.Effect<void, never, Scope.Scope>
  readonly events: Stream.Stream<SchedulerEvent>
}

