import { resolve } from "node:path"
import { playwright } from "@vitest/browser-playwright"
import { defineConfig } from "vitest/config"

const packageRoot = resolve(import.meta.dirname)
const repositoryRoot = resolve(packageRoot, "../..")

export default defineConfig({
  root: packageRoot,
  resolve: {
    alias: {
      "@lute-lang/runtime/browser": resolve(packageRoot, "../runtime/src/browser.ts"),
      "@lute-lang/runtime": resolve(packageRoot, "../runtime/src/index.ts"),
      "@lute-lang/player": resolve(packageRoot, "../player/src/index.ts")
    }
  },
  server: {
    fs: { allow: [repositoryRoot] }
  },
  test: {
    include: ["test/browser/**/*.browser.test.ts"],
    // Compiles the real fixtures (examples/player-demo) the `test/browser/real` suite mounts.
    globalSetup: ["test/browser/globalSetup.ts"],
    browser: {
      enabled: true,
      headless: true,
      provider: playwright({
        launchOptions: {
          headless: true,
          args: ["--use-gl=swiftshader", "--enable-unsafe-swiftshader", "--autoplay-policy=no-user-gesture-required"]
        }
      }),
      instances: [{ browser: "chromium" }]
    }
  }
})
