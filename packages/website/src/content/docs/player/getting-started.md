---
title: Player getting started
description: Compile a Lute project, validate player.bindings.yaml, serve its bundle and wasm, and mount the 0.41 web player.
---

This guide uses the repository's `examples/player-demo`; the same steps work for a project of your own.

## 1. Compile the project

Build a complete bundle. `--all` is required because the browser runtime reads `project.index.json` and every artifact named by it.

```console
lute compile --all examples/player-demo -o dist/bundle
```

The bundle directory must contain `project.index.json` and all referenced artifact JSON files. Copy or serve the 0.41 runtime binding at `lute_runtime_wasm_bg.wasm` as well.

## 2. Write and validate bindings

Create `player.bindings.yaml` beside `lute.project.yaml`. It maps compiled staging ids to files and supplies the canvas and named effects. Paths are relative to `assetsBase` (default `assets/`). A minimal file is:

```yaml
bindingsVersion: "0.41"
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
lute check-project examples/player-demo
```

`check-project` validates the schema, domain and cast keys, referenced files, effects, and flow occasions. A malformed shape is `E-BINDING-SHAPE`, an unknown effect or parameter is `E-BINDING-EFFECT`, a missing asset is `E-BINDING-FILE`, and a key outside the merged project vocabulary is `E-BINDING-KEY`. Missing, unused, and all other diagnostics are available with `--json`.

## 3. Mount in a page

Install exact 0.42 packages:

```console
bun add @lute-lang/player-web@0.42.0 @lute-lang/player@0.42.0 @lute-lang/runtime@0.42.0 effect@4.0.2 @effect/platform-browser@4.0.2 pixi.js@8.22.0 @pixi/sound@6.0.1
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

`bundleUrl` points to the directory containing `project.index.json`; `wasmUrl` points to the browser wasm binding. `assetsUrl` overrides `assetsBase`. Omit it to resolve the configured base relative to the bindings URL or page URL. `mountPlayer` returns `act`, `view`, and `unmount`; call `unmount()` to release Pixi, runtime, audio, and storage resources.

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

The script compiles `examples/player-demo`, bundles its `web/main.ts`, copies the runtime wasm, and starts a small server. Set `LUTE_BIN` to another `lute` executable or `PORT` to choose a port. The demo serves the bindings at `/player.bindings.yaml`, the compiled bundle at `/bundle/`, wasm at `/wasm/`, and generated assets at `/assets/`.
