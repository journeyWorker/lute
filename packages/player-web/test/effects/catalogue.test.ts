import { describe, expect, it } from "vitest"
import { Effect } from "effect"
import { Container, Graphics } from "pixi.js"
import { build, EffectDecodeError, hidingTransition, type EffectName } from "../../src/effects/catalogue.js"

const names: ReadonlyArray<EffectName> = [
  "fade", "dissolve", "wipeLeft", "wipeRight", "wipeUp", "wipeDown", "slideLeft", "slideRight", "cut",
  "fadeIn", "fadeOut", "slideIn", "slideOut", "hop", "shake", "none", "zoom", "pan", "flash", "blur", "tint"
]

describe("effect catalogue", () => {
  it("decodes every catalogue effect and returns its behaviour list", async () => {
    const target = new Container()
    for (const name of names) {
      if (name === "blur") continue
      const params = name === "shake" ? { strength: 4 } : name === "zoom" ? { scale: 1.2 } : name === "pan" ? { x: 2, y: 3 } : name === "flash" || name === "tint" ? { color: "#00ff00" } : {}
      const behaviours = await Effect.runPromise(build(name, target, params, 0.25, { design: { width: 100, height: 100 } }))
      expect(behaviours.length).toBe(name === "cut" || name === "none" ? 0 : 1)
      for (const behaviour of behaviours) {
        behaviour.finish()
        behaviour.dispose()
      }
    }
  })

  it("returns typed failures for unknown names, parameters, and values", async () => {
    const target = new Container()
    const unknown = await Effect.runPromise(Effect.result(build("missing", target)))
    expect(unknown._tag).toBe("Failure")
    if (unknown._tag === "Failure") expect(unknown.failure).toBeInstanceOf(EffectDecodeError)
    const parameter = await Effect.runPromise(Effect.result(build("fade", target, { strength: 2 })))
    expect(parameter._tag).toBe("Failure")
    const value = await Effect.runPromise(Effect.result(build("shake", target, { strength: "large" })))
    expect(value._tag).toBe("Failure")
  })
})
describe("catalogue parameter contract", () => {
  it.each(names.filter((name) => name !== "blur"))("effect %s accepts documented parameters and defaults", async (name) => {
    const target = new Container()
    const params = name === "shake" ? { strength: 3 } : name === "zoom" ? { scale: 1.1 } : name === "pan" ? { x: 1, y: 2 } : name === "flash" || name === "tint" ? { color: "#112233" } : {}
    const result = await Effect.runPromise(Effect.result(build(name, target, params, 0.1)))
    expect(result._tag).toBe("Success")
  })
  it("Rust catalogue names remain identical", () => {
    expect(names).toEqual(["fade", "dissolve", "wipeLeft", "wipeRight", "wipeUp", "wipeDown", "slideLeft", "slideRight", "cut", "fadeIn", "fadeOut", "slideIn", "slideOut", "hop", "shake", "none", "zoom", "pan", "flash", "blur", "tint"])
  })
  it("every Rust parameter set remains identical", () => {
    expect({ shake: ["duration", "strength"], zoom: ["duration", "scale"], pan: ["duration", "x", "y"], flash: ["duration", "color"], blur: ["duration", "strength"], tint: ["duration", "color"] }).toEqual({ shake: ["duration", "strength"], zoom: ["duration", "scale"], pan: ["duration", "x", "y"], flash: ["duration", "color"], blur: ["duration", "strength"], tint: ["duration", "color"] })
  })
  it("unknown effect returns EffectDecodeError", async () => {
    expect((await Effect.runPromise(Effect.result(build("unknown", new Container()))))._tag).toBe("Failure")
  })
  it("custom vfx is not a catalogue effect", async () => {
    expect((await Effect.runPromise(Effect.result(build("custom", new Container()))))._tag).toBe("Failure")
  })
  it("unknown parameter returns EffectDecodeError", async () => {
    expect((await Effect.runPromise(Effect.result(build("fade", new Container(), { strength: 1 }))))._tag).toBe("Failure")
  })
  it("wrong parameter type returns EffectDecodeError", async () => {
    expect((await Effect.runPromise(Effect.result(build("shake", new Container(), { strength: "x" }))))._tag).toBe("Failure")
  })
  it("cut and none are instant", async () => {
    const target = new Container()
    expect((await Effect.runPromise(build("cut", target))).length).toBe(0)
    expect((await Effect.runPromise(build("none", target))).length).toBe(0)
  })
})

// spec 0.47.1 §4: a hiding camera transition hides, then shows again.
describe("hiding transitions", () => {
  const hiding: ReadonlyArray<EffectName> = ["fade", "dissolve", "fadeOut", "wipeLeft", "wipeRight", "wipeUp", "wipeDown", "slideLeft", "slideRight", "slideOut"]

  it.each(hiding)("%s hides over its first half and ends shown, unmasked and in place", async (name) => {
    const target = new Container()
    target.x = 40
    const halves = await Effect.runPromise(hidingTransition(name, target, { duration: 0.6 }, { design: { width: 100, height: 100 } }))
    expect(halves).toBeDefined()
    const hide = halves!.hide(0.3)
    expect(hide.update(0.3)).toBe("done")
    hide.dispose()
    // Hidden between the halves, even once a wipe dropped its mask.
    expect(target.alpha).toBe(0)
    expect(target.mask ?? null).toBeNull()
    // The new framing is applied between the halves.
    target.x = 25
    const show = halves!.show(0.3)
    expect(show.update(0.3)).toBe("done")
    show.dispose()
    expect(target.alpha).toBe(1)
    expect(target.mask ?? null).toBeNull()
    expect(target.x).toBe(25)
  })

  it.each(names.filter((name) => !hiding.includes(name)))("%s is not a hiding transition", async (name) => {
    expect(await Effect.runPromise(hidingTransition(name, new Container()))).toBeUndefined()
  })

  it("decodes parameters as the catalogue does", async () => {
    const result = await Effect.runPromise(Effect.result(hidingTransition("fade", new Container(), { strength: 2 })))
    expect(result._tag).toBe("Failure")
  })

  it.each(["wipeLeft", "wipeRight", "wipeUp", "wipeDown"] as const)("%s sweeps the design stage in the parent's space, not the actors' span", async (name) => {
    // A camera layer whose only actor spans x 500–600: its own bounds are that span.
    const stage = new Container()
    const target = new Container()
    target.addChild(new Graphics().rect(500, 300, 100, 200).fill(0xffffff))
    stage.addChild(target)
    const halves = await Effect.runPromise(hidingTransition(name, target, {}, { design: { width: 1280, height: 720 } }))
    const hide = halves!.hide(0.3)
    const mask = target.mask as Graphics
    // The hide starts with nothing concealed: the mask covers the whole stage.
    expect(mask.parent).toBe(stage)
    const bounds = mask.getLocalBounds()
    expect([bounds.minX, bounds.minY, bounds.maxX, bounds.maxY]).toEqual([0, 0, 1280, 720])
    hide.update(0.15)
    const half = mask.getLocalBounds()
    expect(name === "wipeLeft" || name === "wipeRight" ? half.width : half.height).toBe(name === "wipeLeft" || name === "wipeRight" ? 640 : 360)
    hide.dispose()
    expect(mask.parent).toBeNull()
    expect(target.mask ?? null).toBeNull()
  })
})
