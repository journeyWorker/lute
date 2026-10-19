/** Effect director: owns the progression actor and all player side effects. */
import { Context, Deferred, Effect, Exit, Fiber, Layer, Queue, Schema, Scope, Stream, SubscriptionRef } from "effect"
import { createActor } from "xstate"
import * as Runtime from "@lute-lang/runtime"
import type { Errors } from "@lute-lang/runtime"
import { derivePlayerView, playerMachine } from "../machine/playerMachine.js"
import type {
  ConfirmAction, Cue, CueContext, CueFactoryShape, CueFactoryWrapper, CueSchedulerShape, DirectorCommand, Input, LineGate,
  MachineEvent, Output, Overlay, PlayerView, SaveRuntime, Stage
} from "../model.js"
import type { PlayerSessionShape } from "../session/index.js"
import { localizedEntry, SaveIncompatible } from "../session/index.js"
import type { Bindings } from "../bindings/schema.js"
import type {
  AssetLoader, AudioPort, LineView, PlayerAction, PlayerHost, Presenter, ScreenModel, Settings, SlotId
} from "../ports/index.js"
import { bundleLocales, make as makeCues, mergePlugin, stageFiles, stageMusic, voiceFile } from "../cues/index.js"
import { AssetMissing } from "../ports/index.js"
import type { ProfileStoreShape, SaveStoreShape } from "../stores/index.js"
import { autosavesAt } from "../stores/autosave.js"
import type { Autosave } from "../stores/autosave.js"

export type { Autosave } from "../stores/autosave.js"

/** Who paces lines (spec 0.41.0 §7.5): the director (reveal/release on `advance`), or the presenter. */
export type LinePacing = "director" | "presenter"

export interface DirectorOptions {
  readonly bindings: Bindings
  readonly bundle: Runtime.Bundle.Bundle
  readonly session: PlayerSessionShape
  readonly presenter: Presenter
  readonly audio: AudioPort
  readonly assets: AssetLoader
  readonly profile: ProfileStoreShape
  readonly saves: SaveStoreShape
  readonly host?: PlayerHost
  /** A pre-composed factory, or a wrapper around the default factory. */
  readonly cueFactory?: CueFactoryShape | CueFactoryWrapper
  readonly scheduler: CueSchedulerShape
  /**
   * `"presenter"`: `Presenter.showLine` completes when the reader released the
   * line; `advance` never reveals or releases a line and no auto timer runs
   * for lines. Default `"director"`.
   */
  readonly linePacing?: LinePacing
  /** When the `auto` slot is written; default at a presentation start and at every `awaitChoice`. */
  readonly autosave?: Autosave
}

/** The director is not at an await that takes the operation (spec 0.41.0 §7.2, §7.4). */
export class DirectorBusy extends Schema.TaggedError<DirectorBusy>()("DirectorBusy", {
  flow: Schema.String,
  story: Schema.String
}) {}

export interface DirectorHandle {
  readonly actions: (action: PlayerAction) => Effect.Effect<void>
  readonly view: SubscriptionRef.SubscriptionRef<PlayerView>
  /**
   * Send a runtime input while no batch runs and the story waits at `idle` or
   * `ended` (else `DirectorBusy`, nothing changes). A runtime rejection fails
   * with it and the director stays at the same await. Completes when the
   * runtime accepted the input and its batch started, not when it ends.
   */
  readonly send: (input: Input) => Effect.Effect<void, Errors.RuntimeRejection | Errors.InputLogError | DirectorBusy>
  /**
   * Continue from a host's save (spec 0.41.0 §7.4) in every flow state but
   * `boot`: abandon the running presentation, restore the session, drop the
   * read marks added after the save, rewrite the `auto` slot (when autosave is
   * on), rebuild the stage and re-present the restored await.
   */
  readonly restore: (save: SaveRuntime) => Effect.Effect<void, SaveIncompatible | Errors.RuntimeRejection | Errors.InputLogError | DirectorBusy>
  /** A coherent runtime snapshot, accepted-input log and backlog. */
  readonly snapshot: Effect.Effect<SaveRuntime, Errors.RuntimeRejection>
  readonly shutdown: Effect.Effect<void>
}

