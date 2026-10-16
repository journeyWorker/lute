/** Spec 0.41.0 §7.2–§7.4, §7.6, §7.9, §7.10: the director's host-facing operations. */
import { Deferred, Effect, Exit, Option, SubscriptionRef } from "effect"
import { describe, expect, it } from "@effect/vitest"
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs"
import { tmpdir } from "node:os"
import { join } from "node:path"
import * as Errors from "../../../runtime/src/Errors.js"
import { compileBundle, hubFixtureRoot, repositoryRoot, sessionRoot } from "../../../runtime/test/support.js"
import * as Cues from "../../src/cues/index.js"
import { DirectorBusy } from "../../src/director/index.js"
import { SaveIncompatible } from "../../src/session/index.js"
import type { Bindings } from "../../src/bindings/schema.js"
import type { Input } from "../../src/model.js"
import { flush, withHarness } from "./support.js"
import type { Harness } from "./support.js"

const canvas = { width: 1920, height: 1080 }
const noFlow: Bindings = { bindingsVersion: "0.46", canvas }
const visit: Input = { type: "raiseOccasion", occasion: "visit" }
const gateFixture = join(repositoryRoot, "crates", "lute-cli", "tests", "fixtures", "runtime-gate-041")
const failure = <A, E>(exit: Exit.Exit<A, E>) => Option.getOrUndefined(Exit.findErrorOption(exit))

/** Advance through lines until the story waits for something else. */
const drain = (h: Harness) => Effect.gen(function* () {
  for (let i = 0; i < 100; i++) {
    const view = yield* h.view
    if (view.story !== "typing" && view.story !== "shown" && view.story !== "running") return
    yield* h.action({ type: "advance" })
  }
  throw new Error("director did not settle")
})
const lineText = (h: Harness) => h.lines.map((line) => line.segments.map((segment) => segment._tag === "Text" ? segment.text : "").join(""))
const stateOf = (h: Harness, path: string) => Effect.gen(function* () {
  const { state } = yield* SubscriptionRef.get(h.runtime.current)
  return (yield* h.runtime.program.view(state)).state[path]
})

describe("DirectorHandle.send (§7.2)", () => {
  it.effect("at idle: accepted inputs play as a batch; send completes when the batch started", () => withHarness(hubFixtureRoot, { bindings: noFlow, typing: true }, (h) => Effect.gen(function* () {
    yield* h.action({ type: "newGame" })
    expect((yield* h.view).story).toBe("idle")
    yield* h.director.send(visit)
    yield* flush
    // The batch's first line still types: send did not wait for the presentation.
    expect((yield* h.view).story).toBe("typing")
    expect(lineText(h)).toEqual(["Before the hub."])
    yield* drain(h)
    expect((yield* h.view).story).toBe("choice")
  })))

  it.effect("at idle: a runtime rejection fails send and leaves the idle menu shown", () => withHarness(hubFixtureRoot, {
    bindings: { ...noFlow, flow: { idle: { menu: [{ label: "Visit", occasion: "visit" }] } } }
  }, (h) => Effect.gen(function* () {
    yield* h.action({ type: "newGame" })
    expect(h.screens.at(-1)).toEqual({ _tag: "Idle", options: [{ label: "Visit" }] })
    const hidden = h.hiddenScreens()
    const refused = failure(yield* Effect.exit(h.director.send({ type: "choose", option: "take", request: 1 })))
    expect(refused).toBeInstanceOf(Errors.RuntimeBusy)
    yield* flush
    expect((yield* h.view).story).toBe("idle")
    expect(h.hiddenScreens()).toBe(hidden)
    expect((yield* h.session.snapshot).log).toHaveLength(1)
    // The idle menu still answers.
    yield* h.action({ type: "pickIdle", index: 0 })
    yield* drain(h)
    expect((yield* h.view).story).toBe("choice")
  })))

  it.effect("is busy while a batch runs and at a choice, changing nothing", () => withHarness(hubFixtureRoot, { bindings: noFlow, typing: true }, (h) => Effect.gen(function* () {
    yield* h.action({ type: "newGame" })
    yield* h.director.send(visit)
    yield* flush
    expect(failure(yield* Effect.exit(h.director.send(visit)))).toEqual(new DirectorBusy({ flow: "playing", story: "typing" }))
    yield* drain(h)
    const before = yield* h.session.snapshot
    expect(failure(yield* Effect.exit(h.director.send(visit)))).toEqual(new DirectorBusy({ flow: "playing", story: "choice" }))
    expect((yield* h.session.snapshot).log).toEqual(before.log)
  })))

  it.effect("the send action sends at idle; a refusal is logged and changes nothing", () => withHarness(hubFixtureRoot, { bindings: noFlow }, (h) => Effect.gen(function* () {
    yield* h.action({ type: "newGame" })
    yield* h.action({ type: "send", input: { type: "choose", option: "take", request: 1 } })
    expect((yield* h.view)).toMatchObject({ flow: "playing", story: "idle" })
    yield* h.action({ type: "send", input: visit })
    yield* drain(h)
    expect((yield* h.view).story).toBe("choice")
  })))

  it.effect("a host idle runs on its own fiber: actions work while it waits, send interrupts it", () => Effect.gen(function* () {
    const interrupted = yield* Deferred.make<void>()
    let calls = 0
    yield* withHarness(hubFixtureRoot, {
      bindings: noFlow,
      host: { idle: () => Effect.sync(() => { calls++ }).pipe(Effect.andThen(Effect.never), Effect.onInterrupt(() => Deferred.succeed(interrupted, undefined))) }
    }, (h) => Effect.gen(function* () {
      yield* h.action({ type: "newGame" })
      expect(calls).toBe(1)
      yield* h.action({ type: "openMenu" })
      expect((yield* h.view).overlay).toBe("menu")
      yield* h.action({ type: "close" })
      yield* h.director.send(visit)
      yield* Deferred.await(interrupted)
      yield* drain(h)
      expect((yield* h.view).story).toBe("choice")
    }))
  }))
})

