// tree-sitter-lute/test/json_schema.test.js
//
// Structural lint for declaration YAML (data-catalog foundation B1): the
// published JSON Schemas under `schemas/` must accept every shape the Rust
// deserializers accept. This covers project declaration docs
// (`state:`/`defs:`/`enums:`/`entities:`), plugin manifests and exports, and
// `lute.project.yaml` configuration. This is STRUCTURE-only: CEL validity,
// path resolution, and domain membership stay in the Lute checker (B3) —
// never asserted here.
import { test, expect, describe } from "bun:test";
import Ajv from "ajv";
import Ajv2020 from "ajv/dist/2020.js";
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";

const ROOT = `${import.meta.dir}/../..`;
const DECL_SCHEMA_PATH = `${ROOT}/schemas/lute.schema.json`;
const PLUGIN_SCHEMA_PATH = `${ROOT}/schemas/lute.plugin.json`;
const PROJECT_SCHEMA_PATH = `${ROOT}/schemas/lute.project.json`;

/** Fresh Ajv instance with all published authoring schemas registered.
 * lute.plugin.json cross-references lute.schema.json's definitions. Draft-07
 * keeps a plain `ajv` import sufficient (no `/dist/2020` submodule needed). */
function loadAjv() {
  const declSchema = JSON.parse(readFileSync(DECL_SCHEMA_PATH, "utf8"));
  const pluginSchema = JSON.parse(readFileSync(PLUGIN_SCHEMA_PATH, "utf8"));
  const projectSchema = JSON.parse(readFileSync(PROJECT_SCHEMA_PATH, "utf8"));
  const ajv = new Ajv({ allErrors: true, strict: false });
  ajv.addSchema(declSchema);
  ajv.addSchema(pluginSchema);
  ajv.addSchema(projectSchema);
  return { ajv, declSchema, pluginSchema, projectSchema };
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

// --- all files must be valid JSON Schema of their declared dialect ---

describe("shipped schemas are well-formed", () => {
  test("all files validate against the draft-07 meta-schema they declare", () => {
    const { ajv, declSchema, pluginSchema, projectSchema } = loadAjv();
    for (const schema of [declSchema, pluginSchema, projectSchema]) {
      expect(schema.$schema).toBe("http://json-schema.org/draft-07/schema#");
      expect(ajv.validateSchema(schema), JSON.stringify(ajv.errors)).toBe(true);
    }
  });
});

// --- schemas/lute.project.json: lute.project.yaml + permissions ---

describe("lute.project.json — project configuration", () => {
  test("real generic capability-permissions project validates", () => {
    const { ajv, projectSchema } = loadAjv();
    const doc = Bun.YAML.parse(
      readFileSync(
        `${ROOT}/docs/examples/capability-permissions/lute.project.yaml`,
        "utf8",
      ),
    );
    const { ok, errors } = validateAgainst(ajv, projectSchema.$id, doc);
    expect(ok, JSON.stringify(errors)).toBe(true);
  });

  test("explicit empty permission arrays validate as deny-all sets", () => {
    const { ajv, projectSchema } = loadAjv();
    const doc = Bun.YAML.parse(`
defaultProfile: restricted
permissions:
  bridges: [dialogue/respond]
profiles:
  restricted:
    plugins: {}
    permissions:
      directives: []
      stateWrites: []
      factWrites: []
      bridges: []
      rewards: false
      quests: false
`);
    const { ok, errors } = validateAgainst(ajv, projectSchema.$id, doc);
    expect(ok, JSON.stringify(errors)).toBe(true);
  });

  test("accepts identity stability and rename settings", () => {
    const { ajv, projectSchema } = loadAjv();
    const doc = Bun.YAML.parse(`
defaultProfile: core
profiles:
  core:
    plugins: {}
identity:
  requireStable: true
  renames:
    "quest:old": "quest:new"
`);
    const { ok, errors } = validateAgainst(ajv, projectSchema.$id, doc);
    expect(ok, JSON.stringify(errors)).toBe(true);
  });
  for (const [name, yaml] of [
    ["unknown field", "defaultProfile: p\npermissions: { directive: [bg] }\n"],
    ["null set", "defaultProfile: p\npermissions: { directives: null }\n"],
    ["malformed path", "defaultProfile: p\npermissions: { stateWrites: [scene.*.bad] }\n"],
    ["malformed bridge", "defaultProfile: p\npermissions: { bridges: [dialogue.respond] }\n"],
  ]) {
    test(`rejects ${name}`, () => {
      const { ajv, projectSchema } = loadAjv();
      const { ok } = validateAgainst(ajv, projectSchema.$id, Bun.YAML.parse(yaml));
      expect(ok).toBe(false);
    });
  }
  test("accepts every constraint kind with its declared fields", () => {
    const { ajv, projectSchema } = loadAjv();
    const doc = Bun.YAML.parse(`
defaultProfile: core
profiles:
  core:
    plugins: {}
constraints:
  - { id: reach, kind: reachable, node: scene:intro }
  - { id: complete, kind: completable, quest: main }
  - { id: speak, kind: speaksOnlyWhen, speaker: narrator, when: "true" }
  - { id: slots, kind: noSingleSlotProgress, quest: main }
`);
    const { ok, errors } = validateAgainst(ajv, projectSchema.$id, doc);
    expect(ok, JSON.stringify(errors)).toBe(true);
  });

  for (const [name, yaml] of [
    ["cross-kind field", "constraints: [{ id: x, kind: reachable, quest: q }]\n"],
    ["unknown kind", "constraints: [{ id: x, kind: impossible, node: scene:s }]\n"],
    ["missing required field", "constraints: [{ id: x, kind: completable }]\n"],
  ]) {
    test(`rejects constraint ${name}`, () => {
      const { ajv, projectSchema } = loadAjv();
      const { ok } = validateAgainst(ajv, projectSchema.$id, Bun.YAML.parse(yaml));
      expect(ok).toBe(false);
    });
  }

  test("accepts `diagnostics:` levels, scoped and unscoped (dsl 0.50.0 §6.1)", () => {
    const { ajv, projectSchema } = loadAjv();
    const doc = Bun.YAML.parse(`
defaultProfile: core
diagnostics:
  W-DISPLAY-NAME-DUP: error
  W-STAGE-ABSENT: { level: off, paths: ["scenes/imported/**"] }
`);
    const { ok, errors } = validateAgainst(ajv, projectSchema.$id, doc);
    expect(ok, JSON.stringify(errors)).toBe(true);
  });

  for (const [name, yaml] of [
    ["an error code", "diagnostics: { E-UNKNOWN-ATTR: off }\n"],
    ["a bad level", "diagnostics: { W-WIP: silent }\n"],
    ["empty paths", "diagnostics: { W-WIP: { level: off, paths: [] } }\n"],
    ["an absolute path", "diagnostics: { W-WIP: { level: off, paths: [\"/a/**\"] } }\n"],
    ["a `..` segment", "diagnostics: { W-WIP: { level: off, paths: [\"../a/**\"] } }\n"],
    ["a missing level", "diagnostics: { W-WIP: { paths: [\"a/**\"] } }\n"],
  ]) {
    test(`rejects diagnostics with ${name}`, () => {
      const { ajv, projectSchema } = loadAjv();
      const { ok } = validateAgainst(ajv, projectSchema.$id, Bun.YAML.parse(yaml));
      expect(ok).toBe(false);
    });
  }
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
  scene.affect.marina:
    type: int
    default: 0
  run.gold:
    type: { enum: [bronze, silver, gold] }
defs:
  helped:
    type: bool
    cel: "true"
  bonus:
    type: double
    params:
      mult: double
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
    // bare bool/int/double/string tag nor a single-key tagged map) — a real Rust
    // `E-STATE-DECL` deserialize failure.
    const doc = Bun.YAML.parse("state:\n  scene.affect.marina:\n    type: 42\n");
    const { ok } = validateAgainst(ajv, declSchema.$id, doc);
    expect(ok).toBe(false);
  });

  test("broken declaration: `number` is not a type since 0.32 (int/double replace it)", () => {
    const { ajv, declSchema } = loadAjv();
    // dsl 0.32.0 §1.2: the Rust `Type` enum has no `number` tag (E-STATE-DECL).
    const doc = Bun.YAML.parse("state:\n  scene.affect.marina:\n    type: number\n");
    const { ok } = validateAgainst(ajv, declSchema.$id, doc);
    expect(ok).toBe(false);
  });

  test("broken declaration: a Type tagged-map with two keys is ambiguous", () => {
    const { ajv, declSchema } = loadAjv();
    // `yaml_serde::with::singleton_map_recursive` requires EXACTLY one key to
    // resolve the `Type` tag (types.rs) — `{ enum: [...], list: int }` has
    // two, so real Rust deserialization fails too.
    const doc = Bun.YAML.parse(
      "defs:\n  bad:\n    type: { enum: [a, b], list: int }\n    cel: \"true\"\n",
    );
    const { ok } = validateAgainst(ajv, declSchema.$id, doc);
    expect(ok).toBe(false);
  });

  test("defs: the shorthand string and a type-less long form validate (dsl 0.21.0 §7b)", () => {
    const { ajv, declSchema } = loadAjv();
    const doc = Bun.YAML.parse(
      'defs:\n  ready: "holds(knows(a, b))"\n  level: { cel: "user.level" }\n',
    );
    const { ok, errors } = validateAgainst(ajv, declSchema.$id, doc);
    expect(ok, JSON.stringify(errors)).toBe(true);
  });

  test("defs: `host` is a boolean def key (0.41.0 §3.5)", () => {
    const { ajv, declSchema } = loadAjv();
    const good = Bun.YAML.parse('defs:\n  open: { cel: "user.level > 1", host: true }\n');
    const okGood = validateAgainst(ajv, declSchema.$id, good);
    expect(okGood.ok, JSON.stringify(okGood.errors)).toBe(true);
    const bad = Bun.YAML.parse('defs:\n  open: { cel: "user.level > 1", host: "yes" }\n');
    expect(validateAgainst(ajv, declSchema.$id, bad).ok).toBe(false);
  });

  test("broken def: no `cel`, an unknown key, or `params` without `type` (E-DEF-DECL)", () => {
    const { ajv, declSchema } = loadAjv();
    for (const def of [
      "{ type: bool }",
      '{ type: bool, cel: "true", body: "x" }',
      '{ params: { n: int }, cel: "n > 1" }',
      "5",
    ]) {
      const doc = Bun.YAML.parse(`defs:\n  bad: ${def}\n`);
      const { ok } = validateAgainst(ajv, declSchema.$id, doc);
      expect(ok, def).toBe(false);
    }
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
    members: [sway, fadeOut, hide]
    exits: [fadeOut, hide]
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

  // 0.51.0 §4.1: the loader trims a raise target (`RaiseTargetDecl`), so
  // the schema accepts every spelling the loader does.
  test("plugin raises: directive, reward-kind and occasion files validate", () => {
    const { ajv, pluginSchema } = loadAjv();
    for (const yaml of [
      "directives:\n  - name: playScene\n    attrs:\n      - { name: scene, type: string }\n" +
        "    effects: { raises: [{ occasion: cutscene, target: \" @scene \" }, { occasion: idle }] }\n",
      "rewardKinds:\n  SHOW: { effects: { raises: [{ occasion: cutscene, target: \" scene.chapter2 \" }] } }\n",
      "occasions:\n  cutscene: { target: { prefix: scene, entity: cutsceneId }, raisedBy: declared }\n",
    ]) {
      const { ok, errors } = validateAgainst(ajv, pluginSchema.$id, Bun.YAML.parse(yaml));
      expect(ok, `${yaml}: ${JSON.stringify(errors)}`).toBe(true);
    }
  });

  // 0.51.0 §6.1: `target: pair` states `open`/`close` (optional `inside`,
  // `scope`) and refuses `when`; other targets state `when` only — the
  // loader's shape check.
  test("plugin lints: pair rule shape", () => {
    const { ajv, pluginSchema } = loadAjv();
    const file = (rule) => Bun.YAML.parse(`lints:\n  - id: g\n    message: m\n${rule}`);
    for (const rule of [
      "    target: pair\n    open: 'true'\n    close: 'false'\n",
      "    target: pair\n    open: 'true'\n    close: 'false'\n    inside: 'true'\n    scope: section\n",
      "    target: pair\n    open: 'true'\n    close: 'false'\n    scope: document\n",
      "    target: node\n    when: 'true'\n",
    ]) {
      const { ok, errors } = validateAgainst(ajv, pluginSchema.$id, file(rule));
      expect(ok, `${rule}: ${JSON.stringify(errors)}`).toBe(true);
    }
    for (const rule of [
      "    target: pair\n    when: 'true'\n    open: 'true'\n    close: 'true'\n",
      "    target: pair\n    open: 'true'\n",
      "    target: pair\n    open: 'true'\n    close: 'true'\n    scope: shot\n",
      "    target: node\n    when: 'true'\n    inside: 'true'\n",
      "    target: node\n    when: 'true'\n    open: null\n",
      "    target: pair\n    when: null\n    open: 'true'\n    close: 'true'\n",
      "    target: pair\n    open: 'true'\n    close: 'true'\n    inside: null\n",
      "    target: pair\n    open: 'true'\n    close: 'true'\n    scope: null\n",
      "    target: scene\n    when: 'true'\n    scope: body\n",
      "    target: line\n",
      "    target: node\n    when: 'true'\n    extra: 1\n",
    ]) {
      expect(validateAgainst(ajv, pluginSchema.$id, file(rule)).ok, rule).toBe(false);
    }
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
    ["docs/examples/arcia-project/vocabulary.schema.yaml", "decl"],
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

  test("real shipped rewardKinds exports validate", () => {
    const { ajv, pluginSchema } = loadAjv();
    for (const path of [
      "docs/examples/games/monster-league/plugins/league.engine/rewardkinds/league.yaml",
      "docs/examples/games/starfall-gacha/plugins/starfall.engine/rewardkinds/starfall.yaml",
      "docs/examples/games/lamplight/plugins/lamplight.occasions/rewardkinds/case.yaml",
    ]) {
      const doc = Bun.YAML.parse(readFileSync(`${ROOT}/${path}`, "utf8"));
      const { ok, errors } = validateAgainst(ajv, pluginSchema.$id, doc);
      expect(ok, `${path}: ${JSON.stringify(errors)}`).toBe(true);
    }
  });

  // 0.49.0 §3.3 (#168): `forbidden: true` is admitted, and only alone.
  test("rewardKinds target: forbidden alone validates; combined with another key it does not", () => {
    const { ajv, pluginSchema } = loadAjv();
    const kinds = (target) => ({ rewardKinds: { COINS: { target } } });
    expect(validateAgainst(ajv, pluginSchema.$id, kinds({ forbidden: true })).ok).toBe(true);
    expect(validateAgainst(ajv, pluginSchema.$id, kinds({ provider: "items" })).ok).toBe(true);
    expect(validateAgainst(ajv, pluginSchema.$id, kinds({ forbidden: false, required: true })).ok).toBe(true);
    for (const bad of [
      { forbidden: true, provider: "items" },
      { forbidden: true, entity: "keepsake" },
      { forbidden: true, required: true },
      { forbidden: true, required: false },
      { provider: "items", entity: "keepsake" },
      { forbade: true },
    ]) {
      expect(validateAgainst(ajv, pluginSchema.$id, kinds(bad)).ok, JSON.stringify(bad)).toBe(false);
    }
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

  test("real shipped arcia.minigame assetkinds/art.yaml validates", () => {
    const { ajv, pluginSchema } = loadAjv();
    const doc = Bun.YAML.parse(
      readFileSync(
        `${ROOT}/docs/examples/arcia-project/plugins/arcia.minigame/assetkinds/art.yaml`,
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

  test("broken DirectivesFile: directive missing required `attrs`", () => {
    const { ajv, pluginSchema } = loadAjv();
    const doc = Bun.YAML.parse("directives:\n  - name: bad\n");
    const { ok } = validateAgainst(ajv, pluginSchema.$id, doc);
    expect(ok).toBe(false);
  });

  test("good StampAttrsFile (plugin §14.1 `stampAttrs` export) validates", () => {
    const { ajv, pluginSchema } = loadAjv();
    const doc = Bun.YAML.parse(
      "stampAttrs:\n  - { name: bonusId, type: string }\n  - { name: bonusScore, type: double }\n",
    );
    const { ok, errors } = validateAgainst(ajv, pluginSchema.$id, doc);
    expect(ok, JSON.stringify(errors)).toBe(true);
  });

  test("PluginManifest may declare the `stampAttrs` export", () => {
    const { ajv, pluginSchema } = loadAjv();
    const doc = Bun.YAML.parse(
      "id: demo.plugin\nversion: 0.1.0\nkind: capability\nexports:\n  stampAttrs: stampattrs/\n",
    );
    const { ok, errors } = validateAgainst(ajv, pluginSchema.$id, doc);
    expect(ok, JSON.stringify(errors)).toBe(true);
  });

  test("PluginManifest is closed: old export spelling, unknown key, other kind", () => {
    const { ajv, pluginSchema } = loadAjv();
    for (const text of [
      "id: p\nversion: 0.1.0\nkind: capability\nexports:\n  rewardkinds: rewardkinds/\n",
      "id: p\nversion: 0.1.0\nkind: capability\ndependencies: []\nexports: {}\n",
      "id: p\nversion: 0.1.0\nkind: plugin\nexports: {}\n",
    ]) {
      const { ok } = validateAgainst(ajv, pluginSchema.$id, Bun.YAML.parse(text));
      expect(ok, text).toBe(false);
    }
  });

  test("broken StampAttrsFile: entry missing required `type`", () => {
    const { ajv, pluginSchema } = loadAjv();
    const doc = Bun.YAML.parse("stampAttrs:\n  - { name: bonusId }\n");
    const { ok } = validateAgainst(ajv, pluginSchema.$id, doc);
    expect(ok).toBe(false);
  });
});

describe("generated runtime contract schemas", () => {
  test("every conformance and contract-fixture line validates", () => {
    const events = JSON.parse(readFileSync(`${ROOT}/schemas/lute-events-0.52.schema.json`, "utf8"));
    const snapshots = JSON.parse(readFileSync(`${ROOT}/schemas/lute-snapshot-0.52.schema.json`, "utf8"));
    const ajv = new (Ajv2020)({ allErrors: true, strict: false });
    const eventValidate = ajv.compile(events);
    const snapshotValidate = ajv.compile(snapshots);
    const files = [];
    for (const entry of readdirSync(`${ROOT}/conformance/session`, { withFileTypes: true })) {
      if (!entry.isDirectory()) continue;
      for (const name of ["expected.jsonl", "inputs.jsonl"]) {
        const path = `${ROOT}/conformance/session/${entry.name}/${name}`;
        if (existsSync(path)) files.push(path);
      }
    }
    files.push(`${ROOT}/conformance/session/contract-fixtures.jsonl`);
    let count = 0;
    for (const path of files) {
      for (const line of readFileSync(path, "utf8").split("\n").filter(Boolean)) {
        let value = JSON.parse(line);
        let validate = eventValidate;
        if (Object.hasOwn(value, "snapshotVersion")) {
          validate = snapshotValidate;
        } else if (Object.hasOwn(value, "seed") && !Object.hasOwn(value, "output")) {
          value = value.seed;
        } else if (Object.hasOwn(value, "input") && !Object.hasOwn(value, "output")) {
          value = value.input;
        } else if (Object.hasOwn(value, "verdict")) {
          validate = ajv.compile({ $ref: `${events.$id}#/$defs/Verdict` });
        } else if (Object.hasOwn(value, "premise")) {
          validate = ajv.compile({ $ref: `${events.$id}#/$defs/Premise` });
        } else if (Object.hasOwn(value, "open")) {
          validate = ajv.compile({ $ref: `${events.$id}#/$defs/Gate` });
        }
        expect(validate(value), `${path}: ${JSON.stringify(validate.errors)}`).toBe(true);
        count += 1;
      }
    }
    expect(count).toBeGreaterThan(0);
  });
});

// 0.41.0 §4.4: the index `$defs` of the IR schema are generated from the Rust
// index types; every index the corpus compiles must validate against them.
describe("generated IR index schema", () => {
  test("every compiled project.index.json validates as `projectIndex`", () => {
    const ir = JSON.parse(readFileSync(`${ROOT}/schemas/lute-ir-0.52.schema.json`, "utf8"));
    const ajv = new (Ajv2020)({ allErrors: true, strict: false });
    ajv.addSchema(ir);
    const validate = ajv.compile({ $ref: `${ir.$id}#/$defs/projectIndex` });
    const projects = [
      `${ROOT}/crates/lute-runtime-wasm/tests/fixtures/evaluate`,
      `${ROOT}/crates/lute-cli/tests/fixtures/evaluate-defs`,
    ];
    for (const parent of [`${ROOT}/docs/examples`, `${ROOT}/docs/examples/games`]) {
      for (const entry of readdirSync(parent, { withFileTypes: true })) {
        const dir = `${parent}/${entry.name}`;
        if (entry.isDirectory() && existsSync(`${dir}/lute.project.yaml`)) projects.push(dir);
      }
    }
    let defs = 0;
    for (const project of projects) {
      const out = `${tmpdir()}/lute-index-schema-${process.pid}-${Math.random().toString(36).slice(2)}`;
      const run = Bun.spawnSync([luteBin(), "compile", "--project", project, "-o", out], { cwd: ROOT });
      expect(run.success, `${project}: ${run.stderr.toString()}`).toBe(true);
      const index = JSON.parse(readFileSync(`${out}/project.index.json`, "utf8"));
      expect(validate(index), `${project}: ${JSON.stringify(validate.errors)}`).toBe(true);
      defs += Object.keys(index.defs ?? {}).length;
    }
    const committed = JSON.parse(
      readFileSync(`${ROOT}/crates/lute-runtime-wasm/tests/fixtures/hub-once/project.index.json`, "utf8"),
    );
    expect(validate(committed), JSON.stringify(validate.errors)).toBe(true);
    expect(projects.length).toBeGreaterThan(20);
    expect(defs).toBeGreaterThan(0);
  });

  test("an index def row is closed to the documented shape", () => {
    const ir = JSON.parse(readFileSync(`${ROOT}/schemas/lute-ir-0.52.schema.json`, "utf8"));
    const ajv = new (Ajv2020)({ allErrors: true, strict: false });
    ajv.addSchema(ir);
    const def = ajv.compile({ $ref: `${ir.$id}#/$defs/indexDef` });
    const row = {
      cel: "run.gold >= n",
      expr: { op: ">=", l: { path: "run.gold" }, r: { param: "n" } },
      params: [{ name: "n", type: "int" }],
      host: true,
    };
    expect(def(row), JSON.stringify(def.errors)).toBe(true);
    expect(def({ ...row, params: [{ name: "n", type: "number" }] })).toBe(false);
    expect(def({ cel: "x" })).toBe(false);
  });
});

// 0.51.0 §4.6 (S5 re-review): a `dynamic` raise has a target, the
// `occasion.target` reference with or without a prefix; the runtime's load
// checks the prefix is the occasion's declared one.
describe("IR raise records", () => {
  test("a dynamic raise needs the reference as its target", () => {
    const ir = JSON.parse(readFileSync(`${ROOT}/schemas/lute-ir-0.52.schema.json`, "utf8"));
    const ajv = new (Ajv2020)({ allErrors: true, strict: false });
    ajv.addSchema(ir);
    const raise = ajv.compile({ $ref: `${ir.$id}#/$defs/raiseRecord` });
    for (const ok of [
      { occasion: "cutscene", target: "scene.occasion.target", dynamic: true },
      { occasion: "bell", target: "occasion.target", dynamic: true },
      { occasion: "cutscene", target: "scene.other" },
      { occasion: "cutscene", target: "scene.other", dynamic: false },
      { occasion: "idle" },
    ]) {
      expect(raise(ok), `${JSON.stringify(ok)}: ${JSON.stringify(raise.errors)}`).toBe(true);
    }
    for (const bad of [
      { occasion: "cutscene", target: "scene.other", dynamic: true },
      { occasion: "cutscene", target: ".occasion.target", dynamic: true },
      { occasion: "cutscene", dynamic: true },
    ]) {
      expect(raise(bad), JSON.stringify(bad)).toBe(false);
    }
  });
});

// 0.50.0 §3.5: `<choice>` options carry the plugin stamps they author; the
// option `$defs` are closed, so the schema must name `stamps`.
describe("IR option stamps", () => {
  test("a compiled document with element and option stamps validates", () => {
    const ir = JSON.parse(readFileSync(`${ROOT}/schemas/lute-ir-0.52.schema.json`, "utf8"));
    const ajv = new (Ajv2020)({ allErrors: true, strict: false });
    const validate = ajv.compile(ir);
    const project = `${ROOT}/crates/lute-cli/tests/fixtures/plugin-stamps-050-logic`;
    const run = Bun.spawnSync(
      [luteBin(), "compile", "--project", project, `${project}/scenes/ok.lute`],
      { cwd: ROOT },
    );
    expect(run.success, run.stderr.toString()).toBe(true);
    const doc = JSON.parse(run.stdout.toString());
    const menus = doc.commands.filter((c) => c.kind === "choice" || c.kind === "hub");
    expect(menus.map((c) => c.kind)).toEqual(["choice", "hub"]);
    for (const menu of menus) {
      expect(menu.stamps).toBeDefined();
      for (const option of menu.options) expect(option.stamps).toBeDefined();
    }
    expect(validate(doc), JSON.stringify(validate.errors)).toBe(true);
    // The closed option shape still refuses an undocumented key.
    menus[0].options[0].extra = true;
    expect(validate(doc)).toBe(false);
  });
});

// --- the `lute check --json` report (0.48.0 §3.1), both modes ---

describe("lute check report schema", () => {
  const REPORT_SCHEMA_PATH = `${ROOT}/schemas/lute-check-report-0.48.schema.json`;

  function reportValidator() {
    const ajv = new Ajv({ allErrors: true, strict: false });
    const schema = JSON.parse(readFileSync(REPORT_SCHEMA_PATH, "utf8"));
    expect(schema.$schema).toBe("http://json-schema.org/draft-07/schema#");
    expect(ajv.validateSchema(schema), JSON.stringify(ajv.errors)).toBe(true);
    return ajv.compile(schema);
  }

  function checkJson(args, cwd = ROOT) {
    const out = Bun.spawnSync([luteBin(), "check", ...args, "--json"], { cwd });
    return { code: out.exitCode, json: JSON.parse(out.stdout.toString()) };
  }

  test("document mode (two files) validates", () => {
    const validate = reportValidator();
    const { code, json } = checkJson([
      "docs/examples/haven/scenes/archive.lute",
      "docs/examples/haven/scenes/bridge.lute",
    ]);
    expect(code).toBe(0);
    expect(json.files.map((f) => f.path)).toEqual([
      "docs/examples/haven/scenes/archive.lute",
      "docs/examples/haven/scenes/bridge.lute",
    ]);
    expect(json.project_diagnostics).toBeUndefined();
    expect(validate(json), JSON.stringify(validate.errors)).toBe(true);
  });

  test("whole-project mode validates and carries project_diagnostics", () => {
    const validate = reportValidator();
    for (const project of ["docs/examples/haven", "docs/examples/quayside"]) {
      const { code, json } = checkJson(["--project", project]);
      expect(code, project).toBe(0);
      expect(json.files.length, project).toBeGreaterThan(1);
      expect(Array.isArray(json.project_diagnostics), project).toBe(true);
      expect(validate(json), `${project}: ${JSON.stringify(validate.errors)}`).toBe(true);
    }
  });

  test("a failing document and a schema operand validate", () => {
    const validate = reportValidator();
    const dir = `${tmpdir()}/lute-check-report-${process.pid}-${Math.random().toString(36).slice(2)}`;
    mkdirSync(dir, { recursive: true });
    writeFileSync(`${dir}/bad.lute`, "---\nkind: scene\nid: bad\n---\n@narrator: [broken\n");
    const bad = checkJson([`${dir}/bad.lute`, "docs/examples/haven/world.schema.yaml"]);
    expect(bad.code).toBe(1);
    expect(bad.json.ok).toBe(false);
    expect(bad.json.files[0].ok).toBe(false);
    expect(bad.json.files[0].diagnostics.length).toBeGreaterThan(0);
    expect(bad.json.files[1].resolved).toBeNull();
    expect(validate(bad.json), JSON.stringify(validate.errors)).toBe(true);
  });

  test("an invalid manifest in whole-project mode reports files [] and the manifest diagnostics", () => {
    const validate = reportValidator();
    const dir = `${tmpdir()}/lute-check-manifest-${process.pid}-${Math.random().toString(36).slice(2)}`;
    mkdirSync(dir, { recursive: true });
    writeFileSync(`${dir}/lute.project.yaml`, "profiles: [this is: not valid\n");
    const { code, json } = checkJson(["--project", dir]);
    expect(code).toBe(1);
    expect(json.files).toEqual([]);
    expect(json.project_diagnostics.length).toBeGreaterThan(0);
    expect(json.project_diagnostics[0].path.endsWith("lute.project.yaml")).toBe(true);
    expect(validate(json), JSON.stringify(validate.errors)).toBe(true);
  });

  test("`suppressed` counts and `configured` markers validate (dsl 0.50.0 §6.3-§6.4)", () => {
    const validate = reportValidator();
    const dir = `${tmpdir()}/lute-check-levels-${process.pid}-${Math.random().toString(36).slice(2)}`;
    mkdirSync(`${dir}/scenes`, { recursive: true });
    writeFileSync(
      `${dir}/lute.project.yaml`,
      "defaultProfile: core\nprofiles:\n  core:\n    plugins: {}\ndiagnostics:\n  W-TEXT-GLUE: info\n  W-STAGE-ABSENT: off\n",
    );
    writeFileSync(
      `${dir}/scenes/a.lute`,
      "---\nkind: scene\nid: a\nenums:\n  action:\n    members: [show, hide]\n    exits: [hide]\n  anchor:\n    members: [left, center, right]\n    default: center\n---\n\n## Shot 1.\n\n::actor{character=\"vesna\" action=\"show\"}\n::actor{character=\"vesna\" action=\"hide\"}\n@vesna: Gone <> now.\n",
    );
    const { code, json } = checkJson(["--project", dir]);
    expect(code).toBe(0);
    expect(json.files[0].suppressed).toEqual({ "W-STAGE-ABSENT": 1 });
    expect(json.files[0].diagnostics[0].configured).toBe("lute.project.yaml");
    expect(validate(json), JSON.stringify(validate.errors)).toBe(true);
  });

  test("the schema refuses the 0.47.2 single-document shape", () => {
    const validate = reportValidator();
    expect(
      validate({
        command: "check",
        schemaVersion: "0.43.0.check",
        ok: true,
        diagnostics: [],
        resolved: null,
      }),
    ).toBe(false);
  });
});
