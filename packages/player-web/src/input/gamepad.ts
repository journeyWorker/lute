import type { PlayerAction } from "../../../player/src/ports/index.js"

/** Logical gamepad buttons used by the visual-novel input layer. */
export interface GamepadState {
  readonly a: boolean
  readonly b: boolean
  readonly y: boolean
  readonly rb: boolean
  readonly lb: boolean
  readonly start: boolean
}

/** Read the standard mapping indices into the portable gamepad state. */
export function readGamepadState(gamepad: Gamepad): GamepadState {
  return {
    a: gamepad.buttons[0]?.pressed ?? false,
    b: gamepad.buttons[1]?.pressed ?? false,
    y: gamepad.buttons[3]?.pressed ?? false,
    rb: gamepad.buttons[5]?.pressed ?? false,
    lb: gamepad.buttons[4]?.pressed ?? false,
    start: gamepad.buttons[9]?.pressed ?? false
  }
}

/** Emits actions only on button press edges, while allowing RB hold state. */
export class GamepadEdgeDetector {
  private previous: GamepadState = emptyState()
  /** Poll a state and return newly pressed actions. */
  poll(current: GamepadState): ReadonlyArray<PlayerAction> {
    const actions: PlayerAction[] = []
    if (current.a && !this.previous.a) actions.push({ type: "advance" })
    if (current.b && !this.previous.b) actions.push({ type: "openMenu" })
    if (current.y && !this.previous.y) actions.push({ type: "openBacklog" })
    if (current.lb && !this.previous.lb) actions.push({ type: "rollback" })
    if (current.start && !this.previous.start) actions.push({ type: "openMenu" })
    if (current.rb !== this.previous.rb) actions.push({ type: "skipHold", held: current.rb })
    this.previous = current
    return actions
  }
  /** Reset edge state after a device disconnect. */
  reset(): void { this.previous = emptyState() }
}

function emptyState(): GamepadState { return { a: false, b: false, y: false, rb: false, lb: false, start: false } }
