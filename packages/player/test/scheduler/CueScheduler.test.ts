import { Deferred, Effect, Fiber, Stream, SubscriptionRef } from "effect"
import * as TestClock from "effect/testing/TestClock"
import { describe, expect, it } from "@effect/vitest"
import type { Cue, CueContext } from "../../src/model.js"
import * as Scheduler from "../../src/scheduler/index.js"
import type { AudioPort, LineView, PlayMode, Presenter, StageCue } from "../../src/ports/index.js"
import { flush } from "../director/support.js"

const line: LineView = { lineId: "line", voiceKey: "", role: "dialogue", speaker: "narrator", speakerName: "", segments: [{ _tag: "Text", text: "x", styles: [] }], read: false }
const presenter: Presenter = {
  stage: (_cue: StageCue) => Effect.void,
  showLine: () => Effect.void,
  revealLine: Effect.void,
  clearLine: Effect.void,
  restore: () => Effect.void,
  showChoice: () => Effect.succeed(""),
  hideChoice: Effect.void,
  showScreen: () => Effect.void,
  hideScreen: Effect.void,
  setUiHidden: () => Effect.void,
  applySettings: () => Effect.void,
  capture: Effect.succeed(""),
  actions: Stream.empty
}
const audio: AudioPort = {
  play: () => Effect.void,
  stop: () => Effect.void,
  setVolumes: () => Effect.void
}
const ctx: CueContext = {
  presenter,
  audio,
  bindings: { bindingsVersion: "0.48", canvas: { width: 1, height: 1 } },
  settings: {
    textSpeed: 1,
    autoDelay: 0,
    skipMode: "all",
    volumes: { master: 1, music: 1, sfx: 1, voice: 1 },
    locale: "en",
    windowOpacity: 1,
    readColour: true,
    rollbackDepth: 10
  }
}

const cue = (kind: Cue["kind"], events: string[], options: Partial<Cue> = {}): Cue => ({
  kind,
  timing: { delay: 0, duration: undefined, wait: true, timeline: undefined, at: undefined },
  stage: false,
  files: [],
  line: kind === "line" ? line : undefined,
  execute: () => Effect.sync(() => events.push(`${kind}:execute`)),
  complete: () => Effect.sync(() => events.push(`${kind}:complete`)),
  describe: kind,
  ...options
})
const gate = { started: (_line: LineView) => Effect.void, release: (shown: LineView) => Effect.sync(() => shown.lineId === line.lineId) }

const install = Effect.gen(function* () {
  const scheduler = yield* Scheduler.make()
  const mode = yield* SubscriptionRef.make<PlayMode>("normal")
  return { scheduler, mode }
})

