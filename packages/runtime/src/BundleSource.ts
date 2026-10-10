/**
 * Where a session's bundle comes from (spec 0.39.0 §5.1): a `lute compile
 * --all` output directory, the same layout served over HTTP, or a value the
 * host already holds.
 */
import { Context, Effect, FileSystem, Layer, Path, Schema } from "effect"
import * as HttpClient from "effect/http/HttpClient"
import { Bundle, IndexDocuments } from "./Bundle.js"
import { BundleMalformed, BundleNotFound, BundleUnreadable } from "./Errors.js"
import type { BundleError } from "./Errors.js"

export class BundleSource extends Context.Service<
  BundleSource,
  { readonly load: Effect.Effect<Bundle, BundleError> }
>()("@lute-lang/runtime/BundleSource") {}

const INDEX = "project.index.json"

const decodeJson = Schema.decodeUnknownEffect(Schema.fromJsonString(Schema.Json))
const decodeDocuments = Schema.decodeUnknownEffect(IndexDocuments)

/**
 * Assemble a bundle from a reader of its files: the index, then every
 * artifact the index lists.
 */
const assemble = <E>(read: (path: string) => Effect.Effect<string, E>): Effect.Effect<Bundle, E | BundleMalformed> =>
  Effect.gen(function* () {
    const readJson = (path: string) =>
      Effect.flatMap(read(path), (text) =>
        Effect.mapError(decodeJson(text), (error) => new BundleMalformed({ path, message: error.message }))
      )
    const index = yield* readJson(INDEX)
    const { documents } = yield* Effect.mapError(
      decodeDocuments(index),
      (error) => new BundleMalformed({ path: INDEX, message: error.message })
    )
    const artifacts = yield* Effect.forEach(documents, ({ artifact }) =>
      Effect.map(readJson(artifact), (json) => [artifact, json] as const)
    )
    return { index, artifacts: Object.fromEntries(artifacts) }
  })

/** A bundle the host already holds. */
export const value = (bundle: Bundle): Layer.Layer<BundleSource> =>
  Layer.succeed(BundleSource, { load: Effect.succeed(bundle) })

/** A `lute compile --all` output directory. */
export const directory = (root: string): Layer.Layer<BundleSource, never, FileSystem.FileSystem | Path.Path> =>
  Layer.effect(
    BundleSource,
    Effect.gen(function* () {
      const fs = yield* FileSystem.FileSystem
      const path = yield* Path.Path
      const read = (file: string) => {
        const full = path.join(root, file)
        return Effect.mapError(fs.readFileString(full), (error): BundleError =>
          error.reason._tag === "NotFound"
            ? new BundleNotFound({ path: full, message: error.message })
            : new BundleUnreadable({ path: full, message: error.message })
        )
      }
      return { load: Effect.withSpan(assemble(read), "Lute.BundleSource.directory", { attributes: { root } }) }
    })
  )

/** A `lute compile --all` output served under `base` (a URL ending in `/`). */
export const url = (base: string): Layer.Layer<BundleSource, never, HttpClient.HttpClient> =>
  Layer.effect(
    BundleSource,
    Effect.gen(function* () {
      const client = HttpClient.filterStatusOk(yield* HttpClient.HttpClient)
      const read = (file: string) => {
        const href = new URL(file, base).href
        return client.get(href).pipe(
          Effect.flatMap((response) => response.text),
          Effect.mapError((error): BundleError =>
            error.reason._tag === "StatusCodeError" && error.reason.response.status === 404
              ? new BundleNotFound({ path: href, message: error.message })
              : new BundleUnreadable({ path: href, message: error.message })
          )
        )
      }
      return { load: Effect.withSpan(assemble(read), "Lute.BundleSource.url", { attributes: { base } }) }
    })
  )
