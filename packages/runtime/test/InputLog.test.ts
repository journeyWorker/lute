import { Effect, Exit, Layer, Option } from "effect"
import * as KeyValueStore from "effect/persistence/KeyValueStore"
import { describe, expect, it } from "@effect/vitest"
import * as Contract from "../src/generated/Contract.js"
import * as Errors from "../src/Errors.js"
import * as InputLog from "../src/InputLog.js"
import { decode } from "./support.js"

const seed = decode(Contract.Seed)
const input = decode(Contract.Input)
const persistedServices = InputLog.keyValueStore("k").pipe(Layer.provideMerge(KeyValueStore.layerMemory))

describe("InputLog", () => {
  it.effect("round-trips reset, append, and entries in a key-value store", () => Effect.gen(function* () {
    const log = yield* InputLog.InputLog
    const start = { seed: seed({ derive: true }) }
    const entry = { input: input({ occasion: "visit", type: "raiseOccasion" }) }
    yield* log.reset(start)
    yield* log.append(entry)
    expect(yield* log.entries).toEqual([start, entry])
    yield* log.reset({ seed: seed({ derive: false }) })
    expect(yield* log.entries).toEqual([{ seed: { derive: false } }])
  }).pipe(Effect.provide(persistedServices)))

  it.effect("reports corrupted stored values as InputLogError", () => Effect.gen(function* () {
    const store = yield* KeyValueStore.KeyValueStore
    yield* store.set("k", "not-json")
    const log = yield* InputLog.InputLog
    const result = yield* Effect.exit(log.entries)
    expect(Exit.isFailure(result)).toBe(true)
    if (Exit.isFailure(result)) {
      expect(Option.getOrUndefined(Exit.findErrorOption(result))).toBeInstanceOf(Errors.InputLogError)
    }
  }).pipe(Effect.provide(persistedServices)))
})
