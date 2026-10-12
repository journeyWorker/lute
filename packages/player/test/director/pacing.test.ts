/** Spec 0.41.0 §7.5: presenter-paced lines and buffered confirms. */
import { Effect } from "effect"
import { describe, expect, it } from "@effect/vitest"
import { hubFixtureRoot } from "../../../runtime/test/support.js"
import { withHarness } from "./support.js"

describe("line pacing (§7.5)", () => {
  it.effect("presenter pacing: advance never reveals or releases; the presenter's completion releases", () => withHarness(hubFixtureRoot, { typing: true, linePacing: "presenter" }, (h) => Effect.gen(function* () {
    yield* h.action({ type: "newGame" })
    expect((yield* h.view).story).toBe("typing")
    const cleared = h.clears()
    yield* h.action({ type: "advance" })
    yield* h.action({ type: "advance" })
    expect(h.reveals()).toBe(0)
    expect(h.clears()).toBe(cleared)
    expect((yield* h.view).story).toBe("typing")
    yield* h.confirmLine
    expect(h.clears()).toBe(cleared + 1)
    expect((yield* h.view).story).toBe("choice")
  })))

  it.effect("presenter pacing: auto mode runs no line timer", () => withHarness(hubFixtureRoot, { typing: true, linePacing: "presenter" }, (h) => Effect.gen(function* () {
    yield* h.action({ type: "newGame" })
    yield* h.action({ type: "toggleAuto" })
    expect((yield* h.view).mode).toBe("auto")
    yield* h.tick(60_000)
    expect((yield* h.view).story).toBe("typing")
    yield* h.confirmLine
    expect((yield* h.view).story).toBe("choice")
  })))

  it.effect("presenter pacing: skip completes the typing line at once", () => withHarness(hubFixtureRoot, { typing: true, linePacing: "presenter" }, (h) => Effect.gen(function* () {
    yield* h.action({ type: "setSettings", settings: { skipMode: "all" } })
    yield* h.action({ type: "newGame" })
    expect((yield* h.view).story).toBe("typing")
    yield* h.action({ type: "toggleSkip" })
    expect(h.lineModes).toEqual(["normal", "skip"])
    expect((yield* h.view).story).toBe("choice")
  })))

  it.effect("confirmsLines: an advance while typing is applied as the release once the line is shown", () => withHarness(hubFixtureRoot, { typing: true, confirmsLines: true }, (h) => Effect.gen(function* () {
    yield* h.action({ type: "newGame" })
    const cleared = h.clears()
    yield* h.action({ type: "advance" })
    expect(h.reveals()).toBe(0)
    expect((yield* h.view).story).toBe("typing")
    // The presenter finishes typing: the buffered confirm releases the line.
    yield* h.confirmLine
    expect(h.clears()).toBe(cleared + 1)
    expect((yield* h.view).story).toBe("choice")
  })))

  it.effect("without confirmsLines an advance while typing reveals (0.40 behaviour)", () => withHarness(hubFixtureRoot, { typing: true }, (h) => Effect.gen(function* () {
    yield* h.action({ type: "newGame" })
    yield* h.action({ type: "advance" })
    expect(h.reveals()).toBe(1)
    expect((yield* h.view).story).toBe("shown")
  })))
})
