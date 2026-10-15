import { Context, Effect, Exit, Layer, ManagedRuntime, Scope, Stream, SubscriptionRef } from "effect"
import * as KeyValueStore from "effect/persistence/KeyValueStore"
import * as BrowserKeyValueStore from "@effect/platform-browser/BrowserKeyValueStore"
import * as FetchHttpClient from "effect/http/FetchHttpClient"
import * as RuntimeBrowser from "@lute-lang/runtime/browser"
import * as Runtime from "@lute-lang/runtime"
import { resolveEndedScreen } from "./ui/ended.js"
import { PixiApp } from "./engine/app.js"
import { World } from "./engine/world.js"
import { Ui } from "./ui/ui.js"
import { PixiAssets, type PixiAssetsOptions } from "./assets/pixiAssets.js"
import { createPixiAssetsBackend } from "./assets/pixiAssetsBackend.js"
import { PixiAudio } from "./audio/pixiAudio.js"
import { pixiSoundBackend } from "./audio/pixiSoundBackend.js"
import { PixiPresenter } from "./presenter/pixiPresenter.js"
import { Input } from "./input/index.js"
import { themeFromBindings } from "./theme.js"
import { LutePlayer, BindingsLoader, type Bindings, type LutePlayerHandle, type LutePlayerOptions, type LutePlayerStart, type PlayerAction, type PlayerHost, type PlayerView, type ScreenModel, type Input as PlayerInput, type SaveRuntime } from "@lute-lang/player"

/** A bindings source accepted by the web facade. */
export type BindingsSource = Bindings | string | { readonly url: string }

/** Options for mounting a browser player. */
export type MountOptions = LutePlayerStart & {
  readonly bindings: BindingsSource
  readonly bundleUrl: string
  readonly wasmUrl: string
  readonly assetsUrl?: string
  readonly storageKey?: string
  readonly host?: PlayerHost
  readonly cueFactory?: LutePlayerOptions["cueFactory"]
  readonly linePacing?: LutePlayerOptions["linePacing"]
  readonly autosave?: LutePlayerOptions["autosave"]
}

