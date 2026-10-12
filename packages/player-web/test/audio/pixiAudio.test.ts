import { Effect, Fiber } from "effect"
import { TestClock } from "effect/testing"
import { describe, expect, it } from "@effect/vitest"
import { PixiAudio, AudioDecodeError, type AudioBackend, type AudioPlayback } from "../../src/audio/pixiAudio.js"

interface TestPlayback {
  readonly value: AudioPlayback
  readonly volumes: Array<number>
  readonly stopCount: () => number
}

const playback = (loop: boolean): TestPlayback & { readonly end: () => void } => {
  const volumes: Array<number> = []
  let stopped = 0
  let endPlayback: (() => void) | undefined
  const ended = loop
    ? Effect.never
    : Effect.callback<void>((resume) => {
      endPlayback = () => resume(Effect.void)
    })
  return {
    value: {
      setVolume: (volume) => volumes.push(volume),
      stop: () => { stopped += 1 },
      ended
    },
    volumes,
    stopCount: () => stopped,
    end: () => endPlayback?.()
  }
}

describe("PixiAudio", () => {
  it.effect("queues playback until unlock and composes volumes", () =>
    Effect.gen(function* () {
      const created: Array<TestPlayback & { readonly end: () => void }> = []
      const backend: AudioBackend = {
        load: () => Effect.void,
        play: (_file, options) => {
          const item = playback(options.loop)
          created.push(item)
          return Effect.succeed(item.value)
        },
        unlock: Effect.void
      }
      const audio = new PixiAudio({ backend, initialVolumes: { master: 0.5, music: 0.8, sfx: 1, voice: 1 } })
      const pending = Effect.runFork(audio.play("music", { file: "theme.wav", loop: true, volume: 0.5, fade: 0, restart: false }))
      yield* Effect.yieldNow
      expect(created).toHaveLength(0)
      yield* audio.unlock
      yield* Effect.yieldNow
      expect(created).toHaveLength(1)
      expect(created[0]?.volumes.at(-1)).toBe(0.2)
      yield* Fiber.interrupt(pending)
    }))

  it.effect("stops voice before replacing it", () =>
    Effect.gen(function* () {
      const created: Array<TestPlayback & { readonly end: () => void }> = []
      const backend: AudioBackend = {
        load: () => Effect.void,
        play: (_file, options) => {
          const item = playback(options.loop)
          created.push(item)
          return Effect.succeed(item.value)
        },
        unlock: Effect.void
      }
      const audio = new PixiAudio({ backend })
      yield* audio.unlock
      const first = Effect.runFork(audio.play("voice", { file: "one.wav", loop: true, volume: 1, fade: 0, restart: false }))
      yield* Effect.yieldNow
      yield* audio.play("voice", { file: "two.wav", loop: true, volume: 1, fade: 0, restart: false })
      expect(created[0]?.stopCount()).toBe(1)
      yield* Fiber.interrupt(first)
    }))

  it.effect("drives fades from TestClock", () =>
    Effect.gen(function* () {
      const item = playback(true)
      const backend: AudioBackend = {
        load: () => Effect.void,
        play: () => Effect.succeed(item.value),
        unlock: Effect.void
      }
      const audio = new PixiAudio({ backend })
      yield* audio.unlock
      yield* audio.play("music", { file: "theme.wav", loop: true, volume: 1, fade: 1, restart: false })
      yield* TestClock.adjust(0.5)
      expect(item.volumes.some((volume) => volume > 0 && volume < 1)).toBe(true)
    }))

  it.effect("keeps music looped and crossfades the previous track", () =>
    Effect.gen(function* () {
      const created: Array<TestPlayback & { readonly end: () => void }> = []
      const backend: AudioBackend = {
        load: () => Effect.void,
        play: (_file, options) => {
          const item = playback(options.loop)
          created.push(item)
          return Effect.succeed(item.value)
        },
        unlock: Effect.void
      }
      const audio = new PixiAudio({ backend })
      yield* audio.unlock
      yield* audio.play("music", { file: "one.wav", loop: true, volume: 1, fade: 0, restart: false })
      yield* audio.play("music", { file: "two.wav", loop: true, volume: 1, fade: 0.1, restart: false })
      expect(created).toHaveLength(2)
      expect(created[1]?.volumes[0]).toBe(0)
    }))

  it.effect("allows polyphonic sfx and waits for completion", () =>
    Effect.gen(function* () {
      const created: Array<TestPlayback & { readonly end: () => void }> = []
      const backend: AudioBackend = {
        load: () => Effect.void,
        play: (_file, options) => {
          const item = playback(options.loop)
          created.push(item)
          return Effect.succeed(item.value)
        },
        unlock: Effect.void
      }
      const audio = new PixiAudio({ backend })
      yield* audio.unlock
      const first = Effect.runFork(audio.play("sfx", { file: "one.wav", loop: false, volume: 1, fade: 0, restart: false }))
      const second = Effect.runFork(audio.play("sfx", { file: "two.wav", loop: false, volume: 1, fade: 0, restart: false }))
      yield* Effect.yieldNow
      expect(created).toHaveLength(2)
      created[0]?.end()
      created[1]?.end()
      yield* Fiber.interrupt(first)
      yield* Fiber.interrupt(second)
    }))

  it.effect("stops a channel with a fade and handles missing files", () =>
    Effect.gen(function* () {
      const item = playback(true)
      const backend: AudioBackend = {
        load: (file) => file === "missing.wav" ? Effect.fail(new AudioDecodeError(file, "404")) : Effect.void,
        play: () => Effect.succeed(item.value),
        unlock: Effect.void
      }
      const audio = new PixiAudio({ backend })
      yield* audio.unlock
      yield* audio.play("music", { file: "ok.wav", loop: true, volume: 1, fade: 0, restart: false })
      yield* audio.stop("music", 0)
      expect(item.stopCount()).toBe(1)
      yield* audio.play("music", { file: "missing.wav", loop: true, volume: 1, fade: 0, restart: false })
    }))

  it.effect("crossfades music at the start, midpoint, and end", () =>
    Effect.gen(function* () {
      const item = playback(true)
      const backend: AudioBackend = {
        load: () => Effect.void,
        play: () => Effect.succeed(item.value),
        unlock: Effect.void
      }
      const audio = new PixiAudio({ backend })
      yield* audio.unlock
      yield* audio.play("music", { file: "fade.wav", loop: true, volume: 1, fade: 1, restart: false })
      expect(item.volumes.at(-1)).toBe(0)
      yield* TestClock.adjust(0.5)
      expect(item.volumes.at(-1)).toBeGreaterThan(0)
      expect(item.volumes.at(-1)).toBeLessThan(1)
      yield* TestClock.adjust(0.5)
      expect(item.volumes.at(-1)).toBeGreaterThan(0.9)
    }))

  it.effect("composes setVolumes with an active fade", () =>
    Effect.gen(function* () {
      const item = playback(true)
      const audio = new PixiAudio({
        backend: { load: () => Effect.void, play: () => Effect.succeed(item.value), unlock: Effect.void },
        initialVolumes: { master: 1, music: 1, sfx: 1, voice: 1 }
      })
      yield* audio.unlock
      yield* audio.play("music", { file: "volume.wav", loop: true, volume: 0.8, fade: 1, restart: false })
      yield* TestClock.adjust(0.5)
      yield* audio.setVolumes({ master: 0.5, music: 0.5, sfx: 1, voice: 1 })
      // Mid fade-in (factor ≈ 0.5): 0.8 × 0.25 × 0.5, not the full 0.2.
      expect(item.volumes.at(-1)).toBeCloseTo(0.1, 1)
      expect(item.volumes.at(-1)).toBeLessThan(0.2)
      yield* TestClock.adjust(0.6)
      expect(item.volumes.at(-1)).toBe(0.2)
    }))

  it.effect("fades a channel to silence before stopping", () =>
    Effect.gen(function* () {
      const item = playback(true)
      const audio = new PixiAudio({ backend: { load: () => Effect.void, play: () => Effect.succeed(item.value), unlock: Effect.void } })
      yield* audio.unlock
      yield* audio.play("sfx", { file: "stop.wav", loop: true, volume: 1, fade: 0, restart: false })
      const stopping = Effect.runFork(audio.stop("sfx", 1))
      yield* TestClock.adjust(0.5)
      expect(item.volumes.at(-1)).toBeGreaterThan(0)
      yield* TestClock.adjust(0.5)
      yield* Fiber.join(stopping)
      expect(item.volumes.at(-1)).toBe(0)
      expect(item.stopCount()).toBe(1)
    }))

  const recordingBackend = () => {
    const created: Array<TestPlayback & { readonly end: () => void }> = []
    const loads: Array<string> = []
    const backend: AudioBackend = {
      load: (file) => Effect.sync(() => { loads.push(file) }),
      play: (_file, options) => {
        const item = playback(options.loop)
        created.push(item)
        return Effect.succeed(item.value)
      },
      unlock: Effect.void
    }
    return { backend, created, loads }
  }

  it.effect("only changes the volume of the music file already playing without restart", () =>
    Effect.gen(function* () {
      const { backend, created, loads } = recordingBackend()
      const audio = new PixiAudio({ backend })
      yield* audio.unlock
      yield* audio.play("music", { file: "one.wav", loop: true, volume: 1, fade: 0, restart: true })
      yield* audio.play("music", { file: "one.wav", loop: true, volume: 0.4, fade: 0, restart: false })
      expect(created).toHaveLength(1)
      expect(loads).toEqual(["one.wav"])
      expect(created[0]?.stopCount()).toBe(0)
      expect(created[0]?.volumes.at(-1)).toBe(0.4)
      yield* audio.setVolumes({ master: 0.5, music: 1, sfx: 1, voice: 1 })
      expect(created[0]?.volumes.at(-1)).toBe(0.2)
    }))

  it.effect("fades a same-file volume change over fade without restarting", () =>
    Effect.gen(function* () {
      const { backend, created } = recordingBackend()
      const audio = new PixiAudio({ backend })
      yield* audio.unlock
      yield* audio.play("music", { file: "one.wav", loop: true, volume: 1, fade: 0, restart: false })
      yield* audio.play("music", { file: "one.wav", loop: true, volume: 0.5, fade: 1, restart: false })
      expect(created).toHaveLength(1)
      yield* TestClock.adjust(0.5)
      expect(created[0]?.volumes.at(-1)).toBeCloseTo(0.75, 5)
      yield* TestClock.adjust(0.6)
      expect(created[0]?.volumes.at(-1)).toBe(0.5)
      expect(created[0]?.stopCount()).toBe(0)
    }))

  it.effect("restarts the music file already playing when restart is set", () =>
    Effect.gen(function* () {
      const { backend, created } = recordingBackend()
      const audio = new PixiAudio({ backend })
      yield* audio.unlock
      yield* audio.play("music", { file: "one.wav", loop: true, volume: 1, fade: 0, restart: false })
      yield* audio.play("music", { file: "one.wav", loop: true, volume: 1, fade: 0, restart: true })
      expect(created).toHaveLength(2)
      expect(created[0]?.stopCount()).toBe(1)
      expect(created[1]?.stopCount()).toBe(0)
    }))

  it.effect("plays a stopped music file again without restart", () =>
    Effect.gen(function* () {
      const { backend, created } = recordingBackend()
      const audio = new PixiAudio({ backend })
      yield* audio.unlock
      yield* audio.play("music", { file: "one.wav", loop: true, volume: 1, fade: 0, restart: false })
      yield* audio.stop("music", 0)
      yield* audio.play("music", { file: "one.wav", loop: true, volume: 1, fade: 0, restart: false })
      expect(created).toHaveLength(2)
    }))

  it.effect("plays the incoming file when the same file is fading out", () =>
    Effect.gen(function* () {
      const { backend, created } = recordingBackend()
      const audio = new PixiAudio({ backend })
      yield* audio.unlock
      yield* audio.play("music", { file: "one.wav", loop: true, volume: 1, fade: 0, restart: false })
      yield* audio.play("music", { file: "two.wav", loop: true, volume: 1, fade: 1, restart: false })
      yield* audio.play("music", { file: "one.wav", loop: true, volume: 1, fade: 1, restart: false })
      expect(created).toHaveLength(3)
    }))

  it.effect("fades a track replaced mid volume-change from its live level, never jumping up", () =>
    Effect.gen(function* () {
      const { backend, created } = recordingBackend()
      const audio = new PixiAudio({ backend })
      yield* audio.unlock
      yield* audio.play("music", { file: "one.wav", loop: true, volume: 1, fade: 0, restart: false })
      yield* audio.play("music", { file: "one.wav", loop: true, volume: 0.5, fade: 2, restart: false })
      yield* TestClock.adjust(1)
      const old = created[0]!
      const midpoint = old.volumes.at(-1)!
      expect(midpoint).toBeGreaterThan(0.5)
      expect(midpoint).toBeLessThan(1)
      const replacedAt = old.volumes.length
      yield* audio.play("music", { file: "two.wav", loop: true, volume: 1, fade: 1, restart: false })
      yield* TestClock.adjust(1.1)
      const after = old.volumes.slice(replacedAt)
      expect(after.length).toBeGreaterThan(0)
      expect(Math.max(...after)).toBeLessThanOrEqual(midpoint)
      expect(after.at(-1)).toBe(0)
      expect(old.stopCount()).toBe(1)
    }))

  it.effect("fades a track replaced mid fade-in from its live level, never jumping up", () =>
    Effect.gen(function* () {
      const { backend, created } = recordingBackend()
      const audio = new PixiAudio({ backend })
      yield* audio.unlock
      yield* audio.play("music", { file: "one.wav", loop: true, volume: 1, fade: 2, restart: false })
      yield* TestClock.adjust(1)
      const old = created[0]!
      const midpoint = old.volumes.at(-1)!
      expect(midpoint).toBeGreaterThan(0)
      expect(midpoint).toBeLessThan(1)
      const replacedAt = old.volumes.length
      yield* audio.play("music", { file: "two.wav", loop: true, volume: 1, fade: 1, restart: false })
      yield* TestClock.adjust(1.1)
      const after = old.volumes.slice(replacedAt)
      expect(after.length).toBeGreaterThan(0)
      expect(Math.max(...after)).toBeLessThanOrEqual(midpoint)
      expect(after.at(-1)).toBe(0)
      expect(old.stopCount()).toBe(1)
    }))
})