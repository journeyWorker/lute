import { Effect, Schema } from "effect"
import { BlurFilter, Container } from "pixi.js"
import { concealing, fade, flash, hop, shake, slide, tween, wipe } from "../engine/behaviours.js"
import type { WipeDirection } from "../engine/behaviours.js"
import type { Behaviour } from "../engine/types.js"
import { linear } from "../engine/easing.js"

/**
 * Names in the player effect catalogue. `custom` (a project vfx, spec 0.41.0
 * §7.7) is not one: the presenter handles it before reaching the catalogue.
 */
export type EffectName =
  | "fade" | "dissolve" | "wipeLeft" | "wipeRight" | "wipeUp" | "wipeDown" | "slideLeft" | "slideRight"
  | "cut" | "fadeIn" | "fadeOut" | "slideIn" | "slideOut" | "hop" | "shake" | "none" | "zoom" | "pan"
  | "flash" | "blur" | "tint"

/** Values accepted in a binding effect parameter object. */
export type EffectParams = Readonly<Record<string, number | string | boolean>>

/** Optional scene context needed by camera and full-screen effects. */
export interface EffectContext {
  readonly design?: { readonly width: number; readonly height: number }
  readonly overlay?: Container
  readonly camera?: Container
}

/** Typed decoding failure for an effect name, parameter name, or parameter value. */
export class EffectDecodeError extends Schema.TaggedError<EffectDecodeError>()("EffectDecodeError", {
  effect: Schema.String,
  parameter: Schema.String,
  message: Schema.String
}) {}

/** Compatibility error for callers that distinguish unknown names. */
export class UnknownEffect extends Schema.TaggedError<UnknownEffect>()("UnknownEffect", { name: Schema.String }) {}
/** Compatibility error for callers that distinguish unknown parameters. */
export class UnknownEffectParameter extends Schema.TaggedError<UnknownEffectParameter>()("UnknownEffectParameter", {
  effect: Schema.String,
  parameter: Schema.String
}) {}
/** Compatibility error for callers that distinguish malformed parameter values. */
export class InvalidEffectParameter extends Schema.TaggedError<InvalidEffectParameter>()("InvalidEffectParameter", {
  effect: Schema.String,
  parameter: Schema.String,
  message: Schema.String
}) {}

const accepted: Readonly<Record<EffectName, ReadonlyArray<string>>> = {
  fade: ["duration"], dissolve: ["duration"], wipeLeft: ["duration"], wipeRight: ["duration"],
  wipeUp: ["duration"], wipeDown: ["duration"], slideLeft: ["duration"], slideRight: ["duration"], cut: [],
  fadeIn: ["duration"], fadeOut: ["duration"], slideIn: ["duration"], slideOut: ["duration"], hop: ["duration"],
  shake: ["duration", "strength"], none: [], zoom: ["duration", "scale"], pan: ["duration", "x", "y"],
  flash: ["duration", "color"], blur: ["duration", "strength"], tint: ["duration", "color"]
}

const numberSchema = Schema.Number
const stringSchema = Schema.String
const decodeNumber = (effect: EffectName, parameter: string, value: unknown): Effect.Effect<number, EffectDecodeError> =>
  typeof value !== "number"
    ? Effect.fail(new EffectDecodeError({ effect, parameter, message: "expected a number" }))
    : Schema.decodeUnknownEffect(numberSchema)(value).pipe(
        Effect.mapError(() => new EffectDecodeError({ effect, parameter, message: "expected a number" }))
      )
const decodeString = (effect: EffectName, parameter: string, value: unknown): Effect.Effect<string, EffectDecodeError> =>
  typeof value !== "string"
    ? Effect.fail(new EffectDecodeError({ effect, parameter, message: "expected a string" }))
    : Schema.decodeUnknownEffect(stringSchema)(value).pipe(
        Effect.mapError(() => new EffectDecodeError({ effect, parameter, message: "expected a string" }))
      )
