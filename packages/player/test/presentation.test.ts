/** Spec 0.47.0 §2.7: the first line of a presentation is drawn — the presentation's clear runs before it, never after. */
import { Effect } from "effect"
import { describe, expect, it } from "@effect/vitest"
import { compileTallyKorean } from "./fixtures.js"
import { flush } from "./director/support.js"
import { hostLayer, player, recorder } from "./player.js"

describe("the first line of a presentation (§2.7)", () => {
  it.effect("the presentation clears the dialogue window before its first line is drawn, and the line stays", () =>
    Effect.flatMap(compileTallyKorean, (bundle) => Effect.provide(Effect.scoped(Effect.gen(function* () {
      const recorded = recorder()
      const handle = yield* player(recorded)
      yield* handle.actions({ type: "newGame" })
      yield* flush
      // The first line is shown and waits for the reader: nothing cleared it.
      expect(recorded.dialogue).toEqual(["clear", "line First 3, then 5."])
    })), hostLayer(bundle))))
})
