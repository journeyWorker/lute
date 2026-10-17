// spec 0.47.0 §2.3/§6: a second play-through survives a page reload. Run 1
// of the compiled demo is played to its end, the player is mounted again
// over the same storage (a reload), and New game draws run 2's
// `prev.run`-gated line and the `user.voyages` count run 1 left. §2.2: run 2
// played again in Korean draws the translated count with its value.
import { afterEach, describe, expect, it } from "vitest"
import { mountProject, playReading, playthroughTimeout, shows, waitFor, type RealPlayer } from "../support/real-player.js"

const remembered = "We have climbed toward that light before. I still remember the tide."
const voyages = (count: number): string => `Voyages to the lighthouse so far: ${count}.`
// `examples/player-demo/locales/ko.json`: "지금까지 등대로 떠난 항해: {{user.voyages}}번."
const koreanVoyages = (count: number): string => `지금까지 등대로 떠난 항해: ${count}번.`
const koreanRemembered = "우리, 저 불빛을 향해 올라간 적이 있어. 아직도 그 물때가 기억나."

describe("lineage across a reload over the compiled demo", () => {
  let player: RealPlayer | undefined
  afterEach(async () => {
    await player?.close()
    player = undefined
  })

  it("run 2 after a reload draws the prev.run line and run 1's user value", async () => {
    const storageKey = `lute-test-lineage-${crypto.randomUUID()}`
    player = await mountProject("demo", { storageKey })
    const first = await playReading(player, (_reading, view) => view.flow === "ended", ["release"])
    expect(first.lines.some((line) => shows(line, voyages(0)))).toBe(true)
    expect(first.lines.some((line) => shows(line, remembered))).toBe(false)
    await player.close()

    player = await mountProject("demo", { storageKey })
    const second = await playReading(player, (reading) => reading.lines.some((line) => shows(line, "The lighthouse blinked three times.")))
    const gated = second.lines.find((line) => shows(line, remembered))
    expect(gated).toEqual(["Mara", remembered])
    expect(second.lines.some((line) => shows(line, voyages(1)))).toBe(true)

    // Back to title mid-run, Korean, New game: run 2 again from the lineage.
    const mounted = player
    await mounted.act({ type: "openMenu" })
    await mounted.act({ type: "toTitle" })
    await waitFor(async () => (await mounted.view()).overlay === "confirm", "the discard-progress confirm")
    await mounted.act({ type: "confirm", yes: true })
    await waitFor(async () => (await mounted.view()).flow === "title", "the title")
    await mounted.act({ type: "setSettings", settings: { locale: "ko" } })
    const korean = await playReading(mounted, (reading) => reading.lines.some((line) => shows(line, koreanVoyages(1))))
    expect(korean.lines.find((line) => shows(line, koreanRemembered)), JSON.stringify(korean)).toEqual(["Mara", koreanRemembered])
    expect(korean.lines.at(-1)).toEqual([koreanVoyages(1)])
  }, playthroughTimeout(3))
})
