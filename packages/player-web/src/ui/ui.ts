import { Context, Effect, Layer, Queue, Stream } from "effect"
import { Container, Graphics, Texture } from "pixi.js"
import { Button } from "./widgets/index.js"
import type { PlayerAction, ScreenModel } from "@lute-lang/player"
import type { WorldShape, Theme } from "../engine/types.js"
import { BacklogScreen, HudScreen } from "./screens/backlog.js"
import { ConfirmScreen, EndedScreen, HaltedScreen, IdleScreen, LoadingScreen, MenuScreen, TitleScreen, ScreenView, type EmitAction } from "./screens/common.js"
import { SaveLoadScreen } from "./screens/save-load.js"
import { SettingsScreen } from "./screens/settings.js"

/** UI renderer service contract. */
export interface UiShape {
  readonly show: (screen: ScreenModel) => Effect.Effect<void>
  readonly hide: Effect.Effect<void>
  readonly actions: Stream.Stream<PlayerAction>
  readonly setTheme: (theme: Theme) => void
}

/** UI context service key. */
export const UiTag = Context.Service<UiShape>("Lute.Web.Ui")

/** Design-space screens, overlays and HUD; the application owns viewport scaling. */
export class Ui implements UiShape {
  readonly actions: Stream.Stream<PlayerAction>
  private readonly root: Container
  private theme: Theme
  private readonly textureFor: ((file: string) => Texture | undefined) | undefined
  private current: ScreenView | undefined
  private base: ScreenView | undefined
  private overlay: Container | undefined
  private hud: HudScreen | undefined
  private hudState = ""
  private design = { width: 1280, height: 720 }
  private titleName: string | undefined
  private push: (action: PlayerAction) => void = () => {}

  constructor(root: Container, theme: Theme, _world?: WorldShape, textureFor?: (file: string) => Texture | undefined) {
    this.root = root
    this.theme = theme
    this.textureFor = textureFor
    this.actions = Stream.callback<PlayerAction>((queue) => Effect.sync(() => {
      this.push = (action) => { Queue.offerUnsafe(queue, action) }
    }))
  }

  /** Replace a screen or the topmost modal without leaving old scene children. */
  show(screen: ScreenModel): Effect.Effect<void> {
    return Effect.sync(() => {
      const modal = ["Menu", "Backlog", "SaveLoad", "Settings", "Confirm"].includes(screen._tag)
      if (this.overlay) {
        this.overlay.removeFromParent()
        this.overlay.visible = false
        this.overlay = undefined
      }
      const next = this.build(screen)
      if (modal) {
        const overlay = new Container()
        const dim = new Graphics().rect(0, 0, this.design.width, this.design.height).fill({ color: 0x000000, alpha: 0.6 })
        dim.eventMode = "static"
        overlay.eventMode = "static"
        overlay.on("pointerdown", (event) => {
          if ("preventDefault" in event.nativeEvent) event.nativeEvent.preventDefault()
          if ("stopImmediatePropagation" in event.nativeEvent) event.nativeEvent.stopImmediatePropagation()
          event.stopPropagation()
        })
        overlay.on("pointerup", (event) => {
          if ("preventDefault" in event.nativeEvent) event.nativeEvent.preventDefault()
          if ("stopImmediatePropagation" in event.nativeEvent) event.nativeEvent.stopImmediatePropagation()
          event.stopPropagation()
          if (event.button === 2) this.push({ type: "close" })
        })
        next.layout(this.design, false)
        overlay.addChild(dim, next)
        this.root.addChild(overlay)
        this.overlay = overlay
      } else {
        if (this.base) {
          this.base.removeFromParent()
          this.base.visible = false
        }
        next.layout(this.design, true)
        this.root.addChild(next)
        this.base = next
      }
      this.current = next
      if (this.hud) this.hud.visible = !this.current
    })
  }

  /** Close a modal back to its base screen, or remove the full screen for gameplay. */
  readonly hide: Effect.Effect<void> = Effect.sync(() => {
    if (this.overlay) {
      this.overlay.removeFromParent()
      this.overlay.visible = false
      this.overlay = undefined
      this.current = this.base
    } else {
      if (this.base) {
        this.base.removeFromParent()
        this.base.visible = false
      }
      this.base = undefined
      this.current = undefined
    }
    if (this.hud) this.hud.visible = !this.current
  })

