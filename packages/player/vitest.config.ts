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
    exclude: ["test/browser/**"]
  }
})
