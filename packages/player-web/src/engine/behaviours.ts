import { Container, Graphics } from "pixi.js"
import type { Behaviour } from "./types.js"
import type { Easing } from "./easing.js"
import { linear, quadOut } from "./easing.js"

/** Properties supported by a tween. */
export interface TweenProps {
  readonly x?: number
  readonly y?: number
  readonly alpha?: number
  readonly scaleX?: number
  readonly scaleY?: number
  readonly rotation?: number
  readonly tint?: number
}

/** Wipe direction. */
export type WipeDirection = "left" | "right" | "up" | "down"

type MutableTweenProps = {
  x?: number
  y?: number
  alpha?: number
  scaleX?: number
  scaleY?: number
  rotation?: number
  tint?: number
}
const pool: TweenBehaviour[] = []

const applyTween = (target: Container, props: TweenProps, starts: TweenProps, to: TweenProps, p: number): void => {
  if (to.x !== undefined && starts.x !== undefined) target.x = starts.x + (to.x - starts.x) * p
  if (to.y !== undefined && starts.y !== undefined) target.y = starts.y + (to.y - starts.y) * p
  if (to.alpha !== undefined && starts.alpha !== undefined) target.alpha = starts.alpha + (to.alpha - starts.alpha) * p
  if (to.scaleX !== undefined && starts.scaleX !== undefined) target.scale.x = starts.scaleX + (to.scaleX - starts.scaleX) * p
  if (to.scaleY !== undefined && starts.scaleY !== undefined) target.scale.y = starts.scaleY + (to.scaleY - starts.scaleY) * p
  if (to.rotation !== undefined && starts.rotation !== undefined) target.rotation = starts.rotation + (to.rotation - starts.rotation) * p
  if (to.tint !== undefined && starts.tint !== undefined) target.tint = Math.round(starts.tint + (to.tint - starts.tint) * p)
}

class TweenBehaviour implements Behaviour {
  target!: Container
  props!: TweenProps
  starts!: TweenProps
  to!: TweenProps
  duration = 0
  elapsed = 0
  easing: Easing = linear
  pooled = false
  update(dt: number): "running" | "done" {
    this.elapsed = Math.min(this.duration, this.elapsed + Math.max(0, dt))
    const p = this.duration <= 0 ? 1 : this.easing(this.elapsed / this.duration)
    applyTween(this.target, this.props, this.starts, this.to, p)
    return this.elapsed >= this.duration ? "done" : "running"
  }
  finish(): void {
    applyTween(this.target, this.props, this.starts, this.to, 1)
    this.elapsed = this.duration
  }
  dispose(): void {
    if (this.pooled) return
    this.pooled = true
    pool.push(this)
  }
}

/** Create a pooled property tween. */
export const tween = (
  target: Container,
  props: TweenProps,
  to: TweenProps,
  duration: number,
  easing: Easing = linear
): Behaviour => {
  const value = pool.pop() ?? new TweenBehaviour()
  value.target = target
  value.props = props
  const starts: MutableTweenProps = {}
  if (props.x !== undefined) starts.x = target.x
  if (props.y !== undefined) starts.y = target.y
  if (props.alpha !== undefined) starts.alpha = target.alpha
  if (props.scaleX !== undefined) starts.scaleX = target.scale.x
  if (props.scaleY !== undefined) starts.scaleY = target.scale.y
  if (props.rotation !== undefined) starts.rotation = target.rotation
  if (props.tint !== undefined) starts.tint = target.tint
  value.starts = starts
  value.to = to
  value.duration = Math.max(0, duration)
  value.elapsed = 0
  value.easing = easing
  value.pooled = false
  return value
}

/** Create a pooled alpha tween. */
export const fade = (target: Container, to: number, duration: number): Behaviour =>
  tween(target, { alpha: target.alpha }, { alpha: to }, duration, linear)

class CrossfadeBehaviour implements Behaviour {
  from!: Container
  to!: Container
  fromStart = 1
  toStart = 0
  duration = 0
  elapsed = 0
  pooled = false
  update(dt: number): "running" | "done" {
    this.elapsed = Math.min(this.duration, this.elapsed + Math.max(0, dt))
    const p = this.duration <= 0 ? 1 : this.elapsed / this.duration
    this.from.alpha = this.fromStart * (1 - p)
    this.to.alpha = this.toStart + (1 - this.toStart) * p
    return this.elapsed >= this.duration ? "done" : "running"
  }
  finish(): void {
    this.from.alpha = 0
    this.to.alpha = 1
    this.elapsed = this.duration
  }
  dispose(): void {
    if (this.pooled) return
    this.pooled = true
    crossfadePool.push(this)
  }
}
const crossfadePool: CrossfadeBehaviour[] = []
/** Crossfade two containers over a duration. */
export const crossfade = (from: Container, to: Container, duration: number): Behaviour => {
  const value = crossfadePool.pop() ?? new CrossfadeBehaviour()
  value.from = from
  value.to = to
  value.fromStart = from.alpha
  value.toStart = to.alpha
  value.duration = Math.max(0, duration)
  value.elapsed = 0
  value.pooled = false
  return value
}

