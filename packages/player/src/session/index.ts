/** Runtime-backed player session with rollback, saves, and backlog. */
import { Context, Effect, Exit, Layer, Result, Schema, Semaphore, SubscriptionRef } from "effect"
import { Session as RuntimeSession } from "@lute-lang/runtime"
import type { Contract, Errors } from "@lute-lang/runtime"
import { declaredRaises, recordTranslations } from "../cues/index.js"
import type { Bindings, Input, InputPoint, LogEntry, Output, PendingRaise, SaveRuntime } from "../model.js"
import type { BacklogEntry, Settings } from "../ports/index.js"

/** A failed rollback when no earlier input point is retained. */
export class NothingToRollback extends Schema.TaggedError<NothingToRollback>()("NothingToRollback", {
  steps: Schema.Int
}) {}

/** A save belongs to another runtime program or cannot be restored. */
export class SaveIncompatible extends Schema.TaggedError<SaveIncompatible>()("SaveIncompatible", {
  message: Schema.String
}) {}

/** Result of resolving the configured idle flow. */
export type IdleResult =
  | { readonly _tag: "Input"; readonly input: Input }
  | { readonly _tag: "Menu"; readonly options: ReadonlyArray<{ readonly label: string; readonly input: Input }> }

/** Build the input/menu represented by `bindings.flow.idle`. */
export const idle = (bindings: Bindings): IdleResult | undefined => {
  const flow = bindings.flow
  if (flow === undefined || flow.idle == null) return undefined
  if ("raise" in flow.idle) {
    const value = flow.idle.raise
    return {
      _tag: "Input",
      input: {
        type: "raiseOccasion",
        occasion: value.occasion,
        ...(value.target === undefined ? {} : { target: value.target })
      }
    }
  }
  return {
    _tag: "Menu",
    options: flow.idle.menu.map((option) => ({
      label: option.label,
      input: {
        type: "raiseOccasion",
        occasion: option.occasion,
        ...(option.target === undefined ? {} : { target: option.target })
      }
    }))
  }
}

interface PointRecord {
  readonly point: InputPoint
  readonly snapshot: Contract.Snapshot
  readonly log: ReadonlyArray<LogEntry>
  readonly backlog: ReadonlyArray<BacklogEntry>
  /** The pending raises before the player input (0.51.0 §4.6). */
  readonly pendingRaises: ReadonlyArray<PendingRaise>
}

/** Session service contract used by the director. */
export interface PlayerSessionShape {
  /**
   * Start the story (spec 0.41.0 §7.3, 0.47.0 §2.3). A session at `ended`
   * (also one restored from an ended snapshot) sends `newRun`; so does one at
   * `idle` once it accepted an input. An untouched session presents the
   * runtime session's `started` output. A session that cannot send `newRun`
   * where it waits (untouched, halted, at a choice or a bridge) is restored to
   * the {@link SessionOptions.lineage} when there is one, then sends `newRun`
   * (the input log starts with `{ snapshot: <lineage> }`); without one (or
   * when the runtime refuses it, which drops it) to the snapshot captured
   * when the player session was built. The start is followed by the bound
   * `flow.newGame` raise when there is one, the events concatenated in order
   * with the last await.
   */
  readonly begin: Effect.Effect<Output, Errors.RuntimeRejection | Errors.InputLogError>
  readonly send: (input: Input) => Effect.Effect<Output, Errors.RuntimeRejection | Errors.InputLogError>
  readonly current: Effect.Effect<InputPoint>
  readonly rollback: (steps: number) => Effect.Effect<InputPoint, NothingToRollback | Errors.RuntimeRejection | Errors.InputLogError>
  readonly snapshot: Effect.Effect<SaveRuntime, Errors.RuntimeRejection>
  /** Continue from a save: runtime snapshot, input log, backlog and pending raises; the rollback history is cleared. */
  readonly restore: (save: SaveRuntime) => Effect.Effect<InputPoint, SaveIncompatible | Errors.RuntimeRejection | Errors.InputLogError>
  /**
   * Deliver the next pending declared raise (0.51.0 §4.6) while the runtime
   * waits at `idle`: take the head of the queue, remove it and send it as a
   * `raiseOccasion` input, at once under the session lock, so a raise is
   * delivered exactly once. The send takes no rollback checkpoint: a raise
   * belongs to the rollback step of the player input that caused it. A raise
   * the runtime rejects is logged and dropped, and the next one is tried.
   * `undefined` when the queue is empty or the runtime does not wait at `idle`.
   */
  readonly deliverRaise: Effect.Effect<Output | undefined, Errors.InputLogError>
  readonly backlog: SubscriptionRef.SubscriptionRef<ReadonlyArray<BacklogEntry>>
  readonly idle: Effect.Effect<IdleResult | undefined>
  readonly fingerprint: string
}

