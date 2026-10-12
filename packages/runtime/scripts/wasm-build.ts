// Build the runtime binding (`crates/lute-runtime-wasm`) and its JS glue.
//
//   bun scripts/wasm-build.ts                 debug build    -> src/wasm/
//   bun scripts/wasm-build.ts --release       release build  -> src/wasm/ (wasm-opt -O)
//   bun scripts/wasm-build.ts --test-panic    debug build with the `test-panic` export -> test/wasm-panic/
//
// Tools: `wasm-bindgen` (exactly the crate's wasm-bindgen version) from
// $WASM_BINDGEN, PATH, or the wasm-pack cache (`scripts/wasm-bindgen.ts`);
// for --release, `wasm-opt` from $WASM_OPT or PATH. A tool that cannot be
// spawned is reported in one line, never a stack.
// Panics abort (spec 0.40.0 §4): the binding is built with
// RUSTFLAGS="-C panic=abort"; the workspace profiles are untouched.
import { existsSync, mkdirSync, renameSync, rmSync } from "node:fs"
import { homedir } from "node:os"
import { join, resolve } from "node:path"
import { locate } from "./wasm-bindgen.js"

const repo = resolve(import.meta.dir, "../../..")
const target = process.env.CARGO_TARGET_DIR ?? join(repo, "target")
const flags = new Set(Bun.argv.slice(2))
const release = flags.has("--release")
const testPanic = flags.has("--test-panic")
const outDir = join(import.meta.dir, "..", testPanic ? "test/wasm-panic" : "src/wasm")

const fail = (message: string): never => {
  console.error(message)
  process.exit(1)
}

const run = (
  command: ReadonlyArray<string>,
  hint: string,
  env: Record<string, string | undefined> = process.env
) => {
  let exitCode: number | null
  try {
    exitCode = Bun.spawnSync([...command], { cwd: repo, env, stdout: "inherit", stderr: "inherit" }).exitCode
  } catch (error) {
    return fail(`wasm-build: cannot run \`${command[0]}\` (${error instanceof Error ? error.message : String(error)}); ${hint}`)
  }
  if (exitCode !== 0) {
    console.error(`wasm-build: \`${command.join(" ")}\` exited ${exitCode}`)
    process.exit(exitCode ?? 1)
  }
}

const resolution = locate({
  env: process.env,
  platform: process.platform,
  home: homedir(),
  which: (command) => Bun.which(command),
  exists: existsSync
})
const bindgen = "bindgen" in resolution ? resolution.bindgen : fail(resolution.error)

run(
  [
    "cargo", "build", "-p", "lute-runtime-wasm", "--target", "wasm32-unknown-unknown",
    ...(release ? ["--release"] : []),
    ...(testPanic ? ["--features", "test-panic"] : [])
  ],
  "install Rust with the wasm32-unknown-unknown target (rustup target add wasm32-unknown-unknown)",
  { ...process.env, RUSTFLAGS: "-C panic=abort", CARGO_TARGET_DIR: target }
)

rmSync(outDir, { recursive: true, force: true })
mkdirSync(outDir, { recursive: true })
const wasm = join(target, "wasm32-unknown-unknown", release ? "release" : "debug", "lute_runtime_wasm.wasm")
run([bindgen, wasm, "--target", "web", "--out-dir", outDir], "set WASM_BINDGEN")

if (release) {
  const binary = join(outDir, "lute_runtime_wasm_bg.wasm")
  run(
    [process.env.WASM_OPT ?? "wasm-opt", "-O", binary, "-o", `${binary}.opt`],
    "set WASM_OPT or put binaryen's wasm-opt on PATH"
  )
  renameSync(`${binary}.opt`, binary)
}
