/** Public renderer-free player facade. */
import { Context, Effect, Layer, Scope } from "effect"
import * as KeyValueStore from "effect/persistence/KeyValueStore"
import * as Runtime from "@lute-lang/runtime"
import type { Contract } from "@lute-lang/runtime"
import { decode } from "./bindings/load.js"
import type { Bindings } from "./bindings/schema.js"
import { make as makeCues } from "./cues/index.js"
import { make as makeScheduler } from "./scheduler/index.js"
import { make as makeDirector } from "./director/index.js"
import type { DirectorHandle, DirectorOptions } from "./director/index.js"
import type { PlayerSessionShape } from "./session/index.js"
import { PlayerSession, layer as sessionLayer } from "./session/index.js"
import type { AssetLoader, AudioPort, PlayerAction, PlayerHost, Presenter } from "./ports/index.js"
import type { ProfileStoreShape, SaveStoreShape } from "./stores/index.js"
import { profileLayer, SaveStore, ProfileStore, layer as saveLayer } from "./stores/index.js"

/** Options accepted by {@link LutePlayer.make}. */
export interface LutePlayerOptions {
  /** Parsed bindings or the contents of player.bindings.yaml. */
  readonly bindings: Bindings | string | unknown
  readonly presenter: Presenter
  readonly audio: AudioPort
  readonly assets: AssetLoader
  readonly host?: PlayerHost
  readonly bundle?: Runtime.Bundle.Bundle
  readonly seed?: Contract.Seed
  readonly project?: string
  /** A host may supply a fully assembled player session. */
  readonly session?: PlayerSessionShape
  readonly runtimeSession?: typeof Runtime.Session.Session.Service
  readonly runtimeSessionLayer?: Layer.Layer<Runtime.Session.Session, unknown, never>
  readonly profile?: ProfileStoreShape
  readonly saves?: SaveStoreShape
  readonly keyValueStore?: KeyValueStore.KeyValueStore
  readonly cueFactory?: DirectorOptions["cueFactory"]
  readonly scheduler?: DirectorOptions["scheduler"]
  readonly titleBackground?: string
}

export interface LutePlayerHandle extends DirectorHandle {}

const buildSession = (options: LutePlayerOptions, bindings: Bindings): Effect.Effect<PlayerSessionShape, unknown, Scope.Scope> => {
  if (options.session !== undefined) return Effect.succeed(options.session)
  const runtimeLayer = options.runtimeSession === undefined
    ? options.runtimeSessionLayer
    : Layer.succeed(Runtime.Session.Session, options.runtimeSession)
  if (runtimeLayer === undefined) return Effect.fail(new Error("LutePlayer requires session or runtimeSession"))
  const sessionOptions = options.seed === undefined ? { bindings } : { bindings, seed: options.seed }
  return Layer.build(sessionLayer(sessionOptions)).pipe(
    Effect.provide(runtimeLayer),
    Effect.map((context) => Context.get(context, PlayerSession))
  )
}

const buildProfile = (options: LutePlayerOptions, project: string): Effect.Effect<ProfileStoreShape, unknown, KeyValueStore.KeyValueStore | Scope.Scope> => {
  if (options.profile !== undefined) return Effect.succeed(options.profile)
  return Effect.gen(function* () {
    const store = options.keyValueStore ?? (yield* KeyValueStore.KeyValueStore)
    const context = yield* Layer.build(profileLayer({ project }).pipe(
      Layer.provide(Layer.succeed(KeyValueStore.KeyValueStore, store))
    ))
    return Context.get(context, ProfileStore)
  })
}

const buildSaves = (options: LutePlayerOptions, project: string): Effect.Effect<SaveStoreShape, unknown, KeyValueStore.KeyValueStore | Scope.Scope> => {
  if (options.saves !== undefined) return Effect.succeed(options.saves)
  return Effect.gen(function* () {
    const store = options.keyValueStore ?? (yield* KeyValueStore.KeyValueStore)
    const context = yield* Layer.build(saveLayer({ project }).pipe(
      Layer.provide(Layer.succeed(KeyValueStore.KeyValueStore, store))
    ))
    return Context.get(context, SaveStore)
  })
}


/** Construct a scoped player handle. */
export const make = Effect.fn("Lute.Player.make")(function* (options: LutePlayerOptions) {
  const bindings = yield* decode(options.bindings)
  const session = yield* buildSession(options, bindings)
  const project = options.project ?? session.fingerprint
  const profile = yield* buildProfile(options, project)
  const saves = yield* buildSaves(options, project)
  const cueFactory = options.cueFactory ?? (options.bundle === undefined ? undefined : makeCues({ bundle: options.bundle, bindings }))
  if (cueFactory === undefined) return yield* Effect.fail(new Error("LutePlayer requires bundle or cueFactory"))
  const scheduler = options.scheduler ?? (yield* makeScheduler())
  const directorOptions: DirectorOptions = {
    bindings,
    session,
    presenter: options.presenter,
    audio: options.audio,
    assets: options.assets,
    profile,
    saves,
    cueFactory,
    scheduler,
    ...(options.host === undefined ? {} : { host: options.host }),
    ...(options.seed === undefined ? {} : { seed: options.seed }),
    ...(options.titleBackground === undefined ? {} : { titleBackground: options.titleBackground })
  }
  return yield* makeDirector(directorOptions)
})

/** Namespace-style facade used by hosts (`LutePlayer.make(options)`). */
export const LutePlayer = { make }