const parse = (name: EffectName, params: EffectParams): Effect.Effect<void, EffectDecodeError> =>
  Effect.gen(function* () {
    const allowed = accepted[name]
    for (const parameter of Object.keys(params)) {
      if (!allowed.includes(parameter)) {
        return yield* Effect.fail(new EffectDecodeError({ effect: name, parameter, message: "unknown parameter" }))
      }
    }
    for (const [parameter, value] of Object.entries(params)) {
      if (parameter === "duration" || parameter === "strength" || parameter === "scale" || parameter === "x" || parameter === "y") {
        yield* decodeNumber(name, parameter, value)
      } else if (parameter === "color") {
        yield* decodeString(name, parameter, value)
      }
    }
  })
const valueNumber = (
  effect: EffectName,
  params: EffectParams,
  parameter: string,
  fallback: number
): Effect.Effect<number, EffectDecodeError> => {
  const value = params[parameter]
  return value === undefined ? Effect.succeed(fallback) : decodeNumber(effect, parameter, value)
}
const colour = (effect: EffectName, params: EffectParams): Effect.Effect<number, EffectDecodeError> =>
  decodeString(effect, "color", params.color ?? "#ffffff").pipe(
    Effect.flatMap((source) => {
      const trimmed = source.trim()
      const text = trimmed.startsWith("#") ? trimmed.slice(1) : trimmed.startsWith("0x") ? trimmed.slice(2) : trimmed
      return /^[0-9a-fA-F]{6}$/.test(text)
        ? Effect.succeed(Number.parseInt(text, 16))
        : Effect.fail(new EffectDecodeError({ effect, parameter: "color", message: "expected #RRGGBB" }))
    })
  )
const dimension = (target: Container, ctx: EffectContext): number =>
  Math.max(1, ctx.design?.width ?? target.width, ctx.design?.height ?? target.height)

class BlurBehaviour implements Behaviour {
  readonly filter: BlurFilter
  target: Container
  end = 0
  duration = 0
  elapsed = 0
  constructor(target: Container, strength: number, duration: number) {
    this.target = target
    this.filter = new BlurFilter({ strength: 0 })
    this.end = strength
    this.duration = Math.max(0, duration)
    this.target.filters = [this.filter]
  }
  update(dt: number): "running" | "done" {
    this.elapsed = Math.min(this.duration, this.elapsed + Math.max(0, dt))
    const progress = this.duration <= 0 ? 1 : this.elapsed / this.duration
    this.filter.strength = this.end * progress
    return this.elapsed >= this.duration ? "done" : "running"
  }
  finish(): void {
    this.elapsed = this.duration
    this.filter.strength = this.end
  }
  dispose(): void {
    this.filter.strength = this.end
  }
}
const parseName = (name: string): Effect.Effect<EffectName, EffectDecodeError> => {
  switch (name) {
    case "fade": case "dissolve": case "wipeLeft": case "wipeRight": case "wipeUp": case "wipeDown":
    case "slideLeft": case "slideRight": case "cut": case "fadeIn": case "fadeOut": case "slideIn":
    case "slideOut": case "hop": case "shake": case "none": case "zoom": case "pan": case "flash":
    case "blur": case "tint":
      return Effect.succeed(name)
    default:
      return Effect.fail(new EffectDecodeError({ effect: name, parameter: "", message: "unknown effect" }))
  }
}
const wipeDirection = (name: EffectName): WipeDirection => {
  switch (name) {
    case "wipeLeft": return "left"
    case "wipeRight": return "right"
    case "wipeUp": return "up"
    case "wipeDown": return "down"
    default: return "left"
  }
}

