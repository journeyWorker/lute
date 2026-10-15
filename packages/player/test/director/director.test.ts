import { Effect, Schema, SubscriptionRef } from "effect"
import { describe, expect, it } from "@effect/vitest"
import * as Contract from "../../../runtime/src/generated/Contract.js"
import { compileBundle, hubFixtureRoot, jsonLines, sessionCases, sessionRoot } from "../../../runtime/test/support.js"
import { join } from "node:path"
import { withHarness, record, flush } from "./support.js"

const cases = sessionCases()
const noFlow = { bindingsVersion: "0.45" as const, canvas: { width: 1920, height: 1080 } }
const timed = () => compileBundle(join(sessionRoot, "timed-choice", "project"))
const drain = <A extends { readonly action: (action: { readonly type: "advance" }) => Effect.Effect<void>; readonly view: Effect.Effect<{ readonly story: string }> }>(h: A) => Effect.gen(function* () {
  for (let i = 0; i < 100; i++) {
    const view = yield* h.view
    if (view.story !== "typing" && view.story !== "shown" && view.story !== "running") return
    yield* h.action({ type: "advance" })
  }
  throw new Error("director did not settle")
})

describe("Director", () => {
  it.effect.each(cases)("replays conformance session $name", (testCase) => Effect.gen(function* () {
    const bundle = yield* compileBundle(testCase.project)
    const script = jsonLines(testCase.inputs).map(record)
    const initial = Schema.decodeUnknownSync(Contract.Seed)(script[0]?.seed)
    const inputs = script.slice(1).map((entry) => Schema.decodeUnknownSync(Contract.Input)(entry.input))
    const choices = inputs.filter((input) => input.type === "choose").map((input) => input.option)
    const pending = inputs.filter((input) => input.type !== "choose" && input.type !== "timeout" && input.type !== "bridgeResult")
    const bridge = inputs.find((input) => input.type === "bridgeResult")
    const expected = jsonLines(testCase.expected).map((entry) => Schema.decodeUnknownSync(Contract.Output)(record(entry).output))
    const lineIds = expected.flatMap((output) => output.events.flatMap((event) => {
      if (event.type !== "record") return []
      const value = record(event.record)
      return value.kind === "line" ? [Schema.decodeUnknownSync(Schema.String)(value.lineId)] : []
    }))
    yield* withHarness(bundle, { bindings: noFlow, seed: initial, choices,
      host: { idle: () => Effect.sync(() => pending.shift()), bridge: bridge === undefined ? {} : { minigame: () => Effect.succeed({ fields: bridge.fields }), check: () => Effect.succeed({ fields: bridge.fields }) } }
    }, (h) => Effect.gen(function* () {
      yield* h.action({ type: "newGame" })
      for (let i = 0; i < 200; i++) {
        const view = yield* h.view
        if (view.story === "choice" && inputs.some((input) => input.type === "timeout")) yield* h.tick(5100)
        else if (view.story === "typing" || view.story === "shown") yield* h.action({ type: "advance" })
        else yield* flush
        if (h.lines.length === lineIds.length && (yield* h.view).story === "idle" && pending.length === 0) break
      }
      expect(h.lines.map((line) => line.lineId)).toEqual(lineIds)
      expect((yield* h.session.current).await.type).toBe(expected.at(-1)?.await.type)
    }))
  }))

  it.effect("advance while typing reveals, then releases", () => withHarness(hubFixtureRoot, { typing: true }, (h) => Effect.gen(function* () {
    yield* h.action({ type: "newGame" })
    expect(h.lines).toHaveLength(1)
    // The presentation start cleared the window once before the line.
    const cleared = h.clears()
    yield* h.action({ type: "advance" })
    expect(h.reveals()).toBe(1)
    expect(h.clears()).toBe(cleared)
    yield* h.action({ type: "advance" })
    expect(h.clears()).toBe(cleared + 1)
    expect((yield* h.view).story).toBe("choice")
  })))

  it.effect("auto releases after autoDelay and waits for voice end plus delay", () => Effect.gen(function* () {
    for (const voice of [false, true]) yield* withHarness(hubFixtureRoot, { bindings: { ...noFlow, flow: { newGame: { occasion: "visit" } }, ...(voice ? { voice: { pattern: "voice/{voiceKey}.ogg" } } : {}) } }, (h) => Effect.gen(function* () {
      yield* h.action({ type: "setSettings", settings: { autoDelay: 1 } })
      yield* h.action({ type: "newGame" })
      const cleared = h.clears()
      yield* h.action({ type: "toggleAuto" })
      yield* h.tick(voice ? 1999 : 999)
      expect(h.clears()).toBe(cleared)
      yield* h.tick(1)
      if (voice) {
        expect(h.clears()).toBe(cleared)
        yield* h.tick(999)
        expect(h.clears()).toBe(cleared)
        yield* h.tick(1)
      }
      expect(h.clears()).toBe(cleared + 1)
      expect((yield* h.view).story).toBe("choice")
    }))
  }))

  it.effect("skip read-only stops at the first unread line and skip all passes it", () => withHarness(hubFixtureRoot, {}, (h) => Effect.gen(function* () {
    yield* h.action({ type: "newGame" })
    const cleared = h.clears()
    yield* h.action({ type: "toggleSkip" })
    expect(h.clears()).toBe(cleared)
    expect((yield* h.view).mode).toBe("normal")
    yield* h.action({ type: "setSettings", settings: { skipMode: "all" } })
    yield* h.action({ type: "toggleSkip" })
    expect(h.clears()).toBe(cleared + 1)
    expect((yield* h.view).story).toBe("choice")
  })))

  it.effect("timed choice sends timeout after five seconds and shows the wait branch", () => Effect.gen(function* () {
    const bundle = yield* timed()
    yield* withHarness(bundle, {}, (h) => Effect.gen(function* () {
      yield* h.action({ type: "newGame" }); yield* drain(h)
      expect((yield* h.view).story).toBe("choice")
      yield* h.tick(4999); expect(h.lines).toHaveLength(1)
      yield* h.tick(101); yield* drain(h)
      expect(h.lines.map((line) => line.segments)).toContainEqual([{ _tag: "Text", text: "The knocking stops.", styles: [] }])
      expect((yield* h.session.current).await.type).toBe("idle")
    }))
  }))

  it.effect("opening the menu pauses the countdown and closing resumes it", () => Effect.gen(function* () {
    const bundle = yield* timed()
    yield* withHarness(bundle, {}, (h) => Effect.gen(function* () {
      yield* h.action({ type: "newGame" }); yield* drain(h)
      yield* h.tick(2000)
      yield* h.action({ type: "openMenu" }); yield* h.tick(10000)
      expect((yield* h.session.current).await.type).toBe("awaitChoice")
      expect(h.lines).toHaveLength(1)
      yield* h.action({ type: "close" }); yield* h.tick(3100); yield* drain(h)
      expect((yield* h.session.current).await.type).toBe("idle")
    }))
  }))

  it.effect("quickSave then advance then quickLoad restores stage backlog and await", () => withHarness(hubFixtureRoot, {}, (h) => Effect.gen(function* () {
    yield* h.action({ type: "newGame" }); yield* drain(h)
    const point = yield* h.session.current
    const backlog = yield* SubscriptionRef.get(h.session.backlog)
    yield* h.action({ type: "quickSave" })
    yield* h.action({ type: "choose", option: "take" }); yield* drain(h)
    expect((yield* h.session.current).await).not.toEqual(point.await)
    yield* h.action({ type: "quickLoad" })
    yield* h.action({ type: "confirm", yes: true })
    expect(h.restores.at(-1)).toEqual(point.stage)
    expect((yield* h.session.current).await).toEqual(point.await)
    expect(yield* SubscriptionRef.get(h.session.backlog)).toEqual(backlog)
  })))

  it.effect("rollback restores the previous choice point await and stage", () => withHarness(hubFixtureRoot, {}, (h) => Effect.gen(function* () {
    yield* h.action({ type: "newGame" }); yield* drain(h)
    const first = yield* h.session.current
    yield* h.action({ type: "choose", option: "take" }); yield* drain(h)
    yield* h.action({ type: "rollback" })
    expect((yield* h.session.current).await).toEqual(first.await)
    expect(h.restores.at(-1)).toEqual(first.stage)
  })))

  it.effect("setSettings updates audio volumes and persists the profile", () => withHarness(hubFixtureRoot, {}, (h) => Effect.gen(function* () {
    const volumes = { master: 0.7, music: 0.4, sfx: 0.2, voice: 0.9 }
    yield* h.action({ type: "setSettings", settings: { volumes } }); yield* h.tick(1)
    expect(h.volumes.at(-1)).toEqual(volumes)
    const persisted = yield* h.kv.get(`profile:${h.session.fingerprint}`)
    expect(persisted).toBeDefined()
    expect(record(record(JSON.parse(persisted ?? "{}")).settings).volumes).toEqual(volumes)
  })))

  it.effect("awaitBridge without a host handler shows Halted", () => Effect.gen(function* () {
    const bundle = yield* compileBundle(join(sessionRoot, "bridge-call", "project"))
    yield* withHarness(bundle, { bindings: { ...noFlow, flow: { newGame: { occasion: "chapter" } } } }, (h) => Effect.gen(function* () {
      yield* h.action({ type: "newGame" }); yield* drain(h)
      expect(h.screens.at(-1)?._tag).toBe("Halted")
    }))
  }))

  it.effect("missing asset file still progresses and logs once", () => withHarness(hubFixtureRoot, { bindings: { ...noFlow, flow: { newGame: { occasion: "visit" } }, voice: { pattern: "missing.ogg" } }, missing: new Set(["missing.ogg"]) }, (h) => Effect.gen(function* () {
    yield* h.action({ type: "newGame" }); yield* drain(h)
    expect(h.ensured).toEqual([["missing.ogg"]])
    expect((yield* h.view).story).toBe("choice")
  })))

  it.effect("Title Continue is enabled after an autosave exists", () => withHarness(hubFixtureRoot, {}, (h) => Effect.gen(function* () {
    expect(h.screens.at(-1)).toMatchObject({ _tag: "Title", canContinue: false })
    yield* h.action({ type: "newGame" }); yield* drain(h)
    expect((yield* h.saves.list).some((slot) => slot.slot === "auto")).toBe(true)
    yield* h.action({ type: "toTitle" })
    expect(h.screens.at(-1)).toMatchObject({ _tag: "Confirm" })
    yield* h.action({ type: "confirm", yes: true })
    expect(h.screens.at(-1)).toMatchObject({ _tag: "Title", canContinue: true })
    expect((yield* h.view).flow).toBe("title")
  })))
})
