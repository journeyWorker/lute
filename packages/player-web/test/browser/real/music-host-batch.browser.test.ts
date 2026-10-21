// spec 0.48.1 (#142): a host-driven batch never hangs on locked audio.
// Over the compiled player-demo: `act({ type: "newGame" })` with no user
// gesture runs chapter one's opening batch, whose `::music` cue cannot start
// while audio is locked. Reading on with host actions alone, the batch settles at
// the first choice (a save point with stage music) and no music plays yet; a later
// gesture unlocks audio and starts the stage's music, read from the real
// @pixi/sound library.
import { sound } from "@pixi/sound"
import { afterEach, describe, expect, it } from "vitest"
import { inject } from "vitest"
import { mountProject, waitFor, type RealPlayer } from "../support/real-player.js"

/** The alias @pixi/sound knows a bound file by: its URL below the assets base. */
const soundAlias = (file: string): string => new URL(file, new URL(inject("luteProjects").demo.assetsUrl, location.origin)).href

const playing = (alias: string): boolean => sound.exists(alias, false) && sound.find(alias).isPlaying

describe("a host-driven batch with stage music before audio is unlocked", () => {
  let player: RealPlayer | undefined
  afterEach(async () => {
    await player?.close()
    player = undefined
    sound.stopAll()
    sound.removeAll()
  })

  it("settles at the stage-music save point with no gesture, and a later gesture starts the music", async () => {
    const stage = soundAlias("audio/calm.wav")
    player = await mountProject("demo")
    await waitFor(async () => (await player!.view()).flow === "title", "the title")
    await player.act({ type: "setSettings", settings: { textSpeed: 0 } })
    // No gesture: the host API alone drives the batch, which must not wait for the audio unlock.
    await player.act({ type: "newGame" })
    await waitFor(async () => (await player!.view()).story === "idle", "the idle menu")
    await player.act({ type: "pickIdle", index: 0 })
    // Read to the first choice with host actions alone: the batch ends there, joining the music cue.
    await waitFor(async () => {
      const view = await player!.view()
      if (view.story === "shown") await player!.act({ type: "advance" })
      return view.flow === "playing" && view.story === "choice"
    }, "the batch to settle at the first choice", 30_000)
    expect(playing(stage)).toBe(false)

    // The first gesture: a click (pointer press and release, a corner of the stage).
    for (const type of ["pointerdown", "pointerup"]) {
      player.element.dispatchEvent(new PointerEvent(type, { pointerId: 1, pointerType: "mouse", button: 0, clientX: 4, clientY: 4, bubbles: true, cancelable: true }))
    }
    await waitFor(() => playing(stage), "the stage music to play after the click")
  }, 120_000)
})