type SendError = Errors.RuntimeRejection | Errors.InputLogError | DirectorBusy
type RestoreError = SaveIncompatible | SendError

type Work =
  | { readonly _tag: "event"; readonly event: MachineEvent }
  | { readonly _tag: "command"; readonly command: DirectorCommand }
  | { readonly _tag: "send"; readonly input: Input; readonly reply: Deferred.Deferred<void, SendError> }
  | { readonly _tag: "restore"; readonly save: SaveRuntime; readonly reply: Deferred.Deferred<void, RestoreError> }
  /** A host hook's result, applied in the loop while its await is still current. */
  | { readonly _tag: "host"; readonly generation: number; readonly apply: Effect.Effect<void, unknown, Scope.Scope> }

const isCommand = (value: unknown): value is DirectorCommand =>
  typeof value === "object" && value !== null && "type" in value && typeof value.type === "string"

const seconds = (value: number): number => Math.max(0, value)

/** Runtime director value. Most hosts should use {@link LutePlayer.make}. */
export class Director extends Context.Service<Director, DirectorHandle>()("Lute.Player.Director") {}

export const make = Effect.fn("Lute.Player.Director.make")(function* (options: DirectorOptions) {
  const defaultFactory = makeCues({
    bundle: options.bundle,
    bindings: options.bindings,
    ...(options.host?.plugin === undefined ? {} : { plugin: options.host.plugin })
  })
  const factory = options.cueFactory === undefined
    ? defaultFactory
    : typeof options.cueFactory === "function"
      ? options.cueFactory(defaultFactory)
      : mergePlugin(options.cueFactory, options.host?.plugin)
  if (factory === undefined) {
    return yield* Effect.die(new Error("host.plugin cannot be merged into a cue factory not built by Cues.make; pass a cueFactory wrapper instead"))
  }
  const scheduler = options.scheduler
  const presenterPaced = options.linePacing === "presenter"
  const settings = yield* SubscriptionRef.get(options.profile.settings)
  const view = yield* SubscriptionRef.make<PlayerView>({
    flow: "boot", story: "running", mode: "normal", overlay: "none", countdownRemaining: undefined, uiHidden: false
  })
  const work = yield* Queue.unbounded<Work>()
  const actor = createActor(playerMachine)
  let batchFiber: Fiber.Fiber<void, unknown> | undefined
  // The batch that runs; cleared by the batch itself before it settles.
  let batchRunning: number | undefined
  let batchCount = 0
  // The running batch's cues for the current locale (spec 0.47.0 §2.2): a
  // locale switch re-resolves them, so its next line reads the new locale.
  let batchCues: { readonly output: Output; readonly read: ReadonlySet<string>; locale: string; cues: ReadonlyArray<Cue> } | undefined
  let countdownFiber: Fiber.Fiber<void, never> | undefined
  let autoFiber: Fiber.Fiber<void, never> | undefined
  let choiceFiber: Fiber.Fiber<void, never> | undefined
  // The host hook (idle flow, bridge handler) of the current await, on a fiber
  // of its own; `hostGeneration` invalidates results of an abandoned await.
  let hostFiber: Fiber.Fiber<void, never> | undefined
  let hostGeneration = 0
  const countdownRef = yield* SubscriptionRef.make<number>(0)
  let countdownPaused = false
  let currentLine: LineView | undefined
  const voiceWaiters = new Map<string, Deferred.Deferred<void>>()
  const voiceEnded = new Set<string>()
  let idleOptions: ReadonlyArray<{ readonly label: string; readonly input: Input }> = []
  let currentSettings = settings
  // Spec 0.47.0 §2.2: the bundle's locales; a saved locale it does not carry reads source text.
  const { source: sourceLocale, locales } = bundleLocales(options.bundle)
  const localeOf = (value: Settings): string => locales.includes(value.locale) ? value.locale : sourceLocale
  let closed = false
  const commandBuffer: DirectorCommand[] = []
  // Read marks this director added, by the input point (log length) of their batch.
  const readJournal = new Map<number, ReadonlyArray<string>>()

  const currentView = (): PlayerView => derivePlayerView(actor.getSnapshot())
  const busy = (): DirectorBusy => {
    const { flow, story } = currentView()
    return new DirectorBusy({ flow, story })
  }
  const updateView = Effect.gen(function* () {
    yield* SubscriptionRef.set(view, currentView())
  })
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

  actor.on("*", (value) => {
    if (isCommand(value)) commandBuffer.push(value)
  })

  const interruptBatch = Effect.fn("Lute.Player.Director.interruptBatch")(function* () {
    batchRunning = undefined
    batchCues = undefined
    if (batchFiber !== undefined) {
      yield* Fiber.interrupt(batchFiber)
      batchFiber = undefined
    }
  })

  /** Abandon the host hook of the current await (a `send`, or any input that leaves it). */
  const interruptHost = Effect.fn("Lute.Player.Director.interruptHost")(function* () {
    hostGeneration++
    if (hostFiber !== undefined) {
      yield* Fiber.interrupt(hostFiber)
      hostFiber = undefined
    }
  })

  /** Run a host hook on its own fiber; `apply` its result in the loop while the await is current. */
  const forkHost = <A>(
    hook: Effect.Effect<A>,
    apply: (value: A) => Effect.Effect<void, unknown, Scope.Scope>
  ): Effect.Effect<void, never, Scope.Scope> => Effect.gen(function* () {
    yield* interruptHost()
    const generation = hostGeneration
    hostFiber = yield* Effect.forkScoped(Effect.flatMap(hook, (value) =>
      Queue.offer(work, { _tag: "host", generation, apply: apply(value) })).pipe(Effect.asVoid))
  })

  // The release of the current line. Created when the line starts typing, so
  // a release that arrives while it still types (skip, auto) is kept.
  let lineReleased: Deferred.Deferred<void> | undefined

  const finishLine = Effect.fn("Lute.Player.Director.finishLine")(function* () {
    const released = lineReleased
    currentLine = undefined
    if (released !== undefined) yield* Deferred.succeed(released, undefined)
  })

  const hasVoice = (line: LineView): boolean => line.voiceKey.length > 0 && typeof options.bindings.voice?.pattern === "string"

  const lineGate: LineGate = {
    started: (line) => Effect.gen(function* () {
      currentLine = line
      lineReleased = yield* Deferred.make<void>()
      yield* offerEvent({ type: "line.typing", read: line.read, hasVoice: hasVoice(line) })
    }),
    release: (line) => Effect.gen(function* () {
      const released = lineReleased
      yield* offerEvent({ type: "line.shown", read: line.read, hasVoice: hasVoice(line) })
      if (released === undefined) return
      if (presenterPaced) {
        // The presenter's `showLine` completed: the reader released the line.
        // Skipping that began while it typed is released by the machine — or
        // stops at an unread line, which the presenter then shows normally.
        if ((yield* SubscriptionRef.get(modeRef)) === "skip") {
          const stopped = Stream.runHead(Stream.filter(SubscriptionRef.changes(modeRef), (mode) => mode !== "skip"))
          const outcome = yield* Effect.raceFirst(Effect.as(Deferred.await(released), "released" as const), Effect.as(stopped, "stopped" as const))
          if (outcome === "stopped") yield* options.presenter.showLine(line, "normal")
        }
        if (!(yield* Deferred.isDone(released))) {
          yield* options.presenter.clearLine
          yield* offerEvent({ type: "line.released" })
          yield* finishLine()
        }
      } else yield* Deferred.await(released)
      lineReleased = undefined
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

  // CueScheduler's mode is only normal/skip; auto is normal pacing.
  const modeRef = yield* SubscriptionRef.make<"normal" | "skip">("normal")

  const setMode = (mode: "normal" | "auto" | "skip"): Effect.Effect<void> =>
    SubscriptionRef.set(modeRef, mode === "skip" ? "skip" : "normal")

  const saveSlot = (slot: SlotId): Effect.Effect<void, unknown, Scope.Scope> => Effect.gen(function* () {
    const runtime = yield* options.session.snapshot
    const backlog = yield* SubscriptionRef.get(options.session.backlog)
    const last = backlog.at(-1)
    const label = last === undefined ? "Save" : localizedEntry(last, localeOf(currentSettings)).text
    const thumbnail = yield* options.presenter.capture
    yield* options.saves.save(slot, { project: options.session.fingerprint, runtime, label, thumbnail })
    yield* offerEvent({ type: "save.done" })
    yield* offerEvent({ type: "slot.occupied", slot, occupied: true })
  })

  // An autosave that fails is logged; it never stops the story.
  const autosave = saveSlot("auto").pipe(Effect.catchCause((cause) => Effect.logWarning("autosave failed", cause)))

  // A lineage that cannot be written is logged; it never stops the story.
  const writeLineage = options.session.snapshot.pipe(
    Effect.flatMap(({ snapshot }) => options.profile.setLineage(snapshot)),
    Effect.catchCause((cause) => Effect.logWarning("lineage write failed", cause))
  )

  const playBatch = Effect.fn("Lute.Player.Director.playBatch")(function* (output: Output) {
    const read = yield* SubscriptionRef.get(options.profile.readLines)
    const locale = localeOf(currentSettings)
    const resolved = yield* factory.fromOutput(output, read, locale)
    const localized = { output, read, locale, cues: resolved }
    batchCues = localized
    // A line cue takes the batch's cue at its index for the locale current when
    // it starts (`fromOutput` returns one cue per event, in order), then keeps it.
    const cues = resolved.map((cue, index): Cue => {
      if (cue.kind !== "line") return cue
      let pinned: Cue | undefined
      const current = (): Cue => pinned ??= localized.cues[index] ?? cue
      return {
        kind: cue.kind,
        timing: cue.timing,
        stage: cue.stage,
        files: cue.files,
        describe: cue.describe,
        get line() { return current().line },
        execute: (ctx) => Effect.suspend(() => current().execute(ctx)),
        complete: (ctx) => Effect.suspend(() => current().complete(ctx))
      }
    })
    const files = cues.flatMap((cue) => cue.files)
    const inputPoint = (yield* options.session.current).logLength
    const context: CueContext = { presenter: options.presenter, audio: options.audio, bindings: options.bindings, settings: currentSettings, voiceEnded: markVoiceEnded }
    if (autosavesAt(options.autosave, "presentation") && output.events.some((event) => event.type === "presentation")) yield* autosave
    // Inside the loop: the machine takes the batch start before the batch runs.
    actor.send({ type: "batch.started" })
    const batch = ++batchCount
    batchRunning = batch
    const running = Effect.scoped(Effect.gen(function* () {
      yield* ensureAssets(files)
      yield* scheduler.play(cues, context, modeRef, lineGate)
      const marked = new Set<string>()
      for (const cue of cues) {
        if (cue.line === undefined || read.has(cue.line.lineId) || marked.has(cue.line.lineId)) continue
        yield* options.profile.markRead(cue.line.lineId)
        marked.add(cue.line.lineId)
      }
      if (marked.size > 0) readJournal.set(inputPoint, [...(readJournal.get(inputPoint) ?? []), ...marked])
      const point = yield* options.session.current
      if (autosavesAt(options.autosave, "choice") && point.await.type === "awaitChoice") yield* autosave
      // Spec 0.47.0 §2.3: the lineage is the snapshot at the presented `ended`, written at once.
      if (point.await.type === "ended") yield* writeLineage
      if (batchRunning === batch) batchRunning = undefined
      if (batchCues === localized) batchCues = undefined
      yield* offerEvent({ type: "batch.settled", await: point.await })
    }))
    batchFiber = yield* Effect.forkScoped(running)
  })

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

  /**
   * The restored stage's music (§7.7): silent without music or at `stop` / `fadeOut`.
   * Begun on a fiber of the player scope, started at once and not awaited: a port
   * whose `play` waits (player-web before audio is unlocked) must not hold a
   * restore, and a later restore does not interrupt it (spec 0.47.2 §3).
   */
  const restartMusic = (stage: Stage): Effect.Effect<void, never, Scope.Scope> => {
    const music = stageMusic(options.bindings, stage)
    // Nothing joins the fiber, so a failing custom port is logged here (the command loop no longer sees it).
    const call = (music === undefined ? options.audio.stop("music", 0) : options.audio.play("music", music))
      .pipe(Effect.catchCause((cause) => Effect.logWarning("stage music failed", cause)))
    return Effect.asVoid(Effect.forkScoped(call, { startImmediately: true }))
  }

  const presentStage = (stage: Stage): Effect.Effect<void, never, Scope.Scope> => Effect.gen(function* () {
    yield* ensureAssets(stageFiles(options.bindings, stage))
    yield* options.presenter.restore(stage)
    yield* restartMusic(stage)
  })

  const loadSlot = (slot: SlotId): Effect.Effect<void, unknown, Scope.Scope> => Effect.gen(function* () {
    yield* interruptBatch()
    yield* interruptHost()
    const saved = yield* options.saves.load(slot)
    const point = yield* options.session.restore(saved.runtime)
    // A loaded save starts another lineage: its input points are not the journal's.
    readJournal.clear()
    yield* options.presenter.hideScreen
    yield* presentStage(point.stage)
    yield* offerEvent({ type: "load.done", await: point.await })
  })

  /**
   * Send `input` at `idle` / `ended` (spec 0.41.0 §7.2–§7.3). A refusal (busy,
   * or the runtime's) changes nothing; an accepted input leaves the await and
   * plays its batch.
   */
  const sendInput = (input: Input): Effect.Effect<void, SendError, Scope.Scope> => Effect.gen(function* () {
    const { story } = currentView()
    if (batchRunning !== undefined || (story !== "idle" && story !== "ended")) return yield* Effect.fail(busy())
    const output = yield* options.session.send(input)
    yield* interruptHost()
    // The idle menu or the ending screen gives way to the batch.
    if (idleOptions.length > 0 || story === "ended") yield* options.presenter.hideScreen
    idleOptions = []
    yield* playBatch(output)
  })

  /** Stop everything the current await runs: batch, timers, host hook, choice UI, overlays. */
  const abandonPresentation = Effect.gen(function* () {
    yield* interruptBatch()
    yield* stopTimers()
    yield* interruptHost()
    if (choiceFiber !== undefined) yield* Fiber.interrupt(choiceFiber)
    choiceFiber = undefined
    lineReleased = undefined
    currentLine = undefined
    idleOptions = []
    yield* options.presenter.hideChoice
    yield* options.presenter.hideScreen
    yield* options.presenter.clearLine
  })

  /** A host `restore` (spec 0.41.0 §7.4). */
  const restoreSave = (save: SaveRuntime): Effect.Effect<void, RestoreError, Scope.Scope> => Effect.gen(function* () {
    if (currentView().flow === "boot") return yield* Effect.fail(busy())
    if (save.snapshot.project !== undefined && save.snapshot.project !== options.session.fingerprint) {
      return yield* Effect.fail(new SaveIncompatible({ message: "save belongs to another project" }))
    }
    yield* abandonPresentation
    const restored = yield* Effect.exit(options.session.restore(save))
    if (Exit.isFailure(restored)) {
      yield* offerEvent({ type: "failed", message: "restore failed" })
      return yield* Effect.failCause(restored.cause)
    }
    const point = restored.value
    // The abandoned presentation's read marks go; the marks up to the save stay.
    const dropped: string[] = []
    for (const [inputPoint, lineIds] of readJournal) {
      if (inputPoint <= save.log.length) continue
      dropped.push(...lineIds)
      readJournal.delete(inputPoint)
    }
    if (dropped.length > 0) yield* options.profile.unmarkRead(dropped).pipe(Effect.catchCause((cause) => Effect.logWarning("read history update failed", cause)))
    yield* presentStage(point.stage)
    if (options.autosave !== false) yield* autosave
    actor.send({ type: "load.done", await: point.await })
  })

  const confirmMessage = (action: ConfirmAction | undefined): string => {
    switch (action?.type) {
      case "overwrite": return "Overwrite this save?"
      case "load": return "Load this save? Unsaved progress will be lost."
      case "delete": return "Delete this save?"
      case "toTitle": return "Return to the title screen? Unsaved progress will be lost."
      case undefined: return "Are you sure?"
    }
  }

  /** The screen model of an overlay, built from the current stores. */
  const overlayScreen = (overlay: Overlay, confirm: ConfirmAction | undefined): Effect.Effect<ScreenModel> => {
    switch (overlay) {
      case "menu": return Effect.succeed({ _tag: "Menu" })
      case "backlog":
        return Effect.map(SubscriptionRef.get(options.session.backlog), (entries): ScreenModel => {
          const locale = localeOf(currentSettings)
          return { _tag: "Backlog", entries: entries.map((entry) => localizedEntry(entry, locale)) }
        })
      case "save":
      case "load":
        return options.saves.list.pipe(
          // A corrupt index must not block the screen; the slots read as empty.
          Effect.catchTag("SaveCorrupt", (error) => Effect.as(Effect.logWarning("save index unreadable", error), [])),
          Effect.map((slots): ScreenModel => ({ _tag: "SaveLoad", mode: overlay, slots, page: 0 }))
        )
      case "settings":
        return Effect.succeed({ _tag: "Settings", settings: { ...currentSettings, locale: localeOf(currentSettings) }, locales })
      case "confirm": return Effect.succeed({ _tag: "Confirm", message: confirmMessage(confirm) })
    }
  }

  const failed = (message: string) => (cause: unknown): Effect.Effect<void> =>
    Effect.logError(message, cause).pipe(Effect.andThen(offerEvent({ type: "failed", message })))

  const handleCommand = (command: DirectorCommand): Effect.Effect<void, unknown, Scope.Scope> => {
    switch (command.type) {
      case "session.newGame":
        return Effect.gen(function* () {
          yield* interruptBatch()
          yield* interruptHost()
          const output = yield* options.session.begin
          yield* playBatch(output)
        })
      case "session.continue": return loadSlot("quick")
      case "session.send":
        return Effect.gen(function* () {
          yield* interruptBatch()
          const sent = yield* Effect.exit(options.session.send(command.input))
          if (Exit.isSuccess(sent)) return yield* playBatch(sent.value)
          // A refused answer (a closed option, a stale request) changes
          // nothing in the runtime: present the same await again.
          if (command.input.type === "choose" || command.input.type === "timeout") {
            yield* Effect.logWarning("runtime refused the answer", sent.cause)
            const point = yield* options.session.current
            return yield* offerEvent({ type: "batch.settled", await: point.await })
          }
          return yield* Effect.failCause(sent.cause)
        })
      case "line.reveal": return options.presenter.revealLine
      case "line.release":
        return Effect.gen(function* () {
          // A line released while it types (skip) is first shown in full.
          yield* options.presenter.revealLine
          yield* options.presenter.clearLine
          yield* offerEvent({ type: "line.released" })
          yield* finishLine()
        })
      case "auto.start":
        // Presenter-paced lines run no auto timer: the presenter reads `PlayerView.mode`.
        if (presenterPaced) return Effect.void
        return Effect.gen(function* () {
          if (autoFiber !== undefined) yield* Fiber.interrupt(autoFiber)
          const lineId = currentLine?.lineId
          autoFiber = yield* Effect.forkScoped(Effect.gen(function* () {
            if (command.waitVoice && lineId !== undefined) yield* awaitVoice(lineId)
            yield* Effect.sleep(seconds(currentSettings.autoDelay) * 1000)
            yield* offerEvent({ type: "auto.elapsed" })
            yield* Effect.yieldNow
          }))
        })
      case "auto.cancel": return autoFiber === undefined ? Effect.void : Fiber.interrupt(autoFiber).pipe(Effect.andThen(Effect.sync(() => { autoFiber = undefined })))
      case "countdown.start":
        return Effect.gen(function* () { if (countdownFiber !== undefined) yield* Fiber.interrupt(countdownFiber); countdownPaused = false; countdownFiber = yield* Effect.forkScoped(countdownLoop(command.seconds)) })
      case "countdown.pause": return Effect.sync(() => { countdownPaused = true })
      case "countdown.resume": return Effect.sync(() => { countdownPaused = false })
      case "countdown.cancel": return countdownFiber === undefined ? Effect.void : Fiber.interrupt(countdownFiber).pipe(Effect.andThen(Effect.sync(() => { countdownFiber = undefined })))
      case "choice.show":
        return Effect.gen(function* () {
          choiceFiber = yield* Effect.forkScoped(Effect.gen(function* () {
            const remaining = Stream.fromEffectRepeat(Effect.andThen(Effect.sleep(100), SubscriptionRef.get(countdownRef)))
            const option = yield* options.presenter.showChoice(factory.choiceView(command.await, localeOf(currentSettings)), remaining)
            yield* offerEvent({ type: "choose", option })
          }))
        })
      case "choice.hide": return Effect.andThen(Effect.sync(() => { choiceFiber = undefined }), options.presenter.hideChoice)
      case "bridge.call": {
        const handler = options.host?.bridge?.[command.await.tag]
        if (handler === undefined) return offerEvent({ type: "bridge.unhandled", tag: command.await.tag })
        const request = command.await.request
        // The handler runs on a fiber of its own: actions keep working while
        // the host plays, and its `send` is refused as busy, not deadlocked.
        return forkHost(handler(command.await), (answer) => Effect.gen(function* () {
          const fields = Schema.decodeUnknownSync(Schema.Record(Schema.String, Schema.Json))(answer.fields)
          const output = yield* options.session.send({ type: "bridgeResult", request, fields, ...(answer.writes === undefined ? {} : { writes: answer.writes }) })
          yield* playBatch(output)
          yield* offerEvent({ type: "bridge.answered" })
        }).pipe(Effect.catchCause(failed("bridge answer failed"))))
      }
      case "idle.flow": {
        const sendIdle = (input: Input) => sendInput(input).pipe(Effect.catchCause(failed("idle input failed")))
        // The idle flow runs on a fiber of its own (spec 0.41.0 §7.2): a host
        // `idle` that waits for the game never blocks actions or `send`.
        return forkHost(Effect.gen(function* () {
          const hostInput = options.host?.idle === undefined ? undefined : yield* options.host.idle(command.await)
          return hostInput !== undefined ? { _tag: "Input" as const, input: hostInput } : yield* options.session.idle
        }), (result) => {
          if (result?._tag === "Input") return sendIdle(result.input)
          if (result?._tag !== "Menu") return Effect.void
          idleOptions = result.options
          return options.presenter.showScreen({ _tag: "Idle", options: result.options.map(({ label }) => ({ label })) })
        })
      }
      case "mode.changed": return setMode(command.mode)
      case "save": return saveSlot(command.slot)
      case "load": return loadSlot(command.slot)
      case "delete": return options.saves.delete(command.slot).pipe(Effect.andThen(offerEvent({ type: "slot.occupied", slot: command.slot, occupied: false })))
      case "rollback":
        return Effect.gen(function* () {
          yield* interruptBatch()
          yield* interruptHost()
          const point = yield* options.session.rollback(1)
          // Input points past the rolled-back one are an abandoned branch.
          for (const inputPoint of readJournal.keys()) if (inputPoint > point.logLength) readJournal.delete(inputPoint)
          yield* presentStage(point.stage)
          yield* offerEvent({ type: "load.done", await: point.await })
        })
      case "screen.show": return options.presenter.showScreen(command.screen)
      case "screen.hide": return options.presenter.hideScreen
      case "overlay.show":
        return Effect.flatMap(overlayScreen(command.overlay, command.confirm), options.presenter.showScreen)
      case "overlay.hide": return options.presenter.hideScreen
      case "ui.hidden": return options.presenter.setUiHidden(command.hidden)
      case "settings.apply":
        return Effect.gen(function* () {
          yield* options.profile.updateSettings(command.settings)
          currentSettings = yield* SubscriptionRef.get(options.profile.settings)
          const running = batchCues
          const locale = localeOf(currentSettings)
          if (running !== undefined && running.locale !== locale) {
            running.cues = yield* factory.fromOutput(running.output, running.read, locale)
            running.locale = locale
          }
          yield* options.presenter.applySettings(currentSettings)
          yield* options.audio.setVolumes(currentSettings.volumes)
        })
      case "voice.replay": {
        const file = voiceFile(options.bindings, command.voiceKey)
        return file === undefined ? Effect.void : options.audio.play("voice", { file, loop: false, volume: 1, fade: 0, restart: true })
      }
      case "toTitle":
        return Effect.gen(function* () { yield* interruptBatch(); yield* stopTimers(); yield* interruptHost(); idleOptions = [] })
    }
  }

  const process = Effect.gen(function* () {
    while (!closed) {
      const item = yield* Queue.take(work)
      // A `send` / `restore` is answered once the machine took the batch
      // start or restored await and the view shows it.
      let answer: Effect.Effect<void> = Effect.void
      switch (item._tag) {
        case "event": {
          const event = item.event
          if (event.type === "pickIdle") {
            // The idle menu answers once: a repeated pick (double click, held
            // key) must not send a second input while the first one runs.
            const selected = idleOptions[event.index]
            if (selected !== undefined) yield* sendInput(selected.input).pipe(Effect.catchCause(failed("idle input failed")))
          } else if (event.type === "send") {
            yield* sendInput(event.input).pipe(Effect.catchCause((cause) => Effect.logWarning("send refused", cause)))
          } else if (event.type === "replayVoice") {
            yield* handleCommand({ type: "voice.replay", lineId: event.lineId, voiceKey: event.voiceKey }).pipe(Effect.catchCause(failed("voice replay failed")))
          } else if (event.type === "advance" && presenterPaced) {
            // The presenter takes the reader's confirm itself.
          } else actor.send(event)
          break
        }
        case "command":
          yield* handleCommand(item.command).pipe(Effect.catchCause(failed("director command failed")))
          break
        case "send": {
          const exit = yield* Effect.exit(sendInput(item.input))
          answer = Deferred.done(item.reply, exit).pipe(Effect.asVoid)
          break
        }
        case "restore": {
          const exit = yield* Effect.exit(restoreSave(item.save))
          answer = Deferred.done(item.reply, exit).pipe(Effect.asVoid)
          break
        }
        case "host":
          if (item.generation === hostGeneration) {
            hostFiber = undefined
            yield* item.apply.pipe(Effect.catchCause(failed("host hook failed")))
          }
          break
      }
      while (commandBuffer.length > 0) {
        const command = commandBuffer.shift()
        if (command !== undefined) yield* handleCommand(command).pipe(Effect.catchCause(failed("director command failed")))
      }
      yield* updateView
      yield* answer
    }
  })
  const processor = yield* Effect.forkScoped(process)
  yield* Effect.forkScoped(Stream.runForEach(options.presenter.actions, (action: PlayerAction) => offerEvent(action)))
  actor.start()
  yield* options.presenter.applySettings(currentSettings)
  yield* options.audio.setVolumes(currentSettings.volumes)
  const canContinue = (yield* options.saves.list).some(({ slot }) => slot === "quick" || slot === "auto")
  actor.send({ type: "boot.ready", canContinue, confirmsLines: options.presenter.confirmsLines === true })
  yield* updateView
  while (commandBuffer.length > 0) {
    const command = commandBuffer.shift()
    if (command !== undefined) yield* handleCommand(command).pipe(Effect.catchCause(failed("boot command failed")))
  }

  /** Hand `make` to the loop and wait for its reply; a closed director is busy. */
  const ask = <E>(make: (reply: Deferred.Deferred<void, E | DirectorBusy>) => Work): Effect.Effect<void, E | DirectorBusy> => Effect.gen(function* () {
    if (closed) return yield* Effect.fail(busy())
    const reply = yield* Deferred.make<void, E | DirectorBusy>()
    yield* Queue.offer(work, make(reply))
    return yield* Deferred.await(reply)
  })

  const shutdown = Effect.gen(function* () {
    if (closed) return
    closed = true
    yield* Fiber.interrupt(processor)
    // Questions the loop will not answer any more.
    for (const item of yield* Queue.clear(work)) {
      if (item._tag === "send") yield* Deferred.fail(item.reply, busy())
      else if (item._tag === "restore") yield* Deferred.fail(item.reply, busy())
    }
    yield* Queue.shutdown(work)
    actor.stop()
  })
  return {
    actions: (action: PlayerAction) => offerEvent(action),
    view,
    send: (input: Input) => ask<SendError>((reply) => ({ _tag: "send", input, reply })),
    restore: (save: SaveRuntime) => ask<RestoreError>((reply) => ({ _tag: "restore", save, reply })),
    // The session lock linearizes this with send/restore: a call made while a
    // batch is presenting returns the last committed point without waiting.
    snapshot: options.session.snapshot,
    shutdown
  } satisfies DirectorHandle
})

/** Layer constructor for hosts that already provide all player dependencies. */
export const layer = (options: DirectorOptions): Layer.Layer<Director, unknown> => Layer.effect(Director, make(options))
