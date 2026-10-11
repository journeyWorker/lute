/** Effect director: owns the progression actor and all player side effects. */
import { Context, Deferred, Effect, Exit, Fiber, Layer, Queue, Schema, Scope, Stream, SubscriptionRef } from "effect"
import { createActor } from "xstate"
import type { Contract } from "@lute-lang/runtime"
import { derivePlayerView, playerMachine } from "../machine/playerMachine.js"
import type {
  CueContext, CueFactoryShape, CueSchedulerShape, DirectorCommand, Input, LineGate,
  MachineEvent, Output, PlayerView, Stage
} from "../model.js"
import type { PlayerMachineSnapshot } from "../machine/playerMachine.js"
import type { PlayerSessionShape } from "../session/index.js"
import type { Bindings } from "../bindings/schema.js"
import { resolveFile } from "../bindings/load.js"
import type {
  AssetLoader, AudioPort, LineView, PlayerAction, PlayerHost, Presenter, SlotId
} from "../ports/index.js"
import { AssetMissing } from "../ports/index.js"
import type { ProfileStoreShape, SaveStoreShape } from "../stores/index.js"

export interface DirectorOptions {
  readonly bindings: Bindings
  readonly session: PlayerSessionShape
  readonly presenter: Presenter
  readonly audio: AudioPort
  readonly assets: AssetLoader
  readonly profile: ProfileStoreShape
  readonly saves: SaveStoreShape
  readonly host?: PlayerHost
  readonly cueFactory: CueFactoryShape
  readonly scheduler: CueSchedulerShape
  readonly seed?: Contract.Seed
  readonly titleBackground?: string
}

export interface DirectorHandle {
  readonly actions: (action: PlayerAction) => Effect.Effect<void>
  readonly view: SubscriptionRef.SubscriptionRef<PlayerView>
  readonly shutdown: Effect.Effect<void>
}

interface WorkEvent { readonly _tag: "event"; readonly event: MachineEvent }
interface WorkCommand { readonly _tag: "command"; readonly command: DirectorCommand }
type Work = WorkEvent | WorkCommand

const isCommand = (value: unknown): value is DirectorCommand =>
  typeof value === "object" && value !== null && typeof (value as { readonly type?: unknown }).type === "string"

const commandType = (value: unknown): string => typeof value === "object" && value !== null && "type" in value
  ? String((value as { readonly type: unknown }).type) : "unknown"

const seconds = (value: number): number => Math.max(0, value)

/** Runtime director value. Most hosts should use {@link LutePlayer.make}. */
export class Director extends Context.Service<Director, DirectorHandle>()("Lute.Player.Director") {}

const musicFile = (bindings: Bindings, stage: Stage): string | undefined => {
  const music = stage.music
  if (music == null) return undefined
  if (music.assetId !== undefined && music.assetId !== null) return bindings.assets?.[music.assetId] ?? resolveFile(bindings, music.assetId)
  if (music.mood !== undefined && music.mood !== null) return bindings.music?.moods?.[music.mood]
  return undefined
}

