---
title: Story overviews
description: "Three read-only views of a story chosen by occasions (dsl 0.23.0): `lute beats` prints each occasion's beat ladder with the checker's verdicts, `lute calendar` evaluates play's own eligibility over a grid of state values, and `lute scenario knowledge` traces every fact-guarded condition through the rules to whatever produces its facts. Since 0.24.0: clock and visited axes, per-occasion axes, `--facts` tables and the never-presented list in the calendar, and defs as written with `--expand`. Since 0.26.0: covered fallbacks and kind ladders in `lute beats`, fact-producer edges in `lute scenario --facts`, and counted rule premises in `knowledge`."
---

Once a story is selected by [occasions](/tooling/play/) rather than read top to bottom, no single file answers "what plays at the inn on the evening of day two?". The answer is spread over every beat's `on`, `target`, `priority`, `once`, `after:` and `when`, over the project's rules, and over whatever the save already holds. Since dsl 0.23.0 three commands put it on one screen:

- [`lute beats`](#lute-beats) — the **ladder**: every beat of each occasion and target in selection order, with what `check-project` concludes about it. Static; the project need not check clean.
- [`lute calendar`](#lute-calendar) — the **grid**: for every combination of the state values, quest statuses and facts you name — from a save, or partway along a played route — which beat `lute play` would present. It runs play's own eligibility, so it is exact where the checker is conservative.
- [`lute scenario knowledge`](#lute-scenario-knowledge) — the **knowledge map**: for every condition that reads a fact, which rules conclude it and who produces the facts they need, down to the ones nothing produces yet.

All three are read-only. The normative text is §1 of the [0.23.0 proposal](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md).

Since dsl 0.26.0 the views follow a project several authors write at once: `lute beats` marks a fallback another beat always covers and lists a [kind beat](/tooling/play/#kind-targets) for every member it answers, [`lute scenario --facts`](#fact-edges-lute-scenario---facts) draws which scene's facts open which gate — progress an `after:` graph cannot see — and [`lute refs`](/tooling/cli/#refs) lists who gives what: every value of a directive attribute or reward target, with the documents that use it.

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
$ lute beats <dir> [--occasion <O>]… [--target <T>]… [--expand] [--json]
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
- **once** — `run`, `user`, `day`, `slot` (with a declared [clock](/language/clock/), for a scene, an entry, or a bundle beat's `once="day"` / `once="slot"`), or `no` (a scene's `once: false`, an entry without `once`); `, also` follows it for an [`also` beat](/tooling/play/#composing-occasions), and `, share <key>` for a beat with a [shared spend](/language/beats/#one-event-several-places-share) (dsl 0.25.0): `day, share solWarm`.
- **after** / **when** — the conditions as the author wrote them, `@def` references included (dsl 0.24.0; `--expand` prints each `when` expanded); `-` when absent.
- **verdict** — the project-wide beat diagnostics `check-project` reports about this beat, by name: `unreachable` (`E-BEAT-UNREACHABLE` / `E-ENTRY-UNREACHABLE`), `shadowed` (`W-BEAT-SHADOWED`), `tied` (`W-BEAT-PRIORITY-TIE`), and `once-run-user` (`W-BEAT-ONCE-RUN-USER`). They are the checker's own diagnostics from the same run, not a second analysis; see [Beats](/language/beats/) for what each means. Above, `dockGulls` ties `dockBo` (equal priority, conditions not provably exclusive), `dockEmpty` can never win because the always-eligible `dockGulls` outranks it, and `inn.regular` is spent once per run while its `when` reads only user state.

A targeted occasion gets one ladder per target its beats name; a beat with no `target` answers every raise, so it appears in every target's ladder. When no beat names a target, the occasion has a single ladder headed `(any target)` (`"anyTarget": true` in JSON). Shadowing and ties are `select: first` notions, so a `select: all` or `sequence` ladder never shows them.

**Covered fallbacks** (dsl 0.26.0 §8). A fallback that an earlier, never-spent beat whose `when` it implies always beats can never play, yet it is not shadowed — the beat above it is not always eligible. It is there on purpose (a lead's gate line, kept until an area answers the gate), so the verdict column says what covers it instead of warning: `covered by <id>` (`coveredBy` in `--json`). An area's `solGate` (priority 0) and the lead's `gateFallback` (priority -10) share the `when` `!holds(metWren(wren))`, and entries without `once` are never spent. A [kind beat](/tooling/play/#kind-targets) (dsl 0.26.0 §5) is listed in the ladder of every member some beat names — Gus's own beat outranks it there — and in a `kind:<kind>` ladder for the members no beat names on its own:

```console
$ lute beats . --occasion talk
project root: .

  talk @ npc.gus — select: first
    #  priority  beat                                       kind    once  verdict  after  when
    1  0         trainers.gusRematch "Gus wants a rematch"  bundle  no    -        -      -
    2  0         trainers.challenge "A trainer squares up"  bundle  no    -        -      -

  talk @ npc.sol — select: first
    #  priority  beat          kind   once  verdict             after  when
    1  0         solGate       entry  no    -                   -      !holds(metWren(wren))
    2  -10       gateFallback  entry  no    covered by solGate  -      !holds(metWren(wren))

  talk @ npc.wren — select: first
    #  priority  beat                          kind   once  verdict  after  when
    1  0         wren.talk "Wren at the pier"  scene  no    -        -      -

  talk @ kind:trainer — select: first
    #  priority  beat                                       kind    once  verdict  after  when
    1  0         trainers.challenge "A trainer squares up"  bundle  no    -        -      -
```

`--target` accepts any member of the occasion's domain: `lute beats . --occasion talk --target npc.r16Gus` prints the one ladder `talk @ npc.r16Gus`, the kind beat alone. On Monster League the twelve `covered by` rows are the lead's gate fallbacks, each covered by the area beat that answers its gate.

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

`--json` emits `{ "roots": [ { "root", "ladders": [ { "occasion", "select", "target"? | "anyTarget"?, "beats": [ … ] } ] } ] }`. Each beat is `{ id, kind, document, priority, once, share?, target?, also?, after?, when?, whenAuthored?, title?, coveredBy?, verdicts }`, where `once` is `"run"`, `"user"`, `"day"`, `"slot"`, or `"none"`, `share` the beat's shared-spend key (dsl 0.25.0), `when` carries the expansion and `whenAuthored` the author's text (dsl 0.24.0), `coveredBy` the id of the beat that always beats it (dsl 0.26.0), and `verdicts` carries the full diagnostics, each `{ code, severity, message }`; a kind ladder's `target` is `"kind:<kind>"`:

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

**Defs as written** (dsl 0.24.0). The `when` column shows a def reference the way the author wrote it; `--expand` prints the expansion instead. In the [`beats` scaffold](/tooling/play/#worked-example), `hub.morning` is gated on `!@firstDay`:

```console
$ lute beats . --occasion hubVisit
project root: .

  hubVisit — select: first
    #  priority  beat                           kind   once  verdict  after  when
    1  10        hub.welcome "First arrival"    scene  user  -        -      -
    2  0         hub.morning "Another morning"  scene  no    -        -      !@firstDay
$ lute beats . --occasion hubVisit --expand
project root: .

  hubVisit — select: first
    #  priority  beat                           kind   once  verdict  after  when
    1  10        hub.welcome "First arrival"    scene  user  -        -      -
    2  0         hub.morning "Another morning"  scene  no    -        -      !(run.day == 1)
```

Exit **0** on success, **2** on an I/O failure or an unknown `--occasion`.

## `lute calendar`

```console
$ lute calendar <dir> [--axis <path>=<lo>..<hi> | <path>=<a>,<b>,… | clock[=<d1>..<d2>]]…
                [--occasion <O>[@<axis>[=<value>],…]]… [--target <T>]… [--facts <relation>]…
                [--script <route.play.yaml> [--until <step>]] [--where <cel>]
                [--json | --csv]
```

Evaluate, for **every cell** of a grid of state values and every occasion column, which beats [`lute play`](/tooling/play/) would find eligible there — its own candidates, `once`, `after:` and `when`, with the project's rules applied — and report the winner. Every cell starts from the same world — the declared defaults, or a play script's save with its steps replayed — then gets its axis values written, and its quests settle. Nothing is presented within the grid: each cell is one question, "if the engine raised this occasion now, what would play?", and no cell sees what another did.

- `--axis` (optional, repeatable) names a declared state path and its values: an inclusive integer range `run.day=1..7`, or a list `run.slot=morning,evening`. Each value is checked against the path's declared type. The grid is the product of the axes, and the **first axis varies slowest**; with no `--axis` it is a single cell. More kinds of axis reach what a state write cannot — a quest's status, a fact, the visited set, a declared clock — see [Quest, fact, visited and clock axes](#quest-and-fact-axes).
- `--occasion <O>` (repeatable) — the occasions to evaluate; by default every occasion a beat answers. `<O>@<axis>,…` varies only some axes for that occasion — see [Per-occasion axes and who is where](#per-occasion-axes-and-who-is-where).
- `--facts <relation>` (repeatable, dsl 0.24.0) — print the relation's facts in every cell, once it has settled.
- `--target <T>` (repeatable) — the targets to raise a targeted occasion for; by default every target its beats name, not the rest of its declared [target domain](/tooling/play/#occasions), where no beat answers and every cell would read as a hole. A [kind beat](/tooling/play/#kind-targets) (dsl 0.26.0) names every member of its kind, so each member gets a column, and a cell offers it at that member's raise, after the member's own beats of its priority. When none of its beats names a target, the occasion gets a single column headed `(any)` (`<occasion>@(any)` in the lists below the grid), which only its untargeted beats answer. Each (occasion, target) pair is one column.
- `--script <file>` — a play script every cell starts from: its **save** — `state:`, `facts:`, `visited:`, `presented:`, `quests:`, `entriesRead:` ([Starting from a save](/tooling/play/#starting-from-a-save)) — and then its `steps:`, replayed exactly as `lute play` plays them. A save needs no steps. Without it, every cell starts from the declared defaults and the seed facts.
- `--until <step>` — with `--script`, replay only the steps before this one, named by its 1-based number or its `label:`; the step itself is not played. See [Along a route](#along-a-route).
- `--where <cel>` — keep only the cells where this condition holds; see [Dropping cells no run reaches](#dropping-cells-no-run-reaches).
- `--json` / `--csv` — machine-readable output instead of the grid.

Over the town's first three days:

```console
$ lute calendar . --axis run.day=1..3 --axis run.slot=morning,evening
calendar: . — 6 cell(s) × 4 column(s), from declared defaults

run.day  run.slot  board (all)          dayStart (sequence)   placeVisit     placeVisit
                                                              place.dock     place.inn
1        morning   noteFair, noteFerry  day.bell              dockBo +2      innQuiet
1        evening   noteFair, noteFerry  day.bell              dockGulls +1   inn.ada
2        morning   noteFair, noteFerry  day.bell              dockBo +2      innQuiet
2        evening   noteFair, noteFerry  day.bell              dock.storm +2  inn.ada
3        morning   noteFair             day.bell, day.market  dockGulls +1   innQuiet
3        evening   noteFair             day.bell, day.market  dockGulls +1   inn.ada

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

eligible but never presented in any cell: 1
  dockEmpty [entry, lore/places.lute] placeVisit@place.dock — lost to dock.storm; dockBo; dockGulls

Reading a cell:

- A `select: first` column shows the **winner**, followed by `+N` when `N` more beats were eligible but outranked; the `shadowed` block below the grid names them, winner first. A `-` is an empty cell: nothing is eligible, so the occasion would pass with no story — exactly the holes a calendar exists to find.
- A `select: all` column shows the **offered list**, and a `select: sequence` column the **sequence** that would play, each in selection order.
- `?` marks an **undecided** cell — a `when` the reference runtime cannot decide (a `validAt(…)`, say) decides the outcome, so `lute play` would halt there. The reasons are listed under `undecided`.

Here the dock is never empty — `dockGulls` covers every cell Bo is not on the pier — while the storm takes over the second evening. The rules move Ada: she is at the inn only in the evening. And the calendar lists the beats **never eligible in any cell**, with the reason: `day.farewell` and `inn.again` wait on history (`after:`) that a fresh start does not have, `inn.regular` needs three visits, and `noteDocked` a fact only the engine asserts. Since dsl 0.24.0 it also lists the beats **eligible but never presented** — eligible somewhere, beaten everywhere — with what beat them: `dockEmpty` is always outranked (`--json`: `neverPresented`, each with `beatenBy`; `--csv`: a second table).

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

eligible but never presented in any cell: 3
  innQuiet [entry, lore/places.lute] placeVisit@place.inn — lost to inn.again
  inn.ada [scene, scenes/inn/ada.lute] placeVisit@place.inn — lost to inn.again
  inn.regular [scene, scenes/inn/regular.lute] placeVisit@place.inn — lost to inn.again
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

eligible but never presented in any cell: none
```

The replayed step presents `inn.ada`, which asserts `met(ada)` and starts `ferry`, and spends it for the run. Without a rumour, Ada trusts you and `inn.again` wins the way back; with one, nothing at the inn is eligible — a hole the rumour opens. The header names where the cells start; `--json` carries the same text as `from`.

`--until` needs `--script`. A step number the script does not have, or a label it does not declare, is a usage error — a label gets a did-you-mean — and so is a replay that halts (an unscripted choice, say): the calendar cannot say what a route reaches if it does not play.

### Quest and fact axes

An axis over a declared state path writes it as an `engine:` step would. Some things need more than a write, and each has an axis of its own:

- `quest.<id>.state=unset,active,complete,failed` seeds the quest's **status**, as a save's `quests:` does; a plain write would be overwritten by the lifecycle the cell settles. `unset` and `active` also clear the objective progress a replayed route made, since the axis names a status, not the route's objectives. `quest.<id>.objectives.<oid>.done=false,true` is taken as written, as a save's objective progress. Any other `quest.*` path — `activatedAt`, say — is the lifecycle's own bookkeeping and a usage error, and so is an id no quest declares.
- `holds(<fact>)=true,false` asserts (`true`) or retracts (`false`) a base fact before the rules derive, so derived facts follow it. The fact must be ground, of a declared relation, with members of its domains; a derived relation is refused, since the rules decide it. Quote the axis in the shell: `--axis 'holds(rumor(ada))=false,true'`.
- `visited('<id>')=true,false` (dsl 0.24.0) puts a scene or bundle beat in or out of the cell's visited set, so a beat behind `after: visited(…)` can be read both ways without a route.
- `clock=<d1>..<d2>` (dsl 0.24.0 §1) walks a declared clock: every slot of each day, in clock order — `clock=1..2` over `slots: [morning, afternoon, night]` is six cells, `1 Mon morning` through `2 Tue night` (the weekday label when the clock's `week:` has labels). Each cell writes the clock's `day` and `slot` paths, so `clock.index` and `clock.weekday` read that position. Bare `--axis clock` is one week from day 1 (day 1 alone without a `week:`). The clock axis cannot sit beside an axis over its own `day` or `slot` path, and a project without a clock refuses it.

Anything else is a usage error that lists the axis kinds (with a did-you-mean for a mistyped state path):

```console
$ lute calendar . --axis run.dya=1..3
lute calendar: `--axis run.dya`: `run.dya` is not a declared state path in this project — did you mean `run.day`?; an axis is one of: a declared state path (`run.day=1..7`), `quest.<id>.state=<status>,…`, `quest.<id>.objectives.<oid>.done=true,false`, `holds(<fact>)=true,false`, `visited('<scene or bundle-beat id>')=true,false`, `clock[=<d1>..<d2>]` (every slot of those days, in order)
$ lute calendar . --axis 'holds(trusted(ada))=true,false'
lute calendar: `--axis holds(trusted(ada))`: `trusted(ada)` is derived by rules and cannot be asserted
```

With the visit and the meeting as axes, `inn.again` wins only where both hold:

```console
$ lute calendar . --axis "visited('inn.ada')=false,true" --axis 'holds(met(ada))=false,true' \
    --occasion placeVisit --target place.inn
calendar: . — 4 cell(s) × 1 column(s), from declared defaults

visited('inn.ada')  holds(met(ada))  placeVisit
                                     place.inn
false               false            innQuiet
false               true             innQuiet
true                false            innQuiet
true                true             inn.again +1

shadowed (eligible, not presented):
  visited('inn.ada')=true holds(met(ada))=true  placeVisit@place.inn: inn.again over innQuiet

never eligible in any cell: 2
  inn.ada [scene, scenes/inn/ada.lute] placeVisit@place.inn — when: false
  inn.regular [scene, scenes/inn/regular.lute] placeVisit@place.inn — when: false

eligible but never presented in any cell: none
```

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

eligible but never presented in any cell: none
```

`ferry` starts on `visited('inn.ada')`, which the save holds, so the settle activates it and the note says so.

### Per-occasion axes and who is where

Some occasions happen once a day, but a grid over day × slot evaluates every occasion in every row. `--occasion <O>@<axis>,…` (dsl 0.24.0) varies only the named axes for `O`: it is evaluated where every other axis is at its first value and left blank elsewhere — `dayStart@run.day` reads the bell once per day. `@run.day,run.slot=evening` holds `run.slot` at `evening` instead of its first value. `--facts <relation>` (repeatable) adds a table of the relation's facts in every settled cell — a row per first argument, a column per cell — so the rules that move people become visible:

```console
$ lute calendar . --axis run.day=1..3 --axis run.slot=morning,evening \
    --occasion dayStart@run.day --occasion placeVisit --target place.dock --facts present
```

The grid then reads (the facts table and the lists follow it):

```console
calendar: . — 6 cell(s) × 2 column(s), from declared defaults
  dayStart: varies over run.day only, at run.slot=morning; blank elsewhere

run.day  run.slot  dayStart (sequence)   placeVisit
                                         place.dock
1        morning   day.bell              dockBo +2
1        evening                         dockGulls +1
2        morning   day.bell              dockBo +2
2        evening                         dock.storm +2
3        morning   day.bell, day.market  dockGulls +1
3        evening                         dockGulls +1
```

and `--facts present`, over the same six cells with every occasion evaluated:

```console
facts present(person, place):
person  1/morning  1/evening  2/morning  2/evening  3/morning  3/evening
ada     -          inn        -          inn        -          inn
bo      dock       -          dock       -          -          -
```

In `--json` a per-occasion column carries `varies` and `heldAt`, a cell outside it has no result for that column, and each cell carries a `facts` map (`{ "present": ["present(bo, dock)"] }`); `--csv` adds a `facts:present` column.

With a declared [clock](/language/clock/), the grid is `--axis clock=<d1>..<d2>`, and a per-occasion column names the clock's axes: `dayStart@clock.day` — or the clock's own paths, `dayStart@run.day` — with `,clock.slot=<slot>` (or `,run.slot=<slot>`) to hold another slot than the day's first. Give the harbour a clock over the paths it already has:

```yaml
clock:
  day: run.day
  slot: run.slot
  slots: [morning, evening]
```

```console
$ lute calendar . --axis clock=1..3 --occasion dayStart@clock.day,clock.slot=evening \
    --occasion placeVisit --target place.dock
calendar: . — 6 cell(s) × 2 column(s), from declared defaults
  dayStart: varies over clock.day only, at clock.slot=evening; blank elsewhere

clock      dayStart (sequence)   placeVisit
                                 place.dock
1 morning                        dockBo +2
1 evening  day.bell              dockGulls +1
2 morning                        dockBo +2
2 evening  day.bell              dock.storm +2
3 morning                        dockGulls +1
3 evening  day.bell, day.market  dockGulls +1
```

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

eligible but never presented in any cell: none
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

`--json` emits where the cells start (`from`, the header's text), how many cells `--where` dropped (`pruned`), the axes, the columns, one record per cell, the never-eligible list, and the never-presented list (`neverPresented`, each with `beatenBy`). A column of a targeted occasion carries `target`, or `"anyTarget": true` for the `(any)` column. A cell's `results` hold one entry per column: `winner` (`null` on a `select: all` or `sequence` column, or when nothing is eligible), `presented` (the winner, or the whole offered or sequence list), `shadowed`, and, when a `when` evaluated unknown, `unknown` (`[{ id, reason }]`), with `"undecided": true` when that unknown decides the cell. A cell carries `notes` — a list — when its quest settle halted or moved an axis value:

```json
{
  "axes": [ { "path": "run.day", "values": [3] }, { "path": "run.slot", "values": ["morning"] } ],
  "cells": [
    {
      "at": { "run.day": 3, "run.slot": "morning" },
      "results": [
        { "occasion": "board", "select": "all", "winner": null, "presented": ["noteFair"], "shadowed": [] },
        { "occasion": "dayStart", "select": "sequence", "winner": null, "presented": ["day.bell", "day.market"], "shadowed": [] },
        { "occasion": "placeVisit", "target": "place.dock", "select": "first", "winner": "dockGulls", "presented": ["dockGulls"], "shadowed": ["dockEmpty"] },
        { "occasion": "placeVisit", "target": "place.inn", "select": "first", "winner": "innQuiet", "presented": ["innQuiet"], "shadowed": [] }
      ]
    }
  ],
  "columns": [ { "occasion": "board", "select": "all" }, … ],
  "from": "declared defaults",
  "neverEligible": [
    { "id": "dockBo", "kind": "entry", "document": "lore/places.lute", "on": "placeVisit", "target": "place.dock", "reasons": ["when: false"] },
    …
  ],
  "neverPresented": [
    { "id": "dockEmpty", "kind": "entry", "document": "lore/places.lute", "on": "placeVisit", "target": "place.dock", "beatenBy": ["dockGulls"] }
  ],
  "pruned": 0
}
```

`--csv` writes one row per cell and column — the axis values, then `occasion,target,select,winner,presented,shadowed,unknown,notes`, lists joined with `;` (an `(any)` column's target reads `(any)`) — for a spreadsheet, then, after a blank line, a second table of the beats eligible but never presented, each with what beat it:

```console
$ lute calendar . --axis run.day=2..3 --axis run.slot=evening --occasion placeVisit --csv
run.day,run.slot,occasion,target,select,winner,presented,shadowed,unknown,notes
2,evening,placeVisit,place.dock,first,dock.storm,dock.storm,dockGulls;dockEmpty,,
2,evening,placeVisit,place.inn,first,inn.ada,inn.ada,,,
3,evening,placeVisit,place.dock,first,dockGulls,dockGulls,dockEmpty,,
3,evening,placeVisit,place.inn,first,inn.ada,inn.ada,,,

neverPresented,kind,document,occasion,target,beatenBy
dockEmpty,entry,lore/places.lute,placeVisit,place.dock,dock.storm;dockGulls
```

### Limits and exit codes

The calendar compiles the whole project the way `lute play` does, so, unlike `lute beats`, it needs a project that compiles. With the contradictory storm from [above](#lute-beats):

<!-- lute-diagnostics -->
```console
$ lute calendar . --axis run.day=1..3
./scenes/dock/storm.lute:7:8: error [E-BEAT-UNREACHABLE] beat `dock.storm` is never eligible: its `when` `run.slot == 'morning' && run.slot == 'evening'` is provably false (dsl 0.21.0 §5)
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

List every condition that queries a fact — `holds(…)`, `count(…)`, `countDistinct(…)`, or `validAt(…)` — in every guard slot (dsl 0.24.0): beat, entry and objective guards, line `when=`, `<choice when>`, `<when>` arm tests, `::next`/`::set` `when`, `<on when>`, reward `when`, quest `start`/`fail`, and objective `until`. Conditions are grouped by document, each with its source line, and each queried atom is traced back to what can produce it. A derived atom is followed **through the rules**, with the rule head's variables bound to the atom's constants, so each premise is shown as the ground atom it needs; a base atom ends at its producers:

- **asserted by** — the scenes (by scene key), quests, entries and bundle beats (by canonical id `<document id>.<beat id>`) whose `::assert` can produce it, with their documents;
- **seed facts** — the schema's `facts:` that match it;
- **reserved** — a `reserved: true` relation: the engine asserts it;
- **NO PRODUCER** — nothing asserts it, no seed fact, not reserved, no rule. The condition is waiting on content no one has written.

```console
$ lute scenario . knowledge
project root: .
  knowledge (every fact-guarded condition, by document -> relations read -> producers):

  lore/places.lute
    entry `innQuiet`
      when: !holds(present(ada, inn))
      not present(ada, inn) — holds unless defeated
        defeated when present(ada, inn) is derived ⇐ cel("run.slot == 'evening'")
        rule: present(ada, inn) :- cel("run.slot == 'evening'")
          cel("run.slot == 'evening'") — state condition on run.slot, decided at run time

    entry `dockBo`
      when: holds(present(bo, dock))
      present(bo, dock) — derived by 1 rule
        rule: present(P, dock) :- works(P, dock), cel("run.slot == 'morning' && run.day != 3")
          works(bo, dock) — seed facts works(bo, dock)
          cel("run.slot == 'morning' && run.day != 3") — state condition on run.day, run.slot, decided at run time

    entry `noteDocked`
      when: holds(arrived(dock))
      arrived(dock) — reserved — the engine asserts it

  quests/ferry.lute
    objective `ferry.word`
      done: holds(trusted(ada))
      trusted(ada) — derived by 1 rule
        rule: trusted(P) :- met(P), not rumor(P)
          met(ada) — asserted by scene `inn.ada` (scenes/inn/ada.lute)
          not rumor(ada) — always holds (nothing produces rumor(ada): no assert, seed fact, engine write or rule) — cannot be defeated

  scenes/inn/ada.lute
    scene `inn.ada`
      when: holds(present(ada, inn))
      present(ada, inn) — derived by 1 rule — traced above under entry `innQuiet` in lore/places.lute

  scenes/inn/again.lute
    scene `inn.again`
      when: holds(trusted(ada))
      trusted(ada) — derived by 1 rule — traced above under objective `ferry.word` in quests/ferry.lute
```

Only the rules that can conclude the queried atom are followed: `dockBo` asks for `present(bo, dock)`, so the rule that places Ada at the inn is not listed under it. A derived atom's rules print once per report; every later mention says where they were traced. A negation reads "holds unless defeated", followed by one line per fact that could defeat it — `defeated when <fact> is asserted by …`, `… is derived ⇐ <premises>`, `defeated when the engine asserts <fact> (reserved)`, or `defeated from the start: <fact> is a seed fact`. `not rumor(ada)` has no producer, so it "always holds … — cannot be defeated": the rumour that would make Ada distrust you is a relation declared and never written. (0.23 printed this as `NO PRODUCER`, which read as a warning; a positive read with no producer still says `NO PRODUCER`.) Under [`check-project --wip`](/tooling/cli/#check-project), a guard that is dead only because of such a relation is a warning, not an error.

Every premise of a rule is traced beneath it. A `cel(…)` premise is a state condition the rules cannot decide ahead of time, so it names the paths it reads — `cel("run.slot == 'evening'") — state condition on run.slot, decided at run time` — and an entity-kind atom is a membership test, not a relation that lacks a producer. With the trust rule written `trusted(P) :- person(P), met(P), not rumor(P)`:

```console
$ lute scenario . knowledge --for ferry.word
project root: .
  knowledge (every fact-guarded condition, by document -> relations read -> producers):

  quests/ferry.lute
    objective `ferry.word`
      done: holds(trusted(ada))
      trusted(ada) — derived by 1 rule
        rule: trusted(P) :- person(P), met(P), not rumor(P)
          person(ada) — entity kind `person`; ada is a member
          met(ada) — asserted by scene `inn.ada` (scenes/inn/ada.lute)
          not rumor(ada) — always holds (nothing produces rumor(ada): no assert, seed fact, engine write or rule) — cannot be defeated
```

Since dsl 0.26.0 §6 a rule body may count — `canPass(pier) :- count(hasItem(_)) >= 2` — and the count is traced like any premise: the facts it counts, with their producers beneath it:

```console
$ lute scenario . knowledge --for pier.boat
project root: .
  knowledge (every fact-guarded condition, by document -> relations read -> producers):

  scenes/boat.lute
    scene `pier.boat`
      when: holds(canPass(pier))
      canPass(pier) — derived by 1 rule
        rule: canPass(pier) :- count(hasItem(_)) >= 2
          count(hasItem(_)) >= 2 — counts:
            hasItem(_) — asserted by scene `wren.talk` (scenes/wren.lute)
```

A negated premise the project **can** make false is followed by what would do it. The rule body is instantiated against every fact that may hold in some run (the may set `check-project` decides guards with, here counting every assert site as live), and constants the positive premises force are carried into the negation (`not liar(hollis)`, not `not liar(_)`). Once a bundle beat writes the rumour:

```console
          not rumor(ada) — holds unless defeated
            defeated when rumor(ada) is asserted by beat `town.gossip.whisper` (lore/gossip.lute)
```

A derived defeater names the facts that join to produce it — here an entry guarded on `!holds(present(bo, dock))`:

```console
      not present(bo, dock) — holds unless defeated
        defeated when present(bo, dock) is derived ⇐ works(bo, dock) [seed], cel("run.slot == 'morning' && run.day != 3")
```

A fact with several derivation routes lists every one of them (dsl 0.25.0 §9), separated by ` / `, so the report shows each way the negation can fail, not only the first. Add a rule `present(P, L) :- hired(P, L)` and a scene `dock.hire` that asserts `hired(bo, dock)`:

```console
      not present(bo, dock) — holds unless defeated
        defeated when present(bo, dock) is derived ⇐ works(bo, dock) [seed], cel("run.slot == 'morning' && run.day != 3") / ⇐ hired(bo, dock) [scene `dock.hire` (scenes/dock/hire.lute)]
```

At most three defeating facts are printed per premise, then `… and N more defeating facts`. A negated relation that may hold any tuple (an open argument domain) says so after its defeat line (`` — any `rumor` tuple may hold ``).

`--for <node>` selects: a scene (every guard in it — `inn.again`), a bundle beat, an entry id (`dockBo`), a quest objective as `<quest>.<objective>` (`ferry.word`), `quest:<id>` for every objective of a quest, or `<scene>#<branch>.<choice>` for one choice's guard. With a scene `inn.ask` whose line and choice are fact-guarded:

```console
$ lute scenario . knowledge --for inn.ask
project root: .
  knowledge (every fact-guarded condition, by document -> relations read -> producers):

  scenes/inn/ask.lute
    line `@ada` (line 9)
      when: holds(present(ada, inn))
      present(ada, inn) — derived by 1 rule
        rule: present(ada, inn) :- cel("run.slot == 'evening'")
          cel("run.slot == 'evening'") — state condition on run.slot, decided at run time

    choice `ask.ferry` (line 12)
      when: holds(trusted(ada))
      trusted(ada) — derived by 1 rule
        rule: trusted(P) :- met(P), not rumor(P)
          met(ada) — asserted by scene `inn.ada` (scenes/inn/ada.lute)
          not rumor(ada) — always holds (nothing produces rumor(ada): no assert, seed fact, engine write or rule) — cannot be defeated
```

`--for 'inn.ask#ask.ferry'` keeps only the choice. A node that names no fact-guarded condition is a usage error (exit 2), with a did-you-mean when one is close:

```console
$ lute scenario . knowledge --for ferry.wrod
lute scenario knowledge: `--for ferry.wrod` names no fact-guarded condition (a scene, bundle beat, entry, `quest:<id>`, `<quest>.<objective>` or `<scene>#<branch>.<choice>`) — did you mean `ferry.word`?
```

`--format json` (given before the subcommand, like every `scenario` option) emits `{ "roots": [ { "root", "elements", "relations" } ] }`. Each element is `{ node, document, line?, for, conditions, reads }` — `for` lists the `--for` handles that select it (`["inn.ask", "inn.ask#ask.ferry"]`), `conditions` maps the slot (`when`, `done`, …) to its expanded text, and `reads` lists the atoms it queries, a negated read as `!present(ada, inn)`. `relations` maps each relation involved to `{ declared, derived, reserved, seedFacts, assertedBy, rules }`, each rule `{ rule, premises }`, where a premise is `{ relation, negated? }`, `{ entityKind }` for a kind atom, or `{ cel }` for a `cel(…)` premise (its condition text). `--format dot` is refused: the knowledge map is not a graph.

`lute scenario envelope` words its Facts rows the same way (`works/2 (producible) — seed facts works(bo, dock)`, `arrived/1 (producible) — reserved — the engine asserts it`, `trusted/1 (producible) — derived by 1 rule`), and [`lute lore`](/tooling/cli/#lore) ends with a Derived section built from the same traces: each conclusion the rules can reach, the rule instance and evidence behind it, and the conditions it gates.

Exit **0** on success, **2** on an I/O failure or an unmatched `--for`.

### What the scenario graph leaves out

The [scenario graph](/connectivity/scene-graph/) draws only prerequisites that connectivity analyzes. A quest's edges come from its `after=`, its subquest tree, the anchoring conjuncts of its `start`, and the `::accept`s that take it up (dsl 0.24.0 §2, 0.25.0 §4); see [Quests & scenes](/language/quests-and-scenes/#quest). The bare `lute scenario` view says what it therefore did not draw. Take a lighthouse: a quest `relight` with `start="entry.keeperLog.everRead && visited('arrival')"` and two subquests, `oil` and `wick`; a `salvage` quest the engine accepts from a board (`accept="external"`); a `lostDog` quest only a test accepts; two keeper beats, `greeting` with `after="visited('arrival')"` and `warning` gated by `when="visited('keeper.greeting')"`; and a scene `farewell` with `after: completed("salvage")`:

```console
$ lute scenario .
project root: .
  topological layers:
    layer 0: scene(arrival), scene(farewell), beat(keeper.warning), entry(keeperLog)
    layer 1: quest(relight), beat(keeper.greeting)
    layer 2: quest(oil), quest(wick)
  edges (prerequisite -> dependent) [atom kind(s)]:
    scene(arrival) -> quest(relight) [start]
    scene(arrival) -> beat(keeper.greeting) [visited]
    quest(relight) -> quest(oil) [subquest]
    quest(relight) -> quest(wick) [subquest]
    entry(keeperLog) -> quest(relight) [start]
  unanchored (no `after` — available from the start of play; no prerequisites in this graph):
    quest(lostDog)
    quest(salvage)
    beat(keeper.warning) — its `when` reads visited('keeper.greeting'), which gates it but draws no edge; write `after="visited('keeper.greeting')"` to anchor it (dsl 0.25.0 §3)
  note: 6 `visited()`/`completed()`/`active()` reference(s) not drawn — a quest's edges come from its `after`, its subquest tree, its `start` conjuncts and its `::accept`s:
    scene(farewell) -> completed("salvage") — quest(salvage) is on no edge (no `after`, tree, `start` anchor or `::accept`)
    quest(relight) reads visited('keeper.warning') in its objective climb done — a condition read, not an anchor
    quest(oil) reads visited('arrival') in its objective fetch done — a condition read, not an anchor
    quest(wick) reads visited('arrival') in its objective trim done — a condition read, not an anchor
    quest(salvage) reads visited('arrival') in its objective dive done — a condition read, not an anchor
    quest(lostDog) reads visited('arrival') in its objective find done — a condition read, not an anchor
```

`relight` is anchored by its `start` conjuncts, so it needs no `after=`, and `oil` and `wick` hang off it by `[subquest]` edges. `keeper.greeting` is ordered by its own `after=`; `keeper.warning` reads the same kind of fact in its `when`, which gates it but orders nothing, so it is listed with the `after=` to write. `salvage` and `lostDog` are on no edge, so the `completed("salvage")` in `farewell`'s `after:` is not drawn either, and an objective's `visited()` read is a condition, never an anchor.

`--format json` carries the unanchored nodes as `unanchored` on the root, the beat hints as `unanchoredHints` (node → hint), and the undrawn references as `omitted`, each `{ from, kind, quest }` for a `completed`/`active` reference or `{ from, kind: "visited", scene, slot }` for a quest's `visited()` read. `lute scenario . reach quest:relight` lists the quest's anchors (`anchors`, each `{ kind, from }`, in JSON):

```console
$ lute scenario . reach quest:relight
project root: .
reach quest(relight):
  verdict: Reachable — a satisfiable route exists under your declared routes.
  after: (none declared) — anchored (dsl 0.24.0 §2, 0.25.0 §4); each anchor holds before it activates, through any one of its sources:
    [start] entry(keeperLog)
    [start] scene(arrival)
  referenced node(s) (see the anchors above — this is NOT a flat requirement list):
    - scene(arrival): Reachable — a satisfiable route exists under your declared routes.
    - entry(keeperLog): Reachable — a satisfiable route exists under your declared routes.
```

Lore entries join the graph only as the source of a quest's `start` anchor, `entry(<id>)`. Bundle beats are always nodes: a beat without `after=` is an entry node, and one with `after=` is the dependent of the edges it draws. `lute scenario . reach keeper.greeting` (or `reach beat:keeper.greeting`) prints the file that declares it with its `on`, `target` and `when`; see [The scene graph](/connectivity/scene-graph/#bundle-beats).

### Fact edges (`lute scenario --facts`)

```console
$ lute scenario <dir> [--format text|json|dot] --facts
```

Progress in a collecting game is gated by facts, not by visits: a gym opens on a badge, a tower on a lens. Those gates draw no `after:` edge, so the bare graph puts every gym in layer 0. `--facts` (dsl 0.26.0 §8) adds a **fact edge** `producer -> reader [fact]` wherever a scene's, beat's or quest's gate — its `when:` or `start=` — reads `holds(F)` and the producer asserts `F`; when `F` is derived, the edge runs from whatever asserts a fact the deriving rule needs and names the rule's conclusion, `[hasItem(goodRod), via canPass(pier)]`. The layers are then drawn over the fact edges too, wherever one closes no cycle, so what a fact unlocks sits below what gives it. Wren's scene gives a rod (`::assert{hasItem(goodRod)}`); the lake reads `holds(hasItem(goodRod))`, and the boat reads `holds(canPass(pier))`, which the rule `canPass(pier) :- hasItem(goodRod)` derives:

```console
$ lute scenario .
project root: .
  topological layers:
    layer 0: scene(pier.boat), scene(pier.lake), scene(wren.talk)
  edges (prerequisite -> dependent) [atom kind(s)]:
    (none)
$ lute scenario . --facts
project root: .
  topological layers (after: and fact edges):
    layer 0: scene(wren.talk)
    layer 1: scene(pier.boat), scene(pier.lake)
  edges (prerequisite -> dependent) [atom kind(s)]:
    (none)
  fact edges (producer -> reader) [asserted fact]:
    scene(wren.talk) -> scene(pier.boat) [hasItem(goodRod), via canPass(pier)]
    scene(wren.talk) -> scene(pier.lake) [hasItem(goodRod)]
```

Across a whole game the edges show one area's gate hanging on another's scene — on Monster League, `scene(mid.gameCorner) -> scene(east.ashTowerLens) [hasItem(spectralLens)]`. `--format json` adds `factEdges` to the root, each `{ from, to, fact, via?, layered }` (`via` when the gate reads a derived fact; `layered` whether the edge took part in the layering); `--format dot` draws each one dotted and purple, labelled with the fact. `--facts` is an option of this graph view only: `reach`, `envelope`, and `check-project`'s connectivity passes still read the declared `after:` structure.

## Beat rows in `project.index.json`

The overviews read the same beat table an engine does. Since 0.23.0, a beat row in [`project.index.json`](/tooling/cli/#--all--project-wide-compile-and-index) carries the beat's `when` — with every `@def` expanded, so an engine or a tool can show it without the schema — and its `title`, the label a `select: all` menu shows. Both are omitted when the beat has none, so a project that uses neither compiles byte-identically. A [bundle beat](/tooling/play/#bundle-beats)'s row has kind `bundle`. Since dsl 0.26.0 a [kind beat](/tooling/play/#kind-targets)'s row has `target: "kind:<kind>"` and `targetKind: { kind, prefix, members }` — `{"kind": "trainer", "prefix": "npc", "members": ["gus", "r16Gus"]}` — so an engine can offer it for every `<prefix>.<member>` raise.

```json
{"id": "noteFerry", "kind": "entry", "document": "lore/places.lute", "on": "board", "priority": 0, "when": "run.day <= 2", "title": "Ferry times"}
{"id": "day.market", "kind": "scene", "document": "scenes/day/market.lute", "on": "dayStart", "priority": 0, "once": "run", "when": "run.day == 3", "title": "Market day"}
{"id": "dock.storm", "kind": "scene", "document": "scenes/dock/storm.lute", "on": "placeVisit", "target": "place.dock", "priority": 50, "once": "run", "when": "run.slot == 'evening' && run.day == 2"}
```
