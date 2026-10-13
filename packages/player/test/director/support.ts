import { Deferred, Effect, Layer, Schema, Stream, SubscriptionRef } from "effect"
import { TestClock } from "effect/testing"
import * as KeyValueStore from "effect/persistence/KeyValueStore"
import * as Contract from "../../../runtime/src/generated/Contract.js"
import * as LuteBun from "../../../runtime/src/bun.js"
import { BundleSource } from "../../../runtime/src/BundleSource.js"
import * as RuntimeSession from "../../../runtime/src/Session.js"
import { PlayerSession, layer as sessionLayer } from "../../src/session/index.js"
import { ProfileStore, SaveStore, profileLayer, layer as saveLayer } from "../../src/stores/index.js"
import { make as makeDirector } from "../../src/director/index.js"
import type { Autosave, LinePacing } from "../../src/director/index.js"
import type { CueFactoryOptions } from "../../src/cues/index.js"
import * as Scheduler from "../../src/scheduler/index.js"
import { AssetMissing } from "../../src/ports/index.js"
import type { AudioChannel, AudioPlay, LineView, Presenter, ScreenModel, StageCue, Volumes, PlayerHost } from "../../src/ports/index.js"
import type { Bindings } from "../../src/bindings/schema.js"
import type { CueFactoryShape, CueFactoryWrapper, Stage } from "../../src/model.js"

