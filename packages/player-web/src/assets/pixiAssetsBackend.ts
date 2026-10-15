import {
  AssetsClass,
  detectAvif,
  detectDefaults,
  detectMp4,
  detectOgv,
  detectWebm,
  detectWebp,
  loadJson,
  loadSvg,
  loadTextures,
  loadTxt,
  loadVideoTextures,
  resolveJsonUrl,
  resolveTextureUrl,
  Texture,
  type LoaderParser
} from "pixi.js"
import { Effect } from "effect"
import { sound } from "@pixi/sound"
import type { AssetBackend } from "./pixiAssets.js"

/** Create a private Pixi asset manager with the built-in image parsers attached. */
const createAssetManager = (): AssetsClass => {
  const assets = new AssetsClass()
  assets.loader.parsers.push(loadJson, loadTxt, loadSvg, loadTextures, loadVideoTextures as unknown as LoaderParser)
  assets.resolver.parsers.push(resolveJsonUrl, resolveTextureUrl)
  assets.detections.push(detectDefaults, detectAvif, detectWebp, detectMp4, detectOgv, detectWebm)
  return assets
}

/** Real Pixi/@pixi-sound asset backend used by the browser player. */
export const createPixiAssetsBackend = (): AssetBackend => {
  const assets = createAssetManager()
  return {
    loadTexture: (url) => Effect.tryPromise(() => assets.load<Texture>(url)),
    // Scene sprites retain textures after a cue scope ends; this private manager
    // outlives those display objects for the duration of the player.
    unloadTexture: (_url, _texture) => Effect.void,
    loadAudio: (url) => Effect.callback<void, unknown>((resume) => {
      if (sound.exists(url)) {
        resume(Effect.succeed(undefined))
        return
      }
      try {
        sound.add(url, {
          url,
          preload: true,
          loaded: (error) => resume(error == null ? Effect.succeed(undefined) : Effect.fail(error))
        })
      } catch (error) {
        resume(Effect.fail(error))
      }
    }),
    unloadAudio: (url) => Effect.sync(() => {
      if (sound.exists(url, false)) sound.remove(url)
    })
  }
}

/** Standalone backend for tests and hosts that need one loader instance. */
export const pixiAssetsBackend: AssetBackend = createPixiAssetsBackend()
