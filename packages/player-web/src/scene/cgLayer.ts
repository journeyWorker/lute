import { Effect } from "effect"
import { Container, Sprite } from "pixi.js"
import type { Bindings, EffectSpec, Stage } from "@lute-lang/player"
import { crossfade } from "../engine/behaviours.js"
import type { WorldShape, Theme } from "../engine/types.js"
import { EffectCatalogue } from "../effects/catalogue.js"
import { assetFile, cgPlacement, clearContainer, placeholder, textureFor, type TextureAssets, transitionDuration } from "./shared.js"

export { cgPlacement }

export class CgLayer {
  readonly target: Container
  private readonly world: WorldShape
  private readonly bindings: Bindings
  private readonly assets: TextureAssets
  private readonly design: { readonly width: number; readonly height: number }
  private readonly theme: Theme
  private readonly logger: (message: string) => void
  private current: Container | undefined

  constructor(parent: Container, world: WorldShape, bindings: Bindings, assets: TextureAssets, design: { readonly width: number; readonly height: number }, theme: Theme, logger: (message: string) => void = console.warn) {
    this.target = parent
    this.world = world
    this.bindings = bindings
    this.assets = assets
    this.design = design
    this.theme = theme
    this.logger = logger
  }

  readonly show = (cg: Stage["cg"], transition: EffectSpec | undefined, instant = false, durationOverride?: number): Effect.Effect<void> => {
    if (cg === null || cg === undefined) {
      const old = this.current
      this.current = undefined
      return old === undefined ? Effect.void : Effect.sync(() => { clearContainer(this.target); old.destroy({ children: true }) })
    }
    const file = assetFile(this.bindings, cg.assetId)
    const texture = textureFor(this.assets, file)
    const next = new Container()
    next.label = `Cg:${cg.assetId}`
    if (texture === undefined) {
      this.logger(`Missing binding cg:${cg.assetId}`)
      placeholder(next, `cg:${cg.assetId}`, this.theme, this.design.width, this.design.height)
    } else {
      const config = cg.layout === null || cg.layout === undefined ? undefined : this.bindings.cgLayouts?.[cg.layout]
      const placement = cgPlacement(config?.fit ?? "contain", this.design, { width: texture.width, height: texture.height }, config?.scale ?? 1, config?.x, config?.y)
      const sprite = new Sprite(texture)
      sprite.position.set(placement.x, placement.y)
      sprite.scale.set(placement.scale)
      next.addChild(sprite)
    }
    this.target.addChild(next)
    const previous = this.current
    this.current = next
    const duration = instant ? 0 : transitionDuration(durationOverride, typeof transition?.params.duration === "number" ? transition.params.duration : 0)
    if (previous === undefined || duration <= 0) {
      previous?.destroy({ children: true })
      return Effect.void
    }
    return this.world.run(this.target, crossfade(previous, next, duration)).pipe(Effect.tap(() => Effect.sync(() => previous.destroy({ children: true }))))
  }
}
