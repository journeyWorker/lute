# Runtime execution model

This directory is the **runtime contract**: what an engine must implement to
*consume* a compiled Lute artifact. Lute itself is a total, side-effect-free
compiler — it checks a `.lute` document and lowers it to the execution IR
described by the current schema (`schemas/lute-ir-0.39.schema.json`). It runs
no CEL, no Datalog fixpoint, keeps no fact store, and fires no bridge at
compile time. Everything on the far side of the execution IR is the engine's
job.
These documents describe that job, grounded in `crates/lute-compile` and
`crates/lute-check`.

## Author-time project analysis (0.34.0)

`lute impact`, `lute constraints`, and the evidence fields on analysis
commands are author-time reports, not execution-IR commands. They do not add
runtime obligations: the host still executes the compiled artifact and owns
time advancement. A `bounded` or `heuristic` report must not be interpreted by
an engine as a universal runtime guarantee.

> **Permission boundary (0.17.0):** capability permissions reject forbidden
> authored effects before an artifact is emitted; they do not sandbox the
> engine or authorize its runtime resources. Hosts using permission ceilings
> must also follow
> [`capability-permissions.md`](capability-permissions.md).

## What Lute hands you

One execution IR is produced per `.lute` document (`lute compile <file>` →
`crates/lute-compile/src/lib.rs::compile`). Its shape is the `ExecutionIr`
struct (`ir.rs`):

- an **envelope** — `kind` (`"scene"` | `"quest"` | `"lore"`), `lute`
  (language version), `irVersion` (the version you gate on),
  `capabilitySnapshot` (a hash of the capability snapshot the artifact was
  compiled against), and `meta`. `meta.id` is the **canonical scene key**
  engines and tools join on — the string a `visited("…")` prereq resolves to,
  the prefix every `lineId` / `voiceKey` was derived from, and what
  `project.index.json` keys documents by. A quest keeps its authored quest id
  as identity in the same slot. The descriptive scene-meta fields
  (`character` / `season` / `episode` / `episodeId`, `title`) are emitted only
  when the source supplied them, never something a runtime rederives an id
  from. A scene whose `meta.beat` is present is a **beat**: besides explicit
  flow, the engine may select it when it raises the named occasion (see
  [beats-and-occasions.md](./beats-and-occasions.md));
