import { Context, Effect, Exit, Layer, ManagedRuntime, Scope, Stream, SubscriptionRef } from "effect"
import * as KeyValueStore from "effect/persistence/KeyValueStore"
import * as BrowserKeyValueStore from "@effect/platform-browser/BrowserKeyValueStore"
import * as FetchHttpClient from "effect/http/FetchHttpClient"
import * as RuntimeBrowser from "@lute-lang/runtime/browser"
import * as Runtime from "@lute-lang/runtime"
import { LutePlayer, BindingsLoader, type Bindings, type LutePlayerHandle, type PlayerAction, type PlayerHost, type PlayerView } from "@lute-lang/player"
import { PixiApp } from "./engine/app.js"
import { World } from "./engine/world.js"
import { Ui } from "./ui/ui.js"
import { PixiAssets, type PixiAssetsOptions } from "./assets/pixiAssets.js"
import { pixiAssetsBackend } from "./assets/pixiAssetsBackend.js"
import { PixiAudio } from "./audio/pixiAudio.js"
import { pixiSoundBackend } from "./audio/pixiSoundBackend.js"
import { PixiPresenter } from "./presenter/pixiPresenter.js"
import { Input } from "./input/index.js"
import { themeFromBindings } from "./theme.js"

/** A bindings source accepted by the web facade. */
export type BindingsSource = Bindings | string | { readonly url: string }

/** Options for mounting a browser player. */
export interface MountOptions {
  readonly bindings: BindingsSource
  readonly bundleUrl: string
  readonly wasmUrl: string
  readonly assetsUrl?: string
  readonly storageKey?: string
  readonly host?: PlayerHost
}

/** A scoped web player handle. `view` is the live Effect subscription ref. */
export interface WebPlayerHandle extends LutePlayerHandle {}

const slash = (value: string): string => value.endsWith("/") ? value : `${value}/`
const isUrl = (value: string): boolean => /^[a-z][a-z\d+.-]*:/i.test(value)
const absoluteUrl = (value: string): string => {
  try {
    return new URL(value, typeof globalThis.location === "object" ? globalThis.location.href : "http://localhost/").href
  } catch {
    return value
  }
}

const resolveBase = (source: BindingsSource, bindings: Bindings, assetsUrl: string | undefined): string => {
  if (assetsUrl !== undefined) return slash(assetsUrl)
  const configured = bindings.assetsBase ?? "assets/"
  if (isUrl(configured)) return slash(configured)
  if (typeof source === "object" && source !== null && "url" in source) return new URL(configured, source.url).href
  const origin = typeof globalThis.location === "object" ? globalThis.location.href : "http://localhost/"
  return new URL(configured, origin).href
}

const resolveAsset = (bindings: Bindings, base: string, file: string): string => {
  const mapped = bindings.assets?.[file] ?? file
  if (isUrl(mapped)) return mapped
  try {
    return new URL(mapped.replace(/^\/+/, ""), slash(base)).href
  } catch {
    const origin = typeof globalThis.location === "object" ? globalThis.location.href : "http://localhost/"
    return new URL(mapped.replace(/^\/+/, ""), new URL(slash(base), origin)).href
  }
}

const loadBindings = (source: BindingsSource): Effect.Effect<Bindings, unknown> => {
  if (typeof source === "string") return BindingsLoader.decode(source)
  if (typeof source === "object" && source !== null && "url" in source) {
    return BindingsLoader.loadUrl(source.url).pipe(Effect.provide(FetchHttpClient.layer))
  }
  return Effect.succeed(source)
}

const namespacedStorage = (key: string): Effect.Effect<Layer.Layer<KeyValueStore.KeyValueStore>, never, Scope.Scope> =>
  Effect.map(Layer.build(BrowserKeyValueStore.layerLocalStorage), (context) =>
    Layer.succeed(KeyValueStore.KeyValueStore, KeyValueStore.prefix(Context.get(context, KeyValueStore.KeyValueStore), `${key}:`))
  )

