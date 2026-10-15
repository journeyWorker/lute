# @lute-lang/player-web

The Lute 0.46 browser visual-novel player. It supplies PixiJS 8 rendering, Web Audio, screens, and keyboard/mouse/touch/gamepad input for the renderer-free `@lute-lang/player` core.

## Install

```sh
  bun add @lute-lang/player-web@0.46.0 @lute-lang/player@0.46.0 @lute-lang/runtime@0.46.0 effect@4.0.2 @effect/platform-browser@4.0.2 pixi.js@8.22.0 @pixi/sound@6.0.1
  # npm install @lute-lang/player-web@0.46.0 @lute-lang/player@0.46.0 @lute-lang/runtime@0.46.0 effect@4.0.2 @effect/platform-browser@4.0.2 pixi.js@8.22.0 @pixi/sound@6.0.1
```

The package requires exact `effect@4.0.2` and `@effect/platform-browser@4.0.2` peers. Runtime and player packages are released in lockstep.

## Minimal usage

Serve a `lute compile --all` bundle, the runtime wasm file, the bindings file, and its assets. Then mount into an element:

```ts
import { mountPlayer } from "@lute-lang/player-web"

const options = {
  bindings: { url: "/player.bindings.yaml" },
  bundleUrl: "/bundle/",
  wasmUrl: "/wasm/lute_runtime_wasm_bg.wasm",
  assetsUrl: "/assets/",
  storageKey: "my-game"
} as const
const player = await mountPlayer(document.querySelector("#player")!, options)

await player.act({ type: "newGame" })
const view = await player.view()
// A player save can resume a fresh player:
const save = await player.snapshot()
await player.unmount()
const resumed = await mountPlayer(document.querySelector("#player")!, { ...options, snapshot: save.snapshot })
await resumed.restore(save)
// Later:
await resumed.unmount()
```

`mountPlayer` returns Promise methods `act`, `view`, `send`, `snapshot`, `restore`, `program`, `bundle`, and `unmount`. The methods run the same Effects as the scoped `LuteWeb.mount` handle and reject with the Effect failure. The handle's `bundle` is the exact compiled bundle loaded by the player, so hosts can inspect artifacts without loading it again. `MountOptions` accepts the same `seed`/`snapshot`/`session` start union and `cueFactory`, `linePacing`, and `autosave` options as `LutePlayer.make`. Textures use a private Pixi asset manager; player-web does not initialize Pixi's global `Assets`.

## Cue handling

- **Names.** The dialogue name plate and backlog rows show `speakerName`, the cast display name from the bundle index (`cast: { id: name }`); name colours stay keyed by the speaker id.
- **Music.** `PixiAudio` honours `AudioPlay.restart`: `restart: true` plays the file from its start; without it, the music file already playing only changes volume, fading over `fade`, and a different file crossfades.
- **Custom vfx.** A `StageCue.Vfx` whose `effect.effect` is `"custom"` (a project vfx bound as `{ effect: custom }`, or a vfx type with no binding) has no Pixi effect: the presenter plays nothing and logs `effect.name` once at debug level (`PresenterOptions.debug`, default `console.debug`). Render project effects with your own `Presenter`.

## API and guides

- [Web architecture](../../docs/player/web-architecture.md)
- [Getting started](/player/getting-started/)
- [Controls](/player/controls/)
- [Bindings reference](/player/bindings/)
- [Demo game](../../examples/player-demo/)

Source and releases: [github.com/journeyWorker/lute](https://github.com/journeyWorker/lute).
