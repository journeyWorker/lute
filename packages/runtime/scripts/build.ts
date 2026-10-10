// Build the publishable package into dist/: the three entry points as ESM
// (Effect packages external), their declarations, and the binding built by
// `wasm:build` / `wasm:build:release`.
import { cpSync, existsSync, rmSync } from "node:fs"
import { join, resolve } from "node:path"

const pkg = resolve(import.meta.dir, "..")
const dist = join(pkg, "dist")

if (!existsSync(join(pkg, "src/wasm/lute_runtime_wasm_bg.wasm"))) {
  console.error("build: src/wasm/ is empty — run `bun run wasm:build:release` first")
  process.exit(1)
}

const run = (command: ReadonlyArray<string>) => {
  const result = Bun.spawnSync([...command], { cwd: pkg, stdout: "inherit", stderr: "inherit" })
  if (result.exitCode !== 0) process.exit(result.exitCode ?? 1)
}

rmSync(dist, { recursive: true, force: true })
run([
  "bun", "build", "src/index.ts", "src/bun.ts", "src/browser.ts",
  "--outdir", "dist", "--format", "esm", "--target", "browser", "--splitting",
  "--external", "effect", "--external", "@effect/*", "--external", "./wasm/*"
])
run(["tsc", "-p", "tsconfig.build.json"])
cpSync(join(pkg, "src/wasm"), join(dist, "wasm"), { recursive: true })
