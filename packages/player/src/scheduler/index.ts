import { Context, Deferred, Effect, Fiber, Layer, PubSub, Stream, SubscriptionRef } from "effect"
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
    const pending: Array<Deferred.Deferred<void>> = []
    const publish = (event: SchedulerEvent): Effect.Effect<void> => PubSub.publish(events, event)

    // A forked cue's gate opens in the fiber that runs its `execute` (skip:
    // `complete`), as that effect begins, or when its fiber exits; `play`
    // waits for every gate before it settles.
    const skipped = Effect.as(
      Stream.runHead(Stream.filter(SubscriptionRef.changes(mode), (value) => value === "skip")),
      "skip" as const
    )

    const runCue = (cue: Cue, index: number, timelineOffset = 0, gateOpen?: Deferred.Deferred<void>): Effect.Effect<void> => Effect.gen(function* () {
      const begin = gateOpen === undefined ? Effect.void : Deferred.succeed(gateOpen, undefined)
      const skip = Effect.gen(function* () {
        yield* begin
        yield* cue.complete(ctx)
        if (cue.kind === "line" && cue.line !== undefined) {
          yield* publish({ _tag: "LineShown", line: cue.line })
          yield* gate.release(cue.line)
        }
        yield* publish({ _tag: "CueDone", index, cue })
      })
      if ((yield* SubscriptionRef.get(mode)) === "skip") return yield* skip
      // Switching to skip while the cue waits out its delay completes it at once.
      const wait = cue.timing.delay + timelineOffset
      if (wait > 0 && (yield* Effect.raceFirst(Effect.as(sleep(wait), "normal" as const), skipped)) === "skip") return yield* skip
      yield* publish({ _tag: "CueStarted", index, cue })
      if (cue.kind === "line" && cue.line !== undefined) {
        // The line types until it is fully shown (or revealed); switching to
        // skip while it types completes it at once. Then the gate holds the
        // batch until the line is released.
        yield* gate.started(cue.line)
        const typed = Effect.as(Effect.andThen(begin, cue.execute(ctx)), "normal" as const)
        if ((yield* Effect.raceFirst(typed, skipped)) === "skip") yield* Effect.andThen(begin, cue.complete(ctx))
        yield* publish({ _tag: "LineShown", line: cue.line })
        yield* gate.release(cue.line)
      } else {
        const normal = Effect.as(Effect.andThen(begin, cue.execute(ctx)), "normal" as const)
        const outcome = yield* Effect.raceFirst(normal, skipped)
        if (outcome === "skip") yield* Effect.andThen(begin, cue.complete(ctx))
      }
      yield* publish({ _tag: "CueDone", index, cue })
    })

    /** Forks `run` into the batch scope with one gate per cue it runs; `play` waits for the gates. */
    const fork = (count: number, run: (gates: ReadonlyArray<Deferred.Deferred<void>>) => Effect.Effect<void>) => Effect.gen(function* () {
      const gates = yield* Effect.forEach(Array.from({ length: count }), () => Deferred.make<void>())
      pending.push(...gates)
      const openAll = Effect.forEach(gates, (gateOpen) => Deferred.succeed(gateOpen, undefined), { discard: true })
      return yield* Effect.forkScoped(Effect.ensuring(run(gates), openAll))
    })

    const runTimeline = (
      clips: ReadonlyArray<{ readonly cue: Cue; readonly index: number }>,
      gates: ReadonlyArray<Deferred.Deferred<void>>
    ): Effect.Effect<void> => Effect.forEach(
      clips,
      ({ cue, index }, position) => runCue(cue, index, cue.timing.at ?? 0, gates[position]),
      { concurrency: "unbounded", discard: true }
    )

    for (const [index, cue] of cues.entries()) {
      const timeline = cue.timing.timeline
      if (timeline !== undefined) {
        if (!started.has(timeline)) {
          const clips = groups.get(timeline) ?? []
          const fiber = yield* fork(clips.length, (gates) => runTimeline(clips, gates))
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
      if (cue.timing.wait || cue.kind === "line" || cue.kind === "sequence") yield* runCue(cue, index)
      else {
        const fiber = yield* fork(1, ([gateOpen]) => runCue(cue, index, 0, gateOpen))
        if (cue.stage) settledStage.push(fiber)
      }
    }
    // Every forked cue has started (or its fiber has exited) before the batch settles.
    for (const gateOpen of pending) yield* Deferred.await(gateOpen)
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
