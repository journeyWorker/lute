import { Effect } from "effect"
import { Container, Sprite } from "pixi.js"
import type { Bindings, EffectSpec, Stage } from "@lute-lang/player"
import { EffectCatalogue } from "../effects/catalogue.js"
import type { Behaviour, WorldShape, Theme } from "../engine/types.js"
import { anchorPlacement, clearContainer, placeholder, spriteTexture, type TextureAssets } from "./shared.js"

/** Resolve the character sprite using costume then emotion fallbacks. */
export const resolveActorTexture = spriteTexture

export class ActorSprite {
  readonly target: Container
  readonly actor: Stage["actors"][number]
  private readonly world: WorldShape
  private readonly bindings: Bindings
  private readonly assets: TextureAssets
  private readonly design: { readonly width: number; readonly height: number }
  private readonly theme: Theme
  private readonly logger: (message: string) => void
  private readonly logged = new Set<string>()

  constructor(parent: Container, actor: Stage["actors"][number], world: WorldShape, bindings: Bindings, assets: TextureAssets, design: { readonly width: number; readonly height: number }, theme: Theme, logger: (message: string) => void = console.warn) {
    this.actor = actor
    this.world = world
    this.bindings = bindings
    this.assets = assets
    this.design = design
    this.theme = theme
    this.logger = logger
    this.target = new Container()
    this.target.label = `Actor:${actor.character}`
    parent.addChild(this.target)
    this.updateTexture(actor)
  }

  updateTexture(actor: Stage["actors"][number]): void {
    this.target.removeChildren().forEach((child) => child.destroy({ children: true }))
    const file = resolveActorTexture(this.bindings, actor)
    const texture = this.assets.texture(file ?? "")
    if (texture === undefined) {
      const key = `character:${actor.character}`
      if (!this.logged.has(key)) { this.logged.add(key); this.logger(`Missing binding ${key}`) }
      placeholder(this.target, key, this.theme, this.design.width / 3, this.design.height * 0.8)
    } else {
      const sprite = new Sprite(texture)
      const anchor = actor.anchor === null || actor.anchor === undefined ? undefined : this.bindings.anchors?.[actor.anchor]
      const heightScale = Math.min(1, this.design.height * 0.85 / Math.max(1, texture.height))
      const placement = anchorPlacement(
        anchor === undefined ? { x: 0.5, y: 1, scale: heightScale } : { ...anchor, scale: anchor.scale ?? heightScale },
        this.design,
        { width: texture.width, height: texture.height }
      )
      sprite.position.set(placement.x, placement.y)
      sprite.scale.set(placement.scale)
    }
  }

  readonly play = (action: EffectSpec | undefined, instant = false, durationOverride?: number): Effect.Effect<void> => {
    if (action === undefined || action.effect === "none" || action.effect === "cut" || instant) return Effect.void
    return EffectCatalogue.build(action.effect, this.target, action.params, durationOverride ?? (typeof action.params.duration === "number" ? action.params.duration : 0)).pipe(
      Effect.catch(() => Effect.succeed<ReadonlyArray<Behaviour>>([])),
      Effect.flatMap((behaviours) => Effect.forEach(behaviours, (behaviour) => this.world.run(this.target, behaviour), { discard: true }))
    )
  }

  destroy(): void { this.target.destroy({ children: true }) }
}

export const removeActor = (actor: ActorSprite, action: EffectSpec | undefined, instant = false): Effect.Effect<void> =>
  actor.play(action, instant).pipe(Effect.tap(() => Effect.sync(() => actor.destroy())))
