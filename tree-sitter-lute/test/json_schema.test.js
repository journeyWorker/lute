// tree-sitter-lute/test/json_schema.test.js
//
// Structural lint for declaration YAML (data-catalog foundation B1): the two
// JSON Schemas under `schemas/` must accept every shape the Rust deserializers
// accept (`crates/lute-manifest/src/{schema.rs,types.rs,entities.rs}`) for
// project declaration docs (`state:`/`defs:`/`enums:`/`entities:`, the future
// standalone-`.yaml` form of today's `.schema.lute` frontmatter — see B4) and
// for the plugin manifest + its export files. This is STRUCTURE-only: CEL
// validity, path resolution, and domain membership stay in the Lute checker
// (B3) — never asserted here.
import { test, expect, describe } from "bun:test";
import Ajv from "ajv";
import { readFileSync } from "node:fs";

const ROOT = `${import.meta.dir}/../..`;
const DECL_SCHEMA_PATH = `${ROOT}/schemas/lute.schema.json`;
const PLUGIN_SCHEMA_PATH = `${ROOT}/schemas/lute.plugin.json`;

/** Fresh Ajv instance with both schemas registered (lute.plugin.json
 * cross-references lute.schema.json's `type`/`field`/`literal` defs by $id,
 * so both must be loaded before either is compiled). Draft-07 keeps a plain
 * `ajv` import sufficient (no `/dist/2020` submodule needed). */
function loadAjv() {
  const declSchema = JSON.parse(readFileSync(DECL_SCHEMA_PATH, "utf8"));
  const pluginSchema = JSON.parse(readFileSync(PLUGIN_SCHEMA_PATH, "utf8"));
  const ajv = new Ajv({ allErrors: true, strict: false });
  ajv.addSchema(declSchema);
  ajv.addSchema(pluginSchema);
  return { ajv, declSchema, pluginSchema };
}

function validateAgainst(ajv, schemaId, doc) {
  const validate = ajv.getSchema(schemaId);
  if (!validate) throw new Error(`schema not registered: ${schemaId}`);
  const ok = validate(doc);
  return { ok, errors: validate.errors };
}

// --- schemas/lute.schema.json: state/defs/enums/entities declaration doc ---

