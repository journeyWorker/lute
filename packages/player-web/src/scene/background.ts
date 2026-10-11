import { Effect } from "effect"
import { Container, Sprite } from "pixi.js"
import type { EffectSpec, Stage } from "@lute-lang/player"
import type { Bindings } from "@lute-lang/player"
import { crossfade } from "../engine/behaviours.js"
import type { WorldShape, Theme } from "../engine/types.js"
import { assetFile, clearContainer, placeholder, type TextureAssets, textureFor, transitionDuration } from "./shared.js"

export const backgroundFile = (bindings: Bindings, bg: Stage["bg"]): string | undefined => {
  if (bg === null || bg === undefined) return undefined
  const bound = bg.location === undefined || bg.location === null ? undefined : bindings.backgrounds?.[bg.location]
  return bg.time !== undefined && bg.time !== null ? bound?.times?.[bg.time] ?? bound?.image ?? assetFile(bindings, bg.assetId ?? undefined) : bound?.image ?? assetFile(bindings, bg.assetId ?? undefined)
}

/** Full-design-resolution background with optional texture crossfade. */
export class Background {
  readonly target: Container
  private readonly world: WorldShape
  private readonly bindings: Bindings
  private readonly assets: TextureAssets
  private readonly theme: Theme
  private readonly design: { readonly width: number; readonly height: number }
  private readonly logger: (message: string) => void
  private readonly logged = new Set<string>()
  private current: Container | undefined

  constructor(parent: Container, world: WorldShape, bindings: Bindings, assets: TextureAssets, design: { readonly width: number; readonly height: number }, theme: Theme, logger: (message: string) => void = console.warn) {
    this.target = parent
    this.world = world
    this.bindings = bindings
    this.assets = assets
    this.design = design
    this.theme = theme
    this.logger = logger
    parent.label = parent.label || "BackgroundLayer"
  }

  readonly set = (bg: Stage["bg"], transition: EffectSpec | undefined, instant = false, durationOverride?: number): Effect.Effect<void> => {
    const file = backgroundFile(this.bindings, bg)
    const texture = textureFor(this.assets, file)
    if (file === undefined || texture === undefined) {
      const key = `background:${bg?.location ?? bg?.assetId ?? "missing"}`
      if (!this.logged.has(key)) { this.logged.add(key); this.logger(`Missing binding ${key}`) }
      clearContainer(this.target)
      const next = placeholder(this.target, key, this.theme, this.design.width, this.design.height)
      next.alpha = 1
      this.current = next
      return Effect.void
    }
    const next = new Container()
    next.label = `Background:${file}`
    const sprite = new Sprite(texture)
    sprite.width = this.design.width
    sprite.height = this.design.height
    next.addChild(sprite)
    this.target.addChild(next)
    const previous = this.current
    this.current = next
    const duration = instant ? 0 : transitionDuration(durationOverride, typeof transition?.params.duration === "number" ? transition.params.duration : 0)
    if (previous === undefined || duration <= 0) {
      if (previous !== undefined) previous.destroy({ children: true })
      next.alpha = 1
      return Effect.void
    }
    const behaviour = crossfade(previous, next, duration)
    return this.world.run(this.target, behaviour).pipe(Effect.tap(() => Effect.sync(() => previous.destroy({ children: true }))))
  }
}
