import { Context, Effect, Layer, Option, Schema } from "effect"
import * as Runtime from "@lute-lang/runtime"
import type { Bindings } from "../bindings/schema.js"
import type { BridgeAwait, Cue, CueContext, CueFactoryShape, CueTiming, Event, Output, Stage, ChoiceAwait } from "../model.js"
import type { AudioPlay, ChoiceView, EffectSpec, LineView, PlayerHost, PluginCall, Segment, StageCue } from "../ports/index.js"

type Bundle = Runtime.Bundle.Bundle
type JsonRecord = { readonly [key: string]: unknown }
/** The bundle index `cast`: cast id → display name. */
type Cast = Readonly<Record<string, string>>

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
  // Spec 0.47.0 §2.7: a `presentation` cue clears the dialogue window, so it
  // runs in order — forked, it would wipe the batch's first line.
  const defaultWait = kind === "line" || kind === "sequence" || (kind === "video" && field(value, "display") === "show") || kind === "barrier" || kind === "presentation"
  return {
    delay: number(delayValue) ? delayValue : 0,
    duration: number(durationValue) ? durationValue : undefined,
    wait: boolean(explicitWait) ? explicitWait : defaultWait,
    timeline: number(timelineValue) ? timelineValue : undefined,
    at: number(atValue) ? atValue : undefined
  }
}

/**
 * The bound file of an asset id: its `assets` entry, or the id itself as a
 * file relative to `assetsBase`. Cues and restored stages name bound files
 * the same way; the host's `AssetLoader` / `AudioPort` resolve them against
 * `assetsBase` (`BindingsLoader.resolveFile`).
 */
export const assetFile = (bindings: Bindings, assetId: string): string => bindings.assets?.[assetId] ?? assetId

const filesForAsset = (bindings: Bindings, assetId: unknown): ReadonlyArray<string> =>
  string(assetId) ? [assetFile(bindings, assetId)] : []

/** The bound file of a voice key (`voice.pattern`), when voice is bound. */
export const voiceFile = (bindings: Bindings, voiceKey: string): string | undefined => {
  const pattern = bindings.voice?.pattern
  return pattern && voiceKey ? pattern.replaceAll("{voiceKey}", voiceKey) : undefined
}

/** A music record's or stage's file: `assetId` (see {@link assetFile}), else the bound `mood`. */
const musicFile = (bindings: Bindings, value: { readonly assetId?: unknown; readonly mood?: unknown }): string | undefined =>
  string(value.assetId) ? assetFile(bindings, value.assetId) : string(value.mood) ? bindings.music?.moods?.[value.mood] : undefined

/**
 * Every bound file a restored stage needs, resolved exactly as the cues and
 * {@link stageMusic} resolve them (ensured before `Presenter.restore` and the
 * music restart).
 */
export const stageFiles = (bindings: Bindings, stage: Stage): ReadonlyArray<string> => {
  const files: string[] = []
  const background = backgroundFile(bindings, stage.bg ?? {})
  if (background !== undefined) files.push(background)
  for (const actor of stage.actors) {
    const file = spriteFile(bindings, actor)
    if (file !== undefined) files.push(file)
  }
  files.push(...filesForAsset(bindings, stage.cg?.assetId), ...filesForAsset(bindings, stage.video?.assetId))
  const music = stageMusic(bindings, stage)
  if (music !== undefined) files.push(music.file)
  return files
}

const optionalStage = <T extends StageCue>(cue: T, duration: number | undefined, transition: EffectSpec | undefined): T => ({
  ...cue,
  ...(duration === undefined ? {} : { duration }),
  ...(transition === undefined ? {} : { transition })
})

/** A record's text for `locale`: its `texts[locale]` translation, else the source text. */
export const localizedText = (texts: unknown, locale: string, source: string): string => {
  const translated = isRecord(texts) ? texts[locale] : undefined
  return string(translated) ? translated : source
}

const lineView = (value: JsonRecord, locale: string, readLines: ReadonlySet<string>, cast: Cast): LineView => {
  const lineId = string(value.lineId) ? value.lineId : ""
  const speaker = string(value.speaker) ? value.speaker : "narrator"
  const role = string(value.role) ? value.role : "narration"
  const localizedSegments = record(value, "localeSegments")?.[locale]
  const texts = record(value, "texts")
  const translated = texts !== undefined && string(texts[locale])
  const sourceSegments = field(value, "segments")
  // A translation replaces the source segments: its own segments, else its plain text.
  const rawSegments = Array.isArray(localizedSegments) ? localizedSegments : !translated && Array.isArray(sourceSegments) ? sourceSegments : []
  const text = localizedText(texts, locale, string(value.text) ? value.text : "")
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
    speakerName: speakerName(cast, role, speaker),
    segments,
    read: readLines.has(lineId),
    ...(string(value.emotion) ? { emotion: value.emotion } : {}),
    ...(string(value.variant) || number(value.variant) ? { variant: String(value.variant) } : {})
  }
}

