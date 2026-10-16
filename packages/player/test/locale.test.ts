/** Spec 0.47.0 §2.2: translated text follows the locale setting, over a compiled Korean translation. */
import { Effect, SubscriptionRef } from "effect"
import * as KeyValueStore from "effect/persistence/KeyValueStore"
import { describe, expect, it } from "@effect/vitest"
import { compileQuayKorean } from "./fixtures.js"
import { flush } from "./director/support.js"
import { act, hostLayer, player, recorder, textOf } from "./player.js"
import type { ScreenModel } from "../src/ports/index.js"

const lastScreen = <T extends ScreenModel["_tag"]>(screens: ReadonlyArray<ScreenModel>, tag: T) =>
  [...screens].reverse().find((screen): screen is Extract<ScreenModel, { readonly _tag: T }> => screen._tag === tag)

/** Run `use` over the Korean-compiled fixture with one key-value store for every player it builds. */
const overKorean = <A, E, R>(use: Effect.Effect<A, E, R>) =>
  Effect.flatMap(compileQuayKorean, (bundle) => Effect.provide(use, hostLayer(bundle)))

describe("translated text follows the locale setting (§2.2)", () => {
  it.effect("Settings offers the source locale (the index contentLang) and the compiled locales, a prompt-only one included; a new profile starts at the source locale", () => overKorean(Effect.scoped(Effect.gen(function* () {
    const recorded = recorder()
    const handle = yield* player(recorded)
    yield* act(handle, { type: "openSettings" })
    expect(lastScreen(recorded.screens, "Settings")).toMatchObject({ locales: ["en-US", "ja", "ko"], settings: { locale: "en-US" } })
    yield* act(handle, { type: "close" })
    yield* act(handle, { type: "newGame" })
    expect(recorded.lines.map(textOf)).toEqual(["The bell is ringing."])
    expect(recorded.choices.at(-1)?.prompt).toBe("Where now?")
  }))))

  it.effect("a prompt-only locale translates the branch prompt and leaves the rest in the source locale", () => overKorean(Effect.scoped(Effect.gen(function* () {
    const recorded = recorder()
    const handle = yield* player(recorded)
    yield* act(handle, { type: "setSettings", settings: { locale: "ja" } })
    yield* act(handle, { type: "newGame" })
    expect(recorded.lines.map(textOf)).toEqual(["The bell is ringing."])
    expect(recorded.choices.at(-1)).toMatchObject({ prompt: "さて、どこへ?", options: [{ text: "Walk to the dock" }, { text: "Climb the hill" }] })
  }))))

  it.effect("setSettings mid-run: the next line, the choice, its prompt and the backlog read Korean; a new game stays Korean", () => overKorean(Effect.scoped(Effect.gen(function* () {
    const recorded = recorder()
    const handle = yield* player(recorded)
    // The first line is presented in the source locale, then the reader switches.
    yield* handle.actions({ type: "newGame" })
    yield* flush
    expect(recorded.lines.map(textOf)).toEqual(["The bell is ringing."])
    yield* act(handle, { type: "setSettings", settings: { locale: "ko" } })
    expect(recorded.choices.at(-1)?.options.map((option) => option.text)).toEqual(["부두로 걸어간다", "언덕을 오른다"])
    expect(recorded.choices.at(-1)?.prompt).toBe("이제 어디로?")
    // The backlog, recorded while English was set, is shown in Korean.
    yield* act(handle, { type: "openBacklog" })
    expect(lastScreen(recorded.screens, "Backlog")?.entries.map((entry) => entry.text)).toEqual(["종이 울리고 있어."])
    yield* act(handle, { type: "close" })
    yield* act(handle, { type: "choose", option: "dock" })
    expect(recorded.lines.slice(1).map((line) => [line.speakerName, textOf(line)])).toEqual([["Mara", "물때를 조심해."], ["", "물이 빠진다."]])
    expect((yield* SubscriptionRef.get(handle.view)).flow).toBe("ended")
    // Back to the source locale: the backlog follows when shown again.
    yield* handle.actions({ type: "setSettings", settings: { locale: "en-US" } })
    yield* act(handle, { type: "openBacklog" })
    expect(lastScreen(recorded.screens, "Backlog")?.entries.map((entry) => entry.text)).toEqual(["The bell is ringing.", "Mind the tide.", "The tide goes out."])
    yield* act(handle, { type: "close" })
    // A new game after switching to Korean: the named lines and the choice in Korean.
    yield* handle.actions({ type: "setSettings", settings: { locale: "ko" } })
    yield* act(handle, { type: "toTitle" })
    const shown = recorded.lines.length
    yield* act(handle, { type: "newGame" })
    expect(recorded.lines.slice(shown).map((line) => [line.speakerName, textOf(line)])).toEqual([["Mara", "또 왔구나."], ["Mara", "종이 울리고 있어."]])
    expect(recorded.choices.at(-1)?.options.map((option) => option.text)).toEqual(["부두로 걸어간다", "언덕을 오른다"])
    expect(recorded.choices.at(-1)?.prompt).toBe("이제 어디로?")
  }))))

  it.effect("setSettings while a batch's first line is shown: the next line of the same batch reads Korean", () => overKorean(Effect.scoped(Effect.gen(function* () {
    const recorded = recorder()
    const handle = yield* player(recorded)
    yield* act(handle, { type: "newGame" })
    // `dock` plays one batch of two lines; the reader switches while the first is shown.
    yield* handle.actions({ type: "choose", option: "dock" })
    yield* flush
    expect(recorded.lines.slice(1).map(textOf)).toEqual(["Mind the tide."])
    yield* handle.actions({ type: "setSettings", settings: { locale: "ko" } })
    yield* act(handle, { type: "advance" })
    expect(recorded.lines.slice(1).map(textOf)).toEqual(["Mind the tide.", "물이 빠진다."])
    expect((yield* SubscriptionRef.get(handle.view)).flow).toBe("ended")
  }))))

  it.effect("a saved backlog keeps the translations: continued after a reload it is shown in Korean", () => overKorean(Effect.gen(function* () {
    yield* Effect.scoped(Effect.gen(function* () {
      const handle = yield* player(recorder())
      yield* act(handle, { type: "newGame" })
      yield* act(handle, { type: "quickSave" })
      expect((yield* handle.snapshot).backlog).toEqual([{
        lineId: "quay.mara_0020", voiceKey: "quay.mara-0020", speakerName: "Mara", text: "The bell is ringing.", hasVoice: true, texts: { ko: "종이 울리고 있어." }
      }])
    }))
    yield* Effect.scoped(Effect.gen(function* () {
      const recorded = recorder()
      const handle = yield* player(recorded)
      yield* act(handle, { type: "continue" })
      yield* act(handle, { type: "setSettings", settings: { locale: "ko" } })
      yield* act(handle, { type: "openBacklog" })
      expect(lastScreen(recorded.screens, "Backlog")?.entries.map((entry) => [entry.speakerName, entry.text])).toEqual([["Mara", "종이 울리고 있어."]])
    }))
  })))

  it.effect("a saved locale the bundle does not carry reads source text and Settings selects the source locale", () => overKorean(Effect.gen(function* () {
    yield* Effect.scoped(Effect.gen(function* () {
      const handle = yield* player(recorder())
      // A 0.46 profile saved the old default locale "en".
      const store = yield* KeyValueStore.KeyValueStore
      yield* store.set(`profile:${handle.session.fingerprint}`, JSON.stringify({ profileVersion: "0.46", project: handle.session.fingerprint, readLines: [], settings: { locale: "en" } }))
    }))
    yield* Effect.scoped(Effect.gen(function* () {
      const recorded = recorder()
      const handle = yield* player(recorded)
      yield* act(handle, { type: "openSettings" })
      expect(lastScreen(recorded.screens, "Settings")).toMatchObject({ locales: ["en-US", "ja", "ko"], settings: { locale: "en-US" } })
      yield* act(handle, { type: "close" })
      yield* act(handle, { type: "newGame" })
      expect(recorded.lines.map(textOf)).toEqual(["The bell is ringing."])
    }))
  })))
})
