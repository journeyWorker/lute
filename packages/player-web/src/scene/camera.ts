import { Effect } from "effect"
import type { Bindings, EffectSpec } from "@lute-lang/player"
import { Container } from "pixi.js"
import { EffectCatalogue, type HidingTransition } from "../effects/catalogue.js"
import type { Behaviour, WorldShape } from "../engine/types.js"

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

/** A `::camera` cue as the presenter stages it. */
export interface CameraCue {
  readonly framing: string | undefined
  readonly focus: string | undefined
  readonly move: EffectSpec | undefined
  readonly transition: EffectSpec | undefined
  readonly duration: number | undefined
}

/**
 * The camera layer (the actors). At rest it has alpha 1 and no mask; a cue
 * with a transition ends there too, with the new framing (spec 0.47.1 §4).
 */
export class Camera {
  readonly target: Container
  private readonly world: WorldShape
  private readonly bindings: Bindings
  private readonly design: { readonly width: number; readonly height: number }
  private readonly actors: Container
  private readonly logger: (message: string) => void
  /**
   * The world key camera transitions run under (their behaviours act on
   * `target`), so a later transition cancels them without cancelling a
   * camera move.
   */
  private readonly transitions = new Container()
  /** Bumped by every transition cue and restore: a superseded transition stops. */
  private generation = 0
  constructor(target: Container, actors: Container, world: WorldShape, bindings: Bindings, design: { readonly width: number; readonly height: number }, logger: (message: string) => void = console.warn) {
    this.target = target
    this.actors = actors
    this.world = world
    this.bindings = bindings
    this.design = design
    this.logger = logger
  }

  /**
   * Restore the camera for a restored stage (load, rollback): cancel running
   * camera behaviours, rest at alpha 1 with no mask, then the default framing.
   */
  readonly restore: Effect.Effect<void> = Effect.suspend(() => {
    this.generation += 1
    return Effect.andThen(Effect.andThen(this.world.cancel(this.transitions), this.world.cancel(this.target)), Effect.sync(() => {
      this.rest()
      this.target.scale.set(1)
      this.target.position.set(this.design.width / 2, this.design.height / 2)
      this.target.pivot.set(this.design.width / 2, this.design.height / 2)
    }))
  })

  readonly apply = (framingName: string | undefined, focusName: string | undefined): void => {
    const framing = framingName === undefined ? undefined : this.bindings.framing?.[framingName]
    const focus = focusName === undefined ? undefined : this.findActor(focusName)
    if (focusName !== undefined && focus === undefined) this.logger(`Missing actor focus:${focusName}`)
    const placement = framingPlacement(framing, this.design, focus)
    this.target.scale.set(placement.scale)
    this.target.position.set(placement.x, placement.y)
  }

  /**
   * Stage a camera cue. A `move` keeps precedence: the framing is applied,
   * then the move plays. A transition with a hiding effect hides the layer
   * over the first half of its duration, applies the framing and shows the
   * layer over the second half; any other effect plays after the framing is
   * applied. Either way the cue ends at alpha 1, no mask, the new framing.
   * `cut`, `none`, a zero duration and an instant cue apply it at once.
   */
  readonly stage = (cue: CameraCue, instant: boolean): Effect.Effect<void> =>
    cue.move !== undefined
      ? Effect.andThen(Effect.sync(() => this.apply(cue.framing, cue.focus)), this.move(cue.move, cue.duration, instant))
      : this.transition(cue, cue.transition, instant)

  private readonly move = (move: EffectSpec, duration: number | undefined, instant: boolean): Effect.Effect<void> => {
    if (instant) return Effect.void
    return EffectCatalogue.build(move.effect, this.target, move.params, duration ?? (typeof move.params.duration === "number" ? move.params.duration : 0), { design: this.design, camera: this.target }).pipe(
      Effect.catch(() => Effect.succeed([])),
      Effect.flatMap((behaviours) => Effect.forEach(behaviours, (behaviour) => this.world.run(this.target, behaviour), { discard: true }))
    )
  }

  private readonly transition = (cue: CameraCue, transition: EffectSpec | undefined, instant: boolean): Effect.Effect<void> => Effect.suspend(() => {
    this.generation += 1
    const generation = this.generation
    const current = (): boolean => generation === this.generation
    const settle = Effect.sync(() => {
      if (!current()) return
      this.rest()
      this.apply(cue.framing, cue.focus)
    })
    // A transition still running from an earlier cue is superseded.
    const superseded = this.world.cancel(this.transitions)
    const seconds = transition === undefined ? 0 : Math.max(0, cue.duration ?? (typeof transition.params.duration === "number" ? transition.params.duration : 0))
    if (transition === undefined || instant || seconds <= 0 || transition.effect === "cut" || transition.effect === "none") return Effect.andThen(superseded, settle)
    const ctx = { design: this.design, camera: this.target }
    return Effect.andThen(superseded, EffectCatalogue.hidingTransition(transition.effect, this.target, transition.params, ctx)).pipe(
      Effect.flatMap((hiding) => hiding === undefined
        ? Effect.andThen(Effect.sync(() => this.apply(cue.framing, cue.focus)), EffectCatalogue.build(transition.effect, this.target, transition.params, seconds, ctx)).pipe(
            Effect.flatMap((behaviours) => Effect.forEach(behaviours, (behaviour) => this.run(behaviour), { discard: true }))
          )
        : this.hideAndShow(hiding, seconds / 2, cue, current)),
      Effect.catch(() => Effect.void),
      Effect.andThen(settle),
      // An interrupted cue (skip, load, a new batch) still ends at rest.
      Effect.onInterrupt(() => current() ? Effect.andThen(this.world.cancel(this.transitions), settle) : Effect.void)
    )
  })

  private readonly hideAndShow = (hiding: HidingTransition, half: number, cue: CameraCue, current: () => boolean): Effect.Effect<void> =>
    Effect.suspend(() => this.run(hiding.hide(half))).pipe(
      Effect.andThen(Effect.suspend(() => {
        if (!current()) return Effect.void
        this.apply(cue.framing, cue.focus)
        return this.run(hiding.show(half))
      }))
    )

  private readonly run = (behaviour: Behaviour): Effect.Effect<void> => this.world.run(this.transitions, behaviour)

  private readonly rest = (): void => {
    this.target.alpha = 1
    this.target.mask = null
  }

  private findActor = (character: string): { readonly x: number; readonly y: number } | undefined => {
    const actor = this.actors.children.find((child) => child.label === `Actor:${character}`)
    return actor === undefined ? undefined : { x: actor.x + actor.width / 2, y: actor.y + actor.height / 2 }
  }
}
