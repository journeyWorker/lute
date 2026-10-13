---
title: Player controls
description: Default keyboard, mouse, touch, and gamepad mappings plus auto, skip, backlog, and save controls.
---

`@lute-lang/player-web` maps device events to the core's device-independent `PlayerAction` values. Supply a custom `InputMap` to the input layer when a game needs another mapping. UI widgets consume keyboard events while a menu is open.

## Default keyboard map

| Key | Action |
| --- | --- |
| `Enter`, `Space` | Advance; while typing, reveal the line; while shown, release it |
| `A` | Toggle auto mode |
| `S` | Toggle skip mode |
| `Control` (hold) | Hold skip; release to return to normal |
| `Escape` | Open menu |
| `PageUp`, mouse-wheel up | Open backlog |
| `F5` / `F9` | Quick save / quick load |
| `H` | Hide or show UI |
| `Backspace` | Roll back one input point |
| `1`–`9` | Choose option id `1`–`9` when offered |

Key matching includes both lower- and upper-case `A`, `S`, and `H`. A host can replace this table with `InputMap.keyboard` entries or modifier-aware `KeyBinding` values.

## Mouse and touch

- Primary click or tap advances. It reveals a typing line first, then releases a shown line.
- A downward swipe opens the backlog. Wheel-up does the same.
- A long primary press (500 ms) opens the menu.
- Middle click hides the UI; right click opens the menu. The browser context menu is suppressed.
- Touch uses the same pointer events: tap, long press, and vertical swipe. Pointer cancellation cancels a long press.

## Gamepad

The portable gamepad reader polls the first connected standard-mapped gamepad and emits edge-triggered actions:

| Button | Action |
| --- | --- |
| A (0) | Advance |
| B (1) | Open menu |
| Y (3) | Open backlog |
| LB (4) | Rollback |
| RB (5, hold) | Hold skip |
| Start (9) | Open menu |

A disconnected gamepad resets edge detection; holding a button does not repeatedly advance.

## Modes

Auto mode waits until voice ends (when present), then waits `settings.autoDelay` seconds before releasing a shown line. Any input or overlay cancels the pending auto action. Skip mode can be `read` (the default: stop on unread lines) or `all`. Both modes still stop at choices, bridges, and idle; skip snaps visual effects to their end state. Toggling skip during a batch takes effect at the next cue.

The machine's line progression is deliberately two-step: advance while typing emits `line.reveal`; advance after the line is fully shown emits `line.release`. This keeps input behavior deterministic across renderers. A presenter that takes the confirm itself can declare `confirmsLines: true` (an advance while typing is kept as the release once the line is shown), or the host can choose `linePacing: "presenter"`, where the presenter's `showLine` completion releases the line and advance and auto timers leave lines alone.

## Backlog and read history

Opening backlog pauses overlays and auto/countdown activity. A backlog entry contains `lineId`, the original `voiceKey`, speaker name, text, and `hasVoice`; replay resolves the voice file by that `voiceKey` (never by `lineId`) without changing runtime state. Close the backlog with the UI's close action, then input resumes.

## Saves and quick saves

`F5` sends `quickSave`; `F9` sends `quickLoad`. The menu provides numbered save/load slots, overwrite and delete confirmation, and an auto slot. See [Custom hosts](/player/custom-hosts/#saves-and-profile-data) for the save format, project fingerprint, snapshot stage, and incompatibility behavior. `mountPlayer` persists browser saves under `storageKey` in local storage.

## Action names for custom UIs

The core accepts `advance`, `toggleAuto`, `toggleSkip`, `skipHold`, `rollback`, `openBacklog`, `openMenu`, `openSave`, `openLoad`, `openSettings`, `quickSave`, `quickLoad`, `hideUI`, `choose`, `close`, `newGame`, `continue`, `toTitle`, `saveSlot`, `loadSlot`, `deleteSlot`, `confirm`, `pickIdle`, `replayVoice`, `setSettings`, and `send` (a runtime input at idle or the ending; a refusal is logged and changes nothing). A custom renderer can emit these directly without using browser input listeners.
