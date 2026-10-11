/** Runtime-backed player session with rollback, saves, and backlog. */
import { Context, Effect, Layer, Schema, SubscriptionRef } from "effect"
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
  readonly begin: (flow: Contract.Seed) => Effect.Effect<Output, Errors.RuntimeRejection | Errors.InputLogError>
  readonly send: (input: Input) => Effect.Effect<Output, Errors.RuntimeRejection | Errors.InputLogError>
  readonly current: Effect.Effect<InputPoint>
  readonly rollback: (steps: number) => Effect.Effect<InputPoint, NothingToRollback | Errors.RuntimeRejection | Errors.InputLogError>
  readonly snapshot: Effect.Effect<SaveRuntime, Errors.RuntimeRejection>
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
  readonly seed?: Contract.Seed
  readonly settings?: Pick<Settings, "rollbackDepth" | "locale">
  /** Cast id → display name for backlog entries (narration has none). */
  readonly castNames?: ReadonlyMap<string, string>
}

const linesFromOutput = (
  output: Output,
  locale: string,
  castNames: ReadonlyMap<string, string>
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
      : castNames.get(speaker) ?? speaker
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

/** Construct the service. The supplied runtime Session starts from `seed`. */
export const layer = (
  options: SessionOptions
): Layer.Layer<PlayerSession, never, RuntimeSession.Session> =>
  Layer.effect(PlayerSession, Effect.gen(function* () {
    const runtime = yield* RuntimeSession.Session
    const seed = options.seed ?? {}
    const locale = options.settings?.locale ?? "en"
    const castNames = options.castNames ?? new Map<string, string>()
    const depth = Math.max(0, options.settings?.rollbackDepth ?? 100)
    const backlog = yield* SubscriptionRef.make<ReadonlyArray<BacklogEntry>>([])
    let log: ReadonlyArray<LogEntry> = [{ seed }]
    let history: ReadonlyArray<PointRecord> = []

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
      history = depth === 0 ? [] : [...history, { point: beforePoint, snapshot: before, log: beforeLog, backlog: beforeBacklog }].slice(-depth)
      const output = yield* runtime.send(input)
      log = [...beforeLog, { input }]
      const added = linesFromOutput(output, locale, castNames)
      yield* SubscriptionRef.set(backlog, [...beforeBacklog, ...added])
      return output
    })

    const begin = Effect.fn("Lute.Player.Session.begin")(function* (_flow: Contract.Seed) {
      const configured = options.bindings.flow?.newGame
      if (configured == null) return yield* Effect.succeed(runtime.started)
      return yield* send({ type: "raiseOccasion", occasion: configured.occasion })
    })

    const rollback = Effect.fn("Lute.Player.Session.rollback")(function* (steps: number) {
      if (!Number.isInteger(steps) || steps < 1 || steps > history.length) {
        return yield* Effect.fail(new NothingToRollback({ steps }))
      }
      const target = history[history.length - steps]
      if (target === undefined) return yield* Effect.fail(new NothingToRollback({ steps }))
      yield* runtime.restore(target.snapshot)
      log = target.log
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
      yield* SubscriptionRef.set(backlog, save.backlog)
      const state = yield* SubscriptionRef.get(runtime.current)
      const entries = yield* SubscriptionRef.get(backlog)
      const restored = point(state, entries.length, log.length)
      history = []
      return restored
    })

    return {
      begin,
      send,
      current: currentPoint,
      rollback,
      snapshot,
      restore,
      backlog,
      idle: Effect.succeed(idle(options.bindings)),
      fingerprint: runtime.fingerprint
    }
  }))

/** Layer helper that supplies a runtime Session created by the host. */
export const live = layer
