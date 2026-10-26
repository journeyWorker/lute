/**
 * Spec 0.51.0 §4.6, §5.6: the reference player's pending raise queue — drained
 * before the idle flow, one rollback step with the player input that caused
 * it, cleared by an accepted start, kept in saves — and `PluginCall`'s
 * `raises`/`localized` and locale discovery. Over the compiled `raise-queue`
 * fixture (see `compileRaiseQueue`).
 */
import { Deferred, Effect, Exit, Layer, Option, SubscriptionRef } from "effect"
import { describe, expect, it } from "@effect/vitest"
import * as LuteBun from "../../runtime/src/bun.js"
import { BundleSource } from "../../runtime/src/BundleSource.js"
import * as Errors from "../../runtime/src/Errors.js"
import type * as Contract from "../../runtime/src/generated/Contract.js"
import * as RuntimeSession from "../../runtime/src/Session.js"
import { bundleLocales } from "../src/cues/index.js"
import { NothingToRollback, PlayerSession, layer as sessionLayer } from "../src/session/index.js"
import type { PlayerSessionShape, SessionLineage } from "../src/session/index.js"
import type { Bindings } from "../src/bindings/schema.js"
import type { SaveRuntime } from "../src/model.js"
import type { PluginCall } from "../src/ports/index.js"
import { flush, withHarness } from "./director/support.js"
import type { Harness, Options } from "./director/support.js"
import { compileRaiseQueue } from "./fixtures.js"

const canvas = { width: 1920, height: 1080 }
const bindings: Bindings = { bindingsVersion: "0.50", canvas, flow: { newGame: { occasion: "visit" } } }
const seed: Contract.Seed = { derive: true }
type Start = { readonly seed: Contract.Seed } | { readonly snapshot: Contract.Snapshot }

/** A player session (with `bindings`) over a runtime session of the compiled fixture. */
const overSession = <A, E>(
  use: (session: PlayerSessionShape, runtime: RuntimeSession.SessionShape) => Effect.Effect<A, E>,
  start: Start = { seed },
  lineage?: SessionLineage
) => Effect.flatMap(compileRaiseQueue, (bundle) => {
  const player = sessionLayer({ bindings, ...("seed" in start ? { seed: start.seed } : {}), ...(lineage === undefined ? {} : { lineage }) })
  return Effect.scoped(Effect.gen(function* () {
    return yield* use(yield* PlayerSession, yield* RuntimeSession.Session)
  })).pipe(Effect.provide(player.pipe(Layer.provideMerge(LuteBun.session({ bundle, ...start })))))
})

/** How often each answering scene played (each counts itself). */
const plays = (runtime: RuntimeSession.SessionShape) => Effect.gen(function* () {
  const { state } = yield* SubscriptionRef.get(runtime.current)
  const view = (yield* runtime.program.view(state)).state
  const count = (path: string): unknown => {
    const value = view[path]
    return value !== undefined && "value" in value ? value.value : value
  }
  return { halls: count("run.halls"), yards: count("run.yards"), cellars: count("run.cellars"), vaults: count("run.vaults"), galleries: count("user.galleries") }
})

/** The pending raises, as `occasion target`. */
const pendingOf = (save: SaveRuntime): ReadonlyArray<string> =>
  save.pendingRaises.map(({ occasion, target }) => target === undefined ? occasion : `${occasion} ${target}`)

/** The `raiseOccasion` inputs of the log, as `occasion target`. */
const raisesLogged = (save: SaveRuntime): ReadonlyArray<string> => save.log.flatMap((entry) =>
  "input" in entry && entry.input.type === "raiseOccasion" ? [`${entry.input.occasion}${entry.input.target == null ? "" : ` ${entry.input.target}`}`] : [])

const choose = (session: PlayerSessionShape, option: string) => Effect.gen(function* () {
  const { await: waiting } = yield* session.current
  if (waiting.type !== "awaitChoice") throw new Error(`not at a choice: ${waiting.type}`)
  return yield* session.send({ type: "choose", request: waiting.request, option })
})

const failure = <A, E>(exit: Exit.Exit<A, E>) => Option.getOrUndefined(Exit.findErrorOption(exit))
const hall = { occasion: "cutscene", target: "scene.hall", document: "porch.lute", position: "001-0300" }
const yard = { occasion: "cutscene", target: "scene.yard", document: "porch.lute", position: "001-0400" }
const none = { halls: 0, yards: 0, cellars: 0, vaults: 0, galleries: 0 }

