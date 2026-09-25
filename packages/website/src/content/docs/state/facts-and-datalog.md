---
title: Facts and Datalog
description: The relational fact layer beside Lute's scalar tiers — declared entities, sub-kinds and relations, ground facts asserted and retracted as deltas, and a total Datalog derivation layer that stays terminating by construction.
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
  wounded:    { args: [character], tier: run, reserved: true }   # asserted by the engine only
facts:
  - "inParty(shadowheart)"
  - "atLocation(player, camp)"
```

Content writes **deltas** with the leaf directives `::assert` and `::retract`; the engine maintains the cumulative, time-scoped view. A functional `key:` auto-invalidates the superseded fact. Of the two, only `::retract` admits wildcards (`_`); queries and rule bodies take them too (see [The anonymous variable](#the-anonymous-variable-and-countdistinct)). A `reserved: true` relation (`wounded` above) is the engine's to populate — the facts counterpart of an [`owner: engine`](/state/state-model/#owner-engine) state path. Content never asserts or retracts one (`E-RELATION-RESERVED-WRITE`); in the toolchain, a trace mock or a `lute play` `engine:` step supplies its facts.

```lute
::assert{ atLocation(shadowheart, grove) }
::retract{ atLocation(shadowheart, _) }
```

A relation cannot take the name of a CEL call, macro or keyword — `has`, `holds`, `count`, `isSet`, `now`, and the like. `holds(has(lamp))` could never be written, so declaring such a relation is `E-RELATION-RESERVED-NAME` at its declaration (dsl 0.24.0).

### Sub-kinds: `subsetOf:`

An id belongs to exactly one entity kind, and two kinds listing the same member is `E-ENTITY-KIND-CLASH`. A group that is part of a larger one, such as the companions among the people, is a **sub-kind** instead (dsl 0.24.0 §3):

```lute check
---
kind: scene
id: camp.roster
entities:
  person:    { members: [isolde, corvin, hollis, player] }
  companion: { subsetOf: person, members: [isolde, corvin] }
relations:
  inParty: { args: [companion], tier: run }
  trusts:  { args: [person, person], tier: run }
  loyal:   { args: [companion], derive: true }
rules:
  - "loyal(P) :- inParty(P), trusts(P, player)"
---

# The roster

## Shot 1.

::assert{ inParty(isolde) }
::assert{ trusts(isolde, player) }
@isolde{when="holds(loyal(isolde))"}: I'm with you.
```

Every member of a sub-kind must also be a member of its parent, and a member it shares with its parent, or with a sibling sub-kind, is no clash. A sub-kind is legal wherever a kind is: a relation argument, a `per:` index ([State model](/state/state-model/)), an occasion's target domain. A value of the sub-kind is also a value of the parent, so `trusts(isolde, player)` is legal and the rule above joins a `companion` with a `person` argument. The reverse does not hold: `::assert{ inParty(hollis) }` is `E-FACT-DOMAIN`, because `hollis` is a person but not a companion.

A sub-kind lists its own `members:`. A member outside the parent is `E-ENTITY-KIND-SHAPE`, naming the outsider. The same code covers a parent the document does not declare, a parent declared `open:`, a sub-kind declared `open:`, and a `subsetOf:` chain that loops back on itself.

`check-project`'s `W-DOMAIN-UNREAD` flags a kind that nothing reads. It counts a sub-kind's `subsetOf:` parent, a kind used as a `per:` index, and a kind atom in a rule body or a `holds(…)` condition as reads, alongside relation arguments. The warning lands on the line of the schema that declares the kind, or on the document's own `entities:` key, rather than on the first importer.

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

A rule is `head :- body`, where the body is a comma-separated conjunction of these literals:

| Literal | Example | Meaning |
| --- | --- | --- |
| relation atom | `atLocation(C, L)` | a base or derived fact matches; variables are capitalized, constants are entity members |
| entity kind | `character(C)` | `C` ranges over the members of the entity kind `character`: a membership test, never a fact lookup |
| negation | `not awake(P)` | no matching fact holds (stratified: `awake` must not depend on this head) |
| anonymous variable | `sawAt(W, _, _)` | each `_` is a fresh variable that matches anything (dsl 0.24.0); under `not`, no matching tuple exists at all |
| inequality | `A != B` | two bound terms differ |
| scalar guard | `cel("run.act == 1")` | a CEL condition over scalar state, never a fact query |

```yaml
rules:
  - "suspect(P) :- character(P), not inParty(P)"
  - "rivals(A, B) :- inParty(A), inParty(B), A != B"
  - "canReach(player, moonrise) :- cel(\"run.act >= 2\")"