class ShakeBehaviour implements Behaviour {
  target!: Container
  baseX = 0
  baseY = 0
  strength = 0
  duration = 0
  frequency = 0
  elapsed = 0
  pooled = false
  update(dt: number): "running" | "done" {
    this.elapsed = Math.min(this.duration, this.elapsed + Math.max(0, dt))
    if (this.elapsed >= this.duration) {
      this.target.x = this.baseX
      this.target.y = this.baseY
      return "done"
    }
    const fadeOut = 1 - this.elapsed / this.duration
    const angle = this.elapsed * this.frequency * Math.PI * 2
    this.target.x = this.baseX + Math.sin(angle * 1.13) * this.strength * fadeOut
    this.target.y = this.baseY + Math.cos(angle * 0.97) * this.strength * fadeOut
    return "running"
  }
  finish(): void {
    this.elapsed = this.duration
    this.target.x = this.baseX
    this.target.y = this.baseY
  }
  dispose(): void {
    if (this.pooled) return
    this.pooled = true
    shakePool.push(this)
  }
}
const shakePool: ShakeBehaviour[] = []
/** Shake a container around its original position. */
export const shake = (target: Container, strength: number, duration: number, frequency = 18): Behaviour => {
  const value = shakePool.pop() ?? new ShakeBehaviour()
  value.target = target
  value.baseX = target.x
  value.baseY = target.y
  value.strength = strength
  value.duration = Math.max(0, duration)
  value.frequency = Math.max(0, frequency)
  value.elapsed = 0
  value.pooled = false
  return value
}

class HopBehaviour implements Behaviour {
  target!: Container
  baseY = 0
  height = 0
  duration = 0
  elapsed = 0
  pooled = false
  update(dt: number): "running" | "done" {
    this.elapsed = Math.min(this.duration, this.elapsed + Math.max(0, dt))
    const p = this.duration <= 0 ? 1 : this.elapsed / this.duration
    this.target.y = this.baseY - Math.sin(Math.PI * p) * this.height
    return this.elapsed >= this.duration ? "done" : "running"
  }
  finish(): void {
    this.target.y = this.baseY
    this.elapsed = this.duration
  }
  dispose(): void {
    if (this.pooled) return
    this.pooled = true
    hopPool.push(this)
  }
}
const hopPool: HopBehaviour[] = []
/** Lift a container in a single hop and return it to its baseline. */
export const hop = (target: Container, height: number, duration: number): Behaviour => {
  const value = hopPool.pop() ?? new HopBehaviour()
  value.target = target
  value.baseY = target.y
  value.height = height
  value.duration = Math.max(0, duration)
  value.elapsed = 0
  value.pooled = false
  return value
}

/** Slide a container from one x coordinate to another. */
export const slide = (target: Container, fromX: number, toX: number, duration: number): Behaviour => {
  target.x = fromX
  return tween(target, { x: fromX }, { x: toX }, duration, quadOut)
}

/**
 * Run `inner`, which hides `target`, and keep `target` hidden (alpha 0) once
 * it ended, however it ended: a wipe drops its mask when it ends, so without
 * this the target would show until the next behaviour starts.
 */
export const concealing = (target: Container, inner: Behaviour): Behaviour => ({
  update: (dt) => inner.update(dt),
  finish: () => inner.finish(),
  dispose: () => {
    inner.dispose()
    target.alpha = 0
  }
})

class WipeBehaviour implements Behaviour {
  target!: Container
  maskGraphic!: Graphics
  direction: WipeDirection = "left"
  reveal = true
  width = 1
  height = 1
  duration = 0
  elapsed = 0
  pooled = false
  updateMask(progress: number): void {
    const p = this.reveal ? progress : 1 - progress
    const x = this.direction === "right" ? this.width * (1 - p) : 0
    const y = this.direction === "down" ? this.height * (1 - p) : 0
    const width = this.direction === "left" || this.direction === "right" ? this.width * p : this.width
    const height = this.direction === "up" || this.direction === "down" ? this.height * p : this.height
    this.maskGraphic.clear().rect(x, y, Math.max(0, width), Math.max(0, height)).fill(0xffffff)
  }
  update(dt: number): "running" | "done" {
    this.elapsed = Math.min(this.duration, this.elapsed + Math.max(0, dt))
    this.updateMask(this.duration <= 0 ? 1 : this.elapsed / this.duration)
    return this.elapsed >= this.duration ? "done" : "running"
  }
  finish(): void {
    this.elapsed = this.duration
    this.updateMask(1)
  }
  dispose(): void {
    if (this.pooled) return
    this.target.mask = null
    this.maskGraphic.clear()
    this.maskGraphic.removeFromParent()
    this.pooled = true
    wipePool.push(this)
  }
}
const wipePool: WipeBehaviour[] = []
/**
 * Reveal or conceal a target with a reusable Graphics mask. The mask covers
 * the target's own bounds; with `area` it covers that area from the origin
 * of the target's parent space instead (e.g. the design stage for the
 * camera layer, whose bounds are only the actors' span and move with the
 * framing).
 */
