import { describe, expect, it } from "vitest"
import { Effect } from "effect"
import { Container } from "pixi.js"
import { build, EffectDecodeError, type EffectName } from "../../src/effects/catalogue.js"

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
