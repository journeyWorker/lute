import { Effect, Stream } from "effect"
import { Container, EventBoundary, FederatedPointerEvent, Point, Text } from "pixi.js"
import { describe, expect, it } from "vitest"
import type { ChoiceView } from "@lute-lang/player"
import type { Theme } from "../../../src/engine/types.js"
import { ChoiceMenu } from "../../../src/choice/choiceMenu.js"

const theme: Theme = {
  font: "sans-serif",
  text: 0xffffff,
  accent: 0xffcc66,
  panel: 0x111122,
  panelAlpha: 0.9,
  readText: 0x99aacc
}

const menu: ChoiceView = {
  id: "menu-1",
  construct: "branch",
  prompt: "Choose",
  timeout: 10,
  default: "first",
  options: [
    { id: "first", text: "First way", exit: false, once: false, verdict: "open" },
    { id: "closed", text: "Closed way", exit: false, once: false, verdict: "closed" },
    { id: "third", text: "Third way", exit: true, once: false, verdict: "open" }
  ]
}

describe("ChoiceMenu browser scene", () => {

  it("shows disabled options without selecting them", () => {
    const entity = new ChoiceMenu(new Container(), theme)
    expect(entity.target.visible).toBe(false)
  })

  it("renders the prompt and the authored option text, not option ids", async () => {
    const entity = new ChoiceMenu(new Container(), theme)
    const result = Effect.runPromise(entity.show(menu, Stream.empty))
    const texts: string[] = []
    const collect = (node: Container): void => {
      if (node instanceof Text) texts.push(node.text)
      for (const child of node.children) collect(child)
    }
    collect(entity.target)
    expect(texts).toEqual(expect.arrayContaining(["Choose", "First way", "Closed way", "Third way"]))
    expect(texts).not.toContain("first")
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "1" }))
    await expect(result).resolves.toBe("first")
  })

  it("supports keyboard navigation and enter", async () => {
    const entity = new ChoiceMenu(new Container(), theme)
    const result = Effect.runPromise(entity.show(menu, Stream.empty))
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown" }))
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }))
    await expect(result).resolves.toBe("third")
  })

  it("supports digit selection", async () => {
    const entity = new ChoiceMenu(new Container(), theme)
    const result = Effect.runPromise(entity.show(menu, Stream.empty))
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "3" }))
    await expect(result).resolves.toBe("third")
  })
  it("selects by keyboard and ignores disabled options", async () => {
    const menuEntity = new ChoiceMenu(new Container(), theme)
    const result = Effect.runPromise(menuEntity.show(menu, Stream.make(10, 3)))
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown" }))
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter" }))
    await expect(result).resolves.toBe("third")
  })

  it("selects by digit and follows countdown remaining", async () => {
    const menuEntity = new ChoiceMenu(new Container(), theme)
    const result = Effect.runPromise(menuEntity.show(menu, Stream.make(10)))
    menuEntity.setRemaining(5)
    menuEntity.setRemaining(2)
    window.dispatchEvent(new KeyboardEvent("keydown", { key: "3" }))
    await expect(result).resolves.toBe("third")
    await Effect.runPromise(menuEntity.hide)
    expect(menuEntity.target.visible).toBe(false)
  })

  it("resolves a pointer click on an enabled option", async () => {
    const menuEntity = new ChoiceMenu(new Container(), theme)
    const result = Effect.runPromise(menuEntity.show(menu, Stream.empty))
    const tap = new FederatedPointerEvent(new EventBoundary(menuEntity.target))
    tap.global.copyFrom(new Point(100, 170))
    menuEntity.target.emit("pointertap", tap)
    await expect(result).resolves.toBe("third")
  })
})
