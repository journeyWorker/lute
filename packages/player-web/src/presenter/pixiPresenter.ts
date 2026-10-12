import { Context, Effect, Layer, PubSub, Stream } from "effect"
import type { Bindings, ChoiceView, EffectSpec, LineView, PlayMode, PlayerAction, Presenter, ScreenModel, Settings, Stage, StageCue } from "@lute-lang/player"
import type { TextStyleBinding } from "../text/richText.js"
import type { PixiAppShape } from "../engine/types.js"
import type { WorldShape } from "../engine/types.js"
import { PixiApp } from "../engine/app.js"
import { World } from "../engine/world.js"
import { UiTag, type UiShape } from "../ui/ui.js"
import { ChoiceMenu } from "../choice/choiceMenu.js"
import { DialogueWindow } from "../text/dialogueWindow.js"
import { ActorSprite } from "../scene/actor.js"
import { Background } from "../scene/background.js"
import { Camera } from "../scene/camera.js"
import { CgLayer } from "../scene/cgLayer.js"
import { VideoLayer } from "../scene/videoLayer.js"
import { Vfx } from "../scene/vfx.js"
import { clearContainer, type TextureAssets } from "../scene/shared.js"
import type { Theme } from "../engine/types.js"

export interface PresenterOptions {
  readonly pixi: PixiAppShape
  readonly world: WorldShape
  readonly ui: UiShape
  readonly assets: TextureAssets
  readonly bindings: Bindings
  readonly theme: Theme
  readonly titleBackground?: string
  readonly logger?: (message: string, error?: unknown) => void
  /** Debug-level log; defaults to `console.debug`. Receives one line per custom vfx name. */
  readonly debug?: (message: string) => void
}

/** Pixi implementation of the renderer-free player Presenter port. */
/** Presenter service key. */
export const PixiPresenterTag = Context.Service<PixiPresenter>("Lute.Web.PixiPresenter")

/** Pixi implementation of the renderer-free player Presenter port. */
export class PixiPresenter implements Presenter {
  static readonly layer = (bindings: Bindings, theme: Theme, assets: TextureAssets) =>
    Layer.effect(PixiPresenterTag, Effect.gen(function* () {
      const pixi = yield* PixiApp
      const world = yield* World
      const ui = yield* UiTag
      return new PixiPresenter({ pixi, world, ui, assets, bindings, theme })
    }))

  readonly stage: Presenter["stage"]
  readonly showLine: Presenter["showLine"]
  readonly revealLine: Presenter["revealLine"]
  readonly clearLine: Presenter["clearLine"]
  readonly restore: Presenter["restore"]
  readonly showChoice: Presenter["showChoice"]
  readonly hideChoice: Presenter["hideChoice"]
  readonly showScreen: Presenter["showScreen"]
  readonly hideScreen: Presenter["hideScreen"]
  readonly setUiHidden: Presenter["setUiHidden"]
  readonly applySettings: Presenter["applySettings"]
  readonly capture: Presenter["capture"]
  readonly actions: Stream.Stream<PlayerAction>

  private readonly pixi: PixiAppShape
  private readonly world: WorldShape
  private readonly ui: UiShape
  private readonly assets: TextureAssets
  private readonly bindings: Bindings
  private readonly theme: Theme
  private readonly logger: (message: string, error?: unknown) => void
  private readonly debug: (message: string) => void
  private readonly actors = new Map<string, ActorSprite>()
  private readonly background: Background
  private readonly cg: CgLayer
  private readonly video: VideoLayer
  private readonly camera: Camera
  private readonly vfx: Vfx
  private readonly dialogue: DialogueWindow
  private readonly choice: ChoiceMenu
  private readonly choiceActions: PubSub.PubSub<PlayerAction>
  private readonly missing = new Set<string>()
  private readonly customVfx = new Set<string>()
  private uiHidden = false

