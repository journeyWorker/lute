import { Context, Effect, Layer, Option, Schema } from "effect"
import * as Runtime from "@lute-lang/runtime"
import type { Bindings } from "../bindings/schema.js"
import type { Cue, CueContext, CueFactoryShape, CueTiming, Event, Output } from "../model.js"
import type { AudioPlay, EffectSpec, LineView, Segment, StageCue } from "../ports/index.js"

type Bundle = Runtime.Bundle.Bundle
type JsonRecord = { readonly [key: string]: unknown }

const decodeCommand = Schema.decodeUnknownOption(Runtime.Ir.Command)
const isRecord = (value: unknown): value is JsonRecord => typeof value === "object" && value !== null && !Array.isArray(value)
const string = (value: unknown): value is string => typeof value === "string"
const number = (value: unknown): value is number => typeof value === "number" && Number.isFinite(value)
const boolean = (value: unknown): value is boolean => typeof value === "boolean"
const field = (value: unknown, key: string): unknown => isRecord(value) ? value[key] : undefined
const record = (value: unknown, key: string): JsonRecord | undefined => {
  const result = field(value, key)
  return isRecord(result) ? result : undefined
}
const kindOf = (value: unknown): string | undefined => {
  const kind = field(value, "kind")
  return string(kind) ? kind : undefined
}
const positionOf = (value: unknown): string | undefined => {
  const position = field(value, "position")
  return string(position) ? position : undefined
}

const effect = (value: unknown): EffectSpec | undefined => {
  if (!isRecord(value) || !string(value.effect)) return undefined
  const params: Record<string, number | string | boolean> = {}
  for (const [key, parameter] of Object.entries(value)) {
    if (key !== "effect" && parameter !== null && parameter !== undefined && (number(parameter) || string(parameter) || boolean(parameter))) params[key] = parameter
  }
  return { effect: value.effect, params }
}

const bindingEffect = (values: Readonly<Record<string, unknown>> | undefined, key: unknown): EffectSpec | undefined => {
  if (!values || !string(key)) return undefined
  return effect(values[key])
}

const timingOf = (value: unknown, kind: string): CueTiming => {
  const source = record(value, "timing")
  const durationValue = field(source, "duration")
  const delayValue = field(source, "delay")
  const atValue = field(source, "at")
  const timelineValue = field(source, "timeline")
  const explicitWait = field(source, "wait")
  const defaultWait = kind === "line" || kind === "sequence" || (kind === "video" && field(value, "display") === "show") || kind === "barrier"
  return {
    delay: number(delayValue) ? delayValue : 0,
    duration: number(durationValue) ? durationValue : undefined,
    wait: boolean(explicitWait) ? explicitWait : defaultWait,
    timeline: number(timelineValue) ? timelineValue : undefined,
    at: number(atValue) ? atValue : undefined
  }
}

const filesForAsset = (bindings: Bindings, assetId: unknown): ReadonlyArray<string> => {
  if (!string(assetId) || !bindings.assets) return []
  const file = bindings.assets[assetId]
  return file === undefined ? [] : [file]
}

const optionalStage = <T extends StageCue>(cue: T, duration: number | undefined, transition: EffectSpec | undefined): T => ({
  ...cue,
  ...(duration === undefined ? {} : { duration }),
  ...(transition === undefined ? {} : { transition })
})

const lineView = (value: JsonRecord, locale: string, readLines: ReadonlySet<string>, castNames: ReadonlyMap<string, string>): LineView => {
  const lineId = string(value.lineId) ? value.lineId : ""
  const speaker = string(value.speaker) ? value.speaker : "narrator"
  const role = string(value.role) ? value.role : "narration"
  const localizedSegments = record(value, "localeSegments")
  const selected = localizedSegments?.[locale]
  const fallbackSegments = field(value, "segments")
  const rawSegments = Array.isArray(selected) ? selected : Array.isArray(fallbackSegments) ? fallbackSegments : []
  const texts = record(value, "texts")
  const fallbackText = string(value.text) ? value.text : ""
  const text = texts && string(texts[locale]) ? texts[locale] : fallbackText
  const segments: Segment[] = []
  for (const raw of rawSegments) {
    if (!isRecord(raw)) continue
    if (number(raw.pause)) segments.push({ _tag: "Pause", seconds: raw.pause })
    else if (string(raw.text)) {
      const styles = Array.isArray(raw.styles) ? raw.styles.filter(string) : []
      segments.push({ _tag: "Text", text: raw.text, styles, ...(number(raw.rate) ? { rate: raw.rate } : {}) })
    }
  }
  if (segments.length === 0) segments.push({ _tag: "Text", text, styles: [] })
  return {
    lineId,
    voiceKey: string(value.voiceKey) ? value.voiceKey : "",
    role,
    speaker,
    speakerName: role === "narration" || speaker === "narrator" ? "" : (castNames.get(speaker) ?? speaker),
    segments,
    read: readLines.has(lineId)
  }
}

