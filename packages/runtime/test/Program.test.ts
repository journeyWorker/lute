import { readFileSync } from "node:fs"
import { pathToFileURL } from "node:url"
import { Cause, Effect, Exit, Option, Schema, Scope } from "effect"
import { describe, expect, it } from "@effect/vitest"
import * as BundleSource from "../src/BundleSource.js"
import * as Contract from "../src/generated/Contract.js"
import * as Errors from "../src/Errors.js"
import * as LuteBun from "../src/bun.js"
import * as LuteRuntime from "../src/LuteRuntime.js"
import { canonical, decode, hubFixtureRoot, jsonLines, sessionRoot } from "./support.js"

const input = decode(Contract.Input)
const seed = decode(Contract.Seed)
const expectedLine = decode(Schema.Struct({ output: Contract.Output }))
const expected = jsonLines(`${sessionRoot}/hub-once/expected.jsonl`)

const expectedOutput = (line: number) => expectedLine(expected[line]).output
const encodeOutput = Schema.encodeSync(Contract.Output)
const programEffect = Effect.gen(function* () {
  const source = yield* BundleSource.BundleSource
  const runtime = yield* LuteRuntime.LuteRuntime
  return yield* runtime.load(yield* source.load)
})

const withFixture = <A, E>(
  effect: Effect.Effect<A, E, LuteRuntime.LuteRuntime | BundleSource.BundleSource | Scope.Scope>
) => effect.pipe(Effect.provide(LuteBun.runtime), Effect.provide(LuteBun.bundle(hubFixtureRoot)))

