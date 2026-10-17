/**
 * Lute 0.47.1 §3 (#134): `\{{run.a}}` in a line is literal text. Over the
 * compiled `escape-locale` fixture the player shows the runtime's rendered
 * record — the literal `{{run.a}}` beside the substituted marker — in the
 * source text, a modified line's runs, and a Korean translation that writes
 * `\{{run.a}}` too.
 */
import { Effect } from "effect"
import { describe, expect, it } from "@effect/vitest"
import { compileEscapeKorean } from "./fixtures.js"
import { act, hostLayer, player, recorder, textOf } from "./player.js"

const overEscape = <A, E, R>(use: Effect.Effect<A, E, R>) =>
  Effect.flatMap(compileEscapeKorean, (bundle) => Effect.provide(use, hostLayer(bundle)))

describe("an escaped marker is literal text (0.47.1 §3)", () => {
  it.effect("the source locale shows the literal and the value", () => overEscape(Effect.scoped(Effect.gen(function* () {
    const recorded = recorder()
    const handle = yield* player(recorded)
    yield* act(handle, { type: "newGame" })
    expect(recorded.lines.map(textOf)).toEqual(["Lit {{run.a}} and 3.", "Lit {{run.a}} and 3."])
    expect(recorded.lines[1]?.segments).toEqual([
      { _tag: "Text", text: "Lit {{run.a}}", styles: [], rate: 1.5 },
      { _tag: "Text", text: " and 3.", styles: [] }
    ])
  }))))

  it.effect("a Korean translation's escaped marker stays literal too", () => overEscape(Effect.scoped(Effect.gen(function* () {
    const recorded = recorder()
    const handle = yield* player(recorded)
    yield* act(handle, { type: "setSettings", settings: { locale: "ko" } })
    yield* act(handle, { type: "newGame" })
    expect(recorded.lines.map(textOf)).toEqual(["리터럴 {{run.a}} 그리고 3.", "리터럴 {{run.a}} 그리고 3."])
    expect(recorded.lines[1]?.segments).toEqual([
      { _tag: "Text", text: "리터럴 {{run.a}}", styles: [], rate: 1.5 },
      { _tag: "Text", text: " 그리고 3.", styles: [] }
    ])
  }))))
})