/** Cast id → display name, from the compiled artifacts' cast tables. */
export const castNamesFrom = (artifacts: Readonly<Record<string, unknown>>): Map<string, string> => {
  const names = new Map<string, string>()
  const visit = (value: unknown): void => {
    if (!isRecord(value)) return
    const cast = value.cast
    if (Array.isArray(cast)) {
      for (const member of cast) if (isRecord(member) && string(member.id) && string(member.name)) names.set(member.id, member.name)
    } else if (isRecord(cast)) {
      for (const [id, member] of Object.entries(cast)) if (isRecord(member) && string(member.name)) names.set(id, member.name)
    }
    for (const child of Object.values(value)) if (isRecord(child) || Array.isArray(child)) visit(child)
  }
  for (const artifact of Object.values(artifacts)) visit(artifact)
  return names
}

const commandIndexFrom = (bundle: Bundle): Map<string, Map<string, JsonRecord>> => {
  const result = new Map<string, Map<string, JsonRecord>>()
  const index = isRecord(bundle.index) ? bundle.index : undefined
  const documents = Array.isArray(index?.documents) ? index.documents : []
  const artifactKeys = new Map<string, Set<string>>()
  const addKey = (artifact: string, key: unknown): void => {
    if (!string(key) || key.length === 0) return
    const keys = artifactKeys.get(artifact) ?? new Set<string>()
    keys.add(key)
    artifactKeys.set(artifact, keys)
  }
  // Runtime record.document is the source document path (for example,
  // `game.lute`), not the compiled artifact filename (`game.lute.json`).
  // Keep the indexed path/key aliases and the artifact-derived aliases for
  // synthetic bundles that do not carry an index.
  for (const document of documents) {
    if (!isRecord(document) || !string(document.artifact)) continue
    addKey(document.artifact, document.path)
    addKey(document.artifact, document.key)
  }
  const withoutJson = (value: string): string => value.endsWith(".json") ? value.slice(0, -5) : value
  const basename = (value: string): string => {
    const normalized = value.replaceAll("\\", "/")
    const slash = normalized.lastIndexOf("/")
    return slash < 0 ? normalized : normalized.slice(slash + 1)
  }
  for (const [artifactPath, artifact] of Object.entries(bundle.artifacts)) {
    // Store the set before adding aliases so aliases are not lost when the
    // artifact was absent from the bundle index.
    const keys = artifactKeys.get(artifactPath) ?? new Set<string>()
    artifactKeys.set(artifactPath, keys)
    addKey(artifactPath, field(record(artifact, "meta"), "id"))
    addKey(artifactPath, field(artifact, "document"))
    addKey(artifactPath, withoutJson(artifactPath))
    addKey(artifactPath, withoutJson(basename(artifactPath)))
    for (const documentKey of keys) {
      const commands = result.get(documentKey) ?? new Map<string, JsonRecord>()
      const visit = (value: unknown): void => {
        if (isRecord(value)) {
          const position = positionOf(value)
          if (position && kindOf(value)) commands.set(position, value)
          for (const child of Object.values(value)) visit(child)
        } else if (Array.isArray(value)) for (const child of value) visit(child)
      }
      visit(artifact)
      result.set(documentKey, commands)
    }
  }
  return result
}