```

Because there are no function symbols, the Herbrand base is finite: bottom-up evaluation reaches a least fixpoint in finitely many steps, so **every derivation terminates** — this is Datalog, not Prolog. Safety requires every head/negated/guard variable to appear in a positive body atom (an entity-kind atom such as `character(P)` counts); violations are `E-DATALOG-UNSAFE`. A rule whose head has no variables needs no positive atom at all, so a ground head behind a scalar guard alone — `canReach(player, moonrise) :- cel("run.act >= 2")` — is a legal rule. A negation cycle is `E-DATALOG-UNSTRATIFIED`, and a would-be function term is `E-DATALOG-FUNCTION`. A rule body may carry a scalar CEL guard (`cel("run.act == 1")`) but never a fact query — that firewall (`E-DATALOG-GUARD-FACT`) keeps every dependency visible to the analysis. Derived and `reserved:` relations are read-only to content. The whole layer reduces to data the engine evaluates deterministically; nothing is author-iterated.

An entity-kind atom is a test against the kind's members, never a lookup of `character(…)` facts: no fact is ever asserted under a kind's name. `lute trace`, `test`, `play` and `run` evaluate it that way in a join, under `not`, and in `--explain`. Before 0.24.0 they looked for such facts, so a rule like `inParty(P) :- companion(P), …` passed `check` and derived nothing at run time.

### The anonymous variable and `countDistinct`

A `_` in a rule body stands for a value the rule does not care about. Each `_` is its own fresh variable, so the two in `sawAt(W, _, _)` need not be equal. It never needs binding, so it does not affect safety. Under `not` it is existential: `not sawAt(W, _, _)` holds when `W` saw nobody, anywhere. A `_` in a rule head is `E-DATALOG-PARSE`, because a head argument must be a bound variable or a constant, and so is a `_` in a comparison (`X != _`).

A condition that counts matching facts has two forms. `count(<pattern>)` counts tuples, and `countDistinct(<pattern>, <Var>)` counts the distinct values at the position the variable `<Var>` names:

```lute check
---
kind: scene
id: inquest.hall
entities:
  person: { members: [ada, bram, cole, dora] }
  place:  { members: [dock, mill] }
relations:
  sawAt:     { args: [person, person, place], tier: run }
  testified: { args: [person], derive: true }
  silent:    { args: [person], derive: true }
rules:
  - "testified(W) :- sawAt(W, _, _)"
  - "silent(W) :- person(W), not sawAt(W, _, _)"
facts:
  - "sawAt(ada, bram, dock)"
  - "sawAt(ada, cole, mill)"
  - "sawAt(bram, cole, dock)"
---

# The inquest

## Shot 1.

@narrator{when="count(sawAt(_, _, _)) >= 3"}: Three sightings are on record.
@narrator{when="countDistinct(sawAt(W, _, _), W) >= 2"}: At least two witnesses came forward.
@narrator{when="holds(testified(bram)) && holds(silent(dora))"}: Bram spoke. Dora saw nothing at all.
```

Three sightings, two witnesses: `ada` saw two people and `bram` one. `<Var>` must be a variable that appears in the pattern, otherwise the call is `E-CEL-PROFILE`. `check-project` decides `countDistinct` from its fact envelope as it does `count`, and trace, test and play evaluate it. Like `count` and `holds`, it reads the fact store, so it is forbidden in a rule guard (`E-DATALOG-GUARD-FACT`).

### `@def`s in a rule guard

A rule guard may call any def a condition may, arguments included. Since 0.24.0 the guard is expanded against the project's def table before anything reads it, so `check`, `trace`, `test` and `play` all see the same body:

```lute check
---
kind: scene
id: lamp.room
state:
  run.day: { type: number, default: 1 }
defs:
  firstDay: { type: bool, cel: "run.day == 1" }
entities:
  item: { members: [lamp] }
relations:
  lit: { args: [item], derive: true }
