/**
 * The recorded input log of a session (spec 0.38.0 §9: `(seed or snapshot,
 * inputs)`). Replaying it against the same bundle reproduces every output;
 * its entries have the `inputs.jsonl` line shape.
 */
import { Context, Effect, Layer, Option, Ref, Schema } from "effect"
import * as KeyValueStore from "effect/persistence/KeyValueStore"
import { InputLogError } from "./Errors.js"
import * as Contract from "./generated/Contract.js"

export const LogEntry = Schema.Union([
  Schema.Struct({ seed: Contract.Seed }),
  Schema.Struct({ snapshot: Contract.Snapshot }),
  Schema.Struct({ input: Contract.Input })
])
export type LogEntry = typeof LogEntry.Type

export class InputLog extends Context.Service<
  InputLog,
  {
    /** Start a new log with its `seed` or `snapshot` entry. */
    readonly reset: (start: LogEntry) => Effect.Effect<void, InputLogError>
    /** Record an accepted input. */
    readonly append: (entry: LogEntry) => Effect.Effect<void, InputLogError>
    readonly entries: Effect.Effect<ReadonlyArray<LogEntry>, InputLogError>
  }
>()("@lute-lang/runtime/InputLog") {}

/** A log held in memory for the session's lifetime. */
export const memory: Layer.Layer<InputLog> = Layer.effect(
  InputLog,
  Effect.map(Ref.make<ReadonlyArray<LogEntry>>([]), (log) => ({
    reset: (start) => Ref.set(log, [start]),
    append: (entry) => Ref.update(log, (entries) => [...entries, entry]),
    entries: Ref.get(log)
  }))
)

/** A log persisted under `key` in a `KeyValueStore` (e.g. `localStorage`). */
export const keyValueStore = (key: string): Layer.Layer<InputLog, never, KeyValueStore.KeyValueStore> =>
  Layer.effect(
    InputLog,
    Effect.gen(function* () {
      const store = KeyValueStore.toSchemaStore(yield* KeyValueStore.KeyValueStore, Schema.Array(LogEntry))
      const entries = store.get(key).pipe(
        Effect.map(Option.getOrElse((): ReadonlyArray<LogEntry> => [])),
        Effect.mapError((error) => new InputLogError({ message: error.message }))
      )
      return {
        reset: (start) =>
          Effect.mapError(store.set(key, [start]), (error) => new InputLogError({ message: error.message })),
        append: (entry) =>
          entries.pipe(
            Effect.flatMap((logged) => store.set(key, [...logged, entry])),
            Effect.mapError((error) => new InputLogError({ message: error.message }))
          ),
        entries
      }
    })
  )
