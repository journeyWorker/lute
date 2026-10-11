import { Effect } from "effect"
import { Container, Graphics, Text, TextStyle } from "pixi.js"
import type { LineView, PlayMode, Settings } from "@lute-lang/player"
import type { Behaviour, Theme, WorldShape } from "../engine/types.js"
import { buildRevealTimeline, type RevealTimeline } from "./segments.js"
import { mapTextStyle, type TextStyleBindings } from "./richText.js"

/** Design-space dimensions used to place the dialogue window. */
export interface DialogueDesign {
  readonly width: number
  readonly height: number
}

/** Optional visual configuration for the dialogue entity. */
export interface DialogueWindowOptions {
  readonly nameColors?: Readonly<Record<string, number>>
  readonly textStyles?: TextStyleBindings
  readonly boxWidth?: number
  readonly boxHeight?: number
  readonly fontSize?: number
}

const textStyle = (font: string, fill: number, size: number, italic = false): TextStyle =>
  new TextStyle({ fontFamily: font, fontSize: size, fill, fontStyle: italic ? "italic" : "normal" })

const rolePrefix = (role: string): string => role === "os" ? "(" : ""
const roleSuffix = (role: string): string => role === "os" ? ")" : ""

/** A Pixi dialogue panel with a world-owned typewriter behaviour. */
export class DialogueWindow {
  readonly target: Container
  private readonly panel: Graphics
  private readonly namePlate: Container
  private readonly nameText: Text
  private readonly textArea: Container
  private readonly indicator: Text
  private readonly world: WorldShape
  private readonly theme: Theme
  private readonly options: DialogueWindowOptions
  private readonly graphemeTexts: Text[] = []
  private timeline: RevealTimeline = { graphemes: [], pauses: [], duration: 0, locale: "" }
  private currentLine: LineView | undefined
  private revealed = false
  private panelOpacity = 1

  constructor(parent: Container, world: WorldShape, theme: Theme, options: DialogueWindowOptions = {}) {
    this.world = world
    this.theme = theme
    this.options = options
    this.target = new Container()
    this.target.label = "DialogueWindow"
    this.panel = new Graphics()
    this.namePlate = new Container()
    this.nameText = new Text({ text: "", style: textStyle(theme.font, theme.text, options.fontSize ?? 24) })
    this.textArea = new Container()
    this.indicator = new Text({ text: "▼", style: textStyle(theme.font, theme.accent, 18) })
    this.indicator.visible = false
    this.namePlate.addChild(this.nameText)
    this.target.addChild(this.panel, this.namePlate, this.textArea, this.indicator)
    parent.addChild(this.target)
    this.target.visible = false
    this.relayout({ width: 1280, height: 720 })
  }

  /** Shows a line and completes when its reveal behaviour ends. */
  show = (line: LineView, settings: Settings, mode: PlayMode): Effect.Effect<void> => {
    const self = this
    return Effect.gen(function* () {
      yield* self.world.cancel(self.target)
      self.target.visible = true
      self.currentLine = line
      self.timeline = buildRevealTimeline(line.segments, settings.textSpeed, settings.locale)
      self.revealed = mode === "skip" || self.timeline.duration === 0
      self.indicator.visible = false
      self.namePlate.visible = line.speakerName.length > 0
      const role = line.role
      const displayName = `${rolePrefix(role)}${line.speakerName}${roleSuffix(role)}`
      self.nameText.text = displayName
      self.nameText.style = textStyle(
        self.theme.font,
        self.options.nameColors?.[line.speaker] ?? self.theme.accent,
        self.options.fontSize ?? 24,
        role === "mono" || role === "vo"
      )
      self.clearTextChildren()
      self.buildTextChildren(line, settings)
      self.setOpacity(settings.windowOpacity)
      if (line.read && settings.readColour) self.tintText(self.theme.readText)
      else self.tintText(self.theme.text)
      const behaviour = self.typewriterBehaviour()
      if (self.revealed) behaviour.finish()
      yield* self.world.run(self.target, behaviour)
      self.indicator.visible = true
    })
  }

  /** Finishes the current typewriter and makes the complete line visible. */
  get reveal(): Effect.Effect<void> {
    const self = this
    return Effect.gen(function* () {
      yield* self.world.finishAll(self.target)
      self.revealed = true
      self.indicator.visible = self.currentLine !== undefined
    })
  }

  /** Removes line content and cancels behaviours attached to the window. */
  get clear(): Effect.Effect<void> {
    const self = this
    return Effect.gen(function* () {
      yield* self.world.cancel(self.target)
      self.target.visible = false
      self.currentLine = undefined
      self.timeline = { graphemes: [], pauses: [], duration: 0, locale: "" }
      self.clearTextChildren()
      self.nameText.text = ""
      self.namePlate.visible = false
      self.indicator.visible = false
    })
  }


