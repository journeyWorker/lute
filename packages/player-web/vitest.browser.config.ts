import { resolve } from "node:path"
import { playwright } from "@vitest/browser-playwright"
import { defineConfig } from "vitest/config"

const packageRoot = resolve(import.meta.dirname)
const repositoryRoot = resolve(packageRoot, "../..")

/** Real-bundle files: each plays compiled projects through a mounted player. */
const realSuite = "test/browser/real/**/*.browser.test.ts"

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
    },
    projects: [
      {
        extends: true,
        test: {
          name: "browser",
          include: ["test/browser/**/*.browser.test.ts"],
          exclude: [realSuite]
        }
      },
      {
        extends: true,
        test: {
          name: "real",
          include: [realSuite],
          // Compiles the real fixtures (examples/player-demo) the suite mounts.
          globalSetup: ["test/browser/globalSetup.ts"],
          // One file at a time: their players animate on the per-frame clock
          // (at most 0.1s a frame), so files rendering side by side on a few
          // CPUs (SwiftShader) slow every playthrough past its deadlines.
          fileParallelism: false
        }
      }
    ]
  }
})
