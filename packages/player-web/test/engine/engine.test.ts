import { describe, expect, it } from "vitest"
import { Effect, Fiber, Layer, Queue, Stream } from "effect"
import { Application, Container, Graphics } from "pixi.js"
import type { Layers } from "../../src/engine/types.js"
import { backOut, cubicIn, cubicInOut, cubicOut, elasticOut, linear, quadIn, quadInOut, quadOut, sineInOut } from "../../src/engine/easing.js"
import { crossfade, delay, fade, flash, hop, pulse, shake, slide, tween, wipe } from "../../src/engine/behaviours.js"
import { PixiApp } from "../../src/engine/app.js"
import { World } from "../../src/engine/world.js"
describe("easing", () => {
  it("has stable endpoints and expected midpoint values", () => {
    expect(linear(0.5)).toBe(0.5)
    expect(quadIn(0.5)).toBe(0.25)
    expect(quadOut(0.5)).toBe(0.75)
    expect(quadInOut(0.5)).toBe(0.5)
    expect(cubicIn(0.5)).toBe(0.125)
    expect(cubicOut(0.5)).toBe(0.875)
    expect(cubicInOut(0.5)).toBe(0.5)
    expect(sineInOut(0)).toBe(0)
    expect(sineInOut(1)).toBe(1)
    expect(backOut(0)).toBe(0)
    expect(backOut(1)).toBe(1)
    expect(elasticOut(0)).toBe(0)
    expect(elasticOut(1)).toBe(1)
  })
})

describe("pooled behaviours", () => {
  it("interpolates and snaps every behaviour to its end state", () => {
    const target = new Container()
    target.x = 10
    target.y = 20
    target.alpha = 0.25
    target.scale.set(2)
    target.rotation = 0.2
    const property = tween(target, { x: 10, y: 20, alpha: 0.25, scaleX: 2, scaleY: 2, rotation: 0.2 }, {
      x: 30, y: 40, alpha: 1, scaleX: 3, scaleY: 4, rotation: 1
    }, 1)
    expect(property.update(0.5)).toBe("running")
    expect(target.x).toBe(20)
    expect(target.alpha).toBeCloseTo(0.625)
    property.finish()
    expect(target.x).toBe(30)
    expect(target.scale.y).toBe(4)
    property.dispose()
    const reused = tween(target, { x: 30 }, { x: 31 }, 1)
    expect(reused).toBe(property)
    reused.dispose()

    const alpha = fade(target, 0.2, 1)
    alpha.finish()
    expect(target.alpha).toBeCloseTo(0.2)
    alpha.dispose()
    const from = new Container()
    const to = new Container()
    from.alpha = 1
    to.alpha = 0
    const blend = crossfade(from, to, 1)
    expect(blend.update(0.5)).toBe("running")
    expect(from.alpha).toBe(0.5)
    expect(to.alpha).toBe(0.5)
    blend.finish()
    expect(from.alpha).toBe(0)
    expect(to.alpha).toBe(1)
    blend.dispose()

    const shaker = shake(target, 10, 1, 2)
    expect(shaker.update(0.25)).toBe("running")
    shaker.finish()
    expect(target.x).toBe(30)
    shaker.dispose()
    const hopper = hop(target, 20, 1)
    hopper.update(0.5)
    expect(target.y).toBeLessThan(40)
    hopper.finish()
    expect(target.y).toBe(40)
    hopper.dispose()

    const slider = slide(target, -100, 50, 1)
    expect(target.x).toBe(-100)
    slider.finish()
    expect(target.x).toBe(50)
    slider.dispose()

    const shape = new Graphics().rect(0, 0, 100, 100).fill(0xffffff)
    target.addChild(shape)
    const wiper = wipe(target, "right", 1, true)
    expect(target.mask).not.toBeNull()
    wiper.update(0.5)
    wiper.finish()
    wiper.dispose()
    expect(target.mask).toBeFalsy()

    const overlay = new Container()
    const flasher = flash(overlay, 0xff00ff, 1)
    expect(overlay.tint).toBe(0xff00ff)
    flasher.finish()
    expect(overlay.alpha).toBe(0)
    flasher.dispose()

    const pulser = pulse(target, 1.2, 1)
    expect(pulser.update(0.25)).toBe("running")
    pulser.finish()
    expect(target.scale.x).toBe(3)
    pulser.dispose()
    const timer = delay(1)
    expect(timer.update(0.5)).toBe("running")
    expect(timer.update(0.5)).toBe("done")
    timer.dispose()
  })
})

describe("world", () => {
  it("steps, finishes and cancels against a fake frame stream", async () => {
    const queue = await Effect.runPromise(Queue.unbounded<number>())
    const layers: Layers = {
      background: new Container(), cg: new Container(), camera: new Container(), actors: new Container(),
      video: new Container(), dialogue: new Container(), choice: new Container(), overlay: new Container(),
      transition: new Container()
    }
    const fake = { app: new Application(), layers, design: { width: 100, height: 100 }, frames: Stream.fromQueue(queue) }
    const target = new Container()
    const program = Effect.provide(
      Effect.scoped(Effect.gen(function* () {
        const world = yield* World
        const behavior = tween(target, { x: 0 }, { x: 10 }, 1)
        const running = yield* Effect.forkChild(world.run(target, behavior))
        yield* Queue.offer(queue, 0.5)
        yield* Effect.yieldNow
        expect(target.x).toBe(5)
        yield* world.finishAll(target)
        yield* Fiber.join(running)
        expect(target.x).toBe(10)
        const canceled = yield* Effect.forkChild(world.run(target, delay(10)))
        yield* world.cancel(target)
        yield* Fiber.join(canceled)
        expect(yield* world.size).toBe(0)
      })),
      World.layer
    )
    await Effect.runPromise(Effect.provideService(program, PixiApp, fake))
  })
})
