// spec 0.47.1 §4 (#132): a camera transition ends with the actors drawn.
// Over the compiled player-demo: its opening `::camera{framing="wide"
// transition="fade"}`, chapter two's `wipeLeft` and chapter three's
// `dissolve` cues end at alpha 1 with no mask and their framing; a load
// during the fade cue and a skip (instant cue) during it leave the actors drawn.
import { Effect, Fiber, Stream } from "effect"
import { Rectangle, Sprite, type Container } from "pixi.js"
import { afterEach, describe, expect, it } from "vitest"
import { alphaOf, isDrawn, mountProject, playReading, playthroughTimeout, press, shows, stagePixels, waitFor, type RealPlayer } from "../support/real-player.js"

/** Longer than any camera transition (0.6s) and actor fade (0.4s) in the demo. */
const settle = (): Promise<void> => Effect.runPromise(Effect.sleep("1500 millis"))

const sprites = (root: Container): ReadonlyArray<Sprite> => {
  const found: Sprite[] = []
  const walk = (node: Container): void => {
    if (node instanceof Sprite) found.push(node)
    for (const child of node.children) walk(child)
  }
  walk(root)
  return found
}

const differs = (left: Uint8ClampedArray, right: Uint8ClampedArray): number => {
  let count = 0
  for (let index = 0; index < left.length; index += 4) {
    const delta = Math.max(Math.abs(left[index]! - right[index]!), Math.abs(left[index + 1]! - right[index + 1]!), Math.abs(left[index + 2]! - right[index + 2]!))
    if (delta > 2) count += 1
  }
  return count
}

/**
 * The camera layer rests (alpha 1, no mask) and the actors are drawn: an
 * actor sprite is visible and fully opaque, and hiding the camera layer
 * changes the pixels where it stands.
 */
const expectActorsDrawn = (player: RealPlayer): void => {
  const { layers } = player.pixi
  expect(layers.camera.alpha).toBe(1)
  expect(layers.camera.mask ?? null).toBeNull()
  const drawn = sprites(layers.actors).filter((sprite) => isDrawn(sprite) && alphaOf(sprite) === 1 && sprite.texture.width > 1)
  expect(drawn.length).toBeGreaterThan(0)
  const dialogueTop = layers.dialogue.getBounds().minY
  const regions = drawn
    .map((sprite) => sprite.getBounds().rectangle)
    .map((bounds) => new Rectangle(Math.ceil(Math.max(0, bounds.x)), Math.ceil(Math.max(0, bounds.y)), Math.floor(Math.min(bounds.right, 1280) - Math.max(0, bounds.x)), Math.floor(Math.min(bounds.bottom, dialogueTop, 720) - Math.max(0, bounds.y))))
    .filter((region) => region.width >= 8 && region.height >= 8)
  expect(regions.length).toBeGreaterThan(0)
  const changed = regions.map((region) => {
    const shown = stagePixels(player.pixi, region)
    layers.camera.visible = false
    const hidden = stagePixels(player.pixi, region)
    layers.camera.visible = true
    return differs(shown, hidden)
  })
  expect(Math.max(...changed)).toBeGreaterThan(0)
}

/**
 * New game from the title (text speed instant, skip unread lines too) and
 * the first idle entry (chapter one), returning once its opening fade cue
 * has started. Audio is unlocked first, at the idle menu, with a device
 * input (a wheel up, which opens the backlog, closed again; the idle screen
 * takes keys): sounds, and the batches that wait for them, wait for the
 * unlock (a load does not: it begins the music without awaiting it).
 */
const startDuringOpeningFade = async (player: RealPlayer): Promise<void> => {
  await waitFor(async () => (await player.view()).flow === "title", "the title")
  await player.act({ type: "setSettings", settings: { textSpeed: 0, skipMode: "all" } })
  await player.act({ type: "newGame" })
  await waitFor(async () => (await player.view()).story === "idle", "the idle menu")
  player.element.dispatchEvent(new WheelEvent("wheel", { deltaY: -100, bubbles: true, cancelable: true }))
  await waitFor(async () => (await player.view()).overlay === "backlog", "the backlog a wheel up opens")
  await player.act({ type: "close" })
  await waitFor(async () => (await player.view()).overlay === "none", "the backlog to close")
  await player.act({ type: "pickIdle", index: 0 })
  await waitFor(() => player.pixi.layers.camera.alpha < 1, "the opening fade cue to start")
}