describe("continuing at ended (§7.3)", () => {
  it.effect("send at ended accepts an outside-run raise and newRun, and plays on", () => Effect.gen(function* () {
    const bundle = yield* compileBundle(gateFixture)
    yield* withHarness(bundle, { bindings: noFlow }, (h) => Effect.gen(function* () {
      yield* h.action({ type: "newGame" })
      yield* h.director.send({ type: "raiseOccasion", occasion: "verdict" })
      yield* drain(h)
      expect(yield* h.view).toMatchObject({ flow: "ended", story: "ended" })
      expect(h.screens.at(-1)).toMatchObject({ _tag: "Ended" })
      // A closed occasion is the runtime's refusal.
      expect(failure(yield* Effect.exit(h.director.send({ type: "raiseOccasion", occasion: "gallery" })))).toBeInstanceOf(Errors.OccasionGateClosed)
      const hidden = h.hiddenScreens()
      yield* h.director.send({ type: "raiseOccasion", occasion: "title" })
      expect(h.hiddenScreens()).toBe(hidden + 1)
      yield* drain(h)
      expect(lineText(h)).toContain("The title screen.")
      expect((yield* h.view).flow).toBe("ended")
      yield* h.action({ type: "send", input: { type: "newRun" } })
      yield* drain(h)
      expect(yield* h.view).toMatchObject({ flow: "playing", story: "idle" })
    }))
  }))

  it.effect("a second new game from the title starts a new run instead of replaying started", () => Effect.gen(function* () {
    const bundle = yield* compileBundle(join(sessionRoot, "new-run", "project"))
    yield* withHarness(bundle, { bindings: { ...noFlow, flow: { newGame: { occasion: "visit" } } } }, (h) => Effect.gen(function* () {
      yield* h.action({ type: "newGame" }); yield* drain(h)
      expect(h.lines).toHaveLength(1)
      yield* h.action({ type: "toTitle" })
      yield* h.action({ type: "confirm", yes: true })
      expect((yield* h.view).flow).toBe("title")
      yield* h.action({ type: "newGame" }); yield* drain(h)
      expect(h.lines).toHaveLength(2)
      expect(yield* stateOf(h, "user.value")).toEqual({ kind: "int", value: 2 })
      expect((yield* h.session.snapshot).log.map((entry) => "input" in entry ? entry.input.type : "start"))
        .toEqual(["start", "raiseOccasion", "newRun", "raiseOccasion"])
    }))
  }))
})

