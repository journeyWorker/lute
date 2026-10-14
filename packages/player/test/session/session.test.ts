import { Effect, Layer, SubscriptionRef } from "effect"
import * as Contract from "../../../runtime/src/generated/Contract.js"
import * as InputLog from "../../../runtime/src/InputLog.js"
import * as LuteBun from "../../../runtime/src/bun.js"
import * as RuntimeSession from "../../../runtime/src/Session.js"
import { describe, expect, it } from "@effect/vitest"
import { PlayerSession, NothingToRollback, SaveIncompatible, idle, layer as playerLayer } from "../../src/session/index.js"
import { canonical, compileBundle, hubFixtureRoot, repositoryRoot, sessionRoot } from "../../../runtime/test/support.js"
import { join } from "node:path"

const started: Contract.Seed = { derive: true }
const bindings = { bindingsVersion: "0.43", canvas: { width: 1920, height: 1080 }, flow: { newGame: { occasion: "visit" } } }
const noFlowBindings = { bindingsVersion: "0.43", canvas: { width: 1920, height: 1080 } }
const runtimeLayer = (bundle: string, seed = started) => RuntimeSession.layer({ seed }).pipe(
  Layer.provide(Layer.mergeAll(LuteBun.runtime, LuteBun.bundle(bundle))),
  Layer.provideMerge(InputLog.memory)
)
const sessionLayer = (bundle: string, options: Parameters<typeof playerLayer>[0] = { bindings }) =>
  playerLayer(options).pipe(Layer.provide(runtimeLayer(bundle, options.seed ?? started)))
const hub = () => runtimeLayer(hubFixtureRoot)
/** The player session with its runtime session in context too. */
const withRuntime = (bundle: string, options: Parameters<typeof playerLayer>[0]) =>
  playerLayer(options).pipe(Layer.provideMerge(runtimeLayer(bundle, options.seed ?? started)))
const gateFixture = join(repositoryRoot, "crates", "lute-cli", "tests", "fixtures", "runtime-gate-041")
const stateOf = (path: string) => Effect.gen(function* () {
  const runtime = yield* RuntimeSession.Session
  const { state } = yield* SubscriptionRef.get(runtime.current)
  return (yield* runtime.program.view(state)).state[path]
})

const choose = (option: string, request: number) => ({ option, request, type: "choose" as const })
const raise = { occasion: "visit", type: "raiseOccasion" as const }