describe("lute.schema.json — declaration doc (state/defs/enums/entities)", () => {
  // Mirrors a real project schema doc (dsl §9.2/§9.3, data-catalog A3): the
  // `uses:` chain plus inline `state:`/`defs:`/`enums:`/`entities:` blocks a
  // scene imports. Shapes match `crates/lute-check/src/meta.rs::parse_meta_kind`
  // (state/defs) and `crates/lute-manifest/src/entities.rs` (enums/entities).
  const GOOD_DECL_YAML = `
uses: shapes.yaml
extends: [base.yaml]
plugins:
  showcase.pack: { resultScope: run }
state:
  scene.affect.bianca:
    type: number
    default: 0
  run.gold:
    type: { enum: [bronze, silver, gold] }
defs:
  helped:
    type: bool
    cel: "true"
  bonus:
    type: number
    params:
      mult: number
    cel: "mult * 2"
enums:
  action: [wave, bow]
  mood: [calm, tense]
entities:
  character:
    members: [shadowheart, halsin]
  npc:
    open: engine
`;

  test("good inline state/defs/enums/entities declaration validates", () => {
    const { ajv, declSchema } = loadAjv();
    const doc = Bun.YAML.parse(GOOD_DECL_YAML);
    const { ok, errors } = validateAgainst(ajv, declSchema.$id, doc);
    expect(ok, JSON.stringify(errors)).toBe(true);
  });

  test("broken declaration: state entry `type` is a bare number, not a Type form", () => {
    const { ajv, declSchema } = loadAjv();
    // `type: 42` cannot deserialize into the manifest `Type` enum (neither a
    // bare bool/number/string tag nor a single-key tagged map) — a real Rust
    // `E-STATE-DECL` deserialize failure.
    const doc = Bun.YAML.parse("state:\n  scene.affect.bianca:\n    type: 42\n");
    const { ok } = validateAgainst(ajv, declSchema.$id, doc);
    expect(ok).toBe(false);
  });

  test("broken declaration: a Type tagged-map with two keys is ambiguous", () => {
    const { ajv, declSchema } = loadAjv();
    // `serde_yaml::with::singleton_map_recursive` requires EXACTLY one key to
    // resolve the `Type` tag (types.rs) — `{ enum: [...], list: number }` has
    // two, so real Rust deserialization fails too.
    const doc = Bun.YAML.parse(
      "defs:\n  bad:\n    type: { enum: [a, b], list: number }\n    cel: \"true\"\n",
    );
    const { ok } = validateAgainst(ajv, declSchema.$id, doc);
    expect(ok).toBe(false);
  });

  test("broken declaration: unknown top-level key", () => {
    const { ajv, declSchema } = loadAjv();
    // `bogusKey` is not in `crate::meta::UNIVERSAL_KEYS` — real Rust checker
    // rejects it with `E-META-UNKNOWN-KEY`.
    const doc = Bun.YAML.parse("bogusKey: true\nenums:\n  action: [wave]\n");
    const { ok } = validateAgainst(ajv, declSchema.$id, doc);
    expect(ok).toBe(false);
  });

  test("entities: neither `members` nor `open` is rejected (silently produces no domain in Rust)", () => {
    const { ajv, declSchema } = loadAjv();
    const doc = Bun.YAML.parse("entities:\n  bogus: { nope: true }\n");
    const { ok } = validateAgainst(ajv, declSchema.$id, doc);
    expect(ok).toBe(false);
  });

  test("entities: `open` value is ignored, presence alone selects the open shape", () => {
    const { ajv, declSchema } = loadAjv();
    const doc = Bun.YAML.parse("entities:\n  npc: { open: false }\n");
    const { ok, errors } = validateAgainst(ajv, declSchema.$id, doc);
    expect(ok, JSON.stringify(errors)).toBe(true);
  });
});

// --- schemas/lute.plugin.json: plugin manifest + export files ---

