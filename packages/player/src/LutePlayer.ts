/**
 * The public player facade: composes bindings, the runtime session, the
 * stores, the cue pipeline and the director over the host's ports
 * (docs/player/architecture.md).
 */
import { Context, Effect, Layer } from "effect"
import type { Scope } from "effect"
import type * as KeyValueStore from "effect/persistence/KeyValueStore"
import * as Runtime from "@lute-lang/runtime"
import type { Contract } from "@lute-lang/runtime"
import { decode } from "./bindings/load.js"
import type { BindingsInvalid } from "./bindings/load.js"
import type { Bindings } from "./bindings/schema.js"
import { make as makeCues } from "./cues/index.js"
import { make as makeDirector } from "./director/index.js"
import type { Autosave, DirectorHandle, LinePacing } from "./director/index.js"
import type { AssetLoader, AudioPort, PlayerHost, Presenter } from "./ports/index.js"
import { make as makeScheduler } from "./scheduler/index.js"
import { layer as sessionLayer, PlayerSession } from "./session/index.js"
import { layer as saveLayer, profileLayer, ProfileStore, SaveStore } from "./stores/index.js"
import type { ProfileCorrupt, SaveCorrupt } from "./stores/index.js"

/**
 * Where the player starts (spec 0.41.0 §7.1): a new game's `seed` (default
 * `{ derive: true }`), a runtime `snapshot`, or a runtime `session` the host
 * built — the player drives it and never closes it.
 */
export type LutePlayerStart =
  | { readonly seed?: Contract.Seed; readonly snapshot?: never; readonly session?: never }
  | { readonly snapshot: Contract.Snapshot; readonly seed?: never; readonly session?: never }
  | { readonly session: Runtime.Session.SessionShape; readonly seed?: never; readonly snapshot?: never }

/** What a host supplies to {@link LutePlayer.make}. */
export type LutePlayerOptions = LutePlayerStart & {
  /** Decoded bindings, or the text of `player.bindings.yaml`. */
  readonly bindings: Bindings | string
  readonly presenter: Presenter
  readonly audio: AudioPort
  readonly assets: AssetLoader
  readonly host?: PlayerHost
  /** Who paces lines (spec 0.41.0 §7.5); default `"director"`. */
  readonly linePacing?: LinePacing
  /** When the `auto` slot is written (spec 0.41.0 §7.9); default at a presentation start and every `awaitChoice`. */
  readonly autosave?: Autosave
}

/** A running player: send actions and inputs, restore, observe the view, query the session, shut down. */
export interface LutePlayerHandle extends DirectorHandle {
  /** The runtime session in use: read `current`, subscribe to `events`. */
  readonly session: Runtime.Session.SessionShape
  /** `session.program`: `view` / `evaluate` / `def` / `gate` / `candidates` without a second load. */
  readonly program: Runtime.Program.Program
}

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

/**
 * The services a host provides when it passes its own `session`: the same
 * bundle (the cue factory reads artifacts and the index from it) and the
 * key-value store.
 */
export type LutePlayerSessionRequirements =
  | Runtime.BundleSource.BundleSource
  | KeyValueStore.KeyValueStore

const build = Effect.fn("Lute.Player.make")(function* (options: LutePlayerOptions) {
  const bindings = yield* decode(options.bindings)
  const bundle = yield* (yield* Runtime.BundleSource.BundleSource).load
  const seed: Contract.Seed | undefined = options.session === undefined && options.snapshot === undefined
    ? options.seed ?? { derive: true }
    : undefined
  const session: Runtime.Session.SessionShape = options.session !== undefined
    ? options.session
    : Context.get(
      yield* Layer.build(Runtime.Session.layer(options.snapshot !== undefined ? { snapshot: options.snapshot } : { seed: seed ?? { derive: true } })),
      Runtime.Session.Session
    )
  const playerSession = Context.get(
    yield* Layer.build(sessionLayer({ bindings, ...(seed === undefined ? {} : { seed }), cast: bundle.index.cast ?? {} }).pipe(
      Layer.provide(Layer.succeed(Runtime.Session.Session, session))
    )),
    PlayerSession
  )
  const project = playerSession.fingerprint
  const built = yield* Layer.build(Layer.mergeAll(saveLayer({ project }), profileLayer({ project })))
  const director = yield* makeDirector({
    bindings,
    session: playerSession,
    presenter: options.presenter,
    audio: options.audio,
    assets: options.assets,
    profile: Context.get(built, ProfileStore),
    saves: Context.get(built, SaveStore),
    cueFactory: makeCues({ bundle, bindings, ...(options.host?.plugin === undefined ? {} : { plugin: options.host.plugin }) }),
    scheduler: yield* makeScheduler(),
    ...(options.host === undefined ? {} : { host: options.host }),
    ...(options.linePacing === undefined ? {} : { linePacing: options.linePacing }),
    ...(options.autosave === undefined ? {} : { autosave: options.autosave })
  })
  return { ...director, session, program: session.program } satisfies LutePlayerHandle
})

/** Why a player could not start: bindings, bundle, runtime start, input log, stores. */
export type LutePlayerError =
  | BindingsInvalid
  | Runtime.Errors.BundleError
  | Runtime.Errors.RuntimeRejection
  | Runtime.Errors.InputLogError
  | ProfileCorrupt
  | SaveCorrupt

/** Start a player in the current scope; it boots to the title screen. */
export function make(
  options: LutePlayerOptions & { readonly session: Runtime.Session.SessionShape }
): Effect.Effect<LutePlayerHandle, LutePlayerError, Scope.Scope | LutePlayerSessionRequirements>
export function make(options: LutePlayerOptions): Effect.Effect<LutePlayerHandle, LutePlayerError, Scope.Scope | LutePlayerRequirements>
export function make(options: LutePlayerOptions): Effect.Effect<LutePlayerHandle, LutePlayerError, Scope.Scope | LutePlayerRequirements> {
  return build(options)
}

/** Namespace-style facade: `LutePlayer.make(options)`. */
export const LutePlayer = { make }
