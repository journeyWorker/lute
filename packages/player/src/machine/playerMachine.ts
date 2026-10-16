/** Pure interaction statechart for the Lute player. */
import { setup, types } from "xstate"
import type {
  Await,
  ConfirmAction,
  DirectorCommand,
  MachineEvent,
  PlayerMachineContext,
  PlayerView,
} from "../model.js"
import type { ScreenModel } from "../ports/index.js"

const eventSchemas = {
  advance: types<Extract<MachineEvent, { readonly type: "advance" }>>(),
  toggleAuto: types<Extract<MachineEvent, { readonly type: "toggleAuto" }>>(),
  toggleSkip: types<Extract<MachineEvent, { readonly type: "toggleSkip" }>>(),
  skipHold: types<Extract<MachineEvent, { readonly type: "skipHold" }>>(),
  rollback: types<Extract<MachineEvent, { readonly type: "rollback" }>>(),
  openBacklog: types<Extract<MachineEvent, { readonly type: "openBacklog" }>>(),
  openMenu: types<Extract<MachineEvent, { readonly type: "openMenu" }>>(),
  openSave: types<Extract<MachineEvent, { readonly type: "openSave" }>>(),
  openLoad: types<Extract<MachineEvent, { readonly type: "openLoad" }>>(),
  openSettings: types<Extract<MachineEvent, { readonly type: "openSettings" }>>(),
  quickSave: types<Extract<MachineEvent, { readonly type: "quickSave" }>>(),
  quickLoad: types<Extract<MachineEvent, { readonly type: "quickLoad" }>>(),
  hideUI: types<Extract<MachineEvent, { readonly type: "hideUI" }>>(),
  choose: types<Extract<MachineEvent, { readonly type: "choose" }>>(),
  close: types<Extract<MachineEvent, { readonly type: "close" }>>(),
  newGame: types<Extract<MachineEvent, { readonly type: "newGame" }>>(),
  continue: types<Extract<MachineEvent, { readonly type: "continue" }>>(),
  toTitle: types<Extract<MachineEvent, { readonly type: "toTitle" }>>(),
  saveSlot: types<Extract<MachineEvent, { readonly type: "saveSlot" }>>(),
  loadSlot: types<Extract<MachineEvent, { readonly type: "loadSlot" }>>(),
  deleteSlot: types<Extract<MachineEvent, { readonly type: "deleteSlot" }>>(),
  confirm: types<Extract<MachineEvent, { readonly type: "confirm" }>>(),
  pickIdle: types<Extract<MachineEvent, { readonly type: "pickIdle" }>>(),
  replayVoice: types<Extract<MachineEvent, { readonly type: "replayVoice" }>>(),
  setSettings: types<Extract<MachineEvent, { readonly type: "setSettings" }>>(),
  send: types<Extract<MachineEvent, { readonly type: "send" }>>(),
  "boot.ready": types<Extract<MachineEvent, { readonly type: "boot.ready" }>>(),
  "batch.started": types<Extract<MachineEvent, { readonly type: "batch.started" }>>(),
  "batch.settled": types<Extract<MachineEvent, { readonly type: "batch.settled" }>>(),
  "line.typing": types<Extract<MachineEvent, { readonly type: "line.typing" }>>(),
  "line.shown": types<Extract<MachineEvent, { readonly type: "line.shown" }>>(),
  "line.released": types<Extract<MachineEvent, { readonly type: "line.released" }>>(),
  "voice.ended": types<Extract<MachineEvent, { readonly type: "voice.ended" }>>(),
  "auto.elapsed": types<Extract<MachineEvent, { readonly type: "auto.elapsed" }>>(),
  "countdown.tick": types<Extract<MachineEvent, { readonly type: "countdown.tick" }>>(),
  "countdown.expired": types<Extract<MachineEvent, { readonly type: "countdown.expired" }>>(),
  "bridge.answered": types<Extract<MachineEvent, { readonly type: "bridge.answered" }>>(),
  "bridge.unhandled": types<Extract<MachineEvent, { readonly type: "bridge.unhandled" }>>(),
  "load.done": types<Extract<MachineEvent, { readonly type: "load.done" }>>(),
  "save.done": types<Extract<MachineEvent, { readonly type: "save.done" }>>(),
  "slots.listed": types<Extract<MachineEvent, { readonly type: "slots.listed" }>>(),
  "slot.occupied": types<Extract<MachineEvent, { readonly type: "slot.occupied" }>>(),
  failed: types<Extract<MachineEvent, { readonly type: "failed" }>>(),
}

