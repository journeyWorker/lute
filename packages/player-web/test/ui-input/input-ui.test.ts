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
describe("input map", () => {
  it("resolves defaults and modifier requirements", () => {
    expect(resolveKeyboardAction(key("Enter"))).toEqual({ type: "advance" })
    expect(resolveKeyboardAction(key("F5"))).toEqual({ type: "quickSave" })
    const map = { ...defaultInputMap, keyboard: { CtrlS: { action: { type: "quickSave" } as PlayerAction, ctrl: true } } }
    expect(resolveKeyboardAction(key("CtrlS"), map)).toEqual(undefined)
    expect(resolveKeyboardAction(key("CtrlS", { ctrlKey: true }), map)).toEqual({ type: "quickSave" })
  })
  it("recognizes long press and swipe gestures", () => {
    const long = new LongPressRecognizer(); long.start(100); expect(long.check(599)).toBe(false); expect(long.check(600)).toBe(true); expect(long.check(700)).toBe(false)
    const swipe = new SwipeRecognizer(); swipe.start(400); expect(swipe.end(470)).toBe("down"); swipe.start(400); expect(swipe.end(300)).toBe("up")
  })
})
describe("gamepad edges", () => {
  it("emits once per press and tracks RB hold", () => {
    const detector = new GamepadEdgeDetector(); const idle = { a: false, b: false, y: false, rb: false, lb: false, start: false }; const pressed = { ...idle, a: true, rb: true }
    expect(detector.poll(pressed)).toEqual([{ type: "advance" }, { type: "skipHold", held: true }]); expect(detector.poll(pressed)).toEqual([]); expect(detector.poll(idle)).toEqual([{ type: "skipHold", held: false }])
  })
})
describe("focus and virtualization", () => {
  it("navigates registered focusables in tab order", () => {
    const manager = new FocusManager(); const calls: number[] = []
    const nodes = [0, 1, 2].map((index) => { const node = new Container(); const item = { focusable: true, focused: false, focusNode: node, activate: () => calls.push(index) }; manager.register(item); return item })
    manager.focus(0); manager.move(1); manager.handleKey("Enter"); expect(calls).toEqual([1]); expect(nodes[1]?.focused).toBe(true)
  })
  it("computes a bounded virtualized range", () => { expect(visibleRange(100, 20, 100, 0)).toEqual({ start: 0, end: 6 }); expect(visibleRange(100, 20, 100, 200)).toEqual({ start: 10, end: 16 }); expect(visibleRange(3, 20, 100, 200)).toEqual({ start: 3, end: 3 }) })
})
describe("save slot paging", () => {
  it("keeps a stable 2 by 3 page shape", () => { const slots = Array.from({ length: 8 }, (_, slot) => ({ slot, label: String(slot), savedAt: "", thumbnail: undefined })); expect(SLOTS_PER_PAGE).toBe(6); expect(pageSlots(slots, 0)).toHaveLength(6); expect(pageSlots(slots, 1).map((slot) => slot?.slot)).toEqual([6, 7, undefined, undefined, undefined, undefined]) })
})
describe("complete mapping and focus behaviour", () => {
  it("contains every default keyboard binding", () => {
    expect(Object.keys(defaultInputMap.keyboard)).toEqual(expect.arrayContaining(["Enter", " ", "a", "s", "Escape", "PageUp", "F5", "F9", "h", "Backspace", "1", "9"]))
  })

  it("uses arrows for control values and wraps tab focus", () => {
    const manager = new FocusManager()
    const values: string[] = []
    const items = ["one", "two"].map((name) => ({ focusable: true, focused: false, focusNode: new Container(), activate: () => values.push(name) }))
    manager.register(items[0]!)
    manager.register(items[1]!)
    manager.focus(0)
    manager.handleKey("ArrowLeft")
    manager.handleKey("Enter")
    expect(values).toEqual(["two"])
    manager.handleKey("Tab")
    manager.handleKey("Enter")
    expect(values).toEqual(["two", "one"])
  })

  it("clamps list scrolling and handles page keys", () => {
    const list = new ScrollList(Array.from({ length: 20 }, (_, index) => index), theme, (_value: number) => new Container(), 20, 100, 200)
    list.setOffset(9999)
    expect(list.scrollOffset).toBe(list.maxOffset)
    list.handleKey("PageUp")
    expect(list.scrollOffset).toBe(list.maxOffset - 100)
  })
})