/** A scoped web player handle. `view` is the live Effect subscription ref. */
export interface WebPlayerHandle extends LutePlayerHandle {
  readonly bundle: Runtime.Bundle.Bundle
}

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
    // Built in the caller's scope: the app, its canvas and the ticker live
    // until the player is unmounted (`Effect.provide(layer)` would close them
    // as soon as this effect returned).
    const services = yield* Layer.build(worldLayer)
    const pixi = Context.get(services, PixiApp)
    const world = Context.get(services, World)
    const assetsOptions: PixiAssetsOptions = { assetsBase, backend: createPixiAssetsBackend() }
    const assets = new PixiAssets(assetsOptions)
    const ui = new Ui(pixi.layers.overlay, theme, world, (file) => assets.texture(file))
    ui.setDesign(bindings.canvas, "Lute")
    // Cues name bound files; the backend loads them below the assets base.
    const resolveSound = (file: string): string => resolveAsset(bindings, assetsBase, file)
    const audio = new PixiAudio({
      backend: {
        load: (file) => pixiSoundBackend.load(resolveSound(file)),
        play: (file, playOptions) => pixiSoundBackend.play(resolveSound(file), playOptions),
        unlock: pixiSoundBackend.unlock
      }
    })
    const title = bindings.ui?.title
    const titleBackground = title === undefined || title === null
      ? undefined
      : resolveAsset(bindings, assetsBase, title.background)
    if (titleBackground !== undefined) yield* assets.ensure([titleBackground])
    let player: LutePlayerHandle | undefined
    const resolveEnded = (screen: Extract<ScreenModel, { _tag: "Ended" }>): Effect.Effect<ScreenModel> =>
      Effect.gen(function* () {
        const ended = (bindings.ui as (typeof bindings.ui & { readonly ended?: { readonly by: string; readonly screens: Record<string, { readonly title: string; readonly text: string; readonly background?: string | null }> } | null }) | undefined)?.ended
        if (player === undefined) return screen
        const current = yield* SubscriptionRef.get(player.session.current)
        const selected = yield* resolveEndedScreen(player.program, current.state, ended, screen)
        if (selected === undefined || selected.background === undefined || selected.background === null) return selected ?? screen
        const background = resolveAsset(bindings, assetsBase, selected.background)
        yield* assets.ensure([background])
        return { ...selected, background }
      }).pipe(Effect.scoped, Effect.catch(() => Effect.succeed(screen)))
    const presenterOptions = titleBackground === undefined
      ? { pixi, world, ui, assets, bindings, theme, resolveEnded }
      : { pixi, world, ui, assets, bindings, theme, titleBackground, resolveEnded }
    const presenter = new PixiPresenter(presenterOptions)
    const storage = yield* namespacedStorage(options.storageKey ?? "lute")
    const requirements = Layer.mergeAll(
      RuntimeBrowser.runtime(absoluteUrl(options.wasmUrl)),
      RuntimeBrowser.bundle(absoluteUrl(slash(options.bundleUrl))),
      Runtime.InputLog.memory,
      storage
    )
    const requirementsContext = yield* Layer.build(requirements)
    const bundle = yield* Context.get(requirementsContext, Runtime.BundleSource.BundleSource).load
    const cachedRequirements = Context.add(requirementsContext, Runtime.BundleSource.BundleSource, {
      load: Effect.succeed(bundle)
    })
    const start: LutePlayerStart = "snapshot" in options
      ? { snapshot: options.snapshot }
      : "session" in options
        ? { session: options.session }
        : options.seed === undefined ? {} : { seed: options.seed }
    const playerOptions: LutePlayerOptions = {
      ...start,
      bindings,
      presenter,
      audio,
      assets,
      ...(options.host === undefined ? {} : { host: options.host }),
      ...(options.cueFactory === undefined ? {} : { cueFactory: options.cueFactory }),
      ...(options.linePacing === undefined ? {} : { linePacing: options.linePacing }),
      ...(options.autosave === undefined ? {} : { autosave: options.autosave })
    }
    player = yield* LutePlayer.make(playerOptions).pipe(Effect.provide(Layer.succeedContext(cachedRequirements)))
    let unlocked = false
    const dispatch = (action: PlayerAction): Effect.Effect<void> => Effect.gen(function* () {
      if (!unlocked) {
        unlocked = true
        yield* audio.unlock.pipe(Effect.catch(() => Effect.void))
      }
      yield* player.actions(action)
    })
    const keys = (event: KeyboardEvent): void => {
      if (ui.handleKey(event.key)) event.preventDefault()
    }
    element.addEventListener("keydown", keys, true)
    yield* Effect.addFinalizer(() => Effect.sync(() => element.removeEventListener("keydown", keys, true)))
    yield* Effect.forkScoped(Stream.runForEach(Input.layer(element, undefined, () => ui.hasScreen, (event) => ui.pointerAction(event.clientX, event.clientY)), dispatch))
    yield* Effect.forkScoped(Stream.runForEach(SubscriptionRef.changes(player.view), (view) => Effect.sync(() => {
      ui.setPlaying(view.flow === "playing" && view.story !== "idle", { auto: view.mode === "auto", skip: view.mode === "skip", hidden: view.uiHidden })
    })))
    if (title !== undefined && title !== null) {
      const file = resolveAsset(bindings, assetsBase, title.music)
      yield* Effect.forkScoped(audio.play("music", { file, loop: true, volume: 1, fade: 0, restart: true }).pipe(Effect.catch(() => Effect.void)))
    }
    return { ...player, bundle }
  })
}

/** Mount a web player in the current Effect scope. */
export const mount = (element: HTMLElement, options: MountOptions): Effect.Effect<WebPlayerHandle, unknown, Scope.Scope> =>
  Effect.flatMap(loadBindings(options.bindings), (bindings) => runMount(element, options, bindings, resolveBase(options.bindings, bindings, options.assetsUrl)))

/** Promise facade for non-Effect hosts; this is the only Promise API. */
export const mountPlayer = async (element: HTMLElement, options: MountOptions): Promise<{
  readonly unmount: () => Promise<void>
  readonly view: () => Promise<PlayerView>
  readonly act: (action: PlayerAction) => Promise<void>
  readonly send: (input: PlayerInput) => Promise<void>
  readonly snapshot: () => Promise<SaveRuntime>
  readonly restore: (save: SaveRuntime) => Promise<void>
  /** The handle's runtime session: read `session.current` for the state the `program` queries take. */
  readonly session: WebPlayerHandle["session"]
  readonly program: WebPlayerHandle["program"]
  readonly bundle: Runtime.Bundle.Bundle
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
      act: (action) => runtime.runPromise(handle.actions(action)),
      send: (input) => runtime.runPromise(handle.send(input)),
      snapshot: () => runtime.runPromise(handle.snapshot),
      restore: (save) => runtime.runPromise(handle.restore(save)),
      session: handle.session,
      program: handle.program,
      bundle: handle.bundle
    }
  } catch (error) {
    await runtime.dispose()
    await Scope.close(scope, Exit.fail(error))
    throw error
  }
}

/** Namespace-style web facade. */
export const LuteWeb = { mount, mountPlayer }
