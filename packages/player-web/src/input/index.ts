export { defaultInputMap, resolveKeyboardAction, LongPressRecognizer, SwipeRecognizer, layer, type InputMap, type KeyBinding } from "./map.js"
export { GamepadEdgeDetector, readGamepadState, type GamepadState } from "./gamepad.js"

import { layer } from "./map.js"

/** Namespaced input layer API. */
export const Input = { layer }
