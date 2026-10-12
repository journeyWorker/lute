import { Deferred, Effect, Layer, Schema, Stream, SubscriptionRef } from "effect"
import { TestClock } from "effect/testing"
import * as KeyValueStore from "effect/persistence/KeyValueStore"
import * as Contract from "../../../runtime/src/generated/Contract.js"
import * as LuteBun from "../../../runtime/src/bun.js"
import { BundleSource } from "../../../runtime/src/BundleSource.js"
import { PlayerSession, layer as sessionLayer } from "../../src/session/index.js"
import { ProfileStore, SaveStore, profileLayer, layer as saveLayer } from "../../src/stores/index.js"
import { make as makeDirector } from "../../src/director/index.js"
import * as CueFactory from "../../src/cues/index.js"
import * as Scheduler from "../../src/scheduler/index.js"
import { AssetMissing } from "../../src/ports/index.js"
import type { AudioChannel, AudioPlay, LineView, Presenter, ScreenModel, StageCue, Volumes, PlayerHost } from "../../src/ports/index.js"
import type { Bindings } from "../../src/bindings/schema.js"
import type { Stage } from "../../src/model.js"

export const flush = Effect.forEach(Array.from({ length: 100 }), () => Effect.yieldNow, { discard: true })
export const record = (value: unknown): Readonly<Record<string, unknown>> => Schema.decodeUnknownSync(Schema.Record(Schema.String, Schema.Unknown))(value)
export const seed = { derive: true }
export interface Options {
  readonly host?: PlayerHost
  readonly bindings?: Bindings
  readonly choices?: string[]
  readonly typing?: boolean
  readonly voiceMs?: number
  readonly missing?: ReadonlySet<string>
  readonly seed?: Contract.Seed
}
export const harness = (bundle: string, options: Options = {}) => Effect.gen(function* () {
  const session = yield* PlayerSession
  const source = yield* BundleSource
  const compiled = yield* source.load
  const profile = yield* ProfileStore
  const saves = yield* SaveStore
  const kv = yield* KeyValueStore.KeyValueStore
  const lines: LineView[] = []
  const stages: StageCue[] = []
  const restores: Stage[] = []
  const screens: ScreenModel[] = []
  const plays: { channel: AudioChannel; sound: AudioPlay }[] = []
  const stops: { channel: AudioChannel; fade: number }[] = []
  const volumes: Volumes[] = []
  const ensured: ReadonlyArray<string>[] = []
  const choices = options.choices ?? []
  let typing: Deferred.Deferred<void> | undefined
  let reveals = 0
  let clears = 0
  const presenter: Presenter = {
    stage: (cue) => Effect.sync(() => { stages.push(cue) }),
    showLine: (line) => Effect.gen(function* () {
      lines.push(line)
      if (options.typing) { typing = yield* Deferred.make<void>(); yield* Deferred.await(typing) }
    }),
    revealLine: Effect.gen(function* () { reveals++; if (typing) yield* Deferred.succeed(typing, undefined) }),
    clearLine: Effect.sync(() => { clears++ }),
    restore: (stage) => Effect.sync(() => { restores.push(stage) }),
    showChoice: () => Effect.suspend(() => { const next = choices.shift(); return next === undefined ? Effect.never : Effect.succeed(next) }),
    hideChoice: Effect.void,
    showScreen: (screen) => Effect.sync(() => { screens.push(screen) }),
    hideScreen: Effect.void,
    setUiHidden: () => Effect.void,
    applySettings: () => Effect.void,
    capture: Effect.succeed("data:image/png;base64,test"),
    actions: Stream.empty
  }
  const bindings = options.bindings ?? { bindingsVersion: "0.40", canvas: { width: 1920, height: 1080 }, flow: { newGame: { occasion: "visit" } } }
  const director = yield* makeDirector({ session, profile, saves, presenter, bindings, seed: options.seed ?? seed,
    ...(options.host === undefined ? {} : { host: options.host }),
    cueFactory: CueFactory.make({ bundle: compiled, bindings }), scheduler: yield* Scheduler.make(),
    audio: {
      play: (channel, sound) => Effect.gen(function* () { plays.push({ channel, sound }); if (channel === "voice") yield* Effect.sleep(options.voiceMs ?? 2000) }),
      stop: (channel, fade) => Effect.sync(() => { stops.push({ channel, fade }) }),
      setVolumes: (value) => Effect.sync(() => { volumes.push(value) })
    },
    assets: { ensure: (files) => Effect.suspend(() => { ensured.push(files); const missing = files.find((file) => options.missing?.has(file)); return missing === undefined ? Effect.void : Effect.fail(new AssetMissing({ file: missing, message: "missing fixture" })) }), progress: Stream.empty }
  })
  const action = (action: Parameters<typeof director.actions>[0]) => director.actions(action).pipe(Effect.andThen(flush))
  return { director, session, profile, saves, kv, lines, stages, restores, screens, plays, stops, volumes, ensured, action,
    reveals: () => reveals, clears: () => clears,
    view: SubscriptionRef.get(director.view),
    tick: (ms: number) => TestClock.adjust(ms).pipe(Effect.andThen(flush)) }
})
export const withHarness = <A, E, R>(bundle: string, options: Options, use: (h: Effect.Success<ReturnType<typeof harness>>) => Effect.Effect<A, E, R>) => {
  const bindings = options.bindings ?? { bindingsVersion: "0.40" as const, canvas: { width: 1920, height: 1080 }, flow: { newGame: { occasion: "visit" } } }
  const runtime = LuteBun.session({ bundle, seed: options.seed ?? seed })
  const player = sessionLayer({ bindings }).pipe(Layer.provide(runtime))
  return Effect.scoped(Effect.gen(function* () {
    const session = yield* PlayerSession
    return yield* Effect.flatMap(harness(bundle, options), use).pipe(Effect.provide(Layer.mergeAll(
      profileLayer({ project: session.fingerprint, debounceMs: 0 }), saveLayer({ project: session.fingerprint })
    )))
  })).pipe(Effect.provide(Layer.mergeAll(player, LuteBun.bundle(bundle), KeyValueStore.layerMemory)))
}
