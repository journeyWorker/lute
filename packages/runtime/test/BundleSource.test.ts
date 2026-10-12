import { readFileSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import { Effect, Exit, Option } from "effect"
import { describe, expect, it } from "@effect/vitest"
import * as BundleSource from "../src/BundleSource.js"
import * as Errors from "../src/Errors.js"
import * as LuteBun from "../src/bun.js"
import { compileBundle, evaluateFixtureRoot } from "./support.js"

const load = (directory: string) =>
  Effect.flatMap(BundleSource.BundleSource, (source) => source.load).pipe(
    Effect.provide(LuteBun.bundle(directory))
  )

describe("BundleSource", () => {
  it.effect("decodes the index as Ir.projectIndex (spec 0.41.0 §4.4)", () => Effect.gen(function* () {
    const directory = yield* compileBundle(evaluateFixtureRoot)
    const bundle = yield* load(directory)
    const [document] = bundle.index.documents
    expect(document?.kind).toBe("scene")
    expect(bundle.artifacts[document?.artifact ?? ""]).toBeDefined()
    expect(bundle.index.defs?.["atLeast"]?.params).toEqual([{ name: "n", type: "int" }])
  }))

  it.effect("rejects an index key the schema does not know as malformed", () => Effect.gen(function* () {
    const directory = yield* compileBundle(evaluateFixtureRoot)
    const path = join(directory, "project.index.json")
    writeFileSync(path, JSON.stringify({ ...JSON.parse(readFileSync(path, "utf8")), bogus: 1 }))
    const exit = yield* Effect.exit(load(directory))
    const error = Option.getOrUndefined(Exit.findErrorOption(exit))
    expect(error).toBeInstanceOf(Errors.BundleMalformed)
    expect(error).toMatchObject({ path: "project.index.json" })
  }))
})
