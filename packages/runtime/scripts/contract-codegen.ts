import { readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

type JsonSchema = any;

const root = resolve(import.meta.dir, "../../..");
const schemaPaths = [
  resolve(root, "schemas/lute-events-0.39.schema.json"),
  resolve(root, "schemas/lute-snapshot-0.39.schema.json")
];
const outputPath = resolve(import.meta.dir, "../src/generated/Contract.ts");

const schemas = await Promise.all(schemaPaths.map(async (path) => JSON.parse(await readFile(path, "utf8")) as JsonSchema));
const defs = new Map<string, JsonSchema>();
const canonical = (value: unknown): unknown => {
  if (Array.isArray(value)) return value.map(canonical);
  if (value && typeof value === "object") {
    return Object.fromEntries(
      Object.entries(value)
        .filter(([key]) => key !== "title")
        .sort(([a], [b]) => a.localeCompare(b))
        .map(([key, entry]) => [key, canonical(entry)])
    );
  }
  return value;
};
for (const schema of schemas) {
  for (const [name, definition] of Object.entries(schema.$defs as Record<string, JsonSchema>)) {
    const prior = defs.get(name);
    if (prior && JSON.stringify(canonical(prior)) !== JSON.stringify(canonical(definition))) {
      throw new Error(`conflicting shared definition: ${name}`);
    }
    defs.set(name, definition);
  }
}

const pointer = (path: string): never => {
  throw new Error(`unsupported JSON Schema shape at ${path}`);
};
const quote = (value: unknown): string => JSON.stringify(value);
const refName = (ref: string, path: string): string => {
  const match = /^#\/\$defs\/([^/]+)$/.exec(ref);
  if (!match) return pointer(`${path}/$ref`);
  return match[1]!;
};
const refExpr = (ref: string, path: string): string => `Schema.suspend(() => ${refName(ref, path)})`;
function formatExpression(source: string): string[] {
  const lines: string[] = [];
  const stack: Array<{ close: string; multiline: boolean }> = [];
  let current = "";
  let indent = 0;
  let quoted: string | undefined;
  const pushLine = () => {
    if (current.trim()) lines.push(current.trimEnd());
    current = "  ".repeat(indent);
  };
  for (let index = 0; index < source.length; index += 1) {
    const character = source[index]!;
    if (quoted) {
      current += character;
      if (character === quoted && source[index - 1] !== "\\") quoted = undefined;
      continue;
    }
    if (character === "\"" || character === "'") {
      quoted = character;
      current += character;
      continue;
    }
    if (current.trim() === "" && /\s/.test(character)) continue;
    if (source.startsWith(".pipe(", index) || source.startsWith(".annotate(", index)) {
      pushLine();
      const token = source.startsWith(".pipe(", index) ? ".pipe(" : ".annotate(";
      current += token;
      stack.push({ close: ")", multiline: true });
      indent += 1;
      pushLine();
      index += token.length - 1;
      continue;
    }
    if (character === "{" || character === "[") {
      current += character;
      stack.push({ close: character === "{" ? "}" : "]", multiline: true });
      indent += 1;
      if (source[index + 1] !== (character === "{" ? "}" : "]")) pushLine();
      continue;
    }
    if (character === "(") {
      current += character;
      stack.push({ close: ")", multiline: false });
      continue;
    }
    if (character === "," && stack.at(-1)?.multiline === true) {
      current += character;
      pushLine();
      continue;
    }
    if (character === "}" || character === "]" || character === ")") {
      const entry = stack.pop();
      if (!entry || entry.close !== character) {
        throw new Error(`formatter delimiter mismatch near ${source.slice(Math.max(0, index - 20), index + 20)}`);
      }
      if (!entry.multiline) {
        current += character;
        continue;
      }
      pushLine();
      indent -= 1;
      current = `${"  ".repeat(indent)}${character}`;
      continue;
    }
    current += character;
  }
  pushLine();
  if (stack.length > 0) throw new Error(`formatter unclosed delimiter in ${source}`);
  return lines;
}


function nullable(schema: JsonSchema): { base: JsonSchema; isNullable: boolean } {
  if (Array.isArray(schema.type) && schema.type.includes("null")) {
    const types = schema.type.filter((type: string) => type !== "null");
    if (types.length !== 1) return { base: schema, isNullable: false };
    return { base: { ...schema, type: types[0] }, isNullable: true };
  }
  const choices = schema.anyOf;
  if (Array.isArray(choices) && choices.length === 2) {
    const nullIndex = choices.findIndex((choice: JsonSchema) => choice.type === "null");
    if (nullIndex >= 0) return { base: choices[1 - nullIndex], isNullable: true };
  }
  return { base: schema, isNullable: false };
}

function taggedUnion(branches: JsonSchema[]): string | undefined {
  for (const tag of ["type", "kind", "verdict", "premise"]) {
    if (branches.every((branch) => branch.type === "object" && branch.properties?.[tag]?.const !== undefined)) {
      return tag;
    }
  }
  return undefined;
}

function expression(schema: JsonSchema, path: string): string {
  if (schema === true || Object.keys(schema).length === 0) return "Schema.Json";
  if (schema === false) return pointer(path);
  if (schema.$ref) return refExpr(schema.$ref, path);

  if (schema.const !== undefined) return `Schema.Literal(${quote(schema.const)})`;
  if (Array.isArray(schema.enum)) return `Schema.Literals(${quote(schema.enum)})`;

  if (Array.isArray(schema.oneOf) || Array.isArray(schema.anyOf)) {
    const key = Array.isArray(schema.oneOf) ? "oneOf" : "anyOf";
    const branches = schema[key] as JsonSchema[];
    const maybeNullable = nullable(schema);
    if (maybeNullable.isNullable) return `Schema.NullOr(${expression(maybeNullable.base, `${path}/${key}`)})`;
    const members = branches.map((branch, index) => expression(branch, `${path}/${key}/${index}`));
    const tag = key === "oneOf" ? taggedUnion(branches) : undefined;
    const union = `Schema.Union([${members.join(", ")}])`;
    return tag ? `${union}.pipe(Schema.toTaggedUnion(${quote(tag)}))` : union;
  }

  if (Array.isArray(schema.type)) {
    const types = schema.type.filter((type: string) => type !== "null");
    if (types.length === 1 && schema.type.includes("null")) {
      return `Schema.NullOr(${expression({ ...schema, type: types[0] }, path)})`;
    }
    if (types.length === 0 && schema.type.includes("null")) return "Schema.Null";
    return `Schema.Union([${types.map((type: string) => expression({ ...schema, type }, path)).join(", ")}])`;
  }

  switch (schema.type) {
    case "null": return "Schema.Null";
    case "string": {
      if (schema.pattern !== undefined) {
        return `Schema.String.pipe(Schema.check(Schema.isPattern(new RegExp(${quote(schema.pattern)}))))`;
      }
      return "Schema.String";
    }
    case "boolean": return "Schema.Boolean";
    case "number": return schema.minimum !== undefined
      ? `Schema.Number.pipe(Schema.check(Schema.isGreaterThanOrEqualTo(${quote(schema.minimum)})))`
      : "Schema.Number";
    case "integer": return schema.minimum !== undefined
      ? `Schema.Int.pipe(Schema.check(Schema.isGreaterThanOrEqualTo(${quote(schema.minimum)})))`
      : "Schema.Int";
    case "array": {
      if (Array.isArray(schema.prefixItems)) {
        if (schema.items !== undefined && schema.items !== false) return pointer(`${path}/items`);
        return `Schema.Tuple([${schema.prefixItems.map((item: JsonSchema, index: number) => expression(item, `${path}/prefixItems/${index}`)).join(", ")}])`;
      }
      if (schema.items === undefined) return pointer(`${path}/items`);
      const item = expression(schema.items, `${path}/items`);
      return schema.uniqueItems === true ? `Schema.UniqueArray(${item})` : `Schema.Array(${item})`;
    }
    case "object": {
      const properties = schema.properties as Record<string, JsonSchema> | undefined;
      const propertyEntries = Object.entries(properties ?? {});
      const additional = schema.additionalProperties;
      if (propertyEntries.length === 0 && additional === true) return "Schema.Record(Schema.String, Schema.Json)";
      if (propertyEntries.length === 0 && additional && typeof additional === "object") {
        return `Schema.Record(Schema.String, ${expression(additional, `${path}/additionalProperties`)})`;
      }
      if (additional === true && propertyEntries.length > 0) return pointer(`${path}/additionalProperties`);
      if (additional && typeof additional === "object" && propertyEntries.length > 0) return pointer(`${path}/additionalProperties`);
      if (properties === undefined && additional === undefined) return pointer(path);
      const required = new Set<string>((schema.required ?? []) as string[]);
      const fields = propertyEntries.map(([name, property]) => {
        const value = nullable(property);
        const inner = expression(value.base, `${path}/properties/${name}`);
        const field = value.isNullable ? `Schema.NullOr(${inner})` : inner;
        return `${quote(name)}: ${required.has(name) ? field : `Schema.optionalKey(${field})`}`;
      });
      const struct = `Schema.Struct({${fields.join(", ")}})`;
      return struct;
    }
    default: return pointer(path);
  }
}

function jsdoc(description: unknown): string[] {
  if (typeof description !== "string") return [];
  const clean = description
    .replace(/\[([^\]]+)\]\([^)]+\)/g, "$1")
    .replace(/\[([^\]]+)\]/g, "$1");
  return ["/**", ...clean.split("\n").map((line) => ` * ${line}`), " */"];
}