/** A line's display name: the index `cast` name of its speaker, empty for narration. */
export const speakerName = (cast: Cast, role: string, speaker: string): string =>
  role === "narration" || speaker === "narrator" ? "" : (cast[speaker] ?? speaker)

/**
 * Document → position → IR command. A runtime record's `document` is the
 * source document path (`game.lute`), not the artifact file
 * (`game.lute.json`): the index `documents` rows map both the path and the
 * document key to the artifact.
 */
const commandIndexFrom = (bundle: Bundle): Map<string, Map<string, JsonRecord>> => {
  const result = new Map<string, Map<string, JsonRecord>>()
  for (const document of bundle.index.documents) {
    const artifact = bundle.artifacts[document.artifact]
    if (artifact === undefined) continue
    const commands = new Map<string, JsonRecord>()
    const visit = (value: unknown): void => {
      if (isRecord(value)) {
        const position = positionOf(value)
        if (position && kindOf(value)) commands.set(position, value)
        for (const child of Object.values(value)) visit(child)
      } else if (Array.isArray(value)) for (const child of value) visit(child)
    }
    visit(artifact)
    result.set(document.path, commands)
    result.set(document.key, commands)
  }
  return result
}

/** The locales a loaded bundle carries (spec 0.47.0 §2.2). */
export interface BundleLocales {
  /** The language the bundle's source text is written in. */
  readonly source: string
  /** The source locale, then every compiled locale (sorted, without duplicates). */
  readonly locales: ReadonlyArray<string>
}

/**
 * A bundle's source locale — the index's `contentLang`, else the
 * `contentLang` of the first artifact (in `index.documents` order) whose meta
 * declares one, else `"en"` — and its compiled locales: the keys of every
 * `texts`/`promptTexts` map in its artifacts.
 */
export const bundleLocales = (bundle: Bundle): BundleLocales => {
  const indexLang = bundle.index.contentLang
  let source = indexLang !== undefined && indexLang.length > 0 ? indexLang : undefined
  const compiled = new Set<string>()
  const visit = (value: unknown): void => {
    if (Array.isArray(value)) for (const child of value) visit(child)
    else if (isRecord(value)) {
      for (const [key, child] of Object.entries(value)) {
        if ((key === "texts" || key === "promptTexts") && isRecord(child)) for (const locale of Object.keys(child)) compiled.add(locale)
        else visit(child)
      }
    }
  }
  for (const document of bundle.index.documents) {
    const artifact = bundle.artifacts[document.artifact]
    const contentLang = field(record(artifact, "meta"), "contentLang")
    if (source === undefined && string(contentLang) && contentLang.length > 0) source = contentLang
    visit(artifact)
  }
  const sourceLocale = source ?? "en"
  compiled.delete(sourceLocale)
  return { source: sourceLocale, locales: [sourceLocale, ...[...compiled].sort()] }
}

/** A line record's translations, as a backlog entry keeps them. */
export interface LineTranslations {
  readonly texts?: Readonly<Record<string, string>>
  readonly localeSegments?: Readonly<Record<string, Runtime.Ir.segments>>
}

const decodeTranslations = Schema.decodeUnknownOption(Schema.Struct({
  texts: Schema.optionalKey(Schema.Record(Schema.String, Schema.String)),
  localeSegments: Schema.optionalKey(Schema.Record(Schema.String, Runtime.Ir.segments))
}))

/**
 * The translations a runtime `line` record carries: its `texts` and
 * `localeSegments`, each `{{…}}` marker already replaced by the value the
 * runtime rendered (spec 0.47.0 §2.2), when it has any.
 */
export const recordTranslations = (value: { readonly texts?: unknown; readonly localeSegments?: unknown }): LineTranslations | undefined => {
  const decoded = decodeTranslations({
    ...(value.texts === undefined ? {} : { texts: value.texts }),
    ...(value.localeSegments === undefined ? {} : { localeSegments: value.localeSegments })
  })
  if (Option.isNone(decoded)) return undefined
  const { texts, localeSegments } = decoded.value
  if (texts === undefined && localeSegments === undefined) return undefined
  return { ...(texts === undefined ? {} : { texts }), ...(localeSegments === undefined ? {} : { localeSegments }) }
}

