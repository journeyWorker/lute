import { existsSync, rmSync } from "node:fs"
import { join, resolve } from "node:path"

const pkg = resolve(import.meta.dir, "..")
const dist = join(pkg, "dist")
rmSync(dist, { recursive: true, force: true })
const result = await Bun.build({
  entrypoints: [join(pkg, "src/index.ts")],
  outdir: dist,
  format: "esm",
  target: "browser",
  external: ["effect", "@effect/*", "@lute-lang/runtime", "xstate", "@xstate/effect", "yaml"]
})
if (!result.success) {
  for (const log of result.logs) console.error(log)
  process.exit(1)
}
const tsc = Bun.spawnSync(["tsc", "-p", "tsconfig.build.json"], { cwd: pkg, stdout: "inherit", stderr: "inherit" })
if (tsc.exitCode !== 0) process.exit(tsc.exitCode ?? 1)
if (!existsSync(join(dist, "index.js"))) {
  console.error("player build did not produce dist/index.js")
  process.exit(1)
}