const LINE_RUNTIME_FIELDS = ["lineId", "role", "speaker", "text", "voiceKey"] as const
const lineRuntimeFields = (value: JsonRecord): JsonRecord =>
  Object.fromEntries(LINE_RUNTIME_FIELDS.flatMap((key) => (key in value ? [[key, value[key]]] : [])))

const decodeRecord = (value: JsonRecord): JsonRecord => {
  const decoded = decodeCommand(value)
  if (Option.isSome(decoded) && isRecord(decoded.value)) {
    return { ...decoded.value, kind: value.kind, family: value.family, position: value.position }
  }
  if (value.kind === "camera" && value.family === "staging" && string(value.position)) return value
  throw new Error(`invalid runtime command at ${string(value.position) ? value.position : "<unknown>"}`)
}
const noop = (_ctx: CueContext): Effect.Effect<void> => Effect.void
const cueBase = (kind: Cue["kind"], timing: CueTiming, stage: boolean, files: ReadonlyArray<string>, describe: string, execute: Cue["execute"], complete: Cue["complete"], line?: LineView): Cue => ({
  kind, timing, stage, files, line, execute, complete, describe
})

const fileForBackground = (bindings: Bindings, value: JsonRecord): string | undefined => {
  const location = string(value.location) ? value.location : undefined
  const time = string(value.time) ? value.time : undefined
  const background = location && bindings.backgrounds ? bindings.backgrounds[location] : undefined
  return background === undefined ? undefined : time && background.times?.[time] ? background.times[time] : background.image
}

const spriteFile = (bindings: Bindings, value: JsonRecord): string | undefined => {
  const character = string(value.character) && bindings.characters ? bindings.characters[value.character] : undefined
  if (!character) return undefined
  const costume = string(value.costume) && character.costumes ? character.costumes[value.costume] : undefined
  if (costume) return string(value.emotion) && costume.emotions?.[value.emotion] ? costume.emotions[value.emotion] : costume.default
  return string(value.emotion) && character.sprite.emotions?.[value.emotion] ? character.sprite.emotions[value.emotion] : character.sprite.default
}

