import { Effect } from "effect"
import { Container, Graphics } from "pixi.js"
import type { EffectSpec } from "@lute-lang/player"
import { EffectCatalogue } from "../effects/catalogue.js"
import type { WorldShape } from "../engine/types.js"

export class Vfx {
  readonly target: Container
  private readonly world: WorldShape
  private readonly design: { readonly width: number; readonly height: number }
  constructor(parent: Container, world: WorldShape, design: { readonly width: number; readonly height: number }) {
    this.world = world
    this.design = design
    this.target = new Container()
    this.target.label = "Vfx"
    parent.addChild(this.target)
  }
  readonly play = (effect: EffectSpec, duration: number | undefined, instant = false): Effect.Effect<void> => {
    const target = new Container()
    target.label = `Vfx:${effect.effect}`
    this.target.addChild(target)
    if (effect.effect === "flash") target.addChild(new Graphics().rect(0, 0, this.design.width, this.design.height).fill(0xffffff))
    const seconds = duration ?? (typeof effect.params.duration === "number" ? effect.params.duration : 0)
    if (instant) return Effect.sync(() => { target.alpha = 0; target.destroy({ children: true }) })
    return EffectCatalogue.build(effect.effect, target, effect.params, seconds, { design: this.design, overlay: target }).pipe(
      Effect.catch(() => Effect.succeed([])),
      Effect.flatMap((behaviours) => Effect.forEach(behaviours, (behaviour) => this.world.run(target, behaviour), { discard: true })),
      Effect.tap(() => Effect.sync(() => { target.alpha = 0; target.destroy({ children: true }) }))
    )
  }
}
