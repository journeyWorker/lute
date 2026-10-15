import { Effect } from "effect"
import { describe, expect, it } from "vitest"
import { resolveEndedScreen, type EndedProgramView } from "../../src/ui/ended.js"

const screen = { _tag: "Ended" as const, reason: "terminal" as const }
const binding = { by: "run.ending", screens: { restore: { title: "Restored", text: "A new dawn" } } }
const program = (value: unknown, fail = false): { view: (state: unknown) => Effect.Effect<EndedProgramView, unknown> } => ({ view: () => fail ? Effect.fail("query") as Effect.Effect<EndedProgramView, unknown> : Effect.succeed({ state: { "run.ending": value } }) })

describe("resolveEndedScreen", () => {
  it("maps a string member to its configured screen", async () => {
    await expect(Effect.runPromise(resolveEndedScreen(program("restore"), {}, binding, screen))).resolves.toMatchObject({ title: "Restored", text: "A new dawn" })
  })
  it.each([undefined, "unknown", 7])("returns undefined for %s", async (value) => {
    await expect(Effect.runPromise(resolveEndedScreen(program(value), {}, binding, screen))).resolves.toBeUndefined()
  })
  it("returns undefined for a failed query or absent binding", async () => {
    await expect(Effect.runPromise(resolveEndedScreen(program("restore", true), {}, binding, screen))).resolves.toBeUndefined()
    await expect(Effect.runPromise(resolveEndedScreen(program("restore"), {}, undefined, screen))).resolves.toBeUndefined()
  })
})
