import { readFileSync } from "node:fs"
import { resolve } from "node:path"
import { Effect, Exit, Option } from "effect"
import { describe, expect, it } from "@effect/vitest"
import { BindingsInvalid, decode, resolveFile } from "../../src/bindings/load.js"

const docsPath = resolve(import.meta.dirname, "../../../../docs/player/bindings.md")
const cleanFixturePath = resolve(import.meta.dirname, "../../../../crates/lute-cli/tests/fixtures/bindings-clean/player.bindings.yaml")
const docsExample = readFileSync(docsPath, "utf8").match(/```yaml\n([\s\S]*?)\n```/)?.[1]
if (docsExample === undefined) throw new Error("bindings documentation example is missing")

const failure = (exit: Exit.Exit<unknown, BindingsInvalid>): BindingsInvalid => {
  if (Exit.isSuccess(exit)) throw new Error("expected bindings decoding to fail")
  const error = Option.getOrUndefined(Exit.findErrorOption(exit))
  if (!(error instanceof BindingsInvalid)) throw new Error("expected BindingsInvalid")
  return error
}

describe("player bindings loading", () => {
  it.effect("decodes the complete YAML example from the bindings documentation", () =>
    Effect.gen(function* () {
      const bindings = yield* decode(docsExample)
      expect(bindings.bindingsVersion).toBe("0.52")
      expect(bindings.characters?.guide?.sprite.default).toBe("char/guide.png")
      expect(bindings.flow?.newGame?.occasion).toBe("start")
    }))

  it.effect("decodes the clean CLI fixture", () =>
    Effect.gen(function* () {
      const bindings = yield* decode(readFileSync(cleanFixturePath, "utf8"))
      expect(bindings).toEqual({ bindingsVersion: "0.52", canvas: { width: 1920, height: 1080 } })
    }))

  it.effect("rejects unknown keys with a BindingsInvalid path", () =>
    Effect.gen(function* () {
      const rejected = yield* Effect.exit(decode({ bindingsVersion: "0.52", canvas: { width: 1, height: 1 }, mystery: true }))
      const error = failure(rejected)
      expect(error.path).toContain("bindings.mystery")
    }))

  it.effect("rejects wrong types with a BindingsInvalid path", () =>
    Effect.gen(function* () {
      const rejected = yield* Effect.exit(decode({ bindingsVersion: "0.52", canvas: { width: "wide", height: 1 } }))
      const error = failure(rejected)
      expect(error.path).toBe("bindings.canvas.width")
    }))

  it.effect("reports YAML syntax errors with line and column", () =>
    Effect.gen(function* () {
      const rejected = yield* Effect.exit(decode("bindingsVersion: [\ncanvas: {}"))
      const error = failure(rejected)
      expect(error.path).toBe("bindings")
      expect(error.message).toMatch(/line \d+, column \d+/)
    }))

  it.effect("resolves relative assets with an assets base with or without a slash", () =>
    Effect.gen(function* () {
      const bindings = yield* decode({ bindingsVersion: "0.52", canvas: { width: 1, height: 1 }, assetsBase: "media", assets: { theme: "audio/theme.ogg" } })
      expect(resolveFile(bindings, "theme")).toBe("media/audio/theme.ogg")
      expect(resolveFile({ ...bindings, assetsBase: "media/" }, "theme")).toBe("media/audio/theme.ogg")
    }))

  it.effect("resolves assets against absolute URLs and preserves absolute mapped URLs", () =>
    Effect.gen(function* () {
      const bindings = yield* decode({ bindingsVersion: "0.52", canvas: { width: 1, height: 1 }, assetsBase: "https://cdn.example.test/assets/", assets: { theme: "audio/theme.ogg", remote: "https://other.example.test/theme.ogg" } })
      expect(resolveFile({ ...bindings, assetsBase: "https://cdn.example.test/assets" }, "theme")).toBe("https://cdn.example.test/assets/audio/theme.ogg")
      expect(resolveFile(bindings, "remote")).toBe("https://other.example.test/theme.ogg")
    }))
})
