import { Container, Graphics, Sprite, Text, Texture } from "pixi.js"
import type { PlayerAction, ScreenModel } from "@lute-lang/player"
import type { Theme } from "../../engine/types.js"
import { Button, FocusManager, Panel } from "../widgets/index.js"

/** Screen output callback. */
export type EmitAction = (action: PlayerAction) => void

/** Common themed screen container with focus traversal. */
export abstract class ScreenView extends Container {
  readonly focus = new FocusManager()
  protected readonly theme: Theme
  protected readonly emitAction: EmitAction
  protected readonly panel: Panel
  protected readonly title: Text
  protected readonly panelWidth: number
  protected readonly panelHeight: number

  constructor(heading: string, theme: Theme, emit: EmitAction, width = 900, height = 650) {
    super()
    this.theme = theme
    this.emitAction = emit
    this.panelWidth = width
    this.panelHeight = height
    this.panel = new Panel(width, height, theme)
    this.title = new Text({ text: heading, style: { fontFamily: theme.font, fontSize: 34, fill: theme.text } })
    this.title.position.set(32, 24)
    this.addChild(this.panel, this.title)
  }

  /** Centre this screen inside the configured design coordinates. */
  layout(design: { readonly width: number; readonly height: number }, full = false): void {
    if (full) {
      this.panel.clear().rect(0, 0, design.width, design.height).fill({ color: this.theme.panel, alpha: this.theme.panelAlpha })
      this.position.set(0, 0)
      return
    }
    this.position.set((design.width - this.panelWidth) / 2, (design.height - this.panelHeight) / 2)
  }

  /** Add a button and register it in keyboard focus order. */
  protected button(text: string, action: PlayerAction, x: number, y: number, disabled = false): Button {
    const button = new Button(text, this.theme, () => this.emitAction(action))
    button.position.set(x, y)
    button.setEnabled(!disabled)
    this.addChild(button)
    this.focus.register(button)
    return button
  }

  /** Route key events to focusable controls. */
  handleKey(key: string): boolean { return this.focus.handleKey(key) }

  /** Clear scene children and focus before rebuilding dynamic content. */
  protected clearContent(): void { while (this.children.length > 2) this.removeChildAt(2) }
}

/** Title screen with continue gating and optional background data URL. */
export class TitleScreen extends ScreenView {
  constructor(model: Extract<ScreenModel, { _tag: "Title" }>, theme: Theme, emit: EmitAction, gameTitle = "Lute") {
    super(gameTitle, theme, emit, 1280, 720)
    if (model.background !== undefined) {
      const background = new Sprite(Texture.from(model.background))
      background.width = 1280
      background.height = 720
      background.alpha = 0.65
      this.addChildAt(background, 1)
    }
    this.title.position.set(72, 90)
    this.title.style = { ...this.title.style, fontSize: 64 }
    this.button("New game", { type: "newGame" }, 72, 160)
    this.button("Continue", { type: "continue" }, 72, 230, !model.canContinue)
    this.button("Load", { type: "openLoad" }, 72, 300)
    this.button("Settings", { type: "openSettings" }, 72, 370)
  }
}


/** Pause menu screen. */
export class MenuScreen extends ScreenView {
  constructor(theme: Theme, emit: EmitAction) {
    super("Menu", theme, emit, 620, 500)
    this.button("Resume", { type: "close" }, 50, 100)
    this.button("Save", { type: "openSave" }, 50, 160)
    this.button("Load", { type: "openLoad" }, 50, 220)
    this.button("Settings", { type: "openSettings" }, 50, 280)
    this.button("Title", { type: "toTitle" }, 50, 340)
  }
}

/** Confirmation modal. */
export class ConfirmScreen extends ScreenView {
  constructor(model: Extract<ScreenModel, { _tag: "Confirm" }>, theme: Theme, emit: EmitAction) {
    super("Confirm", theme, emit, 680, 320)
    const message = new Text({ text: model.message, style: { fontFamily: theme.font, fontSize: 22, fill: theme.text, wordWrap: true, wordWrapWidth: 600 } })
    message.position.set(36, 90)
    this.addChild(message)
    this.button("Yes", { type: "confirm", yes: true }, 36, 220)
    this.button("No", { type: "confirm", yes: false }, 300, 220)
  }
}

/** End-of-story screen. */
export class EndedScreen extends ScreenView {
  constructor(model: Extract<ScreenModel, { _tag: "Ended" }>, theme: Theme, emit: EmitAction) {
    super("Ended", theme, emit)
    const text = new Text({ text: model.reason, style: { fontFamily: theme.font, fontSize: 24, fill: theme.text, wordWrap: true, wordWrapWidth: 820 } })
    text.position.set(36, 110)
    this.addChild(text)
    this.button("Back to title", { type: "toTitle" }, 36, 510)
  }
}

/** Halted/error screen. */
export class HaltedScreen extends ScreenView {
  constructor(model: Extract<ScreenModel, { _tag: "Halted" }>, theme: Theme, emit: EmitAction) {
    super("Playback halted", theme, emit)
    const text = new Text({ text: model.message, style: { fontFamily: theme.font, fontSize: 24, fill: theme.text, wordWrap: true, wordWrapWidth: 820 } })
    text.position.set(36, 110)
    this.addChild(text)
    this.button("Back to title", { type: "toTitle" }, 36, 510)
  }
}

/** Loading progress screen. */
export class LoadingScreen extends ScreenView {
  constructor(model: Extract<ScreenModel, { _tag: "Loading" }>, theme: Theme, emit: EmitAction) {
    super("Loading", theme, emit)
    const bar = new Graphics().roundRect(45, 220, 810, 26, 13).fill({ color: theme.panel })
    bar.roundRect(45, 220, 810 * Math.max(0, Math.min(1, model.progress)), 26, 13).fill({ color: theme.accent })
    this.addChild(bar)
  }
}

/** Idle flow menu. */
export class IdleScreen extends ScreenView {
  constructor(model: Extract<ScreenModel, { _tag: "Idle" }>, theme: Theme, emit: EmitAction) {
    super("Continue", theme, emit, 620, 420)
    model.options.forEach((option, index) => {
      const x = 190 + (index % 2) * 270
      const y = 130 + Math.floor(index / 2) * 60
      this.button(option.label, { type: "pickIdle", index }, x, y)
    })
  }
}
