import { describe, expect, it } from "vitest"
import type { Bindings, Stage } from "@lute-lang/player"
import { resolveActorTexture } from "../src/scene/actor.js"
import { anchorPlacement } from "../src/scene/shared.js"
import { cgPlacement } from "../src/scene/cgLayer.js"
import { framingPlacement } from "../src/scene/camera.js"
import { themeFromBindings } from "../src/theme.js"

const bindings: Bindings = {
  bindingsVersion: "0.47",
  canvas: { width: 1920, height: 1080 },
  characters: {
    guide: {
      sprite: { default: "guide.png", emotions: { happy: "guide-happy.png" } },
      costumes: { coat: { default: "guide-coat.png", emotions: { happy: "guide-coat-happy.png" } } },
      nameColor: "#e0c080"
    }
  },
  anchors: { left: { x: 0.25, y: 1, scale: 0.5 } },
  ui: { font: "Test", theme: { text: "#123456", accent: "#abcdef", panel: "#010203", panelAlpha: "0.4", readText: "#778899" } }
}

const actor = (extra: Partial<Stage["actors"][number]> = {}): Stage["actors"][number] => ({ character: "guide", ...extra })

describe("scene entities", () => {
  it("resolves sprite textures through emotion, costume, and fallbacks", () => {
    expect(resolveActorTexture(bindings, actor())).toBe("guide.png")
    expect(resolveActorTexture(bindings, actor({ emotion: "happy" }))).toBe("guide-happy.png")
    expect(resolveActorTexture(bindings, actor({ costume: "coat" }))).toBe("guide-coat.png")
    expect(resolveActorTexture(bindings, actor({ costume: "coat", emotion: "unknown" }))).toBe("guide-coat.png")
    expect(resolveActorTexture(bindings, actor({ emotion: "unknown" }))).toBe("guide.png")
  })

  it("computes anchor positioning in design coordinates", () => {
    expect(anchorPlacement(bindings.anchors?.left, { width: 1920, height: 1080 }, { width: 400, height: 800 })).toEqual({ x: 380, y: 680, scale: 0.5 })
  })

  it("computes camera framing and actor focus transforms", () => {
    expect(framingPlacement({ zoom: 2, x: 10, y: -5 }, { width: 1920, height: 1080 }, undefined)).toEqual({ scale: 2, x: 970, y: 535 })
    expect(framingPlacement({ zoom: 2 }, { width: 1920, height: 1080 }, { x: 400, y: 300 })).toEqual({ scale: 2, x: 2080, y: 1020 })
  })

  it("computes cover and contain CG layouts", () => {
    expect(cgPlacement("cover", { width: 1920, height: 1080 }, { width: 800, height: 800 }, 1, undefined, undefined)).toEqual({ x: 0, y: -420, scale: 2.4, width: 1920, height: 1920 })
    expect(cgPlacement("contain", { width: 1920, height: 1080 }, { width: 800, height: 800 }, 0.5, undefined, undefined)).toEqual({ x: 690, y: 270, scale: 0.675, width: 540, height: 540 })
  })

  it("parses theme colours and applies defaults", () => {
    expect(themeFromBindings(bindings)).toEqual({ font: "Test", text: 0x123456, accent: 0xabcdef, panel: 0x010203, panelAlpha: 0.4, readText: 0x778899 })
    expect(themeFromBindings({})).toEqual({ font: "sans-serif", text: 0xffffff, accent: 0xe0c080, panel: 0x101018, panelAlpha: 0.82, readText: 0x9a9aaa })
  })
})
