import { Effect, Schema, Stream } from "effect"
import { describe, expect, it } from "@effect/vitest"
import * as LuteBrowser from "@lute-lang/runtime/browser"
import * as BundleSource from "../../src/BundleSource.js"
import * as Contract from "../../src/generated/Contract.js"
import * as LuteRuntime from "../../src/LuteRuntime.js"
import * as Replay from "../../src/Replay.js"
import expectedText from "../../../../conformance/session/hub-once/expected.jsonl?raw"
import inputsText from "../../../../conformance/session/hub-once/inputs.jsonl?raw"

declare const __LUTE_RUNTIME_WASM_URL__: string
declare const __LUTE_HUB_BUNDLE_BASE__: string

const expectedLine = Schema.Struct({ output: Contract.Output })

const canonical = (value: unknown): string => {
  const sort = (item: unknown): unknown => {
    if (Array.isArray(item)) return item.map(sort)
    if (item !== null && typeof item === "object") {
      return Object.fromEntries(
        Object.entries(item)
          .sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0))
          .map(([key, child]) => [key, sort(child)])
      )
    }
    return item
  }
  return JSON.stringify(sort(value))
}

const loadProgram = Effect.gen(function* () {
  const source = yield* BundleSource.BundleSource
  const runtime = yield* LuteRuntime.LuteRuntime
  return yield* runtime.load(yield* source.load)
})

describe("browser replay", () => {
  it.effect("replays hub-once through the browser layers", () => Effect.gen(function* () {
    const entries = yield* Replay.parseLog(inputsText)
    const origin = globalThis.location.origin
    const program = yield* loadProgram.pipe(
      Effect.provide(LuteBrowser.runtime(new URL(__LUTE_RUNTIME_WASM_URL__, origin).href)),
      Effect.provide(LuteBrowser.bundle(new URL(__LUTE_HUB_BUNDLE_BASE__, origin).href))
    )
    const actual = yield* Stream.runCollect(Replay.run(program, entries))
    const expected = expectedText
      .split("\n")
      .filter((line) => line.trim().length > 0)
      .map((line) => Schema.decodeUnknownSync(expectedLine)(JSON.parse(line)).output)

    expect(actual).toHaveLength(expected.length)
    for (const [index, value] of actual.entries()) {
      expect(canonical(Schema.encodeSync(Contract.Output)(value)), `hub-once output ${index}`)
        .toBe(canonical(expected[index]))
    }
  }))
})
