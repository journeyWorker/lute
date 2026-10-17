// spec 0.47.0 §2.2/§6: player-web draws translated text for the current
// locale setting. The demo is compiled with its Korean locale bundle
// (`examples/player-demo/locales/bundle.json`); the locale is switched to
// `ko` mid-run (while a line of the opening batch is shown) and kept on a
// new game, and a named line, a choice label and a prompt are drawn in
// Korean, and the `{{user.voyages}}` line with its value. Settings offers
// source + `ko`. Spec 0.47.0 §2.7: the opening's first line is drawn.
import { afterEach, describe, expect, it } from "vitest"
import { drawnTexts, mountProject, playReading, playthroughTimeout, shows, waitFor, type Reading, type RealPlayer } from "../support/real-player.js"

const source = {
  opening: "At six in the morning, the harbor keeps its secrets under a skin of silver water.",
  voyages: "Voyages to the lighthouse so far: 0.",
  mara: "The lighthouse blinked three times. That is not a weather signal.",
  prompt: "The tide is turning beneath the lighthouse.",
  tower: "Follow the signal to the lighthouse"
}
const korean = {
  opening: "아침 여섯 시, 항구는 은빛 물결 아래에 비밀을 숨기고 있다.",
  // `locales/ko.json`'s "지금까지 등대로 떠난 항해: {{user.voyages}}번." in run 1.
  voyages: "지금까지 등대로 떠난 항해: 0번.",
  mara: "등대가 세 번 깜박였어. 날씨 신호가 아니야.",
  prompt: "등대 아래에서 물길이 바뀌고 있다.",
  tower: "신호를 따라 등대로 간다",
  harbor: "항구에 남아 주변을 찾아본다",
  // After choosing `tower` in run 1: Mara's answer, then the `questions` hub.
  climb: "그럼 물이 길을 덮기 전에 올라가자.",
  hubPrompt: "안개가 기다린다. 떠나기 전에 무엇을 물을까?",
  past: "마라에게 어떻게 신호를 아는지 묻는다"
}

/** Open Settings, check it offers the source locale and `ko`, select `ko` and close. */
const chooseKorean = async (player: RealPlayer): Promise<void> => {
  await player.act({ type: "openSettings" })
  const settings = await waitFor(() => {
    const texts = drawnTexts(player.pixi.layers.overlay)
    return texts.includes("Locale") ? texts : undefined
  }, "the Settings screen")
  expect(settings).toContain("en-US")
  expect(settings).toContain("ko")
  await player.act({ type: "setSettings", settings: { locale: "ko" } })
  await player.act({ type: "close" })
  await waitFor(async () => (await player.view()).overlay === "none", "Settings to close")
}

const untilChoice = (reading: Reading): boolean => reading.choices.length > 0

const expectKoreanOpening = (reading: Reading): void => {
  expect(reading.lines.find((line) => shows(line, korean.mara)), JSON.stringify(reading)).toEqual(["Mara", korean.mara])
  const [menu] = reading.choices
  expect(menu).toBeDefined()
  expect(menu).toContain(korean.prompt)
  expect(menu).toContain(korean.tower)
  expect(menu).toContain(korean.harbor)
  expect(menu).not.toContain(source.prompt)
  expect(menu).not.toContain(source.tower)
}

describe("translated text over the compiled demo", () => {
  let player: RealPlayer | undefined
  afterEach(async () => {
    await player?.close()
    player = undefined
  })

  it("switches to ko mid-run and keeps it on a new game", async () => {
    const mounted = player = await mountProject("demo")
    // Run 1 in the source locale up to a line of the opening batch, then Settings mid-batch.
    const before = await playReading(mounted, (reading) => reading.lines.some((line) => shows(line, source.voyages)))
    expect(before.lines[0]).toEqual([source.opening])
    expect(before.lines.at(-1)).toEqual([source.voyages])
    await chooseKorean(mounted)
    // The next line of the same batch, the branch, then (after `tower`) the next batch and the hub.
    const midRun = await playReading(mounted, (reading) => reading.choices.some((menu) => shows(menu, korean.hubPrompt)), ["tower"])
    expectKoreanOpening(midRun)
    expect(midRun.lines.find((line) => shows(line, korean.climb)), JSON.stringify(midRun)).toEqual(["Mara", korean.climb])
    expect(midRun.choices.at(-1)).toContain(korean.hubPrompt)
    expect(midRun.choices.at(-1)).toContain(korean.past)

    // Back to title through the in-game menu (its confirm stays), then New game.
    await mounted.act({ type: "openMenu" })
    await mounted.act({ type: "toTitle" })
    await waitFor(async () => (await mounted.view()).overlay === "confirm", "the discard-progress confirm")
    await mounted.act({ type: "confirm", yes: true })
    await waitFor(async () => (await mounted.view()).flow === "title", "the title")
    await mounted.act({ type: "openSettings" })
    const settings = await waitFor(() => {
      const texts = drawnTexts(mounted.pixi.layers.overlay)
      return texts.includes("Locale") ? texts : undefined
    }, "the Settings screen at the title")
    expect(settings).toContain("en-US")
    expect(settings).toContain("ko")
    await mounted.act({ type: "close" })
    await waitFor(async () => (await mounted.view()).overlay === "none", "Settings to close")
    const newGame = await playReading(mounted, untilChoice)
    expect(newGame.lines.some((line) => shows(line, source.mara)), JSON.stringify(newGame)).toBe(false)
    expectKoreanOpening(newGame)
    expect(newGame.lines[0]).toEqual([korean.opening])
    expect(newGame.lines, JSON.stringify(newGame)).toContainEqual([korean.voyages])
  }, playthroughTimeout(3))
})
