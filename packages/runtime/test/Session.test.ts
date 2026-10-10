import { Context, Effect, Exit, Layer, Scope, Stream, SubscriptionRef } from "effect"
import { describe, expect, it } from "@effect/vitest"
import * as Contract from "../src/generated/Contract.js"
import * as InputLog from "../src/InputLog.js"
import * as LuteBun from "../src/bun.js"
import * as Session from "../src/Session.js"
import { canonical, decode, hubFixtureRoot } from "./support.js"

const input = decode(Contract.Input)
const started = decode(Contract.Seed)({ derive: true })
const raise = input({ occasion: "visit", type: "raiseOccasion" })
const take = input({ option: "take", request: 1, type: "choose" })
const sessionLayer = (options: Session.SessionOptions) => Session.layer(options).pipe(
  Layer.provide(Layer.mergeAll(LuteBun.runtime, LuteBun.bundle(hubFixtureRoot))),
  Layer.provideMerge(InputLog.memory)
)

describe("Session", () => {
  it.effect("serializes concurrent sends and logs them in call order", () => Effect.gen(function* () {
    const session = yield* Session.Session
    const outputs = yield* Effect.all([session.send(raise), session.send(take)], { concurrency: 2 })
    expect(outputs).toHaveLength(2)
    expect(outputs[0]?.await).toMatchObject({ type: "awaitChoice", request: 1 })
    expect(outputs[1]?.await).toMatchObject({ type: "awaitChoice", request: 2 })
    const log = yield* InputLog.InputLog
    expect(yield* log.entries).toEqual([{ seed: started }, { input: raise }, { input: take }])
  }).pipe(Effect.provide(sessionLayer({ seed: started }))))

  it.effect("leaves current and log unchanged after a rejected send", () => Effect.gen(function* () {
    const session = yield* Session.Session
    const before = yield* SubscriptionRef.get(session.current)
    const log = yield* InputLog.InputLog
    const entries = yield* log.entries
    const rejected = yield* Effect.exit(session.send(take))
    expect(rejected._tag).toBe("Failure")
    expect(yield* SubscriptionRef.get(session.current)).toBe(before)
    expect(yield* log.entries).toEqual(entries)
  }).pipe(Effect.provide(sessionLayer({ seed: started }))))

  it.effect("publishes events and awaits to subscribers in order", () => Effect.scoped(Effect.gen(function* () {
    const session = yield* Session.Session
    const events = yield* session.events
    const awaits = yield* session.awaits
    const raised = yield* session.send(raise)
    expect(yield* Stream.runCollect(Stream.take(events, raised.events.length))).toEqual(raised.events)
    expect(yield* Stream.runCollect(Stream.take(awaits, 1))).toEqual([raised.await])
  })).pipe(Effect.provide(sessionLayer({ seed: started }))))

  it.effect("continues in a new session from a saved snapshot", () => Effect.gen(function* () {
    const session = yield* Session.Session
    yield* session.send(raise)
    const snapshot = yield* session.save
    const expected = yield* session.send(take)
    const actual = yield* Effect.gen(function* () {
      const restored = yield* Session.Session
      return yield* restored.send(take)
    }).pipe(Effect.provide(sessionLayer({ snapshot })))
    expect(canonical(actual)).toBe(canonical(expected))
  }).pipe(Effect.provide(sessionLayer({ seed: started }))))

  it.effect("resets its input log to the restored snapshot", () => Effect.gen(function* () {
    const session = yield* Session.Session
    yield* session.send(raise)
    const snapshot = yield* session.save
    yield* session.send(take)
    yield* session.restore(snapshot)
    const log = yield* InputLog.InputLog
    expect(yield* log.entries).toEqual([{ snapshot }])
    expect(canonical(yield* session.save)).toBe(canonical(snapshot))
  }).pipe(Effect.provide(sessionLayer({ seed: started }))))

  it.effect("ends event streams when its scope closes", () => Effect.gen(function* () {
    const scope = yield* Scope.make()
    const context = yield* Scope.provide(scope)(Layer.build(sessionLayer({ seed: started })))
    const session = Context.get(context, Session.Session)
    const events = yield* Effect.scoped(Effect.flatMap(session.events, (events) =>
      Effect.andThen(Scope.close(scope, Exit.void), Stream.runCollect(events))
    ))
    expect(events).toEqual([])
  }))
})
