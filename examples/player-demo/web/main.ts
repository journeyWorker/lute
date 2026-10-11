import { mountPlayer } from "../../../packages/player-web/src/index.ts"
import type { PlayerAction, PlayerView } from "../../../packages/player/src/index.ts"

declare global {
  interface Window {
    luteDemo: {
      readonly view: () => Promise<PlayerView>
      readonly act: (action: PlayerAction) => Promise<void>
    }
  }
}
const host = document.querySelector<HTMLDivElement>("#player")
if (host === null) throw new Error("demo player host is missing")
host.focus()
const player = await mountPlayer(host, {
  bindings: { url: "/player.bindings.yaml" },
  bundleUrl: "/bundle/",
  wasmUrl: "/wasm/lute_runtime_wasm_bg.wasm",
  assetsUrl: "/assets/",
  storageKey: "lute-demo"
})
window.luteDemo = {
  view: (): Promise<PlayerView> => player.view(),
  act: (action: PlayerAction): Promise<void> => player.act(action)
}
