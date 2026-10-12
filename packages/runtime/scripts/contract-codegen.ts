import { access, readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";

type JsonSchema = any;
const root = resolve(import.meta.dir, "../../..");

const cargoManifest = Bun.TOML.parse(await readFile(resolve(root, "Cargo.toml"), "utf8")) as {
  workspace?: { package?: { version?: unknown } };
};
const workspaceVersion = cargoManifest.workspace?.package?.version;
const releaseLine = typeof workspaceVersion === "string" ? /^(\d+\.\d+)\.\d+/.exec(workspaceVersion)?.[1] : undefined;
if (releaseLine === undefined) {
  throw new Error(`root Cargo.toml [workspace.package] version is not major.minor.patch: ${String(workspaceVersion)}`);
}
const schemaPath = (family: string): string => resolve(root, `schemas/${family}-${releaseLine}.schema.json`);

const generationTable = [
  {
    inputPaths: [schemaPath("lute-events"), schemaPath("lute-snapshot")],
    outputPath: resolve(import.meta.dir, "../src/generated/Contract.ts"),
    rootNames: ["Snapshot"]
  },
  {
    inputPaths: [schemaPath("lute-ir")],
    outputPath: resolve(import.meta.dir, "../src/generated/Ir.ts"),
    rootNames: ["Command"]
  },
  {
    inputPaths: [schemaPath("lute-player-bindings")],
    outputPath: resolve(root, "packages/player/src/bindings/schema.ts"),
    rootNames: ["Bindings"]
  }
] as const;
for (const path of generationTable.flatMap((entry) => entry.inputPaths)) {
  await access(path).catch(() => {
    throw new Error(`missing contract schema input: ${path}`);
  });
}
const schemaPaths = generationTable[0].inputPaths;
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
const opaqueDefs: Record<string, true> = { exprNode: true, prereqFormula: true };
const refExpr = (ref: string, _path?: string): string => {
  const match = /^#\/\$defs\/([^/]+)$/.exec(ref);
  if (!match || opaqueDefs[match[1]!]) return "Schema.Json";
  return `Schema.suspend(() => ${match[1]!})`;
};
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
  if (schema.type === undefined) {
    if (Array.isArray(schema.required)) return expression({ ...schema, type: "object" }, path);
    return "Schema.Json";
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
      if (properties === undefined && schema.additionalProperties === undefined) {
        const required = (schema.required ?? []) as string[];
        return required.length === 0
          ? "Schema.Json"
          : `Schema.Struct({${required.map((name) => `${quote(name)}: Schema.Json`).join(", ")}})`;
      }
      const propertyEntries = Object.entries(properties ?? {});
      const additional = schema.additionalProperties;
      const required = new Set<string>((schema.required ?? []) as string[]);
      if (propertyEntries.length === 0 && additional === true) return "Schema.Record(Schema.String, Schema.Json)";
      if (propertyEntries.length === 0 && additional && typeof additional === "object") {
        return `Schema.Record(Schema.String, ${expression(additional, `${path}/additionalProperties`)})`;
      }
      if (additional === true && propertyEntries.length > 0) {
        const struct = `Schema.Struct({${propertyEntries.map(([name, property]) => {
          const value = nullable(property);
          const inner = expression(value.base, `${path}/properties/${name}`);
          const field = value.isNullable ? `Schema.NullOr(${inner})` : inner;
          return `${quote(name)}: ${required.has(name) ? field : `Schema.optionalKey(${field})`}`;
        }).join(", ")}})`;
        return struct;
      }
      if (additional && typeof additional === "object" && propertyEntries.length > 0) {
        const struct = `Schema.Struct({${propertyEntries.map(([name, property]) => {
          const value = nullable(property);
          const inner = expression(value.base, `${path}/properties/${name}`);
          const field = value.isNullable ? `Schema.NullOr(${inner})` : inner;
          return `${quote(name)}: ${required.has(name) ? field : `Schema.optionalKey(${field})`}`;
        }).join(", ")}})`;
        return struct;
      }
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

function refsOf(schema: JsonSchema, into: Set<string>): Set<string> {
  if (Array.isArray(schema)) {
    for (const item of schema) refsOf(item, into);
  } else if (schema && typeof schema === "object") {
    for (const [key, value] of Object.entries(schema)) {
      if (key === "$ref" && typeof value === "string") {
        const match = /^#\/\$defs\/([^/]+)$/.exec(value);
        if (match && !opaqueDefs[match[1]!]) into.add(match[1]!);
      } else {
        refsOf(value, into);
      }
    }
  }
  return into;
}

/** Definitions that reach themselves through `$ref`s; their consts need an explicit type. */
function recursiveDefinitions(definitions: Map<string, JsonSchema>): Set<string> {
  const edges = new Map([...definitions].map(([name, definition]) => [name, refsOf(definition, new Set())]));
  const recursive = new Set<string>();
  for (const name of definitions.keys()) {
    const seen = new Set<string>();
    const pending = [...(edges.get(name) ?? [])];
    while (pending.length > 0) {
      const next = pending.pop()!;
      if (next === name) {
        recursive.add(name);
        break;
      }
      if (seen.has(next)) continue;
      seen.add(next);
      pending.push(...(edges.get(next) ?? []));
    }
  }
  return recursive;
}

/** The TypeScript `Type` of `expression(schema)`, spelled out for recursive definitions. */
function typeExpression(schema: JsonSchema, path: string): string {
  if (schema === true || Object.keys(schema).length === 0) return "Schema.Json";
  if (schema === false) return pointer(path);
  if (schema.$ref) {
    const name = refName(schema.$ref, path);
    return opaqueDefs[name] ? "Schema.Json" : name;
  }
  if (schema.const !== undefined) return quote(schema.const);
  if (Array.isArray(schema.enum)) return `(${schema.enum.map(quote).join(" | ")})`;
  if (Array.isArray(schema.oneOf) || Array.isArray(schema.anyOf)) {
    const key = Array.isArray(schema.oneOf) ? "oneOf" : "anyOf";
    const maybeNullable = nullable(schema);
    if (maybeNullable.isNullable) return `(${typeExpression(maybeNullable.base, `${path}/${key}`)} | null)`;
    const branches = schema[key] as JsonSchema[];
    return `(${branches.map((branch, index) => typeExpression(branch, `${path}/${key}/${index}`)).join(" | ")})`;
  }
  if (Array.isArray(schema.type)) {
    const types = schema.type.filter((type: string) => type !== "null");
    if (types.length === 1 && schema.type.includes("null")) {
      return `(${typeExpression({ ...schema, type: types[0] }, path)} | null)`;
    }
    if (types.length === 0 && schema.type.includes("null")) return "null";
    return `(${types.map((type: string) => typeExpression({ ...schema, type }, path)).join(" | ")})`;
  }
  if (schema.type === undefined) {
    if (Array.isArray(schema.required)) return typeExpression({ ...schema, type: "object" }, path);
    return "Schema.Json";
  }
  switch (schema.type) {
    case "null": return "null";
    case "string": return "string";
    case "boolean": return "boolean";
    case "number":
    case "integer": return "number";
    case "array": {
      if (Array.isArray(schema.prefixItems)) {
        if (schema.items !== undefined && schema.items !== false) return pointer(`${path}/items`);
        return `readonly [${schema.prefixItems.map((item: JsonSchema, index: number) => typeExpression(item, `${path}/prefixItems/${index}`)).join(", ")}]`;
      }
      if (schema.items === undefined) return pointer(`${path}/items`);
      return `ReadonlyArray<${typeExpression(schema.items, `${path}/items`)}>`;
    }
    case "object": {
      const properties = schema.properties as Record<string, JsonSchema> | undefined;
      if (properties === undefined && schema.additionalProperties === undefined) {
        const required = (schema.required ?? []) as string[];
        return required.length === 0
          ? "Schema.Json"
          : `{${required.map((name) => `readonly ${quote(name)}: Schema.Json`).join(", ")}}`;
      }
      const propertyEntries = Object.entries(properties ?? {});
      const additional = schema.additionalProperties;
      if (propertyEntries.length === 0 && additional === true) return "{readonly [x: string]: Schema.Json}";
      if (propertyEntries.length === 0 && additional && typeof additional === "object") {
        return `{readonly [x: string]: ${typeExpression(additional, `${path}/additionalProperties`)}}`;
      }
      const required = new Set<string>((schema.required ?? []) as string[]);
      return `{${propertyEntries.map(([name, property]) => {
        const value = nullable(property);
        const inner = typeExpression(value.base, `${path}/properties/${name}`);
        const field = value.isNullable ? `(${inner} | null)` : inner;
        return `readonly ${quote(name)}${required.has(name) ? "" : "?"}: ${field}`;
      }).join(", ")}}`;
    }
    default: return pointer(path);
  }
}

function definitionLines(name: string, schema: JsonSchema, path: string, recursive: boolean): string[] {
  const out = jsdoc(schema.description);
  const formatted = formatExpression(expression(schema, path));
  if (!recursive) {
    out.push(`export const ${name} = ${formatted[0]!}`, ...formatted.slice(1));
    out[out.length - 1] += ";";
    out.push(`export type ${name} = typeof ${name}.Type;`, "");
    return out;
  }
  const key = Array.isArray(schema.oneOf) ? "oneOf" : "anyOf";
  const members = Array.isArray(schema[key]) && !nullable(schema).isNullable
    ? (schema[key] as JsonSchema[]).map((branch, index) => typeExpression(branch, `${path}/${key}/${index}`))
    : [typeExpression(schema, path)];
  out.push(`export type ${name} =`);
  for (const member of members) {
    const memberLines = formatExpression(member);
    out.push(`  | ${memberLines[0]!}`, ...memberLines.slice(1).map((line) => `  ${line}`));
  }
  out[out.length - 1] += ";";
  out.push(`export const ${name}: Schema.Codec<${name}> = ${formatted[0]!}`, ...formatted.slice(1));
  out[out.length - 1] += ";";
  out.push("");
  return out;
}

const lines: string[] = [
  "// THIS FILE IS GENERATED — DO NOT EDIT.",
  "// Generator: packages/runtime/scripts/contract-codegen.ts",
  `// Sources: schemas/lute-events-${releaseLine}.schema.json, schemas/lute-snapshot-${releaseLine}.schema.json`,
  "",
  'import * as Schema from "effect/Schema";',
  'import type * as SchemaAST from "effect/SchemaAST";',
  "",
  'export const parseOptions = { onExcessProperty: "error" } as const satisfies SchemaAST.ParseOptions;',
  ""
];
const contractRecursive = recursiveDefinitions(defs);
for (const [name, definition] of defs) {
  lines.push(...definitionLines(name, definition, `#/$defs/${name}`, contractRecursive.has(name)));
}
lines.push(...definitionLines("Snapshot", schemas[1]!, "#", false));
await writeFile(generationTable[0].outputPath, `${lines.join("\n")}\n`, "utf8");
const irSchema = JSON.parse(
  await readFile(generationTable[1].inputPaths[0], "utf8")
) as JsonSchema;
const irDefs = new Map<string, JsonSchema>(
  Object.entries(irSchema.$defs as Record<string, JsonSchema>)
);
const irLines: string[] = [
  "// THIS FILE IS GENERATED — DO NOT EDIT.",
  "// Generator: packages/runtime/scripts/contract-codegen.ts",
  `// Source: schemas/lute-ir-${releaseLine}.schema.json`,
  "",
  'import * as Schema from "effect/Schema";',
  ""
];
const irRecursive = recursiveDefinitions(irDefs);
for (const [name, definition] of irDefs) {
  if (name === "command") continue;
  irLines.push(...definitionLines(name, definition, `#/$defs/${name}`, irRecursive.has(name)));
}
const command = irDefs.get("command");
if (command === undefined || !Array.isArray(command.oneOf)) {
  throw new Error("IR schema does not define command union");
}
irLines.push(...jsdoc(command.description));
const commandExpression = `Schema.Union([${command.oneOf
  .map((branch: JsonSchema, index: number) => `Schema.suspend(() => ${refName(branch.$ref, `#/$defs/command/oneOf/${index}`)})`)
  .join(", ")}])`;
const commandFormatted = formatExpression(commandExpression);
irLines.push(`export const Command = ${commandFormatted[0]!}`);
irLines.push(...commandFormatted.slice(1));
irLines[irLines.length - 1] += ";";
irLines.push("export type Command = typeof Command.Type;", "");
await writeFile(generationTable[1].outputPath, `${irLines.join("\n")}\n`, "utf8");
const bindingsSchema = JSON.parse(
  await readFile(generationTable[2].inputPaths[0], "utf8")
) as JsonSchema;
const bindingDefs = new Map<string, JsonSchema>(
  Object.entries(bindingsSchema.$defs as Record<string, JsonSchema>)
);
const bindingLines: string[] = [
  "// THIS FILE IS GENERATED — DO NOT EDIT.",
  "// Generator: packages/runtime/scripts/contract-codegen.ts",
  `// Source: schemas/lute-player-bindings-${releaseLine}.schema.json`,
  "",
  'import * as Schema from "effect/Schema";',
  'import type * as SchemaAST from "effect/SchemaAST";',
  "",
  'export const parseOptions = { onExcessProperty: "error" } as const satisfies SchemaAST.ParseOptions;',
  ""
];
const bindingRecursive = recursiveDefinitions(bindingDefs);
for (const [name, definition] of bindingDefs) {
  bindingLines.push(...definitionLines(name, definition, `#/$defs/${name}`, bindingRecursive.has(name)));
}
bindingLines.push(...definitionLines("Bindings", bindingsSchema, "#", false));
await writeFile(generationTable[2].outputPath, `${bindingLines.join("\n")}\n`, "utf8");
