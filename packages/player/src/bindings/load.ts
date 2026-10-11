/** Loading and validation for `player.bindings.yaml` values. */
import { Effect, Schema } from "effect"
import * as HttpClient from "effect/http/HttpClient"
import { Bindings as BindingsSchema } from "./schema.js"
import type { Bindings } from "../model.js"

/** A bindings document failed parsing or schema validation. */
export class BindingsInvalid extends Schema.TaggedError<BindingsInvalid>()("BindingsInvalid", {
  path: Schema.String,
  message: Schema.String
}) {}

const decodeObject = (value: unknown): Effect.Effect<Bindings, BindingsInvalid> =>
  Effect.mapError(
    Schema.decodeUnknownEffect(BindingsSchema, { onExcessProperty: "error" })(value),
    (error) => new BindingsInvalid({ path: "bindings", message: error.message })
  )

/** Decode a JSON value or an already parsed bindings object. */
export const decode = (value: unknown): Effect.Effect<Bindings, BindingsInvalid> => {
  if (typeof value !== "string") return decodeObject(value)
  return Effect.try({
    try: () => {
      const parsed: unknown = JSON.parse(value)
      return parsed
    },
    catch: (error) => new BindingsInvalid({
      path: "bindings",
      message: `YAML input requires a YAML parser dependency; JSON is supported (${error instanceof Error ? error.message : "invalid JSON"})`
    })
  }).pipe(Effect.flatMap(decodeObject))
}

/** Fetch and decode a bindings document through Effect's HTTP client. */
export const loadUrl = (url: string): Effect.Effect<Bindings, BindingsInvalid, HttpClient.HttpClient> =>
  Effect.gen(function* () {
    const client = yield* HttpClient.HttpClient
    const response = yield* Effect.mapError(client.get(url), (error) => new BindingsInvalid({ path: url, message: error.message }))
    const text = yield* Effect.mapError(response.text, (error) => new BindingsInvalid({ path: url, message: error.message }))
    return yield* decode(text)
  })

/** Resolve a binding asset id or relative file below the configured assets base. */
export const resolveFile = (bindings: Bindings, file: string): string => {
  const mapped = bindings.assets?.[file] ?? file
  const base = bindings.assetsBase ?? "assets/"
  if (/^[a-z][a-z\d+.-]*:/i.test(base)) return new URL(mapped, base).href
  const left = base.endsWith("/") ? base : `${base}/`
  return `${left}${mapped.replace(/^\/+/, "")}`
}