describe("PlayerSession", () => {
  it.effect("begin seeds the runtime and raises the configured newGame occasion", () => Effect.scoped(Effect.gen(function* () {
    const session = yield* PlayerSession
    const output = yield* session.begin
    expect(output.await.type).toBe("awaitChoice")
    expect(output.events.some((event) => event.type === "presentation")).toBe(true)
  })).pipe(Effect.provide(sessionLayer(hubFixtureRoot))))

  it.effect("send choose advances the await", () => Effect.scoped(Effect.gen(function* () {
    const session = yield* PlayerSession
    yield* session.begin
    const output = yield* session.send(choose("take", 1))
    expect(output.await).toMatchObject({ type: "awaitChoice", request: 2 })
  })).pipe(Effect.provide(sessionLayer(hubFixtureRoot))))

  it.effect("rollback restores the previous await, stage and backlog length", () => Effect.scoped(Effect.gen(function* () {
    const session = yield* PlayerSession
    const first = yield* session.begin
    yield* session.send(choose("take", 1))
    const before = yield* session.current
    const rolled = yield* session.rollback(1)
    expect(rolled.await).toEqual(first.await)
    expect(rolled.stage).toEqual(before.stage)
    expect(rolled.backlogLength).toBe(1)
  })).pipe(Effect.provide(sessionLayer(hubFixtureRoot))))

  it.effect("rollback enforces the configured depth cap", () => Effect.scoped(Effect.gen(function* () {
    const session = yield* PlayerSession
    yield* session.begin
    yield* session.send(choose("take", 1))
    yield* session.send(choose("yes", 2))
    yield* session.rollback(1)
    yield* session.rollback(1)
    const error = yield* Effect.flip(session.rollback(1))
    expect(error).toBeInstanceOf(NothingToRollback)
  })).pipe(Effect.provide(sessionLayer(hubFixtureRoot, { bindings, settings: { locale: "en", rollbackDepth: 2 } }))))

  it.effect("rollback at the start fails with NothingToRollback", () => Effect.scoped(Effect.gen(function* () {
    const session = yield* PlayerSession
    const error = yield* Effect.flip(session.rollback(1))
    expect(error).toBeInstanceOf(NothingToRollback)
  })).pipe(Effect.provide(sessionLayer(hubFixtureRoot))))

  it.effect("snapshot restore round trips and preserves the next output", () => Effect.scoped(Effect.gen(function* () {
    const session = yield* PlayerSession
    yield* session.begin
    yield* session.send(choose("take", 1))
    const point = yield* session.current
    const save = yield* session.snapshot
    const original = yield* session.send(choose("yes", 2))
    const restored = yield* session.restore(save)
    expect(restored.await).toEqual(save.snapshot.await)
    expect(restored.stage).toEqual(point.stage)
    expect(restored.backlogLength).toBe(save.backlog.length)
    const replay = yield* session.send(choose("yes", 2))
    expect(canonical(replay)).toBe(canonical(original))
  })).pipe(Effect.provide(sessionLayer(hubFixtureRoot))))

  it.effect("restoring a save from another program fails with SaveIncompatible", () => Effect.scoped(Effect.gen(function* () {
    const session = yield* PlayerSession
    const save = yield* session.snapshot
    const incompatible = { ...save, snapshot: { ...save.snapshot, project: "other-project" } }
    const error = yield* Effect.flip(session.restore(incompatible))
    expect(error).toBeInstanceOf(SaveIncompatible)
  })).pipe(Effect.provide(sessionLayer(hubFixtureRoot))))

  it.effect("timeout input selects the conformance default", () => Effect.gen(function* () {
    const bundle = yield* compileBundle(join(sessionRoot, "timed-choice", "project"))
    return yield* Effect.scoped(Effect.gen(function* () {
      const session = yield* PlayerSession
      yield* session.send(raise)
      const output = yield* session.send({ request: 1, type: "timeout" })
      expect(output.await.type).toBe("idle")
      expect(output.events.some((event) => event.type === "record" && typeof event.record === "object" && event.record !== null && "text" in event.record && event.record.text === "The knocking stops.")).toBe(true)
    }).pipe(Effect.provide(sessionLayer(bundle, { bindings: noFlowBindings }))))
  }))

  it.effect("stage snapshot inputs expose background actor and music at the choice point", () => Effect.gen(function* () {
    const bundle = yield* compileBundle(join(sessionRoot, "stage-snapshot", "project"))
    return yield* Effect.scoped(Effect.gen(function* () {
      const session = yield* PlayerSession
      yield* session.send(raise)
      const current = yield* session.current
      expect(current.stage.bg).toMatchObject({ location: "parlor" })
      expect(current.stage.actors.some((actor) => actor.character === "mira")).toBe(true)
      expect(current.stage.music).toMatchObject({ assetId: "BGM.parlor" })
      const output = yield* session.send(choose("yes", 1))
      expect(output.await.type).toBe("idle")
    }).pipe(Effect.provide(sessionLayer(bundle, { bindings: noFlowBindings }))))
  }))

  it.effect("backlog entries include speaker name and text", () => Effect.scoped(Effect.gen(function* () {
    const session = yield* PlayerSession
    yield* session.send(raise)
    const entries = yield* SubscriptionRef.get(session.backlog)
    expect(entries[0]).toMatchObject({ speakerName: "", text: "Before the hub." })
  })).pipe(Effect.provide(sessionLayer(hubFixtureRoot, { bindings: noFlowBindings }))))

  it("idle resolves raise and menu forms", () => {
    expect(idle({ bindingsVersion: "0.43", canvas: { width: 1, height: 1 }, flow: { idle: { raise: { occasion: "visit", target: "door" } } } })).toEqual({ _tag: "Input", input: { type: "raiseOccasion", occasion: "visit", target: "door" } })
    expect(idle({ bindingsVersion: "0.43", canvas: { width: 1, height: 1 }, flow: { idle: { menu: [{ label: "Visit", occasion: "visit" }] } } })).toEqual({ _tag: "Menu", options: [{ label: "Visit", input: { type: "raiseOccasion", occasion: "visit" } }] })
  })

  it.effect("backlog names come from the index cast", () => Effect.gen(function* () {
    const bundle = yield* compileBundle(join(sessionRoot, "stage-snapshot", "project"))
    return yield* Effect.scoped(Effect.gen(function* () {
      const session = yield* PlayerSession
      yield* session.send(raise)
      const entries = yield* SubscriptionRef.get(session.backlog)
      expect(entries.map((entry) => entry.speakerName)).toContain("Mira")
    }).pipe(Effect.provide(sessionLayer(bundle, { bindings: noFlowBindings, cast: { mira: "Mira" } }))))
  }))

  it.effect("begin presents started once, then starts a new run keeping user progress (§7.3)", () => Effect.gen(function* () {
    const bundle = yield* compileBundle(join(sessionRoot, "new-run", "project"))
    return yield* Effect.scoped(Effect.gen(function* () {
      const session = yield* PlayerSession
      const runtime = yield* RuntimeSession.Session
      const first = yield* session.begin
      expect(canonical(first.events.slice(0, runtime.started.events.length))).toBe(canonical(runtime.started.events))
      expect(first.events.some((event) => event.type === "presentation")).toBe(true)
      expect(yield* stateOf("user.value")).toEqual({ kind: "int", value: 1 })
      const second = yield* session.begin
      // `started` is not replayed: a new run, then the newGame raise.
      expect(yield* stateOf("user.value")).toEqual({ kind: "int", value: 2 })
      expect(second.await.type).toBe("idle")
      expect(second.events.filter((event) => event.type === "presentation")).toHaveLength(1)
      const save = yield* session.snapshot
      expect(save.log.map((entry) => "input" in entry ? entry.input.type : "seed" in entry ? "seed" : "snapshot"))
        .toEqual(["seed", "raiseOccasion", "newRun", "raiseOccasion"])
    }).pipe(Effect.provide(withRuntime(bundle, { bindings, seed: started }))))
  }))

  it.effect("begin twice without a newGame binding sends newRun the second time", () => Effect.gen(function* () {
    const bundle = yield* compileBundle(join(sessionRoot, "new-run", "project"))
    return yield* Effect.scoped(Effect.gen(function* () {
      const session = yield* PlayerSession
      const runtime = yield* RuntimeSession.Session
      expect(yield* session.begin).toBe(runtime.started)
      expect(yield* session.begin).toBe(runtime.started)
      yield* session.send(raise)
      const again = yield* session.begin
      expect(again).not.toBe(runtime.started)
      expect((yield* session.snapshot).log.at(-1)).toEqual({ input: { type: "newRun" } })
    }).pipe(Effect.provide(withRuntime(bundle, { bindings: noFlowBindings, seed: started }))))
  }))

  it.effect("begin after a halt restores the start snapshot, then starts a new run (§7.3)", () => Effect.gen(function* () {
    const bundle = yield* compileBundle(gateFixture)
    return yield* Effect.scoped(Effect.gen(function* () {
      const session = yield* PlayerSession
      yield* session.begin
      // `run.noise` has no value: the gate is undecided and the run halts.
      yield* session.send({ type: "raiseOccasion", occasion: "listen" })
      const point = yield* session.current
      expect(point.await.type).toBe("halted")
      const output = yield* session.begin
      expect(output.await.type).toBe("idle")
      expect((yield* session.snapshot).log).toEqual([{ seed: started }, { input: { type: "newRun" } }])
      expect(yield* SubscriptionRef.get(session.backlog)).toEqual([])
    }).pipe(Effect.provide(withRuntime(bundle, { bindings: noFlowBindings, seed: started }))))
  }))

  it.effect("begin at a choice returns to the start first", () => Effect.scoped(Effect.gen(function* () {
    const session = yield* PlayerSession
    const first = yield* session.begin
    const second = yield* session.begin
    expect(second.await).toEqual(first.await)
  })).pipe(Effect.provide(sessionLayer(hubFixtureRoot, { bindings, seed: started }))))

  it.effect("a session started from a snapshot logs { snapshot } first (§7.1)", () => Effect.gen(function* () {
    const save = yield* Effect.scoped(Effect.gen(function* () {
      const session = yield* PlayerSession
      yield* session.send(raise)
      return yield* session.snapshot
    })).pipe(Effect.provide(sessionLayer(hubFixtureRoot, { bindings: noFlowBindings, seed: started })))
    expect(save.log[0]).toEqual({ seed: started })
    const restored = RuntimeSession.layer({ snapshot: save.snapshot }).pipe(
      Layer.provide(Layer.mergeAll(LuteBun.runtime, LuteBun.bundle(hubFixtureRoot))),
      Layer.provideMerge(InputLog.memory)
    )
    yield* Effect.scoped(Effect.gen(function* () {
      const session = yield* PlayerSession
      const log = (yield* session.snapshot).log
      expect(log).toEqual([{ snapshot: save.snapshot }])
      // Fresh: begin presents the restored session's `started` (no events, the snapshot's await).
      const output = yield* session.begin
      expect(output.events).toEqual([])
      expect(output.await).toEqual(save.snapshot.await)
    })).pipe(Effect.provide(playerLayer({ bindings: noFlowBindings }).pipe(Layer.provide(restored))))
  }))

  it.effect("a refused input changes neither the log nor the rollback history", () => Effect.scoped(Effect.gen(function* () {
    const session = yield* PlayerSession
    yield* session.begin
    const before = yield* session.snapshot
    yield* Effect.flip(session.send(choose("missing", 99)))
    expect((yield* session.snapshot).log).toEqual(before.log)
    const error = yield* Effect.flip(session.rollback(2))
    expect(error).toBeInstanceOf(NothingToRollback)
  })).pipe(Effect.provide(sessionLayer(hubFixtureRoot, { bindings, seed: started }))))
})
