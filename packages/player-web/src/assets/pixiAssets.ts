import { Texture } from "pixi.js"
import { Effect, Fiber, Scope, Stream, SubscriptionRef } from "effect"
import { AssetMissing, type AssetLoader } from "@lute-lang/player"

/** Backend seam for PixiAssets; tests can use an in-memory loader. */
export interface AssetBackend {
  readonly loadTexture: (url: string) => Effect.Effect<Texture, unknown>
  readonly unloadTexture: (url: string, texture: Texture) => Effect.Effect<void, never>
  readonly loadAudio: (url: string) => Effect.Effect<void, unknown>
  readonly unloadAudio: (url: string) => Effect.Effect<void, never>
}

/** Constructor options for {@link PixiAssets}. */
export interface PixiAssetsOptions {
  readonly assetsBase?: string
  readonly backend: AssetBackend
  readonly concurrency?: number
  readonly graceSeconds?: number
}

interface AssetRecord {
  readonly file: string
  readonly url: string
  readonly kind: "texture" | "video" | "audio"
  readonly value: Texture | undefined
  refs: number
}

const AUDIO_EXTENSIONS = /\.(?:aac|flac|m4a|mp3|ogg|wav|webm)(?:[?#].*)?$/i
const VIDEO_EXTENSIONS = /\.(?:avi|m4v|mov|mp4|webm)(?:[?#].*)?$/i


const uniqueFiles = (files: ReadonlyArray<string>): ReadonlyArray<string> => {
  const result: Array<string> = []
  const seen = new Set<string>()
  for (const file of files) {
    if (file.length > 0 && !seen.has(file)) {
      seen.add(file)
      result.push(file)
    }
  }
  return result
}

const pathKind = (file: string): AssetRecord["kind"] => {
  if (VIDEO_EXTENSIONS.test(file)) return "video"
  if (AUDIO_EXTENSIONS.test(file)) return "audio"
  return "texture"
}
/** Resolve a binding-relative file against the configured assets base URL. */
const resolveAssetUrl = (file: string, base: string): string => {
  try {
    return new URL(file, base).href
  } catch {
    const origin = typeof globalThis.location === "object" ? globalThis.location.href : "http://localhost/"
    return new URL(file, new URL(base, origin)).href
  }
}

/**
 * Pixi-backed, scoped and ref-counted AssetLoader. A scope that calls ensure owns
 * one reference to every requested file; the references are released by its finalizer.
 */
export class PixiAssets implements AssetLoader {
  readonly progress: Stream.Stream<number>

  private readonly backend: AssetBackend
  private readonly base: string
  private readonly maxConcurrency: number
  private readonly graceSeconds: number
  private readonly progressRef: SubscriptionRef.SubscriptionRef<number>
  private readonly records = new Map<string, AssetRecord>()
  private readonly unloadFibers = new Map<string, Fiber.Fiber<void, never>>()

  constructor(options: PixiAssetsOptions) {
    const base = options.assetsBase ?? "assets/"
    this.base = base
    this.backend = options.backend
    this.maxConcurrency = Math.max(1, Math.floor(options.concurrency ?? 4))
    this.graceSeconds = Math.max(0, options.graceSeconds ?? 0.25)
    this.progressRef = Effect.runSync(SubscriptionRef.make(0))
    this.progress = Stream.concat(Stream.succeed(0), SubscriptionRef.changes(this.progressRef))
  }

  /** Ensure all files are loaded and retain them until the caller's scope closes. */
  readonly ensure = (files: ReadonlyArray<string>): Effect.Effect<void, AssetMissing, Scope.Scope> => {
    const requested = uniqueFiles(files)
    return Effect.gen({ self: this }, function* () {
      const self = this
      yield* SubscriptionRef.set(self.progressRef, requested.length === 0 ? 1 : 0)
      let completed = 0
      yield* Effect.forEach(
        requested,
        (file) => self.acquire(file).pipe(
          Effect.tap(() => Effect.sync(() => { completed += 1 })),
          Effect.tap(() => SubscriptionRef.set(self.progressRef, requested.length === 0 ? 1 : completed / requested.length))
        ),
        { concurrency: self.maxConcurrency, discard: true }
      )
      yield* Effect.addFinalizer(() => self.release(requested))
      yield* SubscriptionRef.set(self.progressRef, 1)
    })
  }

  /** Return a loaded image/video texture, or undefined before ensure. */
  readonly texture = (file: string): Texture | undefined => {
    const record = this.records.get(file)
    return record?.kind === "texture" || record?.kind === "video" ? record.value : undefined
  }

  /** Return a loaded video texture, or undefined before ensure. */
  readonly video = (file: string): Texture | undefined => {
    const record = this.records.get(file)
    return record?.kind === "video" ? record.value : undefined
  }

  private readonly acquire = (file: string): Effect.Effect<void, AssetMissing> =>
    Effect.gen({ self: this }, function* () {
      const self = this
      const pending = self.unloadFibers.get(file)
      if (pending !== undefined) {
        yield* Fiber.interrupt(pending)
        self.unloadFibers.delete(file)
      }
      const existing = self.records.get(file)
      if (existing !== undefined) {
        existing.refs += 1
        return
      }
      const url = resolveAssetUrl(file, self.base)
      const kind = pathKind(file)
      const value = kind === "audio"
        ? undefined
        : yield* self.backend.loadTexture(url).pipe(
          Effect.mapError((error) => new AssetMissing({
            file,
            message: error instanceof Error ? error.message : String(error)
          }))
        )
      yield* kind === "audio"
        ? self.backend.loadAudio(url).pipe(
          Effect.mapError((error) => new AssetMissing({
            file,
            message: error instanceof Error ? error.message : String(error)
          }))
        )
        : Effect.void
      self.records.set(file, { file, url, kind, value, refs: 1 })
    })

  private readonly release = (files: ReadonlyArray<string>): Effect.Effect<void> =>
    Effect.gen({ self: this }, function* () {
      const self = this
      for (const file of files) {
        const record = self.records.get(file)
        if (record === undefined) continue
        record.refs -= 1
        if (record.refs > 0) continue
        if (self.graceSeconds === 0) {
          yield* self.unload(record)
          continue
        }
        const fiber = yield* Effect.forkDetach(
          Effect.sleep(self.graceSeconds).pipe(Effect.flatMap(() => self.unload(record)))
        )
        self.unloadFibers.set(file, fiber)
      }
    })

  private readonly unload = (record: AssetRecord): Effect.Effect<void> =>
    Effect.gen({ self: this }, function* () {
      const self = this
      const current = self.records.get(record.file)
      if (current !== record || record.refs > 0) return
      self.records.delete(record.file)
      self.unloadFibers.delete(record.file)
      if (record.kind === "audio") {
        yield* self.backend.unloadAudio(record.url)
      } else if (record.value !== undefined) {
        yield* self.backend.unloadTexture(record.url, record.value)
      }
    })
}

/**
 * Collect binding-referenced files in stable insertion order. Dynamic keys such
 * as effect names, colours, labels and flow occasions are intentionally ignored.
 */
export const assetsFromBindings = (bindings: unknown): ReadonlyArray<string> => {
  const files: Array<string> = []
  const seen = new Set<string>()
  const add = (value: unknown): void => {
    if (typeof value === "string" && value.length > 0 && !seen.has(value)) {
      seen.add(value)
      files.push(value)
    }
  }
  const isRecord = (value: unknown): value is Record<string, unknown> =>
    value !== null && typeof value === "object" && !Array.isArray(value)
  const values = (value: unknown): ReadonlyArray<unknown> =>
    isRecord(value) ? Object.values(value) : []
  const object = (value: unknown): Record<string, unknown> | undefined =>
    isRecord(value) ? value : undefined

  const root = object(bindings)
  if (root === undefined) return files
  const assets = object(root.assets)
  if (assets !== undefined) for (const value of values(assets)) add(value)
  const backgrounds = object(root.backgrounds)
  if (backgrounds !== undefined) {
    for (const background of values(backgrounds)) {
      const entry = object(background)
      if (entry !== undefined) {
        add(entry.image)
        const times = object(entry.times)
        if (times !== undefined) for (const value of values(times)) add(value)
      }
    }
  }
  const characters = object(root.characters)
  if (characters !== undefined) {
    const visitSprite = (sprite: unknown): void => {
      const entry = object(sprite)
      if (entry === undefined) return
      add(entry.default)
      const emotions = object(entry.emotions)
      if (emotions !== undefined) for (const value of values(emotions)) add(value)
    }
    for (const character of values(characters)) {
      const entry = object(character)
      if (entry === undefined) continue
      visitSprite(entry.sprite)
      const costumes = object(entry.costumes)
      if (costumes !== undefined) for (const costume of values(costumes)) visitSprite(costume)
    }
  }
  const music = object(root.music)
  const moods = music === undefined ? undefined : object(music.moods)
  if (moods !== undefined) for (const value of values(moods)) add(value)
  const sfx = object(root.sfx)
  if (sfx !== undefined) for (const value of values(sfx)) add(value)
  const sequences = object(root.sequences)
  if (sequences !== undefined) {
    for (const sequence of values(sequences)) add(object(sequence)?.video)
  }
  add(object(root.voice)?.pattern)
  const title = object(object(root.ui)?.title)
  if (title !== undefined) add(title.background)
  return files
}