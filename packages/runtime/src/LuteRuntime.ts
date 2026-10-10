/**
 * Loads bundles into {@link Program}s (spec 0.39.0 §5.1). A program lives in
 * the scope that loaded it and is freed when the scope closes.
 */
import { Context, Effect, Layer, Scope } from "effect"
import type { Bundle } from "./Bundle.js"
import type * as Errors from "./Errors.js"
import { LuteWasm } from "./LuteWasm.js"
import * as Program from "./Program.js"

export class LuteRuntime extends Context.Service<
  LuteRuntime,
  {
    /**
     * Load a bundle. `IrVersionMismatch` when the runtime does not execute
     * it (another IR line, an unknown command kind, not a bundle).
     */
    readonly load: (bundle: Bundle) => Effect.Effect<Program.Program, Errors.RuntimeRejection, Scope.Scope>
  }
>()("@lute-lang/runtime/LuteRuntime") {}

export const layer: Layer.Layer<LuteRuntime, never, LuteWasm> = Layer.effect(
  LuteRuntime,
  Effect.gen(function* () {
    const wasm = yield* LuteWasm

    const acquire = Effect.fn("Lute.LuteRuntime.load")(function* (bundle: Bundle) {
      const result = wasm.Program.load(JSON.stringify(bundle.index), JSON.stringify(bundle.artifacts))
      const { ok, program, rejected } = result
      result.free()
      if (ok && program !== undefined) return Program.make(program)
      if (rejected !== undefined) return yield* Program.rejection(rejected)
      return yield* Effect.die(new Error("the runtime binding returned neither a program nor a rejection"))
    })

    return {
      load: (bundle) => Effect.acquireRelease(acquire(bundle), (program) => program.free)
    }
  })
)