type TransitionArgs = {
  readonly context: PlayerMachineContext
  readonly event: MachineEvent
}
type Enqueuer = { readonly emit: (command: { readonly type: string }) => void }
type TransitionResult = { readonly target?: string; readonly context?: Partial<PlayerMachineContext> }
type Transition = (args: TransitionArgs, enq: Enqueuer) => TransitionResult | void

const emit = (enq: Enqueuer, command: DirectorCommand): void => enq.emit(command)
const noOverlay = ({ context }: TransitionArgs): boolean => context.overlayStack.length === 0 && !context.uiHidden

const initialContext: PlayerMachineContext = {
  await: undefined,
  line: undefined,
  mode: "normal",
  skipMode: "read",
  overlayStack: [],
  countdownRemaining: undefined,
  uiHidden: false,
  lastLineRead: false,
  hasVoice: false,
  autoPending: false,
  canContinue: false,
  occupiedSlots: [],
  pendingConfirm: undefined,
  haltedMessage: undefined,
  confirmsLines: false,
  advanceBuffered: false,
}

/** The flow after a load, rollback or host `restore`, by the restored await. */
const flowLoadDone: Transition = ({ event }) => {
  if (event.type !== "load.done") return
  if (event.await.type === "ended") return { target: "#playerFlowEnded" }
  if (event.await.type === "halted") return { target: "#playerFlowError" }
  return { target: "#playerFlowPlaying" }
}

const overlayTarget = (stack: ReadonlyArray<PlayerMachineContext["overlayStack"][number]>): string => {
  const top = stack[stack.length - 1]
  if (top === undefined) return "#playerOverlayNone"
  if (top === "save" || top === "load") return top === "save" ? "#playerOverlaySave" : "#playerOverlayLoad"
  return `#playerOverlay${top.slice(0, 1).toUpperCase()}${top.slice(1)}`
}

const overlayOpen = (
  name: PlayerMachineContext["overlayStack"][number],
): Transition => ({ context }, enq) => {
  const stack = [...context.overlayStack, name]
  if (context.countdownRemaining !== undefined) emit(enq, { type: "countdown.pause" })
  emit(enq, { type: "overlay.show", overlay: name, confirm: undefined })
  return {
    target: overlayTarget(stack),
    context: { overlayStack: stack, uiHidden: false },
  }
}

const overlayClose: Transition = ({ context }, enq) => {
  const stack = context.overlayStack.slice(0, -1)
  const top = stack.at(-1)
  emit(enq, top === undefined ? { type: "overlay.hide" } : { type: "overlay.show", overlay: top, confirm: undefined })
  if (stack.length === 0 && context.countdownRemaining !== undefined) emit(enq, { type: "countdown.resume" })
  return {
    target: overlayTarget(stack),
    context: { overlayStack: stack, pendingConfirm: undefined },
  }
}

const hiddenInput: Transition = ({ context }) => ({
  target: overlayTarget(context.overlayStack),
  context: { uiHidden: false },
})

const showHalted = (message: string): ScreenModel => ({ _tag: "Halted", message })
const showEnded = (reason: string): ScreenModel => ({ _tag: "Ended", reason })

const awaitStoryTarget = (await_: Await): string => {
  if (await_.type === "awaitChoice") return await_.menu.timeout === undefined || await_.menu.timeout === null
    ? "#playerStoryChoiceOpen"
    : "#playerStoryChoiceTimed"
  if (await_.type === "awaitBridge") return "#playerStoryBridge"
  if (await_.type === "idle") return "#playerStoryIdle"
  if (await_.type === "ended") return "#playerStoryEnded"
  return "#playerStoryHalted"
}

const choiceSettled: Transition = ({ context, event }, enq) => {
  if (event.type !== "batch.settled" || event.await.type !== "awaitChoice") return
  const timed = event.await.menu.timeout !== undefined && event.await.menu.timeout !== null
  emit(enq, { type: "choice.show", await: event.await })
  if (timed) emit(enq, { type: "countdown.start", seconds: event.await.menu.timeout ?? 0 })
  return {
    target: timed ? "#playerStoryChoiceTimed" : "#playerStoryChoiceOpen",
    context: { await: event.await, countdownRemaining: timed ? event.await.menu.timeout ?? 0 : undefined },
  }
}

