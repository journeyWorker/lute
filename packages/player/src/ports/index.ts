/**
 * The seams between the player core and a renderer/host (spec 0.40.0 §6–§7,
 * 0.41.0 §7, docs/player/architecture.md). `@lute-lang/player-web` implements
 * these with PixiJS and Web Audio; an engine host implements them with its own
 * renderer. Every operation is an Effect that completes when its effect has
 * finished.
 */
import type { Effect, Scope, Stream } from "effect"
import { Schema } from "effect"
import type { Contract, Ir } from "@lute-lang/runtime"
import type { Stage, MenuAwait, BridgeAwait, Input, Await } from "../model.js"

/** One run of line text, or a pause, after locale and style resolution. */
export type Segment =
  | { readonly _tag: "Text"; readonly text: string; readonly styles: ReadonlyArray<string>; readonly rate?: number }
  | { readonly _tag: "Pause"; readonly seconds: number }

/** A line as the reader sees it. */
export interface LineView {
  readonly lineId: string
  readonly voiceKey: string
  readonly role: string
  /** Cast id of the speaker (`narrator` for narration). */
  readonly speaker: string
  /** Display name from the bundle index `cast` (`{ id: name }`); empty for narration. */
  readonly speakerName: string
  readonly segments: ReadonlyArray<Segment>
  /** Already in the read history. */
  readonly read: boolean
  /** The line command's `emotion`, when authored. */
  readonly emotion?: string
  /** The line command's `variant`, when authored. */
  readonly variant?: string
}

/** How a cue is played: normally, or snapped to its end state (skip). */
export type PlayMode = "normal" | "skip"

/** A presenter-facing staging operation decoded from a runtime record. */
export type StageCue =
  | { readonly _tag: "Background"; readonly bg: Stage["bg"]; readonly transition: EffectSpec | undefined; readonly duration: number | undefined }
  | { readonly _tag: "ActorShow"; readonly actor: Stage["actors"][number]; readonly action: EffectSpec | undefined; readonly duration: number | undefined }
  | { readonly _tag: "ActorExit"; readonly character: string; readonly action: EffectSpec | undefined; readonly duration: number | undefined }
  | { readonly _tag: "Cg"; readonly cg: Stage["cg"]; readonly transition: EffectSpec | undefined; readonly duration: number | undefined }
  | { readonly _tag: "Video"; readonly video: Stage["video"]; readonly blocking: boolean }
  | { readonly _tag: "Camera"; readonly focus: string | undefined; readonly framing: string | undefined; readonly move: EffectSpec | undefined; readonly transition: EffectSpec | undefined; readonly duration: number | undefined }
  | { readonly _tag: "Vfx"; readonly effect: EffectSpec; readonly duration: number | undefined }
  | { readonly _tag: "Sequence"; readonly name: string; readonly video: string | undefined }

/**
 * A named effect with its bound parameters: a player catalogue effect, or
 * `effect: "custom"` (a `vfx` with a custom binding or no binding), which the
 * presenter resolves by `name` (spec 0.41.0 §7.7).
 */
export interface EffectSpec {
  readonly effect: string
  readonly params: Readonly<Record<string, number | string | boolean>>
  /** Present exactly when `effect` is `"custom"`: the presenter-defined effect's name. */
  readonly name?: string
}

/** Authored and runtime-resolved choice data for presenters. */
export interface ChoiceView {
  readonly id: string
  readonly construct: "branch" | "hub"
  readonly prompt: string | undefined
  readonly timeout: number | undefined
  readonly default: string | undefined
  readonly options: ReadonlyArray<{
    readonly id: string
    readonly text: string
    readonly verdict: string
    readonly once: boolean
    readonly exit: boolean
  }>
}