describe("the pending raise queue in the session (0.51.0 §4.6)", () => {
  it.effect("a player input and its raises are one rollback step: rollback(1) restores (W0, []) however many were delivered; replaying regenerates them and delivers each once", () => overSession((session, runtime) => Effect.gen(function* () {
    yield* session.begin
    const w0 = yield* session.snapshot
    expect(w0.pendingRaises).toEqual([])
    const p = w0.log.length
    yield* choose(session, "in")
    const w1 = yield* session.snapshot
    expect(w1.pendingRaises).toEqual([{ ...hall, logIndex: p }, { ...yard, logIndex: p }])
    // Neither delivered.
    yield* session.rollback(1)
    expect(yield* session.snapshot).toEqual(w0)
    // Replayed: the same raises again; the first delivered, the second pending.
    yield* choose(session, "in")
    expect((yield* session.snapshot).pendingRaises).toEqual(w1.pendingRaises)
    expect(yield* session.deliverRaise).toBeDefined()
    expect(pendingOf(yield* session.snapshot)).toEqual(["cutscene scene.yard"])
    expect(yield* plays(runtime)).toEqual({ ...none, halls: 1 })
    yield* session.rollback(1)
    expect(yield* session.snapshot).toEqual(w0)
    expect(yield* plays(runtime)).toEqual(none)
    // Both delivered, each once.
    yield* choose(session, "in")
    expect(yield* session.deliverRaise).toBeDefined()
    expect(yield* session.deliverRaise).toBeDefined()
    expect(yield* session.deliverRaise).toBeUndefined()
    const delivered = yield* session.snapshot
    expect(delivered.pendingRaises).toEqual([])
    expect(raisesLogged(delivered)).toEqual(["visit", "cutscene scene.hall", "cutscene scene.yard"])
    expect(yield* plays(runtime)).toEqual({ ...none, halls: 1, yards: 1 })
    // The history grew by one entry for P and its raises: back to W0, then
    // to before `begin`'s own `flow.newGame` raise, then nothing.
    yield* session.rollback(1)
    expect(yield* session.snapshot).toEqual(w0)
    yield* session.rollback(1)
    expect(failure(yield* Effect.exit(session.rollback(1)))).toBeInstanceOf(NothingToRollback)
  })))

  it.effect("a raise the runtime rejects is logged and dropped; nothing is delivered while an input waits", () => overSession((session, runtime) => Effect.gen(function* () {
    yield* session.begin
    yield* choose(session, "vault")
    expect(pendingOf(yield* session.snapshot)).toEqual(["vault", "cutscene scene.yard"])
    // `vault` is closed by its `raisedWhen`: dropped, and `yard` goes.
    expect(yield* session.deliverRaise).toBeDefined()
    const after = yield* session.snapshot
    expect(after.pendingRaises).toEqual([])
    expect(raisesLogged(after)).toEqual(["visit", "cutscene scene.yard"])
    expect(yield* plays(runtime)).toEqual({ ...none, yards: 1 })
    expect(yield* session.deliverRaise).toBeUndefined()
    // At a bridge await the queue waits.
    yield* session.send({ type: "raiseOccasion", occasion: "visit" })
    yield* choose(session, "ask")
    expect((yield* session.current).await.type).toBe("awaitBridge")
    expect(yield* session.deliverRaise).toBeUndefined()
    expect(pendingOf(yield* session.snapshot)).toEqual(["cutscene scene.yard"])
  })))

  it.effect("an accepted newRun clears the queue and enqueues its own raises once; a rejected one keeps it", () => overSession((session, runtime) => Effect.gen(function* () {
    yield* session.begin
    yield* choose(session, "fanfare")
    expect(pendingOf(yield* session.snapshot)).toEqual(["cutscene scene.gallery"])
    yield* session.send({ type: "newRun" })
    const started = yield* session.snapshot
    expect(started.pendingRaises).toEqual([{ occasion: "cutscene", target: "scene.gallery", document: "fanfare.lute", logIndex: started.log.length - 1 }])
    expect(started.log[started.log.length - 1]).toEqual({ input: { type: "newRun" } })
    expect(yield* session.deliverRaise).toBeDefined()
    expect(yield* session.deliverRaise).toBeUndefined()
    expect((yield* plays(runtime)).galleries).toBe(1)
    yield* session.send({ type: "raiseOccasion", occasion: "visit" })
    yield* choose(session, "ask")
    const waiting = yield* session.snapshot
    expect(pendingOf(waiting)).toEqual(["cutscene scene.yard"])
    expect(failure(yield* Effect.exit(session.send({ type: "newRun" })))).toBeInstanceOf(Errors.RuntimeBusy)
    expect((yield* session.snapshot).pendingRaises).toEqual(waiting.pendingRaises)
  })))

  it.effect("a New game back to the start point or through the lineage clears the queue and enqueues the start output's raises once", () => Effect.gen(function* () {
    // A run whose `user.fanfare` is set, at idle: its `newRun` grants `gallery`.
    const fanfare = yield* overSession((session, runtime) => Effect.gen(function* () {
      yield* session.begin
      yield* choose(session, "fanfare")
      expect(yield* session.deliverRaise).toBeDefined()
      return yield* runtime.save
    }))
    expect(fanfare.await.type).toBe("idle")
    const replaced = (session: PlayerSessionShape) => Effect.gen(function* () {
      yield* session.begin
      yield* choose(session, "ask")
      const before = pendingOf(yield* session.snapshot)
      expect(before.at(-1)).toBe("cutscene scene.yard")
      // At the bridge `newRun` cannot run: the start point (or the lineage) replaces the run.
      yield* session.begin
      const after = yield* session.snapshot
      expect(after.log.map((entry) => "input" in entry ? entry.input.type : "seed" in entry ? "seed" : "snapshot")).toEqual(["snapshot", "newRun", "raiseOccasion"])
      expect(after.pendingRaises).toEqual([{ occasion: "cutscene", target: "scene.gallery", document: "fanfare.lute", logIndex: 1 }])
      return before
    })
    // The start point: a session built from that snapshot.
    expect(yield* overSession(replaced, { snapshot: fanfare })).toEqual(["cutscene scene.yard"])
    // The lineage: its first New game already enqueued `gallery` once.
    const lineage: SessionLineage = { get: Effect.succeed(fanfare), drop: Effect.void }
    expect(yield* overSession(replaced, { seed }, lineage)).toEqual(["cutscene scene.gallery", "cutscene scene.yard"])
  }))
})

