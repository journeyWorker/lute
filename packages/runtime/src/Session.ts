/**
 * One running playthrough (spec 0.39.0 §5.2): the host sends inputs, watches
 * the state, and subscribes to events and awaits. The session adds no game
 * policy — it answers nothing and paces nothing.
 */
import { Context, Effect, Layer, PubSub, Scope, Semaphore, Stream, SubscriptionRef } from "effect"
import { BundleSource } from "./BundleSource.js"
import type { BundleError, InputLogError, RuntimeRejection } from "./Errors.js"
import type { Await, Event, Input, Output, Seed, Snapshot } from "./generated/Contract.js"
import { InputLog } from "./InputLog.js"
import { LuteRuntime } from "./LuteRuntime.js"
import type { RuntimeState } from "./Program.js"

/** Where the session stands: its state and what it waits for. */
export interface SessionState {
  readonly state: RuntimeState
  readonly await: Await
}

export class Session extends Context.Service<
  Session,
  {
    /** The program's fingerprint (what snapshots carry). */
    readonly fingerprint: string
    /** The output of `begin`; for a restored session, no events and the snapshot's await. */
    readonly started: Output
    /**
     * Apply one input. Inputs run one at a time, in call order. An accepted
     * input is logged before its events are published; a rejected one
     * changes nothing.
     */
    readonly send: (input: Input) => Effect.Effect<Output, RuntimeRejection | InputLogError>
    readonly current: SubscriptionRef.SubscriptionRef<SessionState>
    /**
     * Subscribe to the events of every later output, in order. The
     * subscription exists once this effect completes, so nothing published
     * afterwards is missed; it ends with its scope or the session's.
     */
    readonly events: Effect.Effect<Stream.Stream<Event>, never, Scope.Scope>
    /** Subscribe to the await of every later output, like `events`. */
    readonly awaits: Effect.Effect<Stream.Stream<Await>, never, Scope.Scope>
    readonly save: Effect.Effect<Snapshot>
    /** Continue from a snapshot; the input log restarts from it. */
    readonly restore: (snapshot: Snapshot) => Effect.Effect<void, RuntimeRejection | InputLogError>
  }
>()("@lute-lang/runtime/Session") {}

/** How a session starts: from a seed, or from a snapshot. */
export type SessionOptions = { readonly seed: Seed } | { readonly snapshot: Snapshot }

/**
 * A session over the bundle of `BundleSource`. The program it loads lives
 * as long as the layer's scope; closing it ends `events` and `awaits`.
 */
export const layer = (
  options: SessionOptions
): Layer.Layer<Session, RuntimeRejection | BundleError | InputLogError, LuteRuntime | BundleSource | InputLog> =>
  Layer.effect(
    Session,
    Effect.gen(function* () {
      const runtime = yield* LuteRuntime
      const log = yield* InputLog
      const bundle = yield* (yield* BundleSource).load
      const program = yield* runtime.load(bundle)

      const started: { readonly state: RuntimeState; readonly output: Output } =
        "seed" in options
          ? yield* program.begin(options.seed)
          : {
              state: yield* program.restore(options.snapshot),
              output: { eventVersion: options.snapshot.snapshotVersion, events: [], await: options.snapshot.await }
            }
      yield* log.reset(options)

      const current = yield* SubscriptionRef.make<SessionState>({
        state: started.state,
        await: started.output.await
      })
      const events = yield* PubSub.unbounded<Event>()
      const awaits = yield* PubSub.unbounded<Await>()
      yield* Effect.addFinalizer(() => Effect.andThen(PubSub.shutdown(events), PubSub.shutdown(awaits)))
      const lock = yield* Semaphore.make(1)

      const send = Effect.fn("Lute.Session.send")(function* (input: Input) {
        const { state } = yield* SubscriptionRef.get(current)
        const step = yield* program.step(state, input)
        yield* log.append({ input })
        yield* SubscriptionRef.set(current, { state: step.state, await: step.output.await })
        yield* PubSub.publishAll(events, step.output.events)
        yield* PubSub.publish(awaits, step.output.await)
        return step.output
      })

      const restore = Effect.fn("Lute.Session.restore")(function* (snapshot: Snapshot) {
        const state = yield* program.restore(snapshot)
        yield* log.reset({ snapshot })
        yield* SubscriptionRef.set(current, { state, await: snapshot.await })
        yield* PubSub.publish(awaits, snapshot.await)
      })

      return {
        fingerprint: program.fingerprint,
        started: started.output,
        send: (input) => lock.withPermits(1)(send(input)),
        current,
        events: Effect.map(PubSub.subscribe(events), Stream.fromSubscription),
        awaits: Effect.map(PubSub.subscribe(awaits), Stream.fromSubscription),
        save: SubscriptionRef.get(current).pipe(
          Effect.flatMap(({ state }) => program.snapshot(state)),
          Effect.withSpan("Lute.Session.save")
        ),
        restore: (snapshot) => lock.withPermits(1)(restore(snapshot))
      }
    })
  )