const makeRecordCue = (value: JsonRecord, bindings: Bindings, locale: string, readLines: ReadonlySet<string>, castNames: ReadonlyMap<string, string>): Cue | undefined => {
  const kind = kindOf(value)
  if (!kind) return undefined
  const timing = timingOf(value, kind)
  const duration = timing.duration
  switch (kind) {
    case "line": {
      const line = lineView(value, locale, readLines, castNames)
      const pattern = bindings.voice?.pattern
      const voice = pattern && line.voiceKey ? pattern.replaceAll("{voiceKey}", line.voiceKey) : undefined
      return cueBase("line", timing, false, voice ? [voice] : [], `line ${line.lineId}`, (ctx) => Effect.gen(function* () {
        if (voice) {
          const played = ctx.audio.play("voice", { file: voice, loop: false, volume: 1, fade: 0 })
          yield* Effect.forkDetach(ctx.voiceEnded === undefined ? played : played.pipe(Effect.ensuring(ctx.voiceEnded(line.lineId))))
        }
        yield* ctx.presenter.showLine(line, "normal")
      }), (ctx) => ctx.presenter.showLine(line, "skip"), line)
    }
    case "bg": {
      const bg = optionalStage({
        _tag: "Background" as const,
        bg: {
          ...(string(value.location) ? { location: value.location } : {}),
          ...(string(value.time) ? { time: value.time } : {}),
          ...(string(value.assetId) ? { assetId: value.assetId } : {})
        },
        transition: bindingEffect(bindings.transitions, value.transition),
        duration
      }, duration, bindingEffect(bindings.transitions, value.transition))
      const file = fileForBackground(bindings, value)
      return cueBase("bg", timing, true, file ? [file] : filesForAsset(bindings, value.assetId), `background ${String(value.location ?? "")}`, (ctx) => ctx.presenter.stage(bg, "normal"), (ctx) => ctx.presenter.stage(bg, "skip"))
    }
    case "actor": {
      const character = string(value.character) ? value.character : ""
      const actor: StageCue = value.exit === true
        ? optionalStage({ _tag: "ActorExit" as const, character, action: bindingEffect(bindings.actions, value.action), duration }, duration, undefined)
        : optionalStage({ _tag: "ActorShow" as const, actor: { character, ...(string(value.anchor) ? { anchor: value.anchor } : {}), ...(string(value.emotion) ? { emotion: value.emotion } : {}), ...(string(value.costume) ? { costume: value.costume } : {}) }, action: bindingEffect(bindings.actions, value.action), duration }, duration, undefined)
      const file = spriteFile(bindings, value)
      return cueBase("actor", timing, true, file ? [file] : [], `actor ${character}`, (ctx) => ctx.presenter.stage(actor, "normal"), (ctx) => ctx.presenter.stage(actor, "skip"))
    }
    case "music": {
      const file = string(value.assetId) && bindings.assets?.[value.assetId] ? bindings.assets[value.assetId] : string(value.mood) ? bindings.music?.moods?.[value.mood] : undefined
      const audio: AudioPlay | undefined = file ? { file, loop: true, volume: 1, fade: duration ?? 0 } : undefined
      return cueBase("music", timing, true, file ? [file] : filesForAsset(bindings, value.assetId), `music ${value.stop === true ? "stop" : String(value.mood ?? "")}`, (ctx) => value.stop === true ? ctx.audio.stop("music", duration ?? 0) : audio ? ctx.audio.play("music", audio) : Effect.void, (ctx) => value.stop === true ? ctx.audio.stop("music", 0) : audio ? ctx.audio.play("music", { ...audio, fade: 0 }) : Effect.void)
    }
    case "sfx": {
      const file = string(value.assetId) && bindings.assets?.[value.assetId] ? bindings.assets[value.assetId] : string(value.sound) ? bindings.sfx?.[value.sound] : undefined
      const audio: AudioPlay | undefined = file ? { file, loop: false, volume: 1, fade: 0 } : undefined
      return cueBase("sfx", timing, false, file ? [file] : [], `sfx ${String(value.sound ?? "")}`, (ctx) => audio ? ctx.audio.play("sfx", audio) : Effect.void, noop)
    }
    case "vfx": {
      const vfx = bindingEffect(bindings.vfx, value.type) ?? { effect: String(value.transition ?? "none"), params: {} }
      const stage: StageCue = optionalStage({ _tag: "Vfx" as const, effect: vfx, duration }, duration, undefined)
      return cueBase("vfx", timing, false, [], `vfx ${String(value.type)}`, (ctx) => ctx.presenter.stage(stage, "normal"), (ctx) => ctx.presenter.stage(stage, "skip"))
    }
    case "camera": {
      const camera: StageCue = optionalStage({ _tag: "Camera" as const, focus: string(value.focus) ? value.focus : undefined, framing: string(value.framing) ? value.framing : undefined, move: bindingEffect(bindings.cameraMove, value.move), transition: bindingEffect(bindings.transitions, value.transition), duration }, duration, undefined)
      return cueBase("camera", timing, false, [], "camera", (ctx) => ctx.presenter.stage(camera, "normal"), (ctx) => ctx.presenter.stage(camera, "skip"))
    }
    case "cg": {
      const shown = value.display !== "hide"
      const cg: StageCue = optionalStage({ _tag: "Cg" as const, cg: shown ? { assetId: String(value.assetId), ...(string(value.layout) ? { layout: value.layout } : {}) } : null, transition: bindingEffect(bindings.transitions, value.transition), duration }, duration, bindingEffect(bindings.transitions, value.transition))
      return cueBase("cg", timing, true, shown ? filesForAsset(bindings, value.assetId) : [], `cg ${shown ? "show" : "hide"}`, (ctx) => ctx.presenter.stage(cg, "normal"), (ctx) => ctx.presenter.stage(cg, "skip"))
    }
    case "video": {
      const shown = value.display !== "hide"
      const video: StageCue = { _tag: "Video", video: shown ? { assetId: String(value.assetId) } : null, blocking: timing.wait }
      return cueBase("video", timing, true, shown ? filesForAsset(bindings, value.assetId) : [], `video ${shown ? "show" : "hide"}`, (ctx) => ctx.presenter.stage(video, "normal"), (ctx) => ctx.presenter.stage(video, "skip"))
    }
    case "sequence": {
      const name = string(value.name) ? value.name : ""
      const sequence: StageCue = { _tag: "Sequence", name, video: bindings.sequences?.[name]?.video }
      const files = sequence.video ? [sequence.video] : []
      return cueBase("sequence", timing, false, files, `sequence ${name}`, (ctx) => ctx.presenter.stage(sequence, "normal"), (ctx) => ctx.presenter.stage(sequence, "skip"))
    }
    case "barrier": return cueBase("barrier", timing, false, [], `barrier ${String(value.timeline)}`, noop, noop)
    case "set": case "assert": case "retract": case "choice": case "match": case "hub": case "jump": case "end": case "quest": case "on": case "entry": case "accept": case "beat": case "plugin":
      return cueBase("bookkeeping", timing, false, [], `bookkeeping ${kind}`, noop, noop)
    default: return undefined
  }
}