rules:
  - "lit(lamp) :- cel(\"@firstDay\")"
---

# The lamp room

## Shot 1.

@narrator{when="holds(lit(lamp))"}: The lamp is already burning.
```

Before 0.24.0 this passed `check` and then never derived: the guard was evaluated unexpanded, read as undecided, and the rule was silently dropped. The expanded body still has to pass the firewall, so a def that queries facts is `E-DATALOG-GUARD-FACT` there. A def the guard cannot expand is `E-RULE-GUARD-DEF`: an undefined name, a wrong argument count, or a `$`, which has no match subject in a rule. When a guard is still undecided at play time, a condition that queries the rule's relation reads unknown rather than false, and play halts there and names what would decide it.

### Entity-indexed state in a rule guard

A path declared with [`per:`](/state/state-model/) holds one number per member of a kind. Anywhere else a member is named (`run.approval.isolde`). In a rule `cel()` guard, `run.approval[P]` reads the member bound to the rule variable `P` (dsl 0.24.0 §3):

```lute check
---
kind: scene
id: camp.fire
state:
  run.approval: { type: number, default: 0, per: companion }
entities:
  person:    { members: [isolde, corvin, hollis] }
  companion: { subsetOf: person, members: [isolde, corvin] }
relations:
  inParty: { args: [companion], tier: run }
  devoted: { args: [companion], derive: true }
rules:
  - "devoted(P) :- inParty(P), cel(\"run.approval[P] >= 5\")"
---

# Camp

## Shot 1.

::assert{ inParty(isolde) }
::set{run.approval.isolde += 5}
@isolde{when="holds(devoted(isolde))"}: I would follow you anywhere.
```

`P` must be bound by a positive body atom that ranges it over the index kind or one of its sub-kinds. Here `inParty(P)` ranges it over `companion`. Each way of getting that wrong has its own code:

- `P` bound only by `person(P)`, a wider kind, is `E-FACT-DOMAIN`: some binding (`hollis`) would read a path that is not declared;
- `P` bound by no positive atom is `E-DATALOG-UNSAFE`;
- `run.trust[P]` over a path declared without `per:` is `E-UNDECLARED`.

Such a rule compiles **grounded**: one IR rule per member of the kind, each guard over a ground path, with the rule's `raw` text suffixed by the binding. The rule above becomes two, one for each companion:

```json
{"head": {"relation": "devoted", "terms": [{"kind": "const", "value": "isolde"}]}, "body": [{"kind": "atom", "atom": {"relation": "inParty", "terms": [{"kind": "const", "value": "isolde"}]}, "negated": false}, {"kind": "guard", "cel": "run.approval.isolde >= 5"}], "raw": "devoted(P) :- inParty(P), cel(\"run.approval[P] >= 5\") [P = isolde]"}
```

IR guards therefore stay CEL over ground terms, and an engine needs nothing new to evaluate them. `lute trace`, `play` and `run` evaluate the same instances.

## Derivation in trace, test, and play

The engine computes the minimal model at run time, and since 0.22.0 the toolchain computes the same one wherever it plays content. `lute trace`, `lute test`, and `lute play` load the project's seed `facts:` and apply its `rules:` — stratified negation included — over the facts you mock and the facts content asserts along the way. All three share one evaluator with the reference runner (`lute run`), so the toolchain cannot disagree with itself about what a rule concludes.

A derived fact therefore behaves like any other. Given

```yaml
entities:
  item: { members: [lamp] }
relations:
  knows:   { args: [item], tier: run }
  broken:  { args: [item], tier: run }
  canMend: { args: [item], derive: true }
rules:
  - "canMend(I) :- knows(I), not broken(I)"