describe("CueScheduler", () => {
  it.effect("delays cue start on Clock", () => Effect.gen(function* () {
    const { scheduler, mode } = yield* install
    const events: string[] = []
    const delayed = cue("sfx", events, { timing: { delay: 2, duration: undefined, wait: true, timeline: undefined, at: undefined } })
    const fiber = yield* Effect.forkScoped(scheduler.play([delayed], ctx, mode, gate))
    yield* TestClock.adjust(1)
    expect(events).toEqual([])
    yield* TestClock.adjust(1)
    yield* Fiber.join(fiber)
    expect(events).toEqual(["sfx:execute"])
  }))
  it.effect("waits blocking cues and overlaps non-blocking cues", () => Effect.gen(function* () {
    const { scheduler, mode } = yield* install
    const events: string[] = []
    const first = cue("sfx", events, { timing: { delay: 0, duration: undefined, wait: false, timeline: undefined, at: undefined }, execute: () => Effect.sleep(1).pipe(Effect.tap(() => Effect.sync(() => events.push("sfx:done")))) })
    const second = cue("bookkeeping", events)
    const fiber = yield* Effect.forkScoped(scheduler.play([first, second], ctx, mode, gate))
    yield* Fiber.join(fiber)
    expect(events).toEqual(["bookkeeping:execute"])
    yield* TestClock.adjust(1)
    expect(events).toContain("sfx:done")
  }))
  it.effect("forwards cue duration to the cue command", () => Effect.gen(function* () {
    const { scheduler, mode } = yield* install
    const seen: number[] = []
    const timed = cue("bg", [], { stage: true, timing: { delay: 0, duration: 3, wait: true, timeline: undefined, at: undefined }, execute: () => Effect.sync(() => seen.push(3)) })
    yield* scheduler.play([timed], ctx, mode, gate)
    expect(seen).toEqual([3])
  }))
  it.effect("starts timeline clips at at offsets", () => Effect.gen(function* () {
    const { scheduler, mode } = yield* install
    const events: string[] = []
    const one = cue("sfx", events, { timing: { delay: 0, duration: undefined, wait: false, timeline: 1, at: 1 } })
    const two = cue("vfx", events, { timing: { delay: 0, duration: undefined, wait: false, timeline: 1, at: 2 } })
    const fiber = yield* Effect.forkScoped(scheduler.play([one, two], ctx, mode, gate))
    yield* TestClock.adjust(1)
    expect(events).toEqual(["sfx:execute"])
    yield* TestClock.adjust(1)
    yield* Fiber.join(fiber)
    expect(events).toEqual(["sfx:execute", "vfx:execute"])
  }))
  it.effect("barrier joins only its timeline group", () => Effect.gen(function* () {
    const { scheduler, mode } = yield* install
    const events: string[] = []
    const slow = cue("bg", events, { timing: { delay: 1, duration: undefined, wait: false, timeline: 1, at: 0 }, stage: true, execute: () => Effect.sleep(1).pipe(Effect.tap(() => Effect.sync(() => events.push("slow:done")))) })
    const other = cue("sfx", events, { timing: { delay: 0, duration: undefined, wait: false, timeline: 2, at: 0 } })
    const barrier = cue("barrier", events, { timing: { delay: 0, duration: undefined, wait: true, timeline: 1, at: 0 } })
    const fiber = yield* Effect.forkScoped(scheduler.play([slow, other, barrier], ctx, mode, gate))
    yield* TestClock.adjust(0)
    expect(events).toContain("sfx:execute")
    yield* TestClock.adjust(2)
    yield* Fiber.join(fiber)
    expect(events).toContain("slow:done")
  }))
  it.effect("settles stage cues at batch end but leaves one-shots running", () => Effect.gen(function* () {
    const { scheduler, mode } = yield* install
    const events: string[] = []
    const stage = cue("bg", events, { stage: true, timing: { delay: 1, duration: undefined, wait: false, timeline: undefined, at: undefined }, execute: () => Effect.sleep(1).pipe(Effect.tap(() => Effect.sync(() => events.push("stage:done")))) })
    const shot = cue("sfx", events, { timing: { delay: 1, duration: undefined, wait: false, timeline: undefined, at: undefined }, execute: () => Effect.sleep(10).pipe(Effect.tap(() => Effect.sync(() => events.push("shot:done")))) })
    const fiber = yield* Effect.forkScoped(scheduler.play([stage, shot], ctx, mode, gate))
    yield* TestClock.adjust(2)
    yield* Fiber.join(fiber)
    expect(events).toContain("stage:done")
    expect(events).not.toContain("shot:done")
  }))
  it.effect("releases lines in order", () => Effect.gen(function* () {
    const { scheduler, mode } = yield* install
    const order: string[] = []
    const lineGate = {
      started: (startedLine: LineView) => Effect.sync(() => order.push(`started:${startedLine.lineId}`)),
      release: (shownLine: LineView) => Effect.sync(() => { order.push(`release:${shownLine.lineId}`); return true })
    }
    const first = cue("line", [], { line })
    const second = cue("line", [], { line: { ...line, lineId: "second" } })
    yield* scheduler.play([first, second], ctx, mode, lineGate)
    expect(order).toEqual(["started:line", "release:line", "started:second", "release:second"])
  }))
  // Spec 0.49.0 §4.2: a `timing.wait: false` line holds no staging cue after it.
  const noWait: Cue["timing"] = { delay: 0, duration: undefined, wait: false, timeline: undefined, at: undefined }
  /** A gate that logs `started:` / `shown:` and holds each line until `release(lineId)`. */
  const heldGate = (order: string[]) => Effect.gen(function* () {
    const releases = new Map<string, Deferred.Deferred<void>>()
    const releaseOf = (lineId: string) => Effect.gen(function* () {
      const existing = releases.get(lineId)
      if (existing !== undefined) return existing
      const created = yield* Deferred.make<void>()
      releases.set(lineId, created)
      return created
    })
    const gate = {
      started: (startedLine: LineView) => Effect.sync(() => order.push(`started:${startedLine.lineId}`)),
      release: (shownLine: LineView) => Effect.andThen(
        Effect.sync(() => order.push(`shown:${shownLine.lineId}`)),
        Effect.flatMap(releaseOf(shownLine.lineId), Deferred.await)
      )
    }
    const release = (lineId: string) => Effect.flatMap(releaseOf(lineId), (released) => Deferred.succeed(released, undefined))
    return { gate, release }
  })
  it.effect("a wait-false line runs the staging cues after it before its release; a presentation, the next line and the batch wait for it (0.49.0 §4.2)", () => Effect.gen(function* () {
    const { scheduler, mode } = yield* install
    const order: string[] = []
    const { gate: lineGate, release } = yield* heldGate(order)
    const held = cue("line", [], { line, timing: noWait })
    const bg = cue("bg", order, { stage: true, timing: noWait })
    const presentation = cue("presentation", order)
    const next = cue("line", [], { line: { ...line, lineId: "next" } })
    let settled = false
    const fiber = yield* Effect.forkScoped(Effect.andThen(scheduler.play([held, bg, presentation, next], ctx, mode, lineGate), Effect.sync(() => { settled = true })))
    yield* flush
    // The held line began first; its shown/release and the bg overlap. The presentation waits.
    expect(order[0]).toBe("started:line")
    expect([...order].sort()).toEqual(["bg:execute", "shown:line", "started:line"])
    expect(settled).toBe(false)
    yield* release("line")
    yield* flush
    expect(order.slice(3)).toEqual(["presentation:execute", "started:next", "shown:next"])
    expect(settled).toBe(false)
    yield* release("next")
    yield* Fiber.join(fiber)
    expect(settled).toBe(true)
  }))
  it.effect("consecutive wait-false lines: each runs its own staging cues and the second waits for the first (0.49.0 §4.2)", () => Effect.gen(function* () {
    const { scheduler, mode } = yield* install
    const order: string[] = []
    const { gate: lineGate, release } = yield* heldGate(order)
    const first = cue("line", [], { line, timing: noWait })
    const night = cue("bg", order, { stage: true, timing: noWait, execute: () => Effect.sync(() => order.push("bg:night")) })
    const second = cue("line", [], { line: { ...line, lineId: "second" }, timing: noWait })
    const dawn = cue("bg", order, { stage: true, timing: noWait, execute: () => Effect.sync(() => order.push("bg:dawn")) })
    let settled = false
    const fiber = yield* Effect.forkScoped(Effect.andThen(scheduler.play([first, night, second, dawn], ctx, mode, lineGate), Effect.sync(() => { settled = true })))
    yield* flush
    expect([...order].sort()).toEqual(["bg:night", "shown:line", "started:line"])
    yield* release("line")
    yield* flush
    expect(order[3]).toBe("started:second")
    expect([...order.slice(3)].sort()).toEqual(["bg:dawn", "shown:second", "started:second"])
    // The last line holds the batch's settling.
    expect(settled).toBe(false)
    yield* release("second")
    yield* Fiber.join(fiber)
    expect(settled).toBe(true)
  }))
  it.effect("skip completes a wait-false line, the cues after it and the next line in one batch (0.49.0 §4.2)", () => Effect.gen(function* () {
    const { scheduler, mode } = yield* install
    const order: string[] = []
    yield* SubscriptionRef.set(mode, "skip")
    const skipGate = {
      started: (startedLine: LineView) => Effect.sync(() => order.push(`started:${startedLine.lineId}`)),
      release: (shownLine: LineView) => Effect.sync(() => order.push(`released:${shownLine.lineId}`))
    }
    const complete = (label: string) => () => Effect.sync(() => order.push(label))
    const held = cue("line", order, { line, timing: noWait, complete: complete("complete:line") })
    const bg = cue("bg", order, { stage: true, timing: noWait, complete: complete("complete:bg") })
    const presentation = cue("presentation", order, { complete: complete("complete:presentation") })
    const next = cue("line", order, { line: { ...line, lineId: "next" }, complete: complete("complete:next") })
    yield* scheduler.play([held, bg, presentation, next], ctx, mode, skipGate)
    // Nothing typed or ran normally; the held line was released before the presentation, which precedes the next line.
    // The forked bg's completion may land anywhere after the held line began.
    expect([...order].sort()).toEqual(["complete:bg", "complete:line", "complete:next", "complete:presentation", "released:line", "released:next"])
    expect(order.filter((entry) => entry !== "complete:bg")).toEqual(["complete:line", "released:line", "complete:presentation", "complete:next", "released:next"])
  }))
  it.effect("a wait-false line that ends the batch holds its settling until released (0.49.0 §4.2)", () => Effect.gen(function* () {
    const { scheduler, mode } = yield* install
    const released = yield* Deferred.make<void>()
    const lineGate = { started: (_line: LineView) => Effect.void, release: (_line: LineView) => Deferred.await(released) }
    const held = cue("line", [], { line, timing: noWait })
    let settled = false
    const fiber = yield* Effect.forkScoped(Effect.andThen(scheduler.play([held], ctx, mode, lineGate), Effect.sync(() => { settled = true })))
    yield* flush
    expect(settled).toBe(false)
    yield* Deferred.succeed(released, undefined)
    yield* Fiber.join(fiber)
    expect(settled).toBe(true)
  }))
  it.effect("skip completes cues without advancing Clock", () => Effect.gen(function* () {
    const { scheduler, mode } = yield* install
    const events: string[] = []
    yield* SubscriptionRef.set(mode, "skip")
    const skipped = cue("video", events, { execute: () => Effect.sleep(100).pipe(Effect.tap(() => Effect.sync(() => events.push("execute")))), complete: () => Effect.sync(() => events.push("complete")) })
    yield* scheduler.play([skipped], ctx, mode, gate)
    expect(events).toEqual(["complete"])
  }))
  it.effect("switches a running cue to complete when skip is enabled", () => Effect.gen(function* () {
    const { scheduler, mode } = yield* install
    const events: string[] = []
    const running = cue("video", events, { execute: () => Effect.sleep(10).pipe(Effect.tap(() => Effect.sync(() => events.push("execute")))), complete: () => Effect.sync(() => events.push("complete")) })
    const fiber = yield* Effect.forkScoped(scheduler.play([running], ctx, mode, gate))
    yield* SubscriptionRef.set(mode, "skip")
    yield* Fiber.join(fiber)
    expect(events).toEqual(["complete"])
  }))
  it.effect("a delayed one-shot cue holds the batch until it starts, not until it finishes (0.47.2 §2)", () => Effect.gen(function* () {
    const { scheduler, mode } = yield* install
    const events: string[] = []
    const shot = cue("sfx", events, { timing: { delay: 2, duration: undefined, wait: false, timeline: undefined, at: undefined }, execute: () => Effect.sync(() => events.push("sfx:execute")).pipe(Effect.andThen(Effect.sleep(100))) })
    let settled = false
    const fiber = yield* Effect.forkScoped(Effect.andThen(scheduler.play([cue("line", events, { line }), shot], ctx, mode, gate), Effect.sync(() => { settled = true })))
    yield* TestClock.adjust(1)
    expect(events).toEqual(["line:execute"])
    expect(settled).toBe(false)
    yield* TestClock.adjust(1)
    yield* Fiber.join(fiber)
    expect(events).toEqual(["line:execute", "sfx:execute"])
  }))
  it.effect("a one-shot cue still in its delay when skip begins is completed at once (0.47.2 §2)", () => Effect.gen(function* () {
    const { scheduler, mode } = yield* install
    const events: string[] = []
    const delayed = cue("sfx", events, { timing: { delay: 5, duration: undefined, wait: false, timeline: undefined, at: undefined } })
    const fiber = yield* Effect.forkScoped(scheduler.play([delayed, cue("line", events, { line })], ctx, mode, gate))
    yield* TestClock.adjust(0)
    expect(events).toEqual(["line:execute"])
    yield* SubscriptionRef.set(mode, "skip")
    yield* Fiber.join(fiber)
    expect(events).toEqual(["line:execute", "sfx:complete"])
  }))
  it.effect("a timeline clip still waiting for its `at` when skip begins is completed at once (0.47.2 §2)", () => Effect.gen(function* () {
    const { scheduler, mode } = yield* install
    const events: string[] = []
    const now = cue("sfx", events, { timing: { delay: 0, duration: undefined, wait: false, timeline: 1, at: 0 } })
    const later = cue("vfx", events, { timing: { delay: 0, duration: undefined, wait: false, timeline: 1, at: 9 } })
    const fiber = yield* Effect.forkScoped(scheduler.play([now, later], ctx, mode, gate))
    yield* TestClock.adjust(0)
    expect(events).toEqual(["sfx:execute"])
    yield* SubscriptionRef.set(mode, "skip")
    yield* Fiber.join(fiber)
    expect(events).toEqual(["sfx:execute", "vfx:complete"])
  }))
  it.effect("interrupts pending batch fibers", () => Effect.gen(function* () {
    const { scheduler, mode } = yield* install
    const events: string[] = []
    const pending = cue("bg", events, { stage: true, execute: () => Effect.sleep(100).pipe(Effect.tap(() => Effect.sync(() => events.push("execute")))), complete: () => Effect.sync(() => events.push("complete")) })
    const fiber = yield* Effect.forkScoped(scheduler.play([pending], ctx, mode, gate))
    yield* Fiber.interrupt(fiber)
    expect(events).toEqual([])
  }))
})
