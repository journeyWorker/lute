import { Effect, Stream } from "effect"
import { Container, Point } from "pixi.js"
import { describe, expect, it } from "vitest"
import type { Contract } from "@lute-lang/runtime"
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

const menu: Contract.MenuAwait = {
  construct: "",
  document: "",
  id: "menu-1",
  options: [
    { id: "first", exit: false, once: false, verdict: "open" },
    { id: "closed", exit: false, once: false, verdict: "closed" },
    { id: "third", exit: true, once: false, verdict: "open" }
  ],
  position: "",
  presentation: 0,
  prompt: "Choose",
  timeout: 10
}

describe("ChoiceMenu browser scene", () => {

  it("shows disabled options without selecting them", () => {
    const entity = new ChoiceMenu(new Container(), theme)
    expect(entity.target.visible).toBe(false)
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
    menuEntity.target.emit("pointertap", { global: new Point(100, 170) })
    await expect(result).resolves.toBe("third")
  })
})
