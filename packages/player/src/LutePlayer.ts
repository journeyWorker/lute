/**
 * The public player facade: composes bindings, the runtime session, the
 * stores, the cue pipeline and the director over the host's ports
 * (docs/player/architecture.md).
 */
import { Context, Effect, Layer } from "effect"
import type * as KeyValueStore from "effect/persistence/KeyValueStore"
import * as Runtime from "@lute-lang/runtime"
import type { Contract } from "@lute-lang/runtime"
import { decode } from "./bindings/load.js"
import type { Bindings } from "./bindings/schema.js"
import { castNamesFrom, make as makeCues } from "./cues/index.js"
import { make as makeDirector } from "./director/index.js"
import type { DirectorHandle } from "./director/index.js"
import type { AssetLoader, AudioPort, PlayerHost, Presenter } from "./ports/index.js"
import { make as makeScheduler } from "./scheduler/index.js"
import { layer as sessionLayer, PlayerSession } from "./session/index.js"
import { layer as saveLayer, profileLayer, ProfileStore, SaveStore } from "./stores/index.js"

/** What a host supplies to {@link LutePlayer.make}. */
export interface LutePlayerOptions {
  /** Decoded bindings, or the text of `player.bindings.yaml`. */
  readonly bindings: Bindings | string
  readonly presenter: Presenter
  readonly audio: AudioPort
  readonly assets: AssetLoader
  readonly host?: PlayerHost
  /** The new game's seed (default: derived from the project). */
  readonly seed?: Contract.Seed
}

/** A running player: send actions, observe the view, shut down. */
export type LutePlayerHandle = DirectorHandle

/**
 * The services a host provides: the runtime and its bundle and input log
 * (`@lute-lang/runtime/bun` or `/browser` layers), and the key-value store
 * saves and the profile live in.
 */
export type LutePlayerRequirements =
  | Runtime.LuteRuntime.LuteRuntime
  | Runtime.BundleSource.BundleSource
  | Runtime.InputLog.InputLog
  | KeyValueStore.KeyValueStore

/** Start a player in the current scope; it boots to the title screen. */
export const make = Effect.fn("Lute.Player.make")(function* (options: LutePlayerOptions) {
  const bindings = yield* decode(options.bindings)
  const seed: Contract.Seed = options.seed ?? { derive: true }
  const bundle = yield* (yield* Runtime.BundleSource.BundleSource).load
  const runtimeSession = Runtime.Session.layer({ seed })
  const playerSession = Context.get(
    yield* Layer.build(sessionLayer({ bindings, seed, castNames: castNamesFrom(bundle.artifacts) }).pipe(Layer.provide(runtimeSession))),
    PlayerSession
  )
  const project = playerSession.fingerprint
  const stores = Layer.mergeAll(saveLayer({ project }), profileLayer({ project }))
  const built = yield* Layer.build(stores)
  return yield* makeDirector({
    bindings,
    seed,
    session: playerSession,
    presenter: options.presenter,
    audio: options.audio,
    assets: options.assets,
    profile: Context.get(built, ProfileStore),
    saves: Context.get(built, SaveStore),
    cueFactory: makeCues({ bundle, bindings }),
    scheduler: yield* makeScheduler(),
    ...(options.host === undefined ? {} : { host: options.host })
  })
})

/** Namespace-style facade: `LutePlayer.make(options)`. */
export const LutePlayer = { make }
