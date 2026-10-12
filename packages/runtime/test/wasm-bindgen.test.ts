import { mkdtempSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { describe, expect, it } from "@effect/vitest"
import {
  cacheEntry,
  HINT,
  locate,
  reportedVersion,
  resolveWasmBindgen,
  WASM_BINDGEN_VERSION,
  type Host
} from "../scripts/wasm-bindgen.js"

const pinned = `wasm-bindgen ${WASM_BINDGEN_VERSION}`
const cached = `/home/u/.cache/.wasm-pack/wasm-bindgen-cargo-install-${WASM_BINDGEN_VERSION}/wasm-bindgen`

const host = (overrides: Partial<Host> = {}): Host => ({
  env: {},
  platform: "linux",
  home: "/home/u",
  which: () => null,
  exists: () => false,
  ...overrides
})

describe("wasm-build: the wasm-bindgen resolver (spec 0.41.0 §6.4)", () => {
  it("prefers $WASM_BINDGEN, then PATH, then the wasm-pack cache", () => {
    const everywhere = host({
      env: { WASM_BINDGEN: "/opt/wb" },
      which: () => "/usr/local/bin/wasm-bindgen",
      exists: (path) => path === cached
    })
    expect(resolveWasmBindgen(everywhere)).toBe("/opt/wb")
    expect(resolveWasmBindgen({ ...everywhere, env: {} })).toBe("/usr/local/bin/wasm-bindgen")
    expect(resolveWasmBindgen({ ...everywhere, env: {}, which: () => null })).toBe(cached)
    expect(resolveWasmBindgen(host())).toBeUndefined()
  })

  it("probes the platform's cache directory", () => {
    const entry = `.wasm-pack/wasm-bindgen-cargo-install-${WASM_BINDGEN_VERSION}`
    expect(cacheEntry(host({ platform: "darwin", home: "/Users/u" })))
      .toBe(`/Users/u/Library/Caches/${entry}/wasm-bindgen`)
    expect(cacheEntry(host({ env: { XDG_CACHE_HOME: "/xdg" } }))).toBe(`/xdg/${entry}/wasm-bindgen`)
    expect(cacheEntry(host())).toBe(`/home/u/.cache/${entry}/wasm-bindgen`)
    expect(cacheEntry(host({ platform: "win32", env: { LOCALAPPDATA: "/local" } })))
      .toBe(`/local/${entry}/wasm-bindgen.exe`)
    expect(cacheEntry(host({ platform: "win32" }))).toBeUndefined()
  })

  it("catches a spawn failure instead of throwing", () => {
    const missing = join(mkdtempSync(join(tmpdir(), "lute-wasm-bindgen-")), "wasm-bindgen")
    expect(reportedVersion(missing)).toBeUndefined()
  })

  it("answers the hint when nothing resolves or the binary cannot run", () => {
    expect(locate(host(), () => pinned)).toEqual({ error: HINT })
    const missing = join(mkdtempSync(join(tmpdir(), "lute-wasm-bindgen-")), "wasm-bindgen")
    expect(locate(host({ env: { WASM_BINDGEN: missing } }))).toEqual({ error: HINT })
    expect(HINT).toBe(
      "wasm-build: need wasm-bindgen 0.2.126 (set WASM_BINDGEN, put it on PATH, " +
        "or cargo install --locked wasm-bindgen-cli --version 0.2.126)"
    )
  })

  it("accepts only the pinned version", () => {
    const configured = host({ env: { WASM_BINDGEN: "/opt/wb" } })
    expect(locate(configured, () => pinned)).toEqual({ bindgen: "/opt/wb" })
    expect(locate(configured, () => "wasm-bindgen 0.2.100")).toEqual({
      error: `${HINT}; \`/opt/wb\` reports wasm-bindgen 0.2.100`
    })
  })
})
