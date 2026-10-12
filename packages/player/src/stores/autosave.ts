/** Rate-limited autosave triggers for choice awaits and chapter presentation starts. */
import { Clock, Effect, Ref } from "effect"
import type { Await } from "../model.js"
import type { SaveCorrupt, SaveData, SaveStoreShape } from "./index.js"

/**
 * When the `auto` slot is written (spec 0.41.0 §7.9): omitted, at a
 * presentation start and at every `awaitChoice`; `"presentation"` / `"choice"`,
 * only there; `false`, never (no snapshot, no capture).
 */
export type Autosave = false | "presentation" | "choice"

/** Whether an autosave mode writes at `point`. */
export const autosavesAt = (mode: Autosave | undefined, point: "presentation" | "choice"): boolean =>
  mode === undefined || mode === point

/** Inputs consumed by the autosave policy. */
export interface AutosavePolicy {
  readonly choiceAwait: (await_: Await) => Effect.Effect<void, SaveCorrupt>
  readonly chapterStart: Effect.Effect<void, SaveCorrupt>
}

/** Construct an autosave policy that writes the `auto` slot at most once per interval. */
export const makeAutosave = (
  store: SaveStoreShape,
  capture: () => Effect.Effect<SaveData>,
  options: { readonly minimumIntervalMs?: number; readonly autosave?: Autosave } = {}
): Effect.Effect<AutosavePolicy, never, never> =>
  Effect.gen(function* () {
    const last = yield* Ref.make<number | undefined>(undefined)
    const minimumIntervalMs = options.minimumIntervalMs ?? 1000
    const trigger: Effect.Effect<void, SaveCorrupt> = Effect.gen(function* () {
      const now = yield* Clock.currentTimeMillis
      const previous = yield* Ref.get(last)
      if (previous !== undefined && now - previous < minimumIntervalMs) return
      const save = yield* capture()
      yield* store.save("auto", save)
      yield* Ref.set(last, now)
    })
    return {
      choiceAwait: (await_: Await) => await_.type === "awaitChoice" && autosavesAt(options.autosave, "choice") ? trigger : Effect.void,
      chapterStart: autosavesAt(options.autosave, "presentation") ? trigger : Effect.void
    }
  })
