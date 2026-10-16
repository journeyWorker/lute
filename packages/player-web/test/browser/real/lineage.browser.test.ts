// spec 0.47.0 §2.3/§6: a second play-through survives a page reload. Run 1
// of the compiled demo is played to its end, the player is mounted again
// over the same storage (a reload), and New game draws run 2's
// `prev.run`-gated line and the `user.voyages` count run 1 left.
import { afterEach, describe, expect, it } from "vitest"
import { mountProject, playReading, shows, type RealPlayer } from "../support/real-player.js"

const remembered = "We have climbed toward that light before. I still remember the tide."
const voyages = (count: number): string => `Voyages to the lighthouse so far: ${count}.`

describe("lineage across a reload over the compiled demo", () => {
  let player: RealPlayer | undefined
  afterEach(async () => {
    await player?.close()
    player = undefined
  })

  it("run 2 after a reload draws the prev.run line and run 1's user value", async () => {
    const storageKey = `lute-test-lineage-${crypto.randomUUID()}`
    player = await mountProject("demo", { storageKey })
    const first = await playReading(player, (_reading, view) => view.flow === "ended", ["release"], undefined, 180_000)
    expect(first.lines.some((line) => shows(line, voyages(0)))).toBe(true)
    expect(first.lines.some((line) => shows(line, remembered))).toBe(false)
    await player.close()

    player = await mountProject("demo", { storageKey })
    const second = await playReading(player, (reading) => reading.lines.some((line) => shows(line, "The lighthouse blinked three times.")))
    const gated = second.lines.find((line) => shows(line, remembered))
    expect(gated).toEqual(["Mara", remembered])
    expect(second.lines.some((line) => shows(line, voyages(1)))).toBe(true)
  }, 300_000)
})
