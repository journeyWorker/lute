import { Effect, Layer, SubscriptionRef } from "effect"
import * as KeyValueStore from "effect/persistence/KeyValueStore"
import { TestClock } from "effect/testing"
import { describe, expect, it } from "@effect/vitest"
import type { Await, SaveRuntime } from "../../src/model.js"
import {
  ProfileStore,
  SaveCorrupt,
  SaveIncompatible,
  SaveStore,
  defaultSettings,
  layer as saveLayer,
  memory,
  profileLayer,
  type SaveData,
  type SaveStoreShape
} from "../../src/stores/index.js"
import { makeAutosave } from "../../src/stores/autosave.js"

const project = "store-test-project"
const world = {
  accepts: [],
  advanceCascadeDepth: 0,
  cadence: { latched: {}, live: {}, rearm: {}, spentSeason: [], windows: [] },
  clockAdvancedByBeat: false,
  deferBy: null,
  deferHandlers: false,
  deferredHandlers: [],
  derive: null,
  facts: [],
  failedObjectives: [],
  nextRunAccepts: [],
  questInstances: {},
  quests: {},
  shareSpentBy: {},
  spentAt: {},
  spentRun: [],
  spentUser: [],
  stage: { actors: [] },
  state: {},
  visited: []
}
const runtime: SaveRuntime = {
  snapshot: { await: { type: "idle" }, continuation: null, project, request: 0, snapshotVersion: "0.43", world },
  log: [],
  backlog: []
}
const saveData = (label: string) => ({ project, label, runtime })

const kvLayer = (values = new Map<string, string>()) => {
  const store = KeyValueStore.make({
    get: (key) => Effect.succeed(values.get(key)),
    getUint8Array: (key) => {
      const value = values.get(key)
      return Effect.succeed(value === undefined ? undefined : new TextEncoder().encode(value))
    },
    set: (key, value) => Effect.sync(() => { values.set(key, typeof value === "string" ? value : new TextDecoder().decode(value)) }),
    remove: (key) => Effect.sync(() => { values.delete(key) }),
    clear: Effect.sync(() => { values.clear() }),
    size: Effect.sync(() => values.size)
  })
  return { layer: Layer.succeed(KeyValueStore.KeyValueStore, store), values }
}

const storesLayer = saveLayer({ project }).pipe(Layer.provide(memory))

describe("SaveStore", () => {
  it.effect("saves quick, auto, and numbered slots, lists metadata, loads, and deletes", () => Effect.gen(function* () {
    const store = yield* SaveStore
    yield* store.save("quick", saveData("quick"))
    yield* store.save("auto", saveData("auto"))
    yield* store.save(2, saveData("numbered"))
    expect(yield* store.list).toEqual([
      { slot: "quick", label: "quick", savedAt: expect.any(String) },
      { slot: "auto", label: "auto", savedAt: expect.any(String) },
      { slot: 2, label: "numbered", savedAt: expect.any(String) }
    ])
    expect((yield* store.load(2)).label).toBe("numbered")
    yield* store.delete("quick")
    expect((yield* store.list).some((entry) => entry.slot === "quick")).toBe(false)
  }).pipe(Effect.provide(storesLayer)))

  it.effect("stamps savedAt from TestClock", () => Effect.gen(function* () {
    const store = yield* SaveStore
    yield* TestClock.setTime(1_700_000_000_000)
    yield* store.save("quick", saveData("clock"))
    expect((yield* store.load("quick")).savedAt).toBe(new Date(1_700_000_000_000).toISOString())
  }).pipe(Effect.provide(Layer.mergeAll(storesLayer, TestClock.layer()))))

  it.effect("keeps runtime out of list metadata", () => {
    const { layer, values } = kvLayer()
    return Effect.gen(function* () {
      const store = yield* SaveStore
      yield* store.save("quick", saveData("metadata"))
      const raw = values.get("save:quick:metadata")
      expect(raw).toBeDefined()
      expect(raw).not.toContain("snapshot")
    }).pipe(Effect.provide(saveLayer({ project }).pipe(Layer.provide(layer))))
  })

  it.effect("reports malformed JSON as SaveCorrupt", () => {
    const { layer } = kvLayer(new Map([["save:quick", "not-json"]]))
    return Effect.gen(function* () {
      const store = yield* SaveStore
      const result = yield* Effect.result(store.load("quick"))
      expect(result._tag).toBe("Failure")
      if (result._tag === "Failure") expect(result.failure).toBeInstanceOf(SaveCorrupt)
    }).pipe(Effect.provide(saveLayer({ project }).pipe(Layer.provide(layer))))
  })

  it.effect("reports a wrong save version as SaveIncompatible", () => {
    const { layer } = kvLayer(new Map([["save:quick", JSON.stringify({ saveVersion: "0.39", project })]]))
    return Effect.gen(function* () {
      const store = yield* SaveStore
      const result = yield* Effect.result(store.load("quick"))
      expect(result._tag).toBe("Failure")
      if (result._tag === "Failure") expect(result.failure).toBeInstanceOf(SaveIncompatible)
    }).pipe(Effect.provide(saveLayer({ project }).pipe(Layer.provide(layer))))
  })

  it.effect("rejects slots outside the configured range", () => Effect.gen(function* () {
    const store = yield* SaveStore
    const first = yield* Effect.result(store.save(0, saveData("bad")))
    const second = yield* Effect.result(store.save(25, saveData("bad")))
    expect(first._tag).toBe("Failure")
    expect(second._tag).toBe("Failure")
    if (first._tag === "Failure") expect(first.failure).toBeInstanceOf(SaveCorrupt)
    if (second._tag === "Failure") expect(second.failure).toBeInstanceOf(SaveCorrupt)
  }).pipe(Effect.provide(saveLayer({ project, maxSlots: 24 }).pipe(Layer.provide(memory)))))
})

