import { Graphics, Text } from "pixi.js"
import type { Theme } from "../../engine/types.js"
import { Widget } from "./base.js"

/** An interactive labelled button with pointer, focus and keyboard states. */
export class Button extends Widget {
  private readonly background: Graphics
  private readonly caption: Text
  private readonly onPress: () => void
  private hovered = false

  constructor(text: string, theme: Theme, onPress: () => void, icon?: string) {
    super(theme, 240, 48)
    this.onPress = onPress
    this.background = new Graphics()
    this.caption = new Text({
      text: icon ? `${icon} ${text}` : text,
      style: { fontFamily: theme.font, fontSize: 20, fill: theme.text }
    })
    this.caption.position.set(14, 11)
    this.addChild(this.background, this.caption)
    this.draw(false)
  }

  /** Invoke the button action when enabled. */
  activate(): void {
    if (this.isEnabled) this.onPress()
  }

  protected setHover(hovered: boolean): void {
    this.hovered = hovered
    this.draw(hovered)
  }

  override setTheme(theme: Theme): void {
    super.setTheme(theme)
    this.caption.style = { ...this.caption.style, fontFamily: theme.font, fill: theme.text }
    this.draw(this.hovered)
  }

  private draw(hovered: boolean): void {
    const color = hovered || this.focused ? this.theme.accent : this.theme.panel
    this.background.clear().roundRect(0, 0, 240, 48, 8).fill({ color, alpha: this.focused ? 0.95 : 0.85 })
  }
}
