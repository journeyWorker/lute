# Narrative

**Semantic id:** `lute.core/1` (baseline document, line, choice, match, hub, and control-flow records). Components/templates expand at compile time and have no semantic id.

## Constructs and rules

- **Scene/section:** `kind: scene` frontmatter (with `title:` as the only document title) and `## Title {#id}` sections define layout. The optional `{#id}` suffix is a unique per-document token (`E-SECTION-DUP` on duplicates) used as identity metadata only, never as a jump target. The IR `sections[]` table lists `{section, heading, id?}` with one-based section numbers; each command `position` starts with its section number.
- **Line/speaker/code:** `@speaker{code,...}: text` emits a line with plain `text`, a stable `lineId` and a `voiceKey`; `code` is per-speaker and may be backfilled by `lute tag`. Every line, narration and mono included, carries `voiceKey` as a join key; differing stripped text under one key is `E-DUP-VOICEKEY`.
- **Roles and POV:** the role is `dialogue`, or `narration` for the narrator; the mutually exclusive bare flags `mono`, `os` and `vo` select roles `mono`, `os` and `vo`. A `mono` line is valid only when its speaker is the effective POV (frontmatter `pov`, else project `defaults.pov`) or is listed in the effective `monoSpeakers`; otherwise `E-MONO-POV`, or `E-MONO-NO-POV` when no POV resolves. Component lines are checked at every `::use` site with the caller's POV and `monoSpeakers`.
- **Inline modifiers:** inside line text, `:name[span]`, `:name{attrs}` and `:name[span]{attrs}` annotate presentation. Core modifiers are `:pause{s=seconds}` and `:speed[text]{rate=rate}`; any other name must be a `textStyle` domain member and takes no attrs. `\:` is a literal colon. A modified line keeps plain `text` and adds `segments` (`{text, styles?, rate?}` runs and `{pause}` leaves); malformed markup is `E-TEXT-MODIFIER`/`E-TEXT-ESCAPE`.
- **Branch/choice:** `<branch>` and `<choice id text>` show eligible options, record the selected id under the choice's `selectionKey`, jump to its target and converge. Guards are evaluated at the choice record.
- **Hub:** `<hub>` may be presented repeatedly; `once`, `exit`, visited state and `<return>` control revisits and convergence.
- **Jump/label:** `::jump{to="label"}` is a forward-only jump to a `::label{name="label"}` in the document-wide label namespace; a label emits no command.
- **Match:** `<match subject>` evaluates `<when>` arms top-to-bottom; first match wins. `is` shorthand and `test` CEL lower to arms; an unset subject matches only `is="unset"`.
- **Components/templates:** typed `component:` files and `::use` expand before lowering; parameter and effect checks apply at every use. Engines never see `::use`.
- **Interpolation/localization:** `{{path}}`, `{{@def(args)}}` and reserved/occasion tokens resolve at presentation and stay verbatim in `text` and `placeholders`. Locale maps join after lowering by stable line/option ids: every translation gets plain `texts[locale]`, and a modified line also gets `localeSegments[locale]`. A translation must keep the source modifier multiset (`E-L10N-MODIFIERS`).

## Evaluation and lowering

Normalize/expand → lower and position → walk command array; control-flow targets and convergence alter the PC. Lines and guards read live state at their position. See [module ordering](../design/modules.md#narrative) and [execution model](../runtime/execution-model.md).

## Diagnostics

Checker validates speaker/cast, ids, roles and POV, option totality, target/converge references, CEL types, match exhaustiveness/dead arms, component cycles/arity/types, inline modifiers, interpolation paths and locale key collisions.

## Example

```lute check
---
kind: scene
id: greeting
title: Greeting
pov: guide
enums:
  textStyle: [emphasis]
state:
  scene.mood: {type: string, default: "calm"}
---
## Opening {#opening}
@guide{code="hello"}: :emphasis[Hello]:pause{s=0.5} {{scene.mood}}.
@guide{code="think", mono}: Quiet today.
<branch id="next">
  <choice id="go" text="Go">
    ::jump{to="done"}
  </choice>
</branch>
<match subject="scene.mood">
  <when is="calm">
    @narrator: All is calm.
  </when>
  <otherwise>
    @narrator: Something stirs.
  </otherwise>
</match>
## Closing {#closing}
::label{name="done"}
@narrator: A destination.
```

The first two lines and the choice lower to:

```json
[
  {"kind": "line", "family": "content", "position": "001-0100", "role": "dialogue", "speaker": "guide", "text": "Hello {{scene.mood}}.", "lineId": "greeting.guide_hello", "voiceKey": "greeting.guide-hello", "placeholders": [{"kind": "path", "path": "scene.mood"}], "segments": [{"text": "Hello", "styles": ["emphasis"]}, {"pause": 0.5}, {"text": " {{scene.mood}}."}]},
  {"kind": "line", "family": "content", "position": "001-0200", "role": "mono", "speaker": "guide", "text": "Quiet today.", "lineId": "greeting.guide_think", "voiceKey": "greeting.guide-think"},
  {"kind": "choice", "family": "control", "position": "001-0300", "branchId": "next", "selectionKey": "scene.choices.next", "options": [{"id": "go", "text": "Go", "lineId": "greeting.next.go", "target": "001-0400"}], "converge": "001-0600"}
]
```

## History

[DSL 0.37 §3.1](../proposals/scenario-dsl/0.37.0.md#31-sections-and-identity), [§3.4–3.6](../proposals/scenario-dsl/0.37.0.md#34-lines-roles-pov-and-voice-joins), [§6](../proposals/scenario-dsl/0.37.0.md#6-localization-contract); [DSL 0.33 §1](../proposals/scenario-dsl/0.33.0.md#1-domain-modules-and-evaluation-order), [execution model](../runtime/execution-model.md).
