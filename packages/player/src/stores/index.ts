/** Versioned save slots and profile persistence for the player. */
import { Clock, Context, Effect, Fiber, Layer, Option, Ref, Schema, SubscriptionRef } from "effect"
import * as KeyValueStore from "effect/persistence/KeyValueStore"
import { Contract, Ir } from "@lute-lang/runtime"
import type { Settings, SlotId, SlotInfo } from "../ports/index.js"
import type { SaveRuntime } from "../model.js"

/** The save and profile format version (`saveVersion`, `profileVersion`): the release line. */
const VERSION: "0.52" = "0.52"

/** A malformed or unreadable persisted save. */
export class SaveCorrupt extends Schema.TaggedError<SaveCorrupt>()("SaveCorrupt", {
  message: Schema.String,
  slot: Schema.String
}) {}

/** A save was created by another project or format version. */
export class SaveIncompatible extends Schema.TaggedError<SaveIncompatible>()("SaveIncompatible", {
  message: Schema.String,
  slot: Schema.String
}) {}

/** A malformed or unreadable persisted profile. */
export class ProfileCorrupt extends Schema.TaggedError<ProfileCorrupt>()("ProfileCorrupt", {
  message: Schema.String
}) {}

const Backlog = Schema.Struct({
  lineId: Schema.String,
  voiceKey: Schema.String,
  speakerName: Schema.String,
  text: Schema.String,
  hasVoice: Schema.Boolean,
  texts: Schema.optionalKey(Schema.Record(Schema.String, Schema.String)),
  localeSegments: Schema.optionalKey(Schema.Record(Schema.String, Ir.segments))
})
const Log = Schema.Union([
  Schema.Struct({ seed: Contract.Seed }),
  Schema.Struct({ snapshot: Contract.Snapshot }),
  Schema.Struct({ input: Contract.Input })
])
// 0.51.0 §4.6: a declared raise the host has not delivered yet.
const PendingRaise = Schema.Struct({
  occasion: Schema.String,
  target: Schema.optionalKey(Schema.String),
  document: Schema.String,
  position: Schema.optionalKey(Schema.String),
  logIndex: Schema.Int
})
const Runtime = Schema.Struct({
  snapshot: Contract.Snapshot,
  log: Schema.Array(Log),
  backlog: Schema.Array(Backlog),
  pendingRaises: Schema.Array(PendingRaise)
})
const Save = Schema.Struct({
  saveVersion: Schema.Literal(VERSION),
  project: Schema.String,
  savedAt: Schema.String,
  label: Schema.String,
  runtime: Runtime,
  thumbnail: Schema.optionalKey(Schema.String)
})
const Metadata = Schema.Struct({
  saveVersion: Schema.Literal(VERSION),
  project: Schema.String,
  savedAt: Schema.String,
  label: Schema.String,
  thumbnail: Schema.optionalKey(Schema.String)
})
/** The metadata visible in a slot list. */
export type SaveMetadata = SlotInfo

export interface SaveData {
  readonly project: string
  readonly savedAt?: string
  readonly label: string
  readonly runtime: SaveRuntime
  readonly thumbnail?: string
}
export interface SaveStoreShape {
  readonly list: Effect.Effect<ReadonlyArray<SaveMetadata>, SaveCorrupt>
  readonly save: (slot: SlotId, save: SaveData) => Effect.Effect<void, SaveCorrupt>
  readonly load: (slot: SlotId) => Effect.Effect<SaveData, SaveCorrupt | SaveIncompatible>
  readonly delete: (slot: SlotId) => Effect.Effect<void, SaveCorrupt>
}

/** Persisted saves, keyed by project fingerprint and slot. */
export class SaveStore extends Context.Service<SaveStore, SaveStoreShape>()(
  "@lute-lang/player/SaveStore"
) {}
export interface SaveStoreOptions {
  readonly project: string
  readonly maxSlots?: number
}

const validSlot = (slot: SlotId, maxSlots: number): boolean =>
  slot === "quick" || slot === "auto" || (typeof slot === "number" && Number.isInteger(slot) && slot >= 1 && slot <= maxSlots)
const slotKey = (slot: SlotId): string => `save:${slot === "quick" || slot === "auto" ? slot : String(slot)}`
const metadataKey = (slot: SlotId): string => `${slotKey(slot)}:metadata`
const slots = (maxSlots: number): ReadonlyArray<SlotId> => ["quick", "auto", ...Array.from({ length: maxSlots }, (_, i) => i + 1)]