describe("DirectorHandle.restore (§7.4)", () => {
  it.effect("from playing: stage, backlog, read marks, auto slot and the re-presented choice", () => Effect.gen(function* () {
    const bundle = yield* compileBundle(join(sessionRoot, "hub-once", "project"))
    yield* withHarness(bundle, { bindings: { ...noFlow, flow: { newGame: { occasion: "visit" } } }, choices: [] }, (h) => Effect.gen(function* () {
      yield* h.action({ type: "newGame" }); yield* drain(h)
      const point = yield* h.session.current
      const save = yield* h.session.snapshot
      yield* h.action({ type: "choose", option: "take" }); yield* drain(h)
      yield* h.action({ type: "choose", option: "yes" }); yield* drain(h)
      const read = yield* SubscriptionRef.get(h.profile.readLines)
      expect([...read]).toEqual(["hub.narrator_0010", "hub.narrator_0020"])
      const hiddenChoices = h.hiddenChoices()
      yield* h.director.restore(save)
      yield* flush
      expect(h.hiddenChoices()).toBeGreaterThan(hiddenChoices)
      expect(h.restores.at(-1)).toEqual(point.stage)
      expect(yield* SubscriptionRef.get(h.session.backlog)).toEqual(save.backlog)
      expect([...(yield* SubscriptionRef.get(h.profile.readLines))]).toEqual(["hub.narrator_0010"])
      expect((yield* h.saves.load("auto")).runtime.log).toEqual(save.log)
      expect((yield* h.session.current).await).toEqual(point.await)
      expect(yield* h.view).toMatchObject({ flow: "playing", story: "choice" })
      // The restored choice answers.
      yield* h.action({ type: "choose", option: "take" }); yield* drain(h)
      expect((yield* h.session.current).await).toMatchObject({ type: "awaitChoice", menu: { id: "answer" } })
    }))
  }))

  it.effect("from ended and from halted: the restored idle runs the idle flow again", () => Effect.gen(function* () {
    const bundle = yield* compileBundle(gateFixture)
    let idles = 0
    yield* withHarness(bundle, { bindings: noFlow, host: { idle: () => Effect.sync(() => { idles++; return undefined }) } }, (h) => Effect.gen(function* () {
      yield* h.action({ type: "newGame" })
      const start = yield* h.session.snapshot
      expect(idles).toBe(1)
      yield* h.director.send({ type: "raiseOccasion", occasion: "verdict" }); yield* drain(h)
      expect((yield* h.view).flow).toBe("ended")
      yield* h.director.restore(start)
      yield* flush
      expect(yield* h.view).toMatchObject({ flow: "playing", story: "idle" })
      expect(idles).toBe(2)
      yield* h.director.send({ type: "raiseOccasion", occasion: "listen" }); yield* drain(h)
      expect(yield* h.view).toMatchObject({ flow: "error", story: "halted" })
      yield* h.director.restore(start)
      yield* flush
      expect(yield* h.view).toMatchObject({ flow: "playing", story: "idle" })
      expect(idles).toBe(3)
    }))
  }))

  it.effect("restarts the stage's music and refuses a save of another project", () => Effect.gen(function* () {
    const bundle = yield* compileBundle(join(sessionRoot, "stage-snapshot", "project"))
    yield* withHarness(bundle, { bindings: { ...noFlow, assets: { "BGM.parlor": "parlor.ogg" }, volumes: { normal: 0.6 } } }, (h) => Effect.gen(function* () {
      yield* h.action({ type: "newGame" })
      yield* h.director.send(visit); yield* drain(h)
      const save = yield* h.session.snapshot
      yield* h.director.restore(save)
      yield* flush
      expect(h.plays.at(-1)).toEqual({ channel: "music", sound: { file: "parlor.ogg", loop: true, volume: 0.6, fade: 0, restart: false } })
      const other = { ...save, snapshot: { ...save.snapshot, project: "other" } }
      expect(failure(yield* Effect.exit(h.director.restore(other)))).toBeInstanceOf(SaveIncompatible)
    }))
  }))

  it.effect("ensures an unmapped music assetId before restarting it (one resolver for files and music)", () => Effect.gen(function* () {
    const bundle = yield* compileBundle(join(sessionRoot, "stage-snapshot", "project"))
    yield* withHarness(bundle, { bindings: noFlow }, (h) => Effect.gen(function* () {
      yield* h.action({ type: "newGame" })
      yield* h.director.send(visit); yield* drain(h)
      const save = yield* h.session.snapshot
      const before = h.media.length
      yield* h.director.restore(save)
      yield* flush
      const media = h.media.slice(before)
      expect(media).toContain("ensure BGM.parlor")
      expect(media).toContain("play music BGM.parlor")
      expect(media.indexOf("ensure BGM.parlor")).toBeLessThan(media.indexOf("play music BGM.parlor"))
    }))
  }))

  it.effect("without autosave the auto slot is not rewritten", () => withHarness(hubFixtureRoot, { autosave: false }, (h) => Effect.gen(function* () {
    yield* h.action({ type: "newGame" }); yield* drain(h)
    const save = yield* h.session.snapshot
    yield* h.director.restore(save)
    yield* flush
    expect((yield* h.saves.list).some((slot) => slot.slot === "auto")).toBe(false)
  })))
})

