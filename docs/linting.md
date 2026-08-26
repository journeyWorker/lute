# Lute linting

`lute lint [PATH]` reviews project content for configurable editorial and
composition concerns. It collects documents by their nearest `lute.project.yaml`,
computes the same metric tables for every enabled rule, and reports findings in
the normal human or `--json` diagnostic form.

Linting complements, rather than replaces, `lute check`:

- **`lute check`** validates language and project semantics. Its contract,
  including existing `W-TIMELINE-*` thresholds, is unchanged.
- **`lute lint`** evaluates advisory content rules. Levels and thresholds are
  project policy, so lint rules do not participate in the capability snapshot or
  `capabilityVersion` and never change artifact identity.

Run `lute lint` with no configuration to use the built-in rule defaults. The
optional `--config PATH` selects a different config file; otherwise the command
uses `<project root>/lute.lint.yaml`. `--json` uses the existing JSON diagnostic
format.

## `lute.lint.yaml`

Place this file next to `lute.project.yaml`:

```yaml
lsp: true
ignore: ["drafts/**"]

rules:
  dialogue-length:      { level: warn, options: { maxWords: 40 } }
  dialogue-ratio:       { level: error, options: { min: 0.35 } }
  emotion-distribution: { level: warn, options: { pairWith: variant } }
  asset-exists:         { level: error, options: { providers: { bg: bg-catalog } } }
  my-plugin/variant-coverage: off
custom:
  - id: too-many-choices
    target: scene
    when: "scene.choices > options.max"
    level: warn
    message: "scene has {scene.choices} choices (budget {options.max})"
    options: { max: 6 }
```

### Top-level keys

| key | type | default | meaning |
| --- | --- | --- | --- |
| `lsp` | boolean | `false` | Publishes lint findings for open documents in the LSP only when this config file exists and sets it to `true`. Linting is otherwise absent from the LSP, even if rules are configured. LSP evaluates `line`, `shot`, `scene`, `speaker`, and `group` targets; `project` rules are CLI-only in v1. |
| `ignore` | list of project-root-relative globs | `[]` | Documents excluded from linting. |
| `rules` | map | `{}` | Per-rule level and option overrides for core and active plugin rules. |
| `custom` | list of rules | `[]` | Project-local declarative CEL rules. |

A `rules` map entry may be either a level shorthand—for example,
`my-plugin/variant-coverage: off`—or `{ level, options }`. Levels are `off`,
`hint`, `info`, `warn`, and `error`, mapping to diagnostic severities Hint, Info,
Warning, and Error. `off` disables the rule. An omitted level retains that rule's
default. Options deep-merge over the rule's declared defaults; scalar values
override.

Core rule ids are bare kebab-case. Plugin rule ids are
`<plugin-id>/<rule-id>`. A custom rule id is bare and MUST NOT collide with a
core rule id. Malformed YAML is an exit-code-2 error. Unknown rules or option
keys, wrong option types, and colliding custom ids are `E-LINT-CONFIG` findings;
the bad entry is skipped, remaining rules run, and the command exits 1.

### Custom rule shape

Every custom entry has the same shape as a plugin rule:

| key | required | meaning |
| --- | --- | --- |
| `id` | yes | Project-local bare rule id. |
| `target` | yes | `line`, `shot`, `scene`, `speaker`, `group`, or `project`. |
| `when` | yes | A CEL assertion over the target row and `options`; it fires when `true`. |
| `level` | no | `off`, `hint`, `info`, `warn`, or `error`. |
| `message` | yes | Finding text. `{path.to.field}` interpolates a metric or option path. `{expr:%}` renders shares as percentages. |
| `options` | no | Defaults exposed to `when` and the message. |

The lint CEL fragment is ground-only: no `@ref` or `$` DSL tokens, state paths,
or macros (`filter`, `map`, `exists`). It supports the shared ground-operation
semantics, metric-path selection and map indexing, and `size()` for strings,
lists, and maps. An unresolved field, type mismatch, or non-boolean `when` result
is `E-LINT-EXPR`, reported once at the rule declaration, and skips that rule—it is
never a silent pass.

## Metric tables

Rules inspect facts calculated once per run. Word counts split the line text on
whitespace (each interpolation counts as one token); character counts are Unicode
scalar counts.

| target | row | fields |
| --- | --- | --- |
| `line` | each content `Line` | `words`, `chars`, `speaker` (`""` for narration), `attrs` (string map; `BoolTrue` is `"true"`) |
| `shot` | each `##` shot | `index` (1-based), `title`, `dialogueLines`, `words`, `firstStagingTag` (or `""`) |
| `scene` | each document | `dialogueLines`, `words`, `bodyNodes` (nested included), `directives`, `sets`, `choices`, `shots`, `maxLineWords`, `avgLineWords`, `dialogueRatio` |
| `speaker` | each document/speaker with dialogue | `lines`, `words`, `axis`, `attrShare` |
| `group` | each document/attribute/value for configured `groupBy` | `attr`, `key`, `count`, `speakers` |
| `project` | project root | `scenes`, `sceneWords`, `spreadRatio` |

`scene.dialogueRatio` is `dialogueLines / bodyNodes`, or `0.0` when there are no
body nodes. `project.sceneWords` is `{ min, max, mean, stddev }` over document
word counts; `spreadRatio` is `max / min`, or `0.0` when `min == 0` or fewer than
two scenes exist.

