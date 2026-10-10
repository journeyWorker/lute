/**
 * The instantiated wasm binding (`crates/lute-runtime-wasm`) as a service.
 *
 * Platform layers (`@lute-lang/runtime/bun`, `@lute-lang/runtime/browser`)
 * obtain the module's bytes and instantiate it through {@link fromBytes};
 * nothing else in the package touches the glue's module state.
 */
import { Context, Effect, Layer } from "effect"
import { WasmLoadError } from "./Errors.js"
import * as Glue from "./wasm/lute_runtime_wasm.js"

/** The binding's entry point: `Program.load` and the result classes it returns. */
export interface LuteWasmShape {
  readonly Program: typeof Glue.Program
}

export class LuteWasm extends Context.Service<LuteWasm, LuteWasmShape>()("@lute-lang/runtime/LuteWasm") {}

/**
 * Instantiate the binding from the `.wasm` bytes. Bytes backed by a
 * `SharedArrayBuffer` are refused: WebAssembly compiles from an
 * `ArrayBuffer`.
 */
export const fromBytes = (bytes: Uint8Array | ArrayBuffer): Effect.Effect<LuteWasmShape, WasmLoadError> =>
  Effect.try({
    try: () => {
      const { buffer, byteOffset, byteLength } = bytes instanceof Uint8Array ? bytes : new Uint8Array(bytes)
      if (!(buffer instanceof ArrayBuffer)) throw new TypeError("the bytes live in a SharedArrayBuffer")
      Glue.initSync({ module: new Uint8Array(buffer, byteOffset, byteLength) })
      return { Program: Glue.Program }
    },
    catch: (cause) => new WasmLoadError({ message: `cannot instantiate the runtime binding: ${String(cause)}` })
  })

/** The binding instantiated from bytes the host already holds. */
export const layerFromBytes = (bytes: Uint8Array | ArrayBuffer): Layer.Layer<LuteWasm, WasmLoadError> =>
  Layer.effect(LuteWasm, fromBytes(bytes))
