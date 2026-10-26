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
  if (result.exitCode !== 0) throw new Error(`lute ${args.join(" ")} failed (${result.exitCode}): ${result.stdout.toString()}${result.stderr.toString()}`)
}

const compiled = new Map<string, string>()

/**
 * `project` compiled with `translations`: `lute loc import <translations>`
 * then `lute compile --project <project> --locale-bundle <bundle> -o <dir>`, with the binary
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
  lute(["compile", "--project", project, "--locale-bundle", localeBundle, "-o", output])
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

/**
 * `tally-locale`: one scene that sets `run.a = 3` and `run.b = 5`, then two
 * narrator lines with `{{run.a}}` / `{{run.b}}` — the second with a `:speed`
 * modifier — whose Korean translations (`ko.json`) put the markers in the
 * other order; source locale `en-US`. `visit` starts it.
 */
export const compileTallyKorean: Effect.Effect<string> = compileTranslated(join(fixtures, "tally-locale", "project"), [join(fixtures, "tally-locale", "ko.json")])

/**
 * `escape-locale`: one scene that sets `run.a = 3`, then two narrator lines
 * with an escaped `\{{run.a}}` beside a real `{{run.a}}` marker — the second
 * inside a `:speed` run — whose Korean translations (`ko.json`) keep a
 * literal `\{{run.a}}` too (lute 0.47.1 §3); source locale `en-US`. `visit`
 * starts it.
 */
export const compileEscapeKorean: Effect.Effect<string> = compileTranslated(join(fixtures, "escape-locale", "project"), [join(fixtures, "escape-locale", "ko.json")])

/**
 * `toll-locale`: one scene that sets `run.gold = 5`, then the branch `gate`
 * (prompt "The toll is {{run.gold}} gold.", options `pay` "Pay {{run.gold}}
 * gold" and `leave` "Walk away"), whose Korean translations (`ko.json`) cover
 * the prompt and `pay` but not `leave` (lute 0.52.0 §6.4); source locale
 * `en`. `visit` starts it.
 */
export const compileTollKorean: Effect.Effect<string> = compileTranslated(join(fixtures, "toll-locale", "project"), [join(fixtures, "toll-locale", "ko.json")])

/**
 * `flow-target-0471` (shared with the CLI's whole-project `check` test): `begin`
 * starts the run and leaves it idle; `player.bindings.yaml`'s idle menu
 * raises `visit` for `place.square` or `place.pier`, each the target of one
 * beat (lute 0.47.1 §2).
 */
export const flowTargetProject = join(repositoryRoot, "crates", "lute-cli", "tests", "fixtures", "flow-target-0471")

/** `lute check --project <project>` passes (call after `compileBundle`, which builds the CLI). */
export const checkProject = (project: string): Effect.Effect<void> => Effect.sync(() => lute(["check", "--project", project]))

/**
 * `chime-batch`: `gate` (occasion `visit`) shows a line, then `::camera` and
 * `::sfx{sound="chime"}` as the last records before a `<branch>`'s choice;
 * `porch` (occasion `begin`) shows a line, then a `::sfx` as the last record
 * before the run goes idle (lute 0.47.2 §2).
 */
export const compileChimeBatch: Effect.Effect<string> = compileBundle(join(fixtures, "chime-batch", "project"))

/**
 * `line-wait`: `dusk` (occasion `visit`) shows a `wait="false"` line, a
 * `::bg` (night), a waiting line, a second `::bg` (dawn), then sets the
 * terminal `run.over`. `porch` (occasion `begin`, `advances: slot`) shows a
 * `wait="false"` line and a `::bg`; its clock advance raises `tick`, whose
 * `dawn` presentation follows in the same batch with one line, then sets
 * `run.over` (lute 0.49.0 §4.2).
 */
export const compileLineWait: Effect.Effect<string> = compileBundle(join(fixtures, "line-wait", "project"))

/**
 * `choice-stamps`: `crossing` (occasion `visit`) offers a `<branch>` stamped
 * `menuType="timed" cue="b1"` whose `left` choice is stamped `cue="c1"
 * weight=2` and whose `right` choice has no stamps, then an unstamped `<hub>`,
 * then sets the terminal `run.over`; `acme.menus` scopes the stamps to the
 * logic elements (lute 0.50.0 §3.1, §3.6).
 */
export const compileChoiceStamps: Effect.Effect<string> = compileBundle(join(fixtures, "choice-stamps", "project"))

/**
 * `stamp-keys`: `dock` (occasion `visit`) shows a line at `001-0100`, a line
 * stamped `kind="note" position="001-0100"`, a `::bg` stamped `kind="note"
 * position="001-0300"` (its own position), then sets the terminal
 * `run.over`; `acme.keys` declares the cross-cutting stamps `kind` and
 * `position` (#200).
 */
export const compileStampKeys: Effect.Effect<string> = compileBundle(join(fixtures, "stamp-keys", "project"))

/**
 * `raise-queue` (0.51.0 §4.6, §5.6): `porch` (occasion `visit`) shows a line,
 * then the branch `door`: `in` calls `::playScene` for `hall` (with the
 * `text` attributes `title`, `texts` and `promptTexts`) and for `yard`;
 * `vault` calls `::openVault` (a raise of `vault`, closed by its
 * `raisedWhen`) then `::playScene` for `yard`; `fanfare` sets `user.fanfare`,
 * which starts the run-tier quest `fanfare`, whose `CUTSCENE` reward raises
 * `gallery` (again at every `newRun`); `ask` calls `::playScene` for `yard`,
 * the bridge `::ask` (which raises `cellar`), then the branch `then`. Each of
 * `hall`, `yard`, `cellar`, `gallery` and `vault` answers its raise and
 * counts its plays (`run.halls` … `user.galleries`). `ko.json` translates the
 * three `text` attributes only; source locale `en`.
 */
export const compileRaiseQueue: Effect.Effect<string> = compileTranslated(join(fixtures, "raise-queue", "project"), [join(fixtures, "raise-queue", "ko.json")])