`speaker.axis` is observed from the speaker's line attributes, not preconfigured:
it has an entry for every observed domain slot and for every observed
slot-plus-stamp-attribute pair (for example, `emotion+variant`). Missing values
bucket as `""`. An axis entry exposes `run`, `runValue`, `streaks`, `streakAvg`,
`distinct`, and `top { value, count, share }`. `run` is the longest same-value
streak; `streakAvg` is lines divided by streaks; `top.share` is top-bucket count
divided by lines. `speaker.attrShare` maps each attribute name to the share of the
speaker's lines carrying it.

## Core rules

All seven core rules are enabled by default. `asset-exists` is inert without a
provider mapping, and `variant-composition` is inert until a line carries its
configured attribute; a bare project therefore has no false noise from either.
Set any rule to `level: error` to make it release-blocking for that project.

| rule | target | default | trigger and defaults |
| --- | --- | --- | --- |
| `dialogue-length` | line | warn | `line.words > maxWords`; `maxWords: 40`. Keeps individual lines readable and performable. |
| `dialogue-ratio` | scene | warn | `bodyNodes >= minNodes` and `dialogueRatio < min`; `minNodes: 10`, `min: 0.35`. Flags scenes with too little dialogue relative to their authored body. |
| `scene-length-spread` | project | warn | `scenes >= 2` and `spreadRatio > maxRatio`; `maxRatio: 3.0`. Finds an unusually uneven scene-length mix. |
| `shot-starts-with-background` | shot | warn | `firstStagingTag != "bg"`, including a shot with no staging directive. Encourages each shot to establish its background first. |
| `emotion-distribution` | speaker | warn | Checks the selected axis once a speaker has at least `minLines: 10`: `domain: emotion`, optional `pairWith`, `runMax: 3`, `streakAvgMin: 1.5`, `maxShare: 0.4`. It applies the bard lineage's hard cap of three identical emotion streaks, thrash floor of 1.5 average streak length, and 40% dominance cap; when `pairWith` is set it also checks the paired axis. One finding per failing speaker joins all reasons. |
| `variant-composition` | speaker and group | warn | `attr: variant`, optional `groupBy`, `minPerGroup: 2`, `minShare: 0.0`, `minLines: 10`. With `groupBy`, groups below `minPerGroup` fire. With `minShare > 0`, speakers meeting its own `minLines` whose `attrShare[attr]` is below the threshold fire. |
| `asset-exists` | line/directive | error | `providers: {}` (inert), `sentinels: [clear, empty, false, none, null, stop]`. For each mapped directive tag, checks `assetId` against the pinned provider snapshot. Absent assets fire; stale catalog data downgrades the finding to warn. Sentinel values are case-insensitively exempt. |

## Plugin rule authoring

A plugin exports `lints/*.yaml`, each a list of the same rule shape used by
`custom`:

```yaml
- id: variant-coverage
  target: speaker
  when: 'speaker.lines >= 10 && speaker.attrShare["variant"] < options.minShare'
  level: warn
  message: "speaker has {speaker.lines} dialogue lines"
  options: { minShare: 0.5 }
```

The loader keeps `id` raw. When a plugin id is `my-plugin`, the active rule id is
`my-plugin/variant-coverage`, which is the name users put under `rules:` and the
basis for its diagnostic code. A malformed plugin entry is `E-LINT-RULE` at load
time and is skipped. Plugin rules come from the project's **default profile**
activation — one rule set per project root, applied to every document
(per-document profile activation is a future refinement).

A plugin rule is data only when it asserts over these fixed, core-computed metric
tables. If the rule needs another metric, traversal, ordering rule, or evaluation
primitive, that is a core change—not a YAML extension. Plugin lint declarations
are advisory and are excluded from both the capability snapshot and
`capabilityVersion`.

## Codes, denial, and exit status

A lint finding code is `L-` plus the uppercased rule id, replacing `/` and every
non-alphanumeric character with `-`: `dialogue-length` becomes
`L-DIALOGUE-LENGTH`; `my-plugin/variant-coverage` becomes
`L-MY-PLUGIN-VARIANT-COVERAGE`.

The built-in rule codes are:

| rule | code |
| --- | --- |
| `dialogue-length` | `L-DIALOGUE-LENGTH` |
| `dialogue-ratio` | `L-DIALOGUE-RATIO` |
| `scene-length-spread` | `L-SCENE-LENGTH-SPREAD` |
| `shot-starts-with-background` | `L-SHOT-STARTS-WITH-BACKGROUND` |
| `emotion-distribution` | `L-EMOTION-DISTRIBUTION` |
| `variant-composition` | `L-VARIANT-COMPOSITION` |
| `asset-exists` | `L-ASSET-EXISTS` |

Use `--deny CODE` repeatedly to promote particular lint codes to errors without
changing their other semantics. `--deny-warnings` promotes warnings as well. These
promotions affect the exit status.

| exit | meaning |
| --- | --- |
| `0` | Clean, or only findings below error severity. |
| `1` | At least one error-severity lint finding (native or denial-promoted), or a semantic config error. |
| `2` | I/O error, malformed YAML, or CLI usage error. |
