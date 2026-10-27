---
title: Porting a content set
description: "How to bring a large authored content set (a spreadsheet, another engine's export) into Lute with an importer: per-surface stamp columns, host tokens, reward effects, project diagnostic levels for an imported subtree, staging a speaker, node and pair lints for structural and dependent-attribute rules, bounded retry loops, scenes that a directive or reward raises, localizable directive text, and what structured values still lack."
---

This guide is for a team that already has a content set in another shape (spreadsheet rows, an
engine's export, an older script format) and writes an **importer** that emits `.lute` documents. The
importer is the author. Lute is not asked to accept what the sheet looked like; the plugin declares
what the sheet's columns, tokens and rewards mean, and `lute check` holds every imported row to it.

Everything here is available since 0.50. The recipes answer the seven problems a large real port
ran into (issues #162, #163, #164, #166, #169, #172 and #177), and each section ends with what the
toolchain catches. A runnable project that uses all of them is
[`docs/examples/plugin-surfaces/`](https://github.com/journeyWorker/lute/tree/main/docs/examples/plugin-surfaces):

```sh
lute check --project docs/examples/plugin-surfaces
lute test  --project docs/examples/plugin-surfaces
lute lint  --project docs/examples/plugin-surfaces
```

The plugin manifest vocabulary is on [Plugin manifests](/plugins/manifests/). This page collects the
port recipes around it.

Since 0.51 the guide also covers the problems that issues #159, #165, #170, #172 and #176 reported:
[retry loops](#retry-loops-with-a-bounded-jump), [scenes that a directive or a reward raises](#scenes-that-a-directive-or-a-reward-raises),
[directive text that players read](#text-in-a-directive-that-players-read), [balance rules](#balance-rules-with-pair) and
[structured attribute values](#structured-values-are-not-supported-yet). The runnable project for these is
[`docs/examples/flow-and-text/`](https://github.com/journeyWorker/lute/tree/main/docs/examples/flow-and-text).

## Columns become stamps on the surface they belong to

A sheet has different columns for different rows: a voice asset for a dialogue row, an asset name for
a background row, a menu type for a choice row, an option key for each option. A **stamp** is a plugin
attribute that is not part of a directive's own declaration. Since 0.49 a stamp is admissible on every
directive and every content line, which is too wide for a column that belongs to one kind of row.
Since 0.50 a `stampAttrs` entry names its surfaces with `on:`:

```yaml
# plugins/acme.stage/stampattrs/columns.yaml
stampAttrs:
  - { name: voiceAssetId, type: { providerRef: voice }, required: true, on: [line] }
  - { name: assetName,    type: string, on: [bg, actor] }
  - { name: menuType,     type: { enum: [plain, timed, minigame] }, on: [branch, hub] }
  - { name: optionKey,    type: string, required: true, on: [choice] }
  - { name: cue,          type: string }                          # cross-cutting, as in 0.49
```

```lute
<branch id="route" prompt="Which way?" menuType="plain">
  <choice id="goLeft" text="Left" optionKey="L1">
    @mira{voiceAssetId="VO.mira.0010"}: Left it is.
  </choice>
</branch>
```

`on:` takes a non-empty list of **surface names**. The vocabulary is closed:

| Surface name | Where the stamp may be written |
|---|---|
| `line` | every content line (`@speaker{…}: text`), including lines in component, quest, objective, `<on>`, entry and beat bodies |
| `branch`, `hub` | the element of that name |
| `choice` | every `<choice>`, in a `<branch>` or a `<hub>` |
| a directive tag | that directive, if it is a directive of the active profile with a stamp carrier: `bg`, `actor`, a plugin directive. Not `jump`, `label` or `clear`, which emit no stamped record |

The four names `line`, `branch`, `hub` and `choice` always mean the core surface, and they are already
reserved as plugin directive names, so a plugin directive tagged `line` cannot be named in `on:`. The
order inside `on:` does not matter.

### What a column can and cannot do

- **Without `on:`** a stamp keeps its 0.49 meaning exactly: every directive that has a stamp carrier
  and every content line. It is **not** admissible on `<branch>`, `<hub>` or `<choice>`. The logic
  tags stay closed unless a stamp names them, so `branch`, `hub` and `choice` are never admissible
  for a cross-cutting stamp.
- **A scoped stamp is refused elsewhere.** Writing `voiceAssetId` on `::bg` is `E-UNKNOWN-ATTR`, and
  the message says where the stamp does apply instead of listing the surface's own attributes.
- **`required: true` needs `on:`.** A cross-cutting stamp cannot be required: a stamp required on every
  record is not a column. Before 0.50 the flag was silently ignored; it is now `E-PLUGIN-PARSE` at
  load. On a scoped stamp it is enforced: a surface that does not write the stamp is `E-MISSING-ATTR`
  on the directive, on the line, or on the whole `<branch>`/`<hub>`/`<choice>` element. The rule applies wherever the
  surface is written, component bodies included, and a line that `::use` splices in is checked once,
  in its component. A declared `default:` does not satisfy it, because a stamp is never injected.
- **One name may have two types**, as long as the entries' surface sets are disjoint: `assetName` can
  be a `string` on `bg` and an `int` on `actor`. Each surface serializes the value with its own type.
  Any overlap is the existing duplicate code: `E-PLUGIN-DUP-ID` within one plugin package, and
  `E-PLUGIN-DUP-ACROSS` between two plugins, each naming the shared surfaces. An entry without `on:`
  overlaps every other entry of its name.
- **A scoped stamp cannot shadow its surface's own attribute.** `prompt` on `branch`, `text` on
  `choice` or `code` on `line` could never be read, so the declaration is
  `E-PLUGIN-RESERVED-STAMP-ATTR`. The surface's own attributes are the directive's declared
  attributes; the line's known attributes and `when`; `id`, `prompt`, `timeout` and `default` on
  `<branch>`; `id` and `prompt` on `<hub>`; and `id`, `text`, `when`, `into`, `value`, `once` and `exit`
  on `<choice>`, plus its retired `label`, `as` and `persist`.
- **An `on:` name that is no surface** is `E-PLUGIN-STAMP-SURFACE` at assembly, with a did-you-mean
  (`bgg` for `bg`). A directive of another plugin is a surface only when that plugin is in your
  `depends:`, so that it is in every profile that activates yours.

Load-time faults are all `E-PLUGIN-PARSE` at the entry: an `on:` that is not a list of strings or is
empty, a surface repeated in `on:`, and `required: true` without `on:`.

The value is typed by the declaration of the surface it is written on, so a mistyped one is the
ordinary `E-ATTR-TYPE` or `E-BAD-ENUM`. That holds for an `@ref` value on `<branch>`, `<hub>` and
`<choice>` too: a def of the wrong type is `E-REF-TYPE`, and a def that folds outside an enum is
`E-BAD-ENUM`.

In the [example project](https://github.com/journeyWorker/lute/tree/main/docs/examples/plugin-surfaces),
a scene that writes `voiceAssetId` on `::bg` (line 8) and a `<choice id="goLeft">` without `optionKey`
(line 10) gives:

<!-- lute-diagnostics unverified="real lute check output (0.50 build); it matches both the scoped-stamp and the generic `has no attribute` format literals, so the matcher reports it ambiguous" -->
```
./scenes/a.lute:8:33: error [E-UNKNOWN-ATTR] `::bg` has no attribute `voiceAssetId`: it is a stamp of `acme.stage` for content lines only
./scenes/a.lute:10:3: error [E-MISSING-ATTR] `<choice id="goLeft">` requires attribute `optionKey` (a stamp of `acme.stage`)
```

### What reaches the artifact

The runtime never reads stamps. A stamp on a record is in that record's `stamps` object, as in 0.49. A
stamp on `<branch>` or `<hub>` is in the `stamps` of the `choice` or `hub` record, and each option
gains its own `stamps` object from its `<choice>` element (omitted when it has none). An unauthored
stamp is never injected. The menu the runtime awaits carries only the address and the verdicts, so a
host that does not use the reference player reads stamps from the IR record at the menu's address, as
it already reads option text. The reference player's `ChoiceView` carries the element's and each
option's `stamps`. See [Runtime contract](/tooling/runtime-contract/).

## `{{playerName}}`: tokens the host substitutes

An importer meets names the engine fills in at render time: the player's name, a rank, a platform.
Declare each one with a `hostTokens` export (`exports: { hostTokens: hosttokens/ }`):

```yaml
# plugins/acme.stage/hosttokens/tokens.yaml
hostTokens:
  playerName: { type: string }
  rank:       { type: int }
  platform:   { type: { enum: [ios, android, web] } }
```

```lute
@mira: Welcome, {{playerName}}. You are our {{rank:ordinal}} guest, and {{platform:capitalize}} is fine.
```

- A token name is an **identifier** (an ASCII letter or `_`, then letters, digits or `_`). `player-name`
  and `2ndName` are `E-PLUGIN-PARSE` at load, because a token is written bare inside `{{…}}`. The value is
  `{ type }` with `string`, `int` or `{ enum: [...] }`.
- A name that reads as a state root or a CEL literal (`scene`, `run`, `user`, `app`, `quest`, `entry`,
  `prev`, `clock`, `occasion`, `season`, `true`, `false`, `null`) is `E-PLUGIN-RESERVED-NAME`:
  `{{run}}` would read as a state root.
- `lute.core` declares `userName: { type: string }`. Redeclaring a token that another plugin, or
  `lute.core`, declares is `E-PLUGIN-DUP-ACROSS`.
- A format hint is checked against the token's type. A `string` or `enum` token takes the text hints
  (`{{platform:capitalize}}`), and a number hint on it is `E-REF-TYPE`. An `int` token takes the
  number hints (`{{rank:ordinal}}`), and a text hint on it is `E-REF-TYPE`.
- An **undeclared** bare name is `E-CEL-PROFILE`, and the message now says to declare the name in a
  plugin's `hostTokens:` export. `{{player-name}}` and `{{2nd}}` are not identifiers, so they keep the
  `E-CEL-PROFILE` they always had.
- The escape `\{{name}}` is unchanged: it is never a placeholder.

A host token is never a state read, and the runtime does not substitute it: every token marker stays
verbatim in the rendered text, and the host or presenter replaces it. That holds for every token, not
only `userName`. The IR placeholder is `{ "kind": "reserved", "token": "playerName" }`, and an
interpolation counts as one word in the lints, as before. `lute context` lists the declared tokens.

## Rewards that assert, retract and accept

A port often carries rows like "this quest hands out a badge" or "finishing it offers the next one".
Before 0.50 a reward kind could only credit a state path, so the fact the badge stood for was invisible to
the checker and the quest it unlocked drew a false `E-QUEST-UNREACHABLE`. A reward kind now declares
`effects:`, in the directive vocabulary:

```yaml
# plugins/acme.stage/rewardkinds/rewards.yaml
rewardKinds:
  BADGE:       { target: { entity: person, required: true }, effects: { asserts: ["met(@target)"] } }
  REVOKE:      { target: { entity: person, required: true }, effects: { retracts: ["met(@target)"] } }
  OFFER_QUEST: { target: { required: true }, effects: { accepts: ["@target"] } }
```

The example project's quests use all three (`quests/visits.lute`):

```lute check-in-project="docs/examples/plugin-surfaces/quests/visits.lute"
---
kind: quest
id: visits.quests
title: Visits
---
/*
Reward effects (0.50):
  - firstVisit's BADGE asserts met(mira) at the grant; secondVisit starts on
    that fact, so it is reachable only because the reward produces it. Its
    `follows` places it in the scenario graph, where `lute scenario --facts`
    draws the fact edge firstVisit -> secondVisit [met(mira)].
  - firstVisit's OFFER_QUEST accepts giftErrand, an accept-driven quest no
    `::accept` names.
  - secondVisit's REVOKE retracts met(mira) when it completes.
*/

<quest id="firstVisit" tier="run" title="Say hello to Mira" start="true">
  <reward kind="BADGE" amount="1" target="mira"/>
  <reward kind="OFFER_QUEST" target="giftErrand"/>
  <objective id="talk" title="Talk to Mira" done="run.talked"/>
</quest>

<quest id="secondVisit" tier="run" title="Visit Mira again" follows="completed('firstVisit')" start="holds('met', ['mira'])">
  <reward kind="REVOKE" target="mira"/>
  <objective id="chat" title="Have a second chat" done="run.chatted"/>
</quest>

<quest id="giftErrand" tier="run" title="Carry Mira's gift to Tomas">
  <objective id="deliver" title="Deliver the gift" done="run.delivered"/>
</quest>
```

`OFFER_QUEST` names `giftErrand`, an accept-driven quest (no `start`). Naming `secondVisit`, which has a
`start`, would be `E-ACCEPT-TARGET`, as an `::accept` of it is.

### What an effect may say

- `effects` has `asserts`, `retracts` and `accepts`, each optional. An empty `effects: {}` is
  `E-PLUGIN-PARSE` ("declares no effect").
- **`writes` is not admitted.** A reward's state write is `credits:`, and `effects.writes` is
  `E-PLUGIN-KEY` saying so.
- Pattern arguments of `asserts` and `retracts` are members, `true` / `false`, `@target` and, in
  `retracts` only, `_`. Any other `@name` is `E-PLUGIN-PARSE`: a `<reward>` takes no attribute but its
  own, so a kind's effects read only `@target`.
- `accepts` entries are `@target` or a literal quest id. A kind with `target: { forbidden: true }` whose
  effects read `@target` is `E-PLUGIN-PARSE`, because the effect could never bind.

### How each `<reward>` is judged

`@target` is bound to the reward's quoted `target=`. Each resolved fact pattern is judged as the
`::assert` or `::retract` of it would be (relation declared, arity, argument kinds and members,
closed-kind membership), with the same codes at the reward's `kind=`, and the message is prefixed with
`` `<reward kind="BADGE">` asserts `met(mira)` (its kind's `effects`): ``. Each resolved quest id in `accepts` is
judged as `::accept` judges it, project-wide: an unknown id (with a did-you-mean), a quest that has a
`start`, and a child that auto-activates are `E-ACCEPT-TARGET`, at the reward's `target=` when the id came
from `@target`, otherwise at `kind=`.

A reward with **no `target=`** applies no effect that reads `@target`, with no extra diagnostic (the
directive rule). A kind with `target: { required: true }` already reports `E-REWARD-TARGET` for it.

### When it happens

At the grant instant, for a reward that is granted (its `outcome` filter matches and its `when=`
holds), in declaration order and **per reward**:

1. the `grant` record and its `credits` write, as before;
2. the reward's `retracts`, then its `asserts`, through the one runtime assert/retract path (the
   `E-FACT-EXCLUSIVE` check included);
3. its `accepts`, through `::accept`'s path. An accept-driven quest activates on the next lifecycle
   round, and a quest already past `unset` is left alone, with the record saying so.

Then the next reward. Objective grants precede quest grants, and all grants of a transition precede
its lifecycle handler body, so at run time a `questComplete` handler reads the fact the reward
asserted. An effect fires exactly once per grant.

Each effect record names the grant it follows with a `grant` key, and `lute play`, `lute run` and
`lute trace` print it. The example's play script (`lute play --project . --script plays/visits.play.yaml`):

```console
── step 1 (the player talks to Mira) · engine ──────────────
  set run.talked = true
  firstVisit.talk done
  quest firstVisit -> complete
  grant[#1 i0] firstVisit BADGE 1 -> mira
  assert met(mira)  (effect of reward BADGE)
  grant[#1 i1] firstVisit OFFER_QUEST 1 -> giftErrand
  quest giftErrand accepted  (effect of reward OFFER_QUEST)
  quest giftErrand -> active
  quest secondVisit -> active
```

### What every analysis sees

Every analysis reads a reward effect at the owner's grant point: the quest's completion (before its
`questComplete` handlers), its failure for `outcome="failed"` (before `questFailed`), or the
objective's completion. A reward's **assert is a producer only**: it adds to what *may* hold and
never to what *must* hold, guarded or not, in the handler bodies, the objective body, or anything
gated on the quest. Lifecycle handlers can be deferred past other quests' grants and the occasion's
beats, a `user`-tier quest stays complete across a reset that clears its facts, and a host may seed
a quest complete without its grants, so no grant-to-fact implication is sound. A reward's
**retract** removes what it matches from what must hold, as a guarded `::retract` does. So:

- `secondVisit` above is reachable: the reward is a producer of its `start` fact, so there is no
  `E-QUEST-UNREACHABLE`. `E-OBJECTIVE-UNSATISFIABLE`, `W-RELATION-UNREAD` and `E-FACT-EXCLUSIVE` see the
  effect too. A guard that tests `met(mira)` after the grant draws neither `W-FACT-GUARANTEED` nor
  `E-ARM-DEAD`: the fact is possible there, not guaranteed.
- `lute scenario` draws the fact edge and the accept edge, and `lute lore` / `lute scenario knowledge`
  name the producer: `` met(mira) — produced by reward BADGE of quest `firstVisit` ``.
- `W-QUEST-NEVER-ACCEPTED` now says "no `::accept` or reward `accepts` in the project names it", and a
  reward accept silences it.
- **Start-cycle detection is unchanged.** A reward accept anchors an edge for reachability and
  connectivity only; it does not gate a quest's start, so it never draws `E-QUEST-START-CYCLE` or
  `E-QUEST-START-SELF`.

An artifact that has a reward effect carries the semantic id `lute.quest.rewardEffects/1`. An engine
that ignores the fields would lose facts and quest activations, so one that does not implement the id
refuses the artifact. See [Runtime contract](/tooling/runtime-contract/).

## Levels for an imported subtree

A first import is noisy. A staging warning on 212 rows is true and not worth fixing on day one, and
`--deny-warnings` in CI means you cannot have a warning you have not decided about. The project
manifest sets a level per diagnostic code, optionally for a path:

```yaml
# lute.project.yaml
diagnostics:
  W-DISPLAY-NAME-DUP: error
  W-STAGE-ABSENT: { level: off, paths: ["scenes/imported/**"] }
```

- `diagnostics:` maps a code to a level (`off`, `info`, `warn`, `error`) or to `{ level, paths }`. One
  entry per code.
- `paths` are project-root-relative globs with the `lute.lint.yaml` `ignore:` syntax (`*` within a
  segment, `**` across segments). With `paths`, the level applies only to diagnostics of documents that
  match; elsewhere the code keeps its native severity. A diagnostic with no document path (a manifest
  diagnostic) matches only an entry without `paths`.
- **Only `W-*` codes are configurable.** An `E-*` code is a contract violation and is never re-levelled,
  and an `L-*` lint rule is set in `lute.lint.yaml` `rules:`. A level that is not one of the four, a code
  the toolchain does not define (with a did-you-mean), or `paths` that are empty, absolute or contain
  `..` is `E-DIAGNOSTICS-CONFIG`, reported once per manifest, and that entry is dropped.

A manifest with `W-STAGE-ABSNT: off` on its line 4 gives:

<!-- lute-diagnostics -->
```
./lute.project.yaml:4:3: error [E-DIAGNOSTICS-CONFIG] `W-STAGE-ABSNT` is not a diagnostic code — did you mean `W-STAGE-ABSENT`?
```

How a level is applied, in this order, at the point that prints a diagnostic:

1. the project level for the code and the document's path;
2. `--deny CODE` names the code: it is promoted to an error **even when the project set it `off`**,
   because an explicit command line beats the config;
3. `--deny-warnings` promotes what is still a warning after step 1. An `off` code stays off, and an
   `info` code stays info.

Errors are never demoted. A code relevelled to `error` fails the verdict and the exit status, as
`--deny` does; in JSON it carries `"severity": "error"` and `"configured": "lute.project.yaml"`, and the
human line carries a `[project]` marker beside `[denied]`. A code relevelled to `info` or `warn` carries
the new severity and the same marker.

**Levels change only what is reported and the exit status.** They never change an analysis, an artifact,
a constraint verdict, the scenario graph, or what a `.test.yaml` or `.play.yaml` expectation observes.
They gate `lute check`, `lute compile` and `lute lint`. The simulating commands (`lute test`,
`lute play`, `lute calendar`) do not apply them at all: they print native diagnostics, and a project
`error` level leaves `lute test`'s results and exit status as they are without it. The LSP applies the
same levels: an `off` diagnostic is not published.

Nothing is hidden without saying so. Every summary line that counts diagnostics appends
`, N suppressed by lute.project.yaml` only when N is above zero, and every JSON report carries a
`"suppressed": { "<code>": n }` map only when it is not empty:

In the example project, `lute check --project .` prints:

```console
ok: ./quests/visits.lute (0 warning(s))
ok: ./scenes/cafe.lute (0 warning(s))
ok: ./scenes/imported/street.lute (0 warning(s), 1 suppressed by lute.project.yaml)
ok: . (3 file(s), 0 project-wide warning(s))
```

A `rules:` entry in `lute.lint.yaml` that names a checker code is `E-LINT-CONFIG`, and the message points
here: `` `W-STAGE-ABSENT` is a checker code, not a lint rule — set its level in `lute.project.yaml` `diagnostics:` ``.

## A line that stages its speaker

`W-STAGE-ABSENT` warns when a character speaks after they left the stage (a declared exit, a `::bg`
scene change, a `::clear`) and nothing showed them again. An importer that knows a row shows an
off-stage speaker emits the staging row first. No new construct is needed, because a record that stages
a character is the `actor` record, from `::actor` or from a plugin directive lowered to it:

```yaml
# plugins/acme.stage/directives/stage.yaml
directives:
  - name: showSpeaker
    attrs:
      - { name: character, required: true, type: string }
      - { name: spriteId, type: string }
    lower:
      record: actor
      fields:
        character: { fromAttr: character }
        costume: { fromAttr: spriteId }
```

```lute
::bg{location="street" time="night"}
::showSpeaker{character="tomas" spriteId="SPR_tomas_02"}
@tomas: Still here?
```

A directive lowered to `actor` stages its `character` like `::actor` (since 0.45), and `character` must
bind to a required attribute or a literal. The actor record has no field named for a sprite, so the
example carries the importer's sprite id in `costume`; a plugin can also keep it in a stamp scoped
`on: [showSpeaker]`. The `W-STAGE-ABSENT` hint now reads "Show them again with an `::actor` (or a plugin
directive declared `lower: { record: actor, fields: { character: { fromAttr: character } } }`)". It
names the full form because `lower: { record: actor }` alone is refused at load (`fields:` is
required) and an empty `fields: {}` is `E-LOWER-RECORD-FIELD`.

While the port is under review, set the code `off` for the imported subtree, as in the
[levels](#levels-for-an-imported-subtree) above, and delete the entry when the importer emits the
staging rows:

```yaml
diagnostics:
  W-STAGE-ABSENT: { level: off, paths: ["scenes/imported/**"] }
```

## Lints for structure and dependent values

Some rules are about where a node sits, not what it contains: every section opens with a `::bg`; a
`::signal` of a kind must be followed by a `<branch>`; a label in a beat starts with `b_`. Others
are about one attribute depending on another: the legal `speed` depends on the `mode`; the legal
option key depends on the menu type of the enclosing `<branch>`. Both are `target: node` lints, which
run over the AST in `lute.lint.yaml` `custom:` rules, in a plugin's `lints:` export, and in the LSP.

A `node` rule gets one row per node, in document order, over every body the checker walks: sections;
`<quest>` declarations with their objectives and `<on>` handlers; `<reward>` children; top-level
`<entry>` and `<beat>`; `<branch>` and `<hub>` choices, choice bodies and a hub's `<return>`;
`<match>` arms and their bodies; and `<timeline>` (one row; clips are not rows). The bindings are
`node`, `scene` (the document row) and `options`:

| Field | Meaning |
|---|---|
| `node.kind` | `line`, `directive` (every `::tag{…}`, core or plugin, `::set` and `::end` included), or the element name (`branch`, `hub`, `choice`, `return`, `match`, `when`, `otherwise`, `timeline`, `quest`, `objective`, `on`, `reward`, `entry`, `beat`) |
| `node.tag` | the directive tag or element name; `""` for a line |
| `node.speaker` | a line's speaker; `""` otherwise |
| `node.attrs` | map string → string of every attribute **as authored**: a key is present only if the author wrote it, and its value is the authored text, unquoted. Lifted attributes (`<branch id>`, `<reward kind target amount>`, `::jump{to}`) and stamps are in it. A bare flag is `"true"`, an `@ref` is its source text, and `::set`, `::assert` and `::retract` have `{}` |
| `node.index`, `node.first`, `node.last` | 0-based position in its body; first and last in that body |
| `node.depth` | 0 for a top-level node of a section or document, plus 1 per enclosing element |
| `node.prev`, `node.next` | `{ kind, tag, attrs }` of the sibling in the same body; every field empty at a body edge |
| `node.parent` | `{ kind, tag, attrs }` of the enclosing element; `kind` is `"section"` at a section's top level and `"document"` at a quest or lore document's top level |
| `node.preceding` | map tag → `{ attrs }`: for each tag, the nearest node with that tag among the earlier siblings of this node and of each ancestor, and the ancestors themselves. It does **not** enter the bodies of earlier siblings (another arm is not on this path), is reset at each section and declaration, and does not key lines |
| `node.section` | `{ index, title, id }` of the enclosing section; `{ 0, "", "" }` outside sections |

`node.attrs` is read from the source, so `<reward kind="COINS" amount="+005"/>` gives `"+005"`, not
`"5"`, and a reward written without `amount` has no `amount` key. The findings span the node, and
`{node.attrs.value}` interpolates into the message.

Two rules keep a lint honest:

- **Guard every index.** A missing key keeps CEL semantics: `node.attrs["x"]` on an absent key is an
  evaluation error (`E-LINT-EXPR`). `&&` short-circuits, so write `"x" in node.attrs && node.attrs["x"] …`,
  `"t" in node.preceding && "k" in node.preceding["t"].attrs && …`, and `key in options.cases && …`
  before indexing an options map.
- **An unknown discriminator is someone else's finding.** A dependent-value rule excludes a
  discriminator it has no case for, and a separate `*-known` rule reports the discriminator itself. Each
  violation is then reported once, by the rule that names it.

The lint expression language also has the CEL string members `s.startsWith(t)`, `s.endsWith(t)` and
`s.contains(t)`, in member-call form, with both operands strings (a non-string operand is an
evaluation error). There are no regexes and no macros.

The recipes below are plugin `lints:` entries. In `lute.lint.yaml` the same entries go under `custom:`
with bare ids; a plugin rule's id is `<plugin-id>/<rule-id>`.

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
```

**One attribute decides another's values** (the `mode` of a move selects its legal `speed`s). The
first rule reports an unknown mode, and the second skips it:

```yaml
lints:
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
```

**A marker directive before the branch decides the legal option values.** `node.preceding` finds the
nearest `::interaction` on the path to each `::option`:

```yaml
lints:
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
```

**The parent `<branch>` carries the discriminator**, as a scoped stamp from the first section of this
guide:

```yaml
lints:
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

### Balance rules with `pair`

A balance rule is a rule about an opener that must be closed: `::group{mode="begin"}` and
`::group{mode="end"}`, a window that opens and closes. It is a `target: pair` lint (since 0.51), in the same
places as a `node` lint. A `pair` rule declares `open` and `close` predicates over the same `node` row, an
optional `inside` predicate, and a `scope`; it has no `when`. A `pair` rule with a `when`, or any other rule
with `open`, `close`, `inside` or `scope`, is `E-PLUGIN-PARSE` in a plugin's `lints:` and `E-LINT-CONFIG` in
`lute.lint.yaml`.

```yaml
lints:
  - id: group-balanced
    target: pair
    open:   'node.tag == "group" && "mode" in node.attrs && node.attrs["mode"] == "begin"'
    close:  'node.tag == "group" && "mode" in node.attrs && node.attrs["mode"] == "end"'
    inside: 'node.kind == "line" || node.tag in ["actor", "sfx"]'   # optional
    scope: body                                                    # body (default) | section
    level: error
    message: "::group begin/end"
```

```lute
::group{mode="begin"}
@mira: Look at this.
@tomas: And this.
::group{mode="end"}
```

That pair is balanced. Each scope runs one pass with one open slot:

- **`scope: body`** (the default) pairs the direct children of each body, in order: the rows that are
  siblings. **`scope: section`** pairs every row of one section or declaration in document order, at every
  depth, so an `open` in one `<choice>` and a `close` in its sibling pair up. `section` is document order,
  not paths.
- A row for which `close` holds while a pair is open closes it. A row that satisfies both `open` and `close`
  is a close while a pair is open, and an open otherwise.
- A row for which `open` holds while a pair is open is a **nested open**: the outer pair stays open. A row
  for which `close` holds with no pair open is an **unopened close**. At the end of the scope, an open pair
  is an **unclosed open**.
- With `inside`, every row strictly between an open and its close for which `inside` is false is an
  **inside fault**. In `body` scope only siblings count, not their descendants.

Each finding carries the rule's code and level, sits on the offending row, and appends a fixed phrase to
the rule's `message`: ` — opened here, with no close in this body` (`in this section` for `section` scope),
` — a close with no open pair in this body`, ` — opened again while the pair from line N is still open`, and
` — not allowed inside the pair opened at line N`. A nested open and an inside fault name the open row as a
related location.

An `inside` violation: the `::bg` between the markers is not a line, `::actor` or `::sfx`, so the rule above
reports it as not allowed inside the pair opened at line 1.

```lute
::group{mode="begin"}
@mira: Look at this.
::bg{location="street" time="night"}
::group{mode="end"}
```

A pair can span a branch. With `scope: section`, a begin before a `<branch>` and an end after it pair
up, and so would a begin inside one `<choice>` and an end inside the next, which a `body` rule would report
as an unclosed open and an unopened close:

```yaml
lints:
  - id: window-balanced
    target: pair
    open:   'node.tag == "window" && "mode" in node.attrs && node.attrs["mode"] == "open"'
    close:  'node.tag == "window" && "mode" in node.attrs && node.attrs["mode"] == "close"'
    scope: section
    level: error
    message: "::window open/close"
```

```lute
::window{mode="open"}
<branch id="buy" prompt="Buy something?">
  <choice id="yes" text="Yes">
    @shopkeeper: Here you go.
  </choice>
  <choice id="no" text="No">
    @shopkeeper: Next time.
  </choice>
</branch>
::window{mode="close"}
```

The guard rule of the node lints holds here too: write `"mode" in node.attrs && …`, because a missing key in an
unguarded predicate is `E-LINT-EXPR` and stops that rule for the document. Each row costs at most three
predicate calls per `pair` rule. See [Linting](/tooling/linting/) for the rest of `lute.lint.yaml`.

## Retry loops with a bounded `::jump`

A sheet row like "play the minigame, and offer it again if the player fails, at most twice" is a loop. Before
0.51 `::jump` could only go forward, so an importer unrolled the loop or hid it in a passthrough directive that no
analysis could see. Since 0.51 a `::jump` that carries `max` may target an **earlier** `::label`:

```lute
::label{name="attempt"}
::tryLock{door="vault"}
<match subject="scene.lock.vault.won">
  <when is="false">
    @mira: Again!
    ::jump{to="attempt" max="2"}
  </when>
  <when is="true">
    @mira: It clicks open.
  </when>
</match>
@theo: That's enough for today.
```

`::tryLock` is a blocking bridge call that writes `scene.lock.vault.won` from the host's answer. `max` is the
number of times the jump may be taken: after `max` takes it is not taken and control falls through to the next
node, so this loop asks at most three times (once, then two retries).

- **`max` is a literal integer from 1 to 99.** An `@ref`, `0`, a negative number or a number above 99 is
  `E-JUMP-MAX`, since a loop bound is static. `max="two"` is only `E-ATTR-TYPE`. `max` on a jump that goes forward
  is `E-JUMP-MAX` too, because it means nothing there. A backward jump without `max` stays `E-JUMP-BACKWARD`, and
  its message now names `max` and the `<hub>` for offering choices again.
- **`when=` and `max` combine:** the jump is taken while `when` holds and takes remain.
- **A loop stays in one body.** The label must be in the jump's own body, or in a body that encloses it through
  `<choice>` bodies of a `<branch>` or `<match>` arms only. A label outside the `<hub>` option the jump is in is
  `E-JUMP-LOOP-SCOPE`.
- **A loop is entered only at its head.** A forward jump from outside to a label inside the loop after its head is
  `E-JUMP-INTO-LOOP`. A forward jump to the head, and a jump out of the loop, are fine.
- **Loops nest, they never cross.** Two loops either nest or are disjoint, or it is `E-JUMP-LOOP-CROSS` at the later
  jump. Two bounded jumps may share one head, each with its own counter.

A nested loop counts for itself. The inner counter returns to 0 every time control reaches the inner head from
outside the inner loop, so an inner `max="2"` inside an outer `max="1"` runs the inner body three times in each of
the two rounds:

```lute
::label{name="round"}
@theo: A new round.
::label{name="knock"}
@theo: Knock.
::jump{to="knock" max="2"}
@theo: Round done.
::jump{to="round" max="1"}
```

Between two takes, control moves only forward, so a region of nesting depth *d* runs at most the product of
(`max` + 1) over its enclosing jumps: every walk ends without the player's help, and the analyses stay finite.

Every flow analysis joins the states of all jumps that reach a label, forward and backward, and iterates a loop head
until it is stable. A read after a label that a jump skipped a `::set` to is `E-MAYBE-UNSET`, a jump past a
re-entrance is `W-STAGE-ABSENT`, and a bounded jump does not end its body (`W-CODE-AFTER-JUMP` stays quiet; its
exhaustion falls through). That also fixes two gaps for forward jumps, so a document that jumps over a write may gain
an `E-MAYBE-UNSET` it did not have in 0.50.

For play and tests: counters are local to one walk and rebuilt by replay, so the runtime snapshot needs nothing
new. A `<branch>` reached again in a later iteration is a new menu, and a scripted `choose:` list for it is
consumed one decision per presentation (`choose: { retry: [yes, yes, no] }` drives three iterations). A `<hub>`
inside a loop starts each visit at its first presentation, and a `once` choice taken on an earlier visit stays
spent. `lute trace` prints `<jump -> attempt (again 1/2)>` and `<jump -> attempt not taken (2/2 used)>`. The
reference runtime re-checks the loops at load and refuses an artifact whose backward transfers are not
well-nested bounded jumps. A `::label` inside a component is compiled once per `::use` instance, so each
instance loops on its own label (#207).

Use a loop for "the same stretch again, a bounded number of times". Use a `<hub>` to offer a menu again until the
player takes an `exit` choice.

## Scenes that a directive or a reward raises

A sheet row "play cutscene X" does not run the cutscene. The host does, by raising an occasion that the scene
answers. Lute does not raise occasions (that is a host duty), so before 0.51 the scene that answered it had no
edge from the row that caused it, and the scenario graph could not order them. Since 0.51 a directive or a
reward kind declares the occasions the host raises in answer to it, and an occasion can say that only declared
raises ever raise it:

```yaml
# plugins/acme.flow/occasions/occasions.yaml
occasions:
  cutscene: { select: first, target: { prefix: scene, entity: cutsceneId }, raisedBy: declared }
# plugins/acme.flow/directives/flow.yaml
directives:
  - name: playScene
    attrs:
      - { name: scene, required: true, type: { entity: cutsceneId } }
    effects:
      raises: [{ occasion: cutscene, target: "@scene" }]
# plugins/acme.flow/rewardkinds/rewards.yaml
rewardKinds:
  CUTSCENE: { target: { entity: cutsceneId, required: true }, effects: { raises: [{ occasion: cutscene, target: "@target" }] } }
```

```lute check-in-project="docs/examples/flow-and-text/scenes/chapter2.lute"
---
kind: scene
id: chapter2
on: cutscene
target: scene.chapter2
---
```

`::playScene{scene="chapter2"}` in a scene, or a `<reward kind="CUTSCENE" target="chapter2"/>` in a quest, now
raises `cutscene` for `scene.chapter2`.

- **`effects.raises`** is a non-empty list of `{ occasion, target? }`, beside `writes`/`asserts`/`retracts` on a
  directive and `asserts`/`retracts`/`accepts` on a reward kind. A reward kind may declare only `raises`. The
  `target` is `@<attr>` on a directive (a declared attribute), `@target` on a reward kind (the reward's `target=`), or
  a literal target written whole (`scene.chapter2`); it is absent only for an occasion declared without a target.
- **`raisedBy: declared`** on an occasion says the host raises it only where a declared `raises` says so. Without it
  a declared raise is recorded and checked but draws no graph edge.
- A shape fault is `E-PLUGIN-PARSE` at load. A directive that has a `lower:` compiles to a record that carries no
  effects, so its `raises` could never reach the host: `E-PLUGIN-INVALID-DIRECTIVE`. At assembly, `E-PLUGIN-RAISES`
  checks the occasion (with a did-you-mean) and the target's shape and type against it. The occasion may come from
  another plugin, which then belongs in `depends:`.
- In `check --project`, `E-RAISES-TARGET` checks each resolved target: a member of the entity kind, and answered by
  some scene or beat. The value is the call's attribute, else the attribute's `default:`; a target with no value
  raises nothing, and one that reads `occasion.target` is dynamic and not judged.
- `W-OCCASION-NEVER-RAISED` warns at a scene or beat that answers a `raisedBy: declared` occasion that no
  `effects.raises` in the project reaches. A `lute play` step that raises it is not a source, and an occasion the
  project clock raises is never warned about. Re-level it in `lute.project.yaml` `diagnostics:` like any `W-*` code.

Every analysis reads a declared raise as **possible, never guaranteed**: the host may refuse it at the occasion's
gate, so a raise adds no fact, write or must-claim in the answering scene. `lute scenario` and
`lute scenario reach` draw a `[raises]` edge from the raiser to every answering scene and beat, and a scene that
declares `after:` keeps `after:` as its route. (`lute impact` reads the semantic model graph and follows neither
`[raises]` nor `[accept]` edges; #210.) The [flow-and-text example](https://github.com/journeyWorker/lute/tree/main/docs/examples/flow-and-text):

```console
$ lute scenario --project docs/examples/flow-and-text
project root: docs/examples/flow-and-text
  topological layers:
    layer 0: scene(hall), scene(lock), quest(finale)
    layer 1: scene(chapter2)
  edges (prerequisite -> dependent) [atom kind(s)]:
    scene(lock) -> scene(chapter2) [raises]
    quest(finale) -> scene(chapter2) [raises]
```

The host performs the raise. After the presentation that ran the call ends, it sends each raise as a
`raiseOccasion` input once the runtime is idle, in record order. For a reward it does so after settling the grant.
The compiled `plugin` and `grant` records carry the resolved `raises`, and `lute play` prints
`  → raises cutscene scene.chapter2` on those lines. The reference player performs them for you, keeps the pending
ones in saves and rollback points, and a custom host does the same (see [Custom hosts](/player/custom-hosts/)).
`lute play` does not raise by itself: a script raises the occasion as its next step.

## Text in a directive that players read

A staged banner, a toast or a chapter title is text a player reads, so it must be translatable. Declare such an
attribute `type: text`:

```yaml
directives:
  - name: banner
    attrs:
      - { name: title,    required: true, type: text }
      - { name: subtitle, type: text }
      - { name: style,    type: { enum: [toast, card] } }
```

```lute
::banner{code="0010" title="Welcome to the hall, {{userName}}" style="card"}
```

- A `text` value is a quoted string in the choice-label grammar: `{{…}}` interpolations of state paths, `@def`s and
  host tokens, each checked as in a label. `\{{` writes a literal `{{` (since 0.52, #211; on 0.51 it was
  `E-STRING-ESCAPE`). An `@ref` value is `E-ATTR-TYPE`.
- `text` is admitted only on an attribute of a directive that stays a `plugin` record: no `lower:` and no
  `bridge:`. It cannot have a `default:`, and it is not a type for stamps, plugin options, payloads or reward-kind
  attributes. A directive with a `text` attribute cannot declare an attribute named `code`. Each fault is
  `E-PLUGIN-PARSE` at the attribute, and a scoped stamp named `code` on such a directive is
  `E-PLUGIN-RESERVED-STAMP-ATTR`.
- **`code=`** is the identity of such a directive, as on a line. The `lineId` of each text attribute is
  `{prefix}~{tag}~{code}~{attr}`: `hall~banner~0010~title`. `~` is reserved for these ids, so a `code` that contains it
  is `E-LINE-CODE-RESERVED` and a `lineId` template that contains it is `E-IDENTITY-TEMPLATE`. Two `::banner` calls
  in one scope with the same `code` are `E-DUP-LINE-CODE`, and two units anywhere in the project with one final
  `lineId` are `E-LINE-ID-DUP`. `code=` on a directive without a `text` attribute is `E-UNKNOWN-ATTR`.
- **`lute tag`** writes `code="…"` into every untagged directive that has a `text` attribute, in the order the
  compiler back-fills; a component is tagged under the snapshots of its users, and users that disagree are
  `E-COMPONENT-TEXT-DIRECTIVE`. With `identity.requireStable`, `W-LINE-CODE-UNTAGGED` reports one without a code.

The translation round trip is the one lines already use, and a text attribute is one more row kind:

```sh
lute tag scenes
lute loc export --project . -o export/en.json
# copy it to export/ko.json (the file stem is the locale) and translate the "text" values,
# keeping every {{…}} marker
lute loc import export/ko.json -o loc/ko.json          # the locale bundle
lute compile scenes/hall.lute --project . --locale-bundle loc/ko.json
```

An exported row is `{ "kind": "directive", "lineId": "hall~banner~0010~title", "key": "banner.title", … }`. Its
`lineId` is `null` until the directive has a `code`, so run `lute tag` first. `lute loc report` counts directive
texts (`directiveTexts`) with the prompts. In the compiled `plugin` record, `fields.title` keeps the source text,
markers as authored, and `localized.title` carries `{ lineId, placeholders, texts }` with the translations merged in. A missing
locale is `W-L10N-MISSING` and a changed source is `W-L10N-STALE`, as for a line. The runtime renders the markers
(a host token stays verbatim), and the reference player's `PluginCall.localized` gives a host handler each text in the
player's locale.

## Structured values are not supported yet

A column that holds a list or a record (a tag list, a voice-asset map keyed by language) does not fit a directive
attribute. `list`, `map` and `record` attribute and stamp types stay refused with `E-PLUGIN-INVALID-DIRECTIVE`,
because no `.lute` value can satisfy them. Structured authoring (#176) is **deferred, not covered**: there is no
spelling in 0.51 for generic enum-list metadata such as `::tagged{tags=…}`, and none for a voice-asset map keyed by
language. One voice key per line, resolved without a locale, is not a per-language asset map.

Where a scalar decomposition fits, use it:

| The row has | Write |
|---|---|
| a few named fields | one attribute per field; scope a stamp to the surface with `on:` |
| text a player reads, in several languages | one `text` attribute |
| a list that really is a set of options | `<choice>` elements |

The refusal message says the same: give each field its own attribute, use a `text` attribute for player-visible
text, or write a list of options as `<choice>` elements. For a stamp the `text` clause is omitted, since a stamp cannot be `text`.

## What the tool catches, and what it does not

| Problem in the port | Caught by |
|---|---|
| A column written on a row it does not belong to | `E-UNKNOWN-ATTR`, naming the surfaces of the stamp |
| A row missing a required column | `E-MISSING-ATTR` for a scoped `required: true` stamp |
| A stamp declared required on every record | `E-PLUGIN-PARSE` at load |
| A stamp named after an attribute its surface already has | `E-PLUGIN-RESERVED-STAMP-ATTR` |
| A misspelled surface in `on:` | `E-PLUGIN-STAMP-SURFACE`, with a did-you-mean |
| Two declarations of one column on overlapping surfaces | `E-PLUGIN-DUP-ID`, `E-PLUGIN-DUP-ACROSS` |
| `{{playerName}}` the plugin does not declare | `E-CEL-PROFILE`, naming the declared tokens |
| A number hint on a text token, or the reverse | `E-REF-TYPE` |
| A badge fact no quest can see | gone: `E-QUEST-UNREACHABLE` reads the reward effect |
| A reward that accepts a quest that does not exist | `E-ACCEPT-TARGET`, with a did-you-mean |
| A warning flood in an imported subtree | `diagnostics:` with `paths:`, counted in the summary |
| A speaker shown off stage | `W-STAGE-ABSENT`, unless the importer emits the staging row |
| A structural or dependent-value rule | a `node` lint |
| A loop the checker could not see | `::jump{max}` with `E-JUMP-MAX`, `E-JUMP-LOOP-SCOPE`, `E-JUMP-INTO-LOOP`, `E-JUMP-LOOP-CROSS` |
| A scene that nothing raises | `W-OCCASION-NEVER-RAISED`; a mistyped raise target is `E-RAISES-TARGET` |
| A raise of an occasion no plugin declares | `E-PLUGIN-RAISES`, with a did-you-mean |
| Player-visible directive text that cannot be translated | `type: text`, `lute tag` and `lute loc` |
| Two translation units with one id | `E-LINE-ID-DUP`; `~` in a `code` is `E-LINE-CODE-RESERVED` |
| An opener that is never closed | a `pair` lint |
| A list or record column | refused; use scalar decompositions |

What Lute does **not** know:

- **Whether a stamp's value means anything to your engine.** `voiceAssetId="VO.mira.0010"` is a string
  (or a `providerRef`, when you pin a catalog). The engine's asset table is yours.
- **What your engine does with a token.** A host token is a promise that the host substitutes it. If the
  host forgets, the marker stays in the text.
- **Whether a quiet subtree is correct.** `off` hides a warning; it does not fix the row. Keep the
  `paths:` globs narrow, and delete the entry when the review is done.
- **Whether the host performs a declared raise.** A `raises` entry is a promise about the host, never a guarantee
  that the scene plays: the occasion's gate may refuse it.
- **Whether a translation is right.** `W-L10N-STALE` says the source changed after the export; it does not read the
  translation.
