// Build the runtime binding (and its test-panic variant) before the suite:
// cargo is incremental, so an up-to-date build is a no-op.
import { resolve } from "node:path"

const run = (args: ReadonlyArray<string>, cwd: string) => {
  const result = Bun.spawnSync([...args], { cwd, stdout: "inherit", stderr: "inherit" })
  if (result.exitCode !== 0) throw new Error(`${args.join(" ")} failed with ${result.exitCode}`)
}

export default function setup() {
  const runtime = resolve(import.meta.dirname, "..")
  run(["bun", "run", "wasm:build"], runtime)
  run(["bun", "run", "wasm:build:test-panic"], runtime)
}
