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
  `capabilitySnapshot` and never change artifact identity.

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
| `lsp` | boolean | `false` | Publishes lint findings for open documents in the LSP only when this config file exists and sets it to `true`. Linting is otherwise absent from the LSP, even if rules are configured. LSP evaluates `line`, `section`, `scene`, `speaker`, `group`, and `node` targets; `project` rules are CLI-only in v1. |
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
| `target` | yes | `line`, `section`, `scene`, `speaker`, `group`, `project`, or `node`. |
| `when` | yes | A CEL assertion over the target row and `options`; it fires when `true`. |
| `level` | no | `off`, `hint`, `info`, `warn`, or `error`. |
| `message` | yes | Finding text. `{path.to.field}` interpolates a metric or option path. `{expr:%}` renders shares as percentages. |
| `options` | no | Defaults exposed to `when` and the message. |

The lint CEL fragment is ground-only: no `@ref` or `$` DSL tokens, state paths,
or macros (`filter`, `map`, `exists`). It supports the shared ground-operation
semantics, metric-path selection and map indexing, `size()` for strings,
lists, and maps, and the string members `s.startsWith(t)`, `s.endsWith(t)` and
`s.contains(t)` (member-call form only; both operands must be strings). There
are no regular expressions. An unresolved field (an absent map key included), a
type mismatch, or a non-boolean `when` result is `E-LINT-EXPR`, reported once at
the rule declaration, and skips that rule for that document—it is never a
silent pass. `&&` and `||` short-circuit, so a rule guards each key it indexes
(`"x" in node.attrs && node.attrs["x"] == "y"`).

## Metric tables

Rules inspect facts calculated once per run. Word counts split the line text on
whitespace (each interpolation counts as one token); character counts are Unicode
scalar counts.

