/**
 * `@lute-lang/runtime/bun` — layers for the Bun runtime (spec 0.39.0 §6.1):
 * the packaged binding read from disk, bundles from a `lute compile --all`
 * directory, and a session composed from them.
 */
import * as BunServices from "@effect/platform-bun/BunServices"
import { Effect, FileSystem, Layer, Path } from "effect"
import * as BundleSource from "./BundleSource.js"
import { WasmLoadError } from "./Errors.js"
import * as InputLog from "./InputLog.js"
import * as LuteRuntime from "./LuteRuntime.js"
import * as LuteWasm from "./LuteWasm.js"
import * as Session from "./Session.js"

/** The binding shipped with this package, read from disk. */
export const wasm: Layer.Layer<LuteWasm.LuteWasm, WasmLoadError> = Layer.effect(
  LuteWasm.LuteWasm,
  Effect.gen(function* () {
    const fs = yield* FileSystem.FileSystem
    const path = yield* Path.Path
    const bytes = yield* path.fromFileUrl(new URL("./wasm/lute_runtime_wasm_bg.wasm", import.meta.url)).pipe(
      Effect.flatMap(fs.readFile),
      Effect.mapError((error) => new WasmLoadError({ message: `cannot read the runtime binding: ${error.message}` }))
    )
    return yield* LuteWasm.fromBytes(bytes)
  })
).pipe(Layer.provide(BunServices.layer))

/** `LuteRuntime` over the packaged binding. */
export const runtime: Layer.Layer<LuteRuntime.LuteRuntime, WasmLoadError> = LuteRuntime.layer.pipe(Layer.provide(wasm))

/** The bundle in a `lute compile --all` output directory. */
export const bundle = (directory: string): Layer.Layer<BundleSource.BundleSource> =>
  BundleSource.directory(directory).pipe(Layer.provide(BunServices.layer))

/**
 * A session over the bundle in `bundle` (a directory), logging inputs in
 * memory unless `log` provides another `InputLog`.
 */
export const session = (
  options: Session.SessionOptions & {
    readonly bundle: string
    readonly log?: Layer.Layer<InputLog.InputLog>
  }
) =>
  Session.layer(options).pipe(
    Layer.provide(Layer.mergeAll(runtime, bundle(options.bundle), options.log ?? InputLog.memory))
  )