```

a guard `holds(canMend(lamp))` is decided by the rule: mock `knows(lamp)` and it holds, add `broken(lamp)` and it does not. You never mock the conclusion, and "false because a negated premise holds" is testable. A mocked derived atom is still accepted; it joins the base facts like a seed. When a rule's `cel(…)` guard reads state the trace has not decided, that rule derives nothing, and the trace reports the conclusion unknown and names the state path that would decide it.

`derive: false` — a key of a trace mock, a `*.test.yaml`, or a play script — or the `--no-derive` flag, which wins over the key, restores the 0.21 model: the seed `facts:` are not loaded, an unmocked derived atom is unknown, and a note names each derived relation read. A test written against that model (an unmocked derived atom reading unknown, or a seeded relation reading empty) pins `derive: false` to keep its verdict. See the [Tracing guide](/tooling/tracing/) and [Playing a story](/tooling/play/).

To see *why* an atom holds, `lute play --explain <atom>` (repeatable) prints its derivation at the end of the playthrough: the rule used, each premise's own support — a seed fact, asserted, or derived in turn — and a negated premise shown `(absent)`:

```
explain canMend(lamp): holds
  canMend(lamp)  ⇐ canMend(I) :- knows(I), not broken(I)
  ├─ knows(lamp)  (asserted)
  └─ not broken(lamp)  (absent)
```

When the atom does not hold, it lists every rule that could conclude it, with the premise that failed:

```
explain canMend(lamp): does not hold
  canMend(I) :- knows(I), not broken(I)
  ├─ knows(lamp)  (asserted)
  └─ ✗ not broken(lamp)  (but it holds: asserted)
```

## How `check-project` analyzes relational guards

A guard that presumes knowledge is the right tool — `@eris{when="holds(knows(player, lumen))"}` shows the line only once the player knows. Since 0.20.0, `check-project` decides every relational query (`holds(…)`, `count(…)`) in every guard slot — a line `when=`, a `<choice when>`, a `<when test>`, a `::next` guard, a lore entry `when`, a quest `start`/`fail`, an objective `done`/`when` — as one of three verdicts:

- **impossible** — no seed, assert, rule, or engine relation anywhere in the project can produce a matching fact;
- **guaranteed** — a matching fact holds on **every** declared route reaching the guard;
- **possible** — anything else: some routes have it, some do not, or the analysis cannot separate them. This is the normal case for a guard, and it is silent.

The verdicts come from two sets:

- **May** — project-wide and flow-insensitive: every ground fact that can be live at any point of any run. The `facts:` seeds, the fact of every `::assert` anywhere in the project root (scenes, quest `<on>`/`<objective>` bodies, lore entries) whose document is not proven unreachable, every fact of a `reserved:` relation (the engine may populate any of them), and whatever the `rules:` derive over that set. May only knows *that* some route can produce a fact, not when. It reads a negated body atom as satisfiable, with one exception (dsl 0.23.0): a negation over a seed fact that nothing in the project retracts or displaces through its `key:` is false. With the seed `inParty(shadowheart)`, `suspect(P) :- character(P), not inParty(P)` can never conclude `suspect(shadowheart)`, so a guard `holds(suspect(shadowheart))` is dead, while `suspect(astarion)` still follows. Once any `::retract{inParty(shadowheart)}` exists, the negation is satisfiable again.
- **Must** — path-sensitive: the facts live on every route to one program point. Within a document it is a forward walk: `::assert` adds a fact (and drops the one its `key:` supersedes), `::retract` removes every match, `<branch>`/`<match>` arms **intersect** where they rejoin (a branch with no unguarded choice, or a non-exhaustive match, also intersects with the set before the block, because no arm may run), and a `<hub>` body may run zero or more times. Across scenes it follows the `after:` graph, exactly like the [scalar envelope](/connectivity/envelopes/): a scene starts from the seeds plus the facts its `after:` formula guarantees — `visited(A)` contributes what holds when `A` ends, `&&` unions, `||` intersects, and `completed(q)`/`active(q)` contribute nothing. Quest bodies and lore entry bodies run at engine-chosen times, so they start from the seeds plus their own guards.

Three rules keep Must sound:

- **Only monotone facts cross time the author does not control.** A fact survives a document boundary only if no `::retract` anywhere in the project matches it, no other assert shares its `key:` tuple with a different value, and its relation is neither `reserved:` nor has an `open: engine` argument. Facts of `tier: scene` and `tier: quest` relations never cross a document boundary — the engine clears them with their episode or quest.
- **Guards are assumptions.** Inside `<when test="holds(F)">`, a `<choice when="holds(F)">`, a guarded `::next`, an `<on when>`, an objective `done`, an entry `when`, or — for the whole scene — a beat scene's frontmatter `when:`, each positive top-level `&&` conjunct `holds(F)` with a ground `F` is in Must — so a nested guard on the same fact is shown redundant. A line `when=` guards only its own line.
- **Counts are intervals.** `count(P)` lies between the number of Must facts matching `P` and the number of May facts matching it; a comparison against `n` is decided when the whole interval falls on one side.

A guard may also assume that a lore entry was read (dsl 0.24.0 §6). Under `entry.X.read` (or `entry.X.read == true`), Must gains the facts that X's body asserts on every route through it and that nothing in the project retracts. The player has read the entry, so its asserts have run. Take an entry that records a suspicion:

```lute
<entry id="keeperLog" category="place">
  @narrator: The keeper's log names Isolde twice.
  ::assert{ suspects(isolde) }
