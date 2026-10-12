/**
 * A loaded bundle and the runtime's step function over it (spec 0.40.0 §5.1,
 * 0.41.0 §6).
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
 *
 * A state is valid for every `Program` loaded from a bundle with the
 * fingerprint of the program that created it (spec 0.41.0 §6.2): a host may
 * query the state of a session it does not drive with its own program of
 * that bundle. Handing it to a program of another fingerprint is a defect.
 */
export interface RuntimeState {
  readonly [TypeId]: typeof TypeId
}

/** A state's binding handle and the fingerprint of the program that created it. */
interface Owned {
  readonly handle: Glue.State
  readonly fingerprint: string
}

const handles = new WeakMap<RuntimeState, Owned>()

const wrap = (handle: Glue.State, fingerprint: string): RuntimeState => {
  const state: RuntimeState = { [TypeId]: TypeId }
  handles.set(state, { handle, fingerprint })
  return state
}

/** The binding handle of `state` for a program of `fingerprint`; any other state dies. */
const handleOf = (state: RuntimeState, fingerprint: string): Effect.Effect<Glue.State> =>
  Effect.suspend(() => {
    const owned = handles.get(state)
    if (owned === undefined) {
      return Effect.die(new Error("a RuntimeState must come from a Program of this package"))
    }
    if (owned.fingerprint !== fingerprint) {
      return Effect.die(
        new Error(
          `a RuntimeState of bundle ${owned.fingerprint} was given to a Program of bundle ${fingerprint}: ` +
            "a state is valid only for programs of the bundle fingerprint that created it"
        )
      )
    }
    return Effect.succeed(owned.handle)
  })

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
  readonly stage: (state: RuntimeState) => Effect.Effect<Contract.Stage>
  readonly view: (state: RuntimeState, options?: { readonly facts?: boolean }) => Effect.Effect<Contract.WorldView>
  /**
   * Evaluate an IR `exprNode` — the `expr` of any `{cel, expr}` pair the IR
   * carries, such as an objective's `visibleWhen.expr`, or the output of
   * `lute expr` — over the state, as the runtime's walk reads a guard (spec
   * 0.41.0 §5.1). A `param` node is not admitted: it is `InvalidInput`.
   */
  readonly evaluate: (
    state: RuntimeState,
    expr: Schema.Json
  ) => Effect.Effect<Contract.Evaluation, Errors.QueryRejection>
  /**
   * Evaluate the bundle's index def `name` over the state, binding its
   * parameters to `args` in declaration order — one JSON scalar each, absent
   * for a parameterless def (spec 0.41.0 §5.1). A wrong arity or an argument
   * that does not fit its parameter type is `InvalidInput`.
   */
  readonly def: (
    state: RuntimeState,
    name: string,
    args?: ReadonlyArray<string | number | boolean>
  ) => Effect.Effect<Contract.Evaluation, Errors.QueryRejection>
  /**
   * Whether a `raiseOccasion` of `occasion` (for `target`) with no writes
   * would pass the seam now (spec 0.41.0 §5.2): `open: false` without
   * `unresolved` is decided closed (its `raisedWhen` is false, or the game
   * is over and the occasion is not `outsideRun`); with `unresolved`, it is
   * undecided. An occasion the bundle does not declare is `InvalidInput`.
   */
  readonly gate: (
    state: RuntimeState,
    occasion: string,
    target?: string
  ) => Effect.Effect<Contract.Gate, Errors.InvalidInput>
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
const decodeStage = fromBinding(Contract.Stage)
const decodeClock = fromBinding(Schema.NullOr(Contract.ClockAt))
const decodeView = fromBinding(Contract.WorldView)
const decodeEvaluation = fromBinding(Contract.Evaluation)
const decodeGate = fromBinding(Contract.Gate)
const encodeExpr = toBinding(Schema.Json)
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

/** Read a query's result object (`evaluate`, `def`, `gate`) and free it. */
const queryOf = <A, E>(
  result: Glue.QueryResult,
  decode: (json: string) => Effect.Effect<A>,
  reject: (rejected: Contract.Rejected) => Effect.Effect<never, E>
): Effect.Effect<A, E> =>
  Effect.suspend(() => {
    const { ok, value, rejected } = result
    if (ok && value !== undefined) return decode(value)
    if (!ok && rejected !== undefined) return Effect.flatMap(decodeRejected(rejected), reject)
    return Effect.die(new Error("the runtime binding returned neither an answer nor a rejection"))
  }).pipe(Effect.ensuring(Effect.sync(() => result.free())))