| target | row | fields |
| --- | --- | --- |
| `line` | each content `Line` | `words`, `chars`, `speaker` (`""` for narration), `attrs` (string map; `BoolTrue` is `"true"`) |
| `section` | each `##` section | `index` (1-based), `title`, `dialogueLines`, `words`, `firstStagingTag` (or `""`), `kind` |
| `scene` | each document | `kind`, `dialogueLines`, `words`, `bodyNodes` (nested included), `directives`, `sets`, `choices`, `sections`, `maxLineWords`, `avgLineWords`, `dialogueRatio` |
| `speaker` | each document/speaker with dialogue | `lines`, `words`, `axis`, `attrShare` |
| `group` | each document/attribute/value for configured `groupBy` | `attr`, `key`, `count`, `speakers` |
| `project` | project root | `scenes`, `sceneWords`, `spreadRatio` |
| `node` | each body node | `kind`, `tag`, `speaker`, `attrs`, `index`, `first`, `last`, `depth`, `prev`, `next`, `parent`, `preceding`, `section` (see [Node rows](#node-rows)) |

`scene.kind` classifies the document (dsl 0.22.0): `scene` for a linear scene,
`beat` for a scene with `on:`, `component` for a document declaring
`component:`, and otherwise its authored `kind:` (`quest`, `lore`, …);
`section.kind` repeats its document's value. `scene.directives` and
`section.firstStagingTag` count staging directives only — `::accept`, `::use`,
`::end`, `::label`, and `::jump` are skipped.

`scene.dialogueRatio` is `dialogueLines / bodyNodes`, or `0.0` when there are no
body nodes. `project.scenes` counts linear scenes only (`scene.kind == "scene"`),
and `project.sceneWords` is `{ min, max, mean, stddev }` over their word counts;
`spreadRatio` is `max / min`, or `0.0` when `min == 0` or fewer than two scenes
exist. Numbers interpolated into a message are rounded to at most two decimals.

`speaker.axis` is observed from the speaker's line attributes, not preconfigured:
it has an entry for every observed domain slot and for every observed
slot-plus-stamp-attribute pair (for example, `emotion+variant`). Missing values
bucket as `""`. An axis entry exposes `run`, `runValue`, `streaks`, `streakAvg`,
`distinct`, and `top { value, count, share }`. `run` is the longest same-value
streak; `streakAvg` is lines divided by streaks; `top.share` is top-bucket count
divided by lines. `speaker.attrShare` maps each attribute name to the share of the
speaker's lines carrying it.

### Node rows

A `node` rule sees one row per node, in document order, in every body the
checker walks: section bodies; `<quest>` declarations and their bodies
(objectives and `<on>` handlers), with each `<reward>` placed at its source
position among its owner's children; top-level `<entry>` and `<beat>`
declarations and their bodies; `<branch>` and `<hub>` choices and choice
bodies, and a hub's `<return>` body; `<match>` arms (`<when>`, `<otherwise>`)
and their bodies; and `<timeline>` as one row (its track clips are not rows).
A `node` rule binds `node`, `scene` (the document row above) and `options`.

| field | meaning |
| --- | --- |
| `kind` | `line` for a content line; `directive` for every `::tag{…}`, including `::set`, `::assert`, `::retract`, `::accept`, `::use`, `::label`, `::jump`, `::clear` and `::end`; otherwise the element name (`branch`, `hub`, `choice`, `return`, `match`, `when`, `otherwise`, `timeline`, `quest`, `objective`, `on`, `reward`, `entry`, `beat`). |
| `tag` | The directive tag or element name; `""` for a line. |
| `speaker` | A line's speaker; `""` otherwise. |
| `attrs` | A string map of every attribute **as written**: a key is present only when the author wrote it, and its value is the written text without quotes (escapes are not decoded). Attributes the parser lifts into fields (`<branch id>`, `<reward kind target amount>`, `::jump{to}`, `when=`) and plugin stamps are included. A bare flag is `"true"`, and an `@ref` value is its source text (`@name`). `<reward kind="COINS" amount="+005"/>` has `amount` `"+005"`, and a reward without `amount` has no `amount` key even though it is compiled with a default. `::set`, `::assert` and `::retract` have `{}`. |
| `index`, `first`, `last` | 0-based position in its body; whether it is the body's first or last node. |
| `depth` | `0` for a section's (or the document's) top-level node, plus one per enclosing element. |
| `prev`, `next` | `{ kind, tag, attrs }` of the sibling before or after it in the same body; every field is empty at a body edge. |
| `parent` | `{ kind, tag, attrs }` of the enclosing element. At a section's top level `kind` is `"section"`; at a quest or lore document's top level it is `"document"`. |
| `preceding` | A map from tag to `{ attrs }`: for each tag, the nearest node with that tag among the earlier siblings of this node and of each of its ancestors, and the ancestors themselves. It does not look inside an earlier sibling's body, so a marker in another branch arm is not seen. It starts empty at each section and each top-level declaration. Lines are not keyed. |
| `section` | `{ index, title, id }` of the enclosing section; `{ 0, "", "" }` outside sections. |

A finding is anchored at the node: the line, the directive, or the whole
element. Messages interpolate row paths such as `{node.attrs.value}`.

Reading `node.preceding["t"]`, testing `"t" in node.preceding` or taking
`size(node.preceding)` costs a map lookup; the scope is never copied per row.
Reading the whole map otherwise (interpolating or comparing it) copies it for
each row that reads it, so prefer keyed lookups.

### Node rule recipes

Every index a rule takes must be guarded, because a missing key is
`E-LINT-EXPR`. A rule whose legal values depend on another attribute (a
discriminator) excludes a discriminator it has no case for, and a separate
`*-known` rule reports the unknown discriminator itself — so each mistake is
reported once, by the rule that names it.

```yaml
lints:
  - id: signal-needs-menu
    target: node
    when: 'node.tag == "signal" && "value" in node.attrs && node.attrs["value"] == "puzzle" && node.next.kind != "branch"'
    level: warn
    message: "a puzzle ::signal must be followed by a <branch>"
  - id: section-opens-with-bg
    target: node
    when: 'node.first && node.parent.kind == "section" && node.tag != "bg"'
    level: warn
    message: "section {node.section.title} opens with {node.tag}"
  - id: beat-label-prefix
    target: node
    when: 'node.tag == "label" && scene.kind == "beat" && "name" in node.attrs && !node.attrs["name"].startsWith("b_")'
    level: error
    message: "a beat's labels start with b_"
  # Same element: `mode` selects the legal `speed`s.
  - id: move-mode-known
    target: node
    when: 'node.tag == "move" && "mode" in node.attrs && !(node.attrs["mode"] in options.speeds)'
    level: error
    message: "unknown move mode {node.attrs.mode}"
    options: { speeds: { walk: [slow, normal], run: [normal, fast] } }
  - id: speed-fits-mode
    target: node
    when: >-
      node.tag == "move" && "speed" in node.attrs && "mode" in node.attrs &&
      node.attrs["mode"] in options.speeds &&
      !(node.attrs["speed"] in options.speeds[node.attrs["mode"]])
    level: error
    message: "speed {node.attrs.speed} is not legal in mode {node.attrs.mode}"
    options: { speeds: { walk: [slow, normal], run: [normal, fast] } }
  # A marker directive before the branch decides the legal option values.
  - id: interaction-kind-known
    target: node
    when: 'node.tag == "interaction" && (!("kind" in node.attrs) || !(node.attrs["kind"] in options.cases))'
    level: error
    message: "::interaction needs a known kind"
    options: { cases: { greet: [wave, bow], gift: [accept, decline] } }
  - id: option-fits-interaction
    target: node
    when: >-
      node.tag == "option" && "value" in node.attrs &&
      "interaction" in node.preceding && "kind" in node.preceding["interaction"].attrs &&
      node.preceding["interaction"].attrs["kind"] in options.cases &&
      !(node.attrs["value"] in options.cases[node.preceding["interaction"].attrs["kind"]])
    level: error
    message: "option {node.attrs.value} does not belong to this interaction"
    options: { cases: { greet: [wave, bow], gift: [accept, decline] } }
  # A plugin stamp on the parent <branch> carries the discriminator.
  - id: menu-type-known
    target: node
    when: 'node.kind == "branch" && "menuType" in node.attrs && !(node.attrs["menuType"] in options.keys)'
    level: error
    message: "unknown menu type {node.attrs.menuType}"
    options: { keys: { plain: [L1, R1], timed: [T1] } }
  - id: choice-fits-menu
    target: node
    when: >-
      node.kind == "choice" && "optionKey" in node.attrs &&
      "menuType" in node.parent.attrs && node.parent.attrs["menuType"] in options.keys &&
      !(node.attrs["optionKey"] in options.keys[node.parent.attrs["menuType"]])
    level: error
    message: "option key {node.attrs.optionKey} is not a {node.parent.attrs.menuType} key"
    options: { keys: { plain: [L1, R1], timed: [T1] } }
```

These rules are shown as a plugin's `lints/*.yaml`; the same entries work under
`custom:` in `lute.lint.yaml`.

## Core rules

All seven core rules are enabled by default. `asset-exists` is inert without a
provider mapping, and `variant-composition` is inert until a line carries its
configured attribute; a bare project therefore has no false noise from either.
Set any rule to `level: error` to make it release-blocking for that project.

| rule | target | default | trigger and defaults |
| --- | --- | --- | --- |
| `dialogue-length` | line | warn | `line.words > maxWords`; `maxWords: 40`. Keeps individual lines readable and performable. |
| `dialogue-ratio` | scene | warn | `scene.kind == "scene"`, `bodyNodes >= minNodes`, and `dialogueRatio < min`; `minNodes: 10`, `min: 0.35`. Flags linear scenes with too little dialogue relative to their authored body. |
| `scene-length-spread` | project | warn | `scenes >= 2` and `spreadRatio > maxRatio`, over linear scenes only; `maxRatio: 3.0`. Finds an unusually uneven scene-length mix. |
| `section-starts-with-background` | section | warn | `section.kind == "scene"` and `firstStagingTag != "bg"`, including a section with no staging directive. Encourages each section of a linear scene to establish its background first; a beat, component, quest, or lore entry is presented into staging someone else set. |
| `emotion-distribution` | speaker | warn | Checks the selected axis once a speaker has at least `minLines: 10`: `domain: emotion`, optional `pairWith`, `runMax: 3`, `streakAvgMin: 1.5`, `maxShare: 0.4`. It applies the upstream lineage's hard cap of three identical emotion streaks, thrash floor of 1.5 average streak length, and 40% dominance cap; when `pairWith` is set it also checks the paired axis. One finding per failing speaker joins all reasons. |
| `variant-composition` | speaker and group | warn | `attr: variant`, optional `groupBy`, `minPerGroup: 2`, `minShare: 0.0`, `minLines: 10`. With `groupBy`, groups below `minPerGroup` fire. With `minShare > 0`, speakers meeting its own `minLines` whose `attrShare[attr]` is below the threshold fire. |
| `asset-exists` | line/directive | error | `providers: {}` (inert), `sentinels: [clear, empty, false, none, null, stop]`. For each mapped directive tag, checks `assetId` against the pinned provider snapshot. Absent assets fire; stale catalog data downgrades the finding to warn. Sentinel values are case-insensitively exempt. |

## Plugin rule authoring

A plugin lists `lints: lints/` under `exports:` in its `plugin.yaml` and ships
`lints/*.yaml` files, each a top-level `lints:` list of the same rule shape used by
`custom`:

```yaml
lints:
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

A plugin rule is data only: it asserts over the core-computed rows above.
Structural predicates — what follows, precedes or encloses a node, and which
attribute values another attribute allows — are `node` rules. A rule that needs
another metric or evaluation primitive is a core change, not a YAML extension;
balanced pairs (an opener that needs a matching closer) are planned as a `pair`
target in 0.51. Plugin lint declarations are advisory and are excluded from both
the capability snapshot and `capabilitySnapshot`.

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
| `section-starts-with-background` | `L-SECTION-STARTS-WITH-BACKGROUND` |
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
