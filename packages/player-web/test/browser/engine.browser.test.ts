import { describe, expect, it } from "vitest"
import { Effect, Layer } from "effect"
import { Container, Graphics } from "pixi.js"
import { PixiApp } from "../../src/engine/app.js"
import { World } from "../../src/engine/world.js"
import { fade, wipe } from "../../src/engine/behaviours.js"

describe("Pixi engine browser", () => {
  it("initializes WebGL2, builds layers in order, and releases its canvas", async () => {
    const element = document.createElement("div")
    element.style.width = "960px"
    element.style.height = "540px"
    document.body.appendChild(element)
    const layer = PixiApp.layer(element, { width: 1920, height: 1080 }, 0x112233)
    await Effect.runPromise(Effect.scoped(Effect.gen(function* () {
      const pixi = yield* PixiApp
      expect(element.querySelector("canvas")).not.toBeNull()
      expect(pixi.app.canvas.getContext("webgl2")).not.toBeNull()
      expect(pixi.app.stage.children).toHaveLength(1)
      const root = pixi.app.stage.children[0]
      if (root === undefined) return
      expect(pixi.layers.camera.children[0]).toBe(pixi.layers.actors)
      expect(root.scale.x).toBeCloseTo(0.5)
      pixi.app.renderer.resize(1600, 900)
      expect(root.scale.x).toBeCloseTo(0.833333)
    }).pipe(Effect.provide(layer))))
    expect(element.querySelector("canvas")).toBeNull()
    element.remove()
  })

  it("drives a fade from the real ticker, applies wipe masks, and renders a solid frame", async () => {
    const element = document.createElement("div")
    element.style.width = "320px"
    element.style.height = "180px"
    document.body.appendChild(element)
    const appLayer = PixiApp.layer(element, { width: 320, height: 180 }, 0x000000)
    const engineLayer = World.layer.pipe(Layer.provideMerge(appLayer))
    await Effect.runPromise(Effect.scoped(Effect.gen(function* () {
      const pixi = yield* PixiApp
      const world = yield* World
      const actor = new Container()
      actor.alpha = 0
      pixi.layers.actors.addChild(actor)
      yield* Effect.forkChild(world.run(actor, fade(actor, 1, 0.02)))
      yield* Effect.sleep("50 millis")
      expect(actor.alpha).toBeCloseTo(1)
      const square = new Graphics().rect(0, 0, 320, 180).fill(0xff0000)
      pixi.layers.background.addChild(square)
      const maskBehaviour = wipe(square, "left", 0.01, true)
      maskBehaviour.finish()
      expect(square.mask).toBeTruthy()
      maskBehaviour.dispose()
      pixi.app.render()
      const gl = pixi.app.canvas.getContext("webgl2")
      expect(gl).not.toBeNull()
      if (gl !== null) {
        const pixel = new Uint8Array(4)
        gl.readPixels(pixi.app.canvas.width / 2, pixi.app.canvas.height / 2, 1, 1, gl.RGBA, gl.UNSIGNED_BYTE, pixel)
        expect(pixel[0]).toBeGreaterThan(200)
        expect(pixel[1]).toBeLessThan(30)
        expect(pixel[2]).toBeLessThan(30)
      }
      expect(pixi.app.canvas.toDataURL("image/png")).toMatch(/^data:image\/png;base64/)
    }).pipe(Effect.provide(engineLayer))))
    expect(element.querySelector("canvas")).toBeNull()
    element.remove()
  })
})