describe("voice replay (§5.4)", () => {
  it.effect("uses voiceKey rather than lineId", () => withHarness(hubFixtureRoot, {
    bindings: { ...noFlow, voice: { pattern: "voice/{voiceKey}.ogg" } }
  }, (h) => Effect.gen(function* () {
    yield* h.action({ type: "replayVoice", lineId: "display-line", voiceKey: "original-voice" })
    expect(h.plays.at(-1)).toEqual({
      channel: "voice",
      sound: { file: "voice/original-voice.ogg", loop: false, volume: 1, fade: 0, restart: true }
    })
  })))
})

describe("autosave (§7.9)", () => {
  const captures = (autosave: false | "presentation" | "choice" | undefined, project: string) => Effect.gen(function* () {
    const bundle = yield* compileBundle(join(sessionRoot, project, "project"))
    return yield* withHarness(bundle, { bindings: { ...noFlow, flow: { newGame: { occasion: "visit" } } }, ...(autosave === undefined ? {} : { autosave }) }, (h) => Effect.gen(function* () {
      yield* h.action({ type: "newGame" }); yield* drain(h)
      return { captures: h.captures(), auto: (yield* h.saves.list).some((slot) => slot.slot === "auto") }
    }))
  })
  it.effect("writes at presentation starts and choices by default, only there with a mode, never with false", () => Effect.gen(function* () {
    // hub-once: one presentation, then a choice.
    expect(yield* captures(undefined, "hub-once")).toEqual({ captures: 2, auto: true })
    expect(yield* captures("presentation", "hub-once")).toEqual({ captures: 1, auto: true })
    expect(yield* captures("choice", "hub-once")).toEqual({ captures: 1, auto: true })
    expect(yield* captures(false, "hub-once")).toEqual({ captures: 0, auto: false })
    // new-run: a presentation that ends at idle.
    expect(yield* captures("choice", "new-run")).toEqual({ captures: 0, auto: false })
    expect(yield* captures("presentation", "new-run")).toEqual({ captures: 1, auto: true })
  }))
})

describe("bridges and plugin records (§7.6, §7.10)", () => {
  it.effect("a bridge answer's writes are sent with the result; answered records never reach plugin", () => Effect.gen(function* () {
    const bundle = yield* compileBundle(join(sessionRoot, "bridge-call", "project"))
    const plugin: Array<string> = []
    yield* withHarness(bundle, {
      bindings: { ...noFlow, flow: { newGame: { occasion: "chapter" } } },
      host: {
        bridge: { check: (request) => Effect.succeed({ fields: { passed: request.attrs.dc === 12 }, writes: { state: [{ path: "run.score", value: 5 }] } }) },
        plugin: { check: (call) => Effect.sync(() => { plugin.push(call.tag) }) }
      }
    }, (h) => Effect.gen(function* () {
      yield* h.action({ type: "newGame" }); yield* drain(h)
      expect(lineText(h)).toEqual(["Persuasion succeeded.", "Score received."])
      const log = (yield* h.session.snapshot).log.at(-1)
      expect(log).toEqual({ input: { type: "bridgeResult", request: 1, fields: { passed: true }, writes: { state: [{ path: "run.score", value: 5 }] } } })
      expect(yield* stateOf(h, "run.score")).toEqual({ kind: "int", value: 5 })
      expect(plugin).toEqual([])
    }))
  }))

  it.effect("a bridge handler runs on its own fiber: its send is refused as busy, not deadlocked", () => Effect.gen(function* () {
    const bundle = yield* compileBundle(join(sessionRoot, "bridge-call", "project"))
    const handle = yield* Deferred.make<Harness["director"]>()
    const refusals: Array<unknown> = []
    const check = () => Effect.gen(function* () {
      const director = yield* Deferred.await(handle)
      refusals.push(failure(yield* Effect.exit(director.send(visit))))
      return { fields: { passed: false } }
    })
    yield* withHarness(bundle, { bindings: { ...noFlow, flow: { newGame: { occasion: "chapter" } } }, host: { bridge: { check } } }, (h) => Effect.gen(function* () {
      yield* Deferred.succeed(handle, h.director)
      yield* h.action({ type: "newGame" }); yield* drain(h)
      expect(refusals).toEqual([new DirectorBusy({ flow: "playing", story: "bridge" })])
      expect(lineText(h)).toEqual(["Persuasion failed.", "Score received."])
    }))
  }))
})

