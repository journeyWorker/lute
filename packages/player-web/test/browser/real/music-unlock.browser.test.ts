// spec 0.47.2 §3 (#138): a load completes while audio is locked.
// Over the compiled player-demo: play with device input to a line whose stage
// has music (chapter one's `calm`) and quick save; mount again over the same
// storage (a fresh player, audio locked); load with the host API and no user
// gesture. The restored await is reached with no music playing; a click then
// unlocks audio and the stage's music (not the title's) plays, read from the
// real @pixi/sound library.
import { sound } from "@pixi/sound"
import { afterEach, describe, expect, it } from "vitest"
import { inject } from "vitest"
import { mountProject, playReading, playthroughTimeout, waitFor, type RealPlayer } from "../support/real-player.js"

/** The alias @pixi/sound knows a bound file by: its URL below the assets base. */
const soundAlias = (file: string): string => new URL(file, new URL(inject("luteProjects").demo.assetsUrl, location.origin)).href

const playing = (alias: string): boolean => sound.exists(alias, false) && sound.find(alias).isPlaying

describe("loading a save with stage music before audio is unlocked", () => {
  let player: RealPlayer | undefined
  afterEach(async () => {
    await player?.close()
    player = undefined
    sound.stopAll()
    sound.removeAll()
  })

  it("reaches the restored await at once and starts the stage's music on the first gesture", async () => {
    const stage = soundAlias("audio/calm.wav")
    const title = soundAlias("ui/title.wav")
    const storageKey = `lute-test-music-unlock-${crypto.randomUUID()}`

    // Play (Escape unlocks audio) to the first line, after chapter one's `::music`.
    player = await mountProject("demo", { storageKey })
    await playReading(player, (reading) => reading.lines.length > 0)
    await waitFor(() => playing(stage), "the stage music to play")
    await player.act({ type: "quickSave" })
    await waitFor(() => Object.keys(localStorage).some((key) => key.startsWith(`${storageKey}:`) && key.includes("quick")), "the quick save to be written")
    await player.close()
    sound.stopAll()
    sound.removeAll()

    // A fresh page over the same storage: audio locked, the title music waiting.
    player = await mountProject("demo", { storageKey })
    await waitFor(async () => (await player!.view()).flow === "title", "the title")
    await player.act({ type: "continue" })
    const restored = await waitFor(async () => {
      const view = await player!.view()
      // The save is the runtime's await: chapter one's first choice.
      return view.flow === "playing" && view.story === "choice" ? view : undefined
    }, "the restored await")
    expect(restored.overlay).toBe("none")
    // Audio is still locked: no music plays (the stage's file is loaded with its assets).
    expect(playing(stage)).toBe(false)
    expect(playing(title)).toBe(false)

    // The first gesture: a click (pointer press and release, a corner of the stage).
    for (const type of ["pointerdown", "pointerup"]) {
      player.element.dispatchEvent(new PointerEvent(type, { pointerId: 1, pointerType: "mouse", button: 0, clientX: 4, clientY: 4, bubbles: true, cancelable: true }))
    }
    await waitFor(() => playing(stage), "the stage music to play after the click")
    // The title music was superseded before its file was even loaded.
    expect(sound.exists(title, false)).toBe(false)
  }, playthroughTimeout())
})
