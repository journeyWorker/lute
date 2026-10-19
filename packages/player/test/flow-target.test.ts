/**
 * Lute 0.47.1 §2 (#133): a bindings `flow` target is the full occasion
 * target `raiseOccasion` takes. Over the compiled `flow-target-0471` fixture
 * (which `lute check --project` accepts, `crates/lute-cli/tests/flow_target_0471.rs`)
 * the idle menu raises `visit` for `place.square` and the beat targeted at
 * it is presented; the bare member the checker now refuses reaches no beat.
 */
import { readFileSync } from "node:fs"
import { join } from "node:path"
import { Effect, SubscriptionRef } from "effect"
import { describe, expect, it } from "@effect/vitest"
import { compileBundle } from "../../runtime/test/support.js"
import { decode } from "../src/bindings/load.js"
import type { Bindings } from "../src/bindings/schema.js"
import { checkProject, flowTargetProject } from "./fixtures.js"
import { act, hostLayer, player, recorder, textOf } from "./player.js"

const fixtureBindings = decode(readFileSync(join(flowTargetProject, "player.bindings.yaml"), "utf8"))

const overTown = <A, E, R>(use: Effect.Effect<A, E, R>) =>
  Effect.flatMap(compileBundle(flowTargetProject), (bundle) => Effect.provide(use, hostLayer(bundle)))

describe("flow targets are raised as written (0.47.1 §2)", () => {
  it.effect("the idle menu raises `place.square`, which `check --project` accepts, and its beat is presented", () => overTown(Effect.scoped(Effect.gen(function* () {
    // The bindings raised below are the ones `lute check --project` accepts.
    yield* checkProject(flowTargetProject)
    const bindings = yield* fixtureBindings
    expect(bindings.flow?.idle).toEqual({ menu: [
      { label: "The square", occasion: "visit", target: "place.square" },
      { label: "The pier", occasion: "visit", target: "place.pier" }
    ] })
    const recorded = recorder()
    const handle = yield* player(recorded, {}, bindings)
    expect(yield* act(handle, { type: "newGame" })).toMatchObject({ flow: "playing", story: "idle" })
    expect(recorded.lines.map(textOf)).toEqual(["The town wakes."])
    expect(recorded.screens.at(-1)).toEqual({ _tag: "Idle", options: [{ label: "The square" }, { label: "The pier" }] })
    yield* act(handle, { type: "pickIdle", index: 0 })
    expect(recorded.lines.map(textOf)).toEqual(["The town wakes.", "The square is busy."])
    expect((yield* handle.snapshot).log.at(-1)).toEqual({ input: { type: "raiseOccasion", occasion: "visit", target: "place.square" } })
    expect((yield* SubscriptionRef.get(handle.view)).story).toBe("idle")
  }))))

  it.effect("a bare member reaches no beat", () => overTown(Effect.scoped(Effect.gen(function* () {
    const decoded = yield* fixtureBindings
    const bindings: Bindings = { ...decoded, flow: { ...decoded.flow, idle: { menu: [{ label: "The square", occasion: "visit", target: "square" }] } } }
    const recorded = recorder()
    const handle = yield* player(recorded, {}, bindings)
    yield* act(handle, { type: "newGame" })
    yield* act(handle, { type: "pickIdle", index: 0 })
    expect(recorded.lines.map(textOf)).toEqual(["The town wakes."])
    expect((yield* handle.snapshot).log.at(-1)).toEqual({ input: { type: "raiseOccasion", occasion: "visit", target: "square" } })
  }))))
})