const settled: Transition = ({ event }, enq) => {
  if (event.type !== "batch.settled") return
  const await_ = event.await
  if (await_.type === "awaitChoice") return choiceSettled({ context: initialContext, event }, enq)
  if (await_.type === "awaitBridge") {
    emit(enq, { type: "bridge.call", await: await_ })
    return { target: "#playerStoryBridge", context: { await: await_ } }
  }
  if (await_.type === "idle") {
    emit(enq, { type: "idle.flow", await: await_ })
    return { target: "#playerStoryIdle", context: { await: await_ } }
  }
  if (await_.type === "ended") {
    emit(enq, { type: "screen.show", screen: showEnded(await_.reason) })
    return { target: "#playerStoryEnded", context: { await: await_ } }
  }
  emit(enq, { type: "screen.show", screen: showHalted(await_.message) })
  return { target: "#playerStoryHalted", context: { await: await_, haltedMessage: await_.message } }
}

const confirmFor = (action: ConfirmAction): Transition => ({ context }, enq) => {
  if (context.autoPending) emit(enq, { type: "auto.cancel" })
  if (context.countdownRemaining !== undefined && context.overlayStack.length === 0) emit(enq, { type: "countdown.pause" })
  emit(enq, { type: "overlay.show", overlay: "confirm", confirm: action })
  return {
    target: "#playerOverlayConfirm",
    context: { overlayStack: [...context.overlayStack, "confirm"], pendingConfirm: action, autoPending: false },
  }
}

const initial = setup({ schemas: { context: types<PlayerMachineContext>(), events: eventSchemas } })

