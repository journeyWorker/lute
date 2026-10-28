---
title: Linting
description: "lute lint — advisory, per-project editorial lints (L-* codes) configured in lute.lint.yaml: the seven core rules, project-local custom rules, plugin lint exports, the metric tables rules read, the display-name advisory W-DISPLAY-NAME-DUP (dsl 0.26.0), and the exit-code contract."
---
## Machine-readable output (0.43)

`lute lint --json` uses the CLI envelope: `command`, `schemaVersion`, `ok`, and the diagnostic payload. Errors have `ok: false` and an `error` object; findings remain in the successful payload. The exit status is authoritative, including when warnings are promoted with `--deny` or `--deny-warnings`.

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
| `lsp` | boolean | `false` | Publishes lint findings for open documents in the LSP only when this config file exists and sets it to `true`. Linting is otherwise absent from the LSP, even if rules are configured. LSP evaluates `line`, `section`, `scene`, `speaker`, `group`, and (since 0.50) `node` targets; `project` rules are CLI-only in v1. |
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
keys, wrong option types, and colliding custom ids are `E-LINT-CONFIG` findings.
Naming a **checker** code in `rules:` is `E-LINT-CONFIG` too, with a hint to the
right file (since 0.50): `` `W-STAGE-ABSENT` is a checker code, not a lint rule — set its level in `lute.project.yaml` `diagnostics:` ``; for an `E-*` id the hint ends `an error cannot be re-levelled`. Lint rule levels (`L-*`) are set here, and only here.
The bad entry is skipped, remaining rules run, and the command exits 1.

### Custom rule shape

Every custom entry has the same shape as a plugin rule:

