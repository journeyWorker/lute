/** Spec 0.50.0 §3.6: `showChoice` carries the stamps authored on the logic elements, read from the IR. */
import { Effect } from "effect"
import { describe, expect, it } from "@effect/vitest"
import { compileChoiceStamps } from "./fixtures.js"
import { act, hostLayer, player, recorder } from "./player.js"

describe("ChoiceView stamps (§3.6)", () => {
  it.effect("the element's and each option's authored stamps reach showChoice; unauthored ones are absent", () =>
    Effect.flatMap(compileChoiceStamps, (bundle) => Effect.provide(Effect.scoped(Effect.gen(function* () {
      const recorded = recorder()
      const handle = yield* player(recorded)
      yield* act(handle, { type: "newGame" })
      const branch = recorded.choices.at(-1)
      expect(branch).toMatchObject({ id: "route", construct: "branch", stamps: { cue: "b1", menuType: "timed" } })
      expect(branch?.options).toStrictEqual([
        { id: "left", text: "Left", verdict: "open", once: false, exit: false, stamps: { cue: "c1", weight: 2 } },
        { id: "right", text: "Right", verdict: "open", once: false, exit: false }
      ])
      yield* act(handle, { type: "choose", option: "left" })
      const hub = recorded.choices.at(-1)
      expect(hub).toMatchObject({ id: "stalls", construct: "hub" })
      expect(hub !== undefined && "stamps" in hub).toBe(false)
      expect(hub?.options.map((option) => "stamps" in option)).toEqual([false, false])
    })), hostLayer(bundle))))
})