describe("cue factory composition (§5.3)", () => {
  const bridgeHost = { bindings: { ...noFlow, flow: { newGame: { occasion: "chapter" } } } } as const
  /** conformance/command-plugin as a `chapter` scene: one unanswered `demo` plugin record. */
  const pluginProject = Effect.suspend(() => {
    const dir = mkdtempSync(join(tmpdir(), "lute-player-plugin-"))
    cpSync(join(repositoryRoot, "conformance", "command-plugin"), dir, { recursive: true })
    for (const file of ["artifact.json", "expected.json", "mock.yaml"]) rmSync(join(dir, file))
    const source = join(dir, "source.lute")
    writeFileSync(source, readFileSync(source, "utf8").replace("kind: scene\n", "kind: scene\nid: trial\non: chapter\n"))
    return compileBundle(dir)
  })
  it.effect("host.plugin wins a tag over a pre-composed factory's handler", () => Effect.gen(function* () {
    const bundle = yield* pluginProject
    const calls: Array<string> = []
    yield* withHarness(bundle, {
      ...bridgeHost,
      cueFactory: { compose: (compiled, bindings) => Cues.make({ bundle: compiled, bindings, plugin: { demo: () => Effect.sync(() => { calls.push("composed") }) } }) },
      host: { plugin: { demo: () => Effect.sync(() => { calls.push("host") }) } }
    }, (h) => Effect.gen(function* () {
      yield* h.action({ type: "newGame" }); yield* drain(h)
      expect(calls).toEqual(["host"])
    }))
  }))

  it.effect("a wrapper wraps the default factory once, and that factory carries host.plugin", () => Effect.gen(function* () {
    const bundle = yield* pluginProject
    const calls: Array<string> = []
    let wraps = 0
    let outputs = 0
    yield* withHarness(bundle, {
      ...bridgeHost,
      cueFactory: (base) => {
        wraps++
        return { ...base, fromOutput: (output, read, locale) => { outputs++; return base.fromOutput(output, read, locale) } }
      },
      host: { plugin: { demo: () => Effect.sync(() => { calls.push("host") }) } }
    }, (h) => Effect.gen(function* () {
      yield* h.action({ type: "newGame" }); yield* drain(h)
      expect(wraps).toBe(1)
      expect(outputs).toBeGreaterThan(0)
      expect(calls).toEqual(["host"])
    }))
  }))

  it.effect("host.plugin with a factory not built by Cues.make is a defect, not ignored", () => Effect.gen(function* () {
    const bundle = yield* pluginProject
    const exit = yield* Effect.exit(withHarness(bundle, {
      ...bridgeHost,
      cueFactory: { compose: (compiled, bindings) => {
        const base = Cues.make({ bundle: compiled, bindings })
        return { choiceView: base.choiceView, fromOutput: base.fromOutput }
      } },
      host: { plugin: { demo: () => Effect.void } }
    }, () => Effect.void))
    expect(Exit.hasDies(exit)).toBe(true)
  }))
})

describe("DirectorHandle.snapshot (§5.2)", () => {
  it.effect("while a line is presenting it returns the committed runtime point, log and backlog together", () => withHarness(hubFixtureRoot, { typing: true }, (h) => Effect.gen(function* () {
    yield* h.action({ type: "newGame" })
    expect((yield* h.view).story).toBe("typing")
    const save = yield* h.director.snapshot
    expect(save).toEqual(yield* h.session.snapshot)
    expect(save.log.at(-1)).toEqual({ input: { type: "raiseOccasion", occasion: "visit" } })
    expect(save.backlog.length).toBeGreaterThan(0)
    expect(save.snapshot.await).toEqual((yield* SubscriptionRef.get(h.runtime.current)).await)
  })))
})