/** Advance through lines until the story waits for something else. */
const settle = (h: Harness) => Effect.gen(function* () {
  for (let i = 0; i < 100; i++) {
    const view = yield* h.view
    if (view.story !== "typing" && view.story !== "shown" && view.story !== "running") return
    yield* h.action({ type: "advance" })
  }
  throw new Error("director did not settle")
})
const lineText = (h: Harness) => h.lines.map((line) => line.segments.map((segment) => segment._tag === "Text" ? segment.text : "").join(""))

/** A harness over the fixture whose `host` may read the harness (set before the first action). */
const overDirector = <A, E, R>(options: (current: () => Harness) => Options, use: (h: Harness) => Effect.Effect<A, E, R>) =>
  Effect.flatMap(compileRaiseQueue, (bundle) => {
    let harness: Harness | undefined
    const current = (): Harness => {
      if (harness === undefined) throw new Error("the harness is not built yet")
      return harness
    }
    return withHarness(bundle, options(current), (h) => {
      harness = h
      return use(h)
    })
  })

describe("the director drains the queue (0.51.0 §4.6)", () => {
  const idleMenu: Bindings = { ...bindings, flow: { newGame: { occasion: "visit" }, idle: { menu: [{ label: "Visit", occasion: "visit" }] } } }
  /** At each host `idle`: what played and how many raises were pending. */
  const atIdle = (seen: Array<unknown>) => (current: () => Harness) => ({
    idle: () => Effect.gen(function* () {
      const h = current()
      seen.push({ ...(yield* plays(h.runtime)), pending: (yield* h.session.snapshot).pendingRaises.length })
      return undefined
    }).pipe(Effect.orDie)
  })

  it.effect("before the host idle and the bindings' idle flow, each raise once; rollback undoes the input with its raises", () => {
    const seen: Array<unknown> = []
    return overDirector((current) => ({ bindings: idleMenu, choices: ["in"], host: atIdle(seen)(current) }), (h) => Effect.gen(function* () {
      yield* h.action({ type: "newGame" })
      yield* settle(h)
      expect((yield* h.view).story).toBe("idle")
      expect(lineText(h)).toEqual(["The porch is quiet.", "The hall.", "The yard."])
      expect(seen).toEqual([{ ...none, halls: 1, yards: 1, pending: 0 }])
      expect(h.screens.at(-1)).toEqual({ _tag: "Idle", options: [{ label: "Visit" }] })
      expect(raisesLogged(yield* h.director.snapshot)).toEqual(["visit", "cutscene scene.hall", "cutscene scene.yard"])
      yield* h.action({ type: "rollback" })
      expect((yield* h.view).story).toBe("choice")
      expect(yield* plays(h.runtime)).toEqual(none)
      const back = yield* h.director.snapshot
      expect(back.pendingRaises).toEqual([])
      expect(raisesLogged(back)).toEqual(["visit"])
    }))
  })

  it.effect("a rejected raise is dropped and the drain goes on", () => {
    const seen: Array<unknown> = []
    return overDirector((current) => ({ bindings, choices: ["vault"], host: atIdle(seen)(current) }), (h) => Effect.gen(function* () {
      yield* h.action({ type: "newGame" })
      yield* settle(h)
      expect(lineText(h)).toEqual(["The porch is quiet.", "The yard."])
      expect(seen).toEqual([{ ...none, yards: 1, pending: 0 }])
    }))
  })

  it.effect("PluginCall carries the declared raises and each text attribute in the current locale; locale discovery reads plugin translations structurally", () => {
    const calls: Array<PluginCall> = []
    const choices = ["in"]
    return overDirector(() => ({ bindings, choices, host: { plugin: { playScene: (call) => Effect.sync(() => { calls.push(call) }) } } }), (h) => Effect.gen(function* () {
      // `texts` and `promptTexts` are attribute names here, never translation maps.
      expect(bundleLocales(yield* (yield* BundleSource).load)).toEqual({ source: "en", locales: ["en", "ko"] })
      yield* h.action({ type: "newGame" })
      yield* settle(h)
      yield* h.action({ type: "setSettings", settings: { locale: "ko" } })
      choices.push("in")
      yield* h.action({ type: "rollback" })
      yield* settle(h)
      const call = (localized: Readonly<Record<string, string>>): PluginCall => ({
        tag: "playScene",
        attrs: { scene: "hall", title: "Welcome, Ada", texts: "Hall texts", promptTexts: "Hall prompt" },
        raises: [{ occasion: "cutscene", target: "scene.hall" }],
        localized,
        document: "porch.lute",
        position: hall.position
      })
      const plain: PluginCall = { tag: "playScene", attrs: { scene: "yard" }, raises: [{ occasion: "cutscene", target: "scene.yard" }], document: "porch.lute", position: yard.position }
      expect(calls).toEqual([
        call({ title: "Welcome, Ada", texts: "Hall texts", promptTexts: "Hall prompt" }),
        plain,
        call({ title: "Ada님, 환영합니다", texts: "홀 텍스트", promptTexts: "홀 프롬프트" }),
        plain
      ])
      expect("localized" in plain).toBe(false)
    }))
  })
})

