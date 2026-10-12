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

  it("wraps focus upward and downward across enabled options", () => {
    const down = navigateChoice(state, "down")
    if (typeof down !== "object" || down === undefined) throw new Error("expected navigation state")
    const wrapped = navigateChoice(down, "down")
    if (typeof wrapped !== "object" || wrapped === undefined) throw new Error("expected navigation state")
    expect(wrapped.focused).toBe(0)
    const up = navigateChoice(state, "up")
    if (typeof up !== "object" || up === undefined) throw new Error("expected navigation state")
    expect(up.focused).toBe(2)
  })

  it("moves focus down", () => {
    const next = navigateChoice(state, "down")
    if (typeof next === "object" && next !== undefined) expect(next.focused).toBe(2)
  })

  it("moves focus up", () => {
    const next = navigateChoice(state, "up")
    if (typeof next === "object" && next !== undefined) expect(next.focused).toBe(2)
  })

  it("wraps focus", () => {
    const next = navigateChoice(state, "down")
    if (typeof next === "object" && next !== undefined) {
      const wrapped = navigateChoice(next, "down")
      if (typeof wrapped === "object" && wrapped !== undefined) expect(wrapped.focused).toBe(0)
    }
  })

  it("selects by digit", () => {
    expect(navigateChoice(state, "3")).toBe("c")
  })

  it("selects by enter", () => {
    expect(navigateChoice(state, "enter")).toBe("a")
  })

  it("does not select disabled options", () => {
    expect(navigateChoice(state, "2")).toBeUndefined()
  })

  it("selects focused and digit options, but never disabled ones", () => {
    expect(navigateChoice(state, "enter")).toBe("a")
    expect(navigateChoice(state, "2")).toBeUndefined()
    expect(navigateChoice(state, "3")).toBe("c")
  })
})