const SaveHeader = Schema.Struct({
  saveVersion: Schema.String,
  project: Schema.String
})

const decode = <S extends Schema.Codec<unknown, unknown, never, never>>(
  schema: S,
  raw: string
): Effect.Effect<S["Type"], Error, never> =>
  Effect.mapError(Schema.decodeUnknownEffect(Schema.fromJsonString(schema))(raw), (error) => new Error(error.message))
const encode = <S extends Schema.Codec<unknown, unknown, never, never>>(
  schema: S,
  value: S["Type"]
): Effect.Effect<string, Error, never> =>
  Effect.mapError(Schema.encodeEffect(Schema.fromJsonString(schema))(value), (error) => new Error(error.message))

/** Layer backed by Effect's KeyValueStore (use `KeyValueStore.layerMemory` in tests). */
export const layer = (
  options: SaveStoreOptions
): Layer.Layer<SaveStore, never, KeyValueStore.KeyValueStore> =>
  Layer.effect(SaveStore, Effect.gen(function* () {
    const store = yield* KeyValueStore.KeyValueStore
    const maxSlots = options.maxSlots ?? 24
    const list: SaveStoreShape["list"] = Effect.gen(function* () {
      const values: SaveMetadata[] = []
      for (const slot of slots(maxSlots)) {
        const raw = yield* Effect.mapError(store.get(metadataKey(slot)), (error) => new SaveCorrupt({ slot: String(slot), message: error.message }))
        if (raw === undefined) continue
        const metadata = yield* Effect.mapError(decode(Metadata, raw), (error) => new SaveCorrupt({ slot: String(slot), message: error.message }))
        if (metadata.project !== options.project) continue
        values.push({ slot, label: metadata.label, savedAt: metadata.savedAt, thumbnail: metadata.thumbnail })
      }
      return values
    })
    return {
      list,
      save: (slot: SlotId, save: SaveData) => {
        if (!validSlot(slot, maxSlots)) return Effect.fail(new SaveCorrupt({ slot: String(slot), message: "invalid save slot" }))
        return Effect.gen(function* () {
          if (save.project !== options.project) return yield* Effect.fail(new SaveCorrupt({ slot: String(slot), message: "save project does not match store" }))
          const savedAt = new Date(yield* Clock.currentTimeMillis).toISOString()
          const payload = {
            saveVersion: VERSION,
            project: save.project,
            savedAt,
            label: save.label,
            runtime: save.runtime,
            ...(save.thumbnail === undefined ? {} : { thumbnail: save.thumbnail })
          }
          const metadata = {
            saveVersion: VERSION,
            project: save.project,
            savedAt,
            label: save.label,
            ...(save.thumbnail === undefined ? {} : { thumbnail: save.thumbnail })
          }
          const encoded = yield* Effect.mapError(encode(Save, payload), (error) => new SaveCorrupt({ slot: String(slot), message: error.message }))
          const encodedMetadata = yield* Effect.mapError(encode(Metadata, metadata), (error) => new SaveCorrupt({ slot: String(slot), message: error.message }))
          yield* Effect.mapError(store.set(slotKey(slot), encoded), (error) => new SaveCorrupt({ slot: String(slot), message: error.message }))
          yield* Effect.mapError(store.set(metadataKey(slot), encodedMetadata), (error) => new SaveCorrupt({ slot: String(slot), message: error.message }))
        })
      },
      load: (slot: SlotId) => {
        if (!validSlot(slot, maxSlots)) return Effect.fail(new SaveCorrupt({ slot: String(slot), message: "invalid save slot" }))
        return Effect.gen(function* () {
          const raw = yield* Effect.mapError(store.get(slotKey(slot)), (error) => new SaveCorrupt({ slot: String(slot), message: error.message }))
          if (raw === undefined) return yield* Effect.fail(new SaveCorrupt({ slot: String(slot), message: "save slot is empty" }))
          const header = yield* Effect.mapError(decode(SaveHeader, raw), (error) => new SaveCorrupt({ slot: String(slot), message: error.message }))
          if (header.saveVersion !== VERSION) {
            return yield* Effect.fail(new SaveIncompatible({ slot: String(slot), message: "save format version does not match current version" }))
          }
          const decoded = yield* Effect.mapError(decode(Save, raw), (error) => new SaveCorrupt({ slot: String(slot), message: error.message }))
          if (decoded.project !== options.project || decoded.runtime.snapshot.project !== options.project) {
            return yield* Effect.fail(new SaveIncompatible({ slot: String(slot), message: "save project does not match current project" }))
          }
          const { runtime, project, savedAt, label, thumbnail } = decoded
          return { project, savedAt, label, runtime, ...(thumbnail === undefined ? {} : { thumbnail }) }
        })
      },
      delete: (slot: SlotId) => {
        if (!validSlot(slot, maxSlots)) return Effect.fail(new SaveCorrupt({ slot: String(slot), message: "invalid save slot" }))
        return Effect.gen(function* () {
          yield* Effect.mapError(store.remove(slotKey(slot)), (error) => new SaveCorrupt({ slot: String(slot), message: error.message }))
          yield* Effect.mapError(store.remove(metadataKey(slot)), (error) => new SaveCorrupt({ slot: String(slot), message: error.message }))
        })
      }
    }
  }))

