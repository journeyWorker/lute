// spec 0.47.0 §2.6/§4.1/§6: the title heading is the bound `ui.title.text`;
// without it no heading is drawn, never "Lute"; a title without
// `background` draws no backdrop. Mounted over compiled projects and the
// wasm runtime; assertions read the drawn Pixi tree.
import { Sprite } from "pixi.js"
import type { Container } from "pixi.js"
import { afterEach, describe, expect, it } from "vitest"
import { drawnTexts, isDrawn, mountProject, waitFor, type RealPlayer } from "../support/real-player.js"

const buttons = ["New game", "Continue", "Load", "Settings"]

const titleTexts = (player: RealPlayer): Promise<ReadonlyArray<string>> => waitFor(() => {
  const texts = drawnTexts(player.pixi.layers.overlay)
  return texts.includes("New game") ? texts : undefined
}, "the title screen")

const drawnSprites = (root: Container): number => {
  let count = 0
  const walk = (node: Container): void => {
    if (node instanceof Sprite && isDrawn(node)) count += 1
    for (const child of node.children) walk(child)
  }
  walk(root)
  return count
}

describe("title heading over compiled projects", () => {
  let player: RealPlayer | undefined
  afterEach(async () => {
    await player?.close()
    player = undefined
  })

  it("draws the demo's bound ui.title.text over its backdrop", async () => {
    player = await mountProject("demo")
    const texts = await titleTexts(player)
    expect(texts).toEqual(["The Signal at Low Tide", ...buttons])
    expect(drawnSprites(player.pixi.layers.overlay)).toBeGreaterThan(0)
  })

  it("draws a ui.title bound by text alone, with no backdrop", async () => {
    player = await mountProject("title-text")
    const texts = await titleTexts(player)
    expect(texts).toEqual(["Harbor Lights", ...buttons])
    expect(drawnSprites(player.pixi.layers.overlay)).toBe(0)
  })

  it("draws no heading (and never \"Lute\") without ui.title", async () => {
    player = await mountProject("cg-stage")
    const texts = await titleTexts(player)
    expect(texts).toEqual(buttons)
    expect(texts).not.toContain("Lute")
  })
})