describe("ProfileStore", () => {
  it.effect("persists markRead and is visible to a new layer over the same KV", () => {
    const { layer } = kvLayer()
    const profile = profileLayer({ project }).pipe(Layer.provide(layer))
    return Effect.gen(function* () {
      yield* Effect.gen(function* () {
        const first = yield* ProfileStore
        yield* first.markRead("line-1")
      }).pipe(Effect.provide(profile))
      yield* Effect.gen(function* () {
        const second = yield* ProfileStore
        expect(yield* SubscriptionRef.get(second.readLines)).toEqual(new Set(["line-1"]))
      }).pipe(Effect.provide(profile))
    })
  })

  it.effect("provides defaults and merges partial settings including nested volumes", () => Effect.gen(function* () {
    const profile = yield* ProfileStore
    expect(yield* SubscriptionRef.get(profile.settings)).toEqual({ ...defaultSettings, textSpeed: 20, volumes: { ...defaultSettings.volumes, music: 0.25 } })
    yield* profile.updateSettings({ volumes: { voice: 0.5 } })
    expect(yield* SubscriptionRef.get(profile.settings)).toEqual({ ...defaultSettings, textSpeed: 20, volumes: { ...defaultSettings.volumes, music: 0.25, voice: 0.5 } })
  }).pipe(Effect.provide(profileLayer({ project, defaults: { textSpeed: 20, volumes: { music: 0.25 } } }).pipe(Layer.provide(memory)))))

  it.effect("debounces settings writes until the TestClock delay", () => {
    const { layer, values } = kvLayer()
    return Effect.gen(function* () {
      const profile = yield* ProfileStore
      yield* profile.updateSettings({ textSpeed: 10 })
      expect(values.has("profile:" + project)).toBe(false)
      yield* TestClock.adjust(250)
      expect(values.has("profile:" + project)).toBe(true)
    }).pipe(Effect.provide(Layer.mergeAll(profileLayer({ project }).pipe(Layer.provide(layer)), TestClock.layer())))
  })
})

describe("autosave", () => {
  it.effect("autosaves choice awaits and presentation starts with rate limiting", () => Effect.gen(function* () {
    const writes: Array<string> = []
    const store: SaveStoreShape = {
      list: Effect.succeed([]),
      save: (_slot, save: SaveData) => Effect.sync(() => { writes.push(`auto:${save.label}`) }),
      load: (_slot) => Effect.fail(new SaveCorrupt({ slot: "auto", message: "unused" })),
      delete: (_slot) => Effect.void
    }
    const policy = yield* makeAutosave(store, () => Effect.succeed(saveData(`save-${writes.length + 1}`)), { minimumIntervalMs: 1000 })
    const choice: Await = { type: "awaitChoice", request: 0, menu: { construct: "", document: "", id: "", options: [], position: "", presentation: 0 } }
    yield* policy.choiceAwait(choice)
    yield* policy.chapterStart
    expect(writes).toEqual(["auto:save-1"])
    yield* TestClock.adjust(1000)
    yield* policy.chapterStart
    expect(writes).toEqual(["auto:save-1", "auto:save-2"])
  }).pipe(Effect.provide(TestClock.layer())))
})