/** Build behaviours for one catalogue effect, returning typed decoding failures. */
export const build = (
  name: string,
  target: Container,
  params: EffectParams = {},
  duration = 0,
  ctx: EffectContext = {}
): Effect.Effect<ReadonlyArray<Behaviour>, EffectDecodeError> =>
  Effect.gen(function* () {
    const effect = yield* parseName(name)
    yield* parse(effect, params)
    const seconds = Math.max(0, duration)
    switch (effect) {
      case "fade": case "dissolve": return [fade(target, 0, seconds)]
      case "wipeLeft": case "wipeRight": case "wipeUp": case "wipeDown":
        return [wipe(target, wipeDirection(effect), seconds, true)]
      case "slideLeft": return [slide(target, target.x + dimension(target, ctx), target.x, seconds)]
      case "slideRight": return [slide(target, target.x - dimension(target, ctx), target.x, seconds)]
      case "cut": case "none": return []
      case "fadeIn": return [fade(target, 1, seconds)]
      case "fadeOut": return [fade(target, 0, seconds)]
      case "slideIn": return [slide(target, target.x + dimension(target, ctx), target.x, seconds)]
      case "slideOut": return [slide(target, target.x, target.x - dimension(target, ctx), seconds)]
      case "hop": return [hop(target, dimension(target, ctx) * 0.08, seconds)]
      case "shake": return [shake(target, yield* valueNumber(effect, params, "strength", 12), seconds)]
      case "zoom": {
        const scale = yield* valueNumber(effect, params, "scale", 1)
        return [tween(target, { scaleX: target.scale.x, scaleY: target.scale.y }, { scaleX: scale, scaleY: scale }, seconds, linear)]
      }
      case "pan": {
        const toX = yield* valueNumber(effect, params, "x", target.x)
        const toY = yield* valueNumber(effect, params, "y", target.y)
        return [tween(target, { x: target.x, y: target.y }, { x: toX, y: toY }, seconds, linear)]
      }
      case "flash": return [flash(ctx.overlay ?? target, yield* colour(effect, params), seconds)]
      case "blur": return [new BlurBehaviour(target, yield* valueNumber(effect, params, "strength", 8), seconds)]
      case "tint": return [tween(target, { tint: target.tint }, { tint: yield* colour(effect, params) }, seconds, linear)]
    }
  })

/**
 * A hiding transition (spec 0.47.1 §4) split in its two halves: `hide`
 * conceals the target (which stays at alpha 0 once it ended), `show` brings
 * it back from wherever it is placed when `show` is called.
 */
export interface HidingTransition {
  readonly hide: (seconds: number) => Behaviour
  readonly show: (seconds: number) => Behaviour
}

/**
 * The hide and show halves of a hiding effect (`fade`, `dissolve`,
 * `fadeOut`, `wipe*`, `slideLeft`, `slideRight`, `slideOut`), or `undefined`
 * for any other effect; decoding failures as {@link build}.
 */
export const hidingTransition = (
  name: string,
  target: Container,
  params: EffectParams = {},
  ctx: EffectContext = {}
): Effect.Effect<HidingTransition | undefined, EffectDecodeError> =>
  Effect.gen(function* () {
    const effect = yield* parseName(name)
    yield* parse(effect, params)
    const shown = (behaviour: Behaviour): Behaviour => {
      target.alpha = 1
      return behaviour
    }
    switch (effect) {
      case "fade": case "dissolve": case "fadeOut":
        return {
          hide: (seconds) => concealing(target, fade(target, 0, seconds)),
          show: (seconds) => {
            target.alpha = 0
            return fade(target, 1, seconds)
          }
        }
      case "wipeLeft": case "wipeRight": case "wipeUp": case "wipeDown":
        return {
          hide: (seconds) => concealing(target, wipe(target, wipeDirection(effect), seconds, false)),
          show: (seconds) => shown(wipe(target, wipeDirection(effect), seconds, true))
        }
      case "slideLeft": case "slideOut":
        return {
          hide: (seconds) => concealing(target, slide(target, target.x, target.x - dimension(target, ctx), seconds)),
          show: (seconds) => shown(slide(target, target.x + dimension(target, ctx), target.x, seconds))
        }
      case "slideRight":
        return {
          hide: (seconds) => concealing(target, slide(target, target.x, target.x + dimension(target, ctx), seconds)),
          show: (seconds) => shown(slide(target, target.x - dimension(target, ctx), target.x, seconds))
        }
      default: return undefined
    }
  })

/** Catalogue facade for callers that prefer an object. */
export const EffectCatalogue = { build, hidingTransition }
