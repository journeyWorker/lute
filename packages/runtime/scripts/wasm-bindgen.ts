// Locate the pinned `wasm-bindgen` for `wasm-build.ts` (spec 0.41.0 §6.4):
// `$WASM_BINDGEN`, then `wasm-bindgen` on PATH, then the wasm-pack cache
// entry for the pinned version. Nothing here exits or prints; the build
// script reports a failed resolution with `HINT`.
import { join } from "node:path"

export const WASM_BINDGEN_VERSION = "0.2.126"

export const HINT =
  `wasm-build: need wasm-bindgen ${WASM_BINDGEN_VERSION} (set WASM_BINDGEN, put it on PATH, ` +
  `or cargo install --locked wasm-bindgen-cli --version ${WASM_BINDGEN_VERSION})`

/** What the resolver reads of the machine it runs on. */
export interface Host {
  readonly env: Readonly<Record<string, string | undefined>>
  readonly platform: NodeJS.Platform
  readonly home: string
  /** The executable `command` names on PATH, or `null`. */
  readonly which: (command: string) => string | null
  readonly exists: (path: string) => boolean
}

/**
 * The wasm-pack cache entry of the pinned version: under `~/Library/Caches`
 * on macOS, `%LOCALAPPDATA%` on Windows, `$XDG_CACHE_HOME` or `~/.cache`
 * elsewhere. `undefined` on Windows without `LOCALAPPDATA`.
 */
export const cacheEntry = (host: Host): string | undefined => {
  const cache =
    host.platform === "darwin"
      ? join(host.home, "Library", "Caches")
      : host.platform === "win32"
        ? host.env.LOCALAPPDATA
        : host.env.XDG_CACHE_HOME || join(host.home, ".cache")
  if (cache === undefined || cache === "") return undefined
  const binary = host.platform === "win32" ? "wasm-bindgen.exe" : "wasm-bindgen"
  return join(cache, ".wasm-pack", `wasm-bindgen-cargo-install-${WASM_BINDGEN_VERSION}`, binary)
}

/** The `wasm-bindgen` to run, in resolution order; `undefined` when none is found. */
export const resolveWasmBindgen = (host: Host): string | undefined => {
  const configured = host.env.WASM_BINDGEN
  if (configured !== undefined && configured !== "") return configured
  const onPath = host.which("wasm-bindgen")
  if (onPath !== null) return onPath
  const cached = cacheEntry(host)
  return cached !== undefined && host.exists(cached) ? cached : undefined
}

/**
 * What `<command> --version` prints, or `undefined` when it cannot be
 * spawned (`ENOENT`, not executable) or exits non-zero.
 */
export const reportedVersion = (command: string): string | undefined => {
  try {
    const result = Bun.spawnSync([command, "--version"], { stdout: "pipe", stderr: "pipe" })
    return result.exitCode === 0 ? result.stdout.toString().trim() : undefined
  } catch {
    return undefined
  }
}

/** The resolved binary of the pinned version, or the message to print. */
export type Resolution = { readonly bindgen: string } | { readonly error: string }

export const locate = (host: Host, version: (command: string) => string | undefined = reportedVersion): Resolution => {
  const bindgen = resolveWasmBindgen(host)
  if (bindgen === undefined) return { error: HINT }
  const reported = version(bindgen)
  if (reported === `wasm-bindgen ${WASM_BINDGEN_VERSION}`) return { bindgen }
  return { error: reported === undefined ? HINT : `${HINT}; \`${bindgen}\` reports ${reported}` }
}