/** The pure player progression statechart. */
export const playerMachine = initial.createMachine({
  id: "player",
  type: "parallel",
  context: initialContext,
  states: {
    flow: {
      initial: "boot",
      states: {
        boot: {
          on: {
            "boot.ready": ({ event }, enq) => {
              emit(enq, { type: "screen.show", screen: { _tag: "Title", canContinue: event.canContinue, background: undefined } })
              return { target: "title", context: { canContinue: event.canContinue, confirmsLines: event.confirmsLines === true } }
            },
            failed: ({ event }, enq) => {
              emit(enq, { type: "screen.show", screen: showHalted(event.message) })
              return { target: "error", context: { haltedMessage: event.message } }
            },
          },
        },
        title: {
          on: {
            newGame: ({ context }, enq) => {
              if (context.uiHidden) return
              emit(enq, { type: "session.newGame" })
              return { target: "loading" }
            },
            continue: ({ context }, enq) => {
              if (context.uiHidden) return
              emit(enq, { type: "session.continue" })
              return { target: "loading" }
            },
            failed: ({ event }, enq) => {
              emit(enq, { type: "screen.show", screen: showHalted(event.message) })
              return { target: "error", context: { haltedMessage: event.message } }
            },
            // A host `restore` from the title screen (spec 0.41.0 §7.4).
            "load.done": flowLoadDone,
          },
        },
        loading: {
          on: {
            "batch.started": { target: "playing" },
            "load.done": flowLoadDone,
            failed: ({ event }, enq) => {
              emit(enq, { type: "screen.show", screen: showHalted(event.message) })
              return { target: "error", context: { haltedMessage: event.message } }
            },
          },
        },
        playing: {
          id: "playerFlowPlaying",
          on: {
            confirm: ({ context, event }) =>
              event.yes && context.pendingConfirm?.type === "toTitle" ? { target: "title" } : undefined,
            "batch.settled": ({ event }: { readonly event: MachineEvent }) => {
              if (event.type !== "batch.settled") return
              if (event.await.type === "ended") return { target: "ended" }
              if (event.await.type === "halted") return { target: "error" }
            },
            "load.done": flowLoadDone,
            failed: ({ event }, enq) => {
              emit(enq, { type: "screen.show", screen: showHalted(event.message) })
              return { target: "error", context: { haltedMessage: event.message } }
            },
          },
        },
        ended: {
          id: "playerFlowEnded",
          on: {
            // An input the host sends at `ended` (`newRun`, an outside-run raise) plays on.
            "batch.started": { target: "playing" },
            "load.done": flowLoadDone,
            confirm: ({ context, event }, enq) => {
              if (!event.yes || context.pendingConfirm?.type !== "toTitle") return
              emit(enq, { type: "screen.show", screen: { _tag: "Title", canContinue: context.canContinue, background: undefined } })
              return { target: "title", context: { pendingConfirm: undefined, overlayStack: [] } }
            },
            // Spec 0.47.0 §2.5: leaving an ended run discards nothing — no confirm.
            // An open overlay (the in-game menu) keeps its confirm.
            toTitle: ({ context }, enq) => {
              if (context.pendingConfirm !== undefined) return
              if (context.overlayStack.length > 0) return confirmFor({ type: "toTitle" })({ context, event: { type: "toTitle" } }, enq)
              emit(enq, { type: "toTitle" })
              emit(enq, { type: "screen.show", screen: { _tag: "Title", canContinue: context.canContinue, background: undefined } })
              return { target: "title" }
            },
          },
        },
        error: {
          id: "playerFlowError",
          on: {
            "load.done": flowLoadDone,
            confirm: ({ context, event }, enq) => {
              if (!event.yes || context.pendingConfirm?.type !== "toTitle") return
              emit(enq, { type: "screen.show", screen: { _tag: "Title", canContinue: context.canContinue, background: undefined } })
              return { target: "title", context: { pendingConfirm: undefined, overlayStack: [] } }
            },
            toTitle: ({ context }, enq) => context.pendingConfirm === undefined
              ? confirmFor({ type: "toTitle" })({ context, event: { type: "toTitle" } }, enq)
              : undefined,
          },
        },
      },
    },
    story: {
      initial: "running",
      states: {
        running: {
          id: "playerStoryRunning",
          on: {
            // A new batch leaves the previous await (an `ended` must not outlive it).
            "batch.started": () => ({ target: "#playerStoryRunning", context: { advanceBuffered: false, await: undefined } }),
            "line.typing": ({ event }) => ({
              target: "#playerStoryTyping",
              context: { line: "typing", lastLineRead: event.read, hasVoice: event.hasVoice, advanceBuffered: false },
            }),
            "line.shown": ({ event }) => ({
              target: "#playerStoryShown",
              context: { line: "shown", lastLineRead: event.read, hasVoice: event.hasVoice },
            }),
            "batch.settled": settled,
          },
        },
        typing: {
          id: "playerStoryTyping",
          on: {
            advance: ({ context }, enq) => {
              if (context.overlayStack.length !== 0 || context.uiHidden) return
              // A presenter that confirms lines itself: keep the confirm for the release.
              if (context.confirmsLines) return { context: { advanceBuffered: true } }
              emit(enq, { type: "line.reveal" })
              return { target: "#playerStoryShown" }
            },
            "auto.elapsed": ({ context }, enq) => {
              if (context.overlayStack.length !== 0 || context.uiHidden) return
              emit(enq, { type: "line.release" })
              return { target: "#playerStoryRunning", context: { line: undefined, autoPending: false } }
            },
            rollback: ({ context }, enq) => {
              if (context.overlayStack.length !== 0 || context.uiHidden) return
              emit(enq, { type: "rollback" })
            },
            "line.shown": ({ context, event }, enq) => {
              if (context.advanceBuffered && context.mode !== "skip") {
                emit(enq, { type: "line.release" })
                return { target: "#playerStoryRunning", context: { line: undefined, autoPending: false, advanceBuffered: false, lastLineRead: event.read, hasVoice: event.hasVoice } }
              }
              return { target: "#playerStoryShown", context: { line: "shown", lastLineRead: event.read, hasVoice: event.hasVoice, advanceBuffered: false } }
            },
          },
        },
        shown: {
          id: "playerStoryShown",
          on: {
            advance: ({ context }, enq) => {
              if (context.overlayStack.length !== 0 || context.uiHidden) return
              emit(enq, { type: "line.release" })
              return { target: "#playerStoryRunning", context: { line: undefined, autoPending: false } }
            },
            "auto.elapsed": ({ context }, enq) => {
              if (context.overlayStack.length !== 0 || context.uiHidden) return
              emit(enq, { type: "line.release" })
              return { target: "#playerStoryRunning", context: { line: undefined, autoPending: false } }
            },
            "line.released": ({ context }) => ({ target: "#playerStoryRunning", context: { line: undefined, autoPending: false } }),
            rollback: ({ context }, enq) => {
              if (context.overlayStack.length !== 0 || context.uiHidden) return
              emit(enq, { type: "rollback" })
            },
          },
        },
        choice: {
          initial: "open",
          states: {
            open: { id: "playerStoryChoiceOpen", on: {} },
            timed: {
              id: "playerStoryChoiceTimed",
              on: {
                "countdown.tick": ({ event }) => ({ context: { countdownRemaining: event.remaining } }),
                "countdown.expired": ({ context }, enq) => {
                  if (context.await?.type !== "awaitChoice") return
                  emit(enq, { type: "countdown.cancel" })
                  emit(enq, { type: "choice.hide" })
                  emit(enq, { type: "session.send", input: { type: "timeout", request: context.await.request } })
                  return { target: "#playerStoryRunning", context: { countdownRemaining: undefined, await: undefined } }
                },
              },
            },
          },
          on: {
            choose: ({ context, event }, enq) => {
              if (context.overlayStack.length !== 0 || context.uiHidden || context.await?.type !== "awaitChoice") return
              if (context.countdownRemaining !== undefined) emit(enq, { type: "countdown.cancel" })
              emit(enq, { type: "choice.hide" })
              emit(enq, { type: "session.send", input: { type: "choose", option: event.option, request: context.await.request } })
              return { target: "#playerStoryRunning", context: { await: undefined, countdownRemaining: undefined } }
            },
            rollback: ({ context }, enq) => {
              if (context.overlayStack.length !== 0 || context.uiHidden) return
              emit(enq, { type: "rollback" })
            },
          },
        },
        bridge: {
          id: "playerStoryBridge",
          on: {
            "bridge.answered": { target: "#playerStoryRunning" },
            "bridge.unhandled": ({ event }, enq) => {
              emit(enq, { type: "screen.show", screen: showHalted(`Bridge '${event.tag}' is unhandled`) })
              return { target: "#playerStoryHalted", context: { haltedMessage: `Bridge '${event.tag}' is unhandled` } }
            },
          },
        },
        idle: {
          id: "playerStoryIdle",
          on: {
            "batch.settled": settled,
            rollback: ({ context }, enq) => {
              if (context.overlayStack.length !== 0 || context.uiHidden) return
              emit(enq, { type: "rollback" })
            },
          },
        },
        ended: { id: "playerStoryEnded", on: { toTitle: { target: "#playerStoryRunning" }, confirm: { target: "#playerStoryRunning" } } },
        halted: { id: "playerStoryHalted", on: { toTitle: { target: "#playerStoryRunning" }, confirm: { target: "#playerStoryRunning" } } },
      },
      on: {
        "batch.started": () => ({ target: "#playerStoryRunning", context: { advanceBuffered: false, await: undefined } }),
        // Acknowledgements the director sends in every state; only some states act on them.
        "line.released": () => undefined,
        "save.done": () => undefined,
        "batch.settled": settled,
        "load.done": ({ event }, enq) => {
          const target = awaitStoryTarget(event.await)
          if (event.await.type === "awaitChoice") {
            emit(enq, { type: "choice.show", await: event.await })
            if (event.await.menu.timeout !== undefined && event.await.menu.timeout !== null) emit(enq, { type: "countdown.start", seconds: event.await.menu.timeout })
          }
          if (event.await.type === "awaitBridge") emit(enq, { type: "bridge.call", await: event.await })
          if (event.await.type === "idle") emit(enq, { type: "idle.flow", await: event.await })
          if (event.await.type === "ended") emit(enq, { type: "screen.show", screen: showEnded(event.await.reason) })
          if (event.await.type === "halted") emit(enq, { type: "screen.show", screen: showHalted(event.await.message) })
          return {
            target,
            context: {
              await: event.await, line: undefined, countdownRemaining: event.await.type === "awaitChoice" ? event.await.menu.timeout ?? undefined : undefined,
              autoPending: false, advanceBuffered: false, haltedMessage: event.await.type === "halted" ? event.await.message : undefined
            },
          }
        },
      },
    },
    mode: {
      initial: "normal",
      states: {
        normal: {
          id: "playerModeNormal",
          on: {
            toggleAuto: ({ context }, enq) => {
              if (context.uiHidden) return
              emit(enq, { type: "mode.changed", mode: "auto" })
              if (context.line === "shown") emit(enq, { type: "auto.start", waitVoice: context.hasVoice })
              return { target: "#playerModeAuto", context: { mode: "auto", autoPending: context.line === "shown" } }
            },
            toggleSkip: ({ context }, enq) => {
              if (context.uiHidden) return
              const hasCurrentLine = context.line !== undefined
              if (context.skipMode === "read" && hasCurrentLine && !context.lastLineRead) {
                return { target: "#playerModeNormal", context: { mode: "normal" } }
              }
              emit(enq, { type: "mode.changed", mode: "skip" })
              if (hasCurrentLine) emit(enq, { type: "line.release" })
              return {
                target: context.skipMode === "all" ? "#playerModeSkipAll" : "#playerModeSkipRead",
                context: { mode: "skip" },
              }
            },
            skipHold: ({ context, event }, enq) => {
              if (context.uiHidden || !event.held) return
              emit(enq, { type: "mode.changed", mode: "skip" })
              return { target: "#playerModeSkipHold", context: { mode: "skip" } }
            },
          },
        },
        auto: {
          id: "playerModeAuto",
          on: {
            toggleAuto: ({ context }, enq) => {
              if (context.uiHidden) return
              emit(enq, { type: "auto.cancel" })
              emit(enq, { type: "mode.changed", mode: "normal" })
              return { target: "#playerModeNormal", context: { mode: "normal", autoPending: false } }
            },
            toggleSkip: ({ context }, enq) => {
              if (context.uiHidden) return
              emit(enq, { type: "auto.cancel" })
              const hasCurrentLine = context.line !== undefined
              if (context.skipMode === "read" && hasCurrentLine && !context.lastLineRead) {
                emit(enq, { type: "mode.changed", mode: "normal" })
                return { target: "#playerModeNormal", context: { mode: "normal", autoPending: false } }
              }
              emit(enq, { type: "mode.changed", mode: "skip" })
              if (hasCurrentLine) emit(enq, { type: "line.release" })
              return { target: context.skipMode === "all" ? "#playerModeSkipAll" : "#playerModeSkipRead", context: { mode: "skip", autoPending: false } }
            },
            "line.shown": ({ context, event }, enq) => {
              // A buffered confirm releases the line at once (story region, same event): no auto timer.
              if (context.advanceBuffered) return { context: { lastLineRead: event.read, hasVoice: event.hasVoice } }
              emit(enq, { type: "auto.start", waitVoice: event.hasVoice })
              return { context: { autoPending: true, lastLineRead: event.read, hasVoice: event.hasVoice } }
            },
            "auto.elapsed": ({ context }) => context.autoPending ? { context: { autoPending: false } } : undefined,
            "*": ({ context }, enq) => {
              if (context.uiHidden || !context.autoPending) return
              emit(enq, { type: "auto.cancel" })
              return { context: { autoPending: false } }
            },
          },
        },
        skip: {
          initial: "read",
          states: {
            read: {
              id: "playerModeSkipRead",
              on: {
                "line.shown": ({ event }, enq) => {
                  if (event.read) {
                    emit(enq, { type: "line.release" })
                    return { context: { lastLineRead: true, line: "shown" } }
                  }
                  emit(enq, { type: "mode.changed", mode: "normal" })
                  return { target: "#playerModeNormal", context: { mode: "normal", lastLineRead: false } }
                },
              },
            },
            all: {
              id: "playerModeSkipAll",
              on: {
                "line.shown": ({ event }, enq) => {
                  emit(enq, { type: "line.release" })
                  return { context: { lastLineRead: event.read, line: "shown" } }
                },
              },
            },
          },
          on: {
            toggleSkip: (_args, enq) => {
              emit(enq, { type: "mode.changed", mode: "normal" })
              return { target: "#playerModeNormal", context: { mode: "normal" } }
            },
            "batch.settled": ({ event }, enq) => {
              if (event.await.type === "awaitChoice" || event.await.type === "awaitBridge" || event.await.type === "idle" || event.await.type === "ended" || event.await.type === "halted") {
                emit(enq, { type: "mode.changed", mode: "normal" })
                return { target: "#playerModeNormal", context: { mode: "normal" } }
              }
            },
          },
        },
        skipHold: {
          id: "playerModeSkipHold",
          on: {
            skipHold: ({ event }, enq) => {
              if (event.held) return
              emit(enq, { type: "mode.changed", mode: "normal" })
              return { target: "#playerModeNormal", context: { mode: "normal" } }
            },
            "line.shown": ({ event }, enq) => {
              emit(enq, { type: "line.release" })
              return { context: { lastLineRead: event.read, line: "shown" } }
            },
            "batch.settled": ({ event }, enq) => {
              if (event.await.type === "awaitChoice" || event.await.type === "awaitBridge" || event.await.type === "idle" || event.await.type === "ended" || event.await.type === "halted") {
                emit(enq, { type: "mode.changed", mode: "normal" })
                return { target: "#playerModeNormal", context: { mode: "normal" } }
              }
            },
          },
        },
      },
      on: {
        setSettings: ({ context, event }, enq) => {
          emit(enq, { type: "settings.apply", settings: event.settings })
          return { context: { skipMode: event.settings.skipMode ?? context.skipMode } }
        },
        "load.done": (_args, enq) => {
          emit(enq, { type: "mode.changed", mode: "normal" })
          return { target: "#playerModeNormal", context: { mode: "normal", autoPending: false } }
        },
      },
    },
    overlay: {
      initial: "none",
      states: {
        none: {
          id: "playerOverlayNone",
          on: {
            openMenu: overlayOpen("menu"),
            openBacklog: overlayOpen("backlog"),
            openSave: overlayOpen("save"),
            openLoad: overlayOpen("load"),
            openSettings: overlayOpen("settings"),
            quickSave: (_args, enq) => {
              emit(enq, { type: "save", slot: "quick" })
            },
            quickLoad: ({ context }, enq) => {
              if (context.overlayStack.length > 0) return
              return confirmFor({ type: "load", slot: "quick" })({ context, event: { type: "quickLoad" } }, enq)
            },
            hideUI: ({ context }, enq) => {
              const hidden = !context.uiHidden
              emit(enq, { type: "ui.hidden", hidden })
              return { target: hidden ? "#playerOverlayHidden" : "#playerOverlayNone", context: { uiHidden: hidden } }
            },
            toTitle: ({ context }, enq) => {
              // At `ended` the flow leaves for the title without a confirm (spec 0.47.0 §2.5).
              if (context.overlayStack.length !== 0 || context.await?.type === "ended") return
              return confirmFor({ type: "toTitle" })({ context, event: { type: "toTitle" } }, enq)
            },
          },
        },
        menu: {
          id: "playerOverlayMenu",
          on: {
            close: overlayClose,
            toTitle: ({ context }, enq) => confirmFor({ type: "toTitle" })({ context, event: { type: "toTitle" } }, enq),
            openBacklog: overlayOpen("backlog"),
            openSave: overlayOpen("save"),
            openLoad: overlayOpen("load"),
            openSettings: overlayOpen("settings"),
          },
        },
        backlog: { id: "playerOverlayBacklog", on: { close: overlayClose, openMenu: overlayOpen("menu") } },
        saveLoad: {
          initial: "save",
          states: {
            save: { id: "playerOverlaySave", on: { close: overlayClose, openLoad: overlayOpen("load"), openSettings: overlayOpen("settings") } },
            load: { id: "playerOverlayLoad", on: { close: overlayClose, openSave: overlayOpen("save"), openSettings: overlayOpen("settings") } },
          },
          on: {
            saveSlot: ({ context, event }, enq) => context.occupiedSlots.includes(event.slot)
              ? confirmFor({ type: "overwrite", slot: event.slot })({ context, event }, enq)
              : (emit(enq, { type: "save", slot: event.slot }), undefined),
            loadSlot: ({ context, event }, enq) => confirmFor({ type: "load", slot: event.slot })({ context, event }, enq),
            deleteSlot: ({ context, event }, enq) => confirmFor({ type: "delete", slot: event.slot })({ context, event }, enq),
            "slots.listed": () => undefined,
          },
        },
        settings: { id: "playerOverlaySettings", on: { close: overlayClose, openMenu: overlayOpen("menu") } },
        confirm: {
          id: "playerOverlayConfirm",
          on: {
            close: overlayClose,
            confirm: ({ context, event }, enq) => {
              const action = context.pendingConfirm
              if (!event.yes || action === undefined) return overlayClose({ context, event }, enq)
              emit(enq, { type: "overlay.hide" })
              if (action.type === "overwrite") emit(enq, { type: "save", slot: action.slot })
              if (action.type === "load") emit(enq, { type: "load", slot: action.slot })
              if (action.type === "delete") emit(enq, { type: "delete", slot: action.slot })
              if (action.type === "toTitle") {
                emit(enq, { type: "toTitle" })
                emit(enq, { type: "screen.show", screen: { _tag: "Title", canContinue: context.canContinue, background: undefined } })
              }
              return { target: "#playerOverlayNone", context: { overlayStack: [], pendingConfirm: undefined } }
            },
          },
        },
        hidden: {
          id: "playerOverlayHidden",
          on: { "*": hiddenInput },
        },
      },
      on: {
        "slot.occupied": ({ context, event }) => {
          const occupiedSlots = event.occupied
            ? context.occupiedSlots.includes(event.slot) ? context.occupiedSlots : [...context.occupiedSlots, event.slot]
            : context.occupiedSlots.filter((slot) => slot !== event.slot)
          // Continue resumes the quick or auto slot.
          const resumable = occupiedSlots.some((slot) => slot === "quick" || slot === "auto")
          return { context: { occupiedSlots, canContinue: context.canContinue || resumable } }
        },
        "load.done": ({ event }) => ({
          target: "#playerOverlayNone",
          context: { await: event.await, mode: "normal", overlayStack: [], uiHidden: false, line: undefined, countdownRemaining: event.await.type === "awaitChoice" ? event.await.menu.timeout ?? undefined : undefined, autoPending: false, pendingConfirm: undefined },
        }),
      },
    },
  },
})


