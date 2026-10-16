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
 * `quay-locale`: one scene (a `prev.run`-gated line, a named line, a
 * `user.runs` counter, a prompted branch, then the terminal `run.over`),
 * source locale `en-US` (the manifest's `defaults.contentLang`, over the quest
 * document's `en-GB`), a Korean export (`ko.json`) and a Japanese one that
 * translates only the branch prompt (`ja.json`).
 */
export const quayProject = join(fixtures, "quay-locale", "project")
const quayTranslations = [join(fixtures, "quay-locale", "ko.json"), join(fixtures, "quay-locale", "ja.json")]

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

const compiled = new Map<string, string>()

/**
 * `project` compiled with `translations`: `lute loc import <translations>`
 * then `lute compile --all <project> --locales <bundle>`, with the binary
 * `compileBundle` builds (once per project).
 */
const compileTranslated = (project: string, translations: ReadonlyArray<string>): Effect.Effect<string> => Effect.gen(function* () {
  const cached = compiled.get(project)
  if (cached !== undefined) return cached
  // Builds the CLI once (and checks the project compiles without locales).
  yield* compileBundle(project)
  const work = mkdtempSync(join(tmpdir(), "lute-player-locale-"))
  const localeBundle = join(work, "locales.json")
  const output = join(work, "bundle")
  lute(["loc", "import", ...translations, "-o", localeBundle])
  lute(["compile", "--all", project, "--locales", localeBundle, "-o", output])
  compiled.set(project, output)
  return output
})

/** `quay-locale` compiled with `ko.json` and `ja.json`. */
export const compileQuayKorean: Effect.Effect<string> = compileTranslated(quayProject, quayTranslations)

/**
 * `hub-locale`: one scene whose prompted hub (`stalls`, "Which stall?") is
 * translated to Korean (`ko.json`, from `lute loc export`), source locale `en`.
 */
export const compileHubKorean: Effect.Effect<string> = compileTranslated(join(fixtures, "hub-locale", "project"), [join(fixtures, "hub-locale", "ko.json")])