  /** Replace theme for future screens. */
  setTheme(theme: Theme): void { this.theme = theme }

  /** Configure the game's design coordinates and the bound title heading (`ui.title.text`; none when absent). */
  setDesign(design: { readonly width: number; readonly height: number }, title?: string): void {
    this.design = { width: Math.max(1, design.width), height: Math.max(1, design.height) }
    this.titleName = title
  }

  /** Standalone viewport layout; mounted UI inherits the application's letterbox. */
  resize(viewWidth: number, viewHeight: number): void {
    const scale = Math.min(viewWidth / this.design.width, viewHeight / this.design.height)
    this.root.scale.set(Math.max(0, scale))
    this.root.position.set((viewWidth - this.design.width * scale) / 2, (viewHeight - this.design.height * scale) / 2)
  }

  /** Render the quick menu independently from modal screen ownership. */
  showHud(state: { readonly auto: boolean; readonly skip: boolean; readonly hidden?: boolean }): void {
    if (this.hud) {
      this.hud.removeFromParent()
      this.hud.visible = false
    }
    this.hud = new HudScreen(this.theme, this.push, state)
    this.hud.position.set(Math.max(16, this.design.width - HudScreen.totalWidth - 20), this.design.height - 32)
    this.root.addChild(this.hud)
  }

  /** Synchronize gameplay chrome without rebuilding controls on every frame. */
  setPlaying(playing: boolean, state: { readonly auto: boolean; readonly skip: boolean; readonly hidden?: boolean }): void {
    if (playing && this.base) {
      this.base.removeFromParent()
      this.base.visible = false
      if (this.current === this.base) this.current = undefined
      this.base = undefined
    }
    const key = `${state.auto}:${state.skip}:${state.hidden}`
    if (playing && key !== this.hudState) { this.showHud(state); this.hudState = key }
    if (this.hud) this.hud.visible = playing && !state.hidden && !this.current
  }

  /** Whether a screen, rather than the story, owns keyboard focus. */
  get hasScreen(): boolean { return this.current !== undefined }
  /** Activate a screen button at viewport CSS coordinates before gameplay input. */
  pointerAction(clientX: number, clientY: number): boolean {
    if (!this.current) return false
    const visit = (node: Container): boolean => {
      const bounds = node.getBounds()
      if (node instanceof Button && node.isEnabled && clientX >= bounds.x && clientX <= bounds.x + bounds.width && clientY >= bounds.y && clientY <= bounds.y + bounds.height) {
        node.activate()
        return true
      }
      return node.children.some((child) => child instanceof Container && visit(child))
    }
    return visit(this.current)
  }

  /** Route modal closing and focus keys before gameplay input. */
  handleKey(key: string): boolean {
    if (this.overlay && key === "Escape") { this.push({ type: "close" }); return true }
    return this.current?.handleKey(key) ?? false
  }

  /** Detach display objects without leaving a stale frame or screen remnant. */
  private disposeLater(node: Container): void {
    node.removeFromParent()
    node.visible = false
  }

  private build(screen: ScreenModel): ScreenView {
    const emit: EmitAction = (action) => this.push(action)
    switch (screen._tag) {
      case "Title": return new TitleScreen(screen, this.theme, emit, this.titleName, this.textureFor)
      case "Backlog": return new BacklogScreen(screen, this.theme, emit)
      case "SaveLoad": return new SaveLoadScreen(screen, this.theme, emit)
      case "Settings": return new SettingsScreen(screen, this.theme, emit)
      case "Confirm": return new ConfirmScreen(screen, this.theme, emit)
      case "Loading": return new LoadingScreen(screen, this.theme, emit)
      case "Halted": return new HaltedScreen(screen, this.theme, emit)
      case "Idle": return new IdleScreen(screen, this.theme, emit)
      case "Menu": return new MenuScreen(this.theme, emit)
      case "Ended": return new EndedScreen(screen, this.theme, emit, this.textureFor)
    }
  }
}

/** Construct a renderer service on an existing design-space root. */
export function uiLayer(root: Container, theme: Theme, world?: WorldShape): Layer.Layer<UiShape> {
  return Layer.succeed(UiTag, new Ui(root, theme, world))
}