  /** Changes panel opacity without changing the user's configured setting. */
  setOpacity = (alpha: number): void => {
    this.panelOpacity = Math.min(1, Math.max(0, alpha))
    this.panel.alpha = this.panelOpacity
  }

  /** Hides or shows the complete dialogue entity. */
  setHidden = (hidden: boolean): void => {
    this.target.visible = !hidden
  }

  /** Repositions and resizes the panel for a new design resolution. */
  relayout = (design: DialogueDesign): void => {
    const width = this.options.boxWidth ?? Math.min(1180, design.width - 100)
    const height = this.options.boxHeight ?? 190
    const x = (design.width - width) / 2
    const y = design.height - height - 30
    this.panel.clear().roundRect(x, y, width, height, 18).fill({ color: this.theme.panel, alpha: this.panelOpacity * this.theme.panelAlpha })
    const existingPlate = this.namePlate.children[0]
    const plate = existingPlate instanceof Graphics ? existingPlate : new Graphics()
    if (!(existingPlate instanceof Graphics)) this.namePlate.addChildAt(plate, 0)
    plate.clear().roundRect(0, 0, 190, 38, 10).fill({ color: this.theme.panel, alpha: 0.92 })
    this.namePlate.x = x + 26
    this.namePlate.y = y - 40
    this.nameText.x = 14
    this.nameText.y = 5
    this.textArea.x = x + 28
    this.textArea.y = y + 52
    this.indicator.x = x + width - 42
    this.indicator.y = y + height - 36
  }

  private clearTextChildren = (): void => {
    for (const child of this.textArea.removeChildren()) child.visible = false
    this.graphemeTexts.length = 0
  }

  private buildTextChildren = (line: LineView, settings: Settings): void => {
    const fontSize = this.options.fontSize ?? 24
    const byStyle: Array<{ styles: ReadonlyArray<string>; text: string }> = []
    for (const segment of line.segments) {
      if (segment._tag !== "Text") continue
      const previous = byStyle[byStyle.length - 1]
      if (previous !== undefined && previous.styles.length === segment.styles.length && previous.styles.every((s, i) => s === segment.styles[i])) {
        previous.text += segment.text
      } else byStyle.push({ styles: segment.styles, text: segment.text })
    }
    for (const run of byStyle) {
      const style = mapTextStyle(run.styles, this.options.textStyles ?? {}, {
        fontFamily: this.theme.font,
        fontSize,
        color: this.theme.text
      })
      const text = new Text({
        text: "",
        style: {
          ...textStyle(style.fontFamily ?? this.theme.font, typeof style.color === "number" ? style.color : this.theme.text, style.fontSize ?? fontSize, style.fontStyle === "italic"),
          wordWrap: true,
          wordWrapWidth: 1110,
          breakWords: true
        }
      })
      text.alpha = style.alpha ?? 1
      text.scale.set(style.scale ?? 1)
      this.textArea.addChild(text)
      const count = Array.from(new Intl.Segmenter(settings.locale || undefined, { granularity: "grapheme" }).segment(run.text)).length
      for (let i = 0; i < count; i += 1) this.graphemeTexts.push(text)
    }
  }

  private tintText = (colour: number): void => {
    for (const text of this.textArea.children) {
      if (text instanceof Text) text.style.fill = colour
    }
  }

  private typewriterBehaviour = (): Behaviour => {
    let elapsed = 0
    let revealedCount = 0
    return {
      update: (dt) => {
        elapsed += Math.max(0, dt)
        while (revealedCount < this.timeline.graphemes.length) {
          const item = this.timeline.graphemes[revealedCount]
          if (item === undefined || item.end > elapsed) break
          const text = this.graphemeTexts[revealedCount]
          if (text !== undefined) text.text += item.grapheme
          revealedCount += 1
        }
        if (revealedCount >= this.timeline.graphemes.length && elapsed >= this.timeline.duration) {
          this.revealed = true
          return "done"
        }
        return "running"
      },
      finish: () => {
        for (let index = revealedCount; index < this.timeline.graphemes.length; index += 1) {
          const item = this.timeline.graphemes[index]
          const text = this.graphemeTexts[index]
          if (item !== undefined && text !== undefined) text.text += item.grapheme
        }
        revealedCount = this.timeline.graphemes.length
        elapsed = this.timeline.duration
        this.revealed = true
      },
      dispose: () => undefined
    }
  }
}
