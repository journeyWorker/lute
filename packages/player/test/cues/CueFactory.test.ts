import { Effect, Fiber, Schema, Stream, SubscriptionRef } from "effect"
import * as TestClock from "effect/testing/TestClock"
import { describe, expect, it } from "@effect/vitest"
import { join } from "node:path"
import * as Runtime from "@lute-lang/runtime"
import * as LuteBun from "../../../runtime/src/bun.js"
import { BundleSource } from "../../../runtime/src/index.js"
import { compileBundle, jsonLines, sessionRoot } from "../../../runtime/test/support.js"
import * as Bindings from "../../src/bindings/schema.js"
import * as CueFactory from "../../src/cues/index.js"
import * as Scheduler from "../../src/scheduler/index.js"
import * as Stores from "../../src/stores/index.js"
import type { Cue, CueContext } from "../../src/model.js"
import type { AudioPlay, AudioPort, PlayMode, PluginCall, Presenter, StageCue } from "../../src/ports/index.js"

const decodeOutput = Schema.decodeUnknownSync(Runtime.Contract.Output)
const decodeBindings = Schema.decodeUnknownSync(Bindings.Bindings)
const bindings = decodeBindings({
  bindingsVersion: "0.41",
  canvas: { width: 1920, height: 1080 },
  assets: { bg: "bg.png", actor: "actor.png", cg: "cg.png", video: "video.mp4", music: "music.ogg", sfx: "sfx.ogg" },
  backgrounds: { foyer: { image: "foyer.png", times: { night: "foyer-night.png" } } },
  characters: { guide: { sprite: { default: "guide.png", emotions: { happy: "guide-happy.png" } }, nameColor: "#fff" } },
  actions: { enter: { effect: "fadeIn", duration: 0.5 } },
  transitions: { fade: { effect: "fade", duration: 0.4 } },
  music: { moods: { calm: "calm.ogg" } },
  sfx: { bell: "bell.ogg" },
  voice: { pattern: "voice/{voiceKey}.ogg" },
  vfx: { flash: { effect: "flash", duration: 0.2 }, lanterns: { effect: "custom" }, tideRise: { effect: "custom", name: "tide-rise", duration: 2 } },
  sequences: { intro: { video: "intro.mp4" } }
})

