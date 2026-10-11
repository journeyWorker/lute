import { resolve } from "node:path"
import { playwright } from "@vitest/browser-playwright"
import { defineConfig } from "vitest/config"

const packageRoot = resolve(import.meta.dirname)
const repositoryRoot = resolve(packageRoot, "../..")

export default defineConfig({
  root: packageRoot,
  resolve: {
    alias: {
      "@lute-lang/runtime": resolve(packageRoot, "../runtime/src/index.ts"),
      "@lute-lang/player": resolve(packageRoot, "../player/src/index.ts")
    }
  },
  server: {
    fs: { allow: [repositoryRoot] }
  },
  test: {
    include: ["test/browser/**/*.browser.test.ts"],
    browser: {
      enabled: true,
      headless: true,
      provider: playwright({
        launchOptions: {
          headless: true,
          args: ["--use-gl=swiftshader", "--enable-unsafe-swiftshader"]
        }
      }),
      instances: [{ browser: "chromium" }]
    }
  }
})
