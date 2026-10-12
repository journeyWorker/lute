import { describe, expect, it } from "vitest"
import { Effect, Stream } from "effect"
import { Container, Text } from "pixi.js"
import type { PlayerAction, Settings } from "@lute-lang/player"
import { layer } from "../../../src/input/map.js"
import { BacklogScreen, HudScreen } from "../../../src/ui/screens/backlog.js"
import { ConfirmScreen, EndedScreen, HaltedScreen, IdleScreen, LoadingScreen, MenuScreen, TitleScreen } from "../../../src/ui/screens/common.js"
import { SaveLoadScreen } from "../../../src/ui/screens/save-load.js"
import { SettingsScreen } from "../../../src/ui/screens/settings.js"
import { Button, Segmented, Slider, Toggle } from "../../../src/ui/widgets/index.js"
const theme = { font: "sans-serif", text: 0xffffff, accent: 0xffcc66, panel: 0x111122, panelAlpha: 0.8, readText: 0xcccccc }
const settings: Settings = { textSpeed: 60, autoDelay: 2, skipMode: "read", volumes: { master: 1, music: 0.8, sfx: 0.8, voice: 0.8 }, locale: "en", windowOpacity: 0.9, readColour: true, rollbackDepth: 20 }
function actions(): { values: PlayerAction[]; emit: (action: PlayerAction) => void } { const values: PlayerAction[] = []; return { values, emit: (action) => values.push(action) } }
function buttons(screen: Container): Button[] { const result: Button[] = []; for (const child of screen.children) { if (child instanceof Button) result.push(child); if (child instanceof Container) result.push(...buttons(child)) }; return result }
function clickAll(screen: Container, emit: (action: PlayerAction) => void): ReadonlyArray<PlayerAction> { const found = buttons(screen); found.forEach((button) => button.activate()); return found.length === 0 ? [] : [] }
function keyStream(): { stream: ReturnType<typeof layer>; collect: Promise<ReadonlyArray<PlayerAction>> } { const stream = layer(document.body); return { stream, collect: Effect.runPromise(Stream.runCollect(Stream.take(stream, 1))) } }

