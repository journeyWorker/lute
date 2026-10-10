/**
 * Replaying a recorded input log (spec 0.39.0 §5.4). Against the bundle
 * that recorded it, a log reproduces every output.
 */
import { Effect, Schema, Stream } from "effect"
import type { RuntimeRejection } from "./Errors.js"
import type { Output } from "./generated/Contract.js"
import { LogEntry } from "./InputLog.js"
import type { Program, RuntimeState } from "./Program.js"

const decodeLine = Schema.decodeUnknownEffect(Schema.fromJsonString(LogEntry))

/** Parse an `inputs.jsonl` text: one log entry per non-empty line. */
export const parseLog = (text: string): Effect.Effect<ReadonlyArray<LogEntry>, Schema.SchemaError> =>
  Effect.forEach(
    text.split("\n").filter((line) => line.trim().length > 0),
    (line) => decodeLine(line)
  )

/**
 * The outputs of a log: `begin` for a `seed` start (a `snapshot` start
 * restores and emits nothing), then one output per `input`. A log that does
 * not start with a seed or snapshot, or restarts midway, dies.
 */
export const run = (program: Program, entries: ReadonlyArray<LogEntry>): Stream.Stream<Output, RuntimeRejection> => {
  const [start, ...inputs] = entries
  const begun: Effect.Effect<{ readonly state: RuntimeState; readonly outputs: ReadonlyArray<Output> }, RuntimeRejection> =
    start === undefined || "input" in start
      ? Effect.die(new Error("an input log starts with a seed or a snapshot"))
      : "seed" in start
        ? Effect.map(program.begin(start.seed), ({ state, output }) => ({ state, outputs: [output] }))
        : Effect.map(program.restore(start.snapshot), (state) => ({ state, outputs: [] }))
  return Stream.unwrap(
    Effect.map(begun, ({ state, outputs }) =>
      Stream.fromIterable(inputs).pipe(
        Stream.mapAccumEffect(
          () => state,
          (state, entry) =>
            "input" in entry
              ? Effect.map(program.step(state, entry.input), (step) => [step.state, [step.output]] as const)
              : Effect.die(new Error("an input log has one seed or snapshot, at its start"))
        ),
        Stream.prepend(outputs)
      )
    )
  )
}