/** Wrap a program the binding loaded. The caller owns its lifetime ({@link free}). */
export const make = (raw: Glue.Program): Program & { readonly free: Effect.Effect<void> } => {
  const fingerprint = raw.fingerprint
  const own = (state: RuntimeState) => handleOf(state, fingerprint)

  const stepOf = (result: Glue.StepResult): Effect.Effect<Step, Errors.RuntimeRejection> =>
    settle(result, () => {
      const { state, output } = result
      return state === undefined || output === undefined
        ? Effect.die(new Error("the runtime binding accepted an input without a state and output"))
        : Effect.map(decodeOutput(output), (output) => ({ state: wrap(state, fingerprint), output }))
    })

  return {
    fingerprint,

    begin: Effect.fn("Lute.Program.begin")(function* (seed: Contract.Seed) {
      const json = yield* encodeSeed(seed)
      return yield* stepOf(raw.begin(json))
    }),

    step: Effect.fn("Lute.Program.step")(function* (state: RuntimeState, input: Contract.Input) {
      const handle = yield* own(state)
      const json = yield* encodeInput(input)
      return yield* stepOf(raw.step(handle, json))
    }),

    snapshot: Effect.fn("Lute.Program.snapshot")(function* (state: RuntimeState) {
      const handle = yield* own(state)
      return yield* decodeSnapshot(raw.snapshot(handle))
    }),

    restore: Effect.fn("Lute.Program.restore")(function* (snapshot: Contract.Snapshot) {
      const json = yield* encodeSnapshot(snapshot)
      const result = raw.restore(json)
      return yield* settle(result, () => {
        const { state } = result
        return state === undefined
          ? Effect.die(new Error("the runtime binding restored a snapshot without a state"))
          : Effect.succeed(wrap(state, fingerprint))
      })
    }),

    candidates: Effect.fn("Lute.Program.candidates")(function* (
      state: RuntimeState,
      occasion: string,
      target?: string
    ) {
      const handle = yield* own(state)
      return yield* decodeCandidates(raw.candidates(handle, occasion, target))
    }),

    eligibility: Effect.fn("Lute.Program.eligibility")(function* (
      state: RuntimeState,
      beat: string,
      member?: string
    ) {
      const handle = yield* own(state)
      const candidate = yield* decodeCandidate(raw.eligibility(handle, beat, member))
      return Option.fromNullOr(candidate)
    }),

    clock: Effect.fn("Lute.Program.clock")(function* (state: RuntimeState) {
      const handle = yield* own(state)
      const clock = yield* decodeClock(raw.clock(handle))
      return Option.fromNullOr(clock)
    }),

    terminal: Effect.fn("Lute.Program.terminal")(function* (state: RuntimeState) {
      const handle = yield* own(state)
      return raw.terminal(handle)
    }),

    stage: Effect.fn("Lute.Program.stage")(function* (state: RuntimeState) {
      const handle = yield* own(state)
      return yield* decodeStage(raw.stage(handle))
    }),

    view: Effect.fn("Lute.Program.view")(function* (
      state: RuntimeState,
      options?: { readonly facts?: boolean }
    ) {
      const handle = yield* own(state)
      return yield* decodeView(raw.view(handle, options?.facts === true))
    }),

    evaluate: Effect.fn("Lute.Program.evaluate")(function* (state: RuntimeState, expr: Schema.Json) {
      const handle = yield* own(state)
      const json = yield* encodeExpr(expr)
      return yield* queryOf(raw.evaluate(handle, json), decodeEvaluation, Errors.fromQueryRejected)
    }),

    def: Effect.fn("Lute.Program.def")(function* (
      state: RuntimeState,
      name: string,
      args?: ReadonlyArray<string | number | boolean>
    ) {
      const handle = yield* own(state)
      const json = args === undefined ? undefined : JSON.stringify(args)
      return yield* queryOf(raw.def(handle, name, json), decodeEvaluation, Errors.fromQueryRejected)
    }),

    gate: Effect.fn("Lute.Program.gate")(function* (state: RuntimeState, occasion: string, target?: string) {
      const handle = yield* own(state)
      return yield* queryOf(raw.gate(handle, occasion, target), decodeGate, Errors.fromGateRejected)
    }),

    free: Effect.sync(() => raw.free())
  }
}