export const flush = Effect.forEach(Array.from({ length: 100 }), () => Effect.yieldNow, { discard: true })
export const record = (value: unknown): Readonly<Record<string, unknown>> => Schema.decodeUnknownSync(Schema.Record(Schema.String, Schema.Unknown))(value)
export const seed = { derive: true }
export const defaultBindings: Bindings = { bindingsVersion: "0.41", canvas: { width: 1920, height: 1080 }, flow: { newGame: { occasion: "visit" } } }
export interface Options {
  readonly host?: PlayerHost
  readonly bindings?: Bindings
  readonly choices?: string[]
  /** `showLine` waits until the line is revealed (`revealLine`) or the reader confirms (`h.confirmLine`). */
  readonly typing?: boolean
  readonly voiceMs?: number
  readonly missing?: ReadonlySet<string>
  readonly seed?: Contract.Seed
  readonly linePacing?: LinePacing
  readonly autosave?: Autosave
  readonly confirmsLines?: boolean
  /** A default-factory wrapper, or a pre-composed factory built from the loaded bundle. */
  readonly cueFactory?: CueFactoryWrapper | { readonly compose: (bundle: CueFactoryOptions["bundle"], bindings: Bindings) => CueFactoryShape }
}
export const harness = (options: Options = {}) => Effect.gen(function* () {
  const session = yield* PlayerSession
  const runtime = yield* RuntimeSession.Session
  const source = yield* BundleSource
  const compiled = yield* source.load
  const profile = yield* ProfileStore
  const saves = yield* SaveStore
  const kv = yield* KeyValueStore.KeyValueStore
  const lines: LineView[] = []
  const lineModes: string[] = []
  const stages: StageCue[] = []
  const restores: Stage[] = []
  const screens: ScreenModel[] = []
  const plays: { channel: AudioChannel; sound: AudioPlay }[] = []
  const stops: { channel: AudioChannel; fade: number }[] = []
  const volumes: Volumes[] = []
  const ensured: ReadonlyArray<string>[] = []
  /** Asset loads and sound starts, in call order. */
  const media: string[] = []
  const choices = options.choices ?? []
  let typing: Deferred.Deferred<void> | undefined
  let reveals = 0
  let clears = 0
  let captures = 0
  let hiddenScreens = 0
  let hiddenChoices = 0
  const presenter: Presenter = {
    stage: (cue) => Effect.sync(() => { stages.push(cue) }),
    showLine: (line, mode) => Effect.gen(function* () {
      lines.push(line)
      lineModes.push(mode)
      if (options.typing && mode === "normal") { typing = yield* Deferred.make<void>(); yield* Deferred.await(typing) }
    }),
    revealLine: Effect.gen(function* () { reveals++; if (typing) yield* Deferred.succeed(typing, undefined) }),
    clearLine: Effect.sync(() => { clears++ }),
    restore: (stage) => Effect.sync(() => { restores.push(stage) }),
    showChoice: () => Effect.suspend(() => { const next = choices.shift(); return next === undefined ? Effect.never : Effect.succeed(next) }),
    hideChoice: Effect.sync(() => { hiddenChoices++ }),
    showScreen: (screen) => Effect.sync(() => { screens.push(screen) }),
    hideScreen: Effect.sync(() => { hiddenScreens++ }),
    setUiHidden: () => Effect.void,
    applySettings: () => Effect.void,
    capture: Effect.sync(() => { captures++; return "data:image/png;base64,test" }),
    actions: Stream.empty,
    ...(options.confirmsLines === undefined ? {} : { confirmsLines: options.confirmsLines })
  }
  const bindings = options.bindings ?? defaultBindings
  const director = yield* makeDirector({ session, profile, saves, presenter, bindings, bundle: compiled,
    ...(options.host === undefined ? {} : { host: options.host }),
    ...(options.linePacing === undefined ? {} : { linePacing: options.linePacing }),
    ...(options.autosave === undefined ? {} : { autosave: options.autosave }),
    ...(options.cueFactory === undefined ? {} : { cueFactory: typeof options.cueFactory === "function" ? options.cueFactory : options.cueFactory.compose(compiled, bindings) }),
    scheduler: yield* Scheduler.make(),
    audio: {
      play: (channel, sound) => Effect.gen(function* () { plays.push({ channel, sound }); media.push(`play ${channel} ${sound.file}`); if (channel === "voice") yield* Effect.sleep(options.voiceMs ?? 2000) }),
      stop: (channel, fade) => Effect.sync(() => { stops.push({ channel, fade }) }),
      setVolumes: (value) => Effect.sync(() => { volumes.push(value) })
    },
    assets: { ensure: (files) => Effect.suspend(() => { ensured.push(files); media.push(...files.map((file) => `ensure ${file}`)); const missing = files.find((file) => options.missing?.has(file)); return missing === undefined ? Effect.void : Effect.fail(new AssetMissing({ file: missing, message: "missing fixture" })) }), progress: Stream.empty }
  })
  const action = (action: Parameters<typeof director.actions>[0]) => director.actions(action).pipe(Effect.andThen(flush))
  return { director, session, runtime, profile, saves, kv, lines, lineModes, stages, restores, screens, plays, stops, volumes, ensured, media, action,
    reveals: () => reveals, clears: () => clears, captures: () => captures, hiddenScreens: () => hiddenScreens, hiddenChoices: () => hiddenChoices,
    /** The reader confirms the typing line (a presenter that takes the confirm itself). */
    confirmLine: Effect.suspend(() => typing === undefined ? Effect.void : Deferred.succeed(typing, undefined)).pipe(Effect.andThen(flush)),
    view: SubscriptionRef.get(director.view),
    tick: (ms: number) => TestClock.adjust(ms).pipe(Effect.andThen(flush)) }
})
export type Harness = Effect.Success<ReturnType<typeof harness>>
export const withHarness = <A, E, R>(bundle: string, options: Options, use: (h: Harness) => Effect.Effect<A, E, R>) => {
  const bindings = options.bindings ?? defaultBindings
  const runtime = LuteBun.session({ bundle, seed: options.seed ?? seed })
  const player = sessionLayer({ bindings, seed: options.seed ?? seed }).pipe(Layer.provideMerge(runtime))
  return Effect.scoped(Effect.gen(function* () {
    const session = yield* PlayerSession
    return yield* Effect.flatMap(harness(options), use).pipe(Effect.provide(Layer.mergeAll(
      profileLayer({ project: session.fingerprint, debounceMs: 0 }), saveLayer({ project: session.fingerprint })
    )))
  })).pipe(Effect.provide(Layer.mergeAll(player, LuteBun.bundle(bundle), KeyValueStore.layerMemory)))
}
