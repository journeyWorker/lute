---
title: The state model
description: Lute's tiered scalar state — the run, user, and app lifetime namespaces (plus episode-local scene), how paths are declared (including enum-typed and per-entity paths), the path-sensitive definite-assignment rules that govern reads and writes, the paths only the engine writes, and prev.run, the previous run's final values.
---

Lute scalar state is a set of typed paths (`number`, `bool`, `string`, `enum`) grouped into **namespaces named by their reset boundary** — the moment the engine clears them. There are four tiers on one axis (*when does it reset?*):

| Namespace | Reset boundary | Typical use |
|---|---|---|
| `scene.*` | episode end (one `.lute` document; survives across its shots) | on-stage state, `scene.choices.*`, `scene.visited.*` |
| `run.*` | new run — one attempt, a sequence of episodes | per-attempt flags, affect, cross-episode carry within an attempt |
| `user.*` | profile/account wipe — survives runs | level, unlocks, meta-progression |
| `app.*` | app uninstall — identity-independent | language, age rating, settings |

The engine **owns and fires every reset**; the language never triggers one. The three persistent tiers — `run` / `user` / `app` — are game/season-global, so they live in a single shared schema document that scenes import with `uses:` (see [State schemas](/state/schemas/)). Only genuinely episode-local `scene.*` declarations may appear inline in a scene, and a scene MUST NOT redeclare or override an imported tier.

## Declaration

Every path read *or written* MUST be declared with a `type` and an optional `default`. There are no bare, un-namespaced state names.

```yaml
state:
  scene.affect.elena: { type: number, default: 0 }
  run.choseHelp:      { type: bool,   default: false }
  user.level:         { type: number, default: 1 }
  app.rating:         { type: { enum: [teen, adult] }, default: teen }
```

