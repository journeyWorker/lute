import { Effect } from "effect"
import { sound, webaudio, type IMediaInstance } from "@pixi/sound"
import { AudioDecodeError, type AudioBackend, type AudioPlayback } from "./pixiAudio.js"

const clamp = (value: number): number => Math.max(0, Math.min(1, value))

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

/** Real @pixi/sound backend used by the browser player. */
export const pixiSoundBackend: AudioBackend = {
  load: (file) => Effect.callback<void, AudioDecodeError>((resume) => {
    if (sound.exists(file)) {
      resume(Effect.succeed(undefined))
      return
    }
    try {
      sound.add(file, {
        url: file,
        preload: true,
        loaded: (error) => resume(error == null ? Effect.succeed(undefined) : Effect.fail(new AudioDecodeError(file, error)))
      })
    } catch (error) {
      resume(Effect.fail(new AudioDecodeError(file, error)))
    }
  }),
  play: (file, options) => Effect.tryPromise({
    try: () => Promise.resolve(sound.play(file, {
      loop: options.loop,
      volume: options.volume
    })).then((instance): AudioPlayback => ({
      setVolume: (volume) => { instance.volume = clamp(volume) },
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