const commands = [
  { kind: "line", family: "content", position: "001-0100", role: "dialogue", speaker: "guide", text: "Hello", lineId: "line-1", voiceKey: "voice-1", texts: { fr: "Bonjour" }, segments: [{ text: "Hello", styles: ["emphasis"], rate: 0.5 }, { pause: 0.25 }] },
  { kind: "bg", family: "staging", position: "001-0200", location: "foyer", time: "night", assetId: "bg" },
  { kind: "actor", family: "staging", position: "001-0300", character: "guide", emotion: "happy", action: "enter" },
  { kind: "cg", family: "staging", position: "001-0400", assetId: "cg", display: "show" },
  { kind: "video", family: "staging", position: "001-0500", assetId: "video", display: "show" },
  { kind: "camera", family: "staging", position: "001-0600", focus: "guide" },
  { kind: "vfx", family: "staging", position: "001-0700", type: "flash" },
  { kind: "sequence", family: "staging", position: "001-0800", name: "intro" },
  { kind: "music", family: "staging", position: "001-0900", mood: "calm", assetId: "music" },
  { kind: "music", family: "staging", position: "001-1000", stop: true },
  { kind: "sfx", family: "staging", position: "001-1100", sound: "bell", assetId: "sfx" },
  { kind: "set", family: "state", position: "001-1200", path: "x", op: "set", value: { cel: "1", expr: { int: 1 } } },
  { kind: "line", family: "content", position: "002-0100", role: "dialogue", speaker: "guide", emotion: "happy", variant: 2, text: "Hi", lineId: "line-2", voiceKey: "" },
  { kind: "line", family: "content", position: "002-0150", role: "narration", speaker: "narrator", text: "Quiet.", lineId: "line-3", voiceKey: "" },
  { kind: "music", family: "staging", position: "002-0200", playback: "start", mood: "calm", volume: "0.5" },
  { kind: "music", family: "staging", position: "002-0300", playback: "change", assetId: "music", volume: "2" },
  { kind: "music", family: "staging", position: "002-0400", playback: "fadeOut" },
  { kind: "music", family: "staging", position: "002-0500", playback: "stop", timing: { duration: 2 } },
  { kind: "music", family: "staging", position: "002-0600", playback: "fadeOut", timing: { duration: 3 } },
  { kind: "music", family: "staging", position: "002-0700", mood: "calm", volume: "loud", timing: { duration: 1.5 } },
  { kind: "music", family: "staging", position: "002-0750", stop: true },
  { kind: "vfx", family: "staging", position: "002-0800", type: "lanterns" },
  { kind: "vfx", family: "staging", position: "002-0900", type: "tideRise" },
  { kind: "vfx", family: "staging", position: "002-1000", type: "sparkle" },
  { kind: "plugin", family: "plugin", position: "002-1100", tag: "well", fields: {} },
  { kind: "plugin", family: "plugin", position: "002-1200", tag: "odds", fields: {} }
]
const index: Runtime.Ir.projectIndex = {
  capabilitySnapshot: "test",
  documents: [{ artifact: "scene.json", key: "scene", kind: "scene", path: "scene.lute" }],
  entities: [],
  enums: [],
  irVersion: "0.41.0",
  prereqEdges: [],
  relations: [],
  requiredSemantics: [],
  rules: [],
  seedFacts: [],
  cast: { guide: "Guide-san" }
}
const bundle: Runtime.Bundle.Bundle = { index, artifacts: { "scene.json": { commands } } }
const output = decodeOutput({ eventVersion: "0.41", await: { type: "idle" }, events: [
  { type: "record", document: "scene", record: { kind: "line", position: "001-0100", speaker: "guide", text: "Hello", lineId: "line-1", voiceKey: "voice-1" } },
  { type: "record", document: "scene", record: { kind: "bg", position: "001-0200" } },
  { type: "record", document: "scene", record: { kind: "actor", position: "001-0300" } },
  { type: "record", document: "scene", record: { kind: "cg", position: "001-0400" } },
  { type: "record", document: "scene", record: { kind: "video", position: "001-0500" } },
  { type: "record", document: "scene", record: { kind: "camera", position: "001-0600" } },
  { type: "record", document: "scene", record: { kind: "vfx", position: "001-0700" } },
  { type: "record", document: "scene", record: { kind: "sequence", position: "001-0800" } },
  { type: "record", document: "scene", record: { kind: "music", position: "001-0900" } },
  { type: "record", document: "scene", record: { kind: "music", position: "001-1000" } },
  { type: "record", document: "scene", record: { kind: "sfx", position: "001-1100" } },
  { type: "record", document: "scene", record: { kind: "set", position: "001-1200" } },
  { type: "presentation", beat: "start", document: "scene", kind: "scene" },
  { type: "presentationEnd", beat: "start", document: "scene", reason: null },
  { type: "quest", document: "scene", commands: [] },
  { type: "clock", from: {}, to: {} }
] })

const factory = CueFactory.make({ bundle, bindings, locale: "fr" })

