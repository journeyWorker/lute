import { Effect, Layer, SubscriptionRef } from "effect"
import * as Contract from "../../../runtime/src/generated/Contract.js"
import * as InputLog from "../../../runtime/src/InputLog.js"
import * as LuteBun from "../../../runtime/src/bun.js"
import * as RuntimeSession from "../../../runtime/src/Session.js"
import { describe, expect, it } from "@effect/vitest"
import { PlayerSession, layer as playerLayer } from "../../src/session/index.js"
import { hubFixtureRoot } from "../../../runtime/test/support.js"

const started: Contract.Seed = { derive: true }
const bindings = {
  bindingsVersion: "0.40",
  canvas: { width: 1920, height: 1080 },
  flow: { newGame: { occasion: "visit" } }
}
const runtimeLayer = RuntimeSession.layer({ seed: started }).pipe(
  Layer.provide(Layer.mergeAll(LuteBun.runtime, LuteBun.bundle(hubFixtureRoot))),
  Layer.provideMerge(InputLog.memory)
)

describe("PlayerSession", () => {
  it.effect("begin, choose, rollback, snapshot restore and timeout use runtime state", () =>
    Effect.scoped(Effect.gen(function* () {
      const session = yield* PlayerSession
      const began = yield* session.begin(started)
      expect(began.await.type).toBe("awaitChoice")
      const chosen = yield* session.send({ option: "take", request: 1, type: "choose" })
      expect(chosen.await.type).toBe("awaitChoice")
      const save = yield* session.snapshot
      const before = yield* session.current
      const rolled = yield* session.rollback(1)
      expect(rolled.await).toEqual(began.await)
      expect(rolled.stage).toEqual(before.stage)
      expect(rolled.backlogLength).toBe(1)
      const restored = yield* session.restore(save)
      expect(restored.await).toEqual(before.await)
      expect(restored.stage).toEqual(before.stage)
      expect((yield* SubscriptionRef.get(session.backlog)).length).toBe(save.backlog.length)
    })).pipe(Effect.provide(playerLayer({ bindings }).pipe(Layer.provide(runtimeLayer))))
  )
})
