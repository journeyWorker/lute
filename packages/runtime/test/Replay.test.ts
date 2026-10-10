import { readFileSync } from "node:fs"
import { Cause, Effect, Exit, Option, Schema, Stream } from "effect"
import { describe, expect, it } from "@effect/vitest"
import * as BundleSource from "../src/BundleSource.js"
import * as Contract from "../src/generated/Contract.js"
import * as LuteBun from "../src/bun.js"
import * as LuteRuntime from "../src/LuteRuntime.js"
import * as Replay from "../src/Replay.js"
import { canonical, compileBundle, decode, jsonLines, sessionCases } from "./support.js"

const expectedLine = decode(Schema.Struct({ output: Contract.Output }))
const input = decode(Contract.Input)
const cases = sessionCases()

const loadProgram = Effect.gen(function* () {
  const source = yield* BundleSource.BundleSource
  const runtime = yield* LuteRuntime.LuteRuntime
  return yield* runtime.load(yield* source.load)
})

describe("Replay", () => {
  for (const testCase of cases) {
    it.effect(`replays ${testCase.name}`, () => Effect.gen(function* () {
      const bundleDir = yield* compileBundle(testCase.project)
      const entries = yield* Replay.parseLog(readFileSync(testCase.inputs, "utf8"))
      const program = yield* loadProgram.pipe(
        Effect.provide(LuteBun.runtime),
        Effect.provide(LuteBun.bundle(bundleDir))
      )
      const actual = yield* Stream.runCollect(Replay.run(program, entries))
      const expected = jsonLines(testCase.expected).map((line) => expectedLine(line).output)
      expect(actual).toHaveLength(expected.length)
      for (const [index, value] of actual.entries()) {
        expect(canonical(Schema.encodeSync(Contract.Output)(value)), `${testCase.name} output ${index}`)
          .toBe(canonical(expected[index]))
      }
    }))
  }

  it.effect("rejects logs that start with an input", () => Effect.gen(function* () {
    const testCase = cases[0]
    if (testCase === undefined) throw new Error("session conformance cases are missing")
    const bundleDir = yield* compileBundle(testCase.project)
    const program = yield* loadProgram.pipe(
      Effect.provide(LuteBun.runtime),
      Effect.provide(LuteBun.bundle(bundleDir))
    )
    const rejected = yield* Effect.exit(
      Stream.runCollect(Replay.run(program, [{ input: input({ occasion: "visit", type: "raiseOccasion" }) }]))
    )
    expect(Exit.isFailure(rejected)).toBe(true)
    if (Exit.isFailure(rejected)) expect(Cause.hasDies(rejected.cause)).toBe(true)
  }))

  it.effect("fails malformed log lines with SchemaError", () => Effect.gen(function* () {
    const rejected = yield* Effect.exit(Replay.parseLog("{not-json}"))
    expect(Exit.isFailure(rejected)).toBe(true)
    if (Exit.isFailure(rejected)) {
      expect(Option.getOrUndefined(Exit.findErrorOption(rejected))).toBeInstanceOf(Schema.SchemaError)
    }
  }))
})
