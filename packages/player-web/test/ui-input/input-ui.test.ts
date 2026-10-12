import { describe, expect, it } from "vitest"
import { Container } from "pixi.js"
import type { PlayerAction } from "@lute-lang/player"
import { FocusManager } from "../../src/ui/widgets/focus-manager.js"
import { pageSlots, SLOTS_PER_PAGE } from "../../src/ui/screens/save-load.js"
import { ScrollList, visibleRange } from "../../src/ui/widgets/scroll-list.js"
import { GamepadEdgeDetector } from "../../src/input/gamepad.js"
import { LongPressRecognizer, SwipeRecognizer, resolveKeyboardAction, defaultInputMap } from "../../src/input/map.js"
const theme = { font: "sans-serif", text: 0xffffff, accent: 0xffcc66, panel: 0x111122, panelAlpha: 0.8, readText: 0xcccccc }
function key(key: string, modifiers: Partial<KeyboardEvent> = {}): KeyboardEvent { return { key, ctrlKey: false, shiftKey: false, altKey: false, ...modifiers } as KeyboardEvent }
const bindings: ReadonlyArray<[string, PlayerAction]> = [["Enter", { type: "advance" }], [" ", { type: "advance" }], ["a", { type: "toggleAuto" }], ["s", { type: "toggleSkip" }], ["Escape", { type: "openMenu" }], ["PageUp", { type: "openBacklog" }], ["F5", { type: "quickSave" }], ["F9", { type: "quickLoad" }], ["h", { type: "hideUI" }], ["Backspace", { type: "rollback" }]]
describe("default input binding rows", () => { bindings.forEach(([input, action]) => it(`${input === " " ? "Space" : input} maps to ${action.type}`, () => expect(resolveKeyboardAction(key(input))).toEqual(action))) })
describe("keys the choice menu owns", () => { it("digits produce no player action (the menu maps them to its options)", () => { for (const digit of "123456789") expect(resolveKeyboardAction(key(digit))).toBeUndefined() }) })
it("honours ctrl/shift/alt modifiers", () => { const map = { ...defaultInputMap, keyboard: { X: { action: { type: "quickSave" } as PlayerAction, ctrl: true, shift: false, alt: false } } }; expect(resolveKeyboardAction(key("X"), map)).toBeUndefined(); expect(resolveKeyboardAction(key("X", { ctrlKey: true }), map)).toEqual({ type: "quickSave" }) })
it("recognizes a 500ms long press", () => { const recognizer = new LongPressRecognizer(); recognizer.start(100); expect(recognizer.check(599)).toBe(false); expect(recognizer.check(600)).toBe(true) })
it("recognizes a downward swipe", () => { const recognizer = new SwipeRecognizer(); recognizer.start(100); expect(recognizer.end(160)).toBe("down") })
it("recognizes an upward swipe", () => { const recognizer = new SwipeRecognizer(); recognizer.start(160); expect(recognizer.end(100)).toBe("up") })
const idle = { a: false, b: false, y: false, rb: false, lb: false, start: false }
it("gamepad A press emits advance", () => expect(new GamepadEdgeDetector().poll({ ...idle, a: true })).toEqual([{ type: "advance" }]))
it("gamepad RB press emits skip hold", () => expect(new GamepadEdgeDetector().poll({ ...idle, rb: true })).toEqual([{ type: "skipHold", held: true }]))
it("gamepad RB release emits skip release", () => { const detector = new GamepadEdgeDetector(); detector.poll({ ...idle, rb: true }); expect(detector.poll(idle)).toEqual([{ type: "skipHold", held: false }]) })
describe("FocusManager navigation", () => {
  function manager(): { focus: FocusManager; calls: string[] } {
    const focus = new FocusManager()
    const calls: string[] = []
    ;["one", "two", "three"].forEach((name) => focus.register({ focusable: true, focused: false, focusNode: new Container(), activate: () => calls.push(name) }))
    return { focus, calls }
  }
  it("activates with Enter", () => { const value = manager(); value.focus.focus(0); value.focus.handleKey("Enter"); expect(value.calls).toEqual(["one"]) })
  it("moves with ArrowRight", () => { const value = manager(); value.focus.focus(0); value.focus.handleKey("ArrowRight"); value.focus.handleKey("Enter"); expect(value.calls).toEqual(["two"]) })
  it("moves with ArrowLeft", () => { const value = manager(); value.focus.focus(1); value.focus.handleKey("ArrowLeft"); value.focus.handleKey("Enter"); expect(value.calls).toEqual(["one"]) })
  it("wraps tab focus", () => { const value = manager(); value.focus.focus(0); value.focus.handleKey("Tab"); value.focus.handleKey("Tab"); value.focus.handleKey("Tab"); value.focus.handleKey("Enter"); expect(value.calls).toEqual(["one"]) })
})
it("ScrollList visible range maths", () => expect(visibleRange(100, 20, 100, 200)).toEqual({ start: 10, end: 16 }))
it("ScrollList clamps above max", () => { const list = new ScrollList(Array.from({ length: 20 }, (_, index) => index), theme, () => new Container(), 20, 100, 200); list.setOffset(9999); expect(list.scrollOffset).toBe(list.maxOffset) })
it("ScrollList handles PageUp", () => { const list = new ScrollList(Array.from({ length: 20 }, (_, index) => index), theme, () => new Container(), 20, 100, 200); list.setOffset(list.maxOffset); list.handleKey("PageUp"); expect(list.scrollOffset).toBe(list.maxOffset - 100) })
it("ScrollList handles ArrowDown", () => { const list = new ScrollList(Array.from({ length: 20 }, (_, index) => index), theme, () => new Container(), 20, 100, 200); list.handleKey("ArrowDown"); expect(list.scrollOffset).toBe(20) })
it("slot paging has six cells", () => { const slots = Array.from({ length: 8 }, (_, slot) => ({ slot, label: String(slot), savedAt: "", thumbnail: undefined })); expect(SLOTS_PER_PAGE).toBe(6); expect(pageSlots(slots, 0)).toHaveLength(6) })
it("slot paging preserves empty cells", () => { const slots = Array.from({ length: 8 }, (_, slot) => ({ slot, label: String(slot), savedAt: "", thumbnail: undefined })); expect(pageSlots(slots, 1).map((slot) => slot?.slot)).toEqual([6, 7, undefined, undefined, undefined, undefined]) })
