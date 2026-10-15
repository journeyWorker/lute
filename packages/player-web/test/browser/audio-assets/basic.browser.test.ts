import { Effect, Exit, Scope } from "effect"
import { Assets } from "pixi.js"
import { describe, expect, it, vi } from "vitest"
import { PixiAssets } from "../../../src/assets/pixiAssets.js"
import { pixiAssetsBackend } from "../../../src/assets/pixiAssetsBackend.js"
import { pixiSoundBackend } from "../../../src/audio/pixiSoundBackend.js"
import { PixiAudio } from "../../../src/audio/pixiAudio.js"

const png = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk+A8AAQUBAScY42YAAAAASUVORK5CYII="

const wavBlob = (): Blob => {
  const bytes = new ArrayBuffer(44 + 160)
  const view = new DataView(bytes)
  const write = (offset: number, text: string): void => {
    for (let index = 0; index < text.length; index += 1) view.setUint8(offset + index, text.charCodeAt(index))
  }
  write(0, "RIFF")
  view.setUint32(4, 196, true)
  write(8, "WAVEfmt ")
  view.setUint32(16, 16, true)
  view.setUint16(20, 1, true)
  view.setUint16(22, 1, true)
  view.setUint32(24, 8000, true)
  view.setUint32(28, 8000, true)
  view.setUint16(32, 1, true)
  view.setUint16(34, 8, true)
  write(36, "data")
  view.setUint32(40, 160, true)
  return new Blob([bytes], { type: "audio/wav" })
}

describe("browser audio and assets", () => {
  it("loads and unloads a generated PNG and WAV", async () => {
    const wavUrl = `${URL.createObjectURL(wavBlob())}#tone.wav`
    const assets = new PixiAssets({ backend: pixiAssetsBackend, graceSeconds: 0 })
    const scope = await Effect.runPromise(Scope.make())
    await Effect.runPromise(Scope.provide(scope)(assets.ensure([png, wavUrl])))
    expect(assets.texture(png)).toBeDefined()
    await Effect.runPromise(Scope.close(scope, Exit.void))
    URL.revokeObjectURL(wavUrl.split("#")[0] ?? wavUrl)
  })
  it("leaves the host Pixi Assets manager available for its own base path", async () => {
    const warning = vi.spyOn(console, "warn").mockImplementation(() => undefined)
    await Assets.init({ basePath: "forge/" })
    expect(Assets.resolver.basePath).toBe("forge/")
    expect(warning).not.toHaveBeenCalledWith(expect.stringContaining("already initialized"))
    warning.mockRestore()
  })

  it("plays a generated WAV through PixiAudio", async () => {
    const wavUrl = `${URL.createObjectURL(wavBlob())}#tone.wav`
    const audio = new PixiAudio({ backend: pixiSoundBackend })
    await Effect.runPromise(audio.unlock)
    await Effect.runPromise(audio.play("sfx", { file: wavUrl, loop: false, volume: 0.1, fade: 0, restart: true }))
    URL.revokeObjectURL(wavUrl.split("#")[0] ?? wavUrl)
  })
})
