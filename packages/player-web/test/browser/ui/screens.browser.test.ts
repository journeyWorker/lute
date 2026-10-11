import { describe, expect, it } from "vitest"
import { Effect, Stream } from "effect"
import { Container } from "pixi.js"
import type { PlayerAction, Settings } from "@lute-lang/player"
import { layer } from "../../../src/input/map.js"
import { BacklogScreen, HudScreen } from "../../../src/ui/screens/backlog.js"
import { ConfirmScreen, EndedScreen, HaltedScreen, IdleScreen, LoadingScreen, MenuScreen, TitleScreen } from "../../../src/ui/screens/common.js"
import { SaveLoadScreen } from "../../../src/ui/screens/save-load.js"
import { SettingsScreen } from "../../../src/ui/screens/settings.js"
import { Button, Slider } from "../../../src/ui/widgets/index.js"

const theme = { font: "sans-serif", text: 0xffffff, accent: 0xffcc66, panel: 0x111122, panelAlpha: 0.8, readText: 0xcccccc }
const settings: Settings = { textSpeed: 60, autoDelay: 2, skipMode: "read", volumes: { master: 1, music: 0.8, sfx: 0.8, voice: 0.8 }, locale: "en", windowOpacity: 0.9, readColour: true, rollbackDepth: 20 }
function actions(): { values: PlayerAction[]; emit: (action: PlayerAction) => void } { const values: PlayerAction[] = []; return { values, emit: (action) => values.push(action) } }
function clickFirstButton(screen: Container): void { const button = screen.children.find((child) => child instanceof Button); expect(button).toBeInstanceOf(Button); (button as Button).activate() }
describe("web screens", () => {
  it("renders every screen model and emits title/menu actions", () => {
    const title = actions(); const titleView = new TitleScreen({ _tag: "Title", canContinue: true, background: undefined }, theme, title.emit); expect(titleView.children.length).toBeGreaterThan(2); clickFirstButton(titleView); expect(title.values).toContainEqual({ type: "newGame" })
    const menu = actions(); const menuView = new MenuScreen(theme, menu.emit); clickFirstButton(menuView); expect(menu.values).toContainEqual({ type: "close" })
    expect(new BacklogScreen({ _tag: "Backlog", entries: [{ lineId: "l1", voiceKey: "v", speakerName: "A", text: "Hi", hasVoice: true }] }, theme, () => {}).children.length).toBeGreaterThan(2)
    expect(new SaveLoadScreen({ _tag: "SaveLoad", mode: "save", page: 0, slots: [] }, theme, () => {}).children.length).toBeGreaterThan(2)
    expect(new SettingsScreen({ _tag: "Settings", settings, locales: ["en", "ja"] }, theme, () => {}).children.length).toBeGreaterThan(2)
    expect(new ConfirmScreen({ _tag: "Confirm", message: "Overwrite?" }, theme, () => {}).children.length).toBeGreaterThan(2)
    expect(new LoadingScreen({ _tag: "Loading", progress: 0.5 }, theme, () => {}).children.length).toBeGreaterThan(2)
    expect(new HaltedScreen({ _tag: "Halted", message: "Error" }, theme, () => {}).children.length).toBeGreaterThan(2)
    expect(new IdleScreen({ _tag: "Idle", options: [{ label: "One" }] }, theme, () => {}).children.length).toBeGreaterThan(2)
    expect(new EndedScreen({ _tag: "Ended", reason: "Done" }, theme, () => {}).children.length).toBeGreaterThan(2)
    expect(new HudScreen(theme, () => {}, { auto: true, skip: false }).children.length).toBe(9)
  })
  it("emits setSettings from slider activation", () => { const state = actions(); const view = new SettingsScreen({ _tag: "Settings", settings, locales: ["en"] }, theme, state.emit); const slider = view.children.find((child) => child instanceof Slider); expect(slider).toBeInstanceOf(Slider); (slider as Slider).activate(); expect(state.values[0]?.type).toBe("setSettings") })
})
describe("browser input layer", () => {
  it("dispatches real DOM keyboard and pointer events", async () => {
    const stream = layer(document.body)
    const keyboard = Effect.runPromise(Stream.runCollect(Stream.take(stream, 1)))
    await Effect.runPromise(Effect.yieldNow)
    document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true }))
    const keyActions = await keyboard
    expect(Array.from(keyActions)).toEqual([{ type: "advance" }])
    const pointerStream = layer(document.body)
    const pointer = Effect.runPromise(Stream.runCollect(Stream.take(pointerStream, 1)))
    await Effect.runPromise(Effect.yieldNow)
    document.body.dispatchEvent(new PointerEvent("pointerdown", { button: 0, clientY: 20, bubbles: true }))
    document.body.dispatchEvent(new PointerEvent("pointerup", { button: 0, clientY: 20, bubbles: true }))
    const pointerActions = await pointer
    expect(Array.from(pointerActions)).toEqual([{ type: "advance" }])
  })
})