describe("CueFactory", () => {
  it.effect("decodes every runtime record and session event without a defect", () => Effect.gen(function* () {
    const cues = yield* factory.fromOutput(output, new Set(["line-1"]))
    expect(cues).toHaveLength(16)
    expect(cues.map(({ kind }) => kind)).toEqual(["line", "bg", "actor", "cg", "video", "camera", "vfx", "sequence", "music", "music", "sfx", "bookkeeping", "presentation", "presentationEnd", "quest", "clock"])
  }))
  it.effect("resolves cast name, locale fallback, segments, read flag and voice file", () => Effect.gen(function* () {
    const cues = yield* factory.fromOutput(output, new Set(["line-1"]))
    const line = cues[0]?.line
    expect(line).toMatchObject({ speakerName: "Guide-san", read: true, voiceKey: "voice-1" })
    expect(line?.segments).toEqual([{ _tag: "Text", text: "Hello", styles: ["emphasis"], rate: 0.5 }, { _tag: "Pause", seconds: 0.25 }])
    expect(cues[0]?.files).toEqual(["voice/voice-1.ogg"])
  }))
  it.effect("falls back from an unavailable locale to source text", () => Effect.gen(function* () {
    const local = CueFactory.make({ bundle, bindings, locale: "ja" })
    const cues = yield* local.fromOutput(output, new Set())
    expect(cues[0]?.line?.segments).toEqual([{ _tag: "Text", text: "Hello", styles: ["emphasis"], rate: 0.5 }, { _tag: "Pause", seconds: 0.25 }])
  }))
  it.effect("maps every StageCue tag and preserves duration and files", () => Effect.gen(function* () {
    const cues = yield* factory.fromOutput(output, new Set())
    expect(cues.slice(1, 11).map((cue) => cue.kind)).toEqual(["bg", "actor", "cg", "video", "camera", "vfx", "sequence", "music", "music", "sfx"])
    expect(cues.find((cue) => cue.kind === "bg")?.files).toEqual(["foyer-night.png"])
    expect(cues.find((cue) => cue.kind === "actor")?.files).toEqual(["guide-happy.png"])
  }))
  it.effect("maps music stop to AudioPort stop through execute and complete", () => Effect.gen(function* () {
    const cues = yield* factory.fromOutput(output, new Set())
    const stop = cues[9]
    expect(stop?.describe).toContain("stop")
    expect(stop?.timing.wait).toBe(false)
  }))
  it.effect("normalizes timing defaults by cue kind", () => Effect.gen(function* () {
    const localOutput = decodeOutput({ eventVersion: "0.41", await: { type: "idle" }, events: [{ type: "record", document: "scene", record: { kind: "video", position: "001-0500" } }, { type: "record", document: "scene", record: { kind: "line", position: "001-0100" } }, { type: "record", document: "scene", record: { kind: "sequence", position: "001-0800" } }] })
    const cues = yield* factory.fromOutput(localOutput, new Set())
    expect(cues.map((cue) => cue.timing.wait)).toEqual([true, true, true])
  }))

  it.effect("resolves a timed branch's prompt, option text, timeout and default from the bundle", () => Effect.gen(function* () {
    const project = join(sessionRoot, "timed-choice", "project")
    const compiled = yield* compileBundle(project)
    const source = yield* Effect.provide(
      Effect.flatMap(BundleSource.BundleSource, (bundleSource) => bundleSource.load),
      LuteBun.bundle(compiled)
    )
    const first = jsonLines(join(sessionRoot, "timed-choice", "expected.jsonl"))
      .map((line) => Schema.decodeUnknownSync(Runtime.Contract.Output)(Schema.decodeUnknownSync(Schema.Record(Schema.String, Schema.Unknown))(line).output))
      .find((output) => output.await.type === "awaitChoice")
    if (first === undefined || first.await.type !== "awaitChoice") throw new Error("timed-choice never awaits a choice")
    const view = CueFactory.make({ bundle: source, bindings }).choiceView(first.await)
    expect(view).toMatchObject({ id: "knock", construct: "branch", prompt: "Open the door?", timeout: 5, default: "wait" })
    expect(view.options.map((option) => [option.id, option.text])).toEqual([["open", "Open it"], ["wait", "Wait"]])
  }))

  it.effect("maps Background StageCue", () => Effect.gen(function* () {
    const cues = yield* factory.fromOutput(output, new Set())
    expect(cues[1]?.kind).toBe("bg")
    expect(cues[1]?.files).toEqual(["foyer-night.png"])
  }))
  it.effect("maps ActorShow StageCue", () => Effect.gen(function* () {
    const cues = yield* factory.fromOutput(output, new Set())
    expect(cues[2]?.kind).toBe("actor")
    expect(cues[2]?.files).toEqual(["guide-happy.png"])
  }))
  it.effect("maps ActorExit StageCue", () => Effect.gen(function* () {
    const exitOutput = decodeOutput({ ...output, events: [{ type: "record", document: "scene", record: { kind: "actor", position: "001-0300", exit: true } }] })
    const cues = yield* factory.fromOutput(exitOutput, new Set())
    expect(cues[0]?.kind).toBe("actor")
    expect(cues[0]?.describe).toContain("guide")
  }))
  it.effect("maps Cg show and hide StageCues", () => Effect.gen(function* () {
    const cues = yield* factory.fromOutput(output, new Set())
    const hidden = yield* factory.fromOutput(decodeOutput({ ...output, events: [{ type: "record", document: "scene", record: { kind: "cg", position: "001-0400", display: "hide" } }] }), new Set())
    expect(cues[3]?.kind).toBe("cg")
    expect(hidden[0]?.kind).toBe("cg")
  }))
  it.effect("maps Video StageCue", () => Effect.gen(function* () {
    const cues = yield* factory.fromOutput(output, new Set())
    expect(cues[4]?.kind).toBe("video")
  }))
  it.effect("maps Camera StageCue", () => Effect.gen(function* () {
    const cues = yield* factory.fromOutput(output, new Set())
    expect(cues[5]?.kind).toBe("camera")
  }))
  it.effect("maps Vfx StageCue", () => Effect.gen(function* () {
    const cues = yield* factory.fromOutput(output, new Set())
    expect(cues[6]?.kind).toBe("vfx")
  }))
  it.effect("maps Sequence StageCue", () => Effect.gen(function* () {
    const cues = yield* factory.fromOutput(output, new Set())
    expect(cues[7]?.kind).toBe("sequence")
    expect(cues[7]?.files).toEqual(["intro.mp4"])
  }))
  it.effect("maps music start to a music cue", () => Effect.gen(function* () {
    const cues = yield* factory.fromOutput(output, new Set())
    expect(cues[8]?.kind).toBe("music")
    expect(cues[8]?.files).toEqual(["music.ogg"])
  }))
  it.effect("maps music stop true to a music cue", () => Effect.gen(function* () {
    const cues = yield* factory.fromOutput(output, new Set())
    expect(cues[9]?.kind).toBe("music")
    expect(cues[9]?.describe).toContain("stop")
  }))
  it.effect("maps sfx to an audio cue", () => Effect.gen(function* () {
    const cues = yield* factory.fromOutput(output, new Set())
    expect(cues[10]?.kind).toBe("sfx")
    expect(cues[10]?.files).toEqual(["sfx.ogg"])
  }))

  it.effect("reads speaker names from the index cast and carries the line's emotion and variant (§7.8)", () => Effect.gen(function* () {
    const cues = yield* factory.fromOutput(recordsOutput([
      { kind: "line", position: "002-0100", speaker: "guide", role: "dialogue", text: "Hi", lineId: "line-2", voiceKey: "" },
      { kind: "line", position: "002-0150", speaker: "narrator", role: "narration", text: "Quiet.", lineId: "line-3", voiceKey: "" },
      { kind: "line", position: "001-0100", speaker: "guide", text: "Hello", lineId: "line-1", voiceKey: "voice-1" }
    ]), new Set())
    expect(cues[0]?.line).toMatchObject({ speaker: "guide", speakerName: "Guide-san", emotion: "happy", variant: "2" })
    expect(cues[1]?.line).toMatchObject({ speakerName: "" })
    expect(cues[1]?.line).not.toHaveProperty("emotion")
    expect(cues[2]?.line).not.toHaveProperty("emotion")
    expect(cues[2]?.line).not.toHaveProperty("variant")
    const uncast = CueFactory.make({ bundle: { ...bundle, index: { ...index, cast: {} } }, bindings })
    expect((yield* uncast.fromOutput(recordsOutput([{ kind: "line", position: "002-0100", speaker: "guide", text: "Hi", lineId: "line-2" }]), new Set()))[0]?.line?.speakerName).toBe("guide")
  }))

  it.effect("plays music by playback, volume and duration (§7.7 table)", () => Effect.gen(function* () {
    const positions = ["002-0200", "002-0300", "002-0400", "002-0500", "002-0600", "002-0700", "002-0750", "001-0900", "001-1000"]
    const cues = yield* factory.fromOutput(recordsOutput(positions.map((position) => ({ kind: "music", position }))), new Set())
    const played = yield* recordAudio(cues, "execute")
    expect(played).toEqual([
      { play: { file: "calm.ogg", loop: true, volume: 0.5, fade: 0, restart: true } },
      { play: { file: "music.ogg", loop: true, volume: 1, fade: 0, restart: false } },
      { stop: 1 },
      { stop: 2 },
      { stop: 3 },
      { play: { file: "calm.ogg", loop: true, volume: 1, fade: 1.5, restart: false } },
      { stop: 0 },
      { play: { file: "music.ogg", loop: true, volume: 1, fade: 0, restart: false } },
      { stop: 0 }
    ])
    // Skip snaps: no fades, the same files and restarts.
    expect(yield* recordAudio(cues, "complete")).toEqual([
      { play: { file: "calm.ogg", loop: true, volume: 0.5, fade: 0, restart: true } },
      { play: { file: "music.ogg", loop: true, volume: 1, fade: 0, restart: false } },
      { stop: 0 }, { stop: 0 }, { stop: 0 },
      { play: { file: "calm.ogg", loop: true, volume: 1, fade: 0, restart: false } },
      { stop: 0 },
      { play: { file: "music.ogg", loop: true, volume: 1, fade: 0, restart: false } },
      { stop: 0 }
    ])
    expect(CueFactory.musicVolume("0.25")).toBe(0.25)
    expect(CueFactory.musicVolume("-1")).toBe(0)
    expect(CueFactory.musicVolume(undefined)).toBe(1)
  }))

  it("collects a restored stage's files with the same resolution its cues and music use", () => {
    const unbound = decodeBindings({ bindingsVersion: "0.41", canvas: { width: 1, height: 1 }, music: { moods: { calm: "calm.ogg" } } })
    const stage = { actors: [], bg: { assetId: "bg/hall.png" }, cg: { assetId: "cg/one.png" }, music: { assetId: "bgm/theme.ogg", mood: "calm" } }
    expect(CueFactory.stageFiles(unbound, stage)).toEqual(["bg/hall.png", "cg/one.png", "bgm/theme.ogg"])
    expect(CueFactory.stageMusic(unbound, stage)?.file).toBe("bgm/theme.ogg")
    // Mapped ids resolve through `assets`; a stopped stage needs no music file.
    expect(CueFactory.stageFiles(bindings, { actors: [], bg: { location: "foyer", time: "night" }, music: { assetId: "music", playback: "stop" } })).toEqual(["foyer-night.png"])
    expect(CueFactory.assetFile(bindings, "music")).toBe("music.ogg")
  })

  it.effect("a bg or music record with an unmapped assetId names that file, as its restored stage does", () => Effect.gen(function* () {
    const unbound = decodeBindings({ bindingsVersion: "0.41", canvas: { width: 1, height: 1 } })
    const local = CueFactory.make({ bundle: { index, artifacts: { "scene.json": { commands: [
      { kind: "bg", family: "staging", position: "003-0100", assetId: "bg/hall.png" },
      { kind: "music", family: "staging", position: "003-0200", assetId: "bgm/theme.ogg" }
    ] } } }, bindings: unbound })
    const cues = yield* local.fromOutput(recordsOutput([{ kind: "bg", position: "003-0100" }, { kind: "music", position: "003-0200" }]), new Set())
    expect(cues.map((cue) => cue.files)).toEqual([["bg/hall.png"], ["bgm/theme.ogg"]])
  }))

  it("silences a restored stage whose music stops or fades out (§7.7)", () => {
    expect(CueFactory.stageMusic(bindings, { actors: [], music: { mood: "calm", playback: "fadeOut" } })).toBeUndefined()
    expect(CueFactory.stageMusic(bindings, { actors: [], music: { mood: "calm", playback: "stop" } })).toBeUndefined()
    expect(CueFactory.stageMusic(bindings, { actors: [] })).toBeUndefined()
    expect(CueFactory.stageMusic(bindings, { actors: [], music: { mood: "calm", playback: "start", volume: "0.4" } }))
      .toEqual({ file: "calm.ogg", loop: true, volume: 0.4, fade: 0, restart: false })
  })

  it.effect("gives StageCue.Vfx an EffectSpec for custom, catalogue and unbound vfx (§7.7)", () => Effect.gen(function* () {
    const cues = yield* factory.fromOutput(recordsOutput(["002-0800", "002-0900", "001-0700", "002-1000"].map((position) => ({ kind: "vfx", position }))), new Set())
    const staged = yield* recordStages(cues)
    expect(staged).toEqual([
      { _tag: "Vfx", effect: { effect: "custom", name: "lanterns", params: {} }, duration: undefined },
      { _tag: "Vfx", effect: { effect: "custom", name: "tide-rise", params: {} }, duration: 2 },
      { _tag: "Vfx", effect: { effect: "flash", params: { duration: 0.2 } }, duration: undefined },
      { _tag: "Vfx", effect: { effect: "custom", name: "sparkle", params: {} }, duration: undefined }
    ])
    expect(staged[2]).not.toHaveProperty(["effect", "name"])
  }))

  it.effect("makes handled plugin records waiting cues that hand the host a PluginCall (§7.6)", () => Effect.gen(function* () {
    const calls: Array<PluginCall> = []
    const hosted = CueFactory.make({ bundle, bindings, plugin: { well: (call) => Effect.sync(() => { calls.push(call) }) } })
    const cues = yield* hosted.fromOutput(recordsOutput([
      { kind: "plugin", position: "002-1100", tag: "well", attrs: { depth: 3, item: "rope" } },
      { kind: "plugin", position: "002-1200", tag: "odds", attrs: {} },
      { kind: "plugin", position: "002-1100", tag: "well", attrs: {}, answered: [{ field: "passed", value: true }] },
      // An effect record shares its plugin's position; it is not a hook.
      { kind: "set", position: "002-1100", path: "run.depth", value: 3, effectOf: "well" }
    ]), new Set())
    expect(cues.map((cue) => cue.kind)).toEqual(["plugin", "bookkeeping", "bookkeeping", "bookkeeping"])
    expect(cues[0]?.timing.wait).toBe(true)
    yield* cues[0]!.execute(silentContext)
    yield* cues[0]!.complete(silentContext)
    expect(calls).toEqual([
      { tag: "well", attrs: { depth: 3, item: "rope" }, document: "scene", position: "002-1100" },
      { tag: "well", attrs: { depth: 3, item: "rope" }, document: "scene", position: "002-1100" }
    ])
  }))

  it.effect("runs plugin waiting cues in order, waiting for each handler", () => Effect.gen(function* () {
    const order: Array<string> = []
    const hosted = CueFactory.make({ bundle, bindings, plugin: {
      well: () => Effect.sleep(5).pipe(Effect.andThen(Effect.sync(() => { order.push("well") }))),
      odds: () => Effect.sync(() => { order.push("odds") })
    } })
    const cues = yield* hosted.fromOutput(recordsOutput([
      { kind: "plugin", position: "002-1100", tag: "well", attrs: {} },
      { kind: "plugin", position: "002-1200", tag: "odds", attrs: {} }
    ]), new Set())
    const scheduler = yield* Scheduler.make()
    const mode = yield* SubscriptionRef.make<PlayMode>("normal")
    const gate = { started: () => Effect.void, release: () => Effect.void }
    const fiber = yield* Effect.forkScoped(scheduler.play(cues, silentContext, mode, gate))
    yield* TestClock.adjust(4)
    expect(order).toEqual([])
    yield* TestClock.adjust(1)
    yield* Fiber.join(fiber)
    expect(order).toEqual(["well", "odds"])
    // Skip mode runs the game action too.
    yield* SubscriptionRef.set(mode, "skip")
    yield* scheduler.play(cues.slice(1), silentContext, mode, gate)
    expect(order).toEqual(["well", "odds", "odds"])
  }))

  it.effect("returns exactly one cue per output event, in event order (§7.6)", () => Effect.gen(function* () {
    const hosted = CueFactory.make({ bundle, bindings, plugin: { well: () => Effect.void } })
    const mixed = decodeOutput({ ...output, events: [
      ...output.events,
      { type: "record", document: "scene", record: { kind: "plugin", position: "002-1100", tag: "well", attrs: {} } },
      { type: "record", document: "scene", record: { kind: "objective", quest: "q" } }
    ] })
    const cues = yield* hosted.fromOutput(mixed, new Set())
    expect(cues).toHaveLength(mixed.events.length)
    expect(cues.slice(-2).map((cue) => cue.kind)).toEqual(["plugin", "bookkeeping"])
  }))
})

