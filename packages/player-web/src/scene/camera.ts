import { Effect } from "effect"
import type { Bindings, EffectSpec } from "@lute-lang/player"
import type { Container } from "pixi.js"
import { EffectCatalogue } from "../effects/catalogue.js"
import type { WorldShape } from "../engine/types.js"

export interface CameraPlacement { readonly scale: number; readonly x: number; readonly y: number }

/** Compute camera transform around the design-space centre. */
export const framingPlacement = (
  framing: { readonly zoom: number; readonly x?: number | null; readonly y?: number | null } | undefined,
  design: { readonly width: number; readonly height: number },
  focus: { readonly x: number; readonly y: number } | undefined
): CameraPlacement => {
  const zoom = framing?.zoom ?? 1
  const centreX = design.width / 2
  const centreY = design.height / 2
  const x = focus === undefined ? centreX + (framing?.x ?? 0) : centreX + (centreX - focus.x) * zoom + (framing?.x ?? 0)
  const y = focus === undefined ? centreY + (framing?.y ?? 0) : centreY + (centreY - focus.y) * zoom + (framing?.y ?? 0)
  return { scale: zoom, x, y }
}

export class Camera {
  readonly target: Container
  private readonly world: WorldShape
  private readonly bindings: Bindings
  private readonly design: { readonly width: number; readonly height: number }
  private readonly actors: Container
  private readonly logger: (message: string) => void
  constructor(target: Container, actors: Container, world: WorldShape, bindings: Bindings, design: { readonly width: number; readonly height: number }, logger: (message: string) => void = console.warn) {
    this.target = target
    this.actors = actors
    this.world = world
    this.bindings = bindings
    this.design = design
    this.logger = logger
  }
  readonly clear = (): void => {
    this.target.scale.set(1)
    this.target.position.set(this.design.width / 2, this.design.height / 2)
    this.target.pivot.set(this.design.width / 2, this.design.height / 2)
  }
  readonly apply = (framingName: string | undefined, focusName: string | undefined): void => {
    const framing = framingName === undefined ? undefined : this.bindings.framing?.[framingName]
    const focus = focusName === undefined ? undefined : this.findActor(focusName)
    if (focusName !== undefined && focus === undefined) this.logger(`Missing actor focus:${focusName}`)
    const placement = framingPlacement(framing, this.design, focus)
    this.target.scale.set(placement.scale)
    this.target.position.set(placement.x, placement.y)
  }
  readonly move = (move: EffectSpec | undefined, duration: number | undefined, instant = false): Effect.Effect<void> => {
    if (move === undefined || instant) return Effect.void
    return EffectCatalogue.build(move.effect, this.target, move.params, duration ?? (typeof move.params.duration === "number" ? move.params.duration : 0), { design: this.design, camera: this.target }).pipe(
      Effect.catch(() => Effect.succeed([])),
      Effect.flatMap((behaviours) => Effect.forEach(behaviours, (behaviour) => this.world.run(this.target, behaviour), { discard: true }))
    )
  }
  private findActor = (character: string): { readonly x: number; readonly y: number } | undefined => {
    const actor = this.actors.children.find((child) => child.label === `Actor:${character}`)
    return actor === undefined ? undefined : { x: actor.x + actor.width / 2, y: actor.y + actor.height / 2 }
  }
}
