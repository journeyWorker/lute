import { Effect, Exit, Fiber, Scope, Stream } from "effect"
import { TestClock } from "effect/testing"
import { describe, expect, it } from "@effect/vitest"
import { Texture } from "pixi.js"
import { AssetMissing } from "@lute-lang/player"
import { PixiAssets, assetsFromBindings, type AssetBackend } from "../../src/assets/pixiAssets.js"

const bindings = {
  assets: { theme: "audio/theme.ogg" },
  backgrounds: { foyer: { image: "bg/foyer.png", times: { night: "bg/foyer-night.png" } } },
  characters: {
    guide: {
      sprite: { default: "char/guide.png", emotions: { happy: "char/guide-happy.png" } },
      costumes: { coat: { default: "char/guide-coat.png" } }
    }
  },
  music: { moods: { calm: "audio/calm.ogg" } },
  sfx: { door: "audio/door.wav" },
  sequences: { opening: { video: "video/opening.mp4" } },
  voice: { pattern: "voice/{voiceKey}.ogg" },
  ui: { title: { background: "bg/title.png", music: "theme" } }
}

const backend = (loaded: Set<string>, unloaded: Set<string>): AssetBackend => ({
  loadTexture: (url) => Effect.sync(() => {
    loaded.add(url)
    return Texture.EMPTY
  }),
  unloadTexture: (url) => Effect.sync(() => { unloaded.add(url) }),
  loadAudio: (url) => Effect.sync(() => { loaded.add(url) }),
  unloadAudio: (url) => Effect.sync(() => { unloaded.add(url) })
})

describe("PixiAssets", () => {
  it("builds the manifest for bindings without expanding voice patterns", () => {
    expect(assetsFromBindings(bindings)).toEqual([
      "audio/theme.ogg",
      "bg/foyer.png",
      "bg/foyer-night.png",
      "char/guide.png",
      "char/guide-happy.png",
      "char/guide-coat.png",
      "audio/calm.ogg",
      "audio/door.wav",
      "video/opening.mp4",
      "bg/title.png"
    ])
  })

  it.effect("refcounts ensure and release, then unloads after the grace period", () =>
    Effect.gen(function* () {
      const loaded = new Set<string>()
      const unloaded = new Set<string>()
      const assets = new PixiAssets({
        assetsBase: "https://example.test/assets/",
        backend: backend(loaded, unloaded),
        graceSeconds: 1
      })
      const first = yield* Scope.make()
      const second = yield* Scope.make()
      yield* Scope.provide(first)(assets.ensure(["bg/a.png"]))
      yield* Scope.provide(second)(assets.ensure(["bg/a.png"]))
      yield* Scope.close(first, Exit.void)
      expect(unloaded).toHaveLength(0)
      yield* Scope.close(second, Exit.void)
      expect(unloaded).toHaveLength(0)
      yield* TestClock.adjust(1)
      expect(unloaded).toContain("https://example.test/assets/bg/a.png")
    }))

  it.effect("reuses an asset released during the grace period", () =>
    Effect.gen(function* () {
      const loaded = new Set<string>()
      const unloaded = new Set<string>()
      const assets = new PixiAssets({ assetsBase: "https://example.test/assets/", backend: backend(loaded, unloaded), graceSeconds: 1 })
      const first = yield* Scope.make()
      yield* Scope.provide(first)(assets.ensure(["bg/reuse.png"]))
      yield* Scope.close(first, Exit.void)
      const second = yield* Scope.make()
      yield* Scope.provide(second)(assets.ensure(["bg/reuse.png"]))
      yield* TestClock.adjust(1)
      expect(unloaded).toHaveLength(0)
      expect(loaded).toHaveLength(1)
      yield* Scope.close(second, Exit.void)
    }))
  it.effect("reports zero, intermediate, and complete progress values", () =>
    Effect.gen(function* () {
      const values: Array<number> = []
      const loaded = new Set<string>()
      const assets = new PixiAssets({ backend: backend(loaded, new Set()), graceSeconds: 0, concurrency: 1 })
      const scope = yield* Scope.make()
      const observer = yield* Effect.forkDetach(Stream.runForEach((value: number) => Effect.sync(() => { values.push(value) }))(assets.progress))
      yield* Effect.yieldNow
      yield* Scope.provide(scope)(assets.ensure(["a.png", "b.png"]))
      yield* Fiber.interrupt(observer)
      expect(values[0]).toBe(0)
      expect(values).toContain(0.5)
      expect(values.at(-1)).toBe(1)
      yield* Scope.close(scope, Exit.void)
    }))

  it.effect("fails with AssetMissing and the requested file on a 404", () =>
    Effect.gen(function* () {
      const missingBackend: AssetBackend = {
        loadTexture: () => Effect.fail(new Error("404")),
        unloadTexture: () => Effect.void,
        loadAudio: () => Effect.void,
        unloadAudio: () => Effect.void
      }
      const assets = new PixiAssets({ backend: missingBackend })
      const error = yield* Effect.flip(Effect.scoped(assets.ensure(["missing.png"])))
      expect(error).toBeInstanceOf(AssetMissing)
      expect(error.file).toBe("missing.png")
    }))
})