// The record's rendered text, segments and translations replace the IR's
// `{{…}}` templates.
const LINE_RUNTIME_FIELDS = ["lineId", "role", "speaker", "text", "voiceKey", "segments", "texts", "localeSegments"] as const
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

/** A background's file: the bound location (and time), else its `assetId` (see {@link assetFile}). */
const backgroundFile = (bindings: Bindings, value: JsonRecord): string | undefined => {
  const location = string(value.location) ? value.location : undefined
  const time = string(value.time) ? value.time : undefined
  const background = location && bindings.backgrounds ? bindings.backgrounds[location] : undefined
  if (background !== undefined) return time && background.times?.[time] ? background.times[time] : background.image
  return string(value.assetId) ? assetFile(bindings, value.assetId) : undefined
}

const spriteFile = (bindings: Bindings, value: JsonRecord): string | undefined => {
  const character = string(value.character) && bindings.characters ? bindings.characters[value.character] : undefined
  if (!character) return undefined
  const costume = string(value.costume) && character.costumes ? character.costumes[value.costume] : undefined
  if (costume) return string(value.emotion) && costume.emotions?.[value.emotion] ? costume.emotions[value.emotion] : costume.default
  return string(value.emotion) && character.sprite.emotions?.[value.emotion] ? character.sprite.emotions[value.emotion] : character.sprite.default
}

/**
 * A music record's or stage's `volume` member, as the gain `bindings.volumes`
 * maps it to (clamped to [0, 1]). Absent → 1; a member without a finite gain
 * → 0 (`lute check` rejects it; a stale build plays silent, never loud).
 * Numeric strings are members like any other, never parsed as numbers.
 */
export const musicVolume = (bindings: Bindings, value: unknown): number => {
  if (value === undefined) return 1
  const gain = string(value) ? bindings.volumes?.[value] : undefined
  return number(gain) && Number.isFinite(gain) ? Math.min(1, Math.max(0, gain)) : 0
}

/**
 * The music a restored stage plays (load, rollback, `restore`): none when the
 * stage has no music or its `playback` is `stop` / `fadeOut` (spec 0.41.0 §7.7).
 */
export const stageMusic = (bindings: Bindings, stage: Stage): AudioPlay | undefined => {
  const music = stage.music
  if (music == null || music.playback === "stop" || music.playback === "fadeOut") return undefined
  const file = musicFile(bindings, music)
  return file === undefined ? undefined : { file, loop: true, volume: musicVolume(bindings, music.volume), fade: 0, restart: false }
}

/**
 * A `vfx` type's effect (spec 0.41.0 §7.7): a catalogue binding gives its
 * effect; a custom binding gives `{ effect: "custom", name: name ?? type }`
 * and its `duration`; an unbound type gives `{ effect: "custom", name: type }`.
 */
const vfxEffect = (bindings: Bindings, type: string): { readonly effect: EffectSpec; readonly duration: number | undefined } => {
  const binding = bindings.vfx?.[type]
  if (binding === undefined) return { effect: { effect: "custom", name: type, params: {} }, duration: undefined }
  if (binding.effect === "custom") {
    return { effect: { effect: "custom", name: binding.name ?? type, params: {} }, duration: binding.duration ?? undefined }
  }
  return { effect: effect(binding) ?? { effect: binding.effect, params: {} }, duration: undefined }
}

