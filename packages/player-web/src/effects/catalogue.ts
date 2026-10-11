import { Schema } from "effect"
import { BlurFilter, Container } from "pixi.js"
import { fade, flash, hop, shake, slide, tween, wipe } from "../engine/behaviours.js"
import type { WipeDirection } from "../engine/behaviours.js"
import type { Behaviour } from "../engine/types.js"
import { linear } from "../engine/easing.js"

/** Names accepted by the 0.40 player effect catalogue. */
export type EffectName =
  | "fade"
  | "dissolve"
  | "wipeLeft"
  | "wipeRight"
  | "wipeUp"
  | "wipeDown"
  | "slideLeft"
  | "slideRight"
  | "cut"
  | "fadeIn"
  | "fadeOut"
  | "slideIn"
  | "slideOut"
  | "hop"
  | "shake"
  | "none"
  | "zoom"
  | "pan"
  | "flash"
  | "blur"
  | "tint"

/** Values accepted in a binding effect parameter object. */
export type EffectParams = Readonly<Record<string, number | string | boolean>>

/** Optional scene context needed by camera and full-screen effects. */
export interface EffectContext {
  readonly design?: { readonly width: number; readonly height: number }
  readonly overlay?: Container
  readonly camera?: Container
}

/** An unknown catalogue effect. */
export class UnknownEffect extends Schema.TaggedError<UnknownEffect>()("UnknownEffect", {
  name: Schema.String
}) {}
/** A parameter not accepted by a catalogue effect. */
export class UnknownEffectParameter extends Schema.TaggedError<UnknownEffectParameter>()("UnknownEffectParameter", {
  effect: Schema.String,
  parameter: Schema.String
}) {}
/** A parameter value with the wrong schema type. */
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
const validNumber = (effect: EffectName, parameter: string, value: unknown): number => {
  if (typeof value !== "number") {
    throw new InvalidEffectParameter({ effect, parameter, message: "expected a number" })
  }
  return Schema.decodeUnknownSync(numberSchema)(value)
}
const validString = (effect: EffectName, parameter: string, value: unknown): string => {
  if (typeof value !== "string") {
    throw new InvalidEffectParameter({ effect, parameter, message: "expected a string" })
  }
  return Schema.decodeUnknownSync(stringSchema)(value)
}
const parse = (name: EffectName, params: EffectParams): void => {
  const allowed = accepted[name]
  for (const parameter of Object.keys(params)) {
    if (!allowed.includes(parameter)) throw new UnknownEffectParameter({ effect: name, parameter })
  }
  for (const [parameter, value] of Object.entries(params)) {
    if (parameter === "duration" || parameter === "strength" || parameter === "scale" || parameter === "x" || parameter === "y") {
      validNumber(name, parameter, value)
    } else if (parameter === "color") {
      validString(name, parameter, value)
    }
  }
}
const number = (effect: EffectName, params: EffectParams, parameter: string, fallback: number): number => {
  const value = params[parameter]
  return value === undefined ? fallback : validNumber(effect, parameter, value)
}
const colour = (effect: EffectName, params: EffectParams): number => {
  const source = validString(effect, "color", params.color ?? "#ffffff").trim()
  const text = source.startsWith("#") ? source.slice(1) : source.startsWith("0x") ? source.slice(2) : source
  if (!/^[0-9a-fA-F]{6}$/.test(text)) {
    throw new InvalidEffectParameter({ effect, parameter: "color", message: "expected #RRGGBB" })
  }
  return Number.parseInt(text, 16)
}
const dimension = (target: Container, ctx: EffectContext): number => Math.max(1, ctx.design?.width ?? target.width, ctx.design?.height ?? target.height)

class BlurBehaviour implements Behaviour {
  readonly filter: BlurFilter
  target: Container
  start = 0
  end = 0
  duration = 0
  elapsed = 0
  constructor(target: Container, strength: number, duration: number) {
    this.target = target
    this.filter = new BlurFilter({ strength: 0 })
    this.start = 0
    this.end = strength
    this.duration = Math.max(0, duration)
    this.target.filters = [this.filter]
  }
  update(dt: number): "running" | "done" {
    this.elapsed = Math.min(this.duration, this.elapsed + Math.max(0, dt))
    const progress = this.duration <= 0 ? 1 : this.elapsed / this.duration
    this.filter.strength = this.start + (this.end - this.start) * progress
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
const parseName = (name: string): EffectName => {
  switch (name) {
    case "fade":
    case "dissolve":
    case "wipeLeft":
    case "wipeRight":
    case "wipeUp":
    case "wipeDown":
    case "slideLeft":
    case "slideRight":
    case "cut":
    case "fadeIn":
    case "fadeOut":
    case "slideIn":
    case "slideOut":
    case "hop":
    case "shake":
    case "none":
    case "zoom":
    case "pan":
    case "flash":
    case "blur":
    case "tint":
      return name
    default:
      throw new UnknownEffect({ name })
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
/** Build the behaviours for one catalogue effect. Invalid names and parameters throw tagged errors. */
export const build = (
  name: string,
  target: Container,
  params: EffectParams = {},
  duration = 0,
  ctx: EffectContext = {}
): ReadonlyArray<Behaviour> => {
  const effect = parseName(name)
  parse(effect, params)
  const seconds = Math.max(0, duration)
  switch (effect) {
    case "fade":
    case "dissolve":
      return [fade(target, 0, seconds)]
    case "wipeLeft":
    case "wipeRight":
    case "wipeUp":
    case "wipeDown":
      return [wipe(target, wipeDirection(effect), seconds, true)]
    case "slideLeft":
      return [slide(target, target.x + dimension(target, ctx), target.x, seconds)]
    case "slideRight":
      return [slide(target, target.x - dimension(target, ctx), target.x, seconds)]
    case "cut":
    case "none":
      return []
    case "fadeIn":
      return [fade(target, 1, seconds)]
    case "fadeOut":
      return [fade(target, 0, seconds)]
    case "slideIn":
      return [slide(target, target.x + dimension(target, ctx), target.x, seconds)]
    case "slideOut":
      return [slide(target, target.x, target.x - dimension(target, ctx), seconds)]
    case "hop":
      return [hop(target, dimension(target, ctx) * 0.08, seconds)]
    case "shake":
      return [shake(target, number(effect, params, "strength", 12), seconds)]
    case "zoom": {
      const scale = number(effect, params, "scale", 1)
      return [tween(target, { scaleX: target.scale.x, scaleY: target.scale.y }, { scaleX: scale, scaleY: scale }, seconds, linear)]
    }
    case "pan": {
      const toX = number(effect, params, "x", target.x)
      const toY = number(effect, params, "y", target.y)
      return [tween(target, { x: target.x, y: target.y }, { x: toX, y: toY }, seconds, linear)]
    }
    case "flash":
      return [flash(ctx.overlay ?? target, colour(effect, params), seconds)]
    case "blur":
      return [new BlurBehaviour(target, number(effect, params, "strength", 8), seconds)]
    case "tint":
      return [tween(target, { tint: target.tint }, { tint: colour(effect, params) }, seconds, linear)]
  }
}

/** Catalogue facade for callers that prefer an object service. */
export const EffectCatalogue = { build }
