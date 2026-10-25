/**
 * #200: a plugin may declare stamps named `kind` and `position` (0.49.0 §3.1
 * nests stamps under `stamps`). The player looks commands up by position
 * among the IR's `commands` only, so such a `stamps` map never stands in for
 * a command. Over the compiled `stamp-keys` fixture.
 */
import { Effect } from "effect"
import { describe, expect, it } from "@effect/vitest"
import { compileStampKeys } from "./fixtures.js"
import { act, hostLayer, player, recorder, textOf } from "./player.js"

describe("stamps named kind and position (#200)", () => {
  it.effect("a stamps map whose kind/position name an earlier or its own command shadows neither", () =>
    Effect.flatMap(compileStampKeys, (bundle) => Effect.provide(Effect.scoped(Effect.gen(function* () {
      const recorded = recorder()
      const handle = yield* player(recorded)
      const view = yield* act(handle, { type: "newGame" })
      expect(recorded.lines.map(textOf)).toEqual(["The tide is out.", "Gulls circle the mast."])
      expect(recorded.effects).toEqual(["stage Background"])
      expect(view.story).toBe("ended")
    })), hostLayer(bundle))))
})