function pushExport(name: string, schema: JsonSchema, path: string): void {
  const formatted = formatExpression(expression(schema, path));
  lines.push(`export const ${name} = ${formatted[0]!}`);
  lines.push(...formatted.slice(1));
  lines[lines.length - 1] += ";";
  lines.push(`export type ${name} = typeof ${name}.Type;`);
  lines.push("");
}

const lines: string[] = [
  "// THIS FILE IS GENERATED — DO NOT EDIT.",
  "// Generator: packages/runtime/scripts/contract-codegen.ts",
  "// Sources: schemas/lute-events-0.39.schema.json, schemas/lute-snapshot-0.39.schema.json",
  "",
  'import * as Schema from "effect/Schema";',
  'import type * as SchemaAST from "effect/SchemaAST";',
  "",
  'export const parseOptions = { onExcessProperty: "error" } as const satisfies SchemaAST.ParseOptions;',
  ""
];
for (const [name, definition] of defs) {
  lines.push(...jsdoc(definition.description));
  pushExport(name, definition, `#/$defs/${name}`);
}
const snapshot = schemas[1]!;
lines.push(...jsdoc(snapshot.description));
pushExport("Snapshot", snapshot, "#");
await writeFile(outputPath, `${lines.join("\n")}\n`, "utf8");
