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
import { existsSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";

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

/** Absolute path to a `lute` binary, preferring an already-built one so the
 * common case costs a stat instead of a cargo invocation. */
function luteBin() {
  for (const rel of ["target/debug/lute", "target/release/lute"]) {
    if (existsSync(`${ROOT}/${rel}`)) return `${ROOT}/${rel}`;
  }
  const build = Bun.spawnSync(["cargo", "build", "--quiet", "-p", "lute-cli"], { cwd: ROOT });
  if (!build.success) throw new Error("no lute binary and `cargo build -p lute-cli` failed");
  return `${ROOT}/target/debug/lute`;
}

/** Run the REAL `lute init` into a throwaway dir and return the tree it wrote.
 * The scaffold is the shape this schema exists to bless — a hand-written
 * fixture can agree with the schema while the tool's own output does not. */
function scaffoldProject(template) {
  const dir = `${tmpdir()}/lute-schema-scaffold-${template}-${process.pid}-${Math.random()
    .toString(36)
    .slice(2)}`;
  const args = ["init", dir];
  if (template) args.push("--template", template);
  const out = Bun.spawnSync([luteBin(), ...args], { cwd: ROOT });
  if (!out.success) {
    throw new Error(`lute init failed: ${out.stderr.toString()}${out.stdout.toString()}`);
  }
  return dir;
}

// --- both files must be valid JSON Schema of their declared dialect ---

describe("shipped schemas are well-formed", () => {
  test("both files validate against the draft-07 meta-schema they declare", () => {
    const { ajv, declSchema, pluginSchema } = loadAjv();
    for (const schema of [declSchema, pluginSchema]) {
      expect(schema.$schema).toBe("http://json-schema.org/draft-07/schema#");
      expect(ajv.validateSchema(schema), JSON.stringify(ajv.errors)).toBe(true);
    }
  });
});

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

// --- dsl 0.9.0 D-D: the `enums:` long form, on BOTH declaration routes ---
//
// 0.9.0 moved content-vocabulary MEMBERS out of the compiler: a project (or a
// plugin) declares them, and members now carry semantics — `exits:` (members
// that end a character's stage presence) and `default:`. `EnumDecl`
// (`crates/lute-manifest/src/schema.rs:46-57`) is an UNTAGGED serde enum, so a
// bare sequence and `{ members:, default:, exits: }` are equally valid; the
// document-frontmatter route (`entities.rs::parse_enums`) accepts the same two
// shapes. Both shipped JSON Schemas must accept both, or the editor
// red-underlines what `lute init` itself writes.

describe("enums long form (dsl 0.9.0 D-D)", () => {
  const LONG_FORM_YAML = `
enums:
  emotion: [neutral, surprised]
  anchor:
    members: [left, center, right]
    default: center
  action:
    members: [sway, fade-out, hide]
    exits: [fade-out, hide]
`;

  test("declaration doc: array form and long form coexist in one `enums:` block", () => {
    const { ajv, declSchema } = loadAjv();
    const doc = Bun.YAML.parse(LONG_FORM_YAML);
    const { ok, errors } = validateAgainst(ajv, declSchema.$id, doc);
    expect(ok, JSON.stringify(errors)).toBe(true);
  });

  test("plugin enums.yaml export: array form and long form coexist", () => {
    const { ajv, pluginSchema } = loadAjv();
    const doc = Bun.YAML.parse(LONG_FORM_YAML);
    const { ok, errors } = validateAgainst(ajv, pluginSchema.$id, doc);
    expect(ok, JSON.stringify(errors)).toBe(true);
  });

  test("long form with ONLY `members:` validates (default/exits are #[serde(default)])", () => {
    const { ajv, declSchema, pluginSchema } = loadAjv();
    const doc = Bun.YAML.parse("enums:\n  mood: { members: [calm, tense] }\n");
    for (const id of [declSchema.$id, pluginSchema.$id]) {
      const { ok, errors } = validateAgainst(ajv, id, doc);
      expect(ok, `${id}: ${JSON.stringify(errors)}`).toBe(true);
    }
  });

  test("long form without `members:` is rejected (no #[serde(default)] on it)", () => {
    const { ajv, declSchema, pluginSchema } = loadAjv();
    // `EnumDecl::Long.members` is not `#[serde(default)]`, so this matches
    // neither untagged variant; `parse_enums` skips the entry outright
    // (`entities.rs:69-71`) rather than declaring an empty closed domain.
    const doc = Bun.YAML.parse("enums:\n  anchor: { default: center }\n");
    for (const id of [declSchema.$id, pluginSchema.$id]) {
      expect(validateAgainst(ajv, id, doc).ok, id).toBe(false);
    }
  });

  test("unknown key inside the long form is rejected", () => {
    const { ajv, declSchema, pluginSchema } = loadAjv();
    // Serde silently IGNORES it, which is precisely why the editor schema
    // closes the object: a typo'd `exit:` would drop the stage-exit semantics
    // with no diagnostic from any layer.
    const doc = Bun.YAML.parse(
      "enums:\n  action:\n    members: [sway, hide]\n    exit: [hide]\n",
    );
    for (const id of [declSchema.$id, pluginSchema.$id]) {
      expect(validateAgainst(ajv, id, doc).ok, id).toBe(false);
    }
  });

  test("non-string member is rejected in both forms", () => {
    const { ajv, declSchema, pluginSchema } = loadAjv();
    for (const yaml of [
      "enums:\n  emotion: [neutral, 42]\n",
      "enums:\n  emotion: { members: [neutral, 42] }\n",
    ]) {
      const doc = Bun.YAML.parse(yaml);
      for (const id of [declSchema.$id, pluginSchema.$id]) {
        expect(validateAgainst(ajv, id, doc).ok, `${id} / ${yaml}`).toBe(false);
      }
    }
  });

  test("a bare string where a member array or long-form map belongs is rejected", () => {
    const { ajv, declSchema, pluginSchema } = loadAjv();
    // `parse_enums` skips a scalar entry (`entities.rs:81-83`) and `EnumDecl`
    // matches neither variant.
    const doc = Bun.YAML.parse("enums:\n  emotion: neutral\n");
    for (const id of [declSchema.$id, pluginSchema.$id]) {
      expect(validateAgainst(ajv, id, doc).ok, id).toBe(false);
    }
  });

  test("non-string `default:` is rejected (EnumDecl::Long.default is Option<String>)", () => {
    const { ajv, declSchema } = loadAjv();
    const doc = Bun.YAML.parse("enums:\n  anchor: { members: [left], default: [left] }\n");
    expect(validateAgainst(ajv, declSchema.$id, doc).ok).toBe(false);
  });

  // The bug this section exists for: `lute init` writes the long form, so the
  // tool's own scaffold was red-underlined by the tool's own editor schema
  // (`editors/vscode/package.json` maps `**/*.schema.yaml` at lute.schema.json).
  for (const template of ["minimal", "investigation"]) {
    test(`the REAL scaffold from \`lute init --template ${template}\` validates`, () => {
      const { ajv, declSchema } = loadAjv();
      const dir = scaffoldProject(template);
      const doc = Bun.YAML.parse(readFileSync(`${dir}/vocabulary.schema.yaml`, "utf8"));
      const { ok, errors } = validateAgainst(ajv, declSchema.$id, doc);
      expect(ok, JSON.stringify(errors)).toBe(true);
    });
  }

  // Every long-form file the repo actually ships, one per declaration route:
  // a project-root schema, a subproject schema, and a plugin `enums` export.
  for (const [rel, route] of [
    ["docs/examples/base.schema.yaml", "decl"],
    ["docs/examples/idola-project/vocabulary.schema.yaml", "decl"],
    ["docs/examples/showcase/plugins/showcase.pack/enums/vocabulary.yaml", "plugin"],
  ]) {
    test(`real shipped ${rel} validates`, () => {
      const { ajv, declSchema, pluginSchema } = loadAjv();
      const doc = Bun.YAML.parse(readFileSync(`${ROOT}/${rel}`, "utf8"));
      const id = route === "decl" ? declSchema.$id : pluginSchema.$id;
      const { ok, errors } = validateAgainst(ajv, id, doc);
      expect(ok, JSON.stringify(errors)).toBe(true);
    });
  }
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

  test("good StampAttrsFile (plugin §14.1 `stampattrs/*.yaml`) validates", () => {
    const { ajv, pluginSchema } = loadAjv();
    const doc = Bun.YAML.parse(
      "stampAttrs:\n  - { name: bonusId, type: string }\n  - { name: bonusScore, type: number }\n",
    );
    const { ok, errors } = validateAgainst(ajv, pluginSchema.$id, doc);
    expect(ok, JSON.stringify(errors)).toBe(true);
  });

  test("PluginManifest may declare the `stampattrs` export", () => {
    const { ajv, pluginSchema } = loadAjv();
    const doc = Bun.YAML.parse(
      "id: demo.plugin\nversion: 0.1.0\nkind: capability\nexports:\n  stampattrs: stampattrs/\n",
    );
    const { ok, errors } = validateAgainst(ajv, pluginSchema.$id, doc);
    expect(ok, JSON.stringify(errors)).toBe(true);
  });

  test("broken StampAttrsFile: entry missing required `type`", () => {
    const { ajv, pluginSchema } = loadAjv();
    const doc = Bun.YAML.parse("stampAttrs:\n  - { name: bonusId }\n");
    const { ok } = validateAgainst(ajv, pluginSchema.$id, doc);
    expect(ok).toBe(false);
  });
});
