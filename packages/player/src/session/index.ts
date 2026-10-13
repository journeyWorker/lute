/** Runtime-backed player session with rollback, saves, and backlog. */
import { Context, Effect, Layer, Schema, Semaphore, SubscriptionRef } from "effect"
import { Session as RuntimeSession } from "@lute-lang/runtime"
import type { Contract, Errors } from "@lute-lang/runtime"
import type { Bindings, Input, InputPoint, LogEntry, Output, SaveRuntime } from "../model.js"
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
}

/** Session service contract used by the director. */
export interface PlayerSessionShape {
  /**
   * Start the story (spec 0.41.0 §7.3). While the session has accepted no
   * input since it was built, the runtime session's `started` output; after
   * that a new run: `newRun`. Either is followed by the bound `flow.newGame`
   * raise when there is one, the events concatenated in order with the last
   * await. A session that waits where `newRun` is refused (halted, at a choice
   * or a bridge) is first restored to the snapshot captured when the player
   * session was built.
   */
  readonly begin: Effect.Effect<Output, Errors.RuntimeRejection | Errors.InputLogError>
  readonly send: (input: Input) => Effect.Effect<Output, Errors.RuntimeRejection | Errors.InputLogError>
  readonly current: Effect.Effect<InputPoint>
  readonly rollback: (steps: number) => Effect.Effect<InputPoint, NothingToRollback | Errors.RuntimeRejection | Errors.InputLogError>
  readonly snapshot: Effect.Effect<SaveRuntime, Errors.RuntimeRejection>
  /** Continue from a save: runtime snapshot, input log and backlog; the rollback history is cleared. */
  readonly restore: (save: SaveRuntime) => Effect.Effect<InputPoint, SaveIncompatible | Errors.RuntimeRejection | Errors.InputLogError>
  readonly backlog: SubscriptionRef.SubscriptionRef<ReadonlyArray<BacklogEntry>>
  readonly idle: Effect.Effect<IdleResult | undefined>
  readonly fingerprint: string
}

/** Runtime-backed player session. Provide the runtime Session layer when composing. */
export class PlayerSession extends Context.Service<PlayerSession, PlayerSessionShape>()(
  "@lute-lang/player/PlayerSession"
) {}

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
  readonly settings?: Pick<Settings, "rollbackDepth" | "locale">
  /** The bundle index `cast` (cast id → display name) for backlog names; narration has none. */
  readonly cast?: Readonly<Record<string, string>>
}

const linesFromOutput = (
  output: Output,
  locale: string,
  cast: Readonly<Record<string, string>>
): ReadonlyArray<BacklogEntry> => {
  const result: BacklogEntry[] = []
  for (const event of output.events) {
    if (event.type !== "record" || typeof event.record !== "object" || event.record === null) continue
    const entries = Object.entries(event.record)
    const value = (key: string): unknown => entries.find(([name]) => name === key)?.[1]
    if (value("kind") !== "line") continue
    const lineIdValue = value("lineId")
    if (typeof lineIdValue !== "string") continue
    const textValue = value("text")
    let text = typeof textValue === "string" ? textValue : ""
    const texts = value("texts")
    if (typeof texts === "object" && texts !== null && !Array.isArray(texts)) {
      const localized = Object.entries(texts).find(([key, item]) => key === locale && typeof item === "string")?.[1]
      if (typeof localized === "string") text = localized
    }
    const speaker = value("speaker")
    const voice = value("voiceKey")
    const role = value("role")
    const speakerName = typeof speaker !== "string" || role === "narration" || speaker === "narrator"
      ? ""
      : cast[speaker] ?? speaker
    const voiceKey = typeof voice === "string" ? voice : ""
    result.push({ lineId: lineIdValue, voiceKey, speakerName, text, hasVoice: voiceKey.length > 0 })
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
    const locale = options.settings?.locale ?? "en"
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
    // No input accepted since the player session was built: `begin` presents `started`.
    let fresh = true

    const currentPoint = Effect.gen(function* () {
      const state = yield* SubscriptionRef.get(runtime.current)
      const entries = yield* SubscriptionRef.get(backlog)
      return point(state, entries.length, log.length)
    })

    const send = Effect.fn("Lute.Player.Session.send")(function* (input: Input) {
      const before = yield* runtime.save
      const beforeState = yield* SubscriptionRef.get(runtime.current)
      const beforeBacklog = yield* SubscriptionRef.get(backlog)
      const beforeLog = log
      const beforePoint = point(beforeState, beforeBacklog.length, beforeLog.length)
      const output = yield* runtime.send(input)
      history = depth === 0 ? [] : [...history, { point: beforePoint, snapshot: before, log: beforeLog, backlog: beforeBacklog }].slice(-depth)
      log = [...beforeLog, { input }]
      fresh = false
      const added = linesFromOutput(output, locale, cast)
      yield* SubscriptionRef.set(backlog, [...beforeBacklog, ...added])
      return output
    })

    const begin = Effect.gen(function* () {
      let fromStart = fresh
      if (!fresh && !acceptsNewRun((yield* SubscriptionRef.get(runtime.current)).await)) {
        yield* runtime.restore(start)
        log = [startEntry]
        history = []
        yield* SubscriptionRef.set(backlog, [])
        fromStart = !acceptsNewRun(start.await)
      }
      const first = fromStart ? runtime.started : yield* send({ type: "newRun" })
      const newGame = options.bindings.flow?.newGame
      if (newGame == null) return first
      const raised = yield* send({ type: "raiseOccasion", occasion: newGame.occasion, ...(newGame.target == null ? {} : { target: newGame.target }) })
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
      fresh = false
      yield* SubscriptionRef.set(backlog, target.backlog)
      history = history.slice(0, history.length - steps)
      return target.point
    })

    const snapshot = Effect.gen(function* () {
      const runtimeSnapshot = yield* runtime.save
      const entries = yield* SubscriptionRef.get(backlog)
      return { snapshot: runtimeSnapshot, log, backlog: entries } satisfies SaveRuntime
    })

    const restore = Effect.fn("Lute.Player.Session.restore")(function* (save: SaveRuntime) {
      if (save.snapshot.project !== undefined && save.snapshot.project !== runtime.fingerprint) {
        return yield* Effect.fail(new SaveIncompatible({ message: "save belongs to another project" }))
      }
      yield* runtime.restore(save.snapshot)
      log = save.log
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
      send: (input: Input) => lock.withPermits(1)(send(input)),
      current: lock.withPermits(1)(currentPoint),
      rollback: (steps: number) => lock.withPermits(1)(rollback(steps)),
      snapshot: lock.withPermits(1)(snapshot),
      restore: (save: SaveRuntime) => lock.withPermits(1)(restore(save)),
      backlog,
      idle: Effect.succeed(idle(options.bindings)),
      fingerprint: runtime.fingerprint
    }
  }))

/** Layer helper that supplies a runtime Session created by the host. */
export const live = layer
