---
title: State schemas
description: The shared, imported source of truth for run/user/app state, defs, entities, the cast, the clock, seasons and the end of the game — the schema document shape, and how uses composes peers while extends refines a base layer.
---

The `run` / `user` / `app` tiers are game/season-global: one persisted value cannot carry per-scene types. So they live in a single **schema document**, the source of truth every scene imports with `uses:`. Scenes then declare only their own `scene.*` locals. Since 0.2.2 a declaration file is plain `.yaml` (no `---`/Lute envelope) — a pure declaration, not a scene.

## Schema shape

A schema declares `state:` (scalar tiers), `defs:` (named typed-CEL macros), `enums:` (declared
member lists), and — when the relational layer is used — `entities:` / `relations:` / `facts:` /
`rules:`. It may also declare the project's `cast:` (dsl 0.23.0), since `0.24.0` one `clock:`, and
since `0.27.0` its `seasons:` and its [`terminal:`](#the-end-of-the-game-terminal) condition.

```yaml
state:
  run.choseHelp: { type: bool, default: false }
  run.day:       { type: int, default: 1, owner: engine }
  user.level:    { type: int, default: 1 }
defs:
  helped: { type: bool, cel: "run.choseHelp" }
```

Each `<path>` segment is a name (letters, digits, `_` or `-`, not starting with `-`); a condition writes one that is not an identifier quoted, `run.visits["lab-b2"]`. A declaration is `{ type, default?, owner?, per? }`. `type: { domain: <name> }` takes its members from a named enum or from an entity kind, the way to type an engine-kept position over a map kind — see [Paths typed by a named enum](/state/state-model/#paths-typed-by-a-named-enum). A `default` is materialized into the tier's initial state at schema load **and** re-materialized whenever the engine fires that tier's reset — so a defaulted path is always assigned, and the checker and engine read the one snapshot. `owner: engine` (0.22.0) marks a path content may read but never `::set` (`E-ENGINE-OWNED-WRITE`); `engine` is the only value it takes — see [`owner: engine`](/state/state-model/#owner-engine). `per: <kind>` (0.24.0) declares one path per member of a closed entity kind — see [One path per entity](/state/state-model/#one-path-per-entity-per).

An `enums:` block does double duty. Its domains are argument types for the
[relational layer](/state/facts-and-datalog/), and since language `0.9.0` they are also how a project
declares the **content vocabulary** — `emotion`, `action`, `anchor`, `mood`, `volume`, `musicAction`,
`vfxType`. The compiler ships no members for those slots, so a schema reached through `uses:` is the
route a multi-document project should prefer for declaring them; `action` and `anchor` additionally
carry required member semantics. That is a distinct concern from the scalar tiers this page is about
— see [Content vocabulary](/language/vocabulary/).

### Keys added in 0.24.0

A schema for a party game with a day clock uses most of them at once:

```yaml
state:
  run.day:      { type: int, default: 1, owner: engine }
  run.slot:     { type: { domain: slot }, default: dawn, owner: engine }
  run.approval: { type: int, default: 0, per: companion }
enums:
  slot:
    members: [dawn, noon, dusk]
    labels: { dawn: Dawn, noon: Midday, dusk: Dusk }
  emotion: [calm, fierce, tired]
entities:
  person:    { members: [isolde, corvin, hollis] }
  companion: { subsetOf: person, members: [isolde, corvin] }
relations:
  inParty: { args: [companion], tier: run }
clock:
  day: run.day
  slot: run.slot
  slots: [dawn, noon, dusk]
cast:
  isolde: { name: Isolde, present: "holds('inParty', ['isolde'])", emotions: [calm, fierce] }
  corvin: { name: "Corvin Hale", present: "holds('inParty', ['corvin'])" }
  hollis: { name: Hollis }
```

- **`clock:`** names the `owner: engine` path that holds the day and, for a clock with slots, the path that holds the slot and the slots in order (a clock may also count whole days only). A schema declares at most one, and a malformed clock is `E-CLOCK-DECL`. It adds the read-only `clock.*` paths and `once: day` / `once: slot` beats, and since 0.27.0 `once: week` when it declares a `week:`. See [The clock](/language/clock/).
- **`labels:`** on a long-form enum gives members display text: `{{run.slot}}` renders `Dawn`, while conditions still compare `run.slot == 'dawn'`. A label for a non-member is `E-ENUM-LABEL-NOT-MEMBER`, and a non-string label is `E-META-VALUE`.
- **`subsetOf:`** on an entity kind declares a sub-kind whose members all belong to the parent. See [Sub-kinds](/state/facts-and-datalog/#sub-kinds-subsetof).
- **`per:`** on a state path declares `run.approval.isolde` and `run.approval.corvin`. Its `default:` may be one value for every member or a map with per-member values and a `_` fallback, `{ _: 0, isolde: 2 }`. See [One path per entity](/state/state-model/#one-path-per-entity-per).
- **`present:`, `emotions:` and `assume:`** on a cast entry say when the character is with the player, which `emotion=` values their lines may use, and whether presence may take the engine's `reserved:` facts as absent. In a schema, a `present:` that does not parse is `E-CEL-PARSE`, one outside the CEL profile is `E-CEL-PROFILE`, and an `emotions:` member outside the schema's `emotion` enum is `E-BAD-ENUM`, each reported at the entry's key. See [The cast](/language/dialogue-and-cast/#the-cast).

### Seasons

A live game runs events that come and go: a harvest festival every autumn, a starfall week the
studio switches on when it likes. Each window starts fresh and may look back at how the last one
ended. A schema declares such windows as **seasons** (dsl 0.27.0 §5), each with the condition that
says when it is live:

```yaml
state:
  user.month:             { type: int, default: 1, owner: engine }
  user.starfallOn:        { type: bool, default: false, owner: engine }
  season.harvest.bushels: { type: int, default: 0 }
  season.starfall.wishes: { type: int, default: 0 }
defs:
  harvestLive:  { type: bool, cel: "user.month >= 9 && user.month <= 10" }
  starfallLive: { type: bool, cel: "user.starfallOn" }
seasons:
  harvest:  { live: "@harvestLive" }
  starfall: { live: "@starfallLive" }
```

Each season is a named state tier:

- **`season.<name>.<field>`** paths are declared under `state:` like `run.*` and `user.*`, with
  defaults, and content reads and writes them (`::set{season.harvest.bushels += 1}`).
- **`prev.season.<name>.<field>`** is a read-only mirror: the values the season's previous window
  ended with. It is unset until the season has opened a second time, so guard a read with `has`,
  as for [`prev.run.*`](/state/state-model/#the-previous-run). Writing it is `E-QUEST-RESERVED-WRITE`, as for `prev.run.*`.
- **`once: season:<name>`** on a scene beat (`once="season:<name>"` on an entry or bundle beat)
  spends the beat until the season opens again. See [Beats](/language/beats/#scene-beat-keys).
- **`<quest tier="season:<name>">`** returns the quest to `unset` when the season opens again. See
  [Quests that come back](/language/quests-and-scenes/#quests-that-come-back-season-tiers-and-rearm).

A season **opens** when its `live` condition turns from false to true. At that moment the engine
copies the current `season.<name>.*` values into `prev.season.<name>.*`, resets `season.<name>.*`
to their declared defaults, clears the season's `once` spends, and returns its season-tier quests to
`unset`. Nothing resets when it closes. Seasons are independent and overlap freely: harvest and
starfall may both be live at once. `lute play` prints each flip, the opening with what it reset
and the values the last window ended with:

```
season harvest opens — season.harvest.* reset to defaults; last window: prev.season.harvest.tokens = 2
season harvest closes
```

```lute
@narrator{when="has(prev.season.harvest.bushels)"}: Last harvest you brought in {{prev.season.harvest.bushels}} bushels.
::set{season.harvest.bushels += 1}
```

The compiled artifact and `project.index.json` carry the declarations as
`seasons: [{ name, live: {cel, expr, authored?} }]`, with the `live` condition after `@def` expansion, so an
engine opens and resets each season from the same source the checker read.

Everything wrong with a season is `E-SEASON-DECL`: a `seasons:` entry that is not a map, has a
missing or empty `live`, an unknown key, or a bad name; two schemas that declare one season
differently; and a `season.<name>.*` path, a `once: season:<name>`, or a `tier="season:<name>"`
that names an undeclared season.

### The end of the game: `terminal:`

Some games end: the ward takes the player, or the run's fate is sealed. A schema may say when with
**`terminal:`**, a CEL condition. Once it holds the game is over, and the engine raises no
occasion, a title screen or a menu between runs included, unless the occasion declares
[`outsideRun: true`](/plugins/manifests/):

```yaml
state:
  run.fate:   { type: { domain: fate }, default: open }
  run.sanity: { type: int, default: 10 }
enums:
  fate: [open, escaped, taken]
terminal: "run.fate == 'taken' || run.sanity <= 0"
```

Only a schema document declares `terminal:`, never `lute.project.yaml`. `defaults.uses` already
makes one schema project-wide, so a manifest key would be a second place to look for the same
thing. Several schemas may each declare one; the project's condition is their `||`, and the game
is over when any of them holds. The frontmatter JSON schema (`schemas/lute.schema.json`) accepts
the key in both forms: the condition string, or the long form `{ when, persists }` below.

The checker reads the condition like any other:

- Its text is checked as a condition slot. An error in it is prefixed `` `terminal: …`: ``, and an
  error in an imported schema's `terminal:` is reported at that schema's line.
- A value that is not a string or a `{ when, persists }` mapping, or names no condition, is
  `E-META-VALUE`: `terminal:` must name when the game is over.
- Every beat is judged under `!terminal`, since no occasion is raised once it holds. A beat whose
  `when` is provably false while the game is not yet over, one that needs `run.fate == 'taken'`,
  is `E-BEAT-UNREACHABLE`. The same seam gates occasions one at a time, through an occasion's
  [`raisedWhen`](/plugins/manifests/#occasion-gates-raisedwhen).

[`lute play`](/tooling/play/) follows the engine. The step after which `terminal:` holds gets a
note that the game is over, and a playthrough that ends there reports `end: terminal`
(`"end": "terminal"` at the root of `--json`). A later `occasion:` or `advance:` step is
`E-OCCASION-GATE` (exit 1). A `newRun: true` step starts a new run, and play goes on only when the
new run makes the condition false: a `terminal:` over run state is reset by it, one that reads what
a new run keeps (`user.*`, `visited(…)`) is not, and `check-project` warns `W-TERMINAL-PERSISTENT`
unless the long form says `persists: true`. With `persists: true` the notes and the refusal say the
game is over for good and never suggest `newRun`.

The condition is judged when an occasion is raised, so the raise that makes it hold plays out: the
beat whose write ends the game finishes, the rest of a `select: sequence` raise plays, and the
`questComplete` / `questFailed` handlers the raise settles run last. An epilogue goes after that
write, or in the `<on event="questComplete">` handler of the quest `terminal:` reads. `::end` in
content never ends the game; see [The game is over](/tooling/play/#the-game-is-over) for every
meaning of "end".

The compiled artifact and `project.index.json` carry the condition at the top level as
`terminal: { cel, expr, authored? }`, after `@def` expansion and joined with `||` when several schemas
declare one, so an engine stops raising occasions from the same source the checker read. Beside
it, `terminalPersists: true` marks an ending that persists (omitted otherwise): an engine that
offers a new run after the game is over does not offer one then.

#### An ending that outlives runs: `persists: true`

A new run normally reopens the game: a `terminal:` over run state is reset by it. An ending meant
to outlive every run, such as a roguelike's permanent ending, reads state a new run keeps, and says
so with the long form:

```yaml
state:
  user.crowned: { type: bool, default: false }
terminal: { when: "user.crowned", persists: true }
```

`when` is the condition, exactly as the short form writes it; `persists` is `true` or `false`
(default `false`, the same as the short form). The long form takes no other key: a slip such as
`persist:` is `E-META-VALUE` and names the key it meant. `persists: true` states the design, so
`W-TERMINAL-PERSISTENT` stays silent, and `lute play` says the game is over for good instead of
offering a new run. On a condition that reads only state a new run forgets (`run.*`, the clock,
run-tier quests and relations) `persists: true` is `E-META-VALUE`: that ending cannot outlive the
run. With several schemas, the project's ending persists only when every declaration says so.

### Defs nothing uses

`check-project` reports `W-DEF-UNUSED` (dsl 0.24.0 §7) for a declared `@def` that no content, no other def, and no rule guard references. It is reported once per project, at the declaration: the schema file's line, or the document's own `defs:` key. Play scripts and tests are not uses. The relational counterpart is `W-RELATION-UNREAD` (see [Relations nothing reads](/state/facts-and-datalog/#relations-nothing-reads)).

<!-- lute-diagnostics -->
```
./world.schema.yaml:5:3: warning [W-DEF-UNUSED] def `stale` is declared but no `@stale` reference uses it anywhere in the project (content, other defs, or rule guards); use it or drop it
```

## Composition: `uses` and `extends`

Imports form a **DAG**: cycles are a static error (the diagnostic prints the chain), schemas are loaded and checked before any scene, duplicate `defs` names across imports are an error (no silent shadow), and two paths to one file resolve to one identity.

`uses:` unions **peer** schemas — a name declared by two peers is a conflict. `extends:` names one or more **base** schemas this document refines: a base is a lower-precedence layer, so if the extending document (or its peers) redeclares a base name, the extending declaration **overrides** it — no duplicate error.

```yaml
# base.schema.yaml
state:
  run.blessed: { type: bool, default: false }
```

```yaml
# child.schema.yaml — refines the base
extends: base.schema.yaml
state:
  run.blessed: { type: bool, default: true }   # default-only override
```

Precedence runs low → high: a document's `extends` bases (recursively) < its `uses` peers < its own inline `state:`/`defs:`. Because persisted state must keep a stable type, an override that changes a path's declared `type` is `E-EXTENDS-STATE-TYPE`; a `default`-only refinement of the same type is allowed silently. `extends` edges reuse the same cycle / missing-file / parse diagnostics (`E-USES-{CYCLE,NOT-FOUND,PARSE}`) as `uses:`.

### Kinds assembled across files: `add:`

When several authors share one project, each area wants to add its own people and places to a kind
the lead owns. Since dsl 0.26.0 §2.3 a schema may **add** members to a kind that exactly one of the
document's imports declares:

```yaml
# schema/roster.schema.yaml (the lead's)
entities:
  person:  { members: [professorOak, mom] }
  trainer: { subsetOf: person, members: [rival] }
```

```yaml unverified="an add: needs the base schema above beside it; both checked together with lute check-project"
# schema/areas/south.schema.yaml (one area's)
entities:
  person:  { add: [grannyWren, oldSalt] }
  trainer: { add: [youngsterTodd, lassMina] }
```

A document importing both sees `person` with every member, and the added trainers are persons too
(a [sub-kind's](/state/facts-and-datalog/#sub-kinds-subsetof) members belong to its parent). Each of
these is `E-ENTITY-KIND-SHAPE`, naming both files:

- an `add:` for a kind no import declares, with a did-you-mean (`persn` suggests `person`);
- an `add:` onto an `open:` kind, whose members the engine registers;
- `add:` beside `members:` or `open:` in one declaration;
- a member listed twice: by two `add:` lists, re-added over the declaration, or twice in one
  kind's or enum's own list (dsl 0.26.0 §2.2), reported at the second one's line with both lines.

<!-- lute-diagnostics -->
```
schema/areas/south.schema.yaml:3:35: error [E-ENTITY-KIND-SHAPE] entity kind `trainer` lists `lassMina` twice — in the `add:` of `schema/areas/east.schema.yaml` (line 7) and in the `add:` of `schema/areas/south.schema.yaml` (line 3); list each member once
```

### Declarations across documents

A path declared in two documents' frontmatter `state:` is one runtime value, so since dsl 0.26.0
§2.1 `check-project` compares the declarations and reports `E-STATE-DECL-CONFLICT` when their
`type`, `default`, `per` or `owner` differ (see [Declaration](/state/state-model/#declaration)).

Schema errors that every importer would repeat — sub-kind and `add:` `E-ENTITY-KIND-SHAPE`,
`E-USES-DUP-STATE`, `E-USES-DUP-DEF`, `E-USES-DUP-RELATION`, a peer `E-KIND-NAME-CLASH`, and
`E-DEF-DECL` for an imported def — are reported by `check-project` once, as a project-wide line at
the schema line ending `(imported by N documents)` (dsl 0.26.0 §2.7), rather than at every importing
document; the importers stay `ok`.

The state schema is *game content* — separate from the engine **capability manifest** (engine vocabulary), which has its own owner and change cadence.
