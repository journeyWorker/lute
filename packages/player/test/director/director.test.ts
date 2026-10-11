import { Effect, Stream, SubscriptionRef } from "effect"
import { describe, expect, it } from "@effect/vitest"
import { make as makeDirector } from "../../src/director/index.js"
import type { PlayerSessionShape } from "../../src/session/index.js"
import { defaultSettings } from "../../src/stores/index.js"
import type { Presenter } from "../../src/ports/index.js"

const bindings = { bindingsVersion: "0.40" as const, canvas: { width: 1920, height: 1080 } }
const profile = Effect.gen(function* () {
  const readLines = yield* SubscriptionRef.make<ReadonlySet<string>>(new Set())
  const settings = yield* SubscriptionRef.make(defaultSettings)
  return { readLines, settings, markRead: () => Effect.void, updateSettings: () => Effect.void }
})
const presenter = (): Presenter => ({
  stage: () => Effect.void,
  showLine: () => Effect.void,
  revealLine: Effect.void,
  clearLine: Effect.void,
  restore: () => Effect.void,
  showChoice: () => Effect.never,
  hideChoice: Effect.void,
  showScreen: () => Effect.void,
  hideScreen: Effect.void,
  setUiHidden: () => Effect.void,
  applySettings: () => Effect.void,
  capture: Effect.succeed("data:image/png;base64,test"),
  actions: Stream.empty
})
const session = {} as PlayerSessionShape
const scheduler = { play: () => Effect.void, events: Stream.empty }
const factory = { fromOutput: () => Effect.succeed([]) }
const saves = (occupied: boolean) => ({
  list: Effect.succeed(occupied ? [{ slot: "quick" as const, label: "", savedAt: "", thumbnail: undefined }] : []),
  save: () => Effect.void,
  load: () => Effect.die(new Error("unused")),
  delete: () => Effect.void
})
const audio = { play: () => Effect.void, stop: () => Effect.void, setVolumes: () => Effect.void }
const assets = { ensure: () => Effect.void, progress: Stream.empty }

const boot = (occupied: boolean) => Effect.scoped(Effect.gen(function* () {
  const p = yield* profile
  const director = yield* makeDirector({ bindings, session, presenter: presenter(), audio, assets, profile: p, saves: saves(occupied), cueFactory: factory, scheduler })
  yield* Effect.yieldNow
  return yield* SubscriptionRef.get(director.view)
}))

describe("Director boot", () => {
  it.effect("shows Title with canContinue false", () => Effect.gen(function* () {
    const view = yield* boot(false)
    expect(view.flow).toBe("title")
  }))
  it.effect("shows Title with canContinue true", () => Effect.gen(function* () {
    const view = yield* boot(true)
    expect(view.flow).toBe("title")
  }))
})
