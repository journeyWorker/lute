/**
 * `@lute-lang/runtime/browser` — layers for browsers (spec 0.39.0 §6.1): the
 * binding fetched from a URL, bundles served over HTTP, and an input log in
 * `localStorage`.
 */
import * as BrowserKeyValueStore from "@effect/platform-browser/BrowserKeyValueStore"
import { Effect, Layer } from "effect"
import * as FetchHttpClient from "effect/http/FetchHttpClient"
import * as HttpClient from "effect/http/HttpClient"
import * as BundleSource from "./BundleSource.js"
import { WasmLoadError } from "./Errors.js"
import * as InputLog from "./InputLog.js"
import * as LuteRuntime from "./LuteRuntime.js"
import * as LuteWasm from "./LuteWasm.js"
import * as Session from "./Session.js"

/** The binding fetched from `url` (where the host serves `lute_runtime_wasm_bg.wasm`). */
export const wasm = (url: string): Layer.Layer<LuteWasm.LuteWasm, WasmLoadError> =>
  Layer.effect(
    LuteWasm.LuteWasm,
    HttpClient.get(url).pipe(
      Effect.flatMap((response) => response.arrayBuffer),
      Effect.mapError((error) => new WasmLoadError({ message: `cannot fetch the runtime binding: ${error.message}` })),
      Effect.flatMap(LuteWasm.fromBytes)
    )
  ).pipe(Layer.provide(FetchHttpClient.layer))

/** `LuteRuntime` over the binding at `url`. */
export const runtime = (url: string): Layer.Layer<LuteRuntime.LuteRuntime, WasmLoadError> =>
  LuteRuntime.layer.pipe(Layer.provide(wasm(url)))

/** The bundle served under `base` (a URL ending in `/`). */
export const bundle = (base: string): Layer.Layer<BundleSource.BundleSource> =>
  BundleSource.url(base).pipe(Layer.provide(FetchHttpClient.layer))

/** An input log kept in `localStorage` under `key`. */
export const inputLog = (key: string): Layer.Layer<InputLog.InputLog> =>
  InputLog.keyValueStore(key).pipe(Layer.provide(BrowserKeyValueStore.layerLocalStorage))

/**
 * A session over the bundle served under `bundle`, with the binding from
 * `wasm`; inputs are logged in `localStorage` under `log` when given, in
 * memory otherwise.
 */
export const session = (
  options: Session.SessionOptions & {
    readonly wasm: string
    readonly bundle: string
    readonly log?: string
  }
) =>
  Session.layer(options).pipe(
    Layer.provide(
      Layer.mergeAll(
        runtime(options.wasm),
        bundle(options.bundle),
        options.log === undefined ? InputLog.memory : inputLog(options.log)
      )
    )
  )