- an envelope field **`requiredSemantics`** immediately after
  `capabilitySnapshot`: a compiler-derived, sorted, duplicate-free list of
  semantic ids used by this artifact. Every executable artifact includes
  `lute.core/1`; authors cannot provide, remove, or reorder the list. The
  registry and trigger mapping are normative in the
  [0.33.0 proposal §2–§3](../proposals/scenario-dsl/0.33.0.md#2-semantic-id-registry).
- a **folded state table** — `state: StateEntry[]` (see
  [state-lifecycle.md](./state-lifecycle.md));
- a **declared vocabulary** — `entities` / `enums` / `relations` / `seedFacts` /
  `rules`, emitted as data; each is omitted when empty. `entities` /
  `relations` / `seedFacts` / `rules` feed your Datalog evaluator (see
  [cel-and-facts.md](./cel-and-facts.md)). **`enums` carries both** the
  relational enum domains AND the document's declared **content and staging**
  vocabulary (`emotion`, `action`, `anchor`, `costume`, `mood`, `volume`,
  `musicPlayback`, `vfxType`, `framing`, `cameraMove`, `transition`,
  `cgLayout`, `sequence`, `textStyle`) when the project declares it in a
  schema the document imports — the compiler ships no members of its own, so
  the artifact is self-describing about the vocabulary it was compiled
  against. Each entry is `{ name, members }`; member-level semantics
  (`exits:`/`default:`) are **not** serialized, because the compiler has
  already resolved them into `actor.exit` and the emitted anchor. A vocabulary
  supplied by a plugin `enums` export does **not** appear here — it is part of
  `capabilitySnapshot` instead. An engine that ignores `enums` is unaffected;
- a flat, ordered **`commands: Command[]`** stream — the executable body;
- a descriptive **`sections`** table — one `{ section, heading, id? }` entry
  per `## ` section, in document order (see
  [Addressing](#addressing));
- an advisory **`prereqEdges`** graph (this document's raw `after` / quest `follows` formulas;
  an `after` row also carries its parsed `formula` — `{"visited":"id"}`,
  `{"completed":"q"}`, `{"active":"q"}`, `{"and":[l,r]}`, `{"or":[l,r]}` — so an
  engine never parses `after` text; connectivity T13 — see [quest-lifecycle.md](./quest-lifecycle.md) for how
  cross-document reachability is out of scope for a single artifact).

A project's engine **unions** the per-document `relations` / `rules` /
`seedFacts` / `entities` / `enums` / `prereqEdges` across every artifact it
loads, exactly as it concatenates the command streams.

## Version negotiation

An engine MUST negotiate in this order, before it opens a playback session or
executes any command:

1. **Exact-minor IR gate.** For pre-1.0 IR, an engine implementing `0.33.x`
   accepts only `0.33.*` artifacts and refuses every other major.minor line
   (`0.32.*`, `0.34.*`, and so on). Patch policy is optional within `0.33`.
   From 1.0 onward, the gate changes to the released major's policy.
2. **Semantic capability gate.** Load the engine's immutable
   `lute.engine.yaml` matrix. Its required fields are `engine`, `irVersion`,
   and unique `supportedIds`; `version` and `description` are descriptive.
   Compare every artifact `requiredSemantics` entry with `supportedIds`.
   Unknown or malformed matrix ids are errors, and an unknown artifact id is
   `E-SEMANTICS-UNKNOWN`.

`lute run --engine <file>` and `lute play --engine <file>` perform both gates.
If any required id is absent, playback MUST be refused **before playback** —
no command, condition, asset, or bridge may run — with exit code 2 and
`E-ENGINE-SEMANTICS`. A malformed matrix uses `E-ENGINE-MATRIX`; an
unsupported exact-minor line uses `E-ENGINE-IR-VERSION`. The built-in
`reference` matrix supports every current registry id.

`lute check --engine <file>` and `lute check-project --engine <file>` perform
the same negotiation at author time, without playback. They report each
lowered construct whose semantic id is absent, including its source span,
semantic id, and engine name, as `E-CHECK-ENGINE-SEMANTICS`. The project form
negotiates the sorted union in its project index and identifies each
contributing document/span.

The project index carries `requiredSemantics`, the sorted union of its
document artifacts, so an engine can negotiate before opening every document.
Plugin snapshot compatibility remains the exact `capabilitySnapshot` check; it
is not part of the semantic matrix. The complete registry, trigger table,
matrix format, and version rules live in the
[0.33.0 proposal](../proposals/scenario-dsl/0.33.0.md#2-semantic-id-registry),
not in this runtime guide.

`lute` (the language version) is informational for the runtime and does not
gate.

## CEL slots and execution IR expressions

Every CEL slot is `{ "cel": "<standard CEL>", "expr": <exprNode>,
"authored"?: "<source when expanded>" }`; `expr` is present on every slot.
The R1 runtime evaluates `expr`, including host-function calls; it never parses
`cel`. The CEL text remains available to other execution-IR consumers.
The IR also carries `celEnv`, declaring exactly the roots and host functions
used by its expressions.

`exprNode` is the typed full-profile tree:

| Shape | Meaning |
|---|---|
| `{"int":3}`, `{"double":3.5}`, `{"bool":true}`, `{"string":"x"}` | typed literal |
| `{"list":[<node>,…]}` | list |
| `{"path":"a.b.c"}` | canonical state read |
| `{"index":<node>,"key":<node>}` | computed index |
| `{"has":"a.b.c"}` | presence |
| `{"op":"!"\|"-","l":<node>}` | unary |
| `{"op":<sym>,"l":<node>,"r":<node>}` | binary, including `in` |
| `{"cond":<node>,"then":<node>,"else":<node>}` | ternary |
| `{"call":"<name>","args":[<node>,…]}` | conversion or host call |

## Activation and evaluation

An engine evaluates each CEL slot against nested maps built from live state:

- each declared root and every intermediate map on the path to a declared slot
  is present;
- a non-reserved slot is present iff it has an effective value (write, else
  seed, else default), so `has(run.tip)` tests whether the slot is present;
- values retain their declared CEL types: `int`, `double`, `bool`, `string`,
  enum members as `string`, and lists as lists;
- reserved engine paths are always present with their reserved defaults: for
  every quest the IR declares, `quest.<id>.state` and
  `quest.<id>.failedBy` are `"unset"`; each objective's
  `quest.<id>.objectives.<oid>.done` and `.failed` are `false`; and every lore
  entry's `entry.<id>.read` and `.everRead` are `false`.

For a `<match>` whose subject is a bare state path with no effective value, an
`is` arm is satisfied only when its value is exactly `"unset"`; other `is`
values are definitely false. Other arms evaluate normally, and an evaluation
error means the arm is not satisfied.

> **History.** Through `0.12.0` the gate was **major.minor**, and consecutive
> releases (`0.11.0`, `0.12.0`) moved the gated line while changing nothing
> an engine reads. `0.13.0` relaxed the gate to MAJOR only for the 0.x line,
> but D6 now makes the pre-1.0 exception explicit: minors MAY change the IR
> shape, so engines pin the exact `irVersion` minor until 1.0. After 1.0,
> compatibility is governed by that major line's policy.

## Addressing

Every command carries a **`position`** (`address.rs`), a string
`"{section}-{(index+1)*100}"` (e.g. `"001-0300"`). The first segment is the
one-based position of the record's `## ` section in document order; the second
is the record's order within that section. `position` is **regenerated on
every compile** — it is build-local execution position, never identity, and
never a localization, patch, or save key.

**Field width.** Both segments are zero-padded to a width computed from the
document — at least `3` for the section and `4` for the index, wider when the
document needs it — and that width is **uniform across the whole artifact**.
Therefore, *within one artifact, lexicographic order over every emitted
`position` equals execution order.*

The stable joins are:

- **`lineId` / `voiceKey`** on every `line`, derived from per-speaker `code`.
  They are what you key localization and voice assets on. Every line carries
  both, whatever its role — narration and `mono` included; a `voiceKey` is a
  join, not a claim that a recording exists. Under the default templates a
  line's `lineId` is `{prefix}.{speaker}_{code}` and its `voiceKey`
  `{prefix}.{speaker}-{code}`, with `{prefix}` = `meta.id`; a line expanded
  from a component `::use` is minted under `{prefix}.{component}#{instance}`,
  where `instance` is the authored key from `instance="…"`, not a sibling
  ordinal. Missing keys are warning-bearing positional fallbacks until
  `lute tag` writes them. A project may re-template both (`identity:` in
  `lute.project.yaml`), so treat them as opaque keys and never parse them.
- **`selectionKey`** on every `choice` and `hub` — the state path that
  records the pick (`scene.choices.<branch or hub id>`).
- **Section ids.** The `sections` table describes the document's sections:
  `{ "section": <one-based position>, "heading": <text>, "id"?: <token> }`.
  `id` is present when the author wrote `## Heading {#id}`; it is identity
  metadata only and is never a jump target. A section without an `id` has
  only its position, which is not stable across edits.

```lute check
---
kind: scene
id: identity-runtime
---
## Opening {#opening}
@narrator{code="intro"}: Stable content.
```

```json
"sections": [
  {
    "section": 1,
    "heading": "Opening",
    "id": "opening"
  }
]
```

Artifacts may carry `identityRenames`, an optional sorted list of
`{from, to}` canonical `NodeKey` pairs. Engines apply these authored migrations
when loading save-shaped identity; they MUST NOT infer renames from `position`,
text, or spans. The same list is exposed by `project.index.json`.

## Control flow

The `commands` array is already in **final execution order**. The engine walks
it with a program counter, resolving control-flow targets — which are all
`position` strings — against a `position → index` map:

- **`jump.target`** — unconditional transfer. An authored
  `::jump{to="name"}` resolves to the position of the first record after the
  matching `::label{name="name"}`; a label emits no record of its own.
- **`choice` / `hub`** — each option carries a `target` (taken when the option
  is chosen) and the record carries a `converge` position (where control
  resumes after the construct); a `hub` may also carry `return`. A `converge`
  may point "one past the last record" of the addressing unit, i.e.
  fall-through.
- **`match`** — `subject` is the matched CEL slot; each arm carries a
  `target`, plus an optional `otherwise` and a `converge`; an `is` field marks
  the semantic shorthand. When the subject is an unset bare state path, `is`
  values other than `"unset"` are definitely false, while `is: "unset"` is
  satisfied. Other arms evaluate normally; evaluation errors are not satisfied.
- **`quest` / `on`** — declaration heads: `objective.body` and `on.body` are
  position targets into separately-emitted body segments (see
  [quest-lifecycle.md](./quest-lifecycle.md)); `entry.body` and `beat.body`
  likewise (see [lore-entries.md](./lore-entries.md)).
- **`end`** — terminates the walk; carries an optional free-form `reason`.
  Equivalent to running off the end of `commands`, except the reason is
  available to the host.
- **`barrier`** — a timeline join (see
  [timeline-semantics.md](./timeline-semantics.md)).

All control-flow targets are resolved to concrete positions at compile time
(`Command::for_each_target`); an unresolved label is a compiler bug, never
shipped. The position-bearing fields are exactly: every command's `position`;
`choice.converge` and `choice.options[].target`; `hub.converge`, `hub.return`
and `hub.options[].target`; `match.converge`, `match.otherwise` and
`match.arms[].target`; `jump.target`; `quest.objectives[].body` (`null` for an
empty body); `on.body`; `entry.body`; and `beat.body`. No other field holds a
position.

## Command records

Every command is a JSON object whose first three keys are `kind`, `family`,
and `position`, followed by the record's own fields and the optional common
siblings `timing`, `provenance`, and `source`. An absent optional is omitted,
never `null`.

### The `family` marker

`family` groups the kinds by what the engine has to do with them. It is fixed
per kind:

| `family` | `kind`s |
|---|---|
| `content` | `line` |
| `staging` | `bg`, `music`, `sfx`, `vfx`, `actor`, `camera`, `cg`, `video`, `sequence` |
| `state` | `set`, `assert`, `retract` |
| `control` | `choice`, `match`, `hub`, `jump`, `end`, `barrier` |
| `declaration` | `quest`, `on`, `entry`, `accept`, `beat` |
| `plugin` | `plugin` |

An engine dispatches on `family` first and then on `kind`. A presentation layer
can route every `staging` record to the stage without enumerating kinds, and a
headless consumer (a test harness, a text-only reader) can skip `staging`
wholesale. `family` never replaces `kind`: an unknown `kind` is still a hard
error, even inside a known family. `accept` is a `declaration` that executes
in flow: reaching it activates its quest (see
[quest-lifecycle.md](./quest-lifecycle.md)).

### Common siblings: `timing`, `provenance`, `source`

- **`timing`** — the record's resolved blocking and scheduling, in one object:
  `wait` (boolean), `duration` / `delay` / `at` (seconds), and `timeline` (the
  zero-based ordinal of the `<timeline>` the record was emitted from — an
  ordinal, not seconds). Each member is omitted when absent, and `timing`
  itself is omitted when every member is. An engine reads timing only from this
  object. A `barrier` has no `timing`: its `timeline` and `at` are direct
  fields (see [timeline-semantics.md](./timeline-semantics.md)).
- **`provenance`** — present only on a record the compiler injected:
  `{ "by": "<rule>", "explanation": "<English>" }`. `by` names the rule
  (`auto-anchor-on-show`, `auto-pose-reset`, …); `explanation` is
  human-readable text nothing dispatches on.
- **`source`** — `{ "component": "<name>" }` on a record expanded from a
  component `::use`.

A plugin may declare cross-cutting stamp attributes; they are flattened beside
these siblings and read like the declaring plugin's directive fields.

### Staging records

Staging records (`family: "staging"`) are the presentation stream. Each
`kind` is named after the directive that produces it. A field the table marks
with a domain holds a member of that project-declared domain (listed in the
envelope's `enums`); the engine maps each member to its own assets and
effects.

| `kind` | Fields |
|---|---|
| `bg` | optional `location`, `time`, `assetId` |
| `music` | optional `playback` (domain `musicPlayback`), `mood` (domain `mood`), `volume` (domain `volume`), `assetId` |
| `sfx` | optional `sound`, `assetId` |
| `vfx` | required `type` (domain `vfxType`); optional `label`, `transition` |
| `actor` | required `character`; optional `anchor` (domain `anchor`), `action` (domain `action`), `exit`, `emotion` (domain `emotion`), `costume` (domain `costume`); injected-only `posReset`, `preload` |
| `camera` | optional `focus` (a cast reference), `framing` (domain `framing`), `move` (domain `cameraMove`), `transition` (domain `transition`) — at least one is present |
| `cg` | required `assetId`, required `display` (`show` \| `hide`); optional `layout` (domain `cgLayout`) |
| `video` | required `assetId`, required `display` (`show` \| `hide`) |
| `sequence` | required `name` (domain `sequence`) |

- **`actor`** carries `exit: true` when its `action` is one of the domain's
  `exits:` members; the character leaves the stage. An omitted authored
  `anchor` resolves to the `anchor` domain's `default:`, emitted as an
  injected `actor` record (`by: "auto-anchor-on-show"`). An authored `emotion`
  updates the character's stage emotion exactly like a line's `emotion`.
- **`camera`** values are opaque domain members; there is no numeric
  transform to interpret, and camera state is not part of save data.
- **`cg`** and **`video`** always carry their resolved `display`; an omitted
  authored value is `show`.
- **`sequence`** is a reference to an engine-owned cinematic by name. Its
  `timing.wait` defaults to `true`: the engine plays the named sequence and
  blocks until it finishes. The reference runner records the reference and
  does not simulate the sequence.

The staging, line, and locale examples in this section are the real
`lute compile` output of this document:

```lute check
---
kind: scene
id: harbor-night
title: Harbor night
pov: mira
enums:
  musicPlayback: [start, stop]
  sequence: [harborOpening]
  framing: [close, wide]
  anchor:
    members: [left, center, right]
    default: center
  action:
    members: [fadeIn, fadeOut]
    exits: [fadeOut]
  emotion: [calm, smile]
  textStyle: [emphasis, whisper]
---

## Arrival {#arrival}

::sequence{name="harborOpening"}
::music{playback="start" assetId="BGM.harbor"}
::actor{character="mira" action="fadeIn" emotion="calm"}
::camera{focus="mira" framing="close" duration="0.5"}
::cg{assetId="CG.harbor"}
@mira{code="greet"}: :emphasis[Hello] :pause{s=0.5}:speed[{{userName}}!]{rate=1.25}
::actor{character="mira" action="fadeOut"}
```

Its staging records — note the injected anchor after the authored `actor`,
the resolved `display` on `cg`, and `exit` on the closing `actor`:

```json
[
  {
    "kind": "sequence",
    "family": "staging",
    "position": "001-0100",
    "name": "harborOpening",
    "timing": {
      "wait": true
    }
  },
  {
    "kind": "music",
    "family": "staging",
    "position": "001-0200",
    "playback": "start",
    "assetId": "BGM.harbor"
  },
  {
    "kind": "actor",
    "family": "staging",
    "position": "001-0300",
    "character": "mira",
    "action": "fadeIn",
    "emotion": "calm"
  },
  {
    "kind": "actor",
    "family": "staging",
    "position": "001-0400",
    "character": "mira",
    "anchor": "center",
    "provenance": {
      "by": "auto-anchor-on-show",
      "explanation": "`mira` shown without an explicit anchor; defaulting to `center`"
    }
  },
  {
    "kind": "camera",
    "family": "staging",
    "position": "001-0500",
    "focus": "mira",
    "framing": "close",
    "timing": {
      "wait": false,
      "duration": 0.5
    }
  },
  {
    "kind": "cg",
    "family": "staging",
    "position": "001-0600",
    "assetId": "CG.harbor",
    "display": "show",
    "timing": {
      "wait": false
    }
  },
  {
    "kind": "actor",
    "family": "staging",
    "position": "001-0800",
    "character": "mira",
    "action": "fadeOut",
    "exit": true
  }
]
```

### Content lines

A `line` (`family: "content"`) carries `role`, `speaker`, `text`, `lineId`,
and `voiceKey`, plus optional presentation attributes (`emotion`, `variant`,
`action`, `dialogMotion`, `as`), `placeholders`, `segments`, `texts`, and
`localeSegments`.

`role` is one of `dialogue` (the default), `narration` (the narrator
speaker), `mono` (interior monologue — only the effective POV character, or
a speaker the document allow-lists in `monoSpeakers`), `os` (off-screen),
or `vo` (voice-over).

`text` is always the **plain text** of the line in the source language:
inline modifier markup is removed, escapes are decoded, and `{{…}}`
interpolation markers stay verbatim for the engine to substitute from
`placeholders`. Backlog, text-to-speech, search, and any consumer that does not
render presentation markup read `text` alone.

### Inline text modifiers: `segments`

An author may mark up a line's text with inline modifiers: `:pause{s=0.5}`
(a pause, in seconds), `:speed[…]{rate=1.25}` (a delivery rate for the span),
and `:name[…]` for any member of the project's `textStyle` domain (for example
`:emphasis[…]`). A line that uses at least one modifier carries
**`segments`**, its presentation as an ordered list of runs; a line without
modifiers has no `segments`, and the engine renders `text`.

Each segment is one of:

| Segment | Meaning |
|---|---|
| `{ "text": "…", "styles"?: […], "rate"?: <number> }` | A text run. `styles` lists the active `textStyle` members, outermost first; `rate` is the active speed multiplier (the innermost `speed` when spans nest). Each is omitted when absent. |
| `{ "pause": <seconds> }` | A pause leaf: stop for that many seconds before the next run. |

Adjacent runs with the same styles and rate are coalesced, and empty runs are
omitted, so the text runs concatenate to exactly `text`. `{{…}}` markers stay
verbatim inside the runs; substitute them from `placeholders` in plain-text
order. Style names are project vocabulary: the engine maps each `textStyle`
member to its own rendering. `pause` and `rate` are presentation timing only
and change no state. The reference runner keeps no wall clock and does not
wait out a pause; `lute play --json` carries each line's `segments` with the
markers already substituted.

The `greet` line of the example document above compiles to:

```json
{
  "kind": "line",
  "family": "content",
  "position": "001-0700",
  "role": "dialogue",
  "speaker": "mira",
  "text": "Hello {{userName}}!",
  "lineId": "harbor-night.mira_greet",
  "voiceKey": "harbor-night.mira-greet",
  "placeholders": [
    {
      "kind": "reserved",
      "token": "userName"
    }
  ],
  "segments": [
    {
      "text": "Hello",
      "styles": [
        "emphasis"
      ]
    },
    {
      "text": " "
    },
    {
      "pause": 0.5
    },
    {
      "text": "{{userName}}!",
      "rate": 1.25
    }
  ]
}
```

### Localized lines: `texts` and `localeSegments`

When an artifact is compiled with a locale bundle (`lute compile --locales`),
each translated line gains **`texts`**: locale tag → the translation's plain
text, keyed on the line's `lineId`. `text` remains the source-language
string. A line with inline modifiers additionally gains
**`localeSegments`**: locale tag → that translation's segments, in the same
shape as `segments`. Every translated locale always appears in `texts`, so
plain-text consumers never need to derive it.

`lute loc export` hands translators the source markup (not the plain text),
and each translation keeps that markup. The merge requires the translation's
modifiers (name, span or leaf form, and attributes) to match the source's as a
multiset — positions may move — and rejects a mismatch with
`E-L10N-MODIFIERS` rather than emit it. An engine can therefore render
`localeSegments[locale]` with the same style and timing vocabulary as the
source. Merging the `ja-JP` translation
`:emphasis[こんにちは]、:pause{s=0.5}:speed[{{userName}}!]{rate=1.25}` into the
`greet` line above adds:

```json
"texts": {
  "ja-JP": "こんにちは、{{userName}}!"
},
"localeSegments": {
  "ja-JP": [
    {
      "text": "こんにちは",
      "styles": [
        "emphasis"
      ]
    },
    {
      "text": "、"
    },
    {
      "pause": 0.5
    },
    {
      "text": "{{userName}}!",
      "rate": 1.25
    }
  ]
}
```

An unmodified line gets only its `texts` entry.

## Dispatcher loop

A minimal engine is a program counter over `commands`, dispatching on `family`
and then `kind`. The kinds below are exactly the `Command` variants (`ir.rs`);
an unknown `kind` must halt with an error.

This loop describes artifact dispatch. The resumable runtime hands resolved
records to a host as events rather than rendering or calling services itself;
see the [event contract](./event-contract.md) for the step and await boundary.

```ts
type Position = string;

// Every CEL slot carries `expr`, including relational queries and visited().
// Evaluate the tree with live state, facts and the declared host functions.
const evalSlot = (expr, state, facts) => evalExpr(expr, state, facts);

function run(artifact: ExecutionIr, state: StateStore, facts: FactStore) {
  assertExactMinorCompatible(artifact.irVersion); // pre-1.0 exact-minor gate
  assertRequiredSemantics(artifact.requiredSemantics, engineMatrix);

  // scene: one continuous command stream. quest / lore: see
  // quest-lifecycle.md and lore-entries.md — declaration heads are consulted
  // by the lifecycle and occasion drivers, not walked as sequential steps.
  const index = new Map<Position, number>();
  artifact.commands.forEach((c, i) => index.set(c.position, i));

  let pc = 0;
  while (pc < artifact.commands.length) {
    const cmd = artifact.commands[pc];
    let next: Position | null = null; // null ⇒ fall through to pc + 1

    switch (cmd.family) {
      case "content": // line; honor cmd.timing, render cmd.segments when present
        present(cmd, state); // substitute cmd.placeholders into text / segments
        break;

      case "staging": // honor cmd.timing (wait, duration, delay, at)
        switch (cmd.kind) {
          case "bg":       stageBg(cmd); break;
          case "music":    stageMusic(cmd); break;    // cmd.playback
          case "sfx":      stageSfx(cmd); break;
          case "vfx":      stageVfx(cmd); break;      // cmd.type
          case "actor":    stageActor(cmd); break;    // cmd.provenance ⇒ injected
          case "camera":   stageCamera(cmd); break;   // focus / framing / move / transition
          case "cg":       stageCg(cmd); break;       // cmd.display, cmd.layout
          case "video":    stageVideo(cmd); break;    // cmd.display
          case "sequence": playSequence(cmd); break;  // blocks while cmd.timing.wait
          default: throw new UnknownCommandKind(cmd.kind);
        }
        break;

      case "state":
        switch (cmd.kind) {
          case "set":     writeState(state, cmd.path, cmd.op, evalSlot(cmd.expr, state, facts)); break;
          case "assert":  facts.assert(cmd.relation, cmd.args); break;   // positive delta
          case "retract": facts.retract(cmd.relation, cmd.args); break;  // negative delta (args may be "_")
          default: throw new UnknownCommandKind(cmd.kind);
        }
        break;

      case "control":
        switch (cmd.kind) {
          case "choice":
          case "hub": {
            // per option: evalSlot(o.expr, …); record the pick under cmd.selectionKey
            const opt = pickOption(cmd, state);
            next = opt ? opt.target : cmd.converge;
            break;
          }
          case "match": {
            const arm = cmd.arms.find(a => truthy(evalSlot(a.expr, state, facts)));
            next = arm ? arm.target : (cmd.otherwise ?? cmd.converge);
            break;
          }
          case "jump":    next = cmd.target; break;
          case "end":     finish(cmd.reason); return;  // terminate the walk
          case "barrier": joinTimeline(cmd.timeline, cmd.at); break; // see timeline-semantics.md
          default: throw new UnknownCommandKind(cmd.kind);
        }
        break;

      case "declaration":
        switch (cmd.kind) {
          case "quest":  registerQuest(cmd); break;     // quest-lifecycle.md
          case "on":     registerHandler(cmd); break;
          case "entry":  registerEntry(cmd); break;     // lore-entries.md
          case "beat":   registerBeat(cmd); break;      // beats-and-occasions.md
          case "accept": acceptQuest(cmd.quest); break; // activates it iff `unset`
          default: throw new UnknownCommandKind(cmd.kind);
        }
        break;

      case "plugin": // bridge calls + resolved effects; see bridge-protocol.md
        callBridgeAndApplyEffects(cmd, state);
        break;

      default:
        throw new UnknownCommandKind(cmd.kind); // version-negotiation: hard error
    }

    pc = next === null ? pc + 1 : index.get(next)!;
  }
}
```
`evalSlot` evaluates the slot's `expr` with the declared `celEnv`, including
the host functions; every slot has a tree, so no CEL-text fallback is needed.
Facts remain the Datalog store and bridge commands remain host operations.
The execution IR is inert data; behavior begins in this dispatcher.
