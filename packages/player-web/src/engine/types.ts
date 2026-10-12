/**
 * Shared engine contracts of the web player (docs/player/web-architecture.md):
 * the Pixi application service, the scene layers, and the entity–behaviour
 * model every module builds on.
 */
import type { Context, Deferred, Effect, Stream } from "effect"
import type { Application, Container } from "pixi.js"

/** Fixed scene layers, back to front. `camera` holds the actors and is what framing/moves transform. */
export interface Layers {
  readonly background: Container
  readonly cg: Container
  readonly camera: Container
  readonly actors: Container
  readonly video: Container
  readonly dialogue: Container
  readonly choice: Container
  readonly overlay: Container
  readonly transition: Container
}

/** The Pixi application as a scoped service (initialized with WebGL, destroyed on scope close). */
export interface PixiAppShape {
  readonly app: Application
  readonly layers: Layers
  /** Design resolution from `bindings.canvas`; the stage is letterboxed into the element. */
  readonly design: { readonly width: number; readonly height: number }
  /** Seconds elapsed per tick. */
  readonly frames: Stream.Stream<number>
}

export type PixiAppTag = Context.Service<PixiAppShape, PixiAppShape>

/**
 * A unit of per-frame behaviour attached to an entity (tween, fade, shake,
 * typewriter, …). `update` advances by `dt` seconds and reports whether it
 * is finished; `finish` jumps to the end state. `done` is completed by the
 * world when the behaviour ends either way.
 */
export interface Behaviour {
  readonly update: (dt: number) => "running" | "done"
  readonly finish: () => void
  /** Release resources (masks, filters) after the behaviour ended. */
  readonly dispose: () => void
}

/** Owns every running behaviour; stepped once per frame by the ticker. */
export interface WorldShape {
  /** Run `behaviour` on `target`; completes when it finished (or was finished by `finishAll`). */
  readonly run: (target: Container, behaviour: Behaviour) => Effect.Effect<void>
  /** Finish every behaviour on `target` (or all) at once. */
  readonly finishAll: (target?: Container) => Effect.Effect<void>
  /** Cancel behaviours on `target` without finishing them (e.g. superseded tweens). */
  readonly cancel: (target: Container) => Effect.Effect<void>
  /** Number of running behaviours (tests, debug overlay). */
  readonly size: Effect.Effect<number>
}

/** A running behaviour's completion signal, as stored by the world. */
export interface Running {
  readonly target: Container
  readonly behaviour: Behaviour
  readonly done: Deferred.Deferred<void>
}

/** UI theme derived from `bindings.ui` (colours as 0xRRGGBB numbers). */
export interface Theme {
  readonly font: string
  readonly text: number
  readonly accent: number
  readonly panel: number
  readonly panelAlpha: number
  readonly readText: number
}
