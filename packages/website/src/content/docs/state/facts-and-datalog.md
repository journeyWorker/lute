---
title: Facts and Datalog
description: The relational fact layer beside Lute's scalar tiers — declared entities and relations, ground facts asserted and retracted as deltas, and a total Datalog derivation layer that stays terminating by construction.
---

Scalar tiers hold magnitudes; they cannot express *relationships between entities* — "Shadowheart is in the party," "the player told Halsin about the grove." Lute adds a **relational fact kernel** beside the scalar tiers: a closed, n-ary fact database over a declared finite vocabulary. A document opts in simply by declaring relations; a document with none behaves exactly as before.

## Entities, relations, ground facts

Entity **kinds** enumerate their members (`members:`) or are engine-open (`open: engine`). A **relation** is a declared predicate with fixed arity and a typed argument signature; each argument ranges over an entity kind, a named enum, or `bool`. A **fact** is a relation applied to matching entities — it is *symbolic* (it holds or it does not; no numeric value slot). Every fact carries a valid-time interval, so retraction is a tombstone, never a deletion.

```yaml
entities:
  character: { members: [shadowheart, halsin, player] }
  location:  { members: [camp, grove, moonrise] }
relations:
  inParty:    { args: [character], tier: run }
  atLocation: { args: [character, location], tier: run, key: [0] }
  canReach:   { args: [character, location], derive: true }
facts:
  - "inParty(shadowheart)"
  - "atLocation(player, camp)"
```

Content writes **deltas** with the leaf directives `::assert` and `::retract`; the engine maintains the cumulative, time-scoped view. A functional `key:` auto-invalidates the superseded fact. Wildcards (`_`) are admitted only in `::retract`.

```lute
::assert{ atLocation(shadowheart, grove) }
::retract{ atLocation(shadowheart, _) }
```

## Querying history with `validAt`

Because every fact carries a valid-time interval, a guard can ask whether a fact held *at a past instant* rather than right now: `validAt(rel(args), t)`, beside the valid-now `holds(rel(args))` and `count(rel(args)) OP n` (see [CEL expressions](/state/cel/)). It is admitted over base relations, and over derived ones whose rules carry no CEL guard in any feeding stratum — a guard reads scalar state, scalars keep no history, so a point-in-the-past derivation through one is ill-defined (`E-VALIDAT-DERIVED`).

`t` must be a `narrativeTime` expression, and there is exactly one an author can write: **`quest.<id>.activatedAt`**, the instant the engine stamps when a quest goes `unset` → `active` (see [Quests & scenes](/language/quests-and-scenes/)). Declaring a `narrativeTime` path of your own is `E-TEMPORAL-ARG`, so before 0.8.0 reserved that slot `validAt` had no anchor to point at and the query was unusable in practice.

Together they express the gate a quest actually wants — not *did this ever happen*, but *did it happen since the quest was taken up*:

```lute
<quest id="theCoffeeDebt" title="Settle the coffee debt">
  <objective id="visitedSinceAccept" title="Go back to the station" done="holds(arrivedSpace(station_front)) && !validAt(arrivedSpace(station_front), quest.theCoffeeDebt.activatedAt)"/>
</quest>
```

The fact is valid now and was *not* valid at the activation instant, so the objective completes only on a visit made after the debt was accepted — never on one the player had already made. (A tag and all of its attributes must sit on one physical line; that `<objective …/>` cannot be wrapped.)

## Datalog derivation

A relation marked `derive: true` is computed by `rules:` — Horn clauses, function-free, with stratified negation. Multiple rules for one head union.

```yaml
rules:
  - "canReach(C, L)  :- atLocation(C, L)"
  - "canReach(C, L2) :- canReach(C, L1), connected(L1, L2)"
```

Because there are no function symbols, the Herbrand base is finite: bottom-up evaluation reaches a least fixpoint in finitely many steps, so **every derivation terminates** — this is Datalog, not Prolog. Safety requires every head/negated/guard variable to appear in a positive body atom; violations are `E-DATALOG-UNSAFE`, a negation cycle is `E-DATALOG-UNSTRATIFIED`, a would-be function term is `E-DATALOG-FUNCTION`. A rule body may carry a scalar CEL guard (`cel("run.act == 1")`) but never a fact query — that firewall (`E-DATALOG-GUARD-FACT`) keeps every dependency visible to the analysis. Derived and `reserved:` relations are read-only to content. The whole layer reduces to data the engine evaluates deterministically; nothing is author-iterated.

## How `check-project` analyzes relational guards

A guard that presumes knowledge is the right tool — `@eris{when="holds(knows(player, lumen))"}` shows the line only once the player knows. Since 0.20.0, `check-project` decides every relational query (`holds(…)`, `count(…)`) in every guard slot — a line `when=`, a `<choice when>`, a `<when test>`, a `::next` guard, a lore entry `when`, a quest `start`/`fail`, an objective `done`/`when` — as one of three verdicts:

- **impossible** — no seed, assert, rule, or engine relation anywhere in the project can produce a matching fact;
- **guaranteed** — a matching fact holds on **every** declared route reaching the guard;
- **possible** — anything else: some routes have it, some do not, or the analysis cannot separate them. This is the normal case for a guard, and it is silent.

The verdicts come from two sets:

