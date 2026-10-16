/**
 * Compiles Lute projects with the real `lute` binary for the player-web
 * suites (spec 0.47.0 §6): node tests drive the output through
 * `@lute-lang/runtime/bun`, the browser suite mounts player-web over it.
 */
import { mkdirSync, rmSync } from "node:fs"
import { join, resolve } from "node:path"

export const packageRoot = resolve(import.meta.dirname, "../..")
export const repositoryRoot = resolve(packageRoot, "../..")
/** The multi-ending demo project (`run.path`: `dawn` / `dusk`). */
export const demoProject = join(repositoryRoot, "examples/player-demo")
/** Compiled test bundles live under the package's (ignored) `node_modules`. */
export const fixturesRoot = join(packageRoot, "node_modules/.lute-test")
/** The packaged runtime binding the browser suite loads. */
export const wasmFile = join(repositoryRoot, "packages/runtime/src/wasm/lute_runtime_wasm_bg.wasm")

const run = (args: ReadonlyArray<string>): void => {
  // Tests never write the developer's telemetry (dsl 0.43.0 §6).
  const result = Bun.spawnSync([...args], { cwd: repositoryRoot, env: { ...process.env, LUTE_TELEMETRY: "0" }, stdout: "pipe", stderr: "pipe" })
  if (result.exitCode !== 0) throw new Error(`${args.join(" ")} failed (${result.exitCode}): ${result.stderr.toString()}`)
}

let binary: string | undefined

/** `LUTE_BIN`, else the workspace debug build (cargo is incremental: an up-to-date build is a no-op). */
export const luteBinary = (): string => {
  if (binary !== undefined) return binary
  const fromEnv = process.env.LUTE_BIN
  if (fromEnv !== undefined && fromEnv !== "") return binary = fromEnv
  run(["cargo", "build", "-q", "-p", "lute-cli"])
  return binary = join(process.env.CARGO_TARGET_DIR ?? join(repositoryRoot, "target"), "debug", "lute")
}

/** `lute compile --all <project>` into `fixturesRoot/<name>`; returns the bundle directory. */
export const compileProject = (name: string, project: string): string => {
  const output = join(fixturesRoot, name)
  rmSync(output, { recursive: true, force: true })
  mkdirSync(fixturesRoot, { recursive: true })
  run([luteBinary(), "compile", "--all", project, "-o", output])
  return output
}
