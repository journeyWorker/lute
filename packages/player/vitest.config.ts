import { resolve } from "node:path"
import { defineConfig } from "vitest/config"

export default defineConfig({
  resolve: {
    alias: {
      "@lute-lang/runtime": resolve(import.meta.dirname, "../runtime/src/index.ts")
    }
  },
  test: {
    include: ["test/**/*.test.ts"],
    exclude: ["test/browser/**"],
    // Session and director suites compile conformance projects with the lute
    // CLI and boot the wasm runtime; on a loaded CI runner that exceeds 5 s.
    testTimeout: 30_000
  }
})