/** Rendering and interaction seam. */
export interface Presenter {
  /** Play a staging cue; completes when its visual effect finished (at once in skip mode). */
  readonly stage: (cue: StageCue, mode: PlayMode) => Effect.Effect<void>
  /**
   * Show and type a line. With the default `linePacing: "director"` it
   * completes when the line is fully shown; with `linePacing: "presenter"` it
   * completes when the reader released the line (the presenter types, pages
   * and takes the confirm itself). `mode: "skip"` completes at once.
   */
  readonly showLine: (line: LineView, mode: PlayMode) => Effect.Effect<void>
  /** Finish typing the current line at once. */
  readonly revealLine: Effect.Effect<void>
  /** Remove the current line from the dialogue window. */
  readonly clearLine: Effect.Effect<void>
  /** Rebuild the whole stage at once (load, rollback, presentation start). */
  readonly restore: (stage: Stage) => Effect.Effect<void>
  /** Show a choice menu; `remaining` streams seconds left for a timed choice. Resolves with the chosen option id. */
  readonly showChoice: (choice: ChoiceView, remaining: Stream.Stream<number>) => Effect.Effect<string>
  readonly hideChoice: Effect.Effect<void>
  /** Render a screen/overlay model (title, backlog, save/load, settings, confirm, loading, halted, idle menu). */
  readonly showScreen: (screen: ScreenModel) => Effect.Effect<void>
  readonly hideScreen: Effect.Effect<void>
  /** Hide or show the whole game UI (dialogue window, HUD). */
  readonly setUiHidden: (hidden: boolean) => Effect.Effect<void>
  /** Apply display settings (text speed, window opacity). */
  readonly applySettings: (settings: Settings) => Effect.Effect<void>
  /** Current frame as a PNG data URL (save thumbnails). */
  readonly capture: Effect.Effect<string>
  /** Player actions produced by on-screen widgets (buttons, menus, slots). */
  readonly actions: Stream.Stream<PlayerAction>
  /**
   * The presenter takes the reader's confirm itself: with the default
   * director pacing, an `advance` that arrives while the line is typing is
   * kept and applied as the release once the line is shown, instead of
   * revealing it (spec 0.41.0 §7.5).
   */
  readonly confirmsLines?: boolean
}

export type AudioChannel = "music" | "sfx" | "voice"

export interface AudioPlay {
  /** A bound file path (resolved from bindings). */
  readonly file: string
  readonly loop: boolean
  /** 0–1, multiplied with the channel volume. */
  readonly volume: number
  /** Crossfade/fade-in seconds (music), or fade-in seconds. */
  readonly fade: number
  /**
   * Music: play the file from its start even when it is already playing. Without
   * `restart`, playing the music file that already plays only changes its
   * volume (fading over `fade`). One-shot channels always start the sound.
   */
  readonly restart: boolean
}

/** Sound seam. `play` completes when the sound ended (looping music: when it started). */
export interface AudioPort {
  readonly play: (channel: AudioChannel, sound: AudioPlay) => Effect.Effect<void>
  readonly stop: (channel: AudioChannel, fade: number) => Effect.Effect<void>
  readonly setVolumes: (volumes: Volumes) => Effect.Effect<void>
}

/** Asset seam; keys are bound file paths. */
export interface AssetLoader {
  /** Load `files`; the assets stay loaded while the scope is open (refcounted). */
  readonly ensure: (files: ReadonlyArray<string>) => Effect.Effect<void, AssetMissing, Scope.Scope>
  /** Loading progress 0–1 of the current `ensure`. */
  readonly progress: Stream.Stream<number>
}

export class AssetMissing extends Schema.TaggedError<AssetMissing>()("AssetMissing", {
  file: Schema.String,
  message: Schema.String
}) {}

/** Device-independent input: the only way a reader acts on the player. */
export type PlayerAction =
  | { readonly type: "advance" }
  | { readonly type: "toggleAuto" }
  | { readonly type: "toggleSkip" }
  | { readonly type: "skipHold"; readonly held: boolean }
  | { readonly type: "rollback" }
  | { readonly type: "openBacklog" }
  | { readonly type: "openMenu" }
  | { readonly type: "openSave" }
  | { readonly type: "openLoad" }
  | { readonly type: "openSettings" }
  | { readonly type: "quickSave" }
  | { readonly type: "quickLoad" }
  | { readonly type: "hideUI" }
  | { readonly type: "choose"; readonly option: string }
  | { readonly type: "close" }
  | { readonly type: "newGame" }
  | { readonly type: "continue" }
  | { readonly type: "toTitle" }
  | { readonly type: "saveSlot"; readonly slot: SlotId }
  | { readonly type: "loadSlot"; readonly slot: SlotId }
  | { readonly type: "deleteSlot"; readonly slot: SlotId }
  | { readonly type: "confirm"; readonly yes: boolean }
  | { readonly type: "pickIdle"; readonly index: number }
  | { readonly type: "replayVoice"; readonly lineId: string; readonly voiceKey: string }
  | { readonly type: "setSettings"; readonly settings: Partial<Settings> }
  /**
   * Send a runtime input at `idle` or `ended`, as `DirectorHandle.send` does;
   * a refusal is logged and changes nothing (spec 0.41.0 §7.2).
   */
  | { readonly type: "send"; readonly input: Input }

