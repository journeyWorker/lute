/**
 * Real-bundle browser harness (spec 0.47.0 §6): mounts player-web with
 * `mount` over a project compiled by the lute binary in `globalSetup.ts`
 * and the packaged wasm runtime, drives it with real player actions, and
 * reads what player-web drew from its Pixi stage (display tree and pixels).
 */
import { Effect, Exit, Scope, SubscriptionRef } from "effect"
import { BitmapText, Container, HTMLText, Rectangle, Text } from "pixi.js"
import { Button } from "../../../src/ui/widgets/index.js"
import { inject, type ProvidedContext } from "vitest"
import type { LutePlayerStart, PlayerAction, PlayerView } from "@lute-lang/player"
import { pixiAppOf } from "../../../src/engine/app.js"
import type { PixiAppShape } from "../../../src/engine/types.js"
import { mount, type MountOptions, type WebPlayerHandle } from "../../../src/mount.js"

export interface RealPlayer {
  readonly element: HTMLElement
  readonly handle: WebPlayerHandle
  /** The mounted Pixi application: `layers` are the stage player-web draws. */
  readonly pixi: PixiAppShape
  readonly view: () => Promise<PlayerView>
  readonly act: (action: PlayerAction) => Promise<void>
  /** Shut the player down and remove its element. */
  readonly close: () => Promise<void>
}

/** What a test may vary: the start (`seed`/`snapshot`/`session`) and the host-side options. */
export type RealMountOptions = LutePlayerStart & Partial<Pick<MountOptions, "storageKey" | "host" | "cueFactory" | "linePacing" | "autosave">>

/**
 * Mount a project compiled in `globalSetup.ts` (`demo` is
 * `examples/player-demo`; others are `test/fixtures/<name>`) in a 1280×720
 * element (the design size, so stage and canvas coordinates agree). Every
 * mount gets a fresh `storageKey` unless `options.storageKey` names one;
 * mounting again with the same key after `close` models a page reload over
 * the same browser storage.
 */
export const mountProject = async (name: keyof ProvidedContext["luteProjects"], options: RealMountOptions = {}): Promise<RealPlayer> => {
  const urls = inject("luteProjects")[name]
  const absolute = (path: string): string => new URL(path, location.origin).href
  const element = document.createElement("div")
  element.style.width = "1280px"
  element.style.height = "720px"
  element.tabIndex = 0
  document.body.appendChild(element)
  const scope = Scope.makeUnsafe()
  const handle = await Effect.runPromise(Effect.provideService(mount(element, {
    bindings: { url: absolute(urls.bindingsUrl) },
    bundleUrl: absolute(urls.bundleUrl),
    wasmUrl: absolute(urls.wasmUrl),
    assetsUrl: absolute(urls.assetsUrl),
    ...options,
    storageKey: options.storageKey ?? `lute-test-${crypto.randomUUID()}`
  }), Scope.Scope, scope))
  const pixi = pixiAppOf(element)
  if (pixi === undefined) throw new Error("player-web mounted no Pixi application")
  return {
    element,
    handle,
    pixi,
    view: () => Effect.runPromise(SubscriptionRef.get(handle.view)),
    act: (action) => Effect.runPromise(handle.actions(action)),
    close: async () => {
      await Effect.runPromise(handle.shutdown)
      await Effect.runPromise(Scope.close(scope, Exit.void))
      element.remove()
    }
  }
}

/** Poll `probe` every frame-ish until it returns a value other than `undefined`/`false`. */
export const waitFor = async <A>(probe: () => A | undefined | false | Promise<A | undefined | false>, what: string, timeoutMs = 10_000): Promise<A> => {
  const deadline = performance.now() + timeoutMs
  for (;;) {
    const value = await probe()
    if (value !== undefined && value !== false) return value
    if (performance.now() > deadline) throw new Error(`timed out waiting for ${what}`)
    await Effect.runPromise(Effect.sleep("20 millis"))
  }
}

