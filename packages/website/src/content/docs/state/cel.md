---
title: CEL expressions
description: The restricted Lute-CEL profile — the closed environment behind every condition and assignment, where it appears, the fact-query surface it admits, and how it compiles to inline CEL strings.
---

Every condition and every `::set` right-hand side in Lute is [CEL](https://cel.dev) — terminating and side-effect-free — interpreted under the **Lute-CEL profile**, a maximally restricted subset. CEL is what keeps the language *total, not Turing-complete*: there is no host-language (JS, Lua, …) evaluation anywhere.

## Where CEL appears

CEL text sits in every guard and value slot:

- `<match subject="S">` subject, `<when test="…">` guards, `<choice when="…">` and `when=` content-line gates
- `::set{path = celExpr}` right-hand sides, and since 0.24.0 a `::set`'s own `when="…"` guard
- quest `<quest start="…" fail="…">` and `<objective done="…" visibleWhen="…" by="…" until="…">` predicates, `<on when="…">` handlers
- beat `when:` / `when=` eligibility conditions
- a cast entry's `present:` condition (0.24.0, see [The cast](/language/dialogue-and-cast/#the-cast))

```lute
<match subject="scene.affect.elena">
  <when is="3..">
    @elena: …
  </when>
  <when test="@chose('couch', 'ignore')">
    @elena: …
  </when>
  <otherwise>
    @elena: …
  </otherwise>
</match>
```

Inside a `<match>`, the token `$` resolves to the subject expression `S` and MUST NOT appear elsewhere. A `@ref` / `@fn(args)` is a **compile-time macro** expanded to inline CEL before evaluation, with params bound from the call arguments — parenthesized and AST-safe.

## The closed environment

The Lute-CEL environment provides exactly: CEL operators, literals, list literals, the ternary `?:`, the `in` membership operator, the `has()` macro, and the single extension `has(<path>)` (true iff a state path is assigned). **For 0.32, `has(select)` checks identifier paths; use `"key" in map` for presence of a non-identifier key.** No other functions exist — a comprehension (`map`/`filter`/`exists`/`all`), `size`, or `matches` is a static error (`E-CEL-PROFILE`). State-path segments are names (letters, digits, `_` or `-`, not starting with `-`); `defs` names and param names are identifiers, read bare. A segment that is not an identifier is written quoted, `quest["zero-coke-001"].state`; after a `.` it would read as a subtraction (`E-PATH-IDENT`).

The operators include **integer `%`** since 0.24.0 (it was `E-CEL-PROFILE` before). `run.day % 7 == 0` and a def `wd: "run.day % 7"`, typed `int` by inference, are clean. Both operands must be integers: a non-`int` operand (`'a' % 2`, a bool path or comparison) or a fractional literal (`run.day % 2.5`) is `E-CEL-TYPE`. `%` is the truncated integer remainder, so `-7 % 3 == -1`. At run time a fractional value or a zero divisor makes the result unknown rather than an error.

```lute check
---
kind: scene
id: tower.stairs
state:
  run.day: { type: int, default: 1 }
  user.deaths: { type: int, default: 0 }
defs:
  weekday: "run.day % 7"
---

## The Stairs

@narrator{when="@weekday == 0"}: A full week on the stairs.
@narrator: This is your {{user.deaths:ordinal}} death.
```

A condition reads declared state paths, and the reserved paths the engine writes without any declaration: the quest slots `quest.<id>.state`, `quest.<id>.activatedAt`, and `quest.<id>.objectives.<o>.done`, and since 0.24.0 `quest.<id>.failedBy` and `quest.<id>.objectives.<o>.failed`; the lore entry flags `entry.<id>.read` (run tier) and `entry.<id>.everRead` (user tier, since 0.22.0); since 0.23.0, `prev.run.<path>`, the value each declared `run.<path>` had when the previous run ended (always maybe-unset; see [The previous run](/state/state-model/#the-previous-run)); and, when a schema declares a [clock](/language/clock/), `clock.index`, `clock.weekday` and `clock.weekdayLabel` (0.24.0). None of them is ever a `::set` target — see [Paths the engine writes](/state/state-model/#paths-the-engine-writes).

When the relational layer is in play, conditions may also read the fact database through a bounded predicate surface: `holds('rel', ['args|_'])` (valid-now membership, any slot may be `_`), `count('rel', ['args|_']) OP n` (distinct valid tuples), `countDistinct('rel', ['args|_'], int) OP n` (since 0.24.0: the distinct values at the 0-based position whose list element is `'_'`, so `countDistinct('sawAt', ['_', '_', '_'], 0)` counts distinct witnesses where `count('sawAt', ['_', '_', '_'])` counts sightings), and `validAt('rel', ['args|_'], T)` (historical form over base relations). `T` is a `narrativeTime` expression — in practice `quest.<id>.activatedAt`, the only one an author can name (see [Quests & scenes](/language/quests-and-scenes/)). Joins under aggregation are expressed as [derived relations](/state/facts-and-datalog/), never multi-relation `count`. A `<match on>` subject must stay a scalar/enum path; fact queries live in guards only, and none of them may appear in a rule's `cel()` guard.

Since 0.21.0, every condition slot may also call `visited('<scene id>')` — true once that scene has been presented in this save, the same visited set a scene's `after:` reads. The argument is one string literal naming a scene of the project, or, since 0.23.0, a [bundle beat](/language/beats/#beat-bundles) by its canonical `<document id>.<beat id>` (`E-CONN-UNKNOWN-NODE` in whole-project `lute check` otherwise). See [Quests & scenes](/language/quests-and-scenes/#quests-meet-scenes-and-occasions).

## Presence of a `per:` member

A `per:` family is indexed by a member of its kind, and a member whose name is not a CEL identifier (`look-around`) is written quoted, `user.stepRank["look-around"]`. To ask whether such a member is set, write the membership form (0.49.0):

```lute
@guide{when="'look-around' in user.stepRank"}: You have already looked around.
@guide{when="!('look-around' in user.stepRank)"}: Take a look around first.
```

`'<member>' in <family>`, with a string literal and a `per:` family, is a presence guard of `<family>.<member>`, exactly equivalent to `has()` of the dotted member: when it is true the member is assigned for the rest of the guarded body, and its negation proves the opposite. A literal that is not a member of the family gets the diagnostic a read of `<family>["<literal>"]` gets. `lute trace` and the runtime evaluate it as `has()`.

`has(user.stepRank["look-around"])` stays `E-CEL-PARSE`, now with the hint ``— `has()` takes a field; for a member that is not an identifier write `'<m>' in <family>` ``.

Every message that names a presence test spells it the same way: `has(<path>)` / `!has(<path>)` when every segment after the root is an identifier, and `'<m>' in <family>` / `!('<m>' in <family>)` when the last segment is a `per:` member that is not. For any other path with a non-identifier segment there is no guard spelling, so the message names only the `after:` and `default:` remedies.

## Interpolation is not CEL

A `{{…}}` interpolation in content text is not a CEL expression. It names one value: a state path (in a beat that runs for a kind's members, also a `per:` family indexed by the member, `{{user.bond[occasion.target]}}`), `{{occasion.target}}` itself there, a def `@ref` / `@ref(args)`, or `userName`. A computed value needs a def. Since 0.24.0 an interpolation may carry one **format hint** after a colon: `{{user.deaths:ordinal}}` renders `1st`, `2nd`, `3rd`, `11th`, `21st`, `111th` (see [Interpolation](/language/dialogue-and-cast/#interpolation)). The hints are `ordinal`, `ordinalWord`, `cardinalWord` and `plural(one|other)` for an `int` or `double`, and `capitalize`, `start` and `indefinite` for text; any other name is `E-CEL-PROFILE`. A numeric hint on a string, enum or bool path or def, or on `{{userName}}`, is `E-REF-TYPE`, and so is a text hint on a numeric path or a bool.

## Compile target

`@ref` macros expand at **compile time**; the resulting inline CEL string is carried in the flat command-record artifact and evaluated at **runtime** by the engine. Everything desugars to flat records plus CEL strings — the compiler↔engine contract. The unset sentinel is CEL `null` (tested with `!has(path)` or a `<when is="unset">` arm), never the string `'unset'` — comparing to that string is `E-UNSET-LITERAL`. The one exception is `quest.<id>.state`: it is always assigned, and `unset` is its first lifecycle member, so `quest.q.state == 'unset'` is the correct test for "not taken up yet" and `has(quest.q.state)` — always true — is `W-QUEST-STATE-HAS`. Since dsl 0.26.0 a string compared with a path whose values are a closed set of names — an enum path, `occasion.target`, a quest's `state` or `failedBy`, `scene.choices.<id>`, an enum param, `$` in a `<match>` — must be one of them, whether by `==`, `!=` (either side) or as an `in [...]` element: `quest.q.state == 'actve'` is `E-WHEN-LITERAL-DOMAIN` (`` did you mean `'active'`? ``), the code a foreign `<when is>` literal gets, in place of the dead-guard error the comparison would otherwise cause.

## 0.32 standard CEL contract

The closed profile uses `int` and `double` (never `number`), rejects mixed
numeric operations, and uses list-form host calls:
`holds(string, list(dyn))`, `count(string, list(dyn))`,
`countDistinct(string, list(dyn), int)`, `validAt(string, list(dyn), int)`,
`now()`, and `visited(string)`. Use `has(select)` and `"key" in map` for presence. Every execution IR carries
`celEnv`; condition errors mean “not satisfied”, while `::set` errors halt without a write. Activation always includes reserved engine paths with defaults: `quest.<id>.state` and `.failedBy` are `"unset"`, each quest objective's `.done` and `.failed` are `false`, and each lore entry's `.read` and `.everRead` are `false`. `lute trace`'s
three-valued result is a preview only.