describe("lute.plugin.json — plugin manifest + export files", () => {
  test("real shipped lute.core plugin.yaml validates", () => {
    const { ajv, pluginSchema } = loadAjv();
    const doc = Bun.YAML.parse(
      readFileSync(`${ROOT}/crates/lute-manifest/assets/lute.core/plugin.yaml`, "utf8"),
    );
    const { ok, errors } = validateAgainst(ajv, pluginSchema.$id, doc);
    expect(ok, JSON.stringify(errors)).toBe(true);
  });

  test("real shipped lute.core enums.yaml validates", () => {
    const { ajv, pluginSchema } = loadAjv();
    const doc = Bun.YAML.parse(
      readFileSync(`${ROOT}/crates/lute-manifest/assets/lute.core/enums.yaml`, "utf8"),
    );
    const { ok, errors } = validateAgainst(ajv, pluginSchema.$id, doc);
    expect(ok, JSON.stringify(errors)).toBe(true);
  });

  test("real shipped lute.core directives/staging.yaml validates (record + builtin lowering, domain/enum attrs)", () => {
    const { ajv, pluginSchema } = loadAjv();
    const doc = Bun.YAML.parse(
      readFileSync(`${ROOT}/crates/lute-manifest/assets/lute.core/directives/staging.yaml`, "utf8"),
    );
    const { ok, errors } = validateAgainst(ajv, pluginSchema.$id, doc);
    expect(ok, JSON.stringify(errors)).toBe(true);
  });

  test("real shipped showcase.pack plugin.yaml (depends + options) validates", () => {
    const { ajv, pluginSchema } = loadAjv();
    const doc = Bun.YAML.parse(
      readFileSync(
        `${ROOT}/docs/examples/showcase/plugins/showcase.pack/plugin.yaml`,
        "utf8",
      ),
    );
    const { ok, errors } = validateAgainst(ajv, pluginSchema.$id, doc);
    expect(ok, JSON.stringify(errors)).toBe(true);
  });

  test("real shipped showcase.pack directives/serve.yaml (providerRef/assetKind/slotId attrs, state.declares, effects.writes, bridge) validates", () => {
    const { ajv, pluginSchema } = loadAjv();
    const doc = Bun.YAML.parse(
      readFileSync(
        `${ROOT}/docs/examples/showcase/plugins/showcase.pack/directives/serve.yaml`,
        "utf8",
      ),
    );
    const { ok, errors } = validateAgainst(ajv, pluginSchema.$id, doc);
    expect(ok, JSON.stringify(errors)).toBe(true);
  });

  test("real shipped showcase.pack defs/showcase.yaml (defs.yaml list form) validates", () => {
    const { ajv, pluginSchema } = loadAjv();
    const doc = Bun.YAML.parse(
      readFileSync(`${ROOT}/docs/examples/showcase/plugins/showcase.pack/defs/showcase.yaml`, "utf8"),
    );
    const { ok, errors } = validateAgainst(ajv, pluginSchema.$id, doc);
    expect(ok, JSON.stringify(errors)).toBe(true);
  });

  test("real shipped idola.minigame assetkinds/art.yaml validates", () => {
    const { ajv, pluginSchema } = loadAjv();
    const doc = Bun.YAML.parse(
      readFileSync(
        `${ROOT}/docs/examples/idola-project/plugins/idola.minigame/assetkinds/art.yaml`,
        "utf8",
      ),
    );
    const { ok, errors } = validateAgainst(ajv, pluginSchema.$id, doc);
    expect(ok, JSON.stringify(errors)).toBe(true);
  });

  test("good inline PluginManifest with depends + options validates", () => {
    const { ajv, pluginSchema } = loadAjv();
    const doc = Bun.YAML.parse(`
id: demo.plugin
version: 0.1.0
kind: capability
depends: [ { id: lute.core, range: "^0.0.1" } ]
exports:
  directives: directives/
  defs: defs/
options:
  - { name: allowedKinds, type: { list: { enum: [a, b] } }, default: [a, b] }
`);
    const { ok, errors } = validateAgainst(ajv, pluginSchema.$id, doc);
    expect(ok, JSON.stringify(errors)).toBe(true);
  });

  test("broken PluginManifest: missing required `version`", () => {
    const { ajv, pluginSchema } = loadAjv();
    const doc = Bun.YAML.parse("id: demo.plugin\nkind: capability\nexports: {}\n");
    const { ok } = validateAgainst(ajv, pluginSchema.$id, doc);
    expect(ok).toBe(false);
  });

  test("broken PluginManifest: unknown export kind (loader.rs LoadError::UnknownExport)", () => {
    const { ajv, pluginSchema } = loadAjv();
    const doc = Bun.YAML.parse(
      "id: demo.plugin\nversion: 0.1.0\nkind: capability\nexports:\n  bogusExport: dir/\n",
    );
    const { ok } = validateAgainst(ajv, pluginSchema.$id, doc);
    expect(ok).toBe(false);
  });

  test("broken DirectivesFile: attr `type` uses a wrong-type Type form (number instead of tag)", () => {
    const { ajv, pluginSchema } = loadAjv();
    const doc = Bun.YAML.parse(
      "directives:\n  - name: bad\n    attrs:\n      - { name: x, type: 5 }\n    lower: { kind: builtin, name: x }\n",
    );
    const { ok } = validateAgainst(ajv, pluginSchema.$id, doc);
    expect(ok).toBe(false);
  });

  test("broken DirectivesFile: directive missing required `lower`", () => {
    const { ajv, pluginSchema } = loadAjv();
    const doc = Bun.YAML.parse("directives:\n  - name: bad\n    attrs: []\n");
    const { ok } = validateAgainst(ajv, pluginSchema.$id, doc);
    expect(ok).toBe(false);
  });
});
