import { Deferred, Effect, Fiber, Option, SubscriptionRef } from "effect"
import { sound, webaudio, type IMediaInstance } from "@pixi/sound"
import type { AudioChannel, AudioPlay, AudioPort, Volumes } from "@lute-lang/player"

/** A typed failure raised by the sound backend while decoding or starting media. */
export class AudioDecodeError extends Error {
  readonly _tag = "AudioDecodeError"

  constructor(readonly file: string, cause: unknown) {
    super(`Unable to decode or play audio ${file}: ${cause instanceof Error ? cause.message : String(cause)}`)
  }
}

/** The playback object required by {@link PixiAudio}; useful for deterministic unit tests. */
export interface AudioPlayback {
  readonly setVolume: (volume: number) => void
  readonly stop: () => void
  readonly ended: Effect.Effect<void>
}

/** Backend seam for PixiAudio. The default implementation delegates to @pixi/sound. */
export interface AudioBackend {
  readonly load: (file: string) => Effect.Effect<void, AudioDecodeError>
  readonly play: (file: string, options: { readonly loop: boolean; readonly volume: number }) => Effect.Effect<AudioPlayback, AudioDecodeError>
  readonly unlock: Effect.Effect<void, AudioDecodeError>
}

interface ActivePlayback {
  readonly channel: AudioChannel
  readonly inputVolume: number
  readonly playback: AudioPlayback
  fade: Fiber.Fiber<void, never> | undefined
}

const clamp = (value: number): number => Math.max(0, Math.min(1, value))
const positive = (value: number): number => Number.isFinite(value) && value > 0 ? value : 0

const waitForMediaEnd = (instance: IMediaInstance): Effect.Effect<void> =>
  Effect.callback<void>((resume) => {
    const finish = () => resume(Effect.succeed(undefined))
    instance.once("end", finish)
    instance.once("stop", finish)
    return Effect.sync(() => {
      instance.off("end", finish)
      instance.off("stop", finish)
    })
  })

const defaultBackend: AudioBackend = {
  load: (file) =>
    Effect.callback<void, AudioDecodeError>((resume) => {
      if (sound.exists(file)) {
        resume(Effect.succeed(undefined))
        return
      }
      try {
        sound.add(file, {
          url: file,
          preload: true,
          loaded: (error) => {
            resume(error === undefined ? Effect.succeed(undefined) : Effect.fail(new AudioDecodeError(file, error)))
          }
        })
      } catch (error) {
        resume(Effect.fail(new AudioDecodeError(file, error)))
      }
    }),
  play: (file, options) =>
    Effect.tryPromise({
      try: () => Promise.resolve(sound.play(file, {
        loop: options.loop,
        volume: options.volume
      })).then((instance) => ({
        setVolume: (volume: number) => { instance.volume = clamp(volume) },
        stop: () => instance.stop(),
        ended: options.loop ? Effect.never : waitForMediaEnd(instance)
      })),
      catch: (error) => new AudioDecodeError(file, error)
    }),
  unlock: Effect.tryPromise({
    try: () => {
      const context = sound.context
      if (context instanceof webaudio.WebAudioContext && context.audioContext.state === "suspended") {
        return context.audioContext.resume().then(() => undefined)
      }
      return Promise.resolve(undefined)
    },
    catch: (error) => new AudioDecodeError("<audio-context>", error)
  })
}

/** Constructor options for {@link PixiAudio}. */
export interface PixiAudioOptions {
  readonly backend?: AudioBackend
  readonly logger?: (message: string, error?: unknown) => void
  readonly initialVolumes?: Volumes
}

/**
 * PixiJS/@pixi-sound implementation of the player AudioPort.
 *
 * Playback is deliberately best-effort: an absent optional file is logged and does
 * not stop the story. Fade jobs run in detached Effect fibers so superseding a
 * channel interrupts the old Clock-driven fade without interrupting the caller.
 */
export class PixiAudio implements AudioPort {
  readonly unlocked: SubscriptionRef.SubscriptionRef<boolean>
  readonly unlock: Effect.Effect<void, AudioDecodeError>
  private readonly backend: AudioBackend
  private readonly logger: (message: string, error?: unknown) => void
  private readonly active: Record<AudioChannel, Array<ActivePlayback>> = {
    music: [],
    sfx: [],
    voice: []
  }
  private currentVolumes: Volumes
  private readonly gate = Deferred.makeUnsafe<void>()
  constructor(options: PixiAudioOptions = {}) {
    this.backend = options.backend ?? defaultBackend
    this.logger = options.logger ?? ((message, error) => console.warn(message, error))
    this.currentVolumes = options.initialVolumes ?? { master: 1, music: 1, sfx: 1, voice: 1 }
    this.unlocked = Effect.runSync(SubscriptionRef.make(false))
    const self = this
    this.unlock = Effect.gen(function* () {
      if (!SubscriptionRef.getUnsafe(self.unlocked)) {
        yield* self.backend.unlock.pipe(Effect.catch((error) => {
          self.logger("Audio context could not be resumed", error)
          return Effect.void
        }))
        yield* SubscriptionRef.set(self.unlocked, true)
        yield* Deferred.succeed(self.gate, undefined)
      }
    })
  }


