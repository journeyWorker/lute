// Compiles the real fixtures the browser suite mounts player-web over
// (spec 0.47.0 §6) and provides their URLs; vite serves them through `/@fs/`.
import { join } from "node:path"
import type { TestProject } from "vitest/node"
import { compileProject, demoLocales, demoProject, packageRoot, wasmFile } from "../support/bundle.js"

/** URLs of a compiled project, as `mount` takes them. */
export interface RealProjectUrls {
  readonly bundleUrl: string
  readonly bindingsUrl: string
  readonly assetsUrl: string
  readonly wasmUrl: string
}

/**
 * Projects the suite compiles: `demo` is examples/player-demo (with its
 * Korean locale bundle), the rest live in test/fixtures.
 */
export const realProjects = {
  demo: { dir: demoProject, locales: demoLocales },
  "cg-stage": { dir: join(packageRoot, "test/fixtures/cg-stage") },
  "camera-stage": { dir: join(packageRoot, "test/fixtures/camera-stage") },
  "title-text": { dir: join(packageRoot, "test/fixtures/title-text") }
} as const satisfies Record<string, { readonly dir: string; readonly locales?: string }>

declare module "vitest" {
  export interface ProvidedContext {
    readonly luteProjects: Record<keyof typeof realProjects, RealProjectUrls>
  }
}

const fsUrl = (path: string): string => `/@fs${path}`
// Every fixture draws from the demo's generated assets.
const assets = `${fsUrl(join(demoProject, "assets"))}/`

export default function setup(project: TestProject): void {
  const urls = Object.fromEntries(Object.entries(realProjects).map(([name, entry]) => [name, {
    bundleUrl: `${fsUrl(compileProject(`browser-${name}`, entry.dir, "locales" in entry ? entry.locales : undefined))}/`,
    bindingsUrl: fsUrl(join(entry.dir, "player.bindings.yaml")),
    assetsUrl: assets,
    wasmUrl: fsUrl(wasmFile)
  }])) as Record<keyof typeof realProjects, RealProjectUrls>
  project.provide("luteProjects", urls)
}