export const make = Effect.fn("Lute.Player.Director.make")(function* (options: DirectorOptions) {
  const factory = options.cueFactory
  const scheduler = options.scheduler
  const settings = yield* SubscriptionRef.get(options.profile.settings)
  const view = yield* SubscriptionRef.make<PlayerView>({
    flow: "boot", story: "running", mode: "normal", overlay: "none", countdownRemaining: undefined, uiHidden: false
  })
  const work = yield* Queue.unbounded<Work>()
  const actor = createActor(playerMachine)
  let batchFiber: Fiber.Fiber<void, unknown> | undefined
  let countdownFiber: Fiber.Fiber<void, never> | undefined
  let autoFiber: Fiber.Fiber<void, never> | undefined
  let countdownRef = yield* SubscriptionRef.make<number>(0)
  let countdownPaused = false
  let lineWait: Deferred.Deferred<void> | undefined
  let currentLine: LineView | undefined
  const voiceWaiters = new Map<string, Deferred.Deferred<void>>()
  const voiceEnded = new Set<string>()
  let idleOptions: ReadonlyArray<{ readonly label: string; readonly input: Input }> = []
  let currentSettings = settings
  let closed = false
  const commandBuffer: DirectorCommand[] = []

  const snapshot = (): PlayerMachineSnapshot => actor.getSnapshot() as unknown as PlayerMachineSnapshot
  const updateView = Effect.flatMap(SubscriptionRef.set(view, derivePlayerView(snapshot())), () => Effect.void)
  const offerEvent = (event: MachineEvent): Effect.Effect<void> => Queue.offer(work, { _tag: "event", event })
  const markVoiceEnded = (lineId: string): Effect.Effect<void> => Effect.gen(function* () {
    const waiter = voiceWaiters.get(lineId)
    if (waiter === undefined) voiceEnded.add(lineId)
    else {
      voiceWaiters.delete(lineId)
      yield* Deferred.succeed(waiter, undefined)
    }
  })
  const awaitVoice = (lineId: string): Effect.Effect<void> => Effect.gen(function* () {
    if (voiceEnded.has(lineId)) return
    const waiter = yield* Deferred.make<void>()
    voiceWaiters.set(lineId, waiter)
    yield* Deferred.await(waiter)
  })
  const offerCommand = (command: DirectorCommand): Effect.Effect<void> => Queue.offer(work, { _tag: "command", command })

  actor.on("*", (value) => {
    if (isCommand(value)) commandBuffer.push(value)
  })

  const interruptBatch = Effect.fn("Lute.Player.Director.interruptBatch")(function* () {
    if (batchFiber !== undefined) {
      yield* Fiber.interrupt(batchFiber)
      batchFiber = undefined
    }
  })

  const finishLine = Effect.fn("Lute.Player.Director.finishLine")(function* () {
    const waiter = lineWait
    lineWait = undefined
    currentLine = undefined
    if (waiter !== undefined) yield* Deferred.succeed(waiter, undefined)
  })

  const lineGate: LineGate = {
    release: (line) => Effect.gen(function* () {
      currentLine = line
      lineWait = yield* Deferred.make<void>()
      yield* offerEvent({ type: "line.shown", read: line.read, hasVoice: line.voiceKey.length > 0 })
      const waiter = lineWait
      if (waiter !== undefined) yield* Deferred.await(waiter)
    })
  }

  const showLoadingDuring = <A, E>(effect: Effect.Effect<A, E, Scope.Scope>): Effect.Effect<A, E, Scope.Scope> => Effect.gen(function* () {
    let shown = false
    const timer = yield* Effect.forkScoped(
      Effect.andThen(Effect.sleep(300), Effect.sync(() => { shown = true }))
        .pipe(Effect.andThen(options.presenter.showScreen({ _tag: "Loading", progress: 0 })))
    )
    const result = yield* Effect.exit(effect)
    yield* Fiber.interrupt(timer)
    if (shown) yield* options.presenter.hideScreen
    return yield* Exit.match(result, {
      onFailure: (cause) => Effect.failCause(cause),
      onSuccess: (value) => Effect.succeed(value)
    })
  })

  const ensureAssets = (files: ReadonlyArray<string>): Effect.Effect<void, never, Scope.Scope> => {
    const unique = [...new Set(files)]
    if (unique.length === 0) return Effect.void
    return showLoadingDuring(options.assets.ensure(unique).pipe(
      Effect.catchTag("AssetMissing", (error: AssetMissing) => Effect.logDebug(`AssetMissing: ${error.file}`))
    ))
  }

  const setMode = (mode: "normal" | "auto" | "skip"): Effect.Effect<void> =>
    SubscriptionRef.set(modeRef, mode === "skip" ? "skip" : "normal")

  const playBatch = Effect.fn("Lute.Player.Director.playBatch")(function* (output: Output) {
    const read = yield* SubscriptionRef.get(options.profile.readLines)
    const cues = yield* factory.fromOutput(output, read)
    const files = cues.flatMap((cue) => cue.files)
    const context: CueContext = { presenter: options.presenter, audio: options.audio, bindings: options.bindings, settings: currentSettings, voiceEnded: markVoiceEnded }
    if (output.events.some((event) => event.type === "presentation")) yield* saveSlot("auto")
    yield* offerEvent({ type: "batch.started" })
    const running = Effect.scoped(Effect.gen(function* () {
      yield* ensureAssets(files)
      yield* scheduler.play(cues, context, modeRef, lineGate)
      for (const cue of cues) if (cue.line !== undefined) yield* options.profile.markRead(cue.line.lineId)
      const point = yield* options.session.current
      if (point.await.type === "awaitChoice") yield* saveSlot("auto")
      yield* offerEvent({ type: "batch.settled", await: point.await })
      batchFiber = undefined
    }))
    batchFiber = yield* Effect.forkScoped(running)
  })

  // CueScheduler's mode is only normal/skip; auto is normal pacing.
  const modeRef = yield* SubscriptionRef.make<"normal" | "skip">("normal")

  const stopTimers = Effect.fn("Lute.Player.Director.stopTimers")(function* () {
    if (countdownFiber !== undefined) yield* Fiber.interrupt(countdownFiber)
    if (autoFiber !== undefined) yield* Fiber.interrupt(autoFiber)
    countdownFiber = undefined
    autoFiber = undefined
    countdownPaused = false
  })

  const countdownLoop = (initial: number): Effect.Effect<void> => Effect.gen(function* () {
    let remaining = initial
    yield* SubscriptionRef.set(countdownRef, remaining)
    while (remaining > 0) {
      yield* Effect.sleep(100)
      if (countdownPaused) continue
      remaining = Math.max(0, Math.round((remaining * 1000 - 100) / 1000 * 1000) / 1000)
      yield* SubscriptionRef.set(countdownRef, remaining)
      yield* offerEvent({ type: "countdown.tick", remaining })
    }
    yield* offerEvent({ type: "countdown.expired" })
  })

  const restartMusic = (stage: Stage): Effect.Effect<void> => Effect.gen(function* () {
    const file = musicFile(options.bindings, stage)
    if (file === undefined) yield* options.audio.stop("music", 0)
    else yield* options.audio.play("music", { file, loop: true, volume: 1, fade: 0 })
  })

  const saveSlot = (slot: SlotId): Effect.Effect<void, unknown, Scope.Scope> => Effect.gen(function* () {
    const runtime = yield* options.session.snapshot
    const backlog = yield* SubscriptionRef.get(options.session.backlog)
    const label = backlog.at(-1)?.text ?? "Save"
    const thumbnail = yield* options.presenter.capture
    yield* options.saves.save(slot, { project: options.session.fingerprint, runtime, label, thumbnail })
    yield* offerEvent({ type: "save.done" })
    yield* offerEvent({ type: "slot.occupied", slot, occupied: true })
  })

  const loadSlot = (slot: SlotId): Effect.Effect<void, unknown, Scope.Scope> => Effect.gen(function* () {
    yield* interruptBatch()
    const saved = yield* options.saves.load(slot)
    const point = yield* options.session.restore(saved.runtime)
    yield* options.presenter.hideScreen
    yield* options.presenter.restore(point.stage)
    yield* restartMusic(point.stage)
    yield* offerEvent({ type: "load.done", await: point.await })
  })

  const handleCommand = (command: DirectorCommand): Effect.Effect<void, unknown, Scope.Scope> => {
    switch (command.type) {
      case "session.newGame":
        return Effect.gen(function* () {
          yield* interruptBatch()
          const output = yield* options.session.begin(options.seed ?? {})
          yield* playBatch(output)
        })
      case "session.continue": return loadSlot("quick")
      case "session.send":
        return Effect.gen(function* () {
          yield* interruptBatch()
          const output = yield* options.session.send(command.input)
          yield* playBatch(output)
        })
      case "line.reveal": return options.presenter.revealLine
      case "line.release": return Effect.gen(function* () { yield* options.presenter.clearLine; yield* finishLine(); yield* offerEvent({ type: "line.released" }) })
      case "auto.start":
        return Effect.gen(function* () {
          if (autoFiber !== undefined) yield* Fiber.interrupt(autoFiber)
          const lineId = currentLine?.lineId
          autoFiber = yield* Effect.forkScoped(Effect.gen(function* () {
            if (command.waitVoice && lineId !== undefined) yield* awaitVoice(lineId)
            yield* Effect.sleep(seconds(currentSettings.autoDelay))
            yield* offerEvent({ type: "auto.elapsed" })
          }))
        })
      case "auto.cancel": return autoFiber === undefined ? Effect.void : Fiber.interrupt(autoFiber).pipe(Effect.andThen(Effect.sync(() => { autoFiber = undefined })))
      case "countdown.start":
        return Effect.gen(function* () { if (countdownFiber !== undefined) yield* Fiber.interrupt(countdownFiber); countdownPaused = false; countdownFiber = yield* Effect.forkScoped(countdownLoop(command.seconds)) })
      case "countdown.pause": return Effect.sync(() => { countdownPaused = true })
      case "countdown.resume": return Effect.sync(() => { countdownPaused = false })
      case "countdown.cancel": return countdownFiber === undefined ? Effect.void : Fiber.interrupt(countdownFiber).pipe(Effect.andThen(Effect.sync(() => { countdownFiber = undefined })))
      case "choice.show":
        return Effect.forkScoped(Effect.gen(function* () {
          const remaining = Stream.fromEffectRepeat(Effect.andThen(Effect.sleep(100), SubscriptionRef.get(countdownRef)))
          const option = yield* options.presenter.showChoice(command.await.menu, remaining)
          yield* offerEvent({ type: "choose", option })
        })).pipe(Effect.asVoid)
      case "choice.hide": return options.presenter.hideChoice
      case "bridge.call": {
        const handler = options.host?.bridge?.[command.await.tag]
        return handler === undefined
          ? offerEvent({ type: "bridge.unhandled", tag: command.await.tag })
          : handler(command.await).pipe(Effect.flatMap((fields) => options.session.send({ type: "bridgeResult", request: command.await.request, fields: Schema.decodeUnknownSync(Schema.Record(Schema.String, Schema.Json))(fields) })), Effect.flatMap(playBatch), Effect.andThen(offerEvent({ type: "bridge.answered" })))
      }
      case "idle.flow":
        return Effect.gen(function* () {
          const hostInput = options.host?.idle === undefined ? undefined : yield* options.host.idle(command.await)
          if (hostInput !== undefined) return yield* handleCommand({ type: "session.send", input: hostInput })
          const result = yield* options.session.idle
          if (result?._tag === "Input") return yield* handleCommand({ type: "session.send", input: result.input })
          if (result?._tag === "Menu") {
            idleOptions = result.options
            yield* options.presenter.showScreen({ _tag: "Idle", options: result.options.map(({ label }) => ({ label })) })
          }
        })
      case "mode.changed": return setMode(command.mode)
      case "save": return saveSlot(command.slot)
      case "load": return loadSlot(command.slot)
      case "delete": return options.saves.delete(command.slot).pipe(Effect.andThen(offerEvent({ type: "slot.occupied", slot: command.slot, occupied: false })))
      case "rollback":
        return Effect.gen(function* () { yield* interruptBatch(); const point = yield* options.session.rollback(1); yield* options.presenter.restore(point.stage); yield* restartMusic(point.stage); yield* offerEvent({ type: "load.done", await: point.await }) })
      case "screen.show": return options.presenter.showScreen(command.screen)
      case "screen.hide": return options.presenter.hideScreen
      case "ui.hidden": return options.presenter.setUiHidden(command.hidden)
      case "settings.apply":
        return Effect.gen(function* () { yield* options.profile.updateSettings(command.settings); currentSettings = yield* SubscriptionRef.get(options.profile.settings); yield* options.presenter.applySettings(currentSettings); yield* options.audio.setVolumes(currentSettings.volumes) })
      case "voice.replay": {
        const pattern = options.bindings.voice?.pattern
        const file = typeof pattern !== "string" ? undefined : resolveFile(options.bindings, pattern.replaceAll("{voiceKey}", command.lineId))
        return file === undefined ? Effect.void : options.audio.play("voice", { file, loop: false, volume: 1, fade: 0 })
      }
      case "toTitle": return Effect.gen(function* () { yield* interruptBatch(); yield* stopTimers(); yield* options.presenter.showScreen({ _tag: "Title", canContinue: true, background: options.titleBackground }) })
    }
  }

  const process = Effect.gen(function* () {
    while (!closed) {
      const item = yield* Queue.take(work)
      if (item._tag === "event") {
        if (item.event.type === "pickIdle") {
          const selected = idleOptions[item.event.index]
          if (selected !== undefined) yield* handleCommand({ type: "session.send", input: selected.input }).pipe(Effect.catchCause(() => offerEvent({ type: "failed", message: "idle input failed" })))
        } else {
          actor.send(item.event)
        }
      } else yield* handleCommand(item.command).pipe(Effect.catchCause(() => offerEvent({ type: "failed", message: "director command failed" })))
      while (commandBuffer.length > 0) {
        const command = commandBuffer.shift()
        if (command !== undefined) yield* handleCommand(command).pipe(Effect.catchCause(() => offerEvent({ type: "failed", message: "director command failed" })))
      }
      yield* updateView
    }
  })
  const processor = yield* Effect.forkScoped(process)
  yield* Effect.forkScoped(Stream.runForEach(options.presenter.actions, (action: PlayerAction) => offerEvent(action)))
  actor.start()
  yield* options.presenter.applySettings(currentSettings)
  yield* options.audio.setVolumes(currentSettings.volumes)
  yield* offerEvent({ type: "boot.ready", canContinue: (yield* options.saves.list).some(({ slot }) => slot === "quick" || slot === "auto") })

  const shutdown = Effect.gen(function* () {
    if (closed) return
    closed = true
    yield* Fiber.interrupt(processor)
    yield* Queue.shutdown(work)
    actor.stop()
  })
  return { actions: (action: PlayerAction) => offerEvent(action), view, shutdown }
})

/** Layer constructor for hosts that already provide all player dependencies. */
export const layer = (options: DirectorOptions): Layer.Layer<Director, unknown> => Layer.effect(Director, make(options))