/** The alpha `node` is drawn with: its own times every ancestor's. */
export const alphaOf = (node: Container): number => {
  let alpha = 1
  for (let current: Container | null = node; current !== null; current = current.parent) alpha *= current.alpha
  return alpha
}

/** Whether `node` and every ancestor is visible and opaque enough to be seen. */
export const isDrawn = (node: Container): boolean => {
  for (let current: Container | null = node; current !== null; current = current.parent) {
    if (!current.visible || !current.renderable || current.alpha <= 0) return false
  }
  return true
}

/** The text of every drawn text object below `root`, in display order. */
export const drawnTexts = (root: Container): ReadonlyArray<string> => {
  const texts: string[] = []
  const walk = (node: Container): void => {
    if ((node instanceof Text || node instanceof BitmapText || node instanceof HTMLText) && isDrawn(node)) texts.push(node.text)
    for (const child of node.children) walk(child)
  }
  walk(root)
  return texts
}

/**
 * Press `key` on the player element: a keydown/keyup pair through
 * player-web's own input path (its device input unlocks audio, as a
 * reader's first key does). Dispatched on the element, so suites running
 * in parallel never share keyboard focus.
 */
export const press = (player: RealPlayer, key: string): void => {
  player.element.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true }))
  player.element.dispatchEvent(new KeyboardEvent("keyup", { key, bubbles: true, cancelable: true }))
}

/**
 * Play until `stop` holds: New game at the title (text speed instant), the
 * first idle menu entry at idle, and at a choice the first open option whose
 * id is in `prefer`, else the first open option. Every other state is
 * advanced with Enter ({@link press}): sounds, and the batches that wait for
 * them, wait for the device input that unlocks player-web's audio.
 */
export const playUntil = async (
  player: RealPlayer,
  stop: (view: PlayerView) => boolean,
  prefer: ReadonlyArray<string> = [],
  timeoutMs = 120_000
): Promise<PlayerView> => {
  const deadline = performance.now() + timeoutMs
  const trail: string[] = []
  for (;;) {
    const view = await player.view()
    if (stop(view)) return view
    const state = `${view.flow}/${view.story}/${view.overlay}`
    if (trail.at(-1) !== state) trail.push(state)
    if (performance.now() > deadline) throw new Error(`stop condition not reached: ${trail.slice(-20).join(" ")}`)
    if (view.flow === "title") {
      await player.act({ type: "setSettings", settings: { textSpeed: 0 } })
      await player.act({ type: "newGame" })
    } else if (view.flow === "playing" && view.story === "idle") {
      await player.act({ type: "pickIdle", index: 0 })
    } else if (view.flow === "playing" && view.story === "choice") {
      const current = await Effect.runPromise(SubscriptionRef.get(player.handle.session.current))
      if (current.await.type === "awaitChoice") {
        const open = current.await.menu.options.filter((option) => option.verdict === "open")
        const option = open.find((candidate) => prefer.includes(candidate.id)) ?? open[0]
        if (option !== undefined) await player.act({ type: "choose", option: option.id })
      }
    } else if (view.flow === "playing") {
      press(player, "Enter")
    }
    await Effect.runPromise(Effect.sleep("15 millis"))
  }
}

/** The dialogue window's drawn texts: the speaker plate (when shown), then the line. */
const drawnLine = (player: RealPlayer): ReadonlyArray<string> =>
  drawnTexts(player.pixi.layers.dialogue).filter((piece) => piece !== "" && piece !== "▼")

/** What {@link playReading} saw player-web draw, in order. */
export interface Reading {
  /** Every dialogue line drawn, as its drawn texts (speaker plate first when shown). */
  readonly lines: Array<ReadonlyArray<string>>
  /** Every choice menu drawn, as its drawn texts (prompt first). */
  readonly choices: Array<ReadonlyArray<string>>
}

/** A drawn line or menu contains `text` (the speaker plate, the line, a prompt or a label). */
export const shows = (drawn: ReadonlyArray<string>, text: string): boolean => drawn.some((piece) => piece.includes(text))