describe("Program", () => {
  it.effect("begins and steps the hub-once fixture", () => Effect.gen(function* () {
    const program = yield* withFixture(programEffect)
    const started = yield* program.begin(seed({ derive: true }))
    expect(canonical(encodeOutput(started.output))).toBe(canonical(expectedOutput(0)))
    const raised = yield* program.step(started.state, input({ occasion: "visit", type: "raiseOccasion" }))
    expect(canonical(encodeOutput(raised.output))).toBe(canonical(expectedOutput(1)))
    const taken = yield* program.step(raised.state, input({ option: "take", request: 1, type: "choose" }))
    expect(canonical(encodeOutput(taken.output))).toBe(canonical(expectedOutput(2)))
  }))

  it.effect("preserves state after busy and mismatched requests", () => Effect.gen(function* () {
    const program = yield* withFixture(programEffect)
    const started = yield* program.begin(seed({ derive: true }))
    const baselineRaised = yield* program.step(started.state, input({ occasion: "visit", type: "raiseOccasion" }))
    const busy = yield* Effect.exit(program.step(started.state, input({ option: "take", request: 1, type: "choose" })))
    expect(Exit.isFailure(busy)).toBe(true)
    if (Exit.isFailure(busy)) {
      expect(Option.getOrUndefined(Exit.findErrorOption(busy))).toBeInstanceOf(Errors.RuntimeBusy)
    }
    const raised = yield* program.step(started.state, input({ occasion: "visit", type: "raiseOccasion" }))
    expect(canonical(raised.output)).toBe(canonical(baselineRaised.output))
    const baselineTaken = yield* program.step(raised.state, input({ option: "take", request: 1, type: "choose" }))
    const mismatch = yield* Effect.exit(program.step(raised.state, input({ option: "take", request: 99, type: "choose" })))
    expect(Exit.isFailure(mismatch)).toBe(true)
    if (Exit.isFailure(mismatch)) {
      expect(Option.getOrUndefined(Exit.findErrorOption(mismatch))).toBeInstanceOf(Errors.RequestMismatch)
    }
    const taken = yield* program.step(raised.state, input({ option: "take", request: 1, type: "choose" }))
    expect(canonical(taken.output)).toBe(canonical(baselineTaken.output))
  }))

  it.effect("restores snapshots and rejects old snapshot versions", () => Effect.gen(function* () {
    const program = yield* withFixture(programEffect)
    const started = yield* program.begin(seed({ derive: true }))
    const raised = yield* program.step(started.state, input({ occasion: "visit", type: "raiseOccasion" }))
    const snapshot = yield* program.snapshot(raised.state)
    const restored = yield* program.restore(snapshot)
    const originalStep = yield* program.step(raised.state, input({ option: "take", request: 1, type: "choose" }))
    const restoredStep = yield* program.step(restored, input({ option: "take", request: 1, type: "choose" }))
    expect(canonical(restoredStep.output)).toBe(canonical(originalStep.output))
    const oldSnapshot = { ...snapshot, snapshotVersion: "0.38.0" }
    const rejected = yield* Effect.exit(program.restore(oldSnapshot))
    expect(Exit.isFailure(rejected)).toBe(true)
    if (Exit.isFailure(rejected)) {
      expect(Option.getOrUndefined(Exit.findErrorOption(rejected))).toBeInstanceOf(Errors.SnapshotVersionMismatch)
    }
  }))

  it.effect("rejects bundles from another IR version", () => Effect.gen(function* () {
    const source = yield* BundleSource.BundleSource
    const runtime = yield* LuteRuntime.LuteRuntime
    const bundle = yield* source.load
    const index = decode(Schema.Record(Schema.String, Schema.Json))(bundle.index)
    const rejected = yield* Effect.exit(runtime.load({
      index: { ...index, irVersion: "0.38.0" },
      artifacts: bundle.artifacts
    }))
    expect(Exit.isFailure(rejected)).toBe(true)
    if (Exit.isFailure(rejected)) {
      expect(Option.getOrUndefined(Exit.findErrorOption(rejected))).toBeInstanceOf(Errors.IrVersionMismatch)
    }
  }).pipe(Effect.provide(LuteBun.runtime), Effect.provide(LuteBun.bundle(hubFixtureRoot))))

  it.effect("answers read-only queries and rejects unknown runtime codes as defects", () => Effect.gen(function* () {
    const program = yield* withFixture(programEffect)
    const started = yield* program.begin(seed({ derive: true }))
    const candidates = yield* program.candidates(started.state, "visit")
    expect(candidates[0]?.id).toBe("hub")
    expect(Option.isNone(yield* program.clock(started.state))).toBe(true)
    expect((yield* program.view(started.state)).state).toEqual({
      "run.answer": { kind: "bool", value: false },
      "run.count": { kind: "int", value: 0 }
    })
    expect(yield* program.terminal(started.state)).toBe(false)
    const unknown = yield* Effect.exit(Errors.fromRejected({ code: "E-NOPE", message: "" }))
    expect(Exit.isFailure(unknown)).toBe(true)
    if (Exit.isFailure(unknown)) expect(Cause.hasDies(unknown.cause)).toBe(true)
  }))

  it.effect("using a program after its scope closes is a defect", () => Effect.gen(function* () {
    const scope = yield* Scope.make()
    const program = yield* Scope.provide(scope)(withFixture(programEffect))
    yield* Scope.close(scope, Exit.void)
    const reused = yield* Effect.exit(program.begin(seed({ derive: true })))
    expect(Exit.isFailure(reused)).toBe(true)
    if (Exit.isFailure(reused)) expect(Cause.hasDies(reused.cause)).toBe(true)
  }))

  it.effect("surfaces a panic from the test wasm binding as a defect", () => Effect.gen(function* () {
    const path = new URL("./wasm-panic/lute_runtime_wasm.js", import.meta.url)
    const bytes = new Uint8Array(readFileSync(new URL("./wasm-panic/lute_runtime_wasm_bg.wasm", import.meta.url)))
    const glue: { readonly initSync: (input: { readonly module: Uint8Array }) => unknown; readonly debug_panic: () => void } =
      yield* Effect.promise(() => import(pathToFileURL(path.pathname).href))
    glue.initSync({ module: bytes })
    const panic = yield* Effect.exit(Effect.sync(() => glue.debug_panic()))
    expect(Exit.isFailure(panic)).toBe(true)
    if (Exit.isFailure(panic)) expect(Cause.hasDies(panic.cause)).toBe(true)
  }))
})
