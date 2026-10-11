import { Context, Effect, Layer, Queue, Stream } from "effect"
import { Container } from "pixi.js"
import type { PlayerAction, ScreenModel } from "@lute-lang/player"
import type { WorldShape, Theme } from "../engine/types.js"
import { BacklogScreen, HudScreen } from "./screens/backlog.js"
import { ConfirmScreen, EndedScreen, HaltedScreen, IdleScreen, LoadingScreen, MenuScreen, TitleScreen } from "./screens/common.js"
import { SaveLoadScreen } from "./screens/save-load.js"
import { SettingsScreen } from "./screens/settings.js"
import { ScreenView, type EmitAction } from "./screens/common.js"

/** UI renderer service contract. */
export interface UiShape {
  readonly show: (screen: ScreenModel) => Effect.Effect<void>
  readonly hide: Effect.Effect<void>
  readonly actions: Stream.Stream<PlayerAction>
  readonly setTheme: (theme: Theme) => void
}

/** UI context service key. */
export const UiTag = Context.Service<UiShape>("Lute.Web.Ui")

/** Screen renderer that owns no game state and emits only player actions. */
export class Ui implements UiShape {
  readonly actions: Stream.Stream<PlayerAction>
  private readonly root: Container
  private readonly world: WorldShape | undefined
  private theme: Theme
  private current: ScreenView | HudScreen | undefined
  private push: (action: PlayerAction) => void = () => {}

  constructor(root: Container, theme: Theme, world?: WorldShape) {
    this.root = root
    this.theme = theme
    this.world = world
    this.actions = Stream.callback<PlayerAction>((queue) => Effect.sync(() => {
      this.push = (action) => { Queue.offerUnsafe(queue, action) }
    }))
  }

  /** Render a new screen model, replacing the previous screen. */
  show(screen: ScreenModel): Effect.Effect<void> {
    const self = this
    return Effect.gen(function* () {
      const next = self.build(screen)
      const previous = self.current
      if (previous !== undefined) previous.removeFromParent()
      self.current = next
      next.alpha = 0
      self.root.addChild(next)
      if (self.world !== undefined) yield* self.world.run(next, fadeBehaviour(next))
      else next.alpha = 1
    })
  }

  /** Hide the currently rendered screen. */
  readonly hide: Effect.Effect<void> = Effect.sync(() => {
    this.current?.removeFromParent()
    this.current = undefined
  })

  /** Replace theme for future screens. */
  setTheme(theme: Theme): void { this.theme = theme }

  /** Render a HUD model over the game scene. */
  showHud(state: { readonly auto: boolean; readonly skip: boolean; readonly hidden?: boolean }): void {
    this.current?.removeFromParent()
    const hud = new HudScreen(this.theme, this.push, state)
    this.current = hud
    this.root.addChild(hud)
  }

  /** Handle keyboard focus in the current screen. */
  handleKey(key: string): boolean { return this.current instanceof ScreenView && this.current.handleKey(key) }

  private build(screen: ScreenModel): ScreenView {
    const emit: EmitAction = (action) => this.push(action)
    switch (screen._tag) {
      case "Title": return new TitleScreen(screen, this.theme, emit)
      case "Backlog": return new BacklogScreen(screen, this.theme, emit)
      case "SaveLoad": return new SaveLoadScreen(screen, this.theme, emit)
      case "Settings": return new SettingsScreen(screen, this.theme, emit)
      case "Confirm": return new ConfirmScreen(screen, this.theme, emit)
      case "Loading": return new LoadingScreen(screen, this.theme, emit)
      case "Halted": return new HaltedScreen(screen, this.theme, emit)
      case "Idle": return new IdleScreen(screen, this.theme, emit)
      case "Menu": return new MenuScreen(this.theme, emit)
      case "Ended": return new EndedScreen(screen, this.theme, emit)
    }
  }
}

/** Construct a UI layer rooted in a Pixi container. */
export function uiLayer(root: Container, theme: Theme, world?: WorldShape): Layer.Layer<UiShape> {
  return Layer.succeed(UiTag, new Ui(root, theme, world))
}

function fadeBehaviour(target: Container): { readonly update: (dt: number) => "running" | "done"; readonly finish: () => void; readonly dispose: () => void } {
  let elapsed = 0
  return {
    update: (dt) => {
      elapsed += dt
      target.alpha = Math.min(1, elapsed / 0.2)
      return elapsed >= 0.2 ? "done" : "running"
    },
    finish: () => { target.alpha = 1 },
    dispose: () => {}
  }
}
