---
title: Story overviews
description: "Three read-only views of a story chosen by occasions (dsl 0.23.0): `lute beats` prints each occasion's beat ladder with the checker's verdicts, `lute calendar` evaluates play's own eligibility over a grid of state values, and `lute scenario knowledge` traces every fact-guarded condition through the rules to whatever produces its facts."
---

Once a story is selected by [occasions](/tooling/play/) rather than read top to bottom, no single file answers "what plays at the inn on the evening of day two?". The answer is spread over every beat's `on`, `target`, `priority`, `once`, `after:` and `when`, over the project's rules, and over whatever the save already holds. Since dsl 0.23.0 three commands put it on one screen:

- [`lute beats`](#lute-beats) — the **ladder**: every beat of each occasion and target in selection order, with what `check-project` concludes about it. Static; the project need not check clean.
- [`lute calendar`](#lute-calendar) — the **grid**: for every combination of the state values, quest statuses and facts you name — from a save, or partway along a played route — which beat `lute play` would present. It runs play's own eligibility, so it is exact where the checker is conservative.
- [`lute scenario knowledge`](#lute-scenario-knowledge) — the **knowledge map**: for every condition that reads a fact, which rules conclude it and who produces the facts they need, down to the ones nothing produces yet.

All three are read-only. The normative text is §1 of the [0.23.0 proposal](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md).

## The example project

The examples on this page run against a small harbour town on a day clock. Its plugin declares three occasions: the morning bell plays every eligible beat in turn (`select: sequence`, see [Composing occasions](/tooling/play/#composing-occasions)), a visit to a place picks one beat, and the notice board offers a list:

```yaml
occasions:
  dayStart:   { select: sequence, description: The bell rings; every eligible beat plays }
  placeVisit: { select: first, target: { prefix: place, entity: place } }
  board:      { select: all, description: Notices by the harbour master's door }
```

The engine owns the clock. Two rules place people by the time of day, and a third says Ada trusts you once you have met her, unless a rumour says otherwise:

```yaml
state:
  run.day:     { type: number, default: 1, owner: engine }
  run.slot:    { type: { enum: [morning, evening] }, default: morning, owner: engine }
  user.visits: { type: number, default: 0, owner: engine }
entities:
  person: { members: [ada, bo] }
  place:  { members: [inn, dock] }
relations:
  works:   { args: [person, place] }
  met:     { args: [person] }
  rumor:   { args: [person] }
  arrived: { args: [place], tier: run, reserved: true }
  present: { args: [person, place], derive: true }
  trusted: { args: [person], derive: true }
facts:
  - "works(bo, dock)"
rules:
  - "present(ada, inn) :- cel(\"run.slot == 'evening'\")"
  - "present(P, dock) :- works(P, dock), cel(\"run.slot == 'morning' && run.day != 3\")"
  - "trusted(P) :- met(P), not rumor(P)"
```

The beats:

| Beat | Document | Answers | Conditions | `once` |
|---|---|---|---|---|
| `day.bell` | `scenes/day/bell.lute` | `dayStart` | priority 10, `title: The bell` | `false` |
| `day.market` | `scenes/day/market.lute` | `dayStart` | `when: 'run.day == 3'`, `title: Market day` | `run` |
| `day.farewell` | `scenes/day/farewell.lute` | `dayStart` | `after: 'completed("ferry")'` | `run` |
| `inn.ada` | `scenes/inn/ada.lute` | `placeVisit` → `place.inn` | priority 20, `when: 'holds(present(ada, inn))'`; asserts `met(ada)` | `run` |
| `inn.again` | `scenes/inn/again.lute` | `placeVisit` → `place.inn` | priority 30, `after: 'visited("inn.ada")'`, `when: 'holds(trusted(ada))'` | `run` |
| `inn.regular` | `scenes/inn/regular.lute` | `placeVisit` → `place.inn` | priority 10, `when: 'user.visits >= 3'` | `run` |
| `dock.storm` | `scenes/dock/storm.lute` | `placeVisit` → `place.dock` | priority 50, `when: "run.slot == 'evening' && run.day == 2"` | `run` |
| `innQuiet`, `dockBo`, `dockGulls`, `dockEmpty` (entries) | `lore/places.lute` | `placeVisit` | see below | — |
| `noteFerry`, `noteFair`, `noteDocked` (entries) | `lore/places.lute` | `board` | see below | — |

The entries in `lore/places.lute`:

```lute
<entry id="innQuiet" on="placeVisit" target="place.inn" category="bark" when="!holds(present(ada, inn))">
  @narrator: The inn is quiet at this hour.
</entry>

<entry id="dockBo" on="placeVisit" target="place.dock" category="bark" when="holds(present(bo, dock))">
  @bo: Mind the nets.
</entry>

<entry id="dockGulls" on="placeVisit" target="place.dock" category="bark">
  @narrator: Gulls fight over a fish head.
</entry>

<entry id="dockEmpty" on="placeVisit" target="place.dock" category="bark" priority="-1">
  @narrator: Nobody on the pier.
</entry>

<entry id="noteFerry" on="board" category="note" title="Ferry times" when="run.day <= 2">
  @narrator: "Ferry leaves on the third day."
</entry>

<entry id="noteFair" on="board" category="note" title="The fair" priority="5">
  @narrator: "Fair on the green, all week."
</entry>

<entry id="noteDocked" on="board" category="note" title="The ferry is in" priority="10" when="holds(arrived(dock))">
  @narrator: "Ferry at the pier. Boarding now."
</entry>
```

And one quest, `quests/ferry.lute`, which starts once you have met Ada at the inn:

```lute
<quest id="ferry" title="Catch the ferry" start="visited('inn.ada')">
  <objective id="word" title="Win Ada's trust" done="holds(trusted(ada))"/>
  <objective id="board" title="Be at the dock on day three" done="run.day >= 3"/>
</quest>
```

## `lute beats`

```console
$ lute beats <dir> [--occasion <O>]… [--target <T>]… [--json]
```

Print every beat of the project, grouped into one **ladder** per occasion — and, for a targeted occasion, one per target — in selection order: priority descending, then project order (the order of `beats` in `project.index.json`). Each row carries what decides the beat's place and what `check-project` concludes about it:

```console
$ lute beats .
project root: .

  board — select: all
    #  priority  beat                          kind   once  verdict  after  when
    1  10        noteDocked "The ferry is in"  entry  no    -        -      holds(arrived(dock))
    2  5         noteFair "The fair"           entry  no    -        -      -
    3  0         noteFerry "Ferry times"       entry  no    -        -      run.day <= 2

  dayStart — select: sequence
    #  priority  beat                     kind   once  verdict  after               when
    1  10        day.bell "The bell"      scene  no    -        -                   -
    2  0         day.farewell             scene  run   -        completed("ferry")  -
    3  0         day.market "Market day"  scene  run   -        -                   run.day == 3

  placeVisit @ place.dock — select: first
    #  priority  beat        kind   once  verdict   after  when
    1  50        dock.storm  scene  run   -         -      run.slot == 'evening' && run.day == 2
    2  0         dockBo      entry  no    -         -      holds(present(bo, dock))
    3  0         dockGulls   entry  no    tied      -      -
    4  -1        dockEmpty   entry  no    shadowed  -      -

  placeVisit @ place.inn — select: first
    #  priority  beat                      kind   once  verdict        after               when
    1  30        inn.again                 scene  run   -              visited("inn.ada")  holds(trusted(ada))
    2  20        inn.ada "Ada at the inn"  scene  run   -              -                   holds(present(ada, inn))
    3  10        inn.regular               scene  run   once-run-user  -                   user.visits >= 3
    4  0         innQuiet                  entry  no    -              -                   !holds(present(ada, inn))
```

- **beat** — the id, followed by its `title` in quotes when it has one (a scene's `title:`, an entry's or bundle beat's `title=`).
- **kind** — `scene`, `entry`, or `bundle` (a [bundle beat](/tooling/play/#bundle-beats), listed under its canonical `<document id>.<beat id>`).
- **once** — `run`, `user`, or `no` (a scene's `once: false`, an entry without `once`); `, also` follows it for an [`also` beat](/tooling/play/#composing-occasions).
- **after** / **when** — the conditions as written, `when` with every [`@def`](/language/params/) expanded; `-` when absent.
- **verdict** — the project-wide beat diagnostics `check-project` reports about this beat, by name: `unreachable` (`E-BEAT-UNREACHABLE` / `E-ENTRY-UNREACHABLE`), `shadowed` (`W-BEAT-SHADOWED`), `tied` (`W-BEAT-PRIORITY-TIE`), and `once-run-user` (`W-BEAT-ONCE-RUN-USER`). They are the checker's own diagnostics from the same run, not a second analysis; see [Beats](/language/beats/) for what each means. Above, `dockGulls` ties `dockBo` (equal priority, conditions not provably exclusive), `dockEmpty` can never win because the always-eligible `dockGulls` outranks it, and `inn.regular` is spent once per run while its `when` reads only user state.

A targeted occasion gets one ladder per target its beats name; a beat with no `target` answers every raise, so it appears in every target's ladder. When no beat names a target, the occasion has a single ladder headed `(any target)` (`"anyTarget": true` in JSON). Shadowing and ties are `select: first` notions, so a `select: all` or `sequence` ladder never shows them.

**The project need not check clean.** The ladder is most useful exactly when something is wrong. With the storm's condition written as a contradiction, `check-project` fails with `E-BEAT-UNREACHABLE`, and the ladder still lists the beat, at the top of the dock, marked:

```console
$ lute beats . --target place.dock
project root: .

  placeVisit @ place.dock — select: first
    #  priority  beat        kind   once  verdict      after  when
    1  50        dock.storm  scene  run   unreachable  -      run.slot == 'morning' && run.slot == 'evening'
    2  0         dockBo      entry  no    -            -      holds(present(bo, dock))
    3  0         dockGulls   entry  no    tied         -      -
    4  -1        dockEmpty   entry  no    shadowed     -      -
```

`--occasion <O>` (repeatable) keeps only those occasions' ladders; `--target <T>` (repeatable) keeps only the ladders raised for that target, so untargeted occasions drop out. An occasion the project does not know is a usage error that lists the known ones; a target no beat names prints `(no beats)`:

```console
$ lute beats . --occasion daystart
lute beats: `--occasion daystart` is not an occasion of this project (known: board, dayStart, placeVisit)
```

`--json` emits `{ "roots": [ { "root", "ladders": [ { "occasion", "select", "target"? | "anyTarget"?, "beats": [ … ] } ] } ] }`. Each beat is `{ id, kind, document, priority, once, target?, also?, after?, when?, title?, verdicts }`, where `once` is `"run"`, `"user"`, or `"none"`, and `verdicts` carries the full diagnostics, each `{ code, severity, message }`:

```json
{
  "document": "lore/places.lute",
  "id": "dockEmpty",
  "kind": "entry",
  "once": "none",
  "priority": -1,
  "target": "place.dock",
  "verdicts": [
    {
      "code": "W-BEAT-SHADOWED",
      "message": "entry `dockEmpty` can never win occasion `placeVisit` for `place.dock`: entry `dockGulls` (priority 0) is ordered before it, is always eligible (no `after:`, and its `when` is absent or always true), and is never spent (an entry without `once`), so it wins every time (dsl 0.21.0 §5)",
      "severity": "warning"
    }
  ]
}
```

Exit **0** on success, **2** on an I/O failure or an unknown `--occasion`.

## `lute calendar`

```console
$ lute calendar <dir> [--axis <path>=<lo>..<hi> | <path>=<a>,<b>,…]…
                [--occasion <O>]… [--target <T>]…
                [--script <route.play.yaml> [--until <step>]] [--where <cel>]
                [--json | --csv]
```

Evaluate, for **every cell** of a grid of state values and every occasion column, which beats [`lute play`](/tooling/play/) would find eligible there — its own candidates, `once`, `after:` and `when`, with the project's rules applied — and report the winner. Every cell starts from the same world — the declared defaults, or a play script's save with its steps replayed — then gets its axis values written, and its quests settle. Nothing is presented within the grid: each cell is one question, "if the engine raised this occasion now, what would play?", and no cell sees what another did.

- `--axis` (optional, repeatable) names a declared state path and its values: an inclusive integer range `run.day=1..7`, or a list `run.slot=morning,evening`. Each value is checked against the path's declared type. The grid is the product of the axes, and the **first axis varies slowest**; with no `--axis` it is a single cell. Two more kinds of axis reach what a state write cannot — a quest's status and a fact — see [Quest and fact axes](#quest-and-fact-axes).
- `--occasion <O>` (repeatable) — the occasions to evaluate; by default every occasion a beat answers.
- `--target <T>` (repeatable) — the targets to raise a targeted occasion for; by default every target its beats name, not the rest of its declared [target domain](/tooling/play/#occasions), where no beat answers and every cell would read as a hole. When none of its beats names a target, the occasion gets a single column headed `(any)` (`<occasion>@(any)` in the lists below the grid), which only its untargeted beats answer. Each (occasion, target) pair is one column.
- `--script <file>` — a play script every cell starts from: its **save** — `state:`, `facts:`, `visited:`, `presented:`, `quests:`, `entriesRead:` ([Starting from a save](/tooling/play/#starting-from-a-save)) — and then its `steps:`, replayed exactly as `lute play` plays them. A save needs no steps. Without it, every cell starts from the declared defaults and the seed facts.
- `--until <step>` — with `--script`, replay only the steps before this one, named by its 1-based number or its `label:`; the step itself is not played. See [Along a route](#along-a-route).
- `--where <cel>` — keep only the cells where this condition holds; see [Dropping cells no run reaches](#dropping-cells-no-run-reaches).
- `--json` / `--csv` — machine-readable output instead of the grid.

Over the town's first three days:

```console
$ lute calendar . --axis run.day=1..3 --axis run.slot=morning,evening
calendar: . — 6 cell(s) × 4 column(s), from declared defaults

run.day  run.slot  board (all)          dayStart (sequence)   placeVisit  placeVisit
                                                              place.inn   place.dock
1        morning   noteFair, noteFerry  day.bell              innQuiet    dockBo +2
1        evening   noteFair, noteFerry  day.bell              inn.ada     dockGulls +1
2        morning   noteFair, noteFerry  day.bell              innQuiet    dockBo +2
2        evening   noteFair, noteFerry  day.bell              inn.ada     dock.storm +2
3        morning   noteFair             day.bell, day.market  innQuiet    dockGulls +1
3        evening   noteFair             day.bell, day.market  inn.ada     dockGulls +1

shadowed (eligible, not presented):
  run.day=1 run.slot=morning  placeVisit@place.dock: dockBo over dockGulls, dockEmpty
  run.day=1 run.slot=evening  placeVisit@place.dock: dockGulls over dockEmpty
  run.day=2 run.slot=morning  placeVisit@place.dock: dockBo over dockGulls, dockEmpty
  run.day=2 run.slot=evening  placeVisit@place.dock: dock.storm over dockGulls, dockEmpty
  run.day=3 run.slot=morning  placeVisit@place.dock: dockGulls over dockEmpty
  run.day=3 run.slot=evening  placeVisit@place.dock: dockGulls over dockEmpty

never eligible in any cell: 4
  noteDocked [entry, lore/places.lute] board — when: false
  day.farewell [scene, scenes/day/farewell.lute] dayStart — after: prerequisite not satisfied
  inn.again [scene, scenes/inn/again.lute] placeVisit@place.inn — after: prerequisite not satisfied
  inn.regular [scene, scenes/inn/regular.lute] placeVisit@place.inn — when: false
```

Reading a cell:

- A `select: first` column shows the **winner**, followed by `+N` when `N` more beats were eligible but outranked; the `shadowed` block below the grid names them, winner first. A `-` is an empty cell: nothing is eligible, so the occasion would pass with no story — exactly the holes a calendar exists to find.
- A `select: all` column shows the **offered list**, and a `select: sequence` column the **sequence** that would play, each in selection order.
- `?` marks an **undecided** cell — a `when` the reference runtime cannot decide (a `validAt(…)`, say) decides the outcome, so `lute play` would halt there. The reasons are listed under `undecided`.

Here the dock is never empty — `dockGulls` covers every cell Bo is not on the pier — while the storm takes over the second evening. The rules move Ada: she is at the inn only in the evening. And the calendar lists the beats **never eligible in any cell**, with the reason: `day.farewell` and `inn.again` wait on history (`after:`) that a fresh start does not have, `inn.regular` needs three visits, and `noteDocked` a fact only the engine asserts.

### From a save

History is what the axes cannot vary, so `--script` supplies it. A save in which the player met Ada on an earlier evening, three visits in, needs no steps:

```yaml
# A save: Ada met on an earlier evening, three visits in all. No steps.
visited: [inn.ada]
facts: ["met(ada)"]
state: { user.visits: 3 }
```

```console
$ lute calendar . --axis run.day=1..3 --axis run.slot=morning,evening \
    --script plays/regular.play.yaml --occasion dayStart --occasion placeVisit --target place.inn
calendar: . — 6 cell(s) × 2 column(s), from the save in plays/regular.play.yaml

run.day  run.slot  dayStart (sequence)                 placeVisit
                                                       place.inn
1        morning   day.bell                            inn.again +2
1        evening   day.bell                            inn.again +2
2        morning   day.bell                            inn.again +2
2        evening   day.bell                            inn.again +2
3        morning   day.bell, day.farewell, day.market  inn.again +2
3        evening   day.bell, day.farewell, day.market  inn.again +2

shadowed (eligible, not presented):
  run.day=1 run.slot=morning  placeVisit@place.inn: inn.again over inn.regular, innQuiet
  run.day=1 run.slot=evening  placeVisit@place.inn: inn.again over inn.ada, inn.regular
  run.day=2 run.slot=morning  placeVisit@place.inn: inn.again over inn.regular, innQuiet
  run.day=2 run.slot=evening  placeVisit@place.inn: inn.again over inn.ada, inn.regular
  run.day=3 run.slot=morning  placeVisit@place.inn: inn.again over inn.regular, innQuiet
  run.day=3 run.slot=evening  placeVisit@place.inn: inn.again over inn.ada, inn.regular

never eligible in any cell: none
```

The visit opens `inn.again`'s `after:`, and `met(ada)` makes `trusted(ada)` derive, so Ada's second scene wins the inn in every cell. The quests **settle in every cell**, after the cell's values are written: the visit starts `ferry`, and from day 3 its objectives both hold, so the quest is complete there and `day.farewell` (`after: completed("ferry")`) joins the morning sequence. A cell whose quest settle halts — an objective the reference runtime cannot decide — carries a note, listed under `notes:` after the grid, and so does a cell where the settle moves an axis value away from what the cell wrote: a handler's write to that path, or a seeded quest status the lifecycle moves on (see [below](#quest-and-fact-axes)).

### Along a route

A save is history written down by hand. A route is history played: when the script has `steps:`, every cell starts from the world those steps leave, replayed exactly as `lute play` plays them — presentations, `once` spent, facts asserted, quests advanced. `--until <step>` stops the replay before a step, named by its number or its `label:`, so the grid shows what that step would find. This route spends an evening at the inn and comes back:

```yaml
# plays/evening.play.yaml
state: { run.slot: evening }
steps:
  - occasion: placeVisit
    target: place.inn
  - label: back at the inn
    occasion: placeVisit
    target: place.inn
```

Stopping before the way back, and asking whether a rumour about Ada would change it:

```console
$ lute calendar . --script plays/evening.play.yaml --until "back at the inn" \
    --axis 'holds(rumor(ada))=false,true' --occasion placeVisit --target place.inn
calendar: . — 2 cell(s) × 1 column(s), from the save in plays/evening.play.yaml, then its step 1 replayed (stopping before step 2, back at the inn)

holds(rumor(ada))  placeVisit
                   place.inn
false              inn.again
true               -

never eligible in any cell: 3
  innQuiet [entry, lore/places.lute] placeVisit@place.inn — when: false
  inn.ada [scene, scenes/inn/ada.lute] placeVisit@place.inn — once: run — already presented this run
  inn.regular [scene, scenes/inn/regular.lute] placeVisit@place.inn — when: false
```

The replayed step presents `inn.ada`, which asserts `met(ada)` and starts `ferry`, and spends it for the run. Without a rumour, Ada trusts you and `inn.again` wins the way back; with one, nothing at the inn is eligible — a hole the rumour opens. The header names where the cells start; `--json` carries the same text as `from`.

`--until` needs `--script`. A step number the script does not have, or a label it does not declare, is a usage error — a label gets a did-you-mean — and so is a replay that halts (an unscripted choice, say): the calendar cannot say what a route reaches if it does not play.

### Quest and fact axes

An axis over a declared state path writes it as an `engine:` step would. Two paths need more than a write, and each has an axis of its own:

- `quest.<id>.state=unset,active,complete,failed` seeds the quest's **status**, as a save's `quests:` does; a plain write would be overwritten by the lifecycle the cell settles. `unset` and `active` also clear the objective progress a replayed route made, since the axis names a status, not the route's objectives. `quest.<id>.objectives.<oid>.done=false,true` is taken as written, as a save's objective progress. Any other `quest.*` path — `activatedAt`, say — is the lifecycle's own bookkeeping and a usage error, and so is an id no quest declares.
- `holds(<fact>)=true,false` asserts (`true`) or retracts (`false`) a base fact before the rules derive, so derived facts follow it. The fact must be ground, of a declared relation, with members of its domains; a derived relation is refused, since the rules decide it. Quote the axis in the shell: `--axis 'holds(rumor(ada))=false,true'`.

The status is a seed, and the settle still runs. From the save above — Ada met, `inn.ada` visited — a quest seeded `unset` does not stay there:

```console
$ lute calendar . --script plays/regular.play.yaml --axis quest.ferry.state=unset,active --occasion dayStart
calendar: . — 2 cell(s) × 1 column(s), from the save in plays/regular.play.yaml

quest.ferry.state  dayStart (sequence)
unset              day.bell
active             day.bell

notes:
  quest.ferry.state=unset: quest.ferry.state settled to active

never eligible in any cell: 2
  day.farewell [scene, scenes/day/farewell.lute] dayStart — after: prerequisite not satisfied
  day.market [scene, scenes/day/market.lute] dayStart — when: false
```

`ferry` starts on `visited('inn.ada')`, which the save holds, so the settle activates it and the note says so.

### Dropping cells no run reaches

Axes are independent, so their product holds combinations no run can reach. `--where <cel>` keeps only the cells where a condition holds — evaluated after the cell's values are written and its quests settle — and the header counts the rest. `ferry` completes only from day 3, so a completed ferry before then is not a state to plan for:

```console
$ lute calendar . --axis run.day=1..3 --axis quest.ferry.state=unset,active,complete \
    --where "quest.ferry.state != 'complete' || run.day >= 3" --occasion dayStart
calendar: . — 7 cell(s) (2 dropped by --where) × 1 column(s), from declared defaults

run.day  quest.ferry.state  dayStart (sequence)
1        unset              day.bell
1        active             day.bell
2        unset              day.bell
2        active             day.bell
3        unset              day.bell, day.market
3        active             day.bell, day.market
3        complete           day.bell, day.farewell, day.market

never eligible in any cell: none
```

`--where` is plain CEL: an `@def` call is not expanded there. A condition that does not parse is a usage error before anything is evaluated, and one that evaluates unknown at some cell — it reads a path that cell leaves undecided — is a usage error naming the cell, since a cell is never dropped, or kept, on a guess. `--json` counts the dropped cells as `pruned`.

### Undecided cells

A cell is `?` when the reference runtime cannot decide a `when` that would decide it. Add a scene at the inn, priority 40, whose `when` is `validAt(met(ada), quest.ferry.activatedAt)` — a historical query `lute play` has no clock for — and the inn column cannot be answered:

```console
$ lute calendar . --axis run.slot=morning,evening --occasion placeVisit --target place.inn
calendar: . — 2 cell(s) × 1 column(s), from declared defaults

run.slot  placeVisit
          place.inn
morning   ? +1
evening   ? +1
```

and the `undecided` block says why, per cell:

```
undecided (an unknown `when` decides the cell; play halts there):
  run.slot=morning  placeVisit@place.inn: inn.oldFriend — `validAt(met(ada), quest.ferry.activatedAt)` evaluates unknown: now()/validAt(...) has no reference-runtime resolution
  run.slot=evening  placeVisit@place.inn: inn.oldFriend — `validAt(met(ada), quest.ferry.activatedAt)` evaluates unknown: now()/validAt(...) has no reference-runtime resolution
```

An unknown `when` below the winner does not make a cell undecided: the winner is presented whatever it evaluates to.

### Machine-readable output

`--json` emits where the cells start (`from`, the header's text), how many cells `--where` dropped (`pruned`), the axes, the columns, one record per cell, and the never-eligible list. A column of a targeted occasion carries `target`, or `"anyTarget": true` for the `(any)` column. A cell's `results` hold one entry per column: `winner` (`null` on a `select: all` or `sequence` column, or when nothing is eligible), `presented` (the winner, or the whole offered or sequence list), `shadowed`, and, when a `when` evaluated unknown, `unknown` (`[{ id, reason }]`), with `"undecided": true` when that unknown decides the cell. A cell carries `notes` — a list — when its quest settle halted or moved an axis value:

```json
{
  "axes": [ { "path": "run.day", "values": [3] }, { "path": "run.slot", "values": ["morning"] } ],
  "cells": [
    {
      "at": { "run.day": 3, "run.slot": "morning" },
      "results": [
        { "occasion": "board", "select": "all", "winner": null, "presented": ["noteFair"], "shadowed": [] },
        { "occasion": "dayStart", "select": "sequence", "winner": null, "presented": ["day.bell", "day.market"], "shadowed": [] },
        { "occasion": "placeVisit", "target": "place.inn", "select": "first", "winner": "innQuiet", "presented": ["innQuiet"], "shadowed": [] },
        { "occasion": "placeVisit", "target": "place.dock", "select": "first", "winner": "dockGulls", "presented": ["dockGulls"], "shadowed": ["dockEmpty"] }
      ]
    }
  ],
  "columns": [ { "occasion": "board", "select": "all" }, … ],
  "from": "declared defaults",
  "neverEligible": [
    { "id": "dockBo", "kind": "entry", "document": "lore/places.lute", "on": "placeVisit", "target": "place.dock", "reasons": ["when: false"] },
    …
  ],
  "pruned": 0
}
```

`--csv` writes one row per cell and column — the axis values, then `occasion,target,select,winner,presented,shadowed,unknown,notes`, lists joined with `;` (an `(any)` column's target reads `(any)`) — for a spreadsheet:

```console
$ lute calendar . --axis run.day=2..3 --axis run.slot=evening --occasion placeVisit --csv
run.day,run.slot,occasion,target,select,winner,presented,shadowed,unknown,notes
2,evening,placeVisit,place.inn,first,inn.ada,inn.ada,,,
2,evening,placeVisit,place.dock,first,dock.storm,dock.storm,dockGulls;dockEmpty,,
3,evening,placeVisit,place.inn,first,inn.ada,inn.ada,,,
3,evening,placeVisit,place.dock,first,dockGulls,dockGulls,dockEmpty,,
```

### Limits and exit codes

The calendar compiles the whole project the way `lute play` does, so, unlike `lute beats`, it needs a project that compiles. With the contradictory storm from [above](#lute-beats):

<!-- lute-diagnostics -->
```console
$ lute calendar . --axis run.day=1..3
./scenes/dock/storm.lute:6:8: error [E-BEAT-UNREACHABLE] beat `dock.storm` is never eligible: its `when` `run.slot == 'morning' && run.slot == 'evening'` is provably false (dsl 0.21.0 §5)
```

Standard error adds `lute play: 1 of 9 document(s) failed to compile; refusing to play`, and the exit is **1**.

Every axis is validated before a cell is evaluated. An undeclared path, a value outside the path's type, an empty or non-integer range, an axis given twice, a quest path other than a status or an objective's `done`, an undeclared quest, a `holds(…)` over a derived or undeclared relation (or with a value other than `true`/`false`), a `--target` that none of the listed occasions takes, and a grid of more than **10,000 cells** are usage errors, as are an `--until` that names no step, a replay that halts, and a `--where` that does not parse or is unknown at some cell:

```console
$ lute calendar . --axis run.slot=noon
lute calendar: `--axis run.slot`: `run.slot` does not take `noon`: the enum's members are morning, evening
$ lute calendar . --axis quest.ferry.activatedAt=1
lute calendar: `--axis quest.ferry.activatedAt`: `quest.ferry.activatedAt` is the quest lifecycle's own bookkeeping and cannot be set per cell — an axis over a quest is `quest.<id>.state` (its status) or `quest.<id>.objectives.<oid>.done`
$ lute calendar . --script plays/evening.play.yaml --until "back at the ink"
lute calendar: plays/evening.play.yaml: `--until back at the ink` names no step number or `label:` of the script — did you mean `back at the inn`?
$ lute calendar . --axis run.day=1..200 --axis user.visits=0..60
lute calendar: the axes' product exceeds 10000 cells
```

Exit **0** on success, **1** when the project does not compile, **2** on an I/O or usage failure.

## `lute scenario knowledge`

```console
$ lute scenario <dir> [--format text|json] knowledge [--for <node>]
```

List every beat, entry, and quest objective whose condition queries a fact — `holds(…)`, `count(…)`, or `validAt(…)` — and trace each queried atom back to what can produce it. A derived atom is followed **through the rules**, with the rule head's variables bound to the atom's constants, so each premise is shown as the ground atom it needs; a base atom ends at its producers:

- **asserted by** — the scenes (by scene key), quests, entries and bundle beats (by canonical id `<document id>.<beat id>`) whose `::assert` can produce it, with their documents;
- **seed facts** — the schema's `facts:` that match it;
- **reserved** — a `reserved: true` relation: the engine asserts it;
- **NO PRODUCER** — nothing asserts it, no seed fact, not reserved, no rule. The condition is waiting on content no one has written.

```console
$ lute scenario . knowledge
project root: .
  knowledge (fact-guarded condition -> relations read -> producers):

  entry `innQuiet` (lore/places.lute)
    when: !holds(present(ada, inn))
    present(ada, inn) — derived by 1 rule
      rule: present(ada, inn) :- cel("run.slot == 'evening'")

  entry `dockBo` (lore/places.lute)
    when: holds(present(bo, dock))
    present(bo, dock) — derived by 1 rule
      rule: present(P, dock) :- works(P, dock), cel("run.slot == 'morning' && run.day != 3")
        works(bo, dock) — seed facts works(bo, dock)

  entry `noteDocked` (lore/places.lute)
    when: holds(arrived(dock))
    arrived(dock) — reserved — the engine asserts it

  objective `ferry.word` (quests/ferry.lute)
    done: holds(trusted(ada))
    trusted(ada) — derived by 1 rule
      rule: trusted(P) :- met(P), not rumor(P)
        met(ada) — asserted by scene `inn.ada` (scenes/inn/ada.lute)
        not rumor(ada) — NO PRODUCER — nothing asserts it, no seed fact, not reserved, no rule

  scene `inn.ada` (scenes/inn/ada.lute)
    when: holds(present(ada, inn))
    present(ada, inn) — derived by 1 rule
      rule: present(ada, inn) :- cel("run.slot == 'evening'")

  scene `inn.again` (scenes/inn/again.lute)
    when: holds(trusted(ada))
    trusted(ada) — derived by 1 rule
      rule: trusted(P) :- met(P), not rumor(P)
        met(ada) — asserted by scene `inn.ada` (scenes/inn/ada.lute)
        not rumor(ada) — NO PRODUCER — nothing asserts it, no seed fact, not reserved, no rule
```

Only the rules that can conclude the queried atom are followed: `dockBo` asks for `present(bo, dock)`, so the rule that places Ada at the inn is not listed under it. `not rumor(ada)` has no producer, so the negation always holds today — the rumour that would make Ada distrust you is a relation declared and never written. Under [`check-project --wip`](/tooling/cli/#check-project), a guard that is dead only because of such a relation is a warning, not an error.

A negated premise the project **can** make false is followed by what would do it. The rule body is instantiated against every fact that may hold in some run (the may set `check-project` decides guards with, here counting every assert site as live), and each fact that matches the negated atom is listed with its producer — or, when it is itself derived, with the premises of one rule instance that derives it. Once a bundle beat writes the rumour:

```console
        not rumor(ada) — asserted by beat `town.gossip.whisper` (lore/gossip.lute)
          can be defeated by rumor(ada) [beat `town.gossip.whisper` (lore/gossip.lute)]
```

A derived defeater names the facts that join to produce it — the clue and the seed that break a deduction:

```console
            not alibi(solt, _) — derived by 1 rule
              can be defeated by alibi(solt, tunnel) ⇐ seen(solt, corridor, tunnel) [entry `mirelaMatch` (lore/talk.lute)], away(corridor) [seed]
```

At most three defeaters are printed per premise, then a count. A negated relation that may hold any tuple (an open argument domain) prints `may be defeated: …` instead.

`--for <node>` selects one element: a scene key (`inn.again`), an entry id (`dockBo`), a quest objective as `<quest>.<objective>` (`ferry.word`), or `quest:<id>` for every objective of a quest. A node that names no fact-guarded element is a usage error (exit 2), with a did-you-mean when one is close:

```console
$ lute scenario . knowledge --for ferry.wrod
lute scenario knowledge: `--for ferry.wrod` names no fact-guarded beat, entry or objective — did you mean `ferry.word`?
```

`--format json` (given before the subcommand, like every `scenario` option) emits `{ "roots": [ { "root", "elements", "relations" } ] }`. Each element is `{ node, document, conditions, reads }` — `conditions` maps the slot (`when`, `done`, …) to its expanded text, and `reads` lists the atoms it queries. `relations` maps each relation involved to `{ declared, derived, reserved, seedFacts, assertedBy, rules }`, each rule `{ rule, premises: [{ relation, negated? }] }`. `--format dot` is refused: the knowledge map is not a graph.

Exit **0** on success, **2** on an I/O failure or an unmatched `--for`.

### What the scenario graph leaves out

The [scenario graph](/connectivity/scene-graph/) draws only prerequisites that connectivity analyzes, and a quest joins it only by declaring `after=`. Since 0.23.0 the bare `lute scenario` view says which references it therefore did not draw — a `completed()`/`active()` in some `after:` that names a quest with no `after=`, and a `visited()` read in such a quest's `start`, `fail`, objective `done`/`by`, or `when`:

```console
$ lute scenario .
project root: .
  topological layers:
    layer 0: scene(day.bell), scene(day.farewell), scene(day.market), scene(dock.storm), scene(inn.ada), scene(inn.regular)
    layer 1: scene(inn.again)
  edges (prerequisite -> dependent) [atom kind(s)]:
    scene(inn.ada) -> scene(inn.again) [visited]
  unanchored (no `after` — available from the start of play; no prerequisites in this graph):
    quest(ferry)
  note: 2 `visited()`/`completed()`/`active()` reference(s) not drawn — a quest joins this graph only by declaring `after` (even `after=""`):
    scene(day.farewell) -> completed("ferry") — quest(ferry) declares no `after`
    quest(ferry) reads visited('inn.ada') — quest(ferry) declares no `after`
```

`--format json` carries the same list as `omitted` on the root, each `{ from, kind, quest }` for a `completed`/`active` reference or `{ from, kind: "visited", scene }` for a quest's `visited()` read. Give `ferry` an `after=""` and both references become edges.

Lore entries are not graph nodes, but bundle beats are. Add the gossip beat from [above](#lute-scenario-knowledge) and layer 0 ends with `beat(town.gossip.whisper)`, an entry node with no edges: a `<beat>` declares no `after`, and its occasion, target and `when` decide when it plays. `lute scenario . reach town.gossip.whisper` (or `reach beat:town.gossip.whisper`) prints the file that declares it with its `on`, `target` and `when`; see [The scene graph](/connectivity/scene-graph/#bundle-beats).

## Beat rows in `project.index.json`

The overviews read the same beat table an engine does. Since 0.23.0, a beat row in [`project.index.json`](/tooling/cli/#--all--project-wide-compile-and-index) carries the beat's `when` — with every `@def` expanded, so an engine or a tool can show it without the schema — and its `title`, the label a `select: all` menu shows. Both are omitted when the beat has none, so a project that uses neither compiles byte-identically. A [bundle beat](/tooling/play/#bundle-beats)'s row has kind `bundle`.

```json
{"id": "noteFerry", "kind": "entry", "document": "lore/places.lute", "on": "board", "priority": 0, "when": "run.day <= 2", "title": "Ferry times"}
{"id": "day.market", "kind": "scene", "document": "scenes/day/market.lute", "on": "dayStart", "priority": 0, "once": "run", "when": "run.day == 3", "title": "Market day"}
{"id": "dock.storm", "kind": "scene", "document": "scenes/dock/storm.lute", "on": "placeVisit", "target": "place.dock", "priority": 50, "once": "run", "when": "run.slot == 'evening' && run.day == 2"}
```
