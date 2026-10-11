import { Graphics, Text } from "pixi.js"
import type { Theme } from "../../engine/types.js"
import { Widget } from "./base.js"

/** A keyboard and pointer controlled 0–1 slider. */
export class Slider extends Widget {
  private readonly track: Graphics
  private readonly knob: Graphics
  private readonly onChange: (value: number) => void
  private value: number
  private dragging = false
  private readonly trackWidth: number

  constructor(value: number, theme: Theme, onChange: (value: number) => void, width = 260) {
    super(theme, width, 36)
    this.trackWidth = width
    this.value = clamp(value)
    this.onChange = onChange
    this.track = new Graphics()
    this.knob = new Graphics()
    this.addChild(this.track, this.knob)
    this.hit.on("pointerdown", (event) => {
      this.dragging = true
      this.setFromPointer(event.global.x)
    })
    this.hit.on("pointermove", (event) => {
      if (this.dragging) this.setFromPointer(event.global.x)
    })
    this.hit.on("pointerup", () => { this.dragging = false })
    this.hit.on("pointerupoutside", () => { this.dragging = false })
    this.draw()
  }

  /** Current slider value. */
  get current(): number {
    return this.value
  }

  /** Set slider value without emitting an event. */
  setValue(value: number): void {
    this.value = clamp(value)
    this.draw()
  }

  activate(): void {
    this.onChange(this.value)
  }

  /** Handle left/right/home/end slider keys. */
  handleKey(key: string): boolean {
    const amount = 0.05
    if (key === "ArrowLeft" || key === "ArrowDown") this.change(this.value - amount)
    else if (key === "ArrowRight" || key === "ArrowUp") this.change(this.value + amount)
    else if (key === "Home") this.change(0)
    else if (key === "End") this.change(1)
    else return false
    return true
  }

  private setFromPointer(x: number): void {
    const origin = this.getGlobalPosition().x
    this.change((x - origin) / this.trackWidth)
  }

  private change(value: number): void {
    this.value = clamp(value)
    this.draw()
    this.onChange(this.value)
  }

  private draw(): void {
    this.track.clear().roundRect(0, 15, this.trackWidth, 6, 3).fill({ color: this.theme.panel, alpha: 0.9 })
    this.track.roundRect(0, 15, this.trackWidth * this.value, 6, 3).fill({ color: this.theme.accent })
    this.knob.clear().circle(this.trackWidth * this.value, 18, 10).fill({ color: this.theme.accent })
  }
}

/** A two-state switch. */
export class Toggle extends Widget {
  private readonly caption: Text
  private readonly background: Graphics
  private readonly onChange: (value: boolean) => void
  private value: boolean

  constructor(label: string, value: boolean, theme: Theme, onChange: (value: boolean) => void) {
    super(theme, 260, 44)
    this.value = value
    this.onChange = onChange
    this.background = new Graphics()
    this.caption = new Text({ text: label, style: { fontFamily: theme.font, fontSize: 19, fill: theme.text } })
    this.caption.position.set(0, 10)
    this.addChild(this.background, this.caption)
    this.draw()
  }

  get current(): boolean {
    return this.value
  }

  setValue(value: boolean): void {
    this.value = value
    this.draw()
  }

  activate(): void {
    if (!this.isEnabled) return
    this.value = !this.value
    this.draw()
    this.onChange(this.value)
  }

  private draw(): void {
    this.background.clear().roundRect(205, 8, 48, 26, 13).fill({ color: this.value ? this.theme.accent : this.theme.panel })
    this.background.circle(this.value ? 240 : 218, 21, 8).fill({ color: this.theme.text })
  }
}

/** A segmented choice control. */
export class Segmented<T extends string> extends Widget {
  private readonly options: ReadonlyArray<T>
  private readonly buttons: ReadonlyArray<Text>
  private readonly onChange: (value: T) => void
  private value: T
  private readonly background: Graphics

  constructor(options: ReadonlyArray<T>, value: T, theme: Theme, onChange: (value: T) => void) {
    super(theme, Math.max(options.length, 1) * 130, 44)
    this.options = options
    this.value = value
    this.onChange = onChange
    this.background = new Graphics()
    this.addChild(this.background)
    this.buttons = options.map((option, index) => {
      const text = new Text({ text: option, style: { fontFamily: theme.font, fontSize: 18, fill: theme.text } })
      text.position.set(index * 130 + 12, 10)
      text.eventMode = "static"
      text.on("pointertap", () => this.select(option))
      this.addChild(text)
      return text
    })
    this.draw()
  }

  get current(): T {
    return this.value
  }

  activate(): void {
    const index = this.options.indexOf(this.value)
    const next = this.options[(index + 1) % this.options.length]
    if (next !== undefined) this.select(next)
  }

  private select(value: T): void {
    this.value = value
    this.draw()
    this.onChange(value)
  }

  private draw(): void {
    this.background.clear()
    this.options.forEach((option, index) => {
      const selected = option === this.value
      this.background.roundRect(index * 130, 0, 130, 44, 5).fill({ color: selected ? this.theme.accent : this.theme.panel, alpha: 0.9 })
    })
    this.buttons.forEach((text, index) => { text.style = { ...text.style, fill: this.theme.text, fontFamily: this.theme.font }; text.alpha = this.options[index] === this.value ? 1 : 0.75 })
  }
}

function clamp(value: number): number {
  return Math.max(0, Math.min(1, Number.isFinite(value) ? value : 0))
}
