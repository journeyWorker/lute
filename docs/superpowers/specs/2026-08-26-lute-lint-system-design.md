# Lute Lint System — Design

**Status:** approved design, pre-implementation.
**Scope:** a configurable, plugin-extensible lint layer (`lute lint`) for content/metric
advisories — distinct from `lute check`'s semantic validation.
**Prior art carried over:** bard's `lute-core/scenario` validator + emotion-dist analyzer
(emotion/variant distribution, streak caps, dialogue metrics, asset catalog checks),
oxlint's config-file + rule-level model.

## 1. Goals / non-goals

Goals:

- Port bard's scenario lints onto the new Lute toolchain: emotion distribution &
  streaks, variant composition, dialogue ratio, dialogue length, scene length spread,
  missing-asset validation, shot-starts-with-background.
- One config file (`lute.lint.yaml`) controlling rule levels, thresholds, and
  project-local custom rules.
- Plugin extensibility via a new `lints` export kind — declarative CEL rules only.
- Reuse the existing `lute_core_span::Diagnostic` model, CLI rendering, and exit-code
  contract.

Non-goals (v1):

- No executable (WASM/process) lint plugins.
- No migration of existing hardcoded `W-TIMELINE-*` thresholds out of `lute check` —
  check's contract is untouched.
- No CEL macros (`filter`/`map`/`exists`) in lint expressions.
- Lint rules do **not** participate in the capability snapshot or `capabilityVersion`:
  lints are advisory and must never change artifact identity.

## 2. Surfaces

- **`lute lint [PATH]`** — new CLI subcommand. Reuses `check-project`'s document
  collection (group by nearest `lute.project.yaml` root), computes metric tables,
  evaluates rules, renders with the existing human/`--json` forms.
  Flags: `--json`, `--deny CODE` (repeatable), `--deny-warnings`, `--config PATH`
  (defaults to `<project root>/lute.lint.yaml`).
- **LSP** — opt-in only: lint diagnostics are published for open documents iff a
  `lute.lint.yaml` exists at the project root **and** it sets `lsp: true`.
  LSP evaluates document-scoped targets only (`line`, `shot`, `scene`, `speaker`,
  `group`); `project`-target rules are CLI-only in v1.
- **`lute check` is untouched.**

Exit codes (same contract as `check`): `0` clean or only sub-error findings; `1` any
error-severity lint finding (native or `--deny`-promoted) or semantic config error;
`2` I/O, malformed YAML, or CLI usage error.

## 3. Configuration — `lute.lint.yaml`

Lives next to `lute.project.yaml`. Absent file ⇒ `lute lint` runs with built-in
defaults; LSP lint stays off.

```yaml
lsp: true                       # default false
ignore: ["drafts/**"]           # doc paths excluded from linting (project-root-relative globs)

rules:                          # level/threshold overrides for core + plugin rules
  dialogue-length:      { level: warn, options: { maxWords: 40 } }
  dialogue-ratio:       { level: error, options: { min: 0.35 } }
  emotion-distribution: { level: warn, options: { pairWith: variant } }
  asset-exists:         { level: error, options: { providers: { bg: bg-catalog } } }
  my-plugin/variant-coverage: off        # shorthand: level only

custom:                         # project-local CEL rules (no plugin needed)
  - id: too-many-choices
    target: scene
    when: "scene.choices > options.max"
    level: warn
    message: "scene has {scene.choices} choices (budget {options.max})"
    options: { max: 6 }
```

- **Levels:** `off | hint | info | warn | error` → `Severity::{Hint, Info, Warning,
  Error}`. `error` findings drive exit 1.
- **Rule ids:** core rules are bare kebab-case; plugin rules are
  `<plugin-id>/<rule-id>`; `custom` ids are bare and MUST NOT collide with a core rule
  id (collision ⇒ `E-LINT-CONFIG`).
- **Config errors:** malformed YAML ⇒ exit 2. Semantic errors — unknown rule id,
  unknown option key, bad option type, colliding custom id — are `E-LINT-CONFIG`
  diagnostics (Error severity, anchored to the config file) and the offending entry is
  skipped; the run continues, exits 1.
- Option maps deep-merge over a rule's declared defaults; scalars override.

## 4. Metric tables

The engine computes fact tables once per run; every rule (core, plugin, custom) is an
assertion over them. Traversal generalizes `lute-cli/src/loc.rs`'s translatable-unit
walk over `lute-syntax` AST (`Document.shots`, nested `Choice`/`Arm`/`Objective`/`On`
bodies, quests).

