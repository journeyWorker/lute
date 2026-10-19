/**
 * Lute 0.47.2 §2 (#137): every cue of a batch starts. Over the compiled
 * `chime-batch` fixture, a `::camera` and a `::sfx` that are the last records
 * before a choice reach the presenter and the audio port before the choice is
 * shown — whatever the autosave mode — and a `::sfx` that is the last record
 * before an `idle` await is played.
 */
import { Effect } from "effect"
import { describe, expect, it } from "@effect/vitest"
import type { Bindings } from "../src/bindings/schema.js"
import type { Autosave } from "../src/stores/autosave.js"
import { compileChimeBatch } from "./fixtures.js"
import { act, hostLayer, player, quayBindings, recorder } from "./player.js"

const bindings: Bindings = { ...quayBindings, sfx: { chime: "chime.ogg" }, flow: { newGame: { occasion: "visit" } } }
const porchBindings: Bindings = { ...bindings, flow: { newGame: { occasion: "begin" } } }

const overChime = <A, E, R>(use: Effect.Effect<A, E, R>) =>
  Effect.flatMap(compileChimeBatch, (bundle) => Effect.provide(use, hostLayer(bundle)))

describe("every cue of a batch starts (0.47.2 §2)", () => {
  const modes: ReadonlyArray<Autosave> = [false, "presentation"]
  for (const autosave of modes) {
    it.effect(`a camera and a sfx last before a choice are started before it is shown (autosave ${JSON.stringify(autosave)})`, () => overChime(Effect.scoped(Effect.gen(function* () {
      const recorded = recorder()
      const handle = yield* player(recorded, { autosave }, bindings)
      expect(yield* act(handle, { type: "newGame" })).toMatchObject({ story: "choice" })
      expect(recorded.effects).toEqual(["stage Camera", "play sfx chime.ogg", "choice"])
    }))))
  }

  it.effect("a sfx last before an idle await is played (default autosave)", () => overChime(Effect.scoped(Effect.gen(function* () {
    const recorded = recorder()
    const handle = yield* player(recorded, {}, porchBindings)
    expect(yield* act(handle, { type: "newGame" })).toMatchObject({ story: "idle" })
    expect(recorded.effects).toEqual(["play sfx chime.ogg"])
  }))))
})