/** Default player settings from §6.7. */
export const defaultSettings: Settings = {
  textSpeed: 40,
  autoDelay: 1,
  skipMode: "read",
  volumes: { master: 1, music: 1, sfx: 1, voice: 1 },
  locale: "en",
  windowOpacity: 1,
  readColour: true,
  rollbackDepth: 100
}

const Volumes = Schema.Struct({
  master: Schema.optionalKey(Schema.Number),
  music: Schema.optionalKey(Schema.Number),
  sfx: Schema.optionalKey(Schema.Number),
  voice: Schema.optionalKey(Schema.Number)
})
const SettingsSchema = Schema.Struct({
  textSpeed: Schema.optionalKey(Schema.Number),
  autoDelay: Schema.optionalKey(Schema.Number),
  skipMode: Schema.optionalKey(Schema.Literals(["read", "all"])),
  volumes: Schema.optionalKey(Volumes),
  locale: Schema.optionalKey(Schema.String),
  windowOpacity: Schema.optionalKey(Schema.Number),
  readColour: Schema.optionalKey(Schema.Boolean),
  rollbackDepth: Schema.optionalKey(Schema.Int)
})
/** The profile as this version writes it. */
const Profile = Schema.Struct({
  profileVersion: Schema.Literal(VERSION),
  project: Schema.String,
  readLines: Schema.Array(Schema.String),
  settings: SettingsSchema,
  lineage: Schema.optionalKey(Contract.Snapshot)
})
/**
 * A stored profile of any version (spec 0.47.0 §2.3): read marks and
 * settings are kept; the lineage only from a profile of this version, and
 * only when it decodes (else the profile has none). A profile is never
 * refused for its version.
 */
const StoredProfile = Schema.Struct({
  profileVersion: Schema.String,
  project: Schema.String,
  readLines: Schema.Array(Schema.String),
  settings: SettingsSchema,
  lineage: Schema.optionalKey(Schema.Unknown)
})
const decodeLineage = Schema.decodeUnknownOption(Contract.Snapshot)

/** Profile service containing read history, mutable player settings and the lineage. */
export interface ProfileStoreShape {
  readonly readLines: SubscriptionRef.SubscriptionRef<ReadonlySet<string>>
  readonly markRead: (lineId: string) => Effect.Effect<void, ProfileCorrupt>
  /** Remove lines from the read history (a host `restore` drops the marks of the abandoned presentation). */
  readonly unmarkRead: (lineIds: Iterable<string>) => Effect.Effect<void, ProfileCorrupt>
  readonly settings: SubscriptionRef.SubscriptionRef<Settings>
  readonly updateSettings: (settings: SettingsChanges) => Effect.Effect<void, ProfileCorrupt>
  /**
   * The lineage (spec 0.47.0 §2.3): the runtime snapshot taken when the
   * session last presented an `ended` output — the user tier and the ended
   * run that `newRun` turns into `prev.run.*`.
   */
  readonly lineage: SubscriptionRef.SubscriptionRef<Contract.Snapshot | undefined>
  /** Replace (or, with `undefined`, drop) the lineage; written at once, not debounced. */
  readonly setLineage: (lineage: Contract.Snapshot | undefined) => Effect.Effect<void, ProfileCorrupt>
}

/** Persisted read history, player preferences and lineage. */
export class ProfileStore extends Context.Service<ProfileStore, ProfileStoreShape>()(
  "@lute-lang/player/ProfileStore"
) {}

