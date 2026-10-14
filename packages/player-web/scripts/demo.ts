import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync } from "node:fs"
import { join, resolve } from "node:path"
import { tmpdir } from "node:os"

const repo = resolve(import.meta.dir, "../../..")
const project = join(repo, "examples/player-demo")
const web = join(project, "web")
const output = mkdtempSync(join(tmpdir(), "lute-player-demo-"))
const bundle = join(output, "bundle")
const wasm = join(output, "wasm")
mkdirSync(wasm, { recursive: true })
const assets = join(project, "assets")
const binCandidates = [
  process.env.LUTE_BIN,
  join(repo, "target/release/lute"),
  join(repo, "target/debug/lute"),
  "lute"
].filter((value): value is string => value !== undefined)
const lute = binCandidates.find((candidate) => candidate === "lute" || existsSync(candidate))
if (lute === undefined) throw new Error("cannot find lute; set LUTE_BIN or build target/{release,debug}/lute")

// The demo build never writes the developer's telemetry (dsl 0.43.0 §6).
const compile = Bun.spawnSync([lute, "compile", "--all", project, "-o", bundle], { cwd: repo, env: { ...process.env, LUTE_TELEMETRY: "0" }, stdout: "inherit", stderr: "inherit" })
if (compile.exitCode !== 0) process.exit(compile.exitCode ?? 1)

const built = await Bun.build({
  entrypoints: [join(web, "main.ts")],
  outdir: output,
  naming: "main.js",
  target: "browser",
  minify: false,
  plugins: [{
    name: "workspace-aliases",
    setup(build) {
      build.onResolve({ filter: /^@lute-lang\/player$/ }, () => ({ path: join(repo, "packages/player/src/index.ts") }))
      build.onResolve({ filter: /^@lute-lang\/runtime$/ }, () => ({ path: join(repo, "packages/runtime/src/index.ts") }))
      build.onResolve({ filter: /^@lute-lang\/runtime\/browser$/ }, () => ({ path: join(repo, "packages/runtime/src/browser.ts") }))
    }
  }]
})
if (!built.success) {
  for (const log of built.logs) console.error(log)
  process.exit(1)
}
cpSync(join(repo, "packages/runtime/src/wasm/lute_runtime_wasm_bg.wasm"), join(output, "wasm", "lute_runtime_wasm_bg.wasm"), { recursive: true })

const port = Number.parseInt(process.env.PORT ?? "4173", 10)
const contentType = (path: string): string => path.endsWith(".html") ? "text/html; charset=utf-8" : path.endsWith(".js") ? "text/javascript; charset=utf-8" : path.endsWith(".json") ? "application/json" : path.endsWith(".yaml") ? "text/yaml; charset=utf-8" : path.endsWith(".png") ? "image/png" : path.endsWith(".wav") ? "audio/wav" : "application/octet-stream"
const file = (path: string): Response => {
  if (!existsSync(path)) return new Response("Not found", { status: 404 })
  return new Response(readFileSync(path), { headers: { "content-type": contentType(path), "cache-control": "no-cache" } })
}
const server = Bun.serve({
  port,
  fetch(request) {
    const url = new URL(request.url)
    if (url.pathname === "/" || url.pathname === "/index.html") return file(join(web, "index.html"))
    if (url.pathname === "/main.js") return file(join(output, "main.js"))
    if (url.pathname === "/player.bindings.yaml") return file(join(project, "player.bindings.yaml"))
    if (url.pathname.startsWith("/bundle/")) return file(join(bundle, decodeURIComponent(url.pathname.slice("/bundle/".length))))
    if (url.pathname.startsWith("/wasm/")) return file(join(wasm, decodeURIComponent(url.pathname.slice("/wasm/".length))))
    if (url.pathname.startsWith("/assets/")) return file(join(assets, decodeURIComponent(url.pathname.slice("/assets/".length))))
    return new Response("Not found", { status: 404 })
  }
})
console.log(`Lute player demo: ${server.url}`)
