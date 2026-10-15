import { Effect, Layer, Stream } from "effect"
import { Container, EventBoundary, FederatedPointerEvent, Point, Sprite, Texture, type Application } from "pixi.js"
import { describe, expect, it } from "vitest"
import type { Bindings, ChoiceView, Stage, StageCue } from "@lute-lang/player"
import type { Layers, Theme } from "../../src/engine/types.js"
import { PixiApp } from "../../src/engine/app.js"
import { World } from "../../src/engine/world.js"
import { Ui } from "../../src/ui/ui.js"
import { PixiPresenter, type PresenterOptions } from "../../src/presenter/pixiPresenter.js"
import { themeFromBindings } from "../../src/theme.js"

const theme: Theme = { font: "sans-serif", text: 0xffffff, accent: 0xe0c080, panel: 0x101018, panelAlpha: 0.82, readText: 0x9a9aaa }
const bindings: Bindings = {
  bindingsVersion: "0.45",
  canvas: { width: 320, height: 180 },
  assets: { bg: "bg.png", night: "night.png", guide: "guide.png", happy: "happy.png", cg: "cg.png", video: "video.png", sequence: "sequence.png" },
  backgrounds: { foyer: { image: "bg.png", times: { night: "night.png" } } },
  characters: { guide: { sprite: { default: "guide.png", emotions: { happy: "happy.png" } }, nameColor: "#e0c080" } },
  anchors: { left: { x: 0.25, y: 1, scale: 0.5 } },
  framing: { close: { zoom: 1.5, x: 3, y: -2 } },
  cgLayouts: { inset: { fit: "contain", scale: 0.5 } },
  sequences: { intro: { video: "sequence.png" } },
  ui: { theme: { text: "#ffffff", accent: "#e0c080" } }
}

const textureMap = (): Readonly<Record<string, Texture>> => {
  const make = (colour: string): Texture => {
    const canvas = document.createElement("canvas")
    canvas.width = 32
    canvas.height = 32
    const context = canvas.getContext("2d")
    if (context !== null) {
      context.fillStyle = colour
      context.fillRect(0, 0, 32, 32)
    }
    canvas.toDataURL("image/png")
    return Texture.from(canvas)
  }
  return {
    "bg.png": make("red"), "night.png": make("blue"), "guide.png": make("green"), "happy.png": make("yellow"),
    "cg.png": make("purple"), "video.png": make("orange"), "sequence.png": make("pink")
  }
}

const makeBindingsAssets = () => {
  const textures = textureMap()
  return { texture: (file: string) => textures[file], video: (file: string) => textures[file] }
}

const withPresenter = async (callback: (presenter: PixiPresenter, pixi: { readonly layers: Layers; readonly app: Application }) => Promise<void>, extra: Pick<PresenterOptions, "debug"> = {}): Promise<void> => {
  const element = document.createElement("div")
  element.style.width = "320px"
  element.style.height = "180px"
  document.body.appendChild(element)
  const appLayer = PixiApp.layer(element, bindings.canvas)
  const engineLayer = World.layer.pipe(Layer.provideMerge(appLayer))
  await Effect.runPromise(Effect.scoped(Effect.gen(function* () {
    const pixi = yield* PixiApp
    const world = yield* World
    const ui = new Ui(pixi.layers.overlay, theme, world)
    ui.setDesign(pixi.design)
    const presenter = new PixiPresenter({ pixi, world, ui, assets: makeBindingsAssets(), bindings, theme: themeFromBindings(bindings), ...extra })
    yield* Effect.promise(() => callback(presenter, pixi))
  }).pipe(Effect.provide(engineLayer))))
  element.remove()
}

const actorCue = (extra: Partial<Stage["actors"][number]> = {}): Extract<StageCue, { readonly _tag: "ActorShow" }> => ({ _tag: "ActorShow", actor: { character: "guide", anchor: "left", ...extra }, action: undefined, duration: undefined })