  constructor(options: PresenterOptions) {
    this.pixi = options.pixi
    this.world = options.world
    this.ui = options.ui
    this.assets = options.assets
    this.bindings = options.bindings
    this.theme = options.theme
    this.logger = options.logger ?? ((message, error) => console.warn(message, error))
    this.debug = options.debug ?? ((message) => console.debug(message))
    const design = this.pixi.design
    this.background = new Background(this.pixi.layers.background, this.world, this.bindings, this.assets, design, this.theme, (message) => this.logMissing(message))
    this.cg = new CgLayer(this.pixi.layers.cg, this.world, this.bindings, this.assets, design, this.theme, (message) => this.logMissing(message))
    this.video = new VideoLayer(this.pixi.layers.video, this.world, this.bindings, this.assets, design, this.theme, (message) => this.logMissing(message))
    this.camera = new Camera(this.pixi.layers.camera, this.pixi.layers.actors, this.world, this.bindings, design, (message) => this.logMissing(message))
    this.vfx = new Vfx(this.pixi.layers.overlay, this.world, design)
    const nameColors: Record<string, number> = {}
    for (const [character, binding] of Object.entries(this.bindings.characters ?? {})) {
      if (binding.nameColor !== null && binding.nameColor !== undefined) {
        const text = binding.nameColor.replace(/^#/, "")
        if (/^[0-9a-f]{6}$/i.test(text)) nameColors[character] = Number.parseInt(text, 16)
      }
    }
    const textStyles = Object.fromEntries(Object.entries(this.bindings.textStyles ?? {}).map(([name, style]) => [name, {
      ...(style.fontWeight === null ? {} : { fontWeight: style.fontWeight as TextStyleBinding["fontWeight"] }),
      ...(style.fontStyle === null ? {} : { fontStyle: style.fontStyle as TextStyleBinding["fontStyle"] }),
      ...(style.color === null ? {} : { color: style.color }),
      ...(style.alpha === null ? {} : { alpha: style.alpha }),
      ...(style.scale === null ? {} : { scale: style.scale }),
      ...(style.letterSpacing === null ? {} : { letterSpacing: style.letterSpacing })
    }]))
    this.dialogue = new DialogueWindow(this.pixi.layers.dialogue, this.world, this.theme, { nameColors, textStyles })
    this.dialogue.relayout(design)
    this.choice = new ChoiceMenu(this.pixi.layers.choice, this.theme)
    this.choiceActions = Effect.runSync(PubSub.unbounded<PlayerAction>())
    this.actions = Stream.merge(this.ui.actions, Stream.fromPubSub(this.choiceActions))
    this.stage = (cue, mode) => this.stageCue(cue, mode)
    this.showLine = (line, mode) => this.dialogue.show(line, this.defaultSettings, mode)
    this.revealLine = this.dialogue.reveal
    this.clearLine = this.dialogue.clear
    this.restore = (stage) => this.restoreStage(stage)
    this.showChoice = (choice, remaining) => this.showChoiceInternal(choice, remaining)
    this.hideChoice = this.choice.hide
    this.showScreen = (screen) => {
      const modal = ["Menu", "Backlog", "SaveLoad", "Settings", "Confirm"].includes(screen._tag)
      const hideDialogue = modal
        ? Effect.void
        : Effect.andThen(this.dialogue.clear, Effect.sync(() => {
            this.dialogue.setHidden(true)
          }))
      return Effect.andThen(hideDialogue, screen._tag === "Title" && options.titleBackground !== undefined
        ? this.ui.show({ ...screen, background: options.titleBackground })
        : this.ui.show(screen))
    }
    this.hideScreen = Effect.andThen(this.ui.hide, Effect.sync(() => {
      this.dialogue.setHidden(this.uiHidden)
    }))
    this.setUiHidden = (hidden) => Effect.sync(() => {
      this.uiHidden = hidden
      this.dialogue.setHidden(hidden)
    })
    this.applySettings = (settings) => Effect.sync(() => {
      this.defaultSettings = { ...this.defaultSettings, ...settings }
      this.dialogue.setOpacity(this.defaultSettings.windowOpacity)
    })
    this.capture = Effect.tryPromise(() => this.pixi.app.renderer.extract.base64({ target: this.pixi.app.stage, format: "png", resolution: Math.min(320 / design.width, 180 / design.height) })).pipe(Effect.catch(() => Effect.succeed("data:image/png;base64,")))
  }

  private defaultSettings: Settings = { locale: "", textSpeed: 30, windowOpacity: 1, autoDelay: 1, readColour: true, skipMode: "all", rollbackDepth: 0, volumes: { master: 1, music: 1, sfx: 1, voice: 1 } }

  private readonly stageCue = (cue: StageCue, mode: PlayMode): Effect.Effect<void> => {
    const instant = mode === "skip"
    switch (cue._tag) {
      case "Background": return this.background.set(cue.bg, cue.transition, instant, cue.duration)
      case "ActorShow": return this.upsertActor(cue.actor, cue.action, instant, cue.duration)
      case "ActorExit": return this.exitActor(cue.character, cue.action, instant, cue.duration)
      case "Cg": return this.cg.show(cue.cg, cue.transition, instant, cue.duration)
      case "Video": return cue.video === null ? this.video.hide() : this.video.show(cue.video, cue.blocking, instant)
      case "Sequence": {
        const file = cue.video === undefined ? this.bindings.sequences?.[cue.name]?.video : cue.video
        const stageVideo = file === undefined ? null : { assetId: file }
        return stageVideo === null ? this.video.hide() : this.video.show(stageVideo, true, instant)
      }
      case "Camera": {
        return Effect.sync(() => this.camera.apply(cue.framing, cue.focus)).pipe(Effect.flatMap(() => this.camera.move(cue.move ?? cue.transition, cue.duration, instant)))
      }
      case "Vfx": return cue.effect.effect === "custom" ? this.logCustomVfx(cue.effect.name ?? "") : this.vfx.play(cue.effect, cue.duration, instant)
    }
  }

  private readonly upsertActor = (actor: Stage["actors"][number], action: EffectSpec | undefined, instant: boolean, duration?: number): Effect.Effect<void> => {
    const current = this.actors.get(actor.character)
    if (current === undefined) {
      const created = new ActorSprite(this.pixi.layers.actors, actor, this.world, this.bindings, this.assets, this.pixi.design, this.theme, (message) => this.logMissing(message))
      this.actors.set(actor.character, created)
      return created.play(action, instant, duration)
    }
    current.updateTexture(actor)
    return current.play(action, instant, duration)
  }

  private readonly exitActor = (character: string, action: EffectSpec | undefined, instant: boolean, duration?: number): Effect.Effect<void> => {
    const actor = this.actors.get(character)
    if (actor === undefined) return Effect.void
    this.actors.delete(character)
    return actor.play(action, instant, duration).pipe(Effect.tap(() => Effect.sync(() => actor.destroy())))
  }

  private readonly restoreStage = (stage: Stage): Effect.Effect<void> => Effect.sync(() => {
    clearContainer(this.pixi.layers.background)
    clearContainer(this.pixi.layers.cg)
    clearContainer(this.pixi.layers.video)
    clearContainer(this.pixi.layers.actors)
    this.actors.clear()
    this.camera.clear()
  }).pipe(
    Effect.flatMap(() => this.background.set(stage.bg, undefined, true)),
    Effect.flatMap(() => Effect.forEach(stage.actors, (actor) => this.upsertActor(actor, undefined, true), { discard: true })),
    Effect.flatMap(() => this.cg.show(stage.cg, undefined, true)),
    Effect.flatMap(() => this.video.show(stage.video, false, true))
  )

  private readonly showChoiceInternal = (choice: ChoiceView, remaining: Stream.Stream<number>): Effect.Effect<string> =>
    this.choice.show(choice, remaining).pipe(
      Effect.catch(() => Effect.succeed("")),
      Effect.tap((option) => Effect.sync(() => { Effect.runSync(PubSub.publish(this.choiceActions, { type: "choose", option })) }))
    )

  private readonly logMissing = (message: string): void => {
    if (this.missing.has(message)) return
    this.missing.add(message)
    this.logger(message)
  }

  /**
   * A custom vfx (spec 0.41.0 §7.7) has no catalogue effect here: the Pixi
   * presenter plays nothing and logs each name once at debug level.
   */
  private readonly logCustomVfx = (name: string): Effect.Effect<void> => Effect.sync(() => {
    if (this.customVfx.has(name)) return
    this.customVfx.add(name)
    this.debug(`Custom vfx "${name}" has no Pixi effect; nothing is played`)
  })
}

export const presenterLayer = (bindings: Bindings, theme: Theme, assets: TextureAssets) =>
  PixiPresenter.layer(bindings, theme, assets)