const runMount = (element: HTMLElement, options: MountOptions, bindings: Bindings, assetsBase: string) => {
  const theme = themeFromBindings(bindings)
  const pixiLayer = PixiApp.layer(element, bindings.canvas)
  const worldLayer = World.layer.pipe(Layer.provideMerge(pixiLayer))
  return Effect.gen(function* () {
    const pixi = yield* PixiApp
    const world = yield* World
    const ui = new Ui(pixi.layers.overlay, theme, world)
    ui.setDesign(bindings.canvas)
    const assetsOptions: PixiAssetsOptions = { assetsBase, backend: pixiAssetsBackend }
    const assets = new PixiAssets(assetsOptions)
    const audio = new PixiAudio({ backend: pixiSoundBackend })
    const title = bindings.ui?.title
    const titleBackground = title === undefined || title === null
      ? undefined
      : resolveAsset(bindings, assetsBase, title.background)
    const presenterOptions = titleBackground === undefined
      ? { pixi, world, ui, assets, bindings, theme }
      : { pixi, world, ui, assets, bindings, theme, titleBackground }
    const presenter = new PixiPresenter(presenterOptions)
    const storage = yield* namespacedStorage(options.storageKey ?? "lute")
    const requirements = Layer.mergeAll(
      RuntimeBrowser.runtime(absoluteUrl(options.wasmUrl)),
      RuntimeBrowser.bundle(absoluteUrl(slash(options.bundleUrl))),
      Runtime.InputLog.memory,
      storage
    )
    const playerOptions = options.host === undefined
      ? { bindings, presenter, audio, assets }
      : { bindings, presenter, audio, assets, host: options.host }
    const player = yield* LutePlayer.make(playerOptions).pipe(Effect.provide(requirements))
    let unlocked = false
    const dispatch = (action: PlayerAction): Effect.Effect<void> => Effect.gen(function* () {
      if (!unlocked) {
        unlocked = true
        yield* audio.unlock.pipe(Effect.catch(() => Effect.void))
      }
      yield* player.actions(action)
    })
    yield* Effect.forkScoped(Stream.runForEach(Input.layer(element), dispatch))
    if (title !== undefined && title !== null) {
      const file = resolveAsset(bindings, assetsBase, title.music)
      yield* Effect.forkScoped(audio.play("music", { file, loop: true, volume: 1, fade: 0 }).pipe(Effect.catch(() => Effect.void)))
    }
    return player
  }).pipe(Effect.provide(worldLayer))
}

/** Mount a web player in the current Effect scope. */
export const mount = (element: HTMLElement, options: MountOptions): Effect.Effect<WebPlayerHandle, unknown, Scope.Scope> =>
  Effect.flatMap(loadBindings(options.bindings), (bindings) => runMount(element, options, bindings, resolveBase(options.bindings, bindings, options.assetsUrl)))

/** Promise facade for non-Effect hosts; this is the only Promise API. */
export const mountPlayer = async (element: HTMLElement, options: MountOptions): Promise<{
  readonly unmount: () => Promise<void>
  readonly view: () => Promise<PlayerView>
  readonly act: (action: PlayerAction) => Promise<void>
}> => {
  const runtime = ManagedRuntime.make(Layer.empty)
  const scope = Scope.makeUnsafe("parallel")
  try {
    const handle = await runtime.runPromise(Effect.provideService(mount(element, options), Scope.Scope, scope))
    let closed = false
    return {
      unmount: async () => {
        if (closed) return
        closed = true
        await runtime.runPromise(handle.shutdown)
        await runtime.runPromise(Scope.close(scope, Exit.void))
        await runtime.dispose()
      },
      view: () => runtime.runPromise(SubscriptionRef.get(handle.view)),
      act: (action) => runtime.runPromise(handle.actions(action))
    }
  } catch (error) {
    await runtime.dispose()
    await Scope.close(scope, Exit.fail(error))
    throw error
  }
}

/** Namespace-style web facade. */
export const LuteWeb = { mount, mountPlayer }