describe("PixiPresenter scene cues", () => {
  it("stages backgrounds with bound time variants", async () => withPresenter(async (presenter, pixi) => {
    await Effect.runPromise(presenter.stage({ _tag: "Background", bg: { location: "foyer", time: "night" }, transition: undefined, duration: undefined }, "skip"))
    expect(pixi.layers.background.children[0]?.children[0]?.label).toBe("Texture:night.png")
  }))

  it("adds actors at anchors with their resolved texture", async () => withPresenter(async (presenter, pixi) => {
    await Effect.runPromise(presenter.stage(actorCue(), "skip"))
    const actor = pixi.layers.actors.children[0]
    expect(actor?.label).toBe("Actor:guide")
    expect(actor?.children[0]).toBeInstanceOf(Sprite)
    expect(actor?.children[0]?.x).toBe(72)
  }))

  it("swaps actor emotions", async () => withPresenter(async (presenter, pixi) => {
    await Effect.runPromise(presenter.stage(actorCue(), "skip"))
    await Effect.runPromise(presenter.stage(actorCue({ emotion: "happy" }), "skip"))
    expect(pixi.layers.actors.children[0]?.children[0]?.label).toBe("Sprite:happy.png")
  }))

  it("removes actors on exit", async () => withPresenter(async (presenter, pixi) => {
    await Effect.runPromise(presenter.stage(actorCue(), "skip"))
    await Effect.runPromise(presenter.stage({ _tag: "ActorExit", character: "guide", action: undefined, duration: undefined }, "skip"))
    expect(pixi.layers.actors.children).toHaveLength(0)
  }))

  it("shows and hides CGs with their layout", async () => withPresenter(async (presenter, pixi) => {
    await Effect.runPromise(presenter.stage({ _tag: "Cg", cg: { assetId: "cg", layout: "inset" }, transition: undefined, duration: undefined }, "skip"))
    const cg = pixi.layers.cg.children[0]?.children[0] as Sprite
    expect(cg).toBeInstanceOf(Sprite)
    expect(cg.scale.x).toBeCloseTo(2.8125)
    await Effect.runPromise(presenter.stage({ _tag: "Cg", cg: null, transition: undefined, duration: undefined }, "skip"))
    expect(pixi.layers.cg.children).toHaveLength(0)
  }))

  it("shows and hides videos", async () => withPresenter(async (presenter, pixi) => {
    await Effect.runPromise(presenter.stage({ _tag: "Video", video: { assetId: "video" }, blocking: false }, "skip"))
    expect(pixi.layers.video.children).toHaveLength(1)
    await Effect.runPromise(presenter.stage({ _tag: "Video", video: null, blocking: false }, "skip"))
    expect(pixi.layers.video.children).toHaveLength(0)
  }))

  it("applies camera framing transforms", async () => withPresenter(async (presenter, pixi) => {
    await Effect.runPromise(presenter.stage({ _tag: "Camera", focus: undefined, framing: "close", move: undefined, transition: undefined, duration: undefined }, "skip"))
    expect(pixi.layers.camera.scale.x).toBe(1.5)
    expect(pixi.layers.camera.x).toBe(163)
    expect(pixi.layers.camera.y).toBe(88)
  }))

  it("finishes flash VFX at zero alpha", async () => withPresenter(async (presenter, pixi) => {
    await Effect.runPromise(presenter.stage({ _tag: "Vfx", effect: { effect: "flash", params: {} }, duration: 0.2 }, "skip"))
    expect(pixi.layers.overlay.children[0]?.alpha).toBe(0)
  }))

  it("plays nothing for a custom vfx and logs each name once at debug level", async () => {
    const debug: Array<string> = []
    await withPresenter(async (presenter, pixi) => {
      const labels = (): Array<string> => {
        const found: Array<string> = []
        const walk = (node: Container): void => {
          found.push(node.label)
          for (const child of node.children) walk(child)
        }
        walk(pixi.layers.overlay)
        return found
      }
      const custom = (name: string): StageCue => ({ _tag: "Vfx", effect: { effect: "custom", name, params: {} }, duration: 2 })
      const before = labels()
      await Effect.runPromise(presenter.stage(custom("lanterns"), "normal"))
      await Effect.runPromise(presenter.stage(custom("lanterns"), "skip"))
      await Effect.runPromise(presenter.stage(custom("tide-rise"), "normal"))
      expect(labels()).toEqual(before)
    }, { debug: (message) => debug.push(message) })
    expect(debug).toHaveLength(2)
    expect(debug[0]).toContain("lanterns")
    expect(debug[1]).toContain("tide-rise")
  })

  it("plays a bound sequence", async () => withPresenter(async (presenter, pixi) => {
    await Effect.runPromise(presenter.stage({ _tag: "Sequence", name: "intro", video: undefined }, "skip"))
    expect(pixi.layers.video.children).toHaveLength(1)
  }))

  it("completes normal effects on the real ticker", async () => withPresenter(async (presenter, pixi) => {
    await Effect.runPromise(presenter.stage({ ...actorCue(), action: { effect: "fadeOut", params: {} }, duration: 0.03 }, "normal"))
    expect(pixi.layers.actors.children[0]?.alpha).toBeCloseTo(0)
  }))

  it("restores the same visual stage reached by cues", async () => withPresenter(async (presenter, pixi) => {
    const stage: Stage = { actors: [{ character: "guide", anchor: "left" }], bg: { location: "foyer" }, cg: { assetId: "cg", layout: "inset" }, video: null, music: null }
    await Effect.runPromise(presenter.stage({ _tag: "Background", bg: stage.bg, transition: undefined, duration: undefined }, "skip"))
    await Effect.runPromise(presenter.stage(actorCue(), "skip"))
    await Effect.runPromise(presenter.stage({ _tag: "Cg", cg: stage.cg, transition: undefined, duration: undefined }, "skip"))
    const reached = [pixi.layers.background.children.length, pixi.layers.actors.children.length, pixi.layers.cg.children.length]
    await Effect.runPromise(presenter.restore(stage))
    expect([pixi.layers.background.children.length, pixi.layers.actors.children.length, pixi.layers.cg.children.length]).toEqual(reached)
  }))

  it("shows and reveals a line", async () => withPresenter(async (presenter, pixi) => {
    const line = { lineId: "line", voiceKey: "", role: "", speaker: "guide", speakerName: "Guide", segments: [{ _tag: "Text" as const, text: "Hello", styles: [] }], read: false }
    await Effect.runPromise(presenter.showLine(line, "skip"))
    await Effect.runPromise(presenter.revealLine)
    expect(pixi.layers.dialogue.children[0]?.label).toBe("DialogueWindow")
  }))

  it("resolves showChoice from a pointer click", async () => withPresenter(async (presenter, pixi) => {
    const menu: ChoiceView = {
      id: "menu",
      construct: "branch",
      prompt: "Choose",
      timeout: undefined,
      default: undefined,
      options: [{ id: "yes", text: "Yes", exit: true, once: false, verdict: "open" }]
    }
    const result = Effect.runPromise(presenter.showChoice(menu, Stream.empty))
    const target = pixi.layers.choice.children[0]
    const tap = new FederatedPointerEvent(new EventBoundary(target ?? new Container()))
    tap.global.copyFrom(new Point(50, 80))
    target?.emit("pointertap", tap)
    await expect(result).resolves.toBe("yes")
  }))

  it("captures a PNG data URL", async () => withPresenter(async (presenter) => {
    await expect(Effect.runPromise(presenter.capture)).resolves.toMatch(/^data:image\/png;base64/)
  }))

  it("renders a placeholder for an unbound member", async () => withPresenter(async (presenter, pixi) => {
    await Effect.runPromise(presenter.stage({ _tag: "ActorShow", actor: { character: "missing" }, action: undefined, duration: undefined }, "skip"))
    expect(pixi.layers.actors.children[0]?.children[0]?.label).toContain("Placeholder:character:missing")
  }))
})
