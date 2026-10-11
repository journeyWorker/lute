import { describe, expect, it } from "@effect/vitest"
import { Effect, Schema } from "effect"
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs"
import { join } from "node:path"
import * as Ir from "../src/generated/Ir.js"
import { compileBundle, repositoryRoot, sessionCases } from "./support.js"

const filesUnder = (root: string): string[] => {
  if (!existsSync(root)) return []
  const out: string[] = []
  const visit = (path: string): void => {
    for (const entry of readdirSync(path)) {
      const child = join(path, entry)
      if (statSync(child).isDirectory()) visit(child)
      else if (entry === "artifact.json" && !child.includes("/conformance/invalid/")) out.push(child)
    }
  }
  visit(root)
  return out.sort()
}

const commandLike = (value: unknown): value is Record<string, unknown> => {
  if (value === null || typeof value !== "object" || Array.isArray(value)) return false
  const record = value as Record<string, unknown>
  return typeof record.kind === "string" && typeof record.position === "string"
}

const walkCommands = (
  value: unknown,
  file: string,
  seen: WeakSet<object>,
  visit: (command: Record<string, unknown>, file: string) => void
): void => {
  if (value === null || typeof value !== "object") return
  if (seen.has(value)) return
  seen.add(value)
  if (commandLike(value)) visit(value, file)
  if (Array.isArray(value)) {
    for (const item of value) walkCommands(item, file, seen, visit)
  } else {
    for (const child of Object.values(value)) walkCommands(child, file, seen, visit)
  }
}

const decodeArtifact = (file: string): number => {
  const document: unknown = JSON.parse(readFileSync(file, "utf8"))
  let count = 0
  walkCommands(document, file, new WeakSet(), (command, source) => {
    const position = command.position
    try {
      Schema.decodeUnknownSync(Ir.Command)(command)
    } catch (error) {
      throw new Error(`${source} @ ${position}: ${String(error)}`)
    }
    count += 1
  })
  return count
}

describe("compiled IR command schemas", () => {
  it.effect("decodes every command recursively in conformance artifacts", () =>
    Effect.gen(function* () {
      const roots = [join(repositoryRoot, "conformance")]
      for (const session of sessionCases()) roots.push(yield* compileBundle(session.project))
      const artifacts = roots.flatMap(filesUnder)
      expect(artifacts.length).toBeGreaterThan(0)
      const decoded = artifacts.map(decodeArtifact).reduce((sum, count) => sum + count, 0)
      expect(decoded).toBeGreaterThan(0)
    })
  )
})