Word count = whitespace-split token count of the line text (interpolations count as
one token each); char count = Unicode scalar count.

| target | one row per | fields |
|---|---|---|
| `line` | content `Line` node | `words`, `chars`, `speaker` (`""` for narration), `attrs` (map: string values; `BoolTrue` → `"true"`) |
| `shot` | `##` shot | `index` (1-based), `title`, `dialogueLines`, `words`, `firstStagingTag` (tag of first staging directive in the shot body, `""` if none) |
| `scene` | document | `dialogueLines`, `words`, `bodyNodes` (all body nodes, nested included), `directives`, `sets`, `choices`, `shots`, `maxLineWords`, `avgLineWords`, `dialogueRatio` (= `dialogueLines / bodyNodes`, `0.0` when `bodyNodes == 0`) |
| `speaker` | (document, speaker), dialogue lines only | `lines`, `words`, `axis` (map, below), `attrShare` (map: attr name → share of this speaker's lines carrying it) |
| `group` | (document, attr, value) for the configured `groupBy` attr | `attr`, `key` (the value), `count`, `speakers` (distinct speaker count) |
| `project` | project root | `scenes`, `sceneWords` (`{min, max, mean, stddev}` over per-document `words`), `spreadRatio` (`max/min`; `0.0` when `min == 0` or `scenes < 2`) |

**`speaker.axis` keys** are derived from observation, not config: one entry per domain
slot appearing on that speaker's lines (e.g. `"emotion"`), plus one per
(slot × stamp-attr) pair observed (key `"emotion+variant"`). Each axis value, computed
over the speaker's lines in document order (a line missing the key buckets as `""`):

```
run        longest identical-bucket streak (int)
runValue   bucket of that longest run
streaks    number of streaks (transitions + 1)
streakAvg  lines / streaks (double)
distinct   distinct bucket count
top        { value, count, share }   share = count / lines
```

This reproduces bard's analyzer axes: run hard cap, thrash floor (streakAvg),
dominance — for both the emotion axis and the sprite (emotion×variant) axis.

## 5. Expression language — the lint CEL fragment

- Parsing: `lute_cel::parse_slot` (existing, parse-only stays true).
- Evaluation: a new ground evaluator in `lute-lint` over metric rows. It reuses
  `lute_check::apply_op`'s R3 ground-operation semantics (D3 — the same table
  `decide()` and `lute-trace` share), extended with:
  - `Ident`/`Select`/`Index` resolution into the bound row (`line.`, `shot.`,
    `scene.`, `speaker.`, `group.`, `project.`) and `options.*`;
  - map indexing (`speaker.axis["emotion"].run`);
  - `size(x)` on strings/lists/maps.
- No `@ref`/`$` DSL tokens, no state paths, no macros. Everything is ground; there is
  no "undecided" — an unresolvable field, type mismatch, or non-boolean `when` result
  is **`E-LINT-EXPR`** (Error, anchored to the rule's declaration site: config file or
  plugin YAML), reported once per rule, and the rule is skipped. Never a silent pass.
- `when` fires the rule when it evaluates `true`.
- **Message templates:** `{path.to.field}` interpolation only (a metric/options path
  evaluated in the same row env; numbers rendered trimmed, `share` fields as
  percentages when formatted via `{expr:%}`). No general CEL in templates.

## 6. Rule model

```yaml
id: variant-coverage          # namespaced by consumer: plugin rules become <plugin-id>/<id>
target: speaker               # line | shot | scene | speaker | group | project
when: "speaker.lines >= 10 && speaker.attrShare[\"variant\"] < options.minShare"
level: warn                   # default level; lute.lint.yaml overrides
message: "…"
options: { minShare: 0.5 }    # defaults; overridable per-project
```

Three sources, one shape:

1. **Core data rules** — embedded YAML in `lute-lint`, evaluated by the same engine
   (dogfooding): `dialogue-length`, `dialogue-ratio`, `scene-length-spread`,
   `shot-starts-with-background`.
2. **Core Rust rules** — logic beyond one assertion: `emotion-distribution`,
   `variant-composition`, `asset-exists`. Each still declares id/target/level/options
   metadata identically, so config handling is uniform.
3. **Plugin rules** — new export kind `lints` (`lints/*.yaml`, list of the rule shape
   above). Loader gets a `lints` arm and `LoadedPlugin.lints`;
   **excluded from `CapabilitySnapshot` and the `capabilityVersion` hash.**
   For each document, active plugin rules = plugins resolved for that document's
   profile; `project`-target plugin rules use the default profile's plugin set.
