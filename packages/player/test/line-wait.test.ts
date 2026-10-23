/**
 * Lute 0.49.0 §4.2 (#174): a line's `timing.wait: false` lets the staging
 * cues after it start while it is presented; the next line still waits for
 * its release. Over the compiled `line-wait` fixture.
 */
import { Effect } from "effect"
import { describe, expect, it } from "@effect/vitest"
import { flush } from "./director/support.js"
import { compileLineWait } from "./fixtures.js"
import { hostLayer, player, quayBindings, recorder, textOf } from "./player.js"

const overLineWait = <A, E, R>(use: Effect.Effect<A, E, R>) =>
  Effect.flatMap(compileLineWait, (bundle) => Effect.provide(use, hostLayer(bundle)))

describe("line wait (0.49.0 §4.2)", () => {
  it.effect("a wait=\"false\" line starts the following bg before its advance; the next line still waits", () => overLineWait(Effect.scoped(Effect.gen(function* () {
    const recorded = recorder()
    const handle = yield* player(recorded, {}, quayBindings)

    yield* handle.actions({ type: "newGame" })
    yield* flush
    // The `wait="false"` line is presented and the night bg has started with no advance.
    expect(recorded.lines.map(textOf)).toEqual(["The lamp flickers."])
    expect(recorded.effects).toEqual(["stage Background"])

    yield* handle.actions({ type: "advance" })
    yield* flush
    // The waiting line holds the dawn bg until its own advance.
    expect(recorded.lines.map(textOf)).toEqual(["The lamp flickers.", "The road is dark."])
    expect(recorded.effects).toEqual(["stage Background"])

    yield* handle.actions({ type: "advance" })
    yield* flush
    expect(recorded.effects).toEqual(["stage Background", "stage Background"])
  }))))
})
