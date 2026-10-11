import { Context, Effect, Fiber, Layer, PubSub, Stream, SubscriptionRef } from "effect"
import type { Cue, CueContext, CueSchedulerShape, LineGate, SchedulerEvent } from "../model.js"
import type { PlayMode as PortPlayMode } from "../ports/index.js"

interface RunningGroup {
  readonly fiber: Fiber.Fiber<void, never>
  readonly stage: boolean
}

const sleep = (seconds: number): Effect.Effect<void> => seconds > 0 ? Effect.sleep(seconds) : Effect.void

/** Runs one output's cues against Clock and the renderer ports. */
export class CueScheduler extends Context.Service<CueScheduler, CueSchedulerShape>()("Lute.Player.CueScheduler") {}

const makeScheduler = (events: PubSub.PubSub<SchedulerEvent>): CueSchedulerShape => {
  const play = Effect.fn("Lute.Player.CueScheduler.play")(function* (
    cues: ReadonlyArray<Cue>,
    ctx: CueContext,
    mode: SubscriptionRef.SubscriptionRef<PortPlayMode>,
    gate: LineGate
  ) {
    const groups = new Map<number, ReadonlyArray<{ readonly cue: Cue; readonly index: number }>>()
    for (const [index, cue] of cues.entries()) {
      if (cue.timing.timeline === undefined) continue
      const prior = groups.get(cue.timing.timeline) ?? []
      groups.set(cue.timing.timeline, [...prior, { cue, index }])
    }
    const started = new Map<number, RunningGroup>()
    const settledStage: Array<Fiber.Fiber<void, never>> = []
    const publish = (event: SchedulerEvent): Effect.Effect<void> => PubSub.publish(events, event)

    const runCue = (cue: Cue, index: number, timelineOffset = 0): Effect.Effect<void> => Effect.gen(function* () {
      const current = yield* SubscriptionRef.get(mode)
      if (current === "skip") {
        yield* cue.complete(ctx)
        if (cue.kind === "line" && cue.line !== undefined) {
          yield* publish({ _tag: "LineShown", line: cue.line })
          yield* gate.release(cue.line)
        }
        yield* publish({ _tag: "CueDone", index, cue })
        return
      }
      if (cue.timing.delay + timelineOffset > 0) yield* sleep(cue.timing.delay + timelineOffset)
      yield* publish({ _tag: "CueStarted", index, cue })
      const normal = Effect.as(cue.execute(ctx), "normal")
      const switched = Effect.as(
        Stream.runHead(Stream.filter(SubscriptionRef.changes(mode), (value) => value === "skip")),
        "skip"
      )
      const outcome = yield* Effect.raceFirst(normal, switched)
      if (outcome === "skip") yield* cue.complete(ctx)
      if (cue.kind === "line" && cue.line !== undefined) {
        yield* publish({ _tag: "LineShown", line: cue.line })
        yield* gate.release(cue.line)
      }
      yield* publish({ _tag: "CueDone", index, cue })
    })

    const runTimeline = (clips: ReadonlyArray<{ readonly cue: Cue; readonly index: number }>): Effect.Effect<void> => Effect.forEach(
      clips,
      ({ cue, index }) => runCue(cue, index, cue.timing.at ?? 0),
      { concurrency: "unbounded", discard: true }
    )

    for (const [index, cue] of cues.entries()) {
      const timeline = cue.timing.timeline
      if (timeline !== undefined) {
        if (!started.has(timeline)) {
          const clips = groups.get(timeline) ?? []
          const fiber = yield* Effect.forkScoped(runTimeline(clips))
          const group: RunningGroup = { fiber, stage: clips.some(({ cue: clip }) => clip.stage) }
          started.set(timeline, group)
          if (group.stage) settledStage.push(fiber)
        }
        if (cue.kind === "barrier") {
          const group = started.get(timeline)
          if (group !== undefined) yield* Fiber.join(group.fiber)
        }
        continue
      }
      const cueEffect = runCue(cue, index)
      if (cue.timing.wait || cue.kind === "line" || cue.kind === "sequence") yield* cueEffect
      else {
        const fiber = yield* Effect.forkScoped(cueEffect)
        if (cue.stage) settledStage.push(fiber)
      }
    }
    for (const fiber of settledStage) yield* Fiber.join(fiber)
  })

  return { play, events: Stream.fromPubSub(events) }
}

/** Installs a scheduler with an event bus. */
export const layer: Layer.Layer<CueScheduler> = Layer.effect(
  CueScheduler,
  Effect.flatMap(PubSub.unbounded<SchedulerEvent>(), (events) => Effect.succeed(makeScheduler(events)))
)

/** Creates a scheduler value with an independent event bus. */
export const make = Effect.fn("Lute.Player.CueScheduler.make")(function* () {
  const events = yield* PubSub.unbounded<SchedulerEvent>()
  return makeScheduler(events)
})