it("Title buttons emit New, Continue, Load and Settings", () => { const value = actions(); buttons(new TitleScreen({ _tag: "Title", canContinue: true, background: undefined }, theme, value.emit)).forEach((button) => button.activate()); expect(value.values.map((action) => action.type)).toEqual(["newGame", "continue", "openLoad", "openSettings"]) })
it("Hud has all nine quick actions", () => { expect(buttons(new HudScreen(theme, () => {}, { auto: true, skip: true })).length).toBe(9) })
it("Backlog voice button emits replayVoice", () => { const value = actions(); const view = new BacklogScreen({ _tag: "Backlog", entries: [{ lineId: "l1", voiceKey: "v", speakerName: "A", text: "Hi", hasVoice: true }] }, theme, value.emit); const voice = buttons(view).find((button) => button.children.some((child) => child instanceof Text && child.text === "Voice")); voice?.activate(); expect(value.values).toContainEqual({ type: "replayVoice", lineId: "l1" }) })
it("Backlog rows show the cast display name", () => {
  const view = new BacklogScreen({ _tag: "Backlog", entries: [{ lineId: "l1", voiceKey: "", speakerName: "에다", text: "Hi", hasVoice: false }] }, theme, () => {})
  const texts = (node: Container): string[] => node.children.flatMap((child) => child instanceof Text ? [child.text] : child instanceof Container ? texts(child) : [])
  expect(texts(view)).toContain("에다")
})
it("SaveLoad empty slot emits saveSlot and close", () => { const value = actions(); const view = new SaveLoadScreen({ _tag: "SaveLoad", mode: "save", page: 0, slots: [] }, theme, value.emit); buttons(view).forEach((button) => button.activate()); expect(value.values).toContainEqual({ type: "close" }); expect(value.values).toContainEqual({ type: "saveSlot", slot: 0 }) })
it("Settings renders all controls", () => { const view = new SettingsScreen({ _tag: "Settings", settings, locales: ["en", "ja"] }, theme, () => {}); expect(view.children.filter((child) => child instanceof Slider || child instanceof Toggle || child instanceof Segmented)).toHaveLength(10) })
it("Confirm yes emits confirm true", () => { const value = actions(); const view = new ConfirmScreen({ _tag: "Confirm", message: "Overwrite?" }, theme, value.emit); buttons(view)[0]?.activate(); expect(value.values).toEqual([{ type: "confirm", yes: true }]) })
it("Confirm no emits confirm false", () => { const value = actions(); const view = new ConfirmScreen({ _tag: "Confirm", message: "Overwrite?" }, theme, value.emit); buttons(view)[1]?.activate(); expect(value.values).toEqual([{ type: "confirm", yes: false }]) })
it("Loading renders progress bar", () => { expect(new LoadingScreen({ _tag: "Loading", progress: 0.5 }, theme, () => {}).children.length).toBeGreaterThan(2) })
it("Halted back button emits toTitle", () => { const value = actions(); buttons(new HaltedScreen({ _tag: "Halted", message: "Error" }, theme, value.emit)).forEach((button) => button.activate()); expect(value.values).toEqual([{ type: "toTitle" }]) })
it("Idle option emits pickIdle", () => { const value = actions(); buttons(new IdleScreen({ _tag: "Idle", options: [{ label: "One" }] }, theme, value.emit)).forEach((button) => button.activate()); expect(value.values).toEqual([{ type: "pickIdle", index: 0 }]) })
it("Ended back button emits toTitle", () => { const value = actions(); buttons(new EndedScreen({ _tag: "Ended", reason: "Done" }, theme, value.emit)).forEach((button) => button.activate()); expect(value.values).toEqual([{ type: "toTitle" }]) })
it("Menu buttons emit resume save load settings title", () => { const value = actions(); buttons(new MenuScreen(theme, value.emit)).forEach((button) => button.activate()); expect(value.values.map((action) => action.type)).toEqual(["close", "openSave", "openLoad", "openSettings", "toTitle"]) })
function settingControl(index: number): PlayerAction { const value = actions(); const view = new SettingsScreen({ _tag: "Settings", settings, locales: ["en", "ja"] }, theme, value.emit); const controls = view.children.filter((child) => child instanceof Slider || child instanceof Toggle || child instanceof Segmented); const control = controls[index]; if (control instanceof Slider || control instanceof Toggle || control instanceof Segmented) control.activate(); return value.values[0]! }
it("Settings text speed emits setSettings", () => expect(settingControl(0)).toEqual({ type: "setSettings", settings: { textSpeed: 60 } }))
it("Settings auto delay emits setSettings", () => expect(settingControl(1)).toEqual({ type: "setSettings", settings: { autoDelay: 2 } }))
it("Settings skip mode emits setSettings", () => expect(settingControl(2)).toEqual({ type: "setSettings", settings: { skipMode: "all" } }))
it("Settings master volume emits setSettings", () => expect(settingControl(3).type).toBe("setSettings"))
it("Settings music volume emits setSettings", () => expect(settingControl(4).type).toBe("setSettings"))
it("Settings sfx volume emits setSettings", () => expect(settingControl(5).type).toBe("setSettings"))
it("Settings voice volume emits setSettings", () => expect(settingControl(6).type).toBe("setSettings"))
it("Settings opacity emits setSettings", () => expect(settingControl(7).type).toBe("setSettings"))
it("Settings read colour emits setSettings", () => expect(settingControl(8)).toEqual({ type: "setSettings", settings: { readColour: false } }))
it("Settings locale emits setSettings", () => expect(settingControl(9)).toEqual({ type: "setSettings", settings: { locale: "ja" } }))
it("Input layer keydown emits advance", async () => { const value = keyStream(); await Effect.runPromise(Effect.yieldNow); document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); expect(Array.from(await value.collect)).toEqual([{ type: "advance" }]) })
it("Input layer pointer tap emits advance", async () => { const value = keyStream(); await Effect.runPromise(Effect.yieldNow); document.body.dispatchEvent(new PointerEvent("pointerdown", { button: 0, clientY: 20, bubbles: true })); document.body.dispatchEvent(new PointerEvent("pointerup", { button: 0, clientY: 20, bubbles: true })); expect(Array.from(await value.collect)).toEqual([{ type: "advance" }]) })
it("Input layer wheel up emits backlog", async () => { const value = keyStream(); await Effect.runPromise(Effect.yieldNow); document.body.dispatchEvent(new WheelEvent("wheel", { deltaY: -20, bubbles: true })); expect(Array.from(await value.collect)).toEqual([{ type: "openBacklog" }]) })
it("Input layer removes listeners on stream close", async () => { const value = keyStream(); await Effect.runPromise(Effect.yieldNow); document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); await value.collect; const event = new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }); document.body.dispatchEvent(event); expect(event.defaultPrevented).toBe(false) })
it("Input layer consumes game keys while UI is open", async () => { let open = true; const stream = layer(document.body, undefined, () => open); const collect = Effect.runPromise(Stream.runCollect(Stream.take(stream, 1))); await Effect.runPromise(Effect.yieldNow); const event = new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true }); document.body.dispatchEvent(event); expect(event.defaultPrevented).toBe(true); open = false; document.body.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); await collect })
