import { readFileSync, writeFileSync } from "node:fs"
import { join } from "node:path"
import { Effect, Exit, Option } from "effect"
import { describe, expect, it } from "@effect/vitest"
import * as BundleSource from "../src/BundleSource.js"
import * as Errors from "../src/Errors.js"
import * as LuteBun from "../src/bun.js"
import { compileBundle, evaluateFixtureRoot, repositoryRoot } from "./support.js"

const load = (directory: string) =>
  Effect.flatMap(BundleSource.BundleSource, (source) => source.load).pipe(
    Effect.provide(LuteBun.bundle(directory))
  )

/** docs/examples/haven declares scene `after:` and quest `follows=` prerequisites. */
const havenRoot = join(repositoryRoot, "docs", "examples", "haven")

const rewriteFirstEdge = (directory: string, edit: (edge: Record<string, unknown>) => Record<string, unknown>): void => {
  const path = join(directory, "project.index.json")
  const index = JSON.parse(readFileSync(path, "utf8")) as { prereqEdges: Array<Record<string, unknown>> }
  const [first, ...rest] = index.prereqEdges
  writeFileSync(path, JSON.stringify({ ...index, prereqEdges: [edit(first ?? {}), ...rest] }))
}

const malformedIndex = (exit: Exit.Exit<unknown, unknown>): void => {
  const error = Option.getOrUndefined(Exit.findErrorOption(exit))
  expect(error).toBeInstanceOf(Errors.BundleMalformed)
  expect(error).toMatchObject({ path: "project.index.json" })
}

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

  it.effect("decodes prereqEdges carrying `after` and `follows` with their node", () => Effect.gen(function* () {
    const directory = yield* compileBundle(havenRoot)
    const bundle = yield* load(directory)
    const edges = bundle.index.prereqEdges
    const after = edges.find((edge) => edge.after !== undefined)
    const follows = edges.find((edge) => edge.follows !== undefined)
    expect(after?.node).toEqual(expect.any(String))
    expect(after?.formula).toBeDefined()
    expect(follows?.node).toEqual(expect.any(String))
    expect(follows?.after).toBeUndefined()
  }))

  it.effect("rejects a prereqEdge with neither `after` nor `follows`", () => Effect.gen(function* () {
    const directory = yield* compileBundle(havenRoot)
    rewriteFirstEdge(directory, (edge) => ({ node: edge["node"] }))
    malformedIndex(yield* Effect.exit(load(directory)))
  }))

  it.effect("rejects a prereqEdge with both `after` and `follows`", () => Effect.gen(function* () {
    const directory = yield* compileBundle(havenRoot)
    rewriteFirstEdge(directory, (edge) => ({ node: edge["node"], after: "visited(\"a\")", follows: "completed(\"b\")" }))
    malformedIndex(yield* Effect.exit(load(directory)))
  }))
})
