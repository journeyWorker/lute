import { Deferred, Effect, Fiber, Option, SubscriptionRef } from "effect"
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
  readonly file: string
  /** The requested level in [0, 1]; mid volume-change ramp it holds the ramp's current value. */
  inputVolume: number
  /** The live fade multiplier in [0, 1] (0 while silent before a fade-in, 1 at rest). */
  factor: number
  readonly playback: AudioPlayback
  fade: Fiber.Fiber<void, never> | undefined
  /** Set once the playback is fading out or stopped; it no longer counts as playing. */
  stopping: boolean
}

const clamp = (value: number): number => Math.max(0, Math.min(1, value))
const positive = (value: number): number => Number.isFinite(value) && value > 0 ? value : 0

/** Constructor options for {@link PixiAudio}. */
export interface PixiAudioOptions {
  readonly backend: AudioBackend
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
  /** The latest music `play`'s token, completed when a later music `play` or `stop` supersedes it; undefined after a `stop`. */
  private musicRequest: Deferred.Deferred<void> | undefined
  constructor(options: PixiAudioOptions) {
    this.backend = options.backend
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


  /**
   * Play a channel sound, waiting for finite sounds to end. Music without
   * `restart` whose file is the one already playing only changes its volume,
   * fading over `fade`, without restarting it (spec 0.41.0 §7.7). A music `play`
   * or `stop` supersedes every earlier music `play` still in progress (waiting for
   * the unlock, its file or the backend): that play completes at once without
   * starting, and a backend play resolving after that is stopped, not recorded.
   */
  readonly play = (channel: AudioChannel, soundToPlay: AudioPlay): Effect.Effect<void> =>
    Effect.gen({ self: this }, function* () {
      const self = this
      // A music play is superseded by any later music play or stop (spec 0.47.2 §3).
      const request = channel === "music" ? self.beginMusicRequest() : undefined
      const superseded = (): boolean => request !== undefined && Deferred.isDoneUnsafe(request)
      if (!SubscriptionRef.getUnsafe(self.unlocked)) {
        // The supersession must wake the wait: the gate may stay closed for good.
        yield* request === undefined
          ? Deferred.await(self.gate)
          : Effect.raceFirst(Deferred.await(self.gate), Deferred.await(request))
      }
      if (superseded()) return
      if (channel === "music" && !soundToPlay.restart) {
        const playing = self.active.music.filter((record) => !record.stopping).at(-1)
        if (playing !== undefined && playing.file === soundToPlay.file) {
          yield* self.changeVolume(playing, clamp(soundToPlay.volume), soundToPlay.fade)
          return
        }
      }
      const loaded = yield* self.backend.load(soundToPlay.file).pipe(
        Effect.catch((error) => {
          self.logger(`Audio file missing or undecodable: ${soundToPlay.file}`, error)
          return Effect.fail(error)
        }),
        Effect.option
      )
      if (Option.isNone(loaded) || superseded()) return
      const inputVolume = clamp(soundToPlay.volume)
      if (channel === "voice") yield* self.stopVoiceImmediately()
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
      if (superseded()) {
        // Superseded while the backend started it: stop it at once, never record it.
        playback.value.stop()
        return
      }
      if (channel === "music") yield* self.replaceMusic(soundToPlay.fade)
      const record: ActivePlayback = {
        channel,
        file: soundToPlay.file,
        inputVolume,
        factor: positive(soundToPlay.fade) > 0 ? 0 : 1,
        playback: playback.value,
        fade: undefined,
        stopping: false
      }
      self.active[channel].push(record)
      self.applyVolume(record)
      if (positive(soundToPlay.fade) > 0) {
        record.fade = yield* self.startFade(record, 1, soundToPlay.fade, false)
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
      self.supersedeMusic(channel)
      const records = self.active[channel].slice()
      if (records.length === 0) return
      for (const record of records) {
        record.stopping = true
        yield* self.interruptFade(record)
      }
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
        for (const record of this.active[channel]) this.applyVolume(record)
      }
    })

  /**
   * Register a music `play`: completes the previous request (superseding its
   * play still in progress, whatever it waits for) and returns this play's own.
   */
  private beginMusicRequest(): Deferred.Deferred<void> {
    this.supersedeMusic("music")
    const request = Deferred.makeUnsafe<void>()
    this.musicRequest = request
    return request
  }

  /** A music `stop` (or a later `play`) supersedes the latest music play still in progress. */
  private supersedeMusic(channel: AudioChannel): void {
    if (channel !== "music" || this.musicRequest === undefined) return
    Deferred.doneUnsafe(this.musicRequest, Effect.void)
    this.musicRequest = undefined
  }

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

  private applyVolume(record: ActivePlayback): void {
    record.playback.setVolume(this.effectiveVolume(record.channel, record.inputVolume) * record.factor)
  }

  /** Fade `record`'s live factor to `to` over `duration` seconds, starting from wherever it is now. */
  private readonly startFade = (
    record: ActivePlayback,
    to: number,
    duration: number,
    stopAtEnd: boolean
  ): Effect.Effect<Fiber.Fiber<void, never>> => {
    const from = record.factor
    return this.ramp(record, duration, (progress) => {
      record.factor = from + (to - from) * progress
    }, stopAtEnd)
  }

  /**
   * Step `record` through `advance(progress)` over `duration` seconds on a
   * detached fiber, applying its volume after each step.
   */
  private readonly ramp = (
    record: ActivePlayback,
    duration: number,
    advance: (progress: number) => void,
    stopAtEnd: boolean
  ): Effect.Effect<Fiber.Fiber<void, never>> => {
    const steps = Math.max(1, Math.ceil(duration * 20))
    const interval = duration / steps
    const fade = Effect.forEach(
      Array.from({ length: steps }, (_, index) => index + 1),
      (step) => Effect.sleep(interval).pipe(
        Effect.flatMap(() => Effect.sync(() => {
          advance(step / steps)
          this.applyVolume(record)
        }))
      ),
      { discard: true }
    ).pipe(
      Effect.flatMap(() => Effect.sync(() => {
        advance(1)
        this.applyVolume(record)
        if (stopAtEnd) record.playback.stop()
      }))
    )
    return Effect.forkDetach(fade)
  }

  /**
   * Move a playing record to a new input volume, fading over `fade` seconds
   * from its current level (finishing an interrupted fade-in); it keeps playing.
   */
  private readonly changeVolume = (record: ActivePlayback, inputVolume: number, fade: number): Effect.Effect<void> =>
    Effect.gen({ self: this }, function* () {
      const self = this
      yield* self.interruptFade(record)
      const duration = positive(fade)
      if (duration === 0) {
        record.inputVolume = inputVolume
        record.factor = 1
        self.applyVolume(record)
        return
      }
      const fromVolume = record.inputVolume
      const fromFactor = record.factor
      record.fade = yield* self.ramp(record, duration, (progress) => {
        record.inputVolume = fromVolume + (inputVolume - fromVolume) * progress
        record.factor = fromFactor + (1 - fromFactor) * progress
      }, false)
    })

  private readonly fadeOutAndStop = (record: ActivePlayback, duration: number): Effect.Effect<void> =>
    Effect.gen({ self: this }, function* () {
      const self = this
      const fiber = yield* self.startFade(record, 0, duration, true)
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
        record.stopping = true
        yield* self.interruptFade(record)
        if (positive(fade) === 0) {
          record.playback.stop()
          self.removeRecord(record)
        } else {
          const fiber = yield* self.startFade(record, 0, fade, true)
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

/** Construct a PixiAudio from an injected backend. */
export const makePixiAudio = (options: PixiAudioOptions): PixiAudio => new PixiAudio(options)