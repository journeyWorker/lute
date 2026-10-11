import { Effect, Exit, Scope } from "effect"
import { TestClock } from "effect/testing"
import { describe, expect, it } from "@effect/vitest"
import { Texture } from "pixi.js"
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
  it("builds a complete stable manifest", () => {
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
      "voice/{voiceKey}.ogg",
      "bg/title.png"
    ])
  })

  it.effect("refcounts scoped assets and unloads after grace", () =>
    Effect.gen(function* () {
      const loaded = new Set<string>()
      const unloaded = new Set<string>()
      const assets = new PixiAssets({
        assetsBase: "https://example.test/assets/",
        backend: backend(loaded, unloaded),
        graceSeconds: 1
      })
      const scope = yield* Scope.make()
      yield* Scope.provide(scope)(assets.ensure(["bg/a.png"]))
      expect(assets.texture("bg/a.png")).toBe(Texture.EMPTY)
      yield* Scope.close(scope, Exit.void)
      expect(unloaded).toHaveLength(0)
      yield* TestClock.adjust(1)
      expect(unloaded).toContain("https://example.test/assets/bg/a.png")
    }))
})