- **May** — project-wide and flow-insensitive: every ground fact that can be live at any point of any run. The `facts:` seeds, the fact of every `::assert` anywhere in the project root (scenes, quest `<on>`/`<objective>` bodies, lore entries) whose document is not proven unreachable, every fact of a `reserved:` relation (the engine may populate any of them), and whatever the `rules:` derive over that set. May only knows *that* some route can produce a fact, not when.
- **Must** — path-sensitive: the facts live on every route to one program point. Within a document it is a forward walk: `::assert` adds a fact (and drops the one its `key:` supersedes), `::retract` removes every match, `<branch>`/`<match>` arms **intersect** where they rejoin (a branch with no unguarded choice, or a non-exhaustive match, also intersects with the set before the block, because no arm may run), and a `<hub>` body may run zero or more times. Across scenes it follows the `after:` graph, exactly like the [scalar envelope](/connectivity/envelopes/): a scene starts from the seeds plus the facts its `after:` formula guarantees — `visited(A)` contributes what holds when `A` ends, `&&` unions, `||` intersects, and `completed(q)`/`active(q)` contribute nothing. Quest bodies and lore entry bodies run at engine-chosen times, so they start from the seeds plus their own guards.

Three rules keep Must sound:

- **Only monotone facts cross time the author does not control.** A fact survives a document boundary only if no `::retract` anywhere in the project matches it, no other assert shares its `key:` tuple with a different value, and its relation is neither `reserved:` nor has an `open: engine` argument. Facts of `tier: scene` and `tier: quest` relations never cross a document boundary — the engine clears them with their episode or quest.
- **Guards are assumptions.** Inside `<when test="holds(F)">`, a `<choice when="holds(F)">`, a guarded `::next`, an `<on when>`, an objective `done`, or an entry `when`, each positive top-level `&&` conjunct `holds(F)` with a ground `F` is in Must — so a nested guard on the same fact is shown redundant. A line `when=` guards only its own line.
- **Counts are intervals.** `count(P)` lies between the number of Must facts matching `P` and the number of May facts matching it; a comparison against `n` is decided when the whole interval falls on one side.

The verdict feeds the same decision procedure that already reports dead scalar guards, so a relational guard that can never hold is reported through the code its slot already owns, and a guaranteed one inside a guard is flagged as redundant:

| Slot | Can never hold | Always holds |
| --- | --- | --- |
| `<when test>`, `<choice when>`, content line `when=`, `::next` guard | `E-ARM-DEAD` | `W-FACT-GUARANTEED` (not `::next`) |
| lore entry `when` | `E-ENTRY-UNREACHABLE` | `W-FACT-GUARANTEED` |
| objective `done` | `E-OBJECTIVE-UNSATISFIABLE` | — (a predicate, not a guard) |
| required objective `when` | `W-OBJECTIVE-HIDDEN` | — |
| quest `start` / `fail` | `E-QUEST-UNREACHABLE` (`fail` also when it always holds) | — |

A query the document cannot even state — an undeclared relation, a wrong arity, an argument outside a closed domain — stays undecided; its own error (`E-RELATION-UNKNOWN`, `E-RELATION-ARITY`, `E-FACT-DOMAIN`) owns it. `E-ENTRY-UNREACHABLE` is new with this analysis, and also fires in single-file `lute check` for an entry `when` that decides false on scalars alone. `W-UNPROVEN-RELATIONAL`, which used to mark every relational gate as "not proven", is gone: its premise no longer holds, and naming it in `--deny` is a usage error.

A small example. The archive asserts one fact unconditionally and another only down one branch arm:

```lute
## The Archive

@narrator: The log is still open on the console.
::assert{ knows(player, lumen) }
<branch id="dig">
  <choice id="readOn" label="Keep reading">
    ::assert{ knows(player, heading) }
    @narrator: The heading was changed eleven years ago.
  </choice>
  <choice id="close" label="Close the log">
    @narrator: You close it.
  </choice>
</branch>
```

The bridge is sequenced `after: 'visited("demo.archive")'` and guards three lines:

```lute
## The Bridge

@eris{when="holds(knows(player, lumen))"}: So you read the log.
@eris{when="holds(knows(player, heading))"}: Then you know where we are going.
@eris{when="holds(knows(eris, heading))"}: I changed it myself.
```

`check-project` settles the first and third and leaves the second alone — `knows(player, heading)` holds only if the player chose `readOn`, which is exactly what that guard is for:

<!-- lute-diagnostics -->
```
./scenes/bridge.lute:10:13: warning [W-FACT-GUARANTEED] guard `holds(knows(player, lumen))` is redundant: `knows(player, lumen)` is asserted on every route to here (./scenes/archive.lute:10) (dsl 0.20.0 §5)
```

<!-- lute-diagnostics unverified="the relational E-ARM-DEAD message is composed in crates/lute-check/src/fact_check.rs, which names the code through the reachability::E_ARM_DEAD constant rather than a string literal, so the scraper cannot pair quote and code; copied verbatim from check-project output" -->
```
./scenes/bridge.lute:12:13: error [E-ARM-DEAD] this gated line can never be shown: its `when` guard `holds(knows(eris, heading))` is provably false — no seed, assert, rule, or engine relation produces `knows(eris, heading)` under your declared routes (dsl 0.20.0 §5)
```

The same two sets decide counts: after the archive, `count(knows(player, _))` lies between 1 and 2, so a guard `count(knows(player, _)) >= 3` is dead and `count(knows(player, _)) >= 1` is redundant.

The analysis needs the whole project — an assert in a sibling document is invisible to a single file, and claiming "never asserted" from one file would be false — so it runs in `check-project` only. Single-file `lute check` leaves relational queries undecided. To see why a guard was judged guaranteed, `lute scenario <dir> envelope <scene>` lists the **guaranteed facts** at the scene's entry beside the scalar tables (see [envelopes](/connectivity/envelopes/#guaranteed-facts)).
