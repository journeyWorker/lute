import { Effect, Schema } from "effect"
import { describe, expect, it } from "@effect/vitest"
import * as Runtime from "@lute-lang/runtime"
import * as Bindings from "../../src/bindings/schema.js"
import * as CueFactory from "../../src/cues/index.js"

const decodeOutput = Schema.decodeUnknownSync(Runtime.Contract.Output)
const decodeBindings = Schema.decodeUnknownSync(Bindings.Bindings)
const bindings = decodeBindings({
  bindingsVersion: "0.40",
  canvas: { width: 1920, height: 1080 },
  assets: { bg: "bg.png", actor: "actor.png", cg: "cg.png", video: "video.mp4", music: "music.ogg", sfx: "sfx.ogg" },
  backgrounds: { foyer: { image: "foyer.png", times: { night: "foyer-night.png" } } },
  characters: { guide: { sprite: { default: "guide.png", emotions: { happy: "guide-happy.png" } }, nameColor: "#fff" } },
  actions: { enter: { effect: "fadeIn", duration: 0.5 } },
  transitions: { fade: { effect: "fade", duration: 0.4 } },
  music: { moods: { calm: "calm.ogg" } },
  sfx: { bell: "bell.ogg" },
  voice: { pattern: "voice/{voiceKey}.ogg" },
  vfx: { flash: { effect: "flash", duration: 0.2 } },
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
  { kind: "set", family: "state", position: "001-1200", path: "x", op: "set", value: { cel: "1", expr: { int: 1 } } }
]
const bundle = { index: {}, artifacts: { "scene.json": { commands } } }
const output = decodeOutput({ eventVersion: "0.40", await: { type: "idle" }, events: [
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
    expect(line).toMatchObject({ speakerName: "guide", read: true, voiceKey: "voice-1" })
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
    const localOutput = decodeOutput({ eventVersion: "0.40", await: { type: "idle" }, events: [{ type: "record", document: "scene", record: { kind: "video", position: "001-0500" } }, { type: "record", document: "scene", record: { kind: "line", position: "001-0100" } }, { type: "record", document: "scene", record: { kind: "sequence", position: "001-0800" } }] })
    const cues = yield* factory.fromOutput(localOutput, new Set())
    expect(cues.map((cue) => cue.timing.wait)).toEqual([true, true, true])
  }))
})
