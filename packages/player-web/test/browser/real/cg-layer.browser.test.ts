// spec 0.47.0 §2.4/§6: a `::cg` covers the actors. The compiled `cg-stage`
// fixture shows `cg.reunion` (inset layout) while Mara stands on stage; the
// pixels where the CG and the actor overlap are the CG's.
import { Rectangle, type Container } from "pixi.js"
import { afterEach, describe, expect, it } from "vitest"
import { alphaOf, isDrawn, mountProject, playUntil, playthroughTimeout, stagePixels, waitFor, type RealPlayer } from "../support/real-player.js"

/** Bounds of the drawn leaves of `layer` once every one of them is fully faded in. */
const settledBounds = (layer: Container): ReadonlyArray<Rectangle> | undefined => {
  const leaves: Container[] = []
  const walk = (node: Container): void => {
    if (node.children.length === 0 && isDrawn(node)) leaves.push(node)
    for (const child of node.children) walk(child)
  }
  walk(layer)
  if (leaves.length === 0 || leaves.some((leaf) => alphaOf(leaf) < 1)) return undefined
  return leaves.map((leaf) => leaf.getBounds().rectangle)
}

const differs = (left: Uint8ClampedArray, right: Uint8ClampedArray): number => {
  let count = 0
  for (let index = 0; index < left.length; index += 4) {
    const delta = Math.max(Math.abs(left[index]! - right[index]!), Math.abs(left[index + 1]! - right[index + 1]!), Math.abs(left[index + 2]! - right[index + 2]!))
    if (delta > 2) count += 1
  }
  return count
}

describe("CG layer over the compiled cg-stage fixture", () => {
  let player: RealPlayer | undefined
  afterEach(async () => {
    await player?.close()
    player = undefined
  })

  it("draws a CG over the actors on stage and under the dialogue window", async () => {
    player = await mountProject("cg-stage")
    const { layers } = player.pixi
    await playUntil(player, () => layers.cg.children.some(isDrawn) && layers.actors.children.some(isDrawn))
    const [cgs, actors] = await waitFor(() => {
      const cg = settledBounds(layers.cg)
      const actor = settledBounds(layers.actors)
      return cg !== undefined && actor !== undefined ? [cg, actor] as const : undefined
    }, "the CG and the actors to settle")
    const cg = cgs[0]!
    // The dialogue window stays drawn above the CG: sample above it.
    const dialogueTop = layers.dialogue.getBounds().minY
    const overlaps = actors
      .map((actor) => new Rectangle(
        Math.ceil(Math.max(actor.x, cg.x)),
        Math.ceil(Math.max(actor.y, cg.y)),
        Math.floor(Math.min(actor.right, cg.right) - Math.max(actor.x, cg.x)),
        Math.floor(Math.min(actor.bottom, cg.bottom, dialogueTop) - Math.max(actor.y, cg.y))
      ))
      .filter((overlap) => overlap.width >= 8 && overlap.height >= 8)
    expect(overlaps.length).toBeGreaterThan(0)
    for (const overlap of overlaps) {
      const shown = stagePixels(player.pixi, overlap)
      layers.camera.visible = false
      const withoutActors = stagePixels(player.pixi, overlap)
      layers.cg.visible = false
      const neither = stagePixels(player.pixi, overlap)
      layers.camera.visible = true
      const withoutCg = stagePixels(player.pixi, overlap)
      layers.cg.visible = true
      // The actor really is drawn in this region when nothing covers it …
      expect(differs(withoutCg, neither)).toBeGreaterThan(0)
      // … and with the CG shown, removing the actors changes no pixel there.
      expect(differs(shown, withoutActors)).toBe(0)
    }
    expect(layers.cg.parent!.getChildIndex(layers.cg)).toBeGreaterThan(layers.cg.parent!.getChildIndex(layers.camera))
    expect(layers.cg.parent!.getChildIndex(layers.cg)).toBeLessThan(layers.cg.parent!.getChildIndex(layers.dialogue))
  }, playthroughTimeout())
})
