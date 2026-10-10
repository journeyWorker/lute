import { mkdtempSync, readFileSync, readdirSync } from "node:fs"
import { tmpdir } from "node:os"
import { join, resolve } from "node:path"
import { Effect, Schema } from "effect"

export const repositoryRoot = resolve(import.meta.dirname, "../../..")
const targetDir = process.env.CARGO_TARGET_DIR ?? join(repositoryRoot, "target")
export const sessionRoot = join(repositoryRoot, "conformance", "session")
export const hubFixtureRoot = join(repositoryRoot, "crates", "lute-runtime-wasm", "tests", "fixtures", "hub-once")

let cliBuilt = false

const command = (args: ReadonlyArray<string>, cwd: string) => {
  const result = Bun.spawnSync([...args], {
    cwd,
    stdout: "pipe",
    stderr: "pipe"
  })
  if (result.exitCode !== 0) {
    throw new Error(`${args.join(" ")} failed (${result.exitCode}): ${result.stderr.toString()}`)
  }
  return result.stdout.toString()
}

export const compileBundle = (projectDir: string): Effect.Effect<string> => Effect.sync(() => {
  if (!cliBuilt) {
    command(["cargo", "build", "-q", "-p", "lute-cli"], repositoryRoot)
    cliBuilt = true
  }
  const output = mkdtempSync(join(tmpdir(), "lute-runtime-bundle-"))
  command([join(targetDir, "debug", "lute"), "compile", "--all", projectDir, "-o", output], repositoryRoot)
  return output
})

export const canonical = (value: unknown): string => {
  const sort = (item: unknown): unknown => {
    if (Array.isArray(item)) return item.map(sort)
    if (item !== null && typeof item === "object") {
      return Object.fromEntries(
        Object.entries(item)
          .sort(([left], [right]) => (left < right ? -1 : left > right ? 1 : 0))
          .map(([key, child]) => [key, sort(child)])
      )
    }
    return item
  }
  return JSON.stringify(sort(value))
}

export interface SessionCase {
  readonly name: string
  readonly project: string
  readonly script: string
  readonly expected: string
  readonly inputs: string
}

export const sessionCases = (): ReadonlyArray<SessionCase> => readdirSync(sessionRoot, { withFileTypes: true })
  .filter((entry) => entry.isDirectory())
  .map(({ name }) => ({
    name,
    project: join(sessionRoot, name, "project"),
    script: join(sessionRoot, name, "project", "script.play.yaml"),
    expected: join(sessionRoot, name, "expected.jsonl"),
    inputs: join(sessionRoot, name, "inputs.jsonl")
  }))
  .filter(({ script }) => {
    try {
      readFileSync(script)
      return true
    } catch {
      return false
    }
  })
  .sort((left, right) => (left.name < right.name ? -1 : left.name > right.name ? 1 : 0))

export const jsonLines = (path: string): ReadonlyArray<unknown> => readFileSync(path, "utf8")
  .split("\n")
  .filter((line) => line.trim().length > 0)
  .map((line) => {
    const value: unknown = JSON.parse(line)
    return value
  })

export const decode = <S extends Schema.Codec<unknown, unknown, never, never>>(schema: S) =>
  (value: unknown): S["Type"] => Schema.decodeUnknownSync(schema)(value)
