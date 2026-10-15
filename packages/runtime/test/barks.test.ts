/**
 * dsl 0.46.0 §3: the "background bark during a conversation" host pattern.
 * Queries answer at every await without changing the state; a static bark's
 * lines are read from the bundle; a bark that records is raised after idle.
 */
import { Deferred, Effect, Fiber, Layer, Ref, Schema, SubscriptionRef } from "effect"
import { describe, expect, it } from "@effect/vitest"
import type { Bundle } from "../src/Bundle.js"
import * as BundleSource from "../src/BundleSource.js"
import type * as Contract from "../src/generated/Contract.js"
import * as InputLog from "../src/InputLog.js"
import * as LuteBun from "../src/bun.js"
import * as Session from "../src/Session.js"
import { barkFixtureRoot, canonical, compileBundle } from "./support.js"

/** The record fields the pattern reads; every other field is ignored. */
const Artifact = Schema.Struct({
  commands: Schema.Array(Schema.Struct({
    kind: Schema.String,
    position: Schema.String,
    id: Schema.optionalKey(Schema.String),
    body: Schema.optionalKey(Schema.String),
    speaker: Schema.optionalKey(Schema.String),
    text: Schema.optionalKey(Schema.String),
    texts: Schema.optionalKey(Schema.Record(Schema.String, Schema.String)),
    voiceKey: Schema.optionalKey(Schema.String),
    lineId: Schema.optionalKey(Schema.String),
    placeholders: Schema.optionalKey(Schema.Unknown)
  }))
})
const decodeArtifact = Schema.decodeUnknownSync(Artifact)

/** The pattern's reading (spec §3.2): the lines of a static entry, else undefined. */
const staticBarkLines = (bundle: Bundle, candidate: Contract.Candidate, locale?: string) => {
  if (candidate.kind !== "entry" || candidate.verdict.verdict !== "eligible") return undefined
  const document = bundle.index.documents.find((item) => item.path === candidate.document)
  if (document === undefined) return undefined
  const { commands } = decodeArtifact(bundle.artifacts[document.artifact])
  const head = commands.find((record) => record.kind === "entry" && record.id === candidate.id)
  const start = commands.findIndex((record) => record.position === head?.body)
  if (start < 0) return undefined
  const next = commands.findIndex((record, index) => index > start && (record.kind === "entry" || record.kind === "beat"))
  const body = commands.slice(start, next < 0 ? commands.length : next)
  if (!body.every((record) => record.kind === "line" && record.placeholders === undefined)) return undefined
  return body.map((record) => ({
    text: (locale === undefined ? undefined : record.texts?.[locale]) ?? record.text,
    speaker: record.speaker === undefined ? undefined : bundle.index.cast?.[record.speaker],
    voiceKey: record.voiceKey,
    lineId: record.lineId
  }))
}

interface Hold {
  readonly reached: Deferred.Deferred<void>
  readonly release: Deferred.Deferred<void>
}

/**
 * An input log whose `append` — called after the runtime step, before the
 * session commits it — signals `reached` and waits for `release` while a hold
 * is set: a `send` stopped mid-flight.
 */
const heldLog = (hold: Ref.Ref<Hold | undefined>) =>
  Layer.effect(InputLog.InputLog, Effect.gen(function* () {
    const inner = yield* InputLog.InputLog
    return {
      ...inner,
      append: (entry) => Effect.flatMap(Ref.get(hold), (held) => held === undefined
        ? Effect.void
        : Effect.andThen(Deferred.succeed(held.reached, undefined), Deferred.await(held.release))
      ).pipe(Effect.andThen(inner.append(entry)))
    }
  })).pipe(Layer.provide(InputLog.memory))

const raise = (occasion: string): Contract.Input => ({ type: "raiseOccasion", occasion })

describe("dsl 0.46.0 §3 background barks", () => {
  it.effect("queries answer at every await, mid-send included, and change nothing; a static bark reads from the bundle", () =>
    Effect.gen(function* () {
      const root = yield* compileBundle(barkFixtureRoot)
      const hold = yield* Ref.make<Hold | undefined>(undefined)
      const sources = Layer.mergeAll(LuteBun.runtime, LuteBun.bundle(root), heldLog(hold))
      yield* Effect.gen(function* () {
        const session = yield* Session.Session
        const source = yield* BundleSource.BundleSource
        const bundle = yield* source.load
        const queriesLeaveStateAlone = (expected: Contract.Await["type"]) => Effect.gen(function* () {
          const point = yield* SubscriptionRef.get(session.current)
          expect(point.await.type).toBe(expected)
          const before = canonical(yield* session.program.snapshot(point.state))
          const candidates = yield* session.program.candidates(point.state, "ambient", "npc.guide")
          yield* session.program.evaluate(point.state, { path: "run.finished" })
          expect(yield* session.program.def(point.state, "ready")).toEqual({ value: { kind: "bool", value: true } })
          yield* session.program.gate(point.state, "ambient", "npc.guide")
          yield* session.program.view(point.state)
          expect(canonical(yield* session.program.snapshot(point.state))).toBe(before)
          expect(canonical(yield* session.program.snapshot((yield* SubscriptionRef.get(session.current)).state))).toBe(before)
          return candidates
        })

        yield* session.send(raise("arrival"))
        const atChoice = yield* queriesLeaveStateAlone("awaitChoice")
        expect(atChoice.map((candidate) => candidate.id)).toEqual(["writingBark", "staticBark", "dynamicBark"])
        const shown = atChoice.flatMap((candidate) => staticBarkLines(bundle, candidate) ?? [])
        expect(shown).toEqual([{
          text: "The lantern is warm.",
          speaker: "Guide",
          voiceKey: "staticBark.guide-ambient.static",
          lineId: "staticBark.guide_ambient.static"
        }])

        // A send stopped after the step but before it commits: queries read
        // the last committed state (still the choice).
        const held: Hold = { reached: yield* Deferred.make<void>(), release: yield* Deferred.make<void>() }
        yield* Ref.set(hold, held)
        const request = (yield* SubscriptionRef.get(session.current)).await
        if (request.type !== "awaitChoice") throw new Error("expected a choice")
        const choosing = yield* Effect.forkChild(session.send({ type: "choose", request: request.request, option: "stay" }))
        yield* Deferred.await(held.reached)
        yield* queriesLeaveStateAlone("awaitChoice")
        yield* Ref.set(hold, undefined)
        yield* Deferred.succeed(held.release, undefined)
        yield* Fiber.join(choosing)

        yield* queriesLeaveStateAlone("awaitBridge")
        const bridge = (yield* SubscriptionRef.get(session.current)).await
        if (bridge.type !== "awaitBridge") throw new Error("expected a bridge")
        yield* session.send({ type: "bridgeResult", request: bridge.request, fields: { outcome: "ok" } })
        yield* queriesLeaveStateAlone("idle")

        // The recording bark is raised once the story is back at idle: it is
        // presented and its write applies (here: the run ends).
        const raised = yield* session.send(raise("ambient"))
        expect(JSON.stringify(raised.events)).toContain("writingBark.guide_ambient.writing")
        const ended = yield* SubscriptionRef.get(session.current)
        expect(yield* session.program.evaluate(ended.state, { path: "run.finished" })).toEqual({ value: { kind: "bool", value: true } })
        yield* queriesLeaveStateAlone("ended")
      }).pipe(Effect.provide(Layer.provideMerge(Session.layer({ seed: { derive: true } }), sources)))
    }).pipe(Effect.scoped))
})
