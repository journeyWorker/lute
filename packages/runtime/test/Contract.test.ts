import { readFile, readdir } from "node:fs/promises";
import { fileURLToPath } from "node:url";
import { resolve } from "node:path";
import * as Effect from "effect/Effect";
import * as Schema from "effect/Schema";
import { describe, expect, it } from "@effect/vitest";
import {
  Input,
  Seed,
  Snapshot,
  StreamLine,
  Verdict,
  parseOptions
} from "../src/generated/Contract.ts";
const repositoryRoot = fileURLToPath(new URL("../../..", import.meta.url));
const sessionRoot = resolve(repositoryRoot, "conformance/session");
const inputLine = Schema.Union([
  Schema.Struct({ seed: Seed }),
  Schema.Struct({ input: Input })
]);

type TestSchema = Schema.ConstraintDecoder<unknown> & Schema.ConstraintEncoder<unknown>;
type ContractCase = {
  readonly schema: TestSchema;
  readonly value: unknown;
  readonly source: string;
  readonly line: number;
};

const canonical = (value: unknown): unknown => {
  if (Array.isArray(value)) return value.map(canonical);
  if (value !== null && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value)
        .sort(([left], [right]) => left.localeCompare(right))
        .map(([key, entry]) => [key, canonical(entry)])
    );
  }
  return value;
};

const canonicalJson = (value: unknown): string => JSON.stringify(canonical(value));
const decode = (schema: TestSchema, value: unknown): unknown => Schema.decodeUnknownSync(schema, parseOptions)(value);
const encode = (schema: TestSchema, value: unknown): unknown => Schema.encodeSync(schema, parseOptions)(value);
const readLines = async (path: string): Promise<readonly unknown[]> => {
  const content = await readFile(path, "utf8");
  return content
    .split("\n")
    .filter((line) => line.trim().length > 0)
    .map((line) => JSON.parse(line));
};

const sessionCases = async (): Promise<readonly ContractCase[]> => {
  const directories = (await readdir(sessionRoot, { withFileTypes: true }))
    .filter((entry) => entry.isDirectory())
    .map((entry) => entry.name);
  const cases: ContractCase[] = [];
  for (const directory of directories) {
    for (const filename of ["expected.jsonl", "inputs.jsonl"] as const) {
      const source = resolve(sessionRoot, directory, filename);
      const lines = await readLines(source);
      lines.forEach((value, index) => {
        cases.push({
          schema: filename === "expected.jsonl" ? StreamLine : inputLine,
          value,
          source,
          line: index + 1
        });
      });
    }
  }
  return cases;
};

const fixtureCases = async (): Promise<readonly ContractCase[]> => {
  const source = resolve(sessionRoot, "contract-fixtures.jsonl");
  const lines = await readLines(source);
  return lines.map((value, index) => ({
    schema: typeof value === "object" && value !== null && "type" in value
      ? Input
      : typeof value === "object" && value !== null && "verdict" in value
        ? Verdict
        : Snapshot,
    value,
    source,
    line: index + 1
  }));
};

const allCases = await sessionCases();
const allFixtures = await fixtureCases();

describe("generated contract schemas", () => {
  it.effect("round-trips every session JSONL line", () => Effect.sync(() => {
    for (const testCase of allCases) {
      const decoded = decode(testCase.schema, testCase.value);
      const encoded = encode(testCase.schema, decoded);
      expect(canonicalJson(encoded), `${testCase.source}:${testCase.line}`).toBe(canonicalJson(testCase.value));
    }
  }));

  it.effect("round-trips every contract fixture", () => Effect.sync(() => {
    for (const testCase of allFixtures) {
      const decoded = decode(testCase.schema, testCase.value);
      const encoded = encode(testCase.schema, decoded);
      expect(canonicalJson(encoded), `${testCase.source}:${testCase.line}`).toBe(canonicalJson(testCase.value));
    }
  }));

  it.effect("rejects malformed contract values", () => Effect.sync(() => {
    expect(() => decode(Input, { type: "unknown" })).toThrow();
    expect(() => decode(Input, { type: "choose", request: 0 })).toThrow();
    expect(() => decode(Input, { type: "choose", request: 0, option: "ok", extra: true })).toThrow();
    expect(() => decode(Input, { type: "choose", request: -1, option: "ok" })).toThrow();
  }));
});