/** Runtime-backed player session. Provide the runtime Session layer when composing. */
export class PlayerSession extends Context.Service<PlayerSession, PlayerSessionShape>()(
  "@lute-lang/player/PlayerSession"
) {}

/** The lineage a player keeps across reloads (spec 0.47.0 §2.3). */
export interface SessionLineage {
  /** The latest lineage: the runtime snapshot taken when an `ended` output was presented. */
  readonly get: Effect.Effect<Contract.Snapshot | undefined>
  /** Forget a lineage the runtime refused to restore. */
  readonly drop: Effect.Effect<void>
}

/** Options for constructing a player session over a runtime Session. */
export interface SessionOptions {
  readonly bindings: Bindings
  /**
   * The seed the runtime session started from: the input log starts with
   * `{ seed }`. Without it (a snapshot start, or a host's session) the log
   * starts with `{ snapshot }`, the session's save when the player session is
   * built.
   */
  readonly seed?: Contract.Seed
  readonly settings?: Pick<Settings, "rollbackDepth">
  /** The bundle index `cast` (cast id → display name) for backlog names; narration has none. */
  readonly cast?: Readonly<Record<string, string>>
  /**
   * Where New game continues a play-through when `newRun` cannot be sent
   * where the session waits. `LutePlayer.make` passes the profile's lineage
   * for a player started without `snapshot` / `session`.
   */
  readonly lineage?: SessionLineage
}

/** A backlog entry with `text` in `locale`: its translation, else the source text. */
export const localizedEntry = (entry: BacklogEntry, locale: string): BacklogEntry => {
  const translated = entry.texts?.[locale]
  return translated === undefined ? entry : { ...entry, text: translated }
}

/**
 * The declared raises of an output (0.51.0 §4.6): each record's `raises`,
 * in record order and list order, tagged with the record's source and the
 * input-log index of the entry the output answers.
 */
const raisesOf = (output: Output, logIndex: number): ReadonlyArray<PendingRaise> => {
  const result: PendingRaise[] = []
  for (const event of output.events) {
    if (event.type !== "record" || typeof event.record !== "object" || event.record === null || !("raises" in event.record)) continue
    const position = "position" in event.record && typeof event.record.position === "string" ? event.record.position : undefined
    for (const raise of declaredRaises(event.record.raises)) {
      result.push({ ...raise, document: event.document, ...(position === undefined ? {} : { position }), logIndex })
    }
  }
  return result
}

const raiseInput = (raise: PendingRaise): Input => ({
  type: "raiseOccasion",
  occasion: raise.occasion,
  ...(raise.target === undefined ? {} : { target: raise.target })
})

const linesFromOutput = (output: Output, cast: Readonly<Record<string, string>>): ReadonlyArray<BacklogEntry> => {
  const result: BacklogEntry[] = []
  for (const event of output.events) {
    if (event.type !== "record" || typeof event.record !== "object" || event.record === null) continue
    const entries = Object.entries(event.record)
    const value = (key: string): unknown => entries.find(([name]) => name === key)?.[1]
    if (value("kind") !== "line") continue
    const lineIdValue = value("lineId")
    if (typeof lineIdValue !== "string") continue
    const textValue = value("text")
    const text = typeof textValue === "string" ? textValue : ""
    const speaker = value("speaker")
    const voice = value("voiceKey")
    const role = value("role")
    const speakerName = typeof speaker !== "string" || role === "narration" || speaker === "narrator"
      ? ""
      : cast[speaker] ?? speaker
    const voiceKey = typeof voice === "string" ? voice : ""
    // The record's translations, markers rendered (spec 0.47.0 §2.2).
    const translated = recordTranslations({ texts: value("texts"), localeSegments: value("localeSegments") })
    result.push({ lineId: lineIdValue, voiceKey, speakerName, text, hasVoice: voiceKey.length > 0, ...translated })
  }
  return result
}
const point = (state: RuntimeSession.SessionState, backlogLength: number, logLength: number): InputPoint => ({
  state: state.state,
  await: state.await,
  stage: state.stage,
  backlogLength,
  logLength
})

