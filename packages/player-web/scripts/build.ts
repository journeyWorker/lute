import { cpSync, existsSync, rmSync } from "node:fs"
import { join, resolve } from "node:path"

const pkg = resolve(import.meta.dir, "..")
const dist = join(pkg, "dist")
rmSync(dist, { recursive: true, force: true })
const result = await Bun.build({
  entrypoints: [join(pkg, "src/index.ts")],
  outdir: dist,
  format: "esm",
  target: "browser",
  external: ["effect", "@effect/*", "pixi.js", "@pixi/sound", "@lute-lang/*"]
})
if (!result.success) {
  for (const log of result.logs) console.error(log)
  process.exit(1)
}
if (!existsSync(join(dist, "index.js"))) process.exit(1)
const declarations = Bun.spawnSync(["tsc", "-p", join(pkg, "tsconfig.build.json")], { cwd: pkg, stdout: "inherit", stderr: "inherit" })
if (declarations.exitCode !== 0) process.exit(declarations.exitCode ?? 1)
const declarationRoot = join(dist, "player-web", "src")
if (existsSync(declarationRoot)) {
  cpSync(declarationRoot, dist, { recursive: true })
  rmSync(join(dist, "player-web"), { recursive: true, force: true })
}
rmSync(join(dist, "player"), { recursive: true, force: true })
rmSync(join(dist, "runtime"), { recursive: true, force: true })