const recordsOutput = (records: ReadonlyArray<Readonly<Record<string, unknown>>>) =>
  decodeOutput({ eventVersion: "0.41", await: { type: "idle" }, events: records.map((record) => ({ type: "record", document: "scene", record })) })

const silentPresenter: Presenter = {
  stage: () => Effect.void, showLine: () => Effect.void, revealLine: Effect.void, clearLine: Effect.void, restore: () => Effect.void,
  showChoice: () => Effect.never, hideChoice: Effect.void, showScreen: () => Effect.void, hideScreen: Effect.void,
  setUiHidden: () => Effect.void, applySettings: () => Effect.void, capture: Effect.succeed(""), actions: Stream.empty
}
const silentAudio: AudioPort = { play: () => Effect.void, stop: () => Effect.void, setVolumes: () => Effect.void }
const silentContext: CueContext = { presenter: silentPresenter, audio: silentAudio, bindings, settings: Stores.defaultSettings }

type AudioOp = { readonly play: AudioPlay } | { readonly stop: number }
const recordAudio = (cues: ReadonlyArray<Cue>, how: "execute" | "complete") => Effect.gen(function* () {
  const ops: Array<AudioOp> = []
  const audio: AudioPort = {
    play: (_channel, sound) => Effect.sync(() => { ops.push({ play: sound }) }),
    stop: (_channel, fade) => Effect.sync(() => { ops.push({ stop: fade }) }),
    setVolumes: () => Effect.void
  }
  for (const cue of cues) yield* cue[how]({ ...silentContext, audio })
  return ops
})

const recordStages = (cues: ReadonlyArray<Cue>) => Effect.gen(function* () {
  const staged: Array<StageCue> = []
  const presenter: Presenter = { ...silentPresenter, stage: (cue) => Effect.sync(() => { staged.push(cue) }) }
  for (const cue of cues) yield* cue.execute({ ...silentContext, presenter })
  return staged
})