export const wipe = (
  target: Container,
  direction: WipeDirection,
  duration: number,
  reveal = true,
  area?: { readonly width: number; readonly height: number }
): Behaviour => {
  const value = wipePool.pop() ?? new WipeBehaviour()
  value.target = target
  value.maskGraphic = value.maskGraphic ?? new Graphics()
  value.direction = direction
  value.reveal = reveal
  value.width = Math.max(1, area?.width ?? target.width)
  value.height = Math.max(1, area?.height ?? target.height)
  if (area !== undefined) target.parent?.addChild(value.maskGraphic)
  value.duration = Math.max(0, duration)
  value.elapsed = 0
  value.pooled = false
  target.mask = value.maskGraphic
  value.updateMask(0)
  return value
}

class FlashBehaviour implements Behaviour {
  target!: Container
  colour = 0xffffff
  duration = 0
  elapsed = 0
  pooled = false
  update(dt: number): "running" | "done" {
    this.elapsed = Math.min(this.duration, this.elapsed + Math.max(0, dt))
    this.target.alpha = this.duration <= 0 ? 0 : 1 - this.elapsed / this.duration
    return this.elapsed >= this.duration ? "done" : "running"
  }
  finish(): void {
    this.elapsed = this.duration
    this.target.alpha = 0
  }
  dispose(): void {
    if (this.pooled) return
    this.pooled = true
    flashPool.push(this)
  }
}
const flashPool: FlashBehaviour[] = []
/** Flash an overlay container and then hide it. */
export const flash = (overlay: Container, colour: number, duration: number): Behaviour => {
  const value = flashPool.pop() ?? new FlashBehaviour()
  value.target = overlay
  value.colour = colour
  value.duration = Math.max(0, duration)
  value.elapsed = 0
  value.pooled = false
  overlay.tint = colour
  overlay.alpha = 1
  return value
}

class PulseBehaviour implements Behaviour {
  target!: Container
  baseX = 1
  baseY = 1
  scale = 1
  period = 1
  elapsed = 0
  pooled = false
  update(dt: number): "running" | "done" {
    this.elapsed += Math.max(0, dt)
    const wave = (Math.sin((this.elapsed / this.period) * Math.PI * 2) + 1) / 2
    this.target.scale.set(this.baseX * (1 + (this.scale - 1) * wave), this.baseY * (1 + (this.scale - 1) * wave))
    return "running"
  }
  finish(): void {
    this.target.scale.set(this.baseX, this.baseY)
  }
  dispose(): void {
    if (this.pooled) return
    this.pooled = true
    pulsePool.push(this)
  }
}
const pulsePool: PulseBehaviour[] = []
/** Pulse a target forever until cancelled. */
export const pulse = (target: Container, scale: number, period: number): Behaviour => {
  const value = pulsePool.pop() ?? new PulseBehaviour()
  value.target = target
  value.baseX = target.scale.x
  value.baseY = target.scale.y
  value.scale = scale
  value.period = Math.max(0.0001, period)
  value.elapsed = 0
  value.pooled = false
  return value
}

class DelayBehaviour implements Behaviour {
  duration = 0
  elapsed = 0
  pooled = false
  update(dt: number): "running" | "done" {
    this.elapsed = Math.min(this.duration, this.elapsed + Math.max(0, dt))
    return this.elapsed >= this.duration ? "done" : "running"
  }
  finish(): void {
    this.elapsed = this.duration
  }
  dispose(): void {
    if (this.pooled) return
    this.pooled = true
    delayPool.push(this)
  }
}
const delayPool: DelayBehaviour[] = []
/** Create a pooled timer behaviour. */
export const delay = (seconds: number): Behaviour => {
  const value = delayPool.pop() ?? new DelayBehaviour()
  value.duration = Math.max(0, seconds)
  value.elapsed = 0
  value.pooled = false
  return value
}