type PlayerStateValue = {
  readonly flow: unknown
  readonly story: unknown
  readonly mode: unknown
  readonly overlay: unknown
}

const leaf = (value: unknown): string | undefined => typeof value === "string" ? value : undefined

const storyLeaf = (value: unknown): string | undefined => {
  const direct = leaf(value)
  if (direct !== undefined) return direct
  if (typeof value === "object" && value !== null && "choice" in value) return "choice"
  return undefined
}

const modeLeaf = (value: unknown): string | undefined => {
  const direct = leaf(value)
  if (direct !== undefined) return direct
  if (typeof value === "object" && value !== null && "skip" in value) return "skip"
  return undefined
}

const overlayLeaf = (value: unknown): PlayerView["overlay"] | undefined => {
  const direct = leaf(value)
  if (direct === "menu" || direct === "backlog" || direct === "settings" || direct === "confirm" || direct === "hidden") return direct
  if (direct === "saveLoad") return "saveLoad"
  if (typeof value === "object" && value !== null && "saveLoad" in value) return "saveLoad"
  return undefined
}

/** Derive the small immutable view consumed by a director or HUD. */
export const derivePlayerView = (snapshot: {
  readonly value: unknown
  readonly context: PlayerMachineContext
}): PlayerView => {
  const value = snapshot.value
  const regions: PlayerStateValue = typeof value === "object" && value !== null && "flow" in value && "story" in value && "mode" in value && "overlay" in value
    ? value
    : { flow: undefined, story: undefined, mode: undefined, overlay: undefined }
  const flowValue = leaf(regions.flow)
  const storyValue = storyLeaf(regions.story)
  const modeValue = modeLeaf(regions.mode)
  const overlayValue = overlayLeaf(regions.overlay)
  return {
    flow: flowValue === "title" || flowValue === "loading" || flowValue === "playing" || flowValue === "ended" || flowValue === "error" ? flowValue : "boot",
    story: storyValue ?? "running",
    mode: modeValue === "auto" || modeValue === "skip" ? modeValue : snapshot.context.mode,
    overlay: overlayValue ?? (snapshot.context.overlayStack.length > 0 ? "saveLoad" : "none"),
    countdownRemaining: snapshot.context.countdownRemaining,
    uiHidden: snapshot.context.uiHidden,
  }
}

/** Alias retained for callers that name the projection as `playerView`. */
export const playerView = derivePlayerView

/** Public snapshot shape accepted by view derivation. */
export interface PlayerMachineSnapshot {
  readonly value: unknown
  readonly context: PlayerMachineContext
}
/** The concrete machine type for actor consumers. */
export type PlayerMachine = typeof playerMachine