Author `state:` is **scalar-only** — `number`, `bool`, `string`, or `enum`, and nothing else. An `enum` path nests its members inside `type:` — `{ type: { enum: [teen, adult] } }`. A sibling `values:` key is not a declaration key at all: `{ type: enum, values: [teen, adult] }` is `E-STATE-DECL` on the schema, and a document that `uses:` that schema fails with `E-USES-PARSE` carrying the `E-STATE-DECL` beneath it. [`docs/examples/showcase/schema/base.schema.yaml:9`](https://github.com/journeyWorker/lute/blob/main/docs/examples/showcase/schema/base.schema.yaml) is the same declaration, written correctly. A declaration whose `type` is `list`, `record`, or `map` is `E-STATE-COLLECTION`, and the declaration is *not installed*: a later read of that path reports a plain `E-UNDECLARED` rather than resolving against a phantom collection-typed slot.

Enforcement is new in 0.8.0, but it removes an ambiguity rather than adding a restriction. The normative text always said scalar, but the shape validator accepted the whole type union and the runtime documentation described `list<…>` / `map<…>` / `record` as valid entry types — three sources, three answers. All three now agree. Collections were always meant to be modelled **relationally**: an inventory is `ownsItem(item)`, not a `list<string>`, so reach for [`relations:`](/state/facts-and-datalog/) instead. Collection-shaped entry types do still reach the compiled artifact, but only through a plugin `state_shapes` expansion — never from an author's `state:` block.

### One path, one declaration

A frontmatter `state:` declaration is not private to its document: `run.southFossil` declared in
two documents is one value at run time. Since dsl 0.26.0 §2.1 `check-project` compares every
declaration of a path, inline or imported, and reports **`E-STATE-DECL-CONFLICT`** naming both
files and lines when their `type`, `default`, `per` or `owner` differ. `lute play` refuses a
project that declares one path with two types.

<!-- lute-diagnostics -->
```
./scenes/b.lute:5:3: error [E-STATE-DECL-CONFLICT] state path `run.southFossil` is declared as bool, default false at `./scenes/a.lute:5` but as enum(none|dome|spiral), default "none" at `./scenes/b.lute:5`; every declaration of one path must agree on type, default, per and owner — they share one runtime value (declare it once in a schema both documents import) (dsl 0.26.0 §2.1)
```

`scene.*` paths are scene-local and exempt, and a declaration that refines one it
[`extends:`](/state/schemas/#composition-uses-and-extends) (an overridden default) is no conflict.
To share a path, declare it once in a schema both documents import.

### Paths typed by a named enum

A path may take its type from a named enum instead of listing members inline: `{ type: { domain: weekday } }` reads its members from the `weekday` enum of the schema, and it counts as a read of that enum, so the enum draws no `W-DOMAIN-UNREAD`. Since `0.24.0` a long-form enum may give its members display **labels**, and interpolating a path typed against it renders the label rather than the member id:

```yaml
state:
  run.weekday: { type: { domain: weekday }, default: sun }
enums:
  weekday:
    members: [sun, mon, tue]
    labels: { sun: Sunday, mon: Monday }
```

`@narrator: Today is {{run.weekday}}.` renders `Today is Sunday.`. A member without a label renders its id (`tue`). Conditions still compare ids: `run.weekday == 'sun'`. A label for something that is not a member is `E-ENUM-LABEL-NOT-MEMBER`, and a label that is not a string is `E-META-VALUE`. A declared [clock](/language/clock/) uses the same mechanism for its weekday names.

### One path per entity: `per:`

A number kept for each member of a group, such as a companion's approval, is one declaration with **`per:`** (dsl 0.24.0 §3) rather than one line per member:

```yaml
state:
  run.approval: { type: number, default: 0, per: companion }
entities:
  person:    { members: [isolde, corvin, hollis] }
  companion: { subsetOf: person, members: [isolde, corvin] }
```

This declares `run.approval.isolde` and `run.approval.corvin`, each `{ type: number, default: 0 }`, and the compiled state table carries one entry per member. Content addresses a member by name: `::set{run.approval.isolde += 1}`, `when="run.approval.corvin >= 3"`, `{{run.approval.isolde}}`. The family itself is not a path, so `when="run.approval > 1"` is `E-UNDECLARED`, and the message says the path is entity-indexed and asks for a member. A Datalog rule is the one place that reads a member through a variable, `cel("run.approval[P] >= 3")` (see [Facts and Datalog](/state/facts-and-datalog/#entity-indexed-state-in-a-rule-guard)).

`per:` names a **closed** entity kind, one with `members:`, declared in the same document as the path. A kind declared `open:`, a kind the document does not declare, or a malformed one is `E-STATE-DECL`, because the checker cannot list the paths it would declare. The kind may be a [sub-kind](/state/facts-and-datalog/#sub-kinds-subsetof). Indexing a path by a kind counts as reading the kind, so it draws no `W-DOMAIN-UNREAD`.

Members may start from different values. A map `default:` gives each member its own, with `_` as the fallback for the members it does not name:

```yaml
state:
  run.approval: { type: number, default: { _: 0, isolde: 2 }, per: companion }
entities:
  companion: { members: [isolde, corvin] }
```

`run.approval.isolde` starts at 2 and `run.approval.corvin` at 0. The compiled state table carries each member's own default, and `check`, `trace`, `run` and `play` all read it. The map is checked strictly, and each of these is `E-STATE-DECL`: a key that is not a member, a value that is not a scalar of the path's type, a member with neither its own value nor a `_`, a map `default:` on a path without `per:`, and a list `default:`.

## Reads and writes

`::set{path <op> celExpr}` writes one path per directive (`=`, `+=`, `-=`, `*=`). Writes target `scene.*` / `run.*` / `user.*`; `app.*` is **content-read-only** (the settings layer owns it — `::set{app.*}` is a static error), and so is every path the engine writes ([below](#paths-the-engine-writes)).

Definite assignment is **path-sensitive**. Reading an undeclared path is `E-UNDECLARED`. A `scene.*` read follows ordinary flow analysis. A `run`/`user`/`app` path is **maybe-unset at scene entry** unless it carries a schema `default`; after entry, a dominating `::set{p = …}` write or a guard (`has(p)` / `isSet(p)`) proves it — otherwise the read is `E-MAYBE-UNSET`. A compound assignment (`+=`/`-=`/`*=`) reads the old value first, so only `=` may be a path's first write. A defaulted path is always assigned; the checker and engine share the one schema snapshot, so they can never disagree.

A write may carry its own guard: `::set{run.best = run.floor when="run.floor > 3"}` (dsl 0.24.0 §1) writes only when the condition holds, like a one-arm `<match>` around it. Because the write may not happen, it is **never** a definite assignment. A later read of an undefaulted path it writes is still maybe-unset:

```lute expect="E-MAYBE-UNSET"
---
kind: scene
id: tower.landing
state:
  run.floor: { type: number, default: 1 }
  run.best: { type: number }
---

## The Landing

::set{run.best = run.floor when="run.floor > 3"}
@narrator: Your best is floor {{run.best}}.
```

Give `run.best` a default, or guard the read with `isSet(run.best)`. The `when=` is checked like a line's: a guard that can never hold is `E-ARM-DEAD`, and `when=` is the only attribute a `::set` takes, since everything else after the operator is the expression.

## Paths the engine writes

Some paths belong to the engine. Content reads them anywhere it reads state — a guard, a `<match on>` subject, an interpolation — and never writes them:

| Path | Written by | A `::set` of it |
|---|---|---|
| `app.*` | the settings layer | `E-APP-READONLY` |
| `quest.<id>.state`, `quest.<id>.activatedAt`, `quest.<id>.objectives.<o>.done`, and since 0.24.0 `quest.<id>.failedBy`, `quest.<id>.objectives.<o>.failed` | the quest lifecycle (see [Quests & scenes](/language/quests-and-scenes/)) | `E-QUEST-RESERVED-WRITE` |
| `entry.<id>.read` | the engine, on a lore entry's first presentation in a run — **run** tier | `E-QUEST-RESERVED-WRITE` |
| `entry.<id>.everRead` | the engine, on a lore entry's first presentation ever — **user** tier | `E-QUEST-RESERVED-WRITE` |
| `prev.run.<path>` | the engine, when a run ends: the value `run.<path>` had then (see [below](#the-previous-run)) | `E-QUEST-RESERVED-WRITE` |
| `clock.index`, `clock.weekday`, `clock.weekdayLabel` | derived from the day and slot of the schema's declared [clock](/language/clock/) (0.24.0) | `E-QUEST-RESERVED-WRITE` |
| a path your schema declares `owner: engine` | the engine | `E-ENGINE-OWNED-WRITE` |

The quest and entry paths are reserved by name: every document may read them without declaring them, and declaring one in `state:` is `E-QUEST-RESERVED-DECL`. The two entry flags are `bool`s, `false` until the entry is first presented (see [Lore entries](/language/lore-entries/)). `entry.<id>.read` resets with the run, so a new run's first read applies the entry's effects again; `entry.<id>.everRead` (0.22.0) is set on the first read ever and no new run resets it.

Two quest paths say why something failed. `quest.<id>.failedBy` reads `unset` until the quest fails, then names the cause: `fail` (its `fail` predicate), `by` or `until` (an objective's deadline), `cascade` (its parent failed), or `superseded` (its `complete="any"` parent completed through another alternative). `quest.<id>.objectives.<o>.failed` is `true` once the objective's `by` or `until` has failed it. An epilogue can therefore tell a missed deadline from a road not taken with `<match on="quest.hunt.failedBy">`. A run-tier quest's reset clears both.

The `clock.*` paths exist only when a schema declares a `clock:`. Without one, reading `clock.index` is `E-UNDECLARED`. `clock.index` counts positions from the start of day 1 (slots, or whole days for a clock without slots), so it only ever grows, and `clock.weekday` / `clock.weekdayLabel` need the clock's `week:`. See [The clock](/language/clock/).

### `owner: engine`

For state of your own that only the engine should write — a day counter, the run's outcome, anything your game loop advances — add **`owner: engine`** to the declaration (0.22.0):

```yaml
state:
  run.day:        { type: number, default: 1, owner: engine }
  run.outcome:    { type: { enum: [fell, fled, won] }, owner: engine }
  user.bond.mara: { type: number, default: 0 }
```

Reads are unrestricted: `when="run.day > 1"` and `{{run.day}}` are ordinary reads. A content `::set` of the path, or of a field under it, is `E-ENGINE-OWNED-WRITE`:

<!-- lute-diagnostics -->
```
./scenes/hub/day-end.lute:12:8: error [E-ENGINE-OWNED-WRITE] `::set` cannot write `run.day`: it is declared `owner: engine` — the engine writes it and content may only read it; in `lute play` write it with an `engine:` step, in a trace/test with a mock (dsl 0.22.0 §1.2)
```

`engine` is the only owner a declaration can name: any other `owner:` value is `E-STATE-DECL`, and a path with no `owner:` stays content-written. The key changes who may write the path, not its type, tier, or default. It binds content, so the checker enforces it and it does not reach the compiled artifact: the path's state-table entry is the same with or without it. `lute context` marks such a path `(owner: engine)` in its state listing.

In the toolchain, the engine's writes come from outside the content. A [`lute play`](/tooling/play/) script writes them with an `engine:` step (a literal, or `{ add: n }` for a numeric delta), and [`lute trace`](/tooling/tracing/) and `lute test` seed them as mocks:

```yaml
steps:
  - occasion: dayEnd
  - label: the engine closes the day
    engine:
      state: { run.day: { add: 1 } }
```

So a harness never needs a stand-in scene that `::set`s engine state to move a playthrough along — `owner: engine` is what refuses one. For facts, a [`reserved: true` relation](/state/facts-and-datalog/) plays the same role: content never asserts or retracts it, and an `engine:` step may.

### The previous run

A hub between runs often wants to talk about the run that just ended: where the player fell, how far they got, how they left. **`prev.run.<path>`** (dsl 0.23.0) reads the value `run.<path>` had when the previous run ended.

```lute check
---
kind: scene
id: hearth.welcomeBack
state:
  run.floor: { type: number, default: 0 }
  run.outcome: { type: { enum: [fell, fled, won] } }
---

# The hearth

## Shot 1.

<match on="prev.run.outcome">
  <when is="fell">
    @wren: You fell, last time. Slower, this time.
  </when>
  <when is="fled">
    @wren: You ran. No shame in that.
  </when>
  <when is="won">
    @wren: Back already? You won, last time.
  </when>
  <when is="unset">
    @wren: First climb? Stay close to the wall.
  </when>
</match>
@wren{when="isSet(prev.run.floor) && prev.run.floor >= 5"}: Floor five, though. Better than most.
```

- **You declare nothing.** Every declared `run.<path>` gets a mirror `prev.run.<path>` of the same type. `prev.*` is reserved: declaring a path under it is `E-STATE-NAMESPACE`, and a typo such as `prev.run.flor` is `E-UNDECLARED` with a did-you-mean.
- **It may be unset.** The mirror has no default, since no run has ended when the first one starts. Every read needs an `isSet(…)` guard or an `unset` arm, otherwise it is `E-MAYBE-UNSET`, even when `run.<path>` has a default. The `unset` arm above answers the first run, and a run that ended with `run.outcome` never set.
- **It is read-only.** A content `::set` of it is `E-QUEST-RESERVED-WRITE`.
- **The engine takes the snapshot.** When a run ends, the engine copies every `run.*` value into `prev.run.*` before the new run resets `run.*`. The mirror is a checker declaration, not a row of the compiled state table, so artifacts do not change.

In the toolchain, [`lute play`](/tooling/play/) takes the snapshot at every `newRun` step. A play script's `state:` seed or a trace mock may also set `prev.run.*` directly, to start from a later run.