</entry>
```

and a scene that offers a choice only to a player who has read it:

```lute
<branch id="ask">
  <choice id="press" label="Ask about the log" when="entry.keeperLog.read">
    @narrator{when="holds(suspects(isolde))"}: The log named her.
  </choice>
  <choice id="leave" label="Leave">
    @narrator: You leave the office.
  </choice>
</branch>
```

The inner guard can only hold, so `check-project` reports it redundant and names the assert that guarantees it:

<!-- lute-diagnostics -->
```
./scenes/office.lute:13:21: warning [W-FACT-GUARANTEED] guard `holds(suspects(isolde))` is redundant: `suspects(isolde)` is asserted on every route to here (./lore/harbor.lute:9) (dsl 0.20.0 §5)
```

Its negation, `!holds(suspects(isolde))`, would be a dead guard. An assert down only one arm of the entry's `<match>` guarantees nothing. `entry.X.everRead` only says the entry was read in some run, possibly an earlier one, so it adds only the entry's `tier: user` and `tier: app` facts, which a new run keeps. A run-tier fact like `suspects` is not added under `everRead`.

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

The analysis needs the whole project — an assert in a sibling document is invisible to a single file, and claiming "never asserted" from one file would be false — so it runs in `check-project` only. Single-file `lute check` leaves each relational query undecided, but still sees a contradiction between two queries in one condition: `holds(P) && !holds(P)` is false whatever holds (dsl 0.23.0, see [How a `when` is decided](/language/beats/#how-a-when-is-decided)). To see why a guard was judged guaranteed, `lute scenario <dir> envelope <scene>` lists the **guaranteed facts** at the scene's entry beside the scalar tables (see [envelopes](/connectivity/envelopes/#guaranteed-facts)). To see which documents produce the facts a guard queries, `lute scenario <dir> knowledge` traces each queried atom through the rules to its producers, or to **no producer** (see [Overviews](/tooling/overviews/#lute-scenario-knowledge)).

### Relations nothing reads

The converse problem is a relation that is written but never read. `check-project` (dsl 0.24.0 §7) reports `W-RELATION-UNREAD` for a declared, non-`reserved:` relation that is asserted, seeded or derived, but that no condition queries (`holds`, `count`, `countDistinct`), no rule body uses, and no def reads. Its facts change nothing:

<!-- lute-diagnostics -->
```
./world.schema.yaml:9:3: warning [W-RELATION-UNREAD] relation `rumor` is written (asserted, seeded, or derived) but never read: no condition queries it (`holds` / `count`), no rule body uses it, and no def reads it — the facts it records change nothing; read it where it matters, or drop it (dsl 0.24.0)
```

It is reported once per project, at the declaration: the schema file's line, or the document's own `relations:` key. Play scripts and scenario tests are not reads, so a relation that only a test inspects still draws it. Its companion for defs is `W-DEF-UNUSED` (see [State schemas](/state/schemas/)).

### Guards over facts not written yet

While a story is being written, a guard is often dead only because the scene that asserts its fact does not exist yet. `lute check-project --wip` (dsl 0.23.0) reports `E-ENTRY-UNREACHABLE`, `E-BEAT-UNREACHABLE`, and `E-OBJECTIVE-UNSATISFIABLE` as warnings when the guard is dead only because a relation has **no producer at all**: no seed, no `::assert` anywhere, no rule, and not `reserved:`. The warning's note says so. A relation that has producers but can never match the query stays an error, as does anything the dead content causes downstream, such as an `E-CONN-UNREACHABLE`. See [Work in progress](/language/beats/#work-in-progress).
