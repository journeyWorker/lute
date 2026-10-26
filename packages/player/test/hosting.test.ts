import { Context, Effect, Layer, Stream, SubscriptionRef } from "effect"
import { describe, expect, it } from "@effect/vitest"
import * as InputLog from "../../runtime/src/InputLog.js"
import * as LuteBun from "../../runtime/src/bun.js"
import * as RuntimeSession from "../../runtime/src/Session.js"
import { hubFixtureRoot } from "../../runtime/test/support.js"
import { LutePlayer } from "../src/LutePlayer.js"
import * as Stores from "../src/stores/index.js"
import type { AssetLoader, AudioPort, Presenter } from "../src/ports/index.js"
import { flush } from "./director/support.js"

const bindings = { bindingsVersion: "0.51", canvas: { width: 1920, height: 1080 }, flow: { newGame: { occasion: "visit" } } }
const presenter: Presenter = {
  stage: () => Effect.void, showLine: () => Effect.void, revealLine: Effect.void, clearLine: Effect.void, restore: () => Effect.void,
  showChoice: () => Effect.never, hideChoice: Effect.void, showScreen: () => Effect.void, hideScreen: Effect.void,
  setUiHidden: () => Effect.void, applySettings: () => Effect.void, capture: Effect.succeed(""), actions: Stream.empty
}
const audio: AudioPort = { play: () => Effect.void, stop: () => Effect.void, setVolumes: () => Effect.void }
const assets: AssetLoader = { ensure: () => Effect.void, progress: Stream.empty }
const ports = { bindings, presenter, audio, assets }

const toChoice = (player: Effect.Success<ReturnType<typeof LutePlayer.make>>) => Effect.gen(function* () {
  yield* player.actions({ type: "newGame" })
  for (let i = 0; i < 20; i++) {
    yield* flush
    if ((yield* SubscriptionRef.get(player.view)).story === "choice") return
    yield* player.actions({ type: "advance" })
  }
  throw new Error("no choice")
})

const externalSession = (start: Parameters<typeof RuntimeSession.layer>[0]) =>
  Layer.build(RuntimeSession.layer(start).pipe(
    Layer.provide(Layer.mergeAll(LuteBun.runtime, LuteBun.bundle(hubFixtureRoot), InputLog.memory))
  )).pipe(Effect.map(context => Context.get(context, RuntimeSession.Session)))

describe("external hosting checkpoint", () => {
  it.effect("restores a SaveRuntime into a fresh external-session player", () => Effect.scoped(Effect.gen(function* () {
    const runtime = yield* externalSession({ seed: { derive: true } })
    const save = yield* Effect.scoped(Effect.gen(function* () {
      const player = yield* LutePlayer.make({ ...ports, session: runtime })
      yield* toChoice(player)
      return yield* player.snapshot
    }).pipe(Effect.provide(Layer.mergeAll(LuteBun.bundle(hubFixtureRoot), Stores.memory))))
    const freshRuntime = yield* externalSession({ snapshot: save.snapshot })
    yield* Effect.scoped(Effect.gen(function* () {
      const player = yield* LutePlayer.make({ ...ports, session: freshRuntime })
      yield* player.restore(save)
      const current = yield* SubscriptionRef.get(freshRuntime.current)
      expect(current.await).toEqual(save.snapshot.await)
      const restored = yield* player.snapshot
      expect(restored.backlog).toEqual(save.backlog)
      expect(restored.log).toEqual(save.log)
    }).pipe(Effect.provide(Layer.mergeAll(LuteBun.bundle(hubFixtureRoot), Stores.memory))))
  })))
})
