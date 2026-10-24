/** Spec 0.47.0 §2.5: leaving an ended run needs no confirm; the in-game menu and the error flow keep it. */
import { Effect } from "effect"
import { describe, expect, it } from "@effect/vitest"
import { join } from "node:path"
import { compileBundle, repositoryRoot } from "../../../runtime/test/support.js"
import type { Bindings } from "../../src/bindings/schema.js"
import { quayProject } from "../fixtures.js"
import { withHarness } from "./support.js"
import type { Harness } from "./support.js"

const canvas = { width: 1920, height: 1080 }
const quayBindings: Bindings = { bindingsVersion: "0.49", canvas, flow: { newGame: { occasion: "visit" } } }
const gateFixture = join(repositoryRoot, "crates", "lute-cli", "tests", "fixtures", "runtime-gate-041")

const drain = (h: Harness) => Effect.gen(function* () {
  for (let i = 0; i < 100; i++) {
    const view = yield* h.view
    if (view.story !== "typing" && view.story !== "shown" && view.story !== "running") return
    yield* h.action({ type: "advance" })
  }
  throw new Error("director did not settle")
})

/** New game, the branch (answered `dock` by the harness), read on to `ended` (the ending screen shows). */
const toEnded = (h: Harness) => Effect.gen(function* () {
  yield* h.action({ type: "newGame" }); yield* drain(h)
  yield* drain(h)
  expect(yield* h.view).toMatchObject({ flow: "ended", story: "ended" })
  expect(h.screens.at(-1)).toMatchObject({ _tag: "Ended" })
})

describe("leaving an ended run (§2.5)", () => {
  it.effect("toTitle at ended goes to the title at once, without the discard-progress confirm", () => Effect.gen(function* () {
    const bundle = yield* compileBundle(quayProject)
    yield* withHarness(bundle, { bindings: quayBindings, choices: ["dock"] }, (h) => Effect.gen(function* () {
      yield* toEnded(h)
      const shown = h.screens.length
      yield* h.action({ type: "toTitle" })
      expect(h.screens.slice(shown).map((screen) => screen._tag)).toEqual(["Title"])
      expect(yield* h.view).toMatchObject({ flow: "title", overlay: "none" })
      // The title starts the next run.
      yield* h.action({ type: "newGame" }); yield* drain(h)
      expect(yield* h.view).toMatchObject({ flow: "playing", story: "choice" })
    }))
  }))

  it.effect("after leaving an ended run, toTitle during the next run's first batch asks the confirm again", () => Effect.gen(function* () {
    const bundle = yield* compileBundle(quayProject)
    yield* withHarness(bundle, { bindings: quayBindings, choices: ["dock"] }, (h) => Effect.gen(function* () {
      yield* toEnded(h)
      yield* h.action({ type: "toTitle" })
      // New game: the first line of run 2 waits for the reader while its batch runs.
      yield* h.action({ type: "newGame" })
      expect(yield* h.view).toMatchObject({ flow: "playing", story: "shown" })
      yield* h.action({ type: "toTitle" })
      expect(h.screens.at(-1)).toMatchObject({ _tag: "Confirm" })
      expect(yield* h.view).toMatchObject({ flow: "playing", overlay: "confirm" })
    }))
  }))

  it.effect("toTitle from the in-game menu at ended keeps the confirm", () => Effect.gen(function* () {
    const bundle = yield* compileBundle(quayProject)
    yield* withHarness(bundle, { bindings: quayBindings, choices: ["dock"] }, (h) => Effect.gen(function* () {
      yield* toEnded(h)
      yield* h.action({ type: "openMenu" })
      yield* h.action({ type: "toTitle" })
      expect(h.screens.at(-1)).toMatchObject({ _tag: "Confirm" })
      expect(yield* h.view).toMatchObject({ flow: "ended", overlay: "confirm" })
      yield* h.action({ type: "confirm", yes: true })
      expect(h.screens.at(-1)).toMatchObject({ _tag: "Title" })
      expect((yield* h.view).flow).toBe("title")
    }))
  }))

  it.effect("toTitle in the error flow keeps the confirm", () => Effect.gen(function* () {
    const bundle = yield* compileBundle(gateFixture)
    yield* withHarness(bundle, { bindings: { bindingsVersion: "0.49", canvas } }, (h) => Effect.gen(function* () {
      yield* h.action({ type: "newGame" })
      yield* h.director.send({ type: "raiseOccasion", occasion: "listen" }); yield* drain(h)
      expect(yield* h.view).toMatchObject({ flow: "error", story: "halted" })
      yield* h.action({ type: "toTitle" })
      expect(h.screens.at(-1)).toMatchObject({ _tag: "Confirm" })
      expect(yield* h.view).toMatchObject({ flow: "error", overlay: "confirm" })
    }))
  }))
})
