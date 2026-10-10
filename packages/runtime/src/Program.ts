/**
 * A loaded bundle and the runtime's step function over it (spec 0.39.0 §5.1).
 *
 * Every value crossing the binding is encoded and decoded with the contract
 * schemas generated from the Rust types. JSON the binding returns that does
 * not decode is contract drift — a defect.
 */
import { Effect, Option, Schema } from "effect"
import * as Errors from "./Errors.js"
import * as Contract from "./generated/Contract.js"
import type * as Glue from "./wasm/lute_runtime_wasm.js"

const TypeId = "~@lute-lang/runtime/RuntimeState"

/**
 * An opaque, immutable runtime state. Stepping from it never invalidates it,
 * so a host may keep earlier states (undo, branching). Its serialized form
 * is {@link Program.snapshot}.
 */
export interface RuntimeState {
  readonly [TypeId]: typeof TypeId
}

const handles = new WeakMap<RuntimeState, Glue.State>()

const wrap = (handle: Glue.State): RuntimeState => {
  const state: RuntimeState = { [TypeId]: TypeId }
  handles.set(state, handle)
  return state
}

const handleOf = (state: RuntimeState): Effect.Effect<Glue.State> => {
  const handle = handles.get(state)
  return handle === undefined
    ? Effect.die(new Error("a RuntimeState must come from a Program of this package"))
    : Effect.succeed(handle)
}

/** One accepted input: the state it produced and the runtime's output. */
export interface Step {
  readonly state: RuntimeState
  readonly output: Contract.Output
}

export interface Program {
  /** The bundle's identity, which snapshots carry. */
  readonly fingerprint: string
  readonly begin: (seed: Contract.Seed) => Effect.Effect<Step, Errors.RuntimeRejection>
  readonly step: (state: RuntimeState, input: Contract.Input) => Effect.Effect<Step, Errors.RuntimeRejection>
  readonly snapshot: (state: RuntimeState) => Effect.Effect<Contract.Snapshot>
  readonly restore: (snapshot: Contract.Snapshot) => Effect.Effect<RuntimeState, Errors.RuntimeRejection>
  readonly candidates: (
    state: RuntimeState,
    occasion: string,
    target?: string
  ) => Effect.Effect<ReadonlyArray<Contract.Candidate>>
  readonly eligibility: (
    state: RuntimeState,
    beat: string,
    member?: string
  ) => Effect.Effect<Option.Option<Contract.Candidate>>
  readonly clock: (state: RuntimeState) => Effect.Effect<Option.Option<Contract.ClockAt>>
  readonly terminal: (state: RuntimeState) => Effect.Effect<boolean>
  readonly view: (state: RuntimeState, options?: { readonly facts?: boolean }) => Effect.Effect<Contract.WorldView>
}

/** Decode JSON the binding returned; a mismatch dies. */
const fromBinding = <S extends Schema.Codec<unknown, unknown, never, never>>(schema: S) => {
  const decode = Schema.decodeUnknownEffect(Schema.fromJsonString(schema), Contract.parseOptions)
  return (json: string): Effect.Effect<S["Type"]> => Effect.orDie(decode(json))
}

/** Encode a host value as the JSON the binding reads. */
const toBinding = <S extends Schema.Codec<unknown, unknown, never, never>>(schema: S) => {
  const encode = Schema.encodeEffect(Schema.fromJsonString(schema), Contract.parseOptions)
  return (value: S["Type"]): Effect.Effect<string> => Effect.orDie(encode(value))
}

const decodeOutput = fromBinding(Contract.Output)
const decodeRejected = fromBinding(Contract.Rejected)
const decodeSnapshot = fromBinding(Contract.Snapshot)
const decodeCandidates = fromBinding(Schema.Array(Contract.Candidate))
const decodeCandidate = fromBinding(Schema.NullOr(Contract.Candidate))
const decodeClock = fromBinding(Schema.NullOr(Contract.ClockAt))
const decodeView = fromBinding(Contract.WorldView)
const encodeSeed = toBinding(Contract.Seed)
const encodeInput = toBinding(Contract.Input)
const encodeSnapshot = toBinding(Contract.Snapshot)