/**
 * Play like {@link playUntil} (text speed instant), recording each dialogue
 * line and choice menu as drawn, and never advancing a line before it has
 * been read: Enter is pressed only on a `shown` line, after recording it,
 * and audio is unlocked first with Escape (the menu, closed again). `stop`
 * sees the reading so far; pass `reading` to continue one.
 */
export const playReading = async (
  player: RealPlayer,
  stop: (reading: Reading, view: PlayerView) => boolean,
  prefer: ReadonlyArray<string> = [],
  reading: Reading = { lines: [], choices: [] },
  timeoutMs = 120_000
): Promise<Reading> => {
  const deadline = performance.now() + timeoutMs
  const trail: string[] = []
  let unlocked = false
  for (;;) {
    const view = await player.view()
    if (stop(reading, view)) return reading
    const state = `${view.flow}/${view.story}/${view.overlay}`
    if (trail.at(-1) !== state) trail.push(state)
    if (performance.now() > deadline) throw new Error(`stop condition not reached: ${trail.slice(-20).join(" ")}; first lines read: ${JSON.stringify(reading.lines.slice(0, 5))}`)
    if (view.flow === "title" && view.overlay === "none") {
      await player.act({ type: "setSettings", settings: { textSpeed: 0 } })
      await player.act({ type: "newGame" })
    } else if (view.flow === "playing" && view.overlay === "none") {
      if (view.story === "idle") {
        await player.act({ type: "pickIdle", index: 0 })
      } else if (!unlocked && view.story === "running") {
        // Unlock audio with a device input that cannot advance a line: Escape
        // opens the menu (retried while a full screen, e.g. loading, takes keys).
        press(player, "Escape")
        unlocked = await waitFor(async () => (await player.view()).overlay === "menu", "the menu Escape opens", 500).then(() => true, () => false)
        if (unlocked) await player.act({ type: "close" })
      } else if (view.story === "choice") {
        const current = await Effect.runPromise(SubscriptionRef.get(player.handle.session.current))
        const drawn = drawnTexts(player.pixi.layers.choice)
        if (current.await.type === "awaitChoice" && drawn.length > 0) {
          reading.choices.push(drawn)
          if (stop(reading, view)) return reading
          const open = current.await.menu.options.filter((option) => option.verdict === "open")
          const option = open.find((candidate) => prefer.includes(candidate.id)) ?? open[0]
          if (option !== undefined) await player.act({ type: "choose", option: option.id })
        }
      } else if (view.story === "shown") {
        // The machine can report a line shown a few frames before its text is drawn.
        const drawn = await waitFor(() => {
          const line = drawnLine(player)
          return line.length > 0 ? line : undefined
        }, "the shown line to be drawn", 3_000).catch(() => drawnLine(player))
        reading.lines.push(drawn)
        if (stop(reading, view)) return reading
        press(player, "Enter")
        unlocked = true
        // One Enter per line: wait until this line is released before reading on.
        const key = drawn.join("\n")
        await waitFor(async () => (await player.view()).story !== "shown" || drawnLine(player).join("\n") !== key, "the line to be released")
      }
    } else if (view.flow === "playing" && view.overlay === "menu") {
      // A late answer to an unlock Escape.
      unlocked = true
      await player.act({ type: "close" })
    }
    await Effect.runPromise(Effect.sleep("15 millis"))
  }
}

/** The drawn button below `root` whose caption is `label`. */
export const buttonLabelled = (root: Container, label: string): Button | undefined => {
  const find = (node: Container): Button | undefined => {
    if (node instanceof Button && isDrawn(node) && node.children.some((child) => child instanceof Text && child.text === label)) return node
    for (const child of node.children) {
      const found = find(child)
      if (found !== undefined) return found
    }
    return undefined
  }
  return find(root)
}

/** RGBA bytes the renderer draws for `frame` (stage coordinates) of the current stage. */
export const stagePixels = (pixi: PixiAppShape, frame: Rectangle): Uint8ClampedArray => {
  pixi.app.render()
  return pixi.app.renderer.extract.pixels({ target: pixi.app.stage, frame }).pixels
}
