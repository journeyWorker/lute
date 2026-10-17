// spec 0.47.0 §2.1/§6: each member of the demo's multi-ending case
// (`ui.ended.by: run.path`) draws its mapped ended screen, played to the
// end over the compiled demo and the wasm runtime.
import { afterEach, describe, expect, it } from "vitest"
import { drawnTexts, mountProject, playUntil, playthroughTimeout, waitFor, type RealPlayer } from "../support/real-player.js"

const endings = [
  { member: "dawn", prefer: ["release"], title: "Dawn: The Shore Remembers", text: "Mara and Elio let the light go to the sea." },
  { member: "dusk", prefer: ["hold"], title: "Dusk: The Lantern Held", text: "The lantern stays closed, and the harbor keeps its signal." }
] as const

describe("ended screens over the compiled demo", () => {
  let player: RealPlayer | undefined
  afterEach(async () => {
    await player?.close()
    player = undefined
  })

  it.each(endings)("draws the $member ending's mapped title", async ({ prefer, title, text }) => {
    player = await mountProject("demo")
    await playUntil(player, (view) => view.flow === "ended", prefer)
    const overlay = player.pixi.layers.overlay
    const drawn = await waitFor(() => {
      const texts = drawnTexts(overlay)
      return texts.includes("Back to title") ? texts : undefined
    }, "the ended screen")
    expect(drawn).toContain(title)
    expect(drawn).toContain(text)
    expect(drawn).not.toContain("The End")
  }, playthroughTimeout())
})
