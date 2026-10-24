/** Spec 0.41.0 §7.1: start options and the handle's session/program. */
import { Context, Effect, Layer, Stream, SubscriptionRef } from "effect"
import * as KeyValueStore from "effect/persistence/KeyValueStore"
import { describe, expect, it } from "@effect/vitest"
import * as InputLog from "../../runtime/src/InputLog.js"
import * as LuteBun from "../../runtime/src/bun.js"
import * as RuntimeSession from "../../runtime/src/Session.js"
import { canonical, hubFixtureRoot } from "../../runtime/test/support.js"
import { LutePlayer } from "../src/LutePlayer.js"
import type { AssetLoader, AudioPort, Presenter } from "../src/ports/index.js"
import { flush } from "./director/support.js"

const bindings = { bindingsVersion: "0.49", canvas: { width: 1920, height: 1080 }, flow: { newGame: { occasion: "visit" } } }
const presenter: Presenter = {
  stage: () => Effect.void, showLine: () => Effect.void, revealLine: Effect.void, clearLine: Effect.void, restore: () => Effect.void,
  showChoice: () => Effect.never, hideChoice: Effect.void, showScreen: () => Effect.void, hideScreen: Effect.void,
  setUiHidden: () => Effect.void, applySettings: () => Effect.void, capture: Effect.succeed(""), actions: Stream.empty
}
const audio: AudioPort = { play: () => Effect.void, stop: () => Effect.void, setVolumes: () => Effect.void }
const assets: AssetLoader = { ensure: () => Effect.void, progress: Stream.empty }
const ports = { bindings, presenter, audio, assets }
const host = Layer.mergeAll(LuteBun.runtime, LuteBun.bundle(hubFixtureRoot), InputLog.memory, KeyValueStore.layerMemory)

/** New game until the first choice. */
const toChoice = (player: Effect.Success<ReturnType<typeof LutePlayer.make>>) => Effect.gen(function* () {
  yield* player.actions({ type: "newGame" })
  for (let i = 0; i < 20; i++) {
    yield* flush
    const view = yield* SubscriptionRef.get(player.view)
    if (view.story === "choice") return
    yield* player.actions({ type: "advance" })
  }
  throw new Error("no choice")
})

describe("LutePlayer.make (§7.1)", () => {
  it.effect("starts from a seed and exposes the session it drives and its program", () => Effect.scoped(Effect.gen(function* () {
    const player = yield* LutePlayer.make({ ...ports, seed: { derive: true } })
    expect(player.program).toBe(player.session.program)
    expect(player.program.fingerprint).toBe(player.session.fingerprint)
    yield* toChoice(player)
    const { state, await: waiting } = yield* SubscriptionRef.get(player.session.current)
    expect(waiting.type).toBe("awaitChoice")
    expect((yield* player.program.view(state)).state).toBeDefined()
  })).pipe(Effect.provide(host)))

  it.effect("queries candidates and view at a shown choice without changing player state", () => Effect.scoped(Effect.gen(function* () {
    const player = yield* LutePlayer.make({ ...ports, seed: { derive: true } })
    yield* toChoice(player)
    const beforeView = yield* SubscriptionRef.get(player.view)
    const before = yield* SubscriptionRef.get(player.session.current)
    const beforeSnapshot = yield* player.program.snapshot(before.state)
    const candidates = yield* player.program.candidates(before.state, "visit")
    yield* player.program.view(before.state)
    expect(candidates[0]).toMatchObject({ id: "hub", kind: "scene", verdict: { verdict: "eligible" } })
    expect(yield* SubscriptionRef.get(player.view)).toEqual(beforeView)
    expect(canonical(yield* player.program.snapshot((yield* SubscriptionRef.get(player.session.current)).state))).toBe(canonical(beforeSnapshot))
  })).pipe(Effect.provide(host)))

  it.effect("starts from a snapshot", () => Effect.gen(function* () {
    const snapshot = yield* Effect.scoped(Effect.gen(function* () {
      const first = yield* LutePlayer.make(ports)
      yield* toChoice(first)
      return yield* first.session.save
    }))
    yield* Effect.scoped(Effect.gen(function* () {
      const player = yield* LutePlayer.make({ ...ports, snapshot })
      expect((yield* SubscriptionRef.get(player.session.current)).await).toEqual(snapshot.await)
      expect(player.session.started.events).toEqual([])
    }))
  }).pipe(Effect.provide(host)))

  it.effect("drives a host session without closing it; only the bundle and the key-value store are required", () => Effect.gen(function* () {
    const runtime = Context.get(
      yield* Layer.build(RuntimeSession.layer({ seed: { derive: true } }).pipe(
        Layer.provide(Layer.mergeAll(LuteBun.runtime, LuteBun.bundle(hubFixtureRoot), InputLog.memory))
      )),
      RuntimeSession.Session
    )
    yield* Effect.scoped(Effect.gen(function* () {
      const player = yield* LutePlayer.make({ ...ports, session: runtime })
      expect(player.session).toBe(runtime)
      expect(player.program).toBe(runtime.program)
      yield* toChoice(player)
    })).pipe(Effect.provide(Layer.mergeAll(LuteBun.bundle(hubFixtureRoot), KeyValueStore.layerMemory)))
    // The player's scope closed; the host's session still answers.
    const output = yield* runtime.send({ type: "choose", option: "take", request: 1 })
    expect(output.await.type).toBe("awaitChoice")
  }).pipe(Effect.scoped))
})