/** `newRun` is accepted at `idle` and `ended` only (event contract). */
const acceptsNewRun = (await_: Contract.Await): boolean => await_.type === "idle" || await_.type === "ended"

/** Construct the service over the runtime Session in context; it never closes that session. */
export const layer = (
  options: SessionOptions
): Layer.Layer<PlayerSession, never, RuntimeSession.Session> =>
  Layer.effect(PlayerSession, Effect.gen(function* () {
    const runtime = yield* RuntimeSession.Session
    const cast = options.cast ?? {}
    const depth = Math.max(0, options.settings?.rollbackDepth ?? 100)
    const lock = yield* Semaphore.make(1)
    const backlog = yield* SubscriptionRef.make<ReadonlyArray<BacklogEntry>>([])
    // Where the player session starts: `begin` returns here when `newRun`
    // cannot run where the session waits.
    const start = yield* runtime.save
    const startEntry: LogEntry = options.seed === undefined ? { snapshot: start } : { seed: options.seed }
    let log: ReadonlyArray<LogEntry> = [startEntry]
    let history: ReadonlyArray<PointRecord> = []
    // 0.51.0 §4.6: the declared raises the host still owes, in delivery order.
    let pending: ReadonlyArray<PendingRaise> = []
    // No input accepted since the player session was built: `begin` presents `started`.
    let fresh = true

    const currentPoint = Effect.gen(function* () {
      const state = yield* SubscriptionRef.get(runtime.current)
      const entries = yield* SubscriptionRef.get(backlog)
      return point(state, entries.length, log.length)
    })

    /**
     * Submit `input`. A player input (`checkpoint`) first keeps the rollback
     * point before it; an automatic declared raise does not, so it belongs to
     * the rollback step of the player input that caused it (0.51.0 §4.6).
     */
    const submit = Effect.fn("Lute.Player.Session.send")(function* (input: Input, checkpoint: boolean) {
      const beforeBacklog = yield* SubscriptionRef.get(backlog)
      const beforeLog = log
      let before: PointRecord | undefined
      if (checkpoint) {
        const snapshot = yield* runtime.save
        const state = yield* SubscriptionRef.get(runtime.current)
        before = { point: point(state, beforeBacklog.length, beforeLog.length), snapshot, log: beforeLog, backlog: beforeBacklog, pendingRaises: pending }
      }
      const output = yield* runtime.send(input)
      if (before !== undefined) history = depth === 0 ? [] : [...history, before].slice(-depth)
      log = [...beforeLog, { input }]
      // An accepted `newRun` starts a run: the old pending raises go, and the
      // start output's are queued once, like any output's.
      const raised = raisesOf(output, beforeLog.length)
      pending = input.type === "newRun" ? raised : [...pending, ...raised]
      fresh = false
      const added = linesFromOutput(output, cast)
      yield* SubscriptionRef.set(backlog, [...beforeBacklog, ...added])
      return output
    })

    const deliverRaise = Effect.gen(function* () {
      while (pending.length > 0 && (yield* SubscriptionRef.get(runtime.current)).await.type === "idle") {
        const [head, ...rest] = pending
        if (head === undefined) break
        pending = rest
        const sent = yield* Effect.result(submit(raiseInput(head), false))
        if (Result.isSuccess(sent)) return sent.success
        if (sent.failure._tag === "InputLogError") return yield* Effect.fail(sent.failure)
        yield* Effect.logWarning("the runtime refused a declared raise; it is dropped", sent.failure, head)
      }
      return undefined
    }).pipe(Effect.withSpan("Lute.Player.Session.deliverRaise"))

    /** Back to where the player session started: the snapshot captured when it was built. */
    const toStart = Effect.gen(function* () {
      yield* runtime.restore(start)
      log = [startEntry]
      history = []
      pending = []
      yield* SubscriptionRef.set(backlog, [])
    })

    /**
     * Continue the play-through of the lineage: restore it and send `newRun`.
     * `undefined` without a lineage, or when the runtime refuses it (the
     * lineage is dropped and the session is back where it was or at its start).
     */
    const continueLineage = Effect.gen(function* () {
      const lineage = options.lineage === undefined ? undefined : yield* options.lineage.get
      if (options.lineage === undefined || lineage === undefined) return undefined
      const restored = yield* Effect.exit(runtime.restore(lineage))
      if (Exit.isSuccess(restored)) {
        log = [{ snapshot: lineage }]
        history = []
        pending = []
        yield* SubscriptionRef.set(backlog, [])
        const sent = yield* Effect.exit(submit({ type: "newRun" }, true))
        if (Exit.isSuccess(sent)) return sent.value
        yield* Effect.logWarning("the lineage refused newRun; New game starts from the start point", sent.cause)
        yield* toStart
      } else yield* Effect.logWarning("the runtime refused the lineage; New game starts from the start point", restored.cause)
      yield* options.lineage.drop
      return undefined
    })

    const begin = Effect.gen(function* () {
      const waiting = (yield* SubscriptionRef.get(runtime.current)).await
      let first: Output | undefined
      if (waiting.type === "ended" || (!fresh && waiting.type === "idle")) first = yield* submit({ type: "newRun" }, true)
      else first = yield* continueLineage
      if (first === undefined) {
        let fromStart = fresh
        if (!fresh) {
          yield* toStart
          fromStart = !acceptsNewRun(start.await)
        }
        if (fromStart) {
          // The start output is presented again: its raises are the queue.
          first = runtime.started
          pending = raisesOf(first, 0)
        } else first = yield* submit({ type: "newRun" }, true)
      }
      const newGame = options.bindings.flow?.newGame
      if (newGame == null) return first
      const raised = yield* submit({ type: "raiseOccasion", occasion: newGame.occasion, ...(newGame.target == null ? {} : { target: newGame.target }) }, true)
      return { ...raised, events: [...first.events, ...raised.events] } satisfies Output
    }).pipe(Effect.withSpan("Lute.Player.Session.begin"))

    const rollback = Effect.fn("Lute.Player.Session.rollback")(function* (steps: number) {
      if (!Number.isInteger(steps) || steps < 1 || steps > history.length) {
        return yield* Effect.fail(new NothingToRollback({ steps }))
      }
      const target = history[history.length - steps]
      if (target === undefined) return yield* Effect.fail(new NothingToRollback({ steps }))
      yield* runtime.restore(target.snapshot)
      log = target.log
      pending = target.pendingRaises
      fresh = false
      yield* SubscriptionRef.set(backlog, target.backlog)
      history = history.slice(0, history.length - steps)
      return target.point
    })

    const snapshot = Effect.gen(function* () {
      const runtimeSnapshot = yield* runtime.save
      const entries = yield* SubscriptionRef.get(backlog)
      return { snapshot: runtimeSnapshot, log, backlog: entries, pendingRaises: pending } satisfies SaveRuntime
    })

    const restore = Effect.fn("Lute.Player.Session.restore")(function* (save: SaveRuntime) {
      if (save.snapshot.project !== undefined && save.snapshot.project !== runtime.fingerprint) {
        return yield* Effect.fail(new SaveIncompatible({ message: "save belongs to another project" }))
      }
      yield* runtime.restore(save.snapshot)
      log = save.log
      pending = save.pendingRaises
      fresh = false
      yield* SubscriptionRef.set(backlog, save.backlog)
      const state = yield* SubscriptionRef.get(runtime.current)
      const entries = yield* SubscriptionRef.get(backlog)
      const restored = point(state, entries.length, log.length)
      history = []
      return restored
    })

    return {
      begin: lock.withPermits(1)(begin),
      send: (input: Input) => lock.withPermits(1)(submit(input, true)),
      current: lock.withPermits(1)(currentPoint),
      rollback: (steps: number) => lock.withPermits(1)(rollback(steps)),
      snapshot: lock.withPermits(1)(snapshot),
      restore: (save: SaveRuntime) => lock.withPermits(1)(restore(save)),
      // Uninterruptible: a raise taken off the queue is always sent.
      deliverRaise: lock.withPermits(1)(Effect.uninterruptible(deliverRaise)),
      backlog,
      idle: Effect.succeed(idle(options.bindings)),
      fingerprint: runtime.fingerprint
    }
  }))

/** Layer helper that supplies a runtime Session created by the host. */
export const live = layer
