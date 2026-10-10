import { describe, expect, it } from "@effect/vitest"
import { Effect, Exit, Schema } from "effect"
import { join } from "node:path"
import * as Contract from "../src/generated/Contract.js"
import { canonical, jsonLines, sessionCases, sessionRoot } from "./support.js"

const InputsLine = Schema.Union([Schema.Struct({ seed: Contract.Seed }), Schema.Struct({ input: Contract.Input })])

/** Decode `value` with `schema` and encode it back; the JSON must not change. */
const roundTrip = <S extends Schema.Codec<unknown, unknown, never, never>>(schema: S, value: unknown, where: string) =>
  Effect.gen(function* () {
    const decoded = yield* Schema.decodeUnknownEffect(schema, Contract.parseOptions)(value)
    const encoded = yield* Schema.encodeEffect(schema, Contract.parseOptions)(decoded)
    expect(canonical(encoded), where).toBe(canonical(value))
  })

/** The schema of a hand-written contract fixture line, by its discriminant. */
const fixtureSchema = (line: unknown) =>
  typeof line === "object" && line !== null && "type" in line
    ? Contract.Input
    : typeof line === "object" && line !== null && "verdict" in line
      ? Contract.Verdict
      : Contract.Snapshot

describe("generated contract schemas", () => {
  it.effect("round-trip every session expected.jsonl and inputs.jsonl line", () =>
    Effect.forEach(sessionCases(), (session) =>
      Effect.andThen(
        Effect.forEach(jsonLines(session.expected), (line, index) =>
          roundTrip(Contract.StreamLine, line, `${session.expected}:${index + 1}`)
        ),
        Effect.forEach(jsonLines(session.inputs), (line, index) =>
          roundTrip(InputsLine, line, `${session.inputs}:${index + 1}`)
        )
      )
    )
  )

  it.effect("round-trip every contract fixture", () => {
    const fixtures = join(sessionRoot, "contract-fixtures.jsonl")
    return Effect.forEach(jsonLines(fixtures), (line, index) =>
      roundTrip(fixtureSchema(line), line, `${fixtures}:${index + 1}`)
    )
  })

  it.effect("refuse malformed values", () =>
    Effect.gen(function* () {
      const decode = Schema.decodeUnknownEffect(Contract.Input, Contract.parseOptions)
      for (const malformed of [
        { type: "unknown" },
        { type: "choose", request: 0 },
        { type: "choose", request: 0, option: "ok", extra: true },
        { type: "choose", request: -1, option: "ok" }
      ]) {
        expect(Exit.isFailure(yield* Effect.exit(decode(malformed))), JSON.stringify(malformed)).toBe(true)
      }
    })
  )
})
