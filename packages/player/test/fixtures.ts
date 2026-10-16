/**
 * Compiled fixture projects for the player suites (spec 0.47.0 §6: tested
 * against real runtime output).
 */
import { mkdtempSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import { Effect } from "effect"
import { compileBundle, repositoryRoot } from "../../runtime/test/support.js"

const fixtures = join(import.meta.dirname, "fixtures")

/**
 * `quay-locale`: one scene (a `prev.run`-gated line, a named line, a branch,
 * a `user.runs` counter, then the terminal `run.over`), source locale `en-US`
 * (the quest document's `contentLang`) and a Korean export (`ko.json`).
 */
export const quayProject = join(fixtures, "quay-locale", "project")
const quayKorean = join(fixtures, "quay-locale", "ko.json")

const luteBinary = join(process.env.CARGO_TARGET_DIR ?? join(repositoryRoot, "target"), "debug", "lute")

const lute = (args: ReadonlyArray<string>): void => {
  const result = Bun.spawnSync([luteBinary, ...args], {
    cwd: repositoryRoot,
    env: { ...process.env, LUTE_TELEMETRY: "0" },
    stdout: "pipe",
    stderr: "pipe"
  })
  if (result.exitCode !== 0) throw new Error(`lute ${args.join(" ")} failed (${result.exitCode}): ${result.stderr.toString()}`)
}

let korean: string | undefined

/**
 * `quay-locale` compiled with its Korean translation: `lute loc import ko.json`
 * then `lute compile --all <project> --locales <bundle>`, with the binary
 * `compileBundle` builds.
 */
export const compileQuayKorean: Effect.Effect<string> = Effect.gen(function* () {
  if (korean !== undefined) return korean
  // Builds the CLI once (and checks the project compiles without locales).
  yield* compileBundle(quayProject)
  const work = mkdtempSync(join(tmpdir(), "lute-player-locale-"))
  const localeBundle = join(work, "locales.json")
  const output = join(work, "bundle")
  lute(["loc", "import", quayKorean, "-o", localeBundle])
  lute(["compile", "--all", quayProject, "--locales", localeBundle, "-o", output])
  korean = output
  return output
})
