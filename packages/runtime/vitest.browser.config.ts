import { resolve } from "node:path"
import { playwright } from "@vitest/browser-playwright"
import { defineConfig } from "vitest/config"

const packageRoot = resolve(import.meta.dirname)
const repositoryRoot = resolve(packageRoot, "../..")
const wasmRoot = resolve(packageRoot, "src/wasm")
const hubFixtureRoot = resolve(repositoryRoot, "crates/lute-runtime-wasm/tests/fixtures/hub-once")

const fsUrl = (path: string) => `/@fs/${path}`

export default defineConfig({
  root: packageRoot,
  resolve: {
    alias: {
      "@lute-lang/runtime/browser": resolve(packageRoot, "src/browser.ts")
    }
  },
  define: {
    __LUTE_RUNTIME_WASM_URL__: JSON.stringify(fsUrl(resolve(wasmRoot, "lute_runtime_wasm_bg.wasm"))),
    __LUTE_HUB_BUNDLE_BASE__: JSON.stringify(`${fsUrl(hubFixtureRoot)}/`)
  },
  server: {
    fs: {
      allow: [repositoryRoot]
    }
  },
  test: {
    include: ["test/browser/**/*.browser.test.ts"],
    testTimeout: 120_000,
    browser: {
      enabled: true,
      headless: true,
      provider: playwright({ launchOptions: { headless: true } }),
      instances: [{ browser: "chromium" }]
    }
  }
})