4. **Custom rules** — `custom:` in `lute.lint.yaml`, same shape.

## 7. v1 core rules and defaults

| id | target | default | logic / options (defaults) |
|---|---|---|---|
| `dialogue-length` | line | warn | `line.words > maxWords` (40) |
| `dialogue-ratio` | scene | warn | fires when `bodyNodes >= minNodes` (10) and `dialogueRatio < min` (0.35) |
| `scene-length-spread` | project | warn | fires when `scenes >= 2 && spreadRatio > maxRatio` (3.0) |
| `shot-starts-with-background` | shot | warn | `shot.firstStagingTag != "bg"` (fires also when the shot has no staging at all) |
| `emotion-distribution` | speaker | warn | Rust. Options: `domain` ("emotion"), `pairWith` (unset), `minLines` (10, below ⇒ skip), `runMax` (3), `streakAvgMin` (1.5), `maxShare` (0.4). Checks the `domain` axis and, when `pairWith` set, the pair axis. One diagnostic per failing speaker, reasons joined (`emotion-run=5 (cap 3); emotion-share=72% (cap 40%)`) — bard's verdict format. |
| `variant-composition` | speaker+group | warn | Rust. Options: `attr` ("variant"), `groupBy` (unset), `minPerGroup` (2), `minShare` (0.0). Inert when no line carries `attr`. With `groupBy`: groups with `count < minPerGroup` fire. With `minShare > 0`: speakers (≥ `minLines` of emotion-distribution? no — own `minLines`, 10) whose `attrShare[attr] < minShare` fire. |
| `asset-exists` | line/directive | error | Rust. Options: `providers` (map: directive tag → provider id, empty ⇒ inert), `sentinels` (`[clear, empty, false, none, null, stop]`, case-insensitive exempt). Resolves each mapped directive's `assetId` attr against the pinned provider snapshot (`IdStatus`): `Absent` ⇒ fire; `Stale` ⇒ downgrade to warn with catalog-stale wording. Closes the gap that core `::bg`/`::sfx`/`::music` `assetId`s are plain strings `check` never validates. |

All enabled by default. `asset-exists` and `variant-composition` are inert until
configured (provider mapping / attr presence), so a bare project gets no false noise.
bard's block-at-ERROR policy is reproduced per-project by setting `level: error`.

## 8. Diagnostics

- Code = `L-` + rule id uppercased, `/` and non-alphanumerics → `-`:
  `L-DIALOGUE-LENGTH`, `L-MY-PLUGIN-VARIANT-COVERAGE`.
- Spans: `line`/`shot` rules anchor to the node; `scene` to the frontmatter/title;
  `speaker` to that speaker's first dialogue line in the document; `group` to the
  first member line; `project` to the first contributing file with
  `RelatedDiagnostic` entries for the others.
- Severity from resolved level. `--deny` interops: lint codes join the deniable set
  (promotion only, unchanged semantics).
- Engine/config diagnostics: `E-LINT-CONFIG` (config semantics), `E-LINT-EXPR`
  (expression failure), `E-LINT-RULE` (malformed plugin rule YAML — reported at load,
  entry skipped).

## 9. Crate layout

New crate `crates/lute-lint`: `config.rs` (lute.lint.yaml), `metrics.rs` (tables),
`eval.rs` (lint CEL fragment), `rules.rs` (registry: embedded data rules + Rust
rules + plugin/custom intake), `engine.rs` (orchestration:
`lint(docs, config, plugin_rules, providers) -> Vec<(PathBuf, Diagnostic)>`).
Depends on `lute-syntax`, `lute-check` (apply_op, CheckInput reuse), `lute-cel`,
`lute-manifest`, `lute-core-span`. CLI and LSP consume it; `lute-check` does not
depend on it.

## 10. Testing

- `lute-lint` unit tests per rule (fixture docs → expected findings), evaluator tests
  (ops, map indexing, E-LINT-EXPR paths), config tests (levels, merge, errors).
- `lute-manifest` loader test for the `lints` export (parse, namespace, snapshot-hash
  non-participation — hash byte-identical with/without lints).
- CLI integration `tests/lint.rs`: default run, config overrides, `off`, custom rule,
  `--json` shape, `--deny` promotion, exit codes, ignore globs.
- LSP: opt-in gating test (no config / `lsp: false` ⇒ no lint diagnostics).