/** Fail with a rejection the binding returned as JSON. */
export const rejection = (json: string): Effect.Effect<never, Errors.RuntimeRejection> =>
  Effect.flatMap(decodeRejected(json), Errors.fromRejected)

/**
 * Read a binding result object and free it. The binding returns either the
 * success fields or `rejected`; anything else violates its contract.
 */
const settle = <A>(
  result: { readonly ok: boolean; readonly rejected: string | undefined; free(): void },
  success: () => Effect.Effect<A>
): Effect.Effect<A, Errors.RuntimeRejection> =>
  Effect.suspend(() => {
    const { ok, rejected } = result
    if (ok) return success()
    if (rejected !== undefined) return rejection(rejected)
    return Effect.die(new Error("the runtime binding returned neither a value nor a rejection"))
  }).pipe(Effect.ensuring(Effect.sync(() => result.free())))

const stepOf = (result: Glue.StepResult): Effect.Effect<Step, Errors.RuntimeRejection> =>
  settle(result, () => {
    const { state, output } = result
    return state === undefined || output === undefined
      ? Effect.die(new Error("the runtime binding accepted an input without a state and output"))
      : Effect.map(decodeOutput(output), (output) => ({ state: wrap(state), output }))
  })

/** Wrap a program the binding loaded. The caller owns its lifetime ({@link free}). */
export const make = (raw: Glue.Program): Program & { readonly free: Effect.Effect<void> } => ({
  fingerprint: raw.fingerprint,

  begin: Effect.fn("Lute.Program.begin")(function* (seed: Contract.Seed) {
    const json = yield* encodeSeed(seed)
    return yield* stepOf(raw.begin(json))
  }),

  step: Effect.fn("Lute.Program.step")(function* (state: RuntimeState, input: Contract.Input) {
    const handle = yield* handleOf(state)
    const json = yield* encodeInput(input)
    return yield* stepOf(raw.step(handle, json))
  }),

  snapshot: Effect.fn("Lute.Program.snapshot")(function* (state: RuntimeState) {
    const handle = yield* handleOf(state)
    return yield* decodeSnapshot(raw.snapshot(handle))
  }),

  restore: Effect.fn("Lute.Program.restore")(function* (snapshot: Contract.Snapshot) {
    const json = yield* encodeSnapshot(snapshot)
    const result = raw.restore(json)
    return yield* settle(result, () => {
      const { state } = result
      return state === undefined
        ? Effect.die(new Error("the runtime binding restored a snapshot without a state"))
        : Effect.succeed(wrap(state))
    })
  }),

  candidates: Effect.fn("Lute.Program.candidates")(function* (
    state: RuntimeState,
    occasion: string,
    target?: string
  ) {
    const handle = yield* handleOf(state)
    return yield* decodeCandidates(raw.candidates(handle, occasion, target))
  }),

  eligibility: Effect.fn("Lute.Program.eligibility")(function* (
    state: RuntimeState,
    beat: string,
    member?: string
  ) {
    const handle = yield* handleOf(state)
    const candidate = yield* decodeCandidate(raw.eligibility(handle, beat, member))
    return Option.fromNullOr(candidate)
  }),

  clock: Effect.fn("Lute.Program.clock")(function* (state: RuntimeState) {
    const handle = yield* handleOf(state)
    const clock = yield* decodeClock(raw.clock(handle))
    return Option.fromNullOr(clock)
  }),

  terminal: Effect.fn("Lute.Program.terminal")(function* (state: RuntimeState) {
    const handle = yield* handleOf(state)
    return raw.terminal(handle)
  }),

  view: Effect.fn("Lute.Program.view")(function* (
    state: RuntimeState,
    options?: { readonly facts?: boolean }
  ) {
    const handle = yield* handleOf(state)
    return yield* decodeView(raw.view(handle, options?.facts === true))
  }),

  free: Effect.sync(() => raw.free())
})
