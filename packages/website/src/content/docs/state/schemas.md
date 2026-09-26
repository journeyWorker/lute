---
title: State schemas
description: The shared, imported source of truth for run/user/app state, defs, entities, the cast and the clock — the schema document shape, and how uses composes peers while extends refines a base layer.
---

The `run` / `user` / `app` tiers are game/season-global: one persisted value cannot carry per-scene types. So they live in a single **schema document**, the source of truth every scene imports with `uses:`. Scenes then declare only their own `scene.*` locals. Since 0.2.2 a declaration file is plain `.yaml` (no `---`/Lute envelope) — a pure declaration, not a scene.

## Schema shape

A schema declares `state:` (scalar tiers), `defs:` (named typed-CEL macros), `enums:` (declared
member lists), and — when the relational layer is used — `entities:` / `relations:` / `facts:` /
`rules:`. It may also declare the project's `cast:` (dsl 0.23.0) and, since `0.24.0`, one `clock:`.

```yaml
state:
  run.choseHelp: { type: bool, default: false }
  run.day:       { type: number, default: 1, owner: engine }
  user.level:    { type: number, default: 1 }
defs:
  helped: { type: bool, cel: "run.choseHelp" }
```

Each `<path>` segment is a CEL-facing identifier (no `-`). A declaration is `{ type, default?, owner?, per? }`. A `default` is materialized into the tier's initial state at schema load **and** re-materialized whenever the engine fires that tier's reset — so a defaulted path is always assigned, and the checker and engine read the one snapshot. `owner: engine` (0.22.0) marks a path content may read but never `::set` (`E-ENGINE-OWNED-WRITE`); `engine` is the only value it takes — see [`owner: engine`](/state/state-model/#owner-engine). `per: <kind>` (0.24.0) declares one path per member of a closed entity kind — see [One path per entity](/state/state-model/#one-path-per-entity-per).

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
  run.day:      { type: number, default: 1, owner: engine }
  run.slot:     { type: { domain: slot }, default: dawn, owner: engine }
  run.approval: { type: number, default: 0, per: companion }
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
  isolde: { name: Isolde, present: "holds(inParty(isolde))", emotions: [calm, fierce] }
  corvin: { name: "Corvin Hale", present: "holds(inParty(corvin))" }
  hollis: { name: Hollis }
```

- **`clock:`** names the `owner: engine` path that holds the day and, for a clock with slots, the path that holds the slot and the slots in order (a clock may also count whole days only). A schema declares at most one, and a malformed clock is `E-CLOCK-DECL`. It adds the read-only `clock.*` paths and `once: day` / `once: slot` beats. See [The clock](/language/clock/).
- **`labels:`** on a long-form enum gives members display text: `{{run.slot}}` renders `Dawn`, while conditions still compare `run.slot == 'dawn'`. A label for a non-member is `E-ENUM-LABEL-NOT-MEMBER`, and a non-string label is `E-META-VALUE`.
- **`subsetOf:`** on an entity kind declares a sub-kind whose members all belong to the parent. See [Sub-kinds](/state/facts-and-datalog/#sub-kinds-subsetof).
- **`per:`** on a state path declares `run.approval.isolde` and `run.approval.corvin`. Its `default:` may be one value for every member or a map with per-member values and a `_` fallback, `{ _: 0, isolde: 2 }`. See [One path per entity](/state/state-model/#one-path-per-entity-per).
- **`present:`, `emotions:` and `assume:`** on a cast entry say when the character is with the player, which `emotion=` values their lines may use, and whether presence may take the engine's `reserved:` facts as absent. In a schema, a `present:` that does not parse is `E-CEL-PARSE`, one outside the CEL profile is `E-CEL-PROFILE`, and an `emotions:` member outside the schema's `emotion` enum is `E-BAD-ENUM`, each reported at the entry's key. See [The cast](/language/dialogue-and-cast/#the-cast).

### Defs nothing uses

`check-project` reports `W-DEF-UNUSED` (dsl 0.24.0 §7) for a declared `@def` that no content, no other def, and no rule guard references. It is reported once per project, at the declaration: the schema file's line, or the document's own `defs:` key. Play scripts and tests are not uses. The relational counterpart is `W-RELATION-UNREAD` (see [Relations nothing reads](/state/facts-and-datalog/#relations-nothing-reads)).

<!-- lute-diagnostics -->
```
./world.schema.yaml:5:3: warning [W-DEF-UNUSED] def `stale` is declared but no `@stale` reference uses it anywhere in the project (content, other defs, or rule guards); use it or drop it (dsl 0.24.0)
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
schema/areas/south.schema.yaml:3:35: error [E-ENTITY-KIND-SHAPE] entity kind `trainer` lists `lassMina` twice — in the `add:` of `schema/areas/east.schema.yaml` (line 7) and in the `add:` of `schema/areas/south.schema.yaml` (line 3); list each member once (dsl 0.26.0 §2.2)
```

### Declarations across documents

A path declared in two documents' frontmatter `state:` is one runtime value, so since dsl 0.26.0
§2.1 `check-project` compares the declarations and reports `E-STATE-DECL-CONFLICT` when their
`type`, `default`, `per` or `owner` differ (see [Declaration](/state/state-model/#declaration)).

Schema errors that every importer would repeat — sub-kind and `add:` `E-ENTITY-KIND-SHAPE`,
`E-USES-DUP-STATE`, `E-USES-DUP-DEF`, `E-USES-DUP-RELATION`, a peer `E-KIND-NAME-CLASH`, and
`E-DEF-DECL` for an imported def — are reported once at the schema line (dsl 0.26.0 §2.7), with
the other importers folded into `(+N more callers)`, rather than at every importing document.

The state schema is *game content* — separate from the engine **capability manifest** (engine vocabulary), which has its own owner and change cadence.
