import { describe, expect, it } from "vitest"
import { choiceNavigation, navigateChoice } from "../../src/choice/choiceMenu.js"

describe("choice navigation", () => {
  const state = choiceNavigation([
    { id: "a", verdict: "open" },
    { id: "b", verdict: "closed" },
    { id: "c", verdict: "open" }
  ])

  it("skips disabled options while moving focus", () => {
    expect(state.focused).toBe(0)
    const next = navigateChoice(state, "down")
    if (typeof next === "object" && next !== undefined) expect(next.focused).toBe(2)
    else throw new Error("expected navigation state")
  })

  it("selects focused and digit options, but never disabled ones", () => {
    expect(navigateChoice(state, "enter")).toBe("a")
    expect(navigateChoice(state, "2")).toBeUndefined()
    expect(navigateChoice(state, "3")).toBe("c")
  })
})