/** Options for a profile store. */
export interface ProfileStoreOptions {
  readonly project: string
  readonly key?: string
  /** Settings of a new profile (`LutePlayer.make` sets `locale` to the bundle's source locale). */
  readonly defaults?: Partial<Omit<Settings, "volumes">> & { readonly volumes?: Partial<Settings["volumes"]> }
  /** Delay before coalesced settings changes are written. */
  readonly debounceMs?: number
}


export type SettingsChanges = Partial<Omit<Settings, "volumes">> & { readonly volumes?: Partial<Settings["volumes"]> }
const mergeSettings = (defaults: Settings, changes: SettingsChanges): Settings => ({
  ...defaults,
  ...changes,
  volumes: { ...defaults.volumes, ...(changes.volumes ?? {}) }
})

/** Profile layer backed by Effect's KeyValueStore. */
export const profileLayer = (
  options: ProfileStoreOptions
): Layer.Layer<ProfileStore, ProfileCorrupt, KeyValueStore.KeyValueStore> =>
  Layer.effect(ProfileStore, Effect.gen(function* () {
    const store = yield* KeyValueStore.KeyValueStore
    const key = options.key ?? `profile:${options.project}`
    const defaults = mergeSettings(defaultSettings, options.defaults ?? {})
    const raw = yield* Effect.mapError(store.get(key), (error) => new ProfileCorrupt({ message: error.message }))
    let read = new Set<string>()
    let settings = defaults
    let lineage: Contract.Snapshot | undefined
    if (raw !== undefined) {
      const decoded = yield* Effect.mapError(decode(StoredProfile, raw), (error) => new ProfileCorrupt({ message: error.message }))
      if (decoded.project !== options.project) return yield* Effect.fail(new ProfileCorrupt({ message: "profile project does not match" }))
      read = new Set(decoded.readLines)
      settings = mergeSettings(defaults, decoded.settings)
      if (decoded.profileVersion === VERSION && decoded.lineage !== undefined) lineage = Option.getOrUndefined(decodeLineage(decoded.lineage))
    }
    const readRef = yield* SubscriptionRef.make<ReadonlySet<string>>(read)
    const settingsRef = yield* SubscriptionRef.make<Settings>(settings)
    const lineageRef = yield* SubscriptionRef.make<Contract.Snapshot | undefined>(lineage)
    const persist = Effect.gen(function* () {
      const current = yield* SubscriptionRef.get(lineageRef)
      const profile: typeof Profile.Type = {
        profileVersion: VERSION,
        project: options.project,
        readLines: [...(yield* SubscriptionRef.get(readRef))],
        settings: yield* SubscriptionRef.get(settingsRef),
        ...(current === undefined ? {} : { lineage: current })
      }
      const encoded = yield* encode(Profile, profile)
      yield* store.set(key, encoded)
    }).pipe(Effect.mapError((error) => new ProfileCorrupt({ message: error.message })))
    const pending = yield* Ref.make<Fiber.Fiber<void, ProfileCorrupt> | undefined>(undefined)
    const debounceMs = options.debounceMs ?? 250
    const schedulePersist = Effect.fn("Lute.Profile.schedulePersist")(function* () {
      const current = yield* Ref.get(pending)
      if (current !== undefined) yield* Fiber.interrupt(current)
      const fiber = yield* Effect.forkDetach(Effect.andThen(Effect.sleep(debounceMs), persist))
      yield* Ref.set(pending, fiber)
    })
    return {
      readLines: readRef,
      markRead: (lineId: string) => Effect.gen(function* () {
        const next = new Set(yield* SubscriptionRef.get(readRef))
        next.add(lineId)
        yield* SubscriptionRef.set(readRef, next)
        yield* persist
      }),
      unmarkRead: (lineIds: Iterable<string>) => Effect.gen(function* () {
        const next = new Set(yield* SubscriptionRef.get(readRef))
        for (const lineId of lineIds) next.delete(lineId)
        yield* SubscriptionRef.set(readRef, next)
        yield* persist
      }),
      settings: settingsRef,
      updateSettings: (changes: SettingsChanges) => Effect.gen(function* () {
        const next = mergeSettings(yield* SubscriptionRef.get(settingsRef), changes)
        yield* SubscriptionRef.set(settingsRef, next)
        yield* schedulePersist()
      }),
      lineage: lineageRef,
      setLineage: (next: Contract.Snapshot | undefined) => Effect.gen(function* () {
        yield* SubscriptionRef.set(lineageRef, next)
        yield* persist
      })
    }
  }))

/** In-memory persistence layer useful for tests and embedded hosts. */
export const memory = KeyValueStore.layerMemory