const makeRecordCue = (value: JsonRecord, bindings: Bindings, locale: string, readLines: ReadonlySet<string>, cast: Cast): Cue | undefined => {
  const kind = kindOf(value)
  if (!kind) return undefined
  const timing = timingOf(value, kind)
  const duration = timing.duration
  switch (kind) {
    case "line": {
      const line = lineView(value, locale, readLines, cast)
      const voice = voiceFile(bindings, line.voiceKey)
      return cueBase("line", timing, false, voice ? [voice] : [], `line ${line.lineId}`, (ctx) => Effect.gen(function* () {
        if (voice) {
          const played = ctx.audio.play("voice", { file: voice, loop: false, volume: 1, fade: 0, restart: true })
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
      const file = backgroundFile(bindings, value)
      return cueBase("bg", timing, true, file ? [file] : [], `background ${String(value.location ?? "")}`, (ctx) => ctx.presenter.stage(bg, "normal"), (ctx) => ctx.presenter.stage(bg, "skip"))
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
      const file = musicFile(bindings, value)
      const playback = string(value.playback) ? value.playback : undefined
      // Spec 0.41.0 §7.7: `stop: true` / `stop` stop at once (or over `duration`),
      // `fadeOut` over `duration ?? 1`; `start` plays from the start; anything
      // else changes to the file (the file already playing only changes volume).
      const stopFade = value.stop === true || playback === "stop" ? duration ?? 0 : playback === "fadeOut" ? duration ?? 1 : undefined
      const audio: AudioPlay | undefined = file ? { file, loop: true, volume: musicVolume(bindings, value.volume), fade: duration ?? 0, restart: playback === "start" } : undefined
      const describe = `music ${stopFade !== undefined ? "stop" : playback ?? String(value.mood ?? value.assetId ?? "")}`
      return cueBase("music", timing, true, stopFade === undefined && file ? [file] : [], describe,
        (ctx) => stopFade !== undefined ? ctx.audio.stop("music", stopFade) : audio ? ctx.audio.play("music", audio) : Effect.void,
        (ctx) => stopFade !== undefined ? ctx.audio.stop("music", 0) : audio ? ctx.audio.play("music", { ...audio, fade: 0 }) : Effect.void)
    }
    case "sfx": {
      const file = string(value.assetId) ? assetFile(bindings, value.assetId) : string(value.sound) ? bindings.sfx?.[value.sound] : undefined
      const audio: AudioPlay | undefined = file ? { file, loop: false, volume: 1, fade: 0, restart: true } : undefined
      return cueBase("sfx", timing, false, file ? [file] : [], `sfx ${String(value.sound ?? "")}`, (ctx) => audio ? ctx.audio.play("sfx", audio) : Effect.void, noop)
    }
    case "vfx": {
      const type = string(value.type) ? value.type : ""
      const { effect: vfx, duration: bound } = vfxEffect(bindings, type)
      const vfxDuration = duration ?? bound
      const stage: StageCue = optionalStage({ _tag: "Vfx" as const, effect: vfx, duration: vfxDuration }, vfxDuration, undefined)
      return cueBase("vfx", timing, false, [], `vfx ${type}`, (ctx) => ctx.presenter.stage(stage, "normal"), (ctx) => ctx.presenter.stage(stage, "skip"))
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

const decodeJsonRecord = Schema.decodeUnknownOption(Schema.Record(Schema.String, Schema.Json))

/**
 * A `plugin` record whose tag has a host `plugin` handler: a waiting cue that
 * runs the handler at the record's place, in skip mode too (a game action is
 * not presentation). Spec 0.41.0 §7.6.
 */
const pluginCue = (
  handler: (call: PluginCall) => Effect.Effect<void>,
  runtimeRecord: JsonRecord,
  command: JsonRecord,
  document: string,
  position: string
): Cue => {
  const tag = string(runtimeRecord.tag) ? runtimeRecord.tag : String(command.tag)
  const attrs: BridgeAwait["attrs"] = Option.getOrElse(decodeJsonRecord(runtimeRecord.attrs), () => ({}))
  // 0.49.0 §3.1: the record's plugin stamps ride beside `attrs`; the runtime
  // omits them when the command has none, and so does the call.
  const stamps = Option.getOrUndefined(decodeJsonRecord(runtimeRecord.stamps))
  const call: PluginCall = { tag, attrs, ...(stamps === undefined ? {} : { stamps }), document, position }
  const run = (_ctx: CueContext): Effect.Effect<void> => handler(call)
  return cueBase("plugin", { ...timingOf(command, "plugin"), wait: true }, false, [], `plugin ${tag}`, run, run)
}

/** The options each `make` factory was built from, so host handlers can be merged in. */
const factoryOptions = new WeakMap<CueFactoryShape, CueFactoryOptions>()
const makeFactory = (options: CueFactoryOptions): CueFactoryShape => {
  const { bundle, bindings } = options
  const commands = commandIndexFrom(bundle)
  const cast: Cast = bundle.index.cast ?? {}
  const plugins = options.plugin ?? {}
  const choiceView = (await_: ChoiceAwait, locale: string): ChoiceView => {
    const source = commands.get(await_.menu.document)?.get(await_.menu.position)
    const authoredOptions = Array.isArray(source?.options) ? source.options : []
    const authoredById: Record<string, JsonRecord> = {}
    for (const option of authoredOptions) if (isRecord(option) && string(option.id)) authoredById[option.id] = option
    const textOf = (value: JsonRecord | undefined, fallback: string): string =>
      localizedText(field(value, "texts"), locale, string(value?.text) ? value.text : fallback)
    const sourcePrompt = field(source, "prompt")
    const fallbackPrompt = string(sourcePrompt) ? sourcePrompt : await_.menu.prompt ?? undefined
    const prompt = fallbackPrompt === undefined ? undefined : localizedText(field(source, "promptTexts"), locale, fallbackPrompt)
    // 0.50.0 §3.6: the element's and each option's plugin stamps, beside and
    // never merged into the authored fields; omitted when none is authored.
    const stampsOf = (value: JsonRecord | undefined) => {
      const stamps = Option.getOrUndefined(decodeJsonRecord(field(value, "stamps")))
      return stamps === undefined ? {} : { stamps }
    }
    return {
      id: await_.menu.id,
      construct: await_.menu.construct === "hub" ? "hub" : "branch",
      prompt,
      timeout: await_.menu.timeout ?? undefined,
      default: await_.menu.default ?? undefined,
      ...stampsOf(source),
      options: await_.menu.options.map((option) => {
        const authoredOption = authoredById[option.id]
        return { id: option.id, text: textOf(authoredOption, option.id), verdict: option.verdict, once: option.once, exit: option.exit, ...stampsOf(authoredOption) }
      })
    }
  }
  return {
    choiceView,
    fromOutput: Effect.fn("Lute.Player.CueFactory.fromOutput")(function* (output: Output, readLines: ReadonlySet<string>, locale: string) {
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
        // Staging records carry resolved fields in `attrs`; the artifact is
        // consulted only for the record kind/envelope (and line presentation
        // text/cast). Never recover a staging media field from the artifact.
        const decoded = decodeRecord(source)
        // Timing is not a record field: it stays on the artifact command.
        const command = decoded.kind === "line"
          ? { ...decoded, ...lineRuntimeFields(runtimeRecord) }
          : decoded.family === "staging"
            ? { ...(record(runtimeRecord, "attrs") ?? {}), kind: decoded.kind, family: decoded.family, position: decoded.position, ...(decoded.timing === undefined ? {} : { timing: decoded.timing }) }
            : decoded
        // Only the `plugin` record itself is a hook — not the effect records
        // (`set` … `effectOf`) that share its position — and never a call the
        // host answered through `awaitBridge`.
        const handler = runtimeRecord.kind === "plugin" && command.kind === "plugin" && !("answered" in runtimeRecord) && string(command.tag) ? plugins[command.tag] : undefined
        const cue = handler !== undefined
          ? pluginCue(handler, runtimeRecord, command, event.document, position)
          : makeRecordCue(command, bindings, locale, readLines, cast)
        if (cue === undefined) return yield* Effect.die(new Error(`unknown runtime record kind: ${String(command.kind)}`))
        cues.push(cue)
      }
      return cues
    })
  }
}

/**
 * A pre-composed factory with `plugin` handlers merged in; `plugin` wins a
 * duplicate tag. `undefined` when there are handlers to merge and the factory
 * was not built by {@link make}.
 */
export const mergePlugin = (factory: CueFactoryShape, plugin: PlayerHost["plugin"] | undefined): CueFactoryShape | undefined => {
  if (plugin === undefined) return factory
  const built = factoryOptions.get(factory)
  return built === undefined ? undefined : make({ ...built, plugin: { ...built.plugin, ...plugin } })
}
export class CueFactory extends Context.Service<CueFactory, CueFactoryShape>()("Lute.Player.CueFactory") {}

export interface CueFactoryOptions {
  readonly bundle: Bundle
  readonly bindings: Bindings
  /** Host `plugin` handlers: their records become waiting cues (spec 0.41.0 §7.6). */
  readonly plugin?: PlayerHost["plugin"]
}

/**
 * Installs a cue factory bound to a compiled bundle and player bindings.
 * `fromOutput` returns exactly one cue per output event, in event order
 * (normative: a host MAY wrap the factory relying on it).
 */
export const layer = (options: CueFactoryOptions): Layer.Layer<CueFactory> => Layer.succeed(CueFactory, make(options))

/** Creates a cue factory value without installing an Effect layer (one cue per output event, in order). */
export const make = (options: CueFactoryOptions): CueFactoryShape => {
  const factory = makeFactory(options)
  factoryOptions.set(factory, options)
  return factory
}