| key | required | meaning |
| --- | --- | --- |
| `id` | yes | Project-local bare rule id. |
| `target` | yes | `line`, `section`, `scene`, `speaker`, `group`, `project`, `node` (since 0.50), or `pair` (since 0.51). |
| `when` | yes, except for `pair` | A CEL assertion over the target row and `options`; it fires when `true`. A `pair` rule takes `open` and `close` instead and refuses `when` (see [`pair` lints](#pair-lints-since-051)). |
| `level` | no | `off`, `hint`, `info`, `warn`, or `error`. |
| `message` | yes | Finding text. `{path.to.field}` interpolates a metric or option path; a number renders trimmed (no trailing `.0`) and rounded to at most two decimals, so a ratio of `3.3333…` reads `3.33`. `{expr:%}` renders shares as rounded percentages. |
| `options` | no | Defaults exposed to `when` and the message. |

The lint CEL fragment is ground-only: no `@ref` or `$` DSL tokens, state paths,
or macros (`filter`, `map`, `exists`). It supports the shared ground-operation
semantics, metric-path selection and map indexing, and `size()` for strings,
lists, and maps. An unresolved field, type mismatch, or non-boolean `when` result
is `E-LINT-EXPR`, reported once at the rule declaration, and skips that rule—it is
never a silent pass.

Since 0.50 the fragment also has the standard CEL string members
`s.startsWith(t)`, `s.endsWith(t)` and `s.contains(t)`, in member-call form only,
with both operands strings; a non-string operand is an evaluation error
(`E-LINT-EXPR`). There are no regexes and no macros.

## `node` lints (since 0.50)

`target: node` (valid in plugin `lints:` exports and `lute.lint.yaml` `custom:` rules) evaluates a rule once per **node** of a document. Rows come in document order over every body the checker walks:

- section bodies;
- `<quest>` declarations and their bodies (objectives, `<on>` handlers);
- `<reward>` children of a quest or objective, at their source position among the owner's children;
- top-level `<entry>` and `<beat>` declarations and their bodies;
- `<branch>` choices and choice bodies;
- `<hub>` choices, choice bodies and its `<return>` body;
- `<match>` arms (`<when>`, `<otherwise>`) and their bodies;
- `<timeline>` (one row; track clips are not rows).

| `node.kind` | for |
| --- | --- |
| `line` | a content line |
| `directive` | every `::tag{…}`: plugin and core directives, `::set`, `::assert`, `::retract`, `::accept`, `::use`, `::label`, `::jump`, `::clear`, `::end` |
| the element name | `branch`, `hub`, `choice`, `return`, `match`, `when`, `otherwise`, `timeline`, `quest`, `objective`, `on`, `reward`, `entry`, `beat` |

A `node` rule reads three bindings: `node`, `scene` (the existing document row) and `options`.

| field | meaning |
| --- | --- |
| `node.kind` | as in the table above |
| `node.tag` | the directive tag or element name; `""` for a line |
| `node.speaker` | a line's speaker; `""` otherwise |
| `node.attrs` | map string → string of every attribute **as authored** (see below) |
| `node.index`, `node.first`, `node.last` | 0-based position in its body; first/last in that body |
| `node.depth` | 0 for a section's (or document's) top-level node, +1 per enclosing element |
| `node.prev`, `node.next` | `{ kind, tag, attrs }` of the sibling in the same body; all fields empty at a body edge |
| `node.parent` | `{ kind, tag, attrs }` of the enclosing element. At a section's top level `kind` is `"section"`; at a quest or lore document's top level `kind` is `"document"`. |
| `node.preceding` | map tag → `{ attrs }`: for each tag, the nearest node with that tag among the earlier siblings of this node and of each of its ancestors, together with the ancestors themselves. It does **not** enter the bodies of earlier siblings (another arm is not on this path). It resets at each section and declaration. Lines are not keyed. |
| `node.section` | `{ index, title, id }` of the enclosing section; `{ 0, "", "" }` outside sections |

**Anchoring and messages** work as for `line` rows: the diagnostic spans the node (the line, directive or element), and `{node.attrs.value}` interpolates.

**Authored attributes.** `node.attrs` has a key iff the author wrote it, and its value is the authored value text, unquoted. This includes attributes the parser lifts into fields (`<branch id>`, `<reward kind target amount>`, `::jump{to}`) and plugin stamps. A bare flag is `"true"`, and an `@ref` value is its source text (`@name`). An unauthored attribute is absent even when the IR carries a default, so `<reward>` without `amount` has no `amount` key. `::set`, `::assert` and `::retract` have `{}`. The text is re-read from the source by the parser's own attribute scanner, not rebuilt from parsed fields: `<reward kind="COINS" amount="+005"/>` gives `node.attrs["amount"] == "+005"`, not `"5"`. The same values result for CRLF line ends and multi-byte text.

**Guard every index.** A missing key keeps CEL semantics: `node.attrs["x"]` on an absent key is an evaluation error (`E-LINT-EXPR`). `&&` short-circuits, so a rule guards **every** index it takes: `"x" in node.attrs && node.attrs["x"] …`, `"t" in node.preceding && "k" in node.preceding["t"].attrs && …`, and `key in options.cases && …` before indexing an options map.

`node.preceding` is read through a borrowed view of one running map, not copied per row, so a `node` rule that reads it stays near-linear in document size. The LSP evaluates `node` rules like the other in-document targets. `node` rules change no IR, event, snapshot or `capabilitySnapshot`, and carry no semantic id.

### Recipes

Every index is guarded. **Policy for an unknown discriminator:** a dependent-value rule excludes a discriminator it has no case for, and a separate `*-known` rule reports the discriminator itself, so each violation is reported once, by the rule that names it.

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
  # #177, same element: `mode` selects the legal `speed`s.
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
  # #177, a marker directive before the branch decides the legal option values.
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
  # #177 with #164 stamps: the parent <branch> carries the discriminator.
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

For a rule whose dependent attribute, discriminator or marker is missing (`preceding` without `interaction`, a marker without `kind`, a parent without `menuType`) there is no finding and no `E-LINT-EXPR`. An unguarded variant (`node.attrs["mode"]` without the membership test) on a node without `mode` reports `E-LINT-EXPR`, which is why the guard is there.

## `pair` lints (since 0.51)

`target: pair` (valid in plugin `lints:` exports and `lute.lint.yaml` `custom:` rules) checks that two kinds of node come in balanced pairs: a `begin` with its `end`, an open with its close. It is for markers a plugin writes as two directives, which neither `node` rules nor the metric tables can pair up.

```yaml
lints:
  - id: group-balanced
    target: pair
    open:   'node.tag == "group" && "mode" in node.attrs && node.attrs["mode"] == "begin"'
    close:  'node.tag == "group" && "mode" in node.attrs && node.attrs["mode"] == "end"'
    inside: 'node.kind == "line" || node.tag in ["actor", "sfx"]'   # optional
    scope: body                                                    # body (default) | section | document
    level: error
    message: "::group begin/end"
```

| key | required | meaning |
| --- | --- | --- |
| `open` | yes | A lint-CEL predicate over the [`node` row](#node-lints-since-050): holds for a node that opens a pair. |
| `close` | yes | The same, for a node that closes it. |
| `inside` | no | A predicate every node strictly between an open and its close must satisfy. |
| `scope` | no | `body` (default), `section` or `document` (since 0.53); see below. |

`open`, `close` and `inside` read the same `node` row as a `node` rule (`kind`, `tag`, `speaker`, `attrs` as authored, `index`, `first`, `last`, `depth`, `prev`, `next`, `parent`, `preceding`, `section`), plus the `scene` and `options` bindings and the string members. A missing key keeps CEL semantics (`E-LINT-EXPR`), so guard every index as in the [recipes](#recipes); the error names the rule and the predicate (`` rule `g2`: `open`: missing key `kind` ``). The loader enforces the shape: on `pair`, `open` and `close` are required and `when` is refused (`` rule `g3`: `when` is refused on `target: pair`, which states `open` and `close` instead ``); on every other target `open`, `close`, `inside` and `scope` are refused; a key written as `null` is refused rather than read as absent (`` `open` is null; give it a value or leave it out ``). A fault is `E-PLUGIN-PARSE` in a plugin's `lints:` and `E-LINT-CONFIG` in `lute.lint.yaml` `custom:`.

### Evaluation

A rule keeps **one open slot** per scope and makes one pass over the scope's nodes in document order:

- **`scope: body`** (the default): each body's direct children, in order — siblings only. Every body (a section, a `<choice>`, a `<match>` arm) is its own scope.
- **`scope: section`**: every node of one section or top-level declaration in document order, at every depth — the unit at which `node.preceding` resets. It is document order, not paths: an `open` in one `<choice>` pairs with a `close` in its sibling.
- **`scope: document`** (since 0.53): one slot for the whole document — every row of every section and top-level declaration, at every depth, in document order. It is the `section` walk without the reset at each `##` heading or declaration, so a pair may open in one section and close in a later one, or in a later top-level declaration.

For each node:

- a node for which `close` holds while a pair is open **closes** it. A node that satisfies both `open` and `close` is a close while a pair is open, and an open otherwise;
- a node for which `open` holds while a pair is open is a **nested open**, and the outer pair stays open;
- a node for which `close` holds with no pair open is an **unopened close**;
- with `inside`, every node strictly between an open and its close for which `inside` is false is an **inside fault** (in `body` scope only the siblings count, not their descendants);
- at the end of the scope, a pair still open is an **unclosed open**.

A finding carries the rule's code and level, anchors at the offending node, and appends one fixed phrase to the rendered `message`:

| fault | appended phrase |
| --- | --- |
| unclosed open | `` — opened here, with no close in this body `` (`in this section` for `scope: section`, `in this document` for `scope: document`) |
| unopened close | `` — a close with no open pair in this body `` (`in this section`, `in this document`) |
| nested open | `` — opened again while the pair from line N is still open `` |
| inside fault | `` — not allowed inside the pair opened at line N `` |

A nested-open and an inside finding also name the open node as `related`.

Take the rule above with this body, where the `::group` is never closed:

```lute
::group{mode="begin"}
@narrator: The crowd gathers.
@narrator: Nobody speaks.
```

`group-balanced` reports `::group begin/end — opened here, with no close in this body` at the `begin` (its code is `L-ACME-FLOW-GROUP-BALANCED` when the `acme.flow` plugin exports it, `L-GROUP-BALANCED` as a `lute.lint.yaml` `custom:` rule). A stray `::group{mode="end"}` with no begin reads `::group begin/end — a close with no open pair in this body`, and a second `begin` before the `end` reads `::group begin/end — opened again while the pair from line N is still open`.

### An `inside` rule

`inside` limits what may sit between the markers. With the `inside` predicate above (a line, `::actor` or `::sfx`), any other directive between the markers is a fault:

```lute
::group{mode="begin"}
@narrator: The crowd gathers.
::bg{assetId="street.night"}
::group{mode="end"}
```

The `::bg` is reported as `::group begin/end — not allowed inside the pair opened at line 1`, with the `begin` as `related`.

### A `section`-scope pair

Under `scope: body`, a `begin` before a `<branch>` and its `end` inside a choice sit in different bodies: the `begin` is an unclosed open and the `end` an unopened close. `scope: section` pairs them across the branch, in document order:

```yaml
lints:
  - id: fx-balanced
    target: pair
    open:   'node.tag == "fx" && "mode" in node.attrs && node.attrs["mode"] == "begin"'
    close:  'node.tag == "fx" && "mode" in node.attrs && node.attrs["mode"] == "end"'
    scope: section
    level: warn
    message: "::fx begin/end"
```

```lute
::fx{mode="begin"}
<branch id="ask">
  <choice id="stay" text="Stay">
    @narrator: You stay.
    ::fx{mode="end"}
  </choice>
  <choice id="go" text="Go">
    @narrator: You go.
  </choice>
</branch>
```

Here the pair is balanced in document order, so there is no finding. A section's slot resets at the next `##` heading or declaration, so a `begin` left open there is an unclosed open (`in this section`).

### A `document`-scope pair

A port that keeps one section per source shot often opens an effect in one section and closes it in a later one. Under `scope: section` the `begin` of the first section is an unclosed open and the `end` of the later one an unopened close; `scope: document` pairs them. The rule, in the project's `lute.lint.yaml`:

```yaml
custom:
  - id: music-balanced
    target: pair
    open:   'node.tag == "music" && node.attrs["playback"] == "start"'
    close:  'node.tag == "music" && node.attrs["playback"] == "stop"'
    scope: document
    level: error
    message: "::music start/stop"
```

and the body of a scene whose two sections carry the pair:

```lute
## Arrival

::bg{assetId="BG.road"}
::music{assetId="BGM.walk" playback="start"}
@narrator: The walk takes a while.

## Gate

::bg{assetId="BG.gate"}
@narrator: Here we are.
::music{assetId="BGM.walk" playback="stop"}
```

```console
$ lute lint scenes/walk.lute
ok: scenes/walk.lute (0 error(s), 0 warning(s))
```

With `scope: section` the same scene is two findings. With `scope: document` and the `stop` line removed, the open is reported at the end of the document:

<!-- lute-diagnostics unverified="verbatim lute lint output; L-MUSIC-BALANCED is the example's own custom rule message from lute.lint.yaml" -->
```console
$ lute lint scenes/walk.lute        # scope: section
scenes/walk.lute:12:1: error [L-MUSIC-BALANCED] ::music start/stop — opened here, with no close in this section
scenes/walk.lute:19:1: error [L-MUSIC-BALANCED] ::music start/stop — a close with no open pair in this section
failed: scenes/walk.lute (2 error(s), 0 warning(s))
$ lute lint scenes/walk.lute        # scope: document, no `stop`
scenes/walk.lute:12:1: error [L-MUSIC-BALANCED] ::music start/stop — opened here, with no close in this document
failed: scenes/walk.lute (1 error(s), 0 warning(s))
```

(Line numbers are those of the full file with its frontmatter.) `node` rules get no scope; `scope` stays refused on every target but `pair`.

### Where it runs, and cost

`pair` rules run in the same node walk as `node` rules, in `lute lint` and in the language server, with each rule evaluated at its node and nothing stored per node; the walk runs when a `node` or a `pair` rule is active. The first `E-LINT-EXPR` in a rule's predicates stops that rule for the document, as for a `node` rule. The state machine's own overhead is linear: at most three predicate calls per `pair` rule per node, and constant slot work. What a predicate does is not claimed linear: `size()` of a long string, for example, is counted by character, so a predicate that reads N characters at each of N nodes costs N². The work counters report which is which: `pair_evals` (predicate invocations) and `pair_slots` (slot opens, closes and frame pushes), and `eval_chars` for the characters a string operation scans. A `pair` rule changes no IR, event, snapshot or `capabilitySnapshot`, and carries no semantic id.

**Lint cost (0.53).** `lute lint` builds the project's shared data once per run and reads only its operands, the manifest, the plugins, the providers and the imports they reach — never the other documents of the project. A rule that is off costs nothing per document; with none active the engine returns before any per-document work. `LUTE_STATS=input-cache` shows the build counts (see [Cost counters](/tooling/cli/#cost-counters-lute_stats)). An unrelated unreadable sibling no longer fails a lint run, and an unreadable operand is reported as its own I/O error (see [`lute lint`](/tooling/cli/#lint)).

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

`scene.kind` classifies the document (dsl 0.22.0), and `section.kind` repeats its
enclosing document's value so a section rule can scope itself without another
binding: `scene` for a linear scene, `beat` for a scene with `on:` (a
[beat](/tooling/play/) answering an occasion), `component` for a document
declaring `component:`, and otherwise its authored `kind:` — `quest`, `lore`, or
any other value. A rule meant for linear episodes guards on
`scene.kind == "scene"`; the three core rules below that encode linear-VN norms
already do. A scene that the manifest's `chapters:` lists is a `beat` too, by the
`on:` its chain gives it — including one that continues the scene before it in
the same raise of a `select: sequence` occasion, on the stage that scene set.

`scene.directives` and `section.firstStagingTag` count **staging** directives only:
`::accept`, `::use`, `::end`, `::label`, and `::jump` present nothing, so they are
skipped — a section that opens with `::accept{quest="…"}` followed by `::bg` has
`firstStagingTag == "bg"`.

`scene.dialogueRatio` is `dialogueLines / bodyNodes`, or `0.0` when there are no
body nodes. `project.scenes` counts linear scenes only (`scene.kind == "scene"`),
and `project.sceneWords` is `{ min, max, mean, stddev }` over their word counts;
`spreadRatio` is `max / min`, or `0.0` when `min == 0` or fewer than two scenes
exist. A beat, component, quest, or lore document never enters the spread.

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
| `dialogue-ratio` | scene | warn | `scene.kind == "scene"`, `bodyNodes >= minNodes`, and `dialogueRatio < min`; `minNodes: 10`, `min: 0.35`. Flags linear scenes with too little dialogue relative to their authored body. |
| `scene-length-spread` | project | warn | `scenes >= 2` and `spreadRatio > maxRatio`, over linear scenes only; `maxRatio: 3.0`. Finds an unusually uneven scene-length mix. |
| `section-starts-with-background` | section | warn | `section.kind == "scene"` and `firstStagingTag != "bg"`, including a section with no staging directive. Encourages each section of a linear scene to establish its background first. |
| `emotion-distribution` | speaker | warn | Checks the selected axis once a speaker has at least `minLines: 10`: `domain: emotion`, optional `pairWith`, `runMax: 3`, `streakAvgMin: 1.5`, `maxShare: 0.4`. It applies the upstream lineage's hard cap of three identical emotion streaks, thrash floor of 1.5 average streak length, and 40% dominance cap; when `pairWith` is set it also checks the paired axis. One finding per failing speaker joins all reasons. Streaks are counted within what a player hears in one sitting: a document's sections and quest bodies together, and each lore bundle `<beat>` on its own; lore `<entry>` lines are not measured, since entries are never heard back to back. |
| `variant-composition` | speaker and group | warn | `attr: variant`, optional `groupBy`, `minPerGroup: 2`, `minShare: 0.0`, `minLines: 10`. With `groupBy`, groups below `minPerGroup` fire. With `minShare > 0`, speakers meeting its own `minLines` whose `attrShare[attr]` is below the threshold fire. |
| `asset-exists` | line/directive | error | `providers: {}` (inert), `sentinels: [clear, empty, false, none, null, stop]`. For each mapped directive tag, checks `assetId` against the pinned provider snapshot. Absent assets fire; stale catalog data downgrades the finding to warn. Sentinel values are case-insensitively exempt. |

`asset-exists` `options.providers` keys are selectors (0.49.0):

| key | checks |
| --- | --- |
| `<tag>` | `::<tag>`'s `assetId` (the pre-0.49 meaning) |
| `<tag>.<attr>` | that attribute of that directive |
| `@<attr>` | that attribute on every content line |

```yaml
asset-exists:
  level: error
  options:
    providers: { bg: bg-catalog, sfx.voiceAssetId: voice-catalog, "@assetId": portrait-catalog }
```

Any other key shape is `E-LINT-CONFIG`. Only literal strings are checked, and `sentinels` apply. A `<tag>` finding reads as before; the others name their source, `` (`@mira` attribute `assetId`) `` for a line attribute and `` (`::sfx` attribute `voiceAssetId`) `` for a directive attribute.

Since 0.22.0 the three linear-VN rules — `dialogue-ratio`,
`scene-length-spread`, and `section-starts-with-background` — judge only linear
scenes. A beat is presented into a moment the engine has already staged, and a
component, quest, or lore entry is spliced into someone else's staging, so a
project built on [beats and occasions](/tooling/play/) no longer drowns in
`L-SECTION-STARTS-WITH-BACKGROUND` for every bark. To hold beats to a norm of their
own, write a `custom:` rule that guards on `scene.kind == "beat"` (or
`section.kind == "beat"`).

## Display names

Since dsl 0.26.0 `lute lint` also reports `W-DISPLAY-NAME-DUP` (dsl
0.26.0 §2.8): two different speakers the dialogue box would show under the same
name. Ids are unique across a project, but a player reads names, and two authors
who each cast a "Hiker Gus" have written one person twice. It fires for two cast
entries with exactly the same `name:`, a cast name equal to a component's
`::use{… name="…"}` display string, and two such strings for different speakers
(`who=`). It is the advisory whole-project `check` reports too, and it keeps its `W-`
code rather than an `L-*` one. The comparison covers the whole project the path
belongs to, so linting one file still compares every cast entry; the finding is
anchored at the first line that speaks one of the names, or at the document's
first line when neither is spoken:

<!-- lute-diagnostics -->
```console
$ lute lint .
lore/trainers.lute:12:3: warning [W-DISPLAY-NAME-DUP] display name `Hiker Gus` is shown for 2 different speakers: `gus` (lore/trainers.lute:12), `r16Gus` (the cast, never spoken) — the dialogue box cannot tell them apart; rename one
ok: . (0 error(s), 1 warning(s))
```

A role name several speakers share on purpose — "Eclipse Grunt", "Gym Guide" —
is not a collision. Mark each such cast entry `sharedName: true` and it is not
counted:

```yaml
cast:
  grunt1: { name: Eclipse Grunt, sharedName: true }
  grunt2: { name: Eclipse Grunt, sharedName: true }
```

The code is not a `lute.lint.yaml` rule, so it has no level to set there. A
`rules:` entry naming it is `E-LINT-CONFIG` with a hint that points to the right
place: `` `W-DISPLAY-NAME-DUP` is a checker code, not a lint rule — set its level in `lute.project.yaml` `diagnostics:` `` (since 0.50; for an `E-*` id the text ends `… an error cannot be re-levelled`).
The project-level `diagnostics:` key (see [the CLI page](/tooling/cli/)) sets a checker `W-*` code to `off`, `info`, `warn` or `error`. `--deny
W-DISPLAY-NAME-DUP`, or `--deny-warnings`, makes it an error and fails the run:

<!-- lute-diagnostics -->
```console
$ lute lint . --deny W-DISPLAY-NAME-DUP
lore/trainers.lute:12:3: error [W-DISPLAY-NAME-DUP] [denied] display name `Hiker Gus` is shown for 2 different speakers: `gus` (lore/trainers.lute:12), `r16Gus` (the cast, never spoken) — the dialogue box cannot tell them apart; rename one
failed: . (1 error(s), 0 warning(s))
```

A project that gates a merge on `check --project DIR --deny-warnings` marks its role
names `sharedName: true` first. See the [multi-author guide](/guides/multi-author/).

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

A plugin rule is data only when it asserts over these fixed, core-computed metric
tables. If the rule needs another metric, traversal, ordering rule, or evaluation
primitive, that is a core change—not a YAML extension. Structural predicates
(what a node is, what surrounds it, which attributes the author wrote) are the
[`node` target](#node-lints-since-050), and balance checks are the [`pair` target](#pair-lints-since-051) (0.51). Plugin lint declarations
are advisory and are excluded from both the capability snapshot and
`capabilitySnapshot`.

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
