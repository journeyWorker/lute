---
title: Player overview
description: The Lute 0.40 renderer-free player core and PixiJS web player, their boundaries, packages, and architecture.
---

Lute 0.40 ships two player packages. [`@lute-lang/player`](https://www.npmjs.com/package/@lute-lang/player) is a renderer-free core: it owns progression, cue scheduling, interaction state, runtime sessions, saves, read history, backlog, auto/skip, rollback, and host bridges. [`@lute-lang/player-web`](https://www.npmjs.com/package/@lute-lang/player-web) supplies a browser implementation using PixiJS 8, Web Audio, screens, and input.

Neither package compiles a story. Compile a project with `lute compile --all`, serve the resulting bundle and runtime wasm, and give the player a checked `player.bindings.yaml` file.

## Architecture

```text
PlayerAction ──► playerMachine ──MachineEvent──► director ──► Presenter
(input)          (pure state)                    (time/effects) ├─ AudioPort
                                                                  └─ AssetLoader
                                  │
                    ┌─────────────┼─────────────┐
                    ▼             ▼             ▼
                 session       cues +       stores
                 runtime       scheduler     saves/profile
                                  │
                        runtime bundle + stage

player-web: mount ─► PixiApp ─► World/PixiPresenter
                         ├─ PixiAudio
                         ├─ PixiAssets
                         ├─ UI screens
                         └─ keyboard/pointer/touch/gamepad input
```

The runtime owns story state, stage, awaits, and outcomes. The player sends inputs and renders returned records; it never chooses a runtime outcome itself. The machine is pure and emits commands. The director owns timers and Effect fibers, invokes ports, and restores the runtime stage after save or rollback. Saves contain runtime snapshot data plus player backlog/log data, never an actor snapshot.

## Packages

| Package | Responsibility | Public entry point |
| --- | --- | --- |
| `@lute-lang/player@0.40.0` | Renderer-free player core, ports, bindings decoder, cues, scheduler, session, stores | `LutePlayer.make(options)` |
| `@lute-lang/player-web@0.40.0` | PixiJS/Web Audio presenter, assets, screens, input, browser mounting | `mountPlayer(element, options)` or `LuteWeb.mount` |
| `@lute-lang/runtime@0.40.0` | Bundle loading, wasm runtime session, event contract, snapshots and queries | `@lute-lang/runtime/browser` |

The core API is intentionally host-neutral. Implement `Presenter`, `AudioPort`, and `AssetLoader` to use a different renderer; use `PlayerHost` for bridge calls, plugin records and idle policy. The player handle also sends runtime inputs at idle or at the ending (`send`), continues from a host checkpoint (`restore`), and exposes the runtime `session` and its `program` for host queries; the player can start from a seed, a snapshot, or a runtime session the host built. See [Custom hosts](/player/custom-hosts/). For the complete staging vocabulary, see [Bindings](/player/bindings/); for the browser path, see [Getting started](/player/getting-started/).
