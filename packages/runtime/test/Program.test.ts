import { readFileSync } from "node:fs"
import { pathToFileURL } from "node:url"
import { Cause, Effect, Exit, Option, Schema, Scope } from "effect"
import { describe, expect, it } from "@effect/vitest"
import * as BundleSource from "../src/BundleSource.js"
import * as Contract from "../src/generated/Contract.js"
import * as Errors from "../src/Errors.js"
import * as LuteBun from "../src/bun.js"
import * as LuteRuntime from "../src/LuteRuntime.js"
import {
  canonical,
  compileBundle,
  decode,
  evaluateFixtureRoot,
  gateFixtureRoot,
  hubFixtureRoot,
  jsonLines,
  sessionRoot
} from "./support.js"

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
    const rejected = yield* Effect.exit(runtime.load({
      index: { ...bundle.index, irVersion: "0.38.0" },
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

  it.effect("evaluates IR expressions and project defs (spec 0.41.0 §5.1)", () => Effect.gen(function* () {
    const bundleDir = yield* compileBundle(evaluateFixtureRoot)
    yield* Effect.gen(function* () {
      const program = yield* programEffect
      const started = yield* program.begin(seed({ facts: ["owned(key)"] }))
      const failure = <A, E>(exit: Exit.Exit<A, E>) => Option.getOrUndefined(Exit.findErrorOption(exit))

      expect(yield* program.evaluate(started.state, { op: "==", l: { path: "run.count" }, r: { int: 0 } }))
        .toEqual({ value: { kind: "bool", value: true } })
      expect(yield* program.evaluate(started.state, {
        call: "holds",
        args: [{ string: "owned" }, { list: [{ string: "key" }] }]
      })).toEqual({ value: { kind: "bool", value: true } })
      expect(yield* program.def(started.state, "ready")).toEqual({ value: { kind: "bool", value: false } })
      expect(yield* program.def(started.state, "score")).toEqual({ value: { kind: "int", value: 1 } })
      expect(yield* program.evaluate(started.state, { path: "run.undeclared" })).toEqual({
        value: { kind: "unknown" },
        unresolved: ["state path `run.undeclared` has no value"]
      })

      const visited = yield* program.step(started.state, input({ occasion: "visit", type: "raiseOccasion" }))
      expect(yield* program.evaluate(visited.state, { call: "visited", args: [{ string: "hall" }] }))
        .toEqual({ value: { kind: "bool", value: true } })
      expect(yield* program.def(visited.state, "ready")).toEqual({ value: { kind: "bool", value: true } })
      expect(yield* program.def(visited.state, "perCount")).toEqual({ value: { kind: "int", value: 10 } })

      expect(failure(yield* Effect.exit(program.def(started.state, "perCount"))))
        .toBeInstanceOf(Errors.EvaluationError)
      expect(failure(yield* Effect.exit(program.def(started.state, "missing")))).toBeInstanceOf(Errors.InvalidInput)
      expect(failure(yield* Effect.exit(program.evaluate(started.state, { nope: 1 }))))
        .toBeInstanceOf(Errors.InvalidInput)
    }).pipe(Effect.provide(LuteBun.runtime), Effect.provide(LuteBun.bundle(bundleDir)))
  }))

  it.effect("binds def args and rejects param nodes in evaluate (spec 0.41.0 §5.1)", () => Effect.gen(function* () {
    const bundleDir = yield* compileBundle(evaluateFixtureRoot)
    yield* Effect.gen(function* () {
      const program = yield* programEffect
      const started = yield* program.begin(seed({ facts: ["owned(key)"] }))
      const failure = <A, E>(exit: Exit.Exit<A, E>) => Option.getOrUndefined(Exit.findErrorOption(exit))
      const yes = { value: { kind: "bool", value: true } }
      const no = { value: { kind: "bool", value: false } }

      expect(yield* program.def(started.state, "atLeast", [0])).toEqual(yes)
      expect(yield* program.def(started.state, "atLeast", [1])).toEqual(no)
      // A nested `@def` call binds its own parameter from the caller's.
      expect(yield* program.def(started.state, "atLeastTwice", [0])).toEqual(yes)
      // Host defs: an entity parameter (closed kind) and a bool parameter.
      expect(yield* program.def(started.state, "owns", ["key"])).toEqual(yes)
      expect(yield* program.def(started.state, "owns", ["map"])).toEqual(no)
      expect(yield* program.def(started.state, "flagIs", [false])).toEqual(yes)
      // Parameterless defs take no args, or an empty array.
      expect(yield* program.def(started.state, "score", [])).toEqual({ value: { kind: "int", value: 1 } })

      for (const [name, args] of [
        ["atLeast", []],
        ["atLeast", [1, 2]],
        ["atLeast", ["1"]],
        ["atLeast", [1.5]],
        ["owns", ["door"]],
        ["owns", [1]],
        ["flagIs", ["true"]],
        ["score", [1]],
        ["anyOf", [1]]
      ] as const) {
        expect(failure(yield* Effect.exit(program.def(started.state, name, args))), `${name}(${JSON.stringify(args)})`)
          .toBeInstanceOf(Errors.InvalidInput)
      }
      expect(failure(yield* Effect.exit(program.def(started.state, "atLeast")))).toBeInstanceOf(Errors.InvalidInput)

      expect(failure(yield* Effect.exit(program.evaluate(started.state, { param: "n" }))))
        .toBeInstanceOf(Errors.InvalidInput)
      expect(failure(yield* Effect.exit(program.evaluate(started.state, {
        op: ">=", l: { path: "run.count" }, r: { param: "n" }
      })))).toBeInstanceOf(Errors.InvalidInput)
    }).pipe(Effect.provide(LuteBun.runtime), Effect.provide(LuteBun.bundle(bundleDir)))
  }))

  it.effect("answers the gate query and rejects closed raises (spec 0.41.0 §5.2)", () => Effect.gen(function* () {
    const bundleDir = yield* compileBundle(gateFixtureRoot)
    yield* Effect.gen(function* () {
      const program = yield* programEffect
      const started = yield* program.begin(seed({}))
      const failure = <A, E>(exit: Exit.Exit<A, E>) => Option.getOrUndefined(Exit.findErrorOption(exit))
      const before = canonical(yield* program.snapshot(started.state))

      expect(yield* program.gate(started.state, "enter", "room.lobby")).toEqual({ open: true })
      expect(yield* program.gate(started.state, "enter", "room.office")).toEqual({ open: false })
      expect(yield* program.gate(started.state, "verdict")).toEqual({ open: true })
      expect(yield* program.gate(started.state, "listen")).toEqual({
        open: false,
        unresolved: ["state path `run.noise` has no value"]
      })
      expect(failure(yield* Effect.exit(program.gate(started.state, "missing")))).toBeInstanceOf(Errors.InvalidInput)

      // A closed raise is a rejection; the state stays the one stepped, the
      // input's own writes included.
      const closed = failure(yield* Effect.exit(program.step(started.state, input({
        type: "raiseOccasion",
        occasion: "enter",
        target: "room.office",
        writes: { state: [{ path: "run.mark", value: 7 }] }
      }))))
      expect(closed).toBeInstanceOf(Errors.OccasionGateClosed)
      expect(closed?.code).toBe("E-OCCASION-GATE")
      expect(closed?.message).toContain("the engine raises `enter` for `room.office` only when")
      expect(canonical(yield* program.snapshot(started.state))).toBe(before)

      // At `ended`, only the `outsideRun` occasion is open.
      const over = yield* program.step(started.state, input({ type: "raiseOccasion", occasion: "verdict" }))
      expect(over.output.await.type).toBe("ended")
      expect(yield* program.gate(over.state, "title")).toEqual({ open: true })
      expect(yield* program.gate(over.state, "gallery")).toEqual({ open: false })
      expect(failure(yield* Effect.exit(program.step(over.state, input({ type: "raiseOccasion", occasion: "gallery" })))))
        .toBeInstanceOf(Errors.OccasionGateClosed)
      const title = yield* program.step(over.state, input({ type: "raiseOccasion", occasion: "title" }))
      expect(JSON.stringify(title.output.events)).toContain("The title screen.")
    }).pipe(Effect.provide(LuteBun.runtime), Effect.provide(LuteBun.bundle(bundleDir)))
  }))

  it.effect("maps E-OCCASION-GATE to OccasionGateClosed", () => Effect.gen(function* () {
    const exit = yield* Effect.exit(Errors.fromRejected({ code: "E-OCCASION-GATE", message: "closed" }))
    const error = Option.getOrUndefined(Exit.findErrorOption(exit))
    expect(error).toBeInstanceOf(Errors.OccasionGateClosed)
    expect(error).toMatchObject({ code: "E-OCCASION-GATE", message: "closed" })
  }))

  it.effect("shares states between programs of one fingerprint (spec 0.41.0 §6.2)", () => Effect.gen(function* () {
    const first = yield* withFixture(programEffect)
    const second = yield* withFixture(programEffect)
    expect(second.fingerprint).toBe(first.fingerprint)
    const started = yield* first.begin(seed({ derive: true }))
    const raise = input({ occasion: "visit", type: "raiseOccasion" })
    expect(canonical(yield* second.view(started.state))).toBe(canonical(yield* first.view(started.state)))
    const viaSecond = yield* second.step(started.state, raise)
    const viaFirst = yield* first.step(started.state, raise)
    expect(canonical(viaSecond.output)).toBe(canonical(viaFirst.output))
    // A state the second program produced is valid for the first.
    expect(canonical(yield* first.snapshot(viaSecond.state))).toBe(canonical(yield* second.snapshot(viaFirst.state)))
  }))

  it.effect("dies on a state of another bundle fingerprint (spec 0.41.0 §6.2)", () => Effect.gen(function* () {
    const hub = yield* withFixture(programEffect)
    const bundleDir = yield* compileBundle(evaluateFixtureRoot)
    const other = yield* programEffect.pipe(Effect.provide(LuteBun.runtime), Effect.provide(LuteBun.bundle(bundleDir)))
    expect(other.fingerprint).not.toBe(hub.fingerprint)
    const started = yield* hub.begin(seed({ derive: true }))
    const misuses: ReadonlyArray<Effect.Effect<unknown, unknown>> = [
      other.view(started.state),
      other.step(started.state, input({ occasion: "visit", type: "raiseOccasion" })),
      other.gate(started.state, "visit"),
      other.def(started.state, "score")
    ]
    for (const misuse of misuses) {
      const exit = yield* Effect.exit(misuse)
      expect(Exit.isFailure(exit)).toBe(true)
      if (Exit.isFailure(exit)) {
        expect(Cause.hasDies(exit.cause)).toBe(true)
        expect(Cause.pretty(exit.cause)).toContain(`a RuntimeState of bundle ${hub.fingerprint}`)
      }
    }
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