const makeEventCue = (event: Exclude<Event, { readonly type: "record" }>): Cue => {
  if (event.type === "presentation") return cueBase("presentation", timingOf({}, "presentation"), false, [], `presentation ${event.beat}`, (ctx) => ctx.presenter.clearLine, (ctx) => ctx.presenter.clearLine)
  if (event.type === "presentationEnd") return cueBase("presentationEnd", timingOf({}, "presentationEnd"), false, [], `presentation end ${event.beat}`, noop, noop)
  if (event.type === "quest") return cueBase("quest", timingOf({}, "quest"), false, [], "quest", noop, noop)
  return cueBase("clock", timingOf({}, "clock"), false, [], "clock", noop, noop)
}

const makeFactory = (bundle: Bundle, bindings: Bindings, locale: string): CueFactoryShape => {
  const commands = commandIndexFrom(bundle)
  const castNames = castNamesFrom(bundle.artifacts)
  return {
    fromOutput: Effect.fn("Lute.Player.CueFactory.fromOutput")(function* (output: Output, readLines: ReadonlySet<string>) {
      const cues: Cue[] = []
      for (const event of output.events) {
        if (event.type !== "record") {
          cues.push(makeEventCue(event))
          continue
        }
        const runtimeRecord = isRecord(event.record) ? event.record : {}
        const position = positionOf(runtimeRecord)
        if (!position) {
          // Session bookkeeping (quest state, objective settlement) carries no
          // source position: nothing to present, kept for order.
          const kind = string(runtimeRecord.kind) ? runtimeRecord.kind : "record"
          cues.push(cueBase("bookkeeping", timingOf(runtimeRecord, kind), false, [], `bookkeeping ${kind}`, noop, noop))
          continue
        }
        const documentCommands = commands.get(event.document)
        const source = documentCommands?.get(position)
        if (!source) return yield* Effect.die(new Error(`runtime record position not found: ${event.document}:${position}`))
        // The IR command is the record's full shape; the runtime record adds
        // only run-time values (a line's resolved text and identities).
        // Other kinds' record fields (e.g. a `set`'s evaluated value) are
        // not command fields and must not overwrite the command.
        const decoded = decodeRecord(source)
        const command = decoded.kind === "line" ? { ...decoded, ...lineRuntimeFields(runtimeRecord) } : decoded
        const cue = makeRecordCue(command, bindings, locale, readLines, castNames)
        if (cue === undefined) return yield* Effect.die(new Error(`unknown runtime record kind: ${String(command.kind)}`))
        cues.push(cue)
      }
      return cues
    })
  }
}

export class CueFactory extends Context.Service<CueFactory, CueFactoryShape>()("Lute.Player.CueFactory") {}

export interface CueFactoryOptions {
  readonly bundle: Bundle
  readonly bindings: Bindings
  readonly locale?: string
}

/** Installs a cue factory bound to a compiled bundle and player bindings. */
export const layer = (options: CueFactoryOptions): Layer.Layer<CueFactory> => Layer.succeed(CueFactory, makeFactory(options.bundle, options.bindings, options.locale ?? "en"))

/** Creates a cue factory value without installing an Effect layer. */
export const make = (options: CueFactoryOptions): CueFactoryShape => makeFactory(options.bundle, options.bindings, options.locale ?? "en")
