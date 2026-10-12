import { Container, Graphics, Rectangle, Text } from "pixi.js"
import type { Theme } from "../../engine/types.js"

/** A callback-free focusable scene node used by FocusManager. */
export interface Focusable {
  readonly focusable: boolean
  focused: boolean
  readonly activate: () => void
  readonly focusNode: Container
  readonly handleKey?: (key: string) => boolean
}

/** Keyboard focus traversal shared by all UI screens. */
export class FocusManager {
  private readonly items: Focusable[] = []
  private index = -1
  private ring: Graphics | undefined

  /** Register a focusable widget in tab order. */
  register(item: Focusable): void {
    if (!this.items.includes(item)) this.items.push(item)
    if (this.index < 0 && item.focusable) this.focus(this.items.indexOf(item))
  }

  /** Remove a widget from traversal. */
  unregister(item: Focusable): void {
    const at = this.items.indexOf(item)
    if (at < 0) return
    this.items.splice(at, 1)
    if (this.index >= this.items.length) this.index = this.items.length - 1
  }

  /** Move focus by a tab or arrow direction. */
  move(delta: number): void {
    if (this.items.length === 0) return
    let next = this.index
    for (let count = 0; count < this.items.length; count += 1) {
      next = (next + delta + this.items.length) % this.items.length
      if (this.items[next]?.focusable) {
        this.focus(next)
        return
      }
    }
  }

  handleKey(key: string): boolean {
    const current = this.items[this.index]
    if (current?.handleKey?.(key) === true) return true
    if (key === "Tab" || key === "ArrowDown" || key === "ArrowRight") {
      this.move(1)
      return true
    }
    if (key === "ArrowUp" || key === "ArrowLeft") {
      this.move(-1)
      return true
    }
    if (key === "Enter" || key === " ") {
      current?.activate()
      return true
    }
    return false
  }

  /** Focus a specific registered index. */
  focus(index: number): void {
    const item = this.items[index]
    if (!item?.focusable) return
    if (this.index >= 0) {
      const previous = this.items[this.index]
      if (previous) previous.focused = false
    }
    this.index = index
    item.focused = true
    this.drawRing(item)
  }

  /** Focus the first available widget. */
  focusFirst(): void {
    const index = this.items.findIndex((item) => item.focusable)
    if (index >= 0) this.focus(index)
  }

  /** Current focused widget, if any. */
  get current(): Focusable | undefined {
    return this.items[this.index]
  }

  private drawRing(item: Focusable): void {
    const parent = item.focusNode.parent
    if (!parent) return
    this.ring?.removeFromParent()
    const bounds = item.focusNode.getLocalBounds()
    const ring = new Graphics()
      .roundRect(item.focusNode.x + bounds.x * item.focusNode.scale.x - 3, item.focusNode.y + bounds.y * item.focusNode.scale.y - 3, bounds.width * item.focusNode.scale.x + 6, bounds.height * item.focusNode.scale.y + 6, 5)
      .stroke({ color: 0xffffff, width: 2, alpha: 0.9 })
    parent.addChild(ring)
    this.ring = ring
  }
}

/** Base class for interactive Pixi widgets. */
export abstract class Widget extends Container implements Focusable {
  readonly focusable = true
  private hasFocus = false
  protected readonly hit: Graphics
  protected theme: Theme
  private enabled = true

  constructor(theme: Theme, width: number, height: number) {
    super()
    this.theme = theme
    this.eventMode = "static"
    this.hitArea = new Rectangle(0, 0, width, height)
    this.cursor = "pointer"
    this.on("pointerdown", (event) => {
      if ("preventDefault" in event.nativeEvent) event.nativeEvent.preventDefault()
      event.stopPropagation()
    })
    this.on("pointerup", (event) => {
      if ("preventDefault" in event.nativeEvent) event.nativeEvent.preventDefault()
      event.stopPropagation()
    })
    this.hit = new Graphics().rect(0, 0, width, height).fill({ color: 0xffffff, alpha: 0.001 })
    this.hit.eventMode = "static"
    this.hit.cursor = "pointer"
    this.addChild(this.hit)
    this.hit.on("pointertap", () => this.activate())
    this.on("pointerout", () => this.setHover(false))
  }

  get focused(): boolean { return this.hasFocus }
  set focused(value: boolean) { this.hasFocus = value; this.setHover(value) }
  get focusNode(): Container {
    return this
  }

  /** Set widget availability and pointer interaction. */
  setEnabled(enabled: boolean): void {
    this.enabled = enabled
    this.alpha = enabled ? 1 : 0.45
    this.eventMode = enabled ? "static" : "none"
    this.hit.eventMode = enabled ? "static" : "none"
  }

  /** True when pointer and keyboard activation are accepted. */
  get isEnabled(): boolean {
    return this.enabled
  }

  /** Implement visual activation. */
  abstract activate(): void

  /** Apply hover visuals. */
  protected setHover(_hovered: boolean): void {}

  /** Update theme colours. */
  setTheme(theme: Theme): void {
    this.theme = theme
  }

}

/** Draw a text label with the active theme. */
export function label(text: string, theme: Theme, size = 22): Text {
  return new Text({ text, style: { fontFamily: theme.font, fontSize: size, fill: theme.text } })
}
