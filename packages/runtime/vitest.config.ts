import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["test/**/*.test.ts"],
    exclude: ["test/browser/**"],
    globalSetup: "./test/globalSetup.ts",
    testTimeout: 120_000
  }
});