describe("the pending raise queue in saves (0.51.0 §4.6)", () => {
  it.effect("a SaveStore round trip at a bridge await and at a choice await: after the load each pending raise is sent exactly once", () => Effect.gen(function* () {
    // Play to the bridge, save; answer it, play to the next choice, save.
    const [atBridge, atChoice] = yield* Effect.flatMap(Deferred.make<void>(), (answer) => overDirector(() => ({
      bindings,
      choices: ["ask"],
      host: { bridge: { ask: () => Effect.as(Deferred.await(answer), { fields: { answer: "yes" } }) } }
    }), (h) => Effect.gen(function* () {
      const saveAs = (slot: number) => Effect.gen(function* () {
        yield* h.saves.save(slot, { project: h.session.fingerprint, label: `slot ${slot}`, runtime: yield* h.director.snapshot })
        return (yield* h.saves.load(slot)).runtime
      })
      yield* h.action({ type: "newGame" })
      yield* settle(h)
      expect((yield* h.view).story).toBe("bridge")
      const bridge = yield* saveAs(1)
      yield* Deferred.succeed(answer, undefined)
      yield* flush
      yield* settle(h)
      expect((yield* h.view).story).toBe("choice")
      const choice = yield* saveAs(2)
      return [bridge, choice] as const
    })))
    expect(pendingOf(atBridge)).toEqual(["cutscene scene.yard"])
    expect(pendingOf(atChoice)).toEqual(["cutscene scene.yard", "cutscene scene.cellar"])
    for (const save of [atBridge, atChoice]) {
      const choices: Array<string> = []
      yield* overDirector(() => ({ bindings, choices, host: { bridge: { ask: () => Effect.succeed({ fields: { answer: "yes" } }) } } }), (h) => Effect.gen(function* () {
        yield* h.action({ type: "newGame" })
        yield* settle(h)
        choices.push("leave")
        yield* h.director.restore(save)
        yield* flush
        yield* settle(h)
        expect((yield* h.view).story).toBe("idle")
        expect(yield* plays(h.runtime)).toEqual({ ...none, yards: 1, cellars: 1 })
        const after = yield* h.director.snapshot
        expect(after.pendingRaises).toEqual([])
        expect(raisesLogged(after)).toEqual(["visit", "cutscene scene.yard", "cutscene scene.cellar"])
      }))
    }
  }))
})
