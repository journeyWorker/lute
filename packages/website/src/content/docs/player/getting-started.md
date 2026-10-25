---
title: Player getting started
description: Compile a Lute project, validate player.bindings.yaml, serve its bundle and wasm, and mount the 0.49 web player.
---

This guide uses the repository's `examples/player-demo`; the same steps work for a project of your own.

## 1. Compile the project

Build a complete bundle. Compile the whole project (no document operand) into a directory, because the browser runtime reads `project.index.json` and every artifact named by it.

```console
lute compile --project examples/player-demo -o dist/bundle
```

The bundle directory must contain `project.index.json` and all referenced artifact JSON files. Copy or serve the 0.41 runtime binding at `lute_runtime_wasm_bg.wasm` as well.

## 2. Write and validate bindings

Create `player.bindings.yaml` beside `lute.project.yaml`. It maps compiled staging ids to files and supplies the canvas and named effects. Paths are relative to `assetsBase` (default `assets/`). A minimal file is:

```yaml
bindingsVersion: "0.49"
canvas: { width: 1280, height: 720 }
volumes: { silent: 0, quiet: 0.25, normal: 0.5, loud: 0.75, full: 1 }
backgrounds:
  foyer: { image: bg/foyer.png }
characters:
  guide: { sprite: { default: char/guide.png } }
assetsBase: assets/
```

The full format is in [Bindings reference](/player/bindings/). Check it after compiling:

```console
lute check --project examples/player-demo
```

Whole-project `lute check` validates the schema, domain and cast keys, referenced files, effects, and flow occasions. A malformed shape is `E-BINDING-SHAPE`, an unknown effect or parameter is `E-BINDING-EFFECT`, a missing asset is `E-BINDING-FILE`, and a key outside the merged project vocabulary is `E-BINDING-KEY`. Missing, unused, and all other diagnostics are available with `--json`.

## 3. Mount in a page

Install the exact 0.49 packages:

```console
bun add @lute-lang/player-web@0.49.1 @lute-lang/player@0.49.1 @lute-lang/runtime@0.49.1 effect@4.0.2 @effect/platform-browser@4.0.2 pixi.js@8.22.0 @pixi/sound@6.0.1
```

`@lute-lang/player` declares `@lute-lang/runtime` and `effect` as peer dependencies, so the page installs them once and every package shares one `effect`.

Mount the player into a focusable element:

```html
<div id="player" tabindex="0"></div>
<script type="module" src="/main.js"></script>
```

```ts
import { mountPlayer } from "@lute-lang/player-web"

const element = document.querySelector<HTMLElement>("#player")!
const player = await mountPlayer(element, {
  bindings: { url: "/player.bindings.yaml" },
  bundleUrl: "/bundle/",
  wasmUrl: "/wasm/lute_runtime_wasm_bg.wasm",
  assetsUrl: "/assets/",
  storageKey: "my-game"
})
await player.act({ type: "newGame" })
```

`bundleUrl` points to the directory containing `project.index.json`;
`wasmUrl` points to the browser wasm binding. `assetsUrl` overrides
`assetsBase`. `mountPlayer` returns `act`, `view`, `send`, `snapshot`, `restore`,
`session`, `program`, `bundle`, and `unmount`; each Promise method is a thin wrapper
around the corresponding Effect and rejects with its failure. `session`,
`program` and `bundle` are the handle's values: `session.current` holds the
state that `program`'s read-only queries take.

To resume in a fresh player from a save, pass the save's runtime snapshot at
mount time and restore the full save:

```ts
const save = await player.snapshot()
await player.unmount()
const fresh = await mountPlayer(element, {
  bindings: { url: "/player.bindings.yaml" },
  bundleUrl: "/bundle/",
  wasmUrl: "/wasm/lute_runtime_wasm_bg.wasm",
  snapshot: save.snapshot
})
await fresh.restore(save)
```

## 4. Serve all files

The page, JavaScript bundle, bindings, bundle JSON, wasm, and assets must be served by the application (or by URLs allowed by its fetch/CORS policy). At minimum expose:

```text
/player.bindings.yaml
/bundle/project.index.json
/bundle/<every artifact path in project.index.json>
/wasm/lute_runtime_wasm_bg.wasm
/assets/<every bound file>
```

Do not open the HTML directly from `file://`; browsers cannot fetch the wasm and bundle reliably from a file URL.

## 5. Run the demo

From the repository root, build the CLI and run the web demo script:

```console
cargo build --release -p lute-cli
bun run --cwd packages/player-web demo
```

The script compiles `examples/player-demo` with its Korean locale bundle (`lute compile --project examples/player-demo --locale-bundle examples/player-demo/locales/bundle.json -o <out>`), bundles its `web/main.ts`, copies the runtime wasm, and starts a small server. Set `LUTE_BIN` to another `lute` executable or `PORT` to choose a port. The demo serves the bindings at `/player.bindings.yaml`, the compiled bundle at `/bundle/`, wasm at `/wasm/`, and generated assets at `/assets/`.

Settings → Locale offers the source locale (`en-US`) and `ko`; switching applies to the next line, choice and prompt shown, mid-run or on a new game. The bundle comes from `lute loc export` run in `examples/player-demo`, translated into `locales/ko.json`, then `lute loc import locales/ko.json -o locales/bundle.json`. A finished run is kept as the player's lineage: after a reload, New game starts run 2 with run 1's `user.*` values and its `prev.run` (the demo's first chapter shows both).

## 6. Stage layers

player-web draws the stage in fixed layers, bottom to top:

1. `background` — the `::bg` backdrop.
2. `camera` — the actors (`::actor`); framing and camera moves transform this layer.
3. `cg` — event illustrations (`::cg`).
4. `video` — `::video` playback.
5. `dialogue` — the dialogue window.
6. `choice` — choice and hub menus.
7. `overlay` — screens and menus (title, save/load, backlog, settings, ended).
8. `transition` — scene transitions.

A `::cg` therefore covers the actors on stage and stays under the dialogue window and the UI. `cgLayouts` place a CG (`fit`, `scale`, `x`, `y`) inside its layer; they do not change the order.

### Camera transitions

The `camera` layer rests fully visible (alpha 1, no mask). A `::camera` cue with a `move` stops a running transition, applies its framing and plays the move; a `move` takes precedence over a `transition`. With a `transition`, the effect decides how the new framing appears:

- `fade`, `dissolve`, `fadeOut`, `wipeLeft`/`wipeRight`/`wipeUp`/`wipeDown`, `slideLeft`, `slideRight` and `slideOut` hide the actors over the first half of the duration, apply the framing and focus, then show the actors again over the second half (alpha back to 1, a wipe reveal, a slide back into place). A wipe sweeps the whole stage, not just the span of the actors.
- Any other effect applies the framing, then plays.

Either way the cue ends with the actors visible (alpha 1, no mask) in the new framing. `cut`, `none`, a zero duration and an instant cue (skip, load) apply the framing at once. A transition that has started plays to its end even when the story reaches a choice first; a later camera cue stops it. Loading or rolling back during a transition stops it and shows the actors in the restored stage.

## 7. Title screen

The title screen's heading is `ui.title.text` when the bindings set it; without it the title draws no heading (player-web never shows a placeholder name). `ui.title.background` and `ui.title.music` are optional: without them the title has no backdrop and plays no music.

```yaml
bindingsVersion: "0.49"
ui:
  title: { text: "The Signal at Low Tide", background: ui/title.png, music: ui/title.wav }
```
