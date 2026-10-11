import { Assets, Texture } from "pixi.js"
import { Effect } from "effect"
import { sound } from "@pixi/sound"
import type { AssetBackend } from "./pixiAssets.js"

/** Real Pixi/@pixi-sound asset backend used by the browser player. */
export const pixiAssetsBackend: AssetBackend = {
  loadTexture: (url) => Effect.tryPromise(() => Assets.load<Texture>(url)),
  unloadTexture: (url) => Effect.tryPromise(() => Assets.unload(url)).pipe(Effect.orDie),
  loadAudio: (url) => Effect.callback<void, unknown>((resume) => {
    if (sound.exists(url)) {
      resume(Effect.succeed(undefined))
      return
    }
    try {
      sound.add(url, {
        url,
        preload: true,
        loaded: (error) => resume(error === undefined ? Effect.succeed(undefined) : Effect.fail(error))
      })
    } catch (error) {
      resume(Effect.fail(error))
    }
  }),
  unloadAudio: (url) => Effect.sync(() => {
    if (sound.exists(url, false)) sound.remove(url)
  })
}