describe("camera transitions over the compiled demo", () => {
  let player: RealPlayer | undefined
  afterEach(async () => {
    await player?.close()
    player = undefined
  })

  it("draws the actors after the opening fade cue", async () => {
    player = await mountProject("demo")
    await playReading(player, (reading) => reading.lines.length > 0)
    await settle()
    expectActorsDrawn(player)
    expect(player.pixi.layers.camera.scale.x).toBe(1)
  }, playthroughTimeout())

  it("ends chapter two's wipeLeft and chapter three's dissolve cues visible, framed", async () => {
    player = await mountProject("demo")
    const reading = await playReading(player, (read) => read.lines.some((line) => shows(line, "The lighthouse door opens onto a stair")))
    await settle()
    expectActorsDrawn(player)
    // `close` framing (zoom 1.3) on mara.
    expect(player.pixi.layers.camera.scale.x).toBeCloseTo(1.3)
    const mark = reading.lines.length
    await playReading(player, (read) => read.lines.slice(mark).some((line) => shows(line, "At midnight, the lantern opens like an eye")), [], reading)
    await settle()
    expectActorsDrawn(player)
    // `wide` framing (zoom 1).
    expect(player.pixi.layers.camera.scale.x).toBeCloseTo(1)
  }, playthroughTimeout(2))

  it("leaves the actors drawn after a load during the fade cue", async () => {
    player = await mountProject("demo")
    await startDuringOpeningFade(player)
    // A host load (spec 0.41.0 §7.4) of the save point the cue runs at: a
    // reader's quick save and confirmed quick load outlast the 0.6s cue here.
    const save = await Effect.runPromise(player.handle.snapshot)
    expect(player.pixi.layers.camera.alpha).toBeLessThan(1)
    await Effect.runPromise(player.handle.restore(save))
    await settle()
    expectActorsDrawn(player)
  }, 180_000)

  it("applies the fade cue at once when skipping starts during it", async () => {
    player = await mountProject("demo")
    await startDuringOpeningFade(player)
    await player.act({ type: "toggleSkip" })
    await waitFor(() => player!.pixi.layers.camera.alpha === 1 && (player!.pixi.layers.camera.mask ?? null) === null, "the skipped cue to rest", 250)
    expect((await player.view()).mode).toBe("skip")
    await player.act({ type: "toggleSkip" })
    await settle()
    expectActorsDrawn(player)
  }, 180_000)
})

interface CameraRecording {
  /** Seconds on the player's clock since recording began: the sum of `pixi.frames`, what its behaviours advance by (at most 0.1s a frame, so a slow machine's clock runs behind wall time). */
  readonly clock: () => number
  /** Stop recording; the camera layer's alpha at every frame. */
  readonly stop: () => Promise<ReadonlyArray<number>>
}

/** Record the camera layer every frame on the player's own clock. */
const recordCamera = (player: RealPlayer): CameraRecording => {
  const alphas: number[] = []
  const { camera } = player.pixi.layers
  let clock = 0
  const fiber = Effect.runFork(Stream.runForEach(player.pixi.frames, (seconds) => Effect.sync(() => {
    clock += seconds
    alphas.push(camera.alpha)
  })))
  return {
    clock: () => clock,
    stop: () => Effect.runPromise(Fiber.interrupt(fiber)).then(() => alphas)
  }
}

/** Poll `probe` until it holds; fail once the recording's clock is past `by` seconds without it. */
const untilClock = async (recording: CameraRecording, probe: () => boolean, by: number, what: string): Promise<void> => {
  for (;;) {
    const now = recording.clock()
    if (probe()) return
    if (now > by) throw new Error(`timed out waiting for ${what}: ${now.toFixed(2)}s on the player's clock, due by ${by.toFixed(2)}s`)
    await Effect.runPromise(Effect.sleep("20 millis"))
  }
}

/** The fixture's `::camera{framing="close" transition="fade" duration="8"}`, in seconds. */
const cueSeconds = 8
/** A few frames (at most 0.1s each) between the player's clock and what the probes read. */
const frameSlack = 0.5

// The compiled `camera-stage` fixture: an 8s fade camera cue is the last cue
// of its batch (a choice follows; the batch settles, autosaving at the
// choice, while the cue still runs: long enough for a slow machine to settle
// before its midpoint), then a fade cue is followed at once by a `move` cue
// with another framing.
describe("camera transitions over the compiled camera-stage fixture", () => {
  let player: RealPlayer | undefined
  afterEach(async () => {
    await player?.close()
    player = undefined
  })

  it("plays a transition that ends its batch: hidden at its midpoint, then at rest", async () => {
    player = await mountProject("camera-stage")
    await waitFor(async () => (await player!.view()).flow === "title", "the title")
    const recording = recordCamera(player)
    await player.act({ type: "setSettings", settings: { textSpeed: 0 } })
    await player.act({ type: "newGame" })
    await waitFor(async () => (await player!.view()).story === "choice", "the choice after the camera cue")
    // The batch settled at the choice before the cue's midpoint …
    expect(player.pixi.layers.camera.alpha).toBeGreaterThan(0.1)
    // … and the transition played on: hidden at its midpoint, then at rest,
    // each within the cue's own timing on the player's clock (the cue began
    // before the choice), never by wall time a slow machine stretches.
    const choiceAt = recording.clock()
    await untilClock(recording, () => player!.pixi.layers.camera.alpha < 0.1, choiceAt + cueSeconds / 2 + frameSlack, "the layer to be hidden at the midpoint")
    await untilClock(recording, () => player!.pixi.layers.camera.alpha === 1, choiceAt + cueSeconds + frameSlack, "the transition to end")
    await settle()
    const alphas = await recording.stop()
    expect(Math.min(...alphas)).toBeLessThan(0.1)
    expectActorsDrawn(player)
    // `close` framing (zoom 1.3).
    expect(player.pixi.layers.camera.scale.x).toBeCloseTo(1.3)
  }, 120_000)

  it("lets a move cue supersede a running transition: the move's framing stays", async () => {
    player = await mountProject("camera-stage")
    await playReading(player, (read) => read.lines.some((line) => shows(line, "The move came after the transition")))
    await settle()
    expectActorsDrawn(player)
    // The move's `close` framing (zoom 1.3), not the superseded transition's `wide` (1).
    expect(player.pixi.layers.camera.scale.x).toBeCloseTo(1.3)
  }, playthroughTimeout())
})
