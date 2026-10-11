import { Effect, Queue, Stream } from "effect"
import type { PlayerAction } from "@lute-lang/player"

/** A keyboard binding with optional modifier requirements. */
export interface KeyBinding {
  readonly action: PlayerAction
  readonly ctrl?: boolean
  readonly shift?: boolean
  readonly alt?: boolean
}

/** Device mapping table used by Input.layer. */
export interface InputMap {
  readonly keyboard: Readonly<Record<string, PlayerAction | KeyBinding>>
  readonly pointer: Readonly<Record<string, PlayerAction>>
}

/** Default keyboard, pointer and gesture table for visual-novel controls. */
export const defaultInputMap: InputMap = {
  keyboard: {
    Enter: { type: "advance" }, " ": { type: "advance" }, a: { type: "toggleAuto" }, A: { type: "toggleAuto" }, s: { type: "toggleSkip" }, S: { type: "toggleSkip" }, Escape: { type: "openMenu" }, PageUp: { type: "openBacklog" }, F5: { type: "quickSave" }, F9: { type: "quickLoad" }, h: { type: "hideUI" }, H: { type: "hideUI" }, Backspace: { type: "rollback" },
    "1": { type: "choose", option: "1" }, "2": { type: "choose", option: "2" }, "3": { type: "choose", option: "3" }, "4": { type: "choose", option: "4" }, "5": { type: "choose", option: "5" }, "6": { type: "choose", option: "6" }, "7": { type: "choose", option: "7" }, "8": { type: "choose", option: "8" }, "9": { type: "choose", option: "9" }
  },
  pointer: { click: { type: "advance" }, contextmenu: { type: "openMenu" }, middle: { type: "hideUI" } }
}

/** Resolve a keyboard event through a mapping table. */
export function resolveKeyboardAction(event: KeyboardEvent, map: InputMap = defaultInputMap): PlayerAction | undefined {
  if (event.key === "Control") return undefined
  const binding = map.keyboard[event.key]
  if (binding === undefined) return undefined
  if (isKeyBinding(binding)) {
    if (binding.ctrl !== undefined && binding.ctrl !== event.ctrlKey) return undefined
    if (binding.shift !== undefined && binding.shift !== event.shiftKey) return undefined
    if (binding.alt !== undefined && binding.alt !== event.altKey) return undefined
    return binding.action
  }
  return binding
}

/** Recognizes a pointer hold without allocating per move event. */
export class LongPressRecognizer {
  private started = -1
  private emitted = false
  /** Start a pointer gesture at timestamp milliseconds. */
  start(at: number): void { this.started = at; this.emitted = false }
  /** End the gesture and return true when it was a long press. */
  end(at: number, threshold = 500): boolean {
    const result = this.started >= 0 && at - this.started >= threshold && !this.emitted
    this.started = -1
    this.emitted = result
    return result
  }
  /** Check a held gesture and emit at most once. */
  check(at: number, threshold = 500): boolean {
    if (this.started < 0 || this.emitted || at - this.started < threshold) return false
    this.emitted = true
    return true
  }
  /** Cancel an active gesture. */
  cancel(): void { this.started = -1; this.emitted = false }
}

/** Recognizes a vertical swipe from pointer coordinates. */
export class SwipeRecognizer {
  private startY = 0
  private active = false
  /** Begin tracking a pointer. */
  start(y: number): void { this.startY = y; this.active = true }
  /** End tracking and return swipe direction when threshold is crossed. */
  end(y: number, threshold = 48): "up" | "down" | undefined {
    if (!this.active) return undefined
    this.active = false
    const delta = y - this.startY
    if (Math.abs(delta) < threshold) return undefined
    return delta < 0 ? "up" : "down"
  }
}

/** Attach DOM input listeners and expose their actions as a scoped stream. */
export function layer(element: HTMLElement, map: InputMap = defaultInputMap, uiConsumes?: () => boolean, uiPointer?: (event: PointerEvent) => boolean): Stream.Stream<PlayerAction> {
  let emit: (action: PlayerAction) => void = () => {}
  return Stream.callback<PlayerAction>((queue) => Effect.acquireRelease(
    Effect.sync(() => {
      emit = (action) => { Queue.offerUnsafe(queue, action) }
      const longPress = new LongPressRecognizer()
      const swipe = new SwipeRecognizer()
      let controlHeld = false
      const down = (event: PointerEvent): void => {
        if (uiConsumes?.() === true || event.defaultPrevented) { longPress.cancel(); return }
        longPress.start(event.timeStamp)
        swipe.start(event.clientY)
        window.setTimeout(() => { if (longPress.check(performance.now())) emit({ type: "openMenu" }) }, 500)
      }
      const up = (event: PointerEvent): void => {
        if (event.button !== 0) return
        if (uiConsumes?.() === true) {
          longPress.cancel()
          uiPointer?.(event)
          return
        }
        const long = longPress.end(event.timeStamp)
        if (long) emit({ type: "openMenu" })
        else if (swipe.end(event.clientY) === "down") emit({ type: "openBacklog" })
        else emit(map.pointer.click ?? { type: "advance" })
      }
      const key = (event: KeyboardEvent): void => {
        if (event.defaultPrevented) return
        if (uiConsumes?.() === true) {
          event.preventDefault()
          return
        }
        if (event.key === "Control" && !controlHeld) {
          controlHeld = true
          emit({ type: "skipHold", held: true })
        }
        const action = resolveKeyboardAction(event, map)
        if (action !== undefined) {
          event.preventDefault()
          emit(action)
        }
      }
      const keyup = (event: KeyboardEvent): void => {
        if (event.key === "Control" && controlHeld) {
          controlHeld = false
          emit({ type: "skipHold", held: false })
        }
      }
      const wheel = (event: WheelEvent): void => {
        if (event.deltaY < 0) { event.preventDefault(); emit({ type: "openBacklog" }) }
        if (event.defaultPrevented) return
      }
      const pointer = (event: PointerEvent): void => {
        if (event.button === 1) { event.preventDefault(); emit(map.pointer.middle ?? { type: "hideUI" }) }
        if (event.button === 2) { event.preventDefault(); emit(map.pointer.contextmenu ?? { type: "openMenu" }) }
      }
      const cancel = (): void => longPress.cancel()
      const context = (event: MouseEvent): void => event.preventDefault()
      element.addEventListener("keydown", key)
      element.addEventListener("keyup", keyup)
      element.addEventListener("pointerdown", down)
      element.addEventListener("pointerup", up)
      element.addEventListener("pointercancel", cancel)
      element.addEventListener("pointerup", pointer)
      element.addEventListener("wheel", wheel, { passive: false })
      element.addEventListener("contextmenu", context)
      return { key, keyup, down, up, pointer, wheel, cancel, context, longPress }
    }),
    (registered) => Effect.sync(() => {
      element.removeEventListener("keydown", registered.key)
      element.removeEventListener("keyup", registered.keyup)
      element.removeEventListener("pointerdown", registered.down)
      element.removeEventListener("pointerup", registered.up)
      element.removeEventListener("pointercancel", registered.cancel)
      element.removeEventListener("pointerup", registered.pointer)
      element.removeEventListener("wheel", registered.wheel)
      element.removeEventListener("contextmenu", registered.context)
      registered.longPress.cancel()
      emit = () => {}
    })
  ))
}

function isKeyBinding(value: PlayerAction | KeyBinding): value is KeyBinding { return "action" in value }