export type SlotId = "quick" | "auto" | number

export interface Volumes {
  readonly master: number
  readonly music: number
  readonly sfx: number
  readonly voice: number
}

export interface Settings {
  /** Characters per second; 0 = instant. */
  readonly textSpeed: number
  /** Seconds a shown line waits in auto mode (after its voice). */
  readonly autoDelay: number
  readonly skipMode: "read" | "all"
  readonly volumes: Volumes
  readonly locale: string
  /** 0–1. */
  readonly windowOpacity: number
  readonly readColour: boolean
  readonly rollbackDepth: number
}

/**
 * One shown line. `text` is the record's source text; a line with
 * translations keeps them (`texts`, `localeSegments`) so the backlog can be
 * shown in the locale current when it opens (spec 0.47.0 §2.2). The `Backlog`
 * screen model's entries carry `text` already resolved for that locale.
 */
export interface BacklogEntry {
  readonly lineId: string
  readonly voiceKey: string
  readonly speakerName: string
  readonly text: string
  readonly hasVoice: boolean
  /** The line record's translations by locale. */
  readonly texts?: Readonly<Record<string, string>>
  /** The line record's localized segments by locale. */
  readonly localeSegments?: Readonly<Record<string, Ir.segments>>
}

export interface SlotInfo {
  readonly slot: SlotId
  readonly label: string
  readonly savedAt: string
  readonly thumbnail: string | undefined
}

/** What a screen renders; the core decides which screen is shown. */
export type ScreenModel =
  | { readonly _tag: "Title"; readonly canContinue: boolean; readonly background: string | undefined }
  | { readonly _tag: "Backlog"; readonly entries: ReadonlyArray<BacklogEntry> }
  | { readonly _tag: "SaveLoad"; readonly mode: "save" | "load"; readonly slots: ReadonlyArray<SlotInfo>; readonly page: number }
  /** `locales`: the bundle's source locale, then its compiled locales; `settings.locale` is one of them. */
  | { readonly _tag: "Settings"; readonly settings: Settings; readonly locales: ReadonlyArray<string> }
  | { readonly _tag: "Confirm"; readonly message: string }
  | { readonly _tag: "Loading"; readonly progress: number }
  | { readonly _tag: "Halted"; readonly message: string }
  | { readonly _tag: "Idle"; readonly options: ReadonlyArray<{ readonly label: string }> }
  | { readonly _tag: "Menu" }
  | { readonly _tag: "Ended"; readonly reason: string }

/** A host's answer to an `awaitBridge`: sent as `bridgeResult { request, fields, writes? }`. */
export interface BridgeAnswer {
  readonly fields: Readonly<Record<string, unknown>>
  /** Host writes applied with the result, before the lines that follow it. */
  readonly writes?: Contract.WritesInput
}

/** A non-blocking `plugin` record handed to {@link PlayerHost.plugin}. */
export interface PluginCall {
  readonly tag: string
  /** The record's resolved directive attributes. */
  readonly attrs: BridgeAwait["attrs"]
  /** The record's source document. */
  readonly document: string
  readonly position: string
}

/** Host hooks: bridges, plugin records and the idle flow. */
export interface PlayerHost {
  /** Answer an `awaitBridge` by its tag; runs on a fiber of its own. */
  readonly bridge?: Readonly<Record<string, (request: BridgeAwait) => Effect.Effect<BridgeAnswer>>>
  /**
   * Run a game action for a `plugin` record by its tag, at the record's place
   * in the batch (a waiting cue, run in skip mode too). Records the host
   * answered through `awaitBridge` are never handed here.
   */
  readonly plugin?: Readonly<Record<string, (call: PluginCall) => Effect.Effect<void>>>
  /**
   * Override the bindings' idle flow: return the input to send, or undefined to
   * wait (for the reader, or for a later `send`). Runs on a fiber of its own; a
   * `send` or any input that leaves `idle` interrupts it.
   */
  readonly idle?: (await_: Await) => Effect.Effect<Input | undefined>
}
