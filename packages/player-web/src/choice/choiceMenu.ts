import { Deferred, Effect, Stream } from "effect"
import { Container, Graphics, Text, TextStyle } from "pixi.js"
import type { Contract } from "@lute-lang/runtime"
import type { Theme } from "../engine/types.js"

/** A menu option with the runtime's visibility verdict and optional display label. */
export interface ChoiceOption {
  readonly id: string
  readonly verdict: string
  readonly label?: string
}

/** Whether a runtime verdict leaves an option selectable. */
export const isChoiceEnabled = (verdict: string): boolean =>
  verdict !== "hidden" &&
  verdict !== "invisible" &&
  verdict !== "closed" &&
  verdict !== "disabled" &&
  verdict !== "false" &&
  verdict !== "ineligible"

/** Immutable keyboard/pointer navigation state. */
export interface ChoiceNavigationState {
  readonly ids: ReadonlyArray<string>
  readonly enabled: ReadonlyArray<boolean>
  readonly focused: number
}

/** Creates navigation state, selecting the first enabled option. */
export const choiceNavigation = (options: ReadonlyArray<ChoiceOption>): ChoiceNavigationState => {
  const enabled = options.map((option) => isChoiceEnabled(option.verdict))
  const first = enabled.findIndex(Boolean)
  return { ids: options.map((option) => option.id), enabled, focused: first < 0 ? -1 : first }
}

/** Applies a navigation key or a one-based digit to a menu state. */
export const navigateChoice = (
  state: ChoiceNavigationState,
  key: "up" | "down" | "enter" | string
): ChoiceNavigationState | string | undefined => {
  if (key === "enter") return state.focused >= 0 && state.enabled[state.focused] === true ? state.ids[state.focused] : undefined
  if (/^[1-9]$/u.test(key)) {
    const index = Number(key) - 1
    return state.enabled[index] === true ? state.ids[index] : undefined
  }
  if (state.focused < 0 || (key !== "up" && key !== "down")) return state
  const direction = key === "up" ? -1 : 1
  let index = state.focused
  for (let count = 0; count < state.ids.length; count += 1) {
    index = (index + direction + state.ids.length) % state.ids.length
    if (state.enabled[index] === true) return { ...state, focused: index }
  }
  return state
}

interface RenderedOption {
  readonly option: ChoiceOption
  readonly button: Graphics
  readonly label: Text
}

const hasLabel = (option: ChoiceOption): option is ChoiceOption & { readonly label: string } =>
  typeof option.label === "string"

/** A vertical, keyboard and pointer accessible choice menu. */
export class ChoiceMenu {
  readonly target: Container
  private readonly theme: Theme
  private readonly panel: Graphics
  private readonly prompt: Text
  private readonly optionsLayer: Container
  private readonly countdown: Graphics
  private readonly rendered: RenderedOption[] = []
  private navigation: ChoiceNavigationState = { ids: [], enabled: [], focused: -1 }
  private deferred: Deferred.Deferred<string> | undefined
  private resolveChoice: ((id: string) => void) | undefined
  private maxRemaining = 0
  private remainingValue = 0
  private pulsePhase = 0

  constructor(parent: Container, theme: Theme) {
    this.theme = theme
    this.target = new Container()
    this.target.label = "ChoiceMenu"
    this.panel = new Graphics()
    this.prompt = new Text({ text: "", style: new TextStyle({ fontFamily: theme.font, fontSize: 24, fill: theme.text, wordWrap: true, wordWrapWidth: 632 }) })
    this.optionsLayer = new Container()
    this.countdown = new Graphics()
    this.target.addChild(this.panel, this.prompt, this.optionsLayer, this.countdown)
    parent.addChild(this.target)
    this.target.position.set(300, 120)
    this.target.eventMode = "static"
    this.target.on("pointerdown", (event) => {
      if ("preventDefault" in event.nativeEvent) event.nativeEvent.preventDefault()
      if ("stopImmediatePropagation" in event.nativeEvent) event.nativeEvent.stopImmediatePropagation()
      event.stopPropagation()
    })
    this.target.on("pointerup", (event) => {
      if ("preventDefault" in event.nativeEvent) event.nativeEvent.preventDefault()
      if ("stopImmediatePropagation" in event.nativeEvent) event.nativeEvent.stopImmediatePropagation()
      event.stopPropagation()
    })
    this.target.on("pointertap", (event: { global: { x: number; y: number } }) => this.pointerAt(event.global.y - this.target.y))
    if (typeof window !== "undefined") window.addEventListener("keydown", this.keydown)
    this.target.visible = false
  }

