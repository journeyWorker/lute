/**
 * The seams between the player core and a renderer/host (spec 0.40.0 §6–§7,
 * docs/player/architecture.md). `@lute-lang/player-web` implements these with
 * PixiJS and Web Audio; an engine host implements them with its own renderer.
 * Every operation is an Effect that completes when its effect has finished.
 */
import type { Effect, Scope, Stream } from "effect"
import { Schema } from "effect"
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
  /** Display name (cast `name`, or empty for narration). */
  readonly speakerName: string
  readonly segments: ReadonlyArray<Segment>
  /** Already in the read history. */
  readonly read: boolean
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

/** A named effect from the player effect catalogue with its bound parameters. */
export interface EffectSpec {
  readonly effect: string
  readonly params: Readonly<Record<string, number | string | boolean>>
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
  /** Show and type a line; completes when the line is fully shown. */
  readonly showLine: (line: LineView, mode: PlayMode) => Effect.Effect<void>
  /** Finish typing the current line at once. */
  readonly revealLine: Effect.Effect<void>
  /** Remove the current line from the dialogue window. */
  readonly clearLine: Effect.Effect<void>
  /** Rebuild the whole stage at once (load, rollback, presentation start). */
  readonly restore: (stage: Stage) => Effect.Effect<void>
  /** Show a choice menu; `remaining` streams seconds left for a timed choice. Resolves with the chosen option id. */
  readonly showChoice: (choice: ChoiceView | MenuAwait, remaining: Stream.Stream<number>) => Effect.Effect<string>
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
}

export type AudioChannel = "music" | "sfx" | "voice"

export interface AudioPlay {
  /** A bound file path (resolved from bindings). */
  readonly file: string
  readonly loop: boolean
  /** 0–1, multiplied with the channel volume. */
  readonly volume: number
  /** Crossfade/fade-in seconds. */
  readonly fade: number
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
  | { readonly type: "replayVoice"; readonly lineId: string }
  | { readonly type: "setSettings"; readonly settings: Partial<Settings> }

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

export interface BacklogEntry {
  readonly lineId: string
  readonly voiceKey: string
  readonly speakerName: string
  readonly text: string
  readonly hasVoice: boolean
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
  | { readonly _tag: "Settings"; readonly settings: Settings; readonly locales: ReadonlyArray<string> }
  | { readonly _tag: "Confirm"; readonly message: string }
  | { readonly _tag: "Loading"; readonly progress: number }
  | { readonly _tag: "Halted"; readonly message: string }
  | { readonly _tag: "Idle"; readonly options: ReadonlyArray<{ readonly label: string }> }
  | { readonly _tag: "Menu" }
  | { readonly _tag: "Ended"; readonly reason: string }

/** Host hooks: bridges and the idle flow. */
export interface PlayerHost {
  readonly bridge?: Readonly<Record<string, (request: BridgeAwait) => Effect.Effect<Readonly<Record<string, unknown>>>>>
  /** Override the bindings' idle flow: return the input to send, or undefined to wait for the reader. */
  readonly idle?: (await_: Await) => Effect.Effect<Input | undefined>
}
