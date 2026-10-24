/**
 * A recording host for `LutePlayer.make` over a compiled fixture (spec
 * 0.47.0 §6): what the player drew — lines, choices, screens — and a driver
 * that reads on until the story waits.
 */
import { Effect, Layer, Stream, SubscriptionRef } from "effect"
import * as KeyValueStore from "effect/persistence/KeyValueStore"
import * as InputLog from "../../runtime/src/InputLog.js"
import * as LuteBun from "../../runtime/src/bun.js"
import type { Bindings } from "../src/bindings/schema.js"
import { LutePlayer } from "../src/LutePlayer.js"
import type { LutePlayerHandle, LutePlayerStart } from "../src/LutePlayer.js"
import type { AssetLoader, AudioPort, ChoiceView, LineView, Presenter, ScreenModel } from "../src/ports/index.js"
import type { PlayerView } from "../src/model.js"
import type { Autosave } from "../src/stores/autosave.js"
import { flush } from "./director/support.js"

export const quayBindings: Bindings = { bindingsVersion: "0.49", canvas: { width: 1920, height: 1080 }, flow: { newGame: { occasion: "visit" } } }

/** The services `LutePlayer.make` needs over `bundle`; one store shared by every player built under it (a reload). */
export const hostLayer = (bundle: string) => Layer.mergeAll(LuteBun.runtime, LuteBun.bundle(bundle), InputLog.memory, KeyValueStore.layerMemory)

export interface Recorder {
  readonly lines: Array<LineView>
  readonly choices: Array<ChoiceView>
  readonly screens: Array<ScreenModel>
  /** The dialogue window in call order: `line <text>` per `showLine`, `clear` per `clearLine`. */
  readonly dialogue: Array<string>
  /** Every stage cue (`stage <tag>`), audio call (`play <channel> <file>`, `stop <channel>`) and shown choice (`choice`), in call order. */
  readonly effects: Array<string>
  readonly presenter: Presenter
  readonly audio: AudioPort
}

export const recorder = (): Recorder => {
  const lines: Array<LineView> = []
  const choices: Array<ChoiceView> = []
  const screens: Array<ScreenModel> = []
  const dialogue: Array<string> = []
  const effects: Array<string> = []
  const presenter: Presenter = {
    stage: (cue) => Effect.sync(() => { effects.push(`stage ${cue._tag}`) }),
    showLine: (line) => Effect.sync(() => {
      lines.push(line)
      dialogue.push(`line ${textOf(line)}`)
    }),
    revealLine: Effect.void,
    clearLine: Effect.sync(() => { dialogue.push("clear") }),
    restore: () => Effect.void,
    // The reader answers through `actions({ type: "choose" })`.
    showChoice: (choice) => Effect.andThen(Effect.sync(() => { choices.push(choice); effects.push("choice") }), Effect.never),
    hideChoice: Effect.void,
    showScreen: (screen) => Effect.sync(() => { screens.push(screen) }),
    hideScreen: Effect.void,
    setUiHidden: () => Effect.void,
    applySettings: () => Effect.void,
    capture: Effect.succeed(""),
    actions: Stream.empty
  }
  const audio: AudioPort = {
    play: (channel, play) => Effect.sync(() => { effects.push(`play ${channel} ${play.file}`) }),
    stop: (channel) => Effect.sync(() => { effects.push(`stop ${channel}`) }),
    setVolumes: () => Effect.void
  }
  return { lines, choices, screens, dialogue, effects, presenter, audio }
}

const assets: AssetLoader = { ensure: () => Effect.void, progress: Stream.empty }

/** A player over the host layer's bundle, drawing into `recorded`, with `bindings` (the quay fixture's by default). */
export const player = (recorded: Recorder, start: LutePlayerStart & { readonly autosave?: Autosave } = {}, bindings: Bindings = quayBindings) =>
  LutePlayer.make({ ...start, bindings, presenter: recorded.presenter, audio: recorded.audio, assets })

/** Act, then advance through lines until the story waits at a choice, `idle`, `ended` or `halted`. */
export const act = (handle: LutePlayerHandle, action: Parameters<LutePlayerHandle["actions"]>[0]) => Effect.gen(function* () {
  yield* handle.actions(action)
  for (let i = 0; i < 50; i++) {
    yield* flush
    const view: PlayerView = yield* SubscriptionRef.get(handle.view)
    if (view.overlay !== "none" || view.flow === "title") return view
    if (view.story === "choice" || view.story === "idle" || view.story === "ended" || view.story === "halted") return view
    yield* handle.actions({ type: "advance" })
  }
  throw new Error("the story never waits")
})

/** The text of a recorded line. */
export const textOf = (line: LineView): string =>
  line.segments.map((segment) => segment._tag === "Text" ? segment.text : "").join("")

/** A state value of the runtime session's current state. */
export const stateOf = (handle: LutePlayerHandle, path: string) => Effect.gen(function* () {
  const { state } = yield* SubscriptionRef.get(handle.session.current)
  return (yield* handle.program.view(state)).state[path]
})
