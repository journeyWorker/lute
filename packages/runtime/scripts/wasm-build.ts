// Build the runtime binding (`crates/lute-runtime-wasm`) and its JS glue.
//
//   bun scripts/wasm-build.ts                 debug build    -> src/wasm/
//   bun scripts/wasm-build.ts --release       release build  -> src/wasm/ (wasm-opt -O)
//   bun scripts/wasm-build.ts --test-panic    debug build with the `test-panic` export -> test/wasm-panic/
//
// Tools: `wasm-bindgen` (exactly the crate's wasm-bindgen version) from
// $WASM_BINDGEN or PATH; for --release, `wasm-opt` from $WASM_OPT or PATH.
// Panics abort (spec 0.39.0 §4): the binding is built with
// RUSTFLAGS="-C panic=abort"; the workspace profiles are untouched.
import { mkdirSync, renameSync, rmSync } from "node:fs"
import { join, resolve } from "node:path"

const WASM_BINDGEN_VERSION = "0.2.126"

const repo = resolve(import.meta.dir, "../../..")
const target = process.env.CARGO_TARGET_DIR ?? join(repo, "target")
const flags = new Set(Bun.argv.slice(2))
const release = flags.has("--release")
const testPanic = flags.has("--test-panic")
const outDir = join(import.meta.dir, "..", testPanic ? "test/wasm-panic" : "src/wasm")

const run = (command: ReadonlyArray<string>, env: Record<string, string | undefined> = process.env) => {
  const result = Bun.spawnSync([...command], { cwd: repo, env, stdout: "inherit", stderr: "inherit" })
  if (result.exitCode !== 0) {
    console.error(`wasm-build: \`${command.join(" ")}\` exited ${result.exitCode}`)
    process.exit(result.exitCode ?? 1)
  }
}

const bindgen = process.env.WASM_BINDGEN ?? "wasm-bindgen"
const version = Bun.spawnSync([bindgen, "--version"], { stdout: "pipe", stderr: "pipe" })
const reported = version.stdout.toString().trim()
if (version.exitCode !== 0 || reported !== `wasm-bindgen ${WASM_BINDGEN_VERSION}`) {
  console.error(`wasm-build: need wasm-bindgen ${WASM_BINDGEN_VERSION} (found: ${reported || "none"}); ` +
    `cargo install --locked wasm-bindgen-cli --version ${WASM_BINDGEN_VERSION}`)
  process.exit(1)
}

run(
  [
    "cargo", "build", "-p", "lute-runtime-wasm", "--target", "wasm32-unknown-unknown",
    ...(release ? ["--release"] : []),
    ...(testPanic ? ["--features", "test-panic"] : [])
  ],
  { ...process.env, RUSTFLAGS: "-C panic=abort", CARGO_TARGET_DIR: target }
)

rmSync(outDir, { recursive: true, force: true })
mkdirSync(outDir, { recursive: true })
const wasm = join(target, "wasm32-unknown-unknown", release ? "release" : "debug", "lute_runtime_wasm.wasm")
run([bindgen, wasm, "--target", "web", "--out-dir", outDir])

if (release) {
  const binary = join(outDir, "lute_runtime_wasm_bg.wasm")
  run([process.env.WASM_OPT ?? "wasm-opt", "-O", binary, "-o", `${binary}.opt`])
  renameSync(`${binary}.opt`, binary)
}