  /** Play a channel sound, waiting for finite sounds to end. */
  readonly play = (channel: AudioChannel, soundToPlay: AudioPlay): Effect.Effect<void> =>
    Effect.gen({ self: this }, function* () {
      const self = this
      if (!SubscriptionRef.getUnsafe(self.unlocked)) {
        yield* Deferred.await(self.gate)
      }
      const loaded = yield* self.backend.load(soundToPlay.file).pipe(
        Effect.catch((error) => {
          self.logger(`Audio file missing or undecodable: ${soundToPlay.file}`, error)
          return Effect.fail(error)
        }),
        Effect.option
      )
      if (Option.isNone(loaded)) return
      const inputVolume = clamp(soundToPlay.volume)
      const playback = yield* self.backend.play(soundToPlay.file, {
        loop: soundToPlay.loop,
        volume: self.effectiveVolume(channel, inputVolume)
      }).pipe(
        Effect.catch((error) => {
          self.logger(`Audio playback failed: ${soundToPlay.file}`, error)
          return Effect.fail(error)
        }),
        Effect.option
      )
      if (Option.isNone(playback)) return
      if (channel === "voice") yield* self.stopVoiceImmediately()
      if (channel === "music") yield* self.replaceMusic(soundToPlay.fade)
      const record: ActivePlayback = {
        channel,
        inputVolume,
        playback: playback.value,
        fade: undefined
      }
      self.active[channel].push(record)
      record.playback.setVolume(soundToPlay.fade > 0 ? 0 : self.effectiveVolume(channel, inputVolume))
      if (positive(soundToPlay.fade) > 0) {
        record.fade = yield* self.startFade(record, 0, 1, soundToPlay.fade, false)
      }
      if (!soundToPlay.loop) {
        yield* record.playback.ended
        self.removeRecord(record)
      }
    })

  /** Fade and stop every playback currently in a channel. */
  readonly stop = (channel: AudioChannel, fade: number): Effect.Effect<void> =>
    Effect.gen({ self: this }, function* () {
      const self = this
      const records = self.active[channel].slice()
      if (records.length === 0) return
      for (const record of records) yield* self.interruptFade(record)
      const duration = positive(fade)
      if (duration === 0) {
        for (const record of records) {
          record.playback.stop()
          self.removeRecord(record)
        }
        return
      }
      yield* Effect.all(records.map((record) => self.fadeOutAndStop(record, duration)), { concurrency: "unbounded" })
    })

  /** Set master/channel gains and apply them immediately to every active instance. */
  readonly setVolumes = (volumes: Volumes): Effect.Effect<void> =>
    Effect.sync(() => {
      this.currentVolumes = {
        master: clamp(volumes.master),
        music: clamp(volumes.music),
        sfx: clamp(volumes.sfx),
        voice: clamp(volumes.voice)
      }
      for (const channel of ["music", "sfx", "voice"] as const) {
        for (const record of this.active[channel]) {
          record.playback.setVolume(this.effectiveVolume(channel, record.inputVolume))
        }
      }
    })

  private effectiveVolume(channel: AudioChannel, inputVolume: number): number {
    return clamp(inputVolume * this.currentVolumes.master * this.currentVolumes[channel])
  }

  private removeRecord(record: ActivePlayback): void {
    const records = this.active[record.channel]
    const index = records.indexOf(record)
    if (index >= 0) records.splice(index, 1)
  }

  private readonly interruptFade = (record: ActivePlayback): Effect.Effect<void> =>
    record.fade === undefined ? Effect.void : Fiber.interrupt(record.fade).pipe(
      Effect.asVoid,
      Effect.tap(() => Effect.sync(() => { record.fade = undefined }))
    )

  private readonly startFade = (
    record: ActivePlayback,
    from: number,
    to: number,
    duration: number,
    stopAtEnd: boolean
  ): Effect.Effect<Fiber.Fiber<void, never>> => {
    const steps = Math.max(1, Math.ceil(duration * 20))
    const interval = duration / steps
    const fade = Effect.forEach(
      Array.from({ length: steps }, (_, index) => index + 1),
      (step) => Effect.sleep(interval).pipe(
        Effect.flatMap(() => Effect.sync(() => {
          const factor = from + (to - from) * (step / steps)
          record.playback.setVolume(this.effectiveVolume(record.channel, record.inputVolume) * factor)
        }))
      ),
      { discard: true }
    ).pipe(
      Effect.flatMap(() => Effect.sync(() => {
        record.playback.setVolume(this.effectiveVolume(record.channel, record.inputVolume) * to)
        if (stopAtEnd) record.playback.stop()
      }))
    )
    return Effect.forkDetach(fade)
  }

  private readonly fadeOutAndStop = (record: ActivePlayback, duration: number): Effect.Effect<void> =>
    Effect.gen({ self: this }, function* () {
      const self = this
      const fiber = yield* self.startFade(record, 1, 0, duration, true)
      yield* Fiber.join(fiber)
      self.removeRecord(record)
    })

  private readonly stopVoiceImmediately = (): Effect.Effect<void> =>
    Effect.gen({ self: this }, function* () {
      const self = this
      const records = self.active.voice.slice()
      for (const record of records) {
        yield* self.interruptFade(record)
        record.playback.stop()
        self.removeRecord(record)
      }
    })

  private readonly replaceMusic = (fade: number): Effect.Effect<void> =>
    Effect.gen({ self: this }, function* () {
      const self = this
      const old = self.active.music.slice()
      for (const record of old) {
        yield* self.interruptFade(record)
        if (positive(fade) === 0) {
          record.playback.stop()
          self.removeRecord(record)
        } else {
          const fiber = yield* self.startFade(record, 1, 0, fade, true)
          record.fade = fiber
          yield* Effect.forkDetach(
            Fiber.join(fiber).pipe(
              Effect.flatMap(() => Effect.sync(() => self.removeRecord(record)))
            )
          )
        }
      }
    })
}

/** Construct a PixiAudio with the real @pixi/sound backend. */
export const makePixiAudio = (options: Omit<PixiAudioOptions, "backend"> = {}): PixiAudio => new PixiAudio(options)