// spec 0.47.0 §2.5/§6: the ended screen's Back to title reaches the title
// without the discard-progress confirm, over a compiled project played to
// its end on the wasm runtime.
import { afterEach, describe, expect, it } from "vitest"
import { buttonLabelled, drawnTexts, mountProject, playReading, playthroughTimeout, waitFor, type RealPlayer } from "../support/real-player.js"

describe("leaving an ended run", () => {
  let player: RealPlayer | undefined
  afterEach(async () => {
    await player?.close()
    player = undefined
  })

  it("Back to title on the ended screen reaches the title with no confirm drawn", async () => {
    const mounted = player = await mountProject("cg-stage")
    await playReading(mounted, (_reading, view) => view.flow === "ended")
    const overlay = mounted.pixi.layers.overlay
    const back = await waitFor(() => buttonLabelled(overlay, "Back to title"), "the ended screen's Back to title")
    back.activate()
    const seen: Array<ReadonlyArray<string>> = []
    const overlays: Array<string> = []
    const title = await waitFor(async () => {
      const view = await mounted.view()
      overlays.push(view.overlay)
      const texts = drawnTexts(overlay)
      seen.push(texts)
      return view.flow === "title" && texts.includes("New game") ? texts : undefined
    }, "the title screen")
    expect(title).toContain("New game")
    expect(overlays).not.toContain("confirm")
    expect(seen.flat()).not.toContain("Yes")
  }, playthroughTimeout())
})
