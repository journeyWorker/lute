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

describe("named easing functions", () => {
  it("linear endpoints and midpoint", () => { expect(linear(0)).toBe(0); expect(linear(0.5)).toBe(0.5); expect(linear(1)).toBe(1) })
  it("quadIn endpoints and midpoint", () => { expect(quadIn(0)).toBe(0); expect(quadIn(0.5)).toBe(0.25); expect(quadIn(1)).toBe(1) })
  it("quadOut endpoints and midpoint", () => { expect(quadOut(0)).toBe(0); expect(quadOut(0.5)).toBe(0.75); expect(quadOut(1)).toBe(1) })
  it("quadInOut endpoints and midpoint", () => { expect(quadInOut(0)).toBe(0); expect(quadInOut(0.5)).toBe(0.5); expect(quadInOut(1)).toBe(1) })
  it("cubicIn endpoints and midpoint", () => { expect(cubicIn(0)).toBe(0); expect(cubicIn(0.5)).toBe(0.125); expect(cubicIn(1)).toBe(1) })
  it("cubicOut endpoints and midpoint", () => { expect(cubicOut(0)).toBe(0); expect(cubicOut(0.5)).toBe(0.875); expect(cubicOut(1)).toBe(1) })
  it("cubicInOut endpoints and midpoint", () => { expect(cubicInOut(0)).toBe(0); expect(cubicInOut(0.5)).toBe(0.5); expect(cubicInOut(1)).toBe(1) })
  it("sineInOut endpoints and midpoint", () => { expect(sineInOut(0)).toBe(0); expect(sineInOut(0.5)).toBeCloseTo(0.5); expect(sineInOut(1)).toBe(1) })
  it("backOut endpoints and midpoint", () => { expect(backOut(0)).toBe(0); expect(backOut(1)).toBe(1); expect(backOut(0.5)).toBeGreaterThan(0.5) })
  it("elasticOut endpoints and midpoint", () => { expect(elasticOut(0)).toBe(0); expect(elasticOut(1)).toBe(1); expect(elasticOut(0.5)).toBeGreaterThan(0.9) })
})

describe("tween properties", () => {
  it.each([
    ["x", "x", 10, 30], ["y", "y", 20, 40], ["alpha", "alpha", 0.2, 0.8],
    ["scaleX", "scaleX", 1, 2], ["scaleY", "scaleY", 1, 3], ["rotation", "rotation", 0.1, 1.1],
    ["tint", "tint", 0x000000, 0xffffff]
  ])("%s interpolation and exact finish", (_label, property, start, end) => {
    const target = new Container()
    if (property === "scaleX" || property === "scaleY") target.scale.set(1)
    else target[property as "x"] = start
    const behaviour = tween(target, { [property]: start }, { [property]: end }, 1)
    behaviour.update(0.5)
    behaviour.finish()
    if (property === "scaleX") expect(target.scale.x).toBe(end)
    else if (property === "scaleY") expect(target.scale.y).toBe(end)
    else expect(target[property as "x"]).toBe(end)
    behaviour.dispose()
  })
})

describe("individual behaviour timing", () => {
  it("fade has fixed-dt update and exact finish", () => { const t = new Container(); const b = fade(t, 0, 1); expect(b.update(0.25)).toBe("running"); b.finish(); expect(t.alpha).toBe(0); b.dispose() })
  it("crossfade has fixed-dt update and exact finish", () => { const a = new Container(); const b = new Container(); const c = crossfade(a, b, 1); expect(c.update(0.25)).toBe("running"); c.finish(); expect(a.alpha).toBe(0); expect(b.alpha).toBe(1); c.dispose() })
  it("shake restores origin on finish", () => { const t = new Container(); t.x = 3; const b = shake(t, 4, 1, 10); expect(b.update(0.1)).toBe("running"); b.finish(); expect(t.x).toBe(3); b.dispose() })
  it("hop restores baseline on finish", () => { const t = new Container(); t.y = 4; const b = hop(t, 10, 1); expect(b.update(0.25)).toBe("running"); b.finish(); expect(t.y).toBe(4); b.dispose() })
  it("slide starts at from and finishes at to", () => { const t = new Container(); const b = slide(t, -10, 20, 1); expect(t.x).toBe(-10); expect(b.update(0.5)).toBe("running"); b.finish(); expect(t.x).toBe(20); b.dispose() })
  it.each(["left", "right", "up", "down"] as const)("wipe %s reveal finishes masked target and disposes mask", (direction) => { const t = new Container(); t.addChild(new Graphics().rect(0, 0, 40, 40).fill(0xffffff)); const b = wipe(t, direction, 1, true); expect(b.update(0.25)).toBe("running"); b.finish(); expect(t.mask).toBeTruthy(); b.dispose(); expect(t.mask).toBeFalsy() })
  it.each(["left", "right", "up", "down"] as const)("wipe %s hide reaches exact hidden state", (direction) => { const t = new Container(); t.addChild(new Graphics().rect(0, 0, 40, 40).fill(0xffffff)); const b = wipe(t, direction, 1, false); b.update(0.5); b.finish(); expect(t.mask).toBeTruthy(); b.dispose(); expect(t.mask).toBeFalsy() })
  it("flash reaches transparent overlay", () => { const t = new Container(); const b = flash(t, 0xabcdef, 1); expect(b.update(0.25)).toBe("running"); b.finish(); expect(t.alpha).toBe(0); b.dispose() })
  it("pulse loops and finish restores scale", () => { const t = new Container(); const b = pulse(t, 1.1, 1); expect(b.update(0.25)).toBe("running"); b.finish(); expect(t.scale.x).toBe(1); b.dispose() })
  it("delay reports running then done", () => { const b = delay(0.5); expect(b.update(0.25)).toBe("running"); expect(b.update(0.25)).toBe("done"); b.finish(); b.dispose() })
  it("dispose releases a wipe mask", () => { const t = new Container(); const b = wipe(t, "left", 1, true); expect(t.mask).toBeTruthy(); b.dispose(); expect(t.mask).toBeFalsy() })
})
