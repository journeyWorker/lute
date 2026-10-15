import { Effect } from "effect"
import { Container, Graphics, Text } from "pixi.js"
import { describe, expect, it } from "vitest"
import type { LineView, Settings } from "@lute-lang/player"
import type { Behaviour, Theme, WorldShape } from "../../../src/engine/types.js"
import { DialogueWindow } from "../../../src/text/dialogueWindow.js"

const theme: Theme = {
  font: "sans-serif",
  text: 0xffffff,
  accent: 0xffcc66,
  panel: 0x111122,
  panelAlpha: 0.9,
  readText: 0x99aacc
}

const settings: Settings = {
  textSpeed: 8,
  autoDelay: 0,
  skipMode: "read",
  volumes: { master: 1, music: 1, sfx: 1, voice: 1 },
  locale: "en",
  windowOpacity: 0.8,
  readColour: true,
  rollbackDepth: 10
}

const line: LineView = {
  lineId: "line-1",
  voiceKey: "",
  role: "dialogue",
  speaker: "alice",
  speakerName: "Alice",
  segments: [{ _tag: "Text", text: "Hello world", styles: [] }],
  read: false
}

const worldFor = (): WorldShape => {
  let active: Behaviour | undefined
  return {
    run: (_target, behaviour) => Effect.sync(() => {
      active = behaviour
      behaviour.update(0.25)
    }),
    finishAll: () => Effect.sync(() => {
      active?.finish()
      active = undefined
    }),
    cancel: () => Effect.sync(() => {
      active?.dispose()
      active = undefined
    }),
    size: Effect.sync(() => active === undefined ? 0 : 1)
  }
}

describe("DialogueWindow browser scene", () => {

  it("renders the speaker name above a reusable plate background", async () => {
    const entity = new DialogueWindow(new Container(), worldFor(), theme)
    await Effect.runPromise(entity.show(line, settings, "skip"))
    const plate = entity.target.children[1]?.children[0]
    const name = entity.target.children[1]?.children[1]
    expect(plate).toBeInstanceOf(Graphics)
    expect(name).toBeInstanceOf(Text)
    expect((name as Text).text).toBe("Alice")
    entity.relayout({ width: 1920, height: 1080 })
    expect(entity.target.children[1]?.children).toHaveLength(2)
    expect(entity.target.children[1]?.children[0]).toBe(plate)
  })

  it("shows the cast display name, coloured by the speaker id, for a line with emotion and variant", async () => {
    const entity = new DialogueWindow(new Container(), worldFor(), theme, { nameColors: { edda: 0x66ccff } })
    const cast: LineView = { ...line, speaker: "edda", speakerName: "에다", emotion: "happy", variant: "wet" }
    await Effect.runPromise(entity.show(cast, settings, "skip"))
    const name = entity.target.children[1]?.children[1]
    expect(name).toBeInstanceOf(Text)
    expect((name as Text).text).toBe("에다")
    expect((name as Text).style.fill).toBe(0x66ccff)
  })

  it("progressively reveals text", async () => {
    const entity = new DialogueWindow(new Container(), worldFor(), theme)
    await Effect.runPromise(entity.show(line, settings, "normal"))
    expect(entity.target.children[2]?.children[0]).toBeDefined()
  })

  it("reveal completes the typewriter", async () => {
    const entity = new DialogueWindow(new Container(), worldFor(), theme)
    await Effect.runPromise(entity.show(line, settings, "normal"))
    await Effect.runPromise(entity.reveal)
    expect(entity.target.children[2]?.children[0]).toBeDefined()
  })

  it("applies the speaker name color", async () => {
    const entity = new DialogueWindow(new Container(), worldFor(), theme, { nameColors: { alice: 0x66ccff } })
    await Effect.runPromise(entity.show(line, settings, "skip"))
    expect(entity.target.children[1]?.children[1]).toBeDefined()
  })
  it("renders speaker and progressively reveals text, then reveals", async () => {
    const parent = new Container()
    const entity = new DialogueWindow(parent, worldFor(), theme, { nameColors: { alice: 0x66ccff } })
    await Effect.runPromise(entity.show(line, settings, "normal"))
    expect(entity.target.visible).toBe(true)
    expect(parent.children).toHaveLength(1)
    expect(entity.target.children[1]?.visible).toBe(true)
    await Effect.runPromise(entity.reveal)
    expect(entity.target.children[2]?.children[0]).toBeDefined()
    entity.setOpacity(0.4)
    expect(entity.target.children[0]?.alpha).toBe(0.4)
    entity.setHidden(true)
    expect(entity.target.visible).toBe(false)
  })

  it("clears the text area", async () => {
    const entity = new DialogueWindow(new Container(), worldFor(), theme)
    await Effect.runPromise(entity.show(line, settings, "skip"))
    await Effect.runPromise(entity.clear)
    expect(entity.target.children[2]?.children).toHaveLength(0)
  })

  it("sets opacity", () => {
    const entity = new DialogueWindow(new Container(), worldFor(), theme)
    entity.setOpacity(0.4)
    expect(entity.target.children[0]?.alpha).toBe(0.4)
  })

  it("sets hidden state", () => {
    const entity = new DialogueWindow(new Container(), worldFor(), theme)
    entity.setHidden(true)
    expect(entity.target.visible).toBe(false)
  })

  it("applies read tint", async () => {
    const entity = new DialogueWindow(new Container(), worldFor(), theme)
    await Effect.runPromise(entity.show({ ...line, read: true }, settings, "skip"))
    expect(entity.target.children[2]?.children[0]).toBeDefined()
  })

  it("hides the name plate for narration and clears the text area", async () => {
    const entity = new DialogueWindow(new Container(), worldFor(), theme)
    const narration = { ...line, speakerName: "", role: "os", read: true }
    await Effect.runPromise(entity.show(narration, settings, "skip"))
    expect(entity.target.children[1]?.visible).toBe(false)
    expect(entity.target.children[2]?.children[0]).toBeDefined()
    await Effect.runPromise(entity.clear)
    expect(entity.target.children[2]?.children).toHaveLength(0)
  })

  it("relayouts the panel and updates visibility", () => {
    const entity = new DialogueWindow(new Container(), worldFor(), theme)
    entity.relayout({ width: 1920, height: 1080 })
    entity.setHidden(true)
    expect(entity.target.visible).toBe(false)
    entity.setHidden(false)
    expect(entity.target.visible).toBe(true)
  })

  it("styles mono and parenthesizes os role names", async () => {
    const entity = new DialogueWindow(new Container(), worldFor(), theme)
    await Effect.runPromise(entity.show({ ...line, role: "mono" }, settings, "skip"))
    const name = entity.target.children[1]?.children[1]
    expect(name instanceof Text).toBe(true)
    if (name instanceof Text) expect(name.text).toBe("Alice")
    await Effect.runPromise(entity.show({ ...line, role: "os" }, settings, "skip"))
    const osName = entity.target.children[1]?.children[1]
    expect(osName instanceof Text).toBe(true)
    if (osName instanceof Text) expect(osName.text).toBe("(Alice)")
  })

  it("styles voice-over role", async () => {
    const entity = new DialogueWindow(new Container(), worldFor(), theme)
    await Effect.runPromise(entity.show({ ...line, role: "vo" }, settings, "skip"))
    expect(entity.target.children[1]?.children[1]).toBeDefined()
  })
})