  /** Shows a menu and resolves with the selected option id. */
  show = (
    menu: Contract.MenuAwait,
    remaining: Stream.Stream<number>
  ): Effect.Effect<string> =>
    (() => {
      const self = this
      return Effect.gen(function* () {
        yield* self.hide
        self.target.visible = true
        const options: ChoiceOption[] = menu.options.map((option) => ({ id: option.id, verdict: option.verdict }))
        self.navigation = choiceNavigation(options)
        self.prompt.text = menu.prompt ?? ""
        self.renderOptions(options)
        self.deferred = yield* Deferred.make<string>()
        const deferred = self.deferred
        self.resolveChoice = (id: string): void => {
          if (self.deferred === deferred) Effect.runSync(Deferred.succeed(deferred, id))
        }
        yield* Stream.runForEach(remaining, (seconds) =>
          Effect.sync(() => {
            if (self.maxRemaining === 0) self.maxRemaining = Math.max(0, seconds)
            self.remainingValue = Math.max(0, seconds)
            self.drawCountdown()
          })
        ).pipe(Effect.forkDetach)
        return yield* Deferred.await(deferred)
      }).pipe(Effect.ensuring(self.hide))
    })()

  /** Hides the menu and removes all option children. */
  readonly hide: Effect.Effect<void> = Effect.sync(() => {
    this.target.visible = false
    this.resolveChoice = undefined
    this.deferred = undefined
    this.maxRemaining = 0
    this.remainingValue = 0
    for (const item of this.rendered) {
      item.button.removeFromParent()
      item.button.visible = false
      item.label.removeFromParent()
      item.label.visible = false
    }
    this.rendered.length = 0
    this.optionsLayer.removeChildren()
  })

  /** Updates the countdown from a test or host without consuming a stream. */
  readonly setRemaining = (seconds: number): void => {
    if (this.maxRemaining === 0) this.maxRemaining = Math.max(0, seconds)
    this.remainingValue = Math.max(0, seconds)
    this.drawCountdown()
  }

  /** Advances the countdown pulse by seconds; called by a ticker if desired. */
  readonly tick = (dt: number): void => {
    this.pulsePhase += Math.max(0, dt)
    if (this.remainingValue < 3) this.drawCountdown()
  }

  private readonly keydown = (event: KeyboardEvent): void => {
    if (!this.target.visible) return
    const key = event.key === "ArrowUp" ? "up" : event.key === "ArrowDown" ? "down" : event.key === "Enter" ? "enter" : event.key
    const result = navigateChoice(this.navigation, key)
    if (typeof result === "string") this.resolveChoice?.(result)
    else if (result !== undefined) {
      this.navigation = result
      this.updateFocus()
    }
  }

  private pointerAt = (y: number): void => {
    for (const [index, item] of this.rendered.entries()) {
      if (y >= item.button.y && y <= item.button.y + item.button.height && this.navigation.enabled[index] === true) {
        this.resolveChoice?.(item.option.id)
        return
      }
    }
  }

  private renderOptions = (options: ReadonlyArray<ChoiceOption>): void => {
    const width = 680
    this.panel.clear().roundRect(0, 0, width, 460, 12).fill({ color: this.theme.panel, alpha: this.theme.panelAlpha })
    this.prompt.x = 24
    this.prompt.y = 20
    options.forEach((option, index) => {
      const button = new Graphics()
      const label = new Text({
        text: hasLabel(option) ? option.label : option.id,
        style: new TextStyle({ fontFamily: this.theme.font, fontSize: 22, fill: this.theme.text })
      })
      button.y = 70 + index * 48
      button.roundRect(20, 0, width - 40, 38, 6).fill({
        color: this.theme.accent,
        alpha: this.navigation.enabled[index] === true ? 0.35 : 0.25
      })
      label.x = 32
      label.y = button.y + 6
      button.eventMode = this.navigation.enabled[index] === true ? "static" : "none"
      button.on("pointertap", () => {
        if (this.navigation.enabled[index] === true) this.resolveChoice?.(option.id)
      })
      this.optionsLayer.addChild(button, label)
      this.rendered.push({ option, button, label })
    })
    this.updateFocus()
    this.drawCountdown()
  }

  private updateFocus = (): void => {
    for (const [index, item] of this.rendered.entries()) {
      const active = index === this.navigation.focused
      item.button.alpha = active ? 1 : 0.8
    }
  }

  private drawCountdown = (): void => {
    const ratio = this.maxRemaining <= 0 ? 0 : Math.min(1, this.remainingValue / this.maxRemaining)
    const pulse = this.remainingValue < 3 ? 0.7 + Math.sin(this.pulsePhase * 8) * 0.3 : 1
    this.countdown.alpha = pulse
    this.countdown.clear().rect(20, 450, 640 * ratio, 6).fill({ color: this.theme.accent, alpha: pulse })
  }
}
