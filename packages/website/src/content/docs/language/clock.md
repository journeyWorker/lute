---
title: The clock
description: "A declared day clock (dsl 0.24.0 §1) — the schema's clock: over engine-owned paths (day, optional slot and slots, raise as one occasion or a slot / dayStart / dayEnd map, week), day-granular clocks, what E-CLOCK-DECL rejects, the read-only clock.index / clock.weekday / clock.weekdayLabel, once: day, once: week (dsl 0.27.0) and once: slot beats and entries, moving time with lute play's advance: and include: steps, the lute calendar clock axis — and the three pieces that shipped beside it: enum member display labels, integer %, and the guarded write ::set{… when=…}."
---

Plenty of games keep time as a day and a part of the day: morning, afternoon, night, then the next
morning. A visual novel lets you visit one place per slot, a life sim opens the market on
Saturdays, a shopkeeper greets you once a day and mutters something shorter the rest of it.

Before dsl 0.24.0 you could hold that time in state, as two [engine-owned](/state/state-model/)
paths such as `run.day` and `run.slot`. The checker saw two unrelated values. It could not know
that the afternoon comes after the morning, that day 6 is a Saturday, or that "once a day" is a
thing a beat can ask for. A **clock** says so (dsl 0.24.0 §1). It is declared over those same two
paths, so it adds meaning, not storage: the order of the slots, a weekday, a monotone position in
time, and a spending rule for beats.

## Declaring a clock

A schema declares at most one clock, beside the state it reads:

```yaml
# world.schema.yaml
state:
  run.day:  { type: number, default: 1, owner: engine }
  run.slot: { type: { enum: [morning, afternoon, night] }, default: morning, owner: engine }
entities:
  person: { members: [wren, hale] }
clock:
  day: run.day
  slot: run.slot
  slots: [morning, afternoon, night]
  raise: slotStart
  week:
    length: 7
    first: 0
    labels: [Mon, Tue, Wed, Thu, Fri, Sat, Sun]
```

| Key | Meaning |
|---|---|
| `day` | required; a `number` state path declared `owner: engine`. Day 1 is the first day |
| `slot` | optional; an enum state path declared `owner: engine`, inline (`{ enum: [...] }`) or a named domain (`{ domain: slot }`). `slot` and `slots` come together or not at all |
| `slots` | with `slot`; the slot enum's members, each exactly once, in the order a day runs through them. The enum's own member order does not matter; this list is the clock's order |
| `raise` | optional; the occasion the engine raises after every advance of the clock, where it stops, so beats can answer "a new slot has started". Or a map naming an occasion for each moment, each key optional: `{ slot, dayStart, dayEnd }` (see [Moving time](#moving-time)) |
| `raiseAtStart` | optional (dsl 0.28.0), default `false`; `true` says the engine raises the `slot` occasion and `dayStart` itself where a run starts, where no advance stops (see [Where the run starts](#where-the-run-starts)) |
| `week` | optional; `length` (required inside `week`, at least 1), `first` (the weekday index of day 1, 0-based, default `0`), and `labels` (one display text per weekday, in weekday order) |
| `last` / `days` | optional (dsl 0.27.0); where the clock ends. `last: { day: 1, slot: h05 }` names the last position (`slot` omitted: that day's last slot; a clock without slots gives `day` alone), and `days: N` is short for `last: { day: N }`. Declare one of the two. See [A clock that ends](#a-clock-that-ends) |

A clock without `slot` and `slots` counts whole days. Each day is its one slot, a position is just
its day (`day 3`), `clock.index` is `day - 1`, and `once: slot` spends like `once: day`. A game that
only counts days no longer needs a one-member slot enum to have a clock.

A clock lives only in a schema, a file reached through `uses:` or the manifest's `defaults:`. A
scene's own frontmatter cannot declare one: `clock:` there is `E-META-UNKNOWN-KEY`. The examples
on this page use a project whose plugin declares two occasions:

```yaml
occasions:
  slotStart: { select: sequence }
  talk:      { select: first, target: { prefix: npc, entity: person } }
```

The engine still owns both paths. Content reads `run.day` and `run.slot` as before and never
writes them (`E-ENGINE-OWNED-WRITE`). The declaration travels to the engine as `clock` on every
compiled artifact and on `project.index.json`, and is omitted when a project has no clock.

### What `E-CLOCK-DECL` rejects

Everything wrong with a clock is `E-CLOCK-DECL`:

- a malformed declaration: a missing `day`, an unknown key, a `raise` that is neither an occasion
  name nor a `{ slot, dayStart, dayEnd }` map, `slot` without `slots` or `slots` without `slot`, an empty or
  repeated `slots` list, a `week.length` of 0, a `week.first` outside `0..length-1`, or a
  `week.labels` list whose length is not `week.length` (`week.labels` is a list, one label per
  weekday; an enum's or entity kind's `labels:` is the map);
- a `day` or `slot` path that is not declared, not of the right type (`day` a `number`, `slot` an
  enum), the same path twice, or not `owner: engine`; a `day` path whose default is below 1 (the
  clock counts from day 1);
- `slots` that are not exactly the slot enum's members;
- a `raise` occasion, or any occasion of a `raise` map, that no plugin declares (with a
  did-you-mean). While no plugin of the project declares occasions, any name passes, as for `on:`;
- a `raise` occasion that declares a `payload:`: an advance has no payload to give it, so raise
  that occasion from the engine instead;
- a `last` / `days` that names no position: both keys at once, `days: 0`, a `last.day` below 1, a
  `last.slot` that is not one of `slots` (or any `last.slot` on a clock without slots), or a clock
  whose `day` / `slot` defaults already stand past its last position (dsl 0.27.0 §4);
- a second clock: two schemas of one project that both declare `clock:`.

Apart from a second clock, which only the project can see, each is reported on the schema, at the line of its `clock:` key, and
`lute check world.schema.yaml` finds them on its own (the `raise` occasions against the plugins of
the project the schema sits in; outside a project, any occasion name passes). A document that uses
the schema carries the same error at its line 1 in a single-file `lute check`, naming the schema, with
the schema line under it. `check-project` instead reports it once, as a project-wide line at the
schema line ending `(imported by N documents)`, and leaves the importers `ok`. A clock over a `run.day`
without `owner: engine`:

<!-- lute-diagnostics unverified="verbatim lute check output, but composed at runtime: crates/lute-check/src/clock.rs wraps each clock problem as `clock:` {what}, a prefix literal below the admission floor, so no single format! literal spans it" -->
```
world.schema.yaml:4:1: error [E-CLOCK-DECL] `clock:` `day: run.day` must be declared `owner: engine` — only the engine moves the clock
```

## Reading the clock

A declared clock adds reserved, read-only paths. Content reads them anywhere a CEL condition
or an interpolation is legal: a beat's `when`, a line's `when=`, a `<match>` subject, a quest
deadline, `{{…}}`.

| Path | Type | Value |
|---|---|---|
| `clock.day` | number | the day path's value (`run.day` above) |
| `clock.slot` | enum of `slots` | the slot path's value. Only on a clock with slots |
| `clock.index` | number | `(day - 1) * len(slots) + ` the slot's position in `slots` (from 0); `day - 1` on a clock without slots. It only grows: day 1 morning is 0, day 1 night is 2, day 2 morning is 3 |
| `clock.weekday` | whole number `0..length-1` | `(week.first + day - 1) mod week.length`. Only with a `week:` |
| `clock.weekdayLabel` | enum of `week.labels` | `week.labels[clock.weekday]`, renderable in `{{…}}`. Only with `week.labels` |
| `clock.ended` | bool | `true` once a finite clock has ended ([A clock that ends](#a-clock-that-ends)). Only with `last:` / `days:` |

The engine derives them from the live `day` and `slot`. They are not state rows, and content never
writes them: `::set{clock.index = 3}` is `E-QUEST-RESERVED-WRITE`, and a `::set` of the day or slot
path itself is `E-ENGINE-OWNED-WRITE`; both messages say to move the clock with an `advance:` step.
Without a clock, `clock.*` is simply undeclared (`E-UNDECLARED`), and so are `clock.slot` on a clock
without slots, `clock.weekday` without a `week:`, `clock.weekdayLabel` without `week.labels` and
`clock.ended` on a clock that never ends.

Three "day" words mean three things: `day:` in the declaration names the state path that counts
days; `clock.day` reads that path's value; `days: N` (or `last.day: N`) is the last day of a clock
that ends.

With the clock above, day 1 is a Monday, so `clock.weekday == 5` is every Saturday:

```lute unverified="reads clock.* from the clock declared in world.schema.yaml, which a standalone check cannot reach and no example project in the repo declares; checked by hand as scenes/market.lute in a scratch project with that schema"
---
kind: scene
id: square.market
on: slotStart
when: "clock.weekday == 5 && run.slot == 'morning'"
once: false
---

# The square

## Shot 1.

@narrator: {{clock.weekdayLabel}}, day {{run.day}}. The market is up before the bells.
```

`clock.index` is the one number that orders time across days, which is what a deadline wants.
`clock.index >= 20` holds from the night of day 7 on, whatever slot comes first in the day. With
`run.day` and `run.slot` alone you would have to spell that condition out slot by slot.

Both weekday paths are typed tightly, so the checker treats them like any closed domain.
`<match on="clock.weekday">` with arms `is="0..4"` and `is="5..6"` is exhaustive without an
`<otherwise>`, and leaving out Sunday is `E-NONEXHAUSTIVE` naming the gap (`` `6` is not
covered ``). `is="7"` is `E-WHEN-LITERAL-DOMAIN`, and a guard `clock.weekday == 7` is dead
(`E-ARM-DEAD` on a line). The labels are the members of `clock.weekdayLabel`, so a misspelled
`is="Sundy"` is `E-WHEN-LITERAL-DOMAIN` too, and since dsl 0.26.0 so is a guard's `== 'Sundy'`,
naming the nearest label (`` did you mean `'Sun'`? ``):

```lute
<match on="clock.weekday">
  <when is="0..4">
    @narrator: Another working day.
  </when>
  <when is="5..6">
    @narrator: The weekend. The square is loud.
  </when>
</match>
```

A component body reads only its params, so a component that shows the weekday takes it as a
param. Type the param `{ domain: clock.weekdayLabel }` (or `{ domain: clock.slot }` for a slot)
instead of copying the labels into an enum. The host passes the live value (`day=@today`, with
`today: "clock.weekdayLabel"` in `defs:`) or a literal label, and the checker judges the body's
`<match>` arms and every literal argument against the clock's own labels: a misspelled
`<when is="Wednesdy">` is one `E-WHEN-LITERAL-DOMAIN` at the component, naming the nearest label.

## Once a day, once a week, once a slot

A beat's `once` (see [Beats](/language/beats/)) gains three values on a project with a clock:

| `once` | Spent | Eligible again |
|---|---|---|
| `day` | from its presentation until the clock's day changes | on the next day |
| `week` | from its presentation until the next clock week starts (dsl 0.27.0 §5); needs the clock's `week:` | when `clock.weekday` returns to `week.first` |
| `slot` | from its presentation until the clock's slot changes (a new slot, or the same slot on another day) | in the next slot |

Scene beats write it in frontmatter (`once: day`), bundle beats as an attribute
(`<beat … once="day">`), and entries as `once="day"`, `once="week"`, or `once="slot"`. The engine keeps where in
time each beat was last presented. For an entry that is a presentation record too, not its read
flags. The flags still decide whether the entry's effects apply (see
[Lore entries](/language/lore-entries/)).

A baker who greets you once a day, and says something short the rest of that slot:

```lute unverified="once: day needs the project's clock (world.schema.yaml above); checked by hand as scenes/wren.lute in a scratch project"
---
kind: scene
id: wren.greeting
on: talk
target: npc.wren
once: day
---

# The bakery

## Shot 1.

@wren: Morning! It's {{clock.weekdayLabel}}, so the rye is fresh.
```

```lute unverified="an entry once of slot needs the project's clock (world.schema.yaml above); checked by hand as lore/barks.lute in a scratch project"
---
kind: lore
title: Bakery barks
---

<entry id="wrenBusy" on="talk" target="npc.wren" category="bark" priority="-1" once="slot">
  @wren: Busy, busy. Come back later.
</entry>
```

[`lute play`](/tooling/play/) names the reason a spent beat is passed over. Talking to Wren three
times on Monday morning, then once on Tuesday:

```
── step 2 · talk → npc.wren ──────────────
  ✓ wrenBusy [entry, priority -1]
  ✗ wren.greeting [scene, priority 0] — once: day — already presented today
  → wrenBusy
  entry wrenBusy (first read)
@wren: Busy, busy. Come back later.
── step 3 · talk → npc.wren ──────────────
  ✗ wren.greeting [scene, priority 0] — once: day — already presented today
  ✗ wrenBusy [entry, priority -1, read] — once: slot — already presented this slot
  → (no eligible beat — the occasion passes)
── step 4 · advance day: day 1 (Mon) morning → day 2 (Tue) morning ──────────────
  set run.day = 2
── step 4 · slotStart (select: sequence) ──────────────
  ✗ square.market [scene, priority 0] — when: false
  → (no eligible beat — the occasion passes)
── step 5 · talk → npc.wren ──────────────
  ✓ wren.greeting [scene, priority 0]
  ✓ wrenBusy [entry, priority -1, read]
  → wren.greeting
@wren: Morning! It's Tue, so the rye is fresh.
```

A week is counted from day 1, which is weekday `week.first`: week *n* holds the days whose
`(day - 1) div week.length` is *n*. So a clock week always starts on the weekday of day 1, not on a
fixed label. With the clock above (`first: 0`, Monday), a beat with `once: week` is spent from the
day it is presented until the next Monday, and `lute play` passes it over meanwhile with
`once: week — already presented this week`; a clock whose `first` is `5` starts every week on a
Saturday. `once: week` in a project whose clock declares no `week:` is `E-BEAT-ATTR`, as `once: day`
is without a clock. A weekly letter:

```lute unverified="once: week needs the project's clock with its week: (world.schema.yaml above); checked by hand as scenes/letters.lute in a scratch project"
---
kind: scene
id: hale.letter
on: talk
target: npc.hale
once: week
---

# The post office

## Shot 1.

@hale: Your weekly letter. Same handwriting as last {{clock.weekdayLabel}}.
```

`day` and `slot` mean nothing without a clock, so a project that declares none rejects them:

```lute expect="E-BEAT-ATTR"
---
kind: scene
id: square.bell
on: townVisit
once: day
---

# The square

## Shot 1.

@narrator: The bell rings.
```

The message says what to do: declare a `clock:` in a schema, or use `run`, `user`, or `false`.
`once: week` likewise needs the clock's `week:`.

## Moving time

Only the engine moves the clock, and only forward. After every advance it settles the quests (a
[`by=` deadline](/language/quests-and-scenes/) the new time passes fails there), then raises the
clock's `raise` occasion, when it declares one, as an ordinary occasion. The two paths reset by
their tier like any other state, so a clock over `run.day` and `run.slot` starts over at day 1
morning with every new run.

A single `raise: slotStart` is raised once per advance, where the clock stops, never at the slots
it passes; `lute play` notes the slots an advance passed without it. The quests, though, settle at every slot an advance passes, raised there or not: a
[season](/state/schemas/#seasons) that opens and closes on the way starts its quests and fails
their deadlines where it does, and a [`rearm=`](/language/quests-and-scenes/#quests-that-come-back-season-tiers-and-rearm)
fires on the day its condition turns true, so one long advance ends in the same quest states as
the same advance taken a slot at a time. When a day's end or start needs its own story, `raise`
takes a map instead, each key optional:

```yaml
clock:
  # day, slot, slots, week as above
  raise: { slot: slotStart, dayStart: dayStart, dayEnd: dayEnd }
```

| Key | Raised |
|---|---|
| `slot` | once after every advance, where the clock stops. `raise: slotStart` is short for `raise: { slot: slotStart }` |
| `dayEnd` | at every midnight an advance crosses, before the crossing: at the day's last slot, with the day not yet advanced |
| `dayStart` | at every midnight an advance crosses, after it: at the next day's first slot |

Each midnight is a stop of its own. The clock moves there, the quests settle, then the occasion is
raised, so a `dayEnd` beat still reads the old day. Writing the clock paths directly, as an
`engine:` step in `lute play` does, raises none of the three.

### Where the run starts

A run starts at a position no advance stopped at, so by these rules the clock raises nothing there:
not the `slot` occasion, and not the `dayStart` of the day the run starts on. Many engines raise
them anyway when a run starts, so the first morning's scene plays. Say so on the clock:

```yaml
clock:
  # day, slot, slots, raise as above
  raiseAtStart: true
```

With `raiseAtStart: true` the run's first position counts as raised for the `slot` occasion and
for `dayStart`; `dayEnd` is unchanged. The checker and `lute calendar` follow it. `lute play` does
not raise anything by itself at the start: a script still plays the first raise with an
`occasion:` step. Without the key, a beat whose `when` holds only where the run starts is
`W-BEAT-UNRAISED`, which names the key if your engine raises it there, and an `occasion:` step
raising the `slot` occasion or `dayStart` at the start carries a note saying the same.

[`lute play`](/tooling/play/) stands in for that engine with an `advance:` step:

- `advance: slot` moves to the next slot, wrapping from the last slot into the next day;
- `advance: <n>` moves `n` slots at once. It never skips a day's close: with `dayEnd` declared it
  stops at each day's last slot on the way to raise it;
- `advance: day` moves to the first slot of the next day, whatever slot it starts from. It raises
  `dayEnd` where the clock stands; the rest of the day is skipped, its slots unsettled.

On a clock without slots, all three move whole days.
Presented beats may declare their own clock movement with `advances: slot`, `advances: day`, or
`advances: <n>` in scene frontmatter, and as an attribute on `<entry>` / `<beat>`. The declaration
is metadata; the engine performs the movement when that beat is presented, including quest settles
and the clock's declared raises:

```yaml
---
kind: scene
id: square.market
on: slotStart
advances: slot
---
```

This is equivalent to an `advance:` for clock movement. Do not put an explicit `advance:` immediately
after the beat to repeat it: `lute play` allows both moves but emits a note explaining that the
clock moved twice. The declaration requires a project clock. Migrate plugin settings
`spendsSlot: true` to `advances: slot`, and `spendsSlots: n` to `advances: n`; the engine, not
the plugin, owns the clock.

For an explicit `advance:` step, the clock writes the clock paths, settles, and raises `slot`,
taking the raised occasion's `pick:`, `choose:`, and selection `expect:` exactly as an `occasion:`
step would. From Monday morning, fifteen slots is Saturday morning:

```yaml
steps:
  - advance: 15
    expect: { presented: [square.market] }
```

```
── step 1 · advance 15: day 1 (Mon) morning → day 6 (Sat) morning ──────────────
  set run.day = 6
── step 1 · slotStart (select: sequence) ──────────────
  ✓ square.market [scene, priority 0]
  → square.market
@narrator: Sat, day 6. The market is up before the bells.
  note: passed day 1 (Mon) afternoon, night; day 2 (Tue) to day 5 (Fri), every slot without raising `slotStart` (1 beat answers it; an `advance:` raises it only where the clock stops)
```

With the `raise` map above and a scene on `dayEnd`, going from Monday afternoon two slots on stops
at Monday night for the day's close, then at Tuesday morning:

```
── step 2 · advance 2: day 1 (Mon) afternoon → day 2 (Tue) morning ──────────────
  set run.slot = "night"
── step 2 · day 1 (Mon) night · dayEnd ──────────────
  ✓ inn.closing [scene, priority 0]
  → inn.closing
@narrator: The inn shutters close on Mon.
  set run.day = 2
  set run.slot = "morning"
── step 2 · day 2 (Tue) morning · dayStart (select: sequence) ──────────────
  (no candidates)
  → (no eligible beat — the occasion passes)
── step 2 · slotStart (select: sequence) ──────────────
  ✗ square.market [scene, priority 0] — when: false
  → (no eligible beat — the occasion passes)
  note: passed day 1 (Mon) night without raising `slotStart` (1 beat answers it; an `advance:` raises it only where the clock stops)
```

On such a step a selection `expect:` reads the whole step: `presented` lists every beat it
presented, each midnight raise's and then the final raise's, in order, while `winner`, `offered`
and `notOffered` judge the final raise, where the clock stops.

An `advance:` step may carry the `engine:` writes that belong to the same moment, such as
`advance: day` beside `engine: { state: { run.leg: 3 } }`. They land where the clock arrives:
after every `dayEnd` / `dayStart` the advance raises on the way, so that evening's `dayEnd` still
reads the day it closes, and before the final settle and raise. One settle follows the last move
and the writes together. Writing the clock's own `day` or `slot` path there is a usage error
(exit 2): a clock move goes in a step of its own, or through `advance:`.

An `engine:` step may still write `run.day` or `run.slot` directly, but one that moves
`clock.index` backward is a usage error (exit 2) naming both positions. `advance:` on a project
without a clock is a usage error too. A week of routine is often the same steps for several
routes, so a step may also be `include: <file>`, which splices that file's steps in its place. The
file's path is relative to the including script, and an include cycle is a usage error.
[Playing a story](/tooling/play/) has the full step reference and the `--json` shape of an advance,
which lists the midnight raises under `advance.days`.

`lute calendar --axis clock=<d1>..<d2>` lays out every slot of those days in clock order, so a
grid reads like a timetable. Bare `--axis clock` is one week from day 1 (day 1 alone without a
`week:`):

```console
$ lute calendar . --axis clock=5..6 --occasion slotStart
calendar: . — 6 cell(s) × 1 column(s), from declared defaults

clock            slotStart (sequence)
5 Fri morning    -
5 Fri afternoon  -
5 Fri night      -
6 Sat morning    square.market
6 Sat afternoon  -
6 Sat night      -
```

An occasion raised once a day should be read once a day, not in every slot row. The calendar
follows the clock's raise map: a cell where no advance would raise the occasion reads `not raised`
— `dayStart` anywhere but a day's first slot, and never on the day the run starts; `dayEnd`
anywhere but a day's last slot; the `slot` occasion at the position the run starts at. With
`raiseAtStart: true`, the `slot` occasion and `dayStart` are raised at that position. A beat
only such cells would take is listed as never eligible. `--occasion dayEnd@clock.day` evaluates
`dayEnd` once per day, at the day's last slot where the clock raises it (`@run.day`, the clock's
own path, works too), and leaves the other rows blank; `,clock.slot=afternoon` reads it where an
`advance: day` taken in the afternoon would close the day.

See [Overviews](/tooling/overviews/) for the rest of the calendar.

## A clock that ends

Some games have a fixed span of time: one night in a closed hospital, from eleven to dawn, and then
the story is over. Since dsl 0.27.0 a clock may say where it ends:

```yaml
clock:
  day: run.night
  slot: run.hour
  slots: [h23, h00, h01, h02, h03, h04, h05]
  raise: { slot: hourStrikes, dayEnd: dawn }
  last: { day: 1, slot: h05 }      # or: days: 1
```

`last:` names the last position. Leave out `slot` for the last slot of that day; a clock without
slots gives `day` alone. `days: N` is short for `last: { day: N }`.

An advance that lands exactly on the last position is an ordinary advance. An advance that would
go past it moves only to the last position, raising `dayEnd` / `dayStart` at every midnight on the
way as always. Then it raises the last day's `dayEnd` once, raises no `slot` occasion, and the clock
has **ended**. Without `last:`, the advance from `h05` would roll into night 2 at `h23` and strike
the hour again. With it:

```
── step 1 · advance 6: day 1 h23 → day 1 h05 ──────────────
  set run.hour = "h05"
── step 1 · hourStrikes ──────────────
  ✓ ward.strike [scene, priority 0]
  → ward.strike
@narrator: The clock strikes at h05.
  note: passed day 1 h00, h01, h02, h03, h04 without raising `hourStrikes` (1 beat answers it; an `advance:` raises it only where the clock stops)
── step 2 · advance slot: day 1 h05 → day 1 h05 · the clock ends (its last position) ──────────────
── step 2 · day 1 h05 · dawn ──────────────
  ✓ ward.dawn [scene, priority 0]
  → ward.dawn
@narrator: Dawn breaks on night 1.
── step 3 · advance slot: day 1 h05 → day 1 h05 ──────────────
── halted: step 3: `advance:` past the clock's last position (day 1 h05) — the clock ended; a `newRun` starts it over (E-CLOCK-END) ──────────────
```

Any `advance:` after the end is `E-CLOCK-END` (exit 1), and so is one that starts past the end
because an `engine:` step moved the day on. A `newRun` resets the day and slot paths and starts the
clock over — when they are run-tier. A clock whose day path is `user.*` keeps its position across
runs, and with it its `once: day` / `once: slot` / `once: week` spends and its end. The play's
new-run step says which: ``the clock is kept: its day `user.dive` outlives the run, so its position,
its once: day / slot / week spends and its end stay``.

The checker knows the end too. `clock.index` ranges over the whole numbers from where the clock
starts to its last position, and the day path and `clock.day` from its default day to the last day.
On a clock that starts and ends on the same day, the slot path and `clock.slot` hold only the slots
from the starting one to the last one. A `when` that needs a later position can never hold:

<!-- lute-diagnostics -->
```
scenes/second.lute:6:8: error [E-BEAT-UNREACHABLE] beat `ward.second` is never eligible: its `when` `run.night == 2` is provably false — the clock ends at its last position, so `run.night` only ranges over 1..1
```

An entry's `when` gets `E-ENTRY-UNREACHABLE` and a line or arm guard `E-ARM-DEAD`, each naming the
clock's end. A `<match>` on the day path or `clock.index` is exhaustive once it covers every value
in range, and one on the slot path of a one-day clock once it covers every slot the clock reaches:
an arm for a slot past the end is `E-ARM-DEAD`. On a clock of several days every slot comes round,
so the slot path alone is not narrowed, but on the last day it is: with `last: { day: 2, slot: h02 }`,
a `when` of `run.night == 2 && run.hour == 'h04'` is unreachable because on day 2 `run.hour` only
holds h23, h00, h01 and h02. `lute calendar --axis clock` stops at the last position.

The checker also reads a guard's clock conditions together. A guard whose reads of the day path,
the slot path, `clock.index`, `clock.day`, `clock.slot` and `clock.weekday` no position of the clock
satisfies at once is provably false, on a clock that ends or one that never does. Night 1 at `h01` is
`clock.index` 2, so this guard can never hold:

<!-- lute-diagnostics -->
```
scenes/over.lute:7:8: error [E-BEAT-UNREACHABLE] beat `ward.over` is never eligible: its `when` `run.night == 1 && run.hour == 'h01' && clock.index > 2` is provably false — no clock position has `run.night == 1`, `run.hour == 'h01'` and `clock.index > 2`
```

A quest objective's `by=` deadline is judged over the same range. On the one-night clock a
deadline for night 2 can never hold, so it never fails the objective, and the checker warns at the
`by`:

<!-- lute-diagnostics -->
```
quests/q.lute:5:59: warning [W-DEADLINE-NEVER] objective `leave` never fails: its deadline `by: run.night >= 2` can never hold (the clock ends at its last position, so `run.night` only ranges over 1..1) — write a deadline the clock can reach, or drop it
```

A deadline can also come too early. When an objective's `done` can only hold at positions where
its `by=` has already passed, the objective fails before it can be done. The checker reads `done`
over the clock directly, and a `visited('<beat>')` from the first position where that beat's `when`
can hold, including through a derived relation whose rules are `cel()` guards over the clock. A
deadline holding on arrival at a position fails the objective before any beat of that position
plays. The checker warns at the `by`:

<!-- lute-diagnostics -->
```
quests/q.lute:8:69: warning [W-DEADLINE-BEFORE-WINDOW] objective `lamp` fails before it can be done: its `done` `visited('ward.lamp')` can first hold at day 2 h03, but its deadline `by: clock.index > 8` already holds at day 2 h01 — move the deadline after that window
```

Here `ward.lamp` is eligible only while `holds(lit(hall))`, and the one rule for `lit(hall)` is
`cel("run.night == 2 && run.hour == 'h03'")`. A `done` that comes true on the same arrival as the
deadline is not a problem, because `done` wins the tie.

### A deadline at the end of time

"Fail if the player is still here at dawn" is `by="clock.ended"`. `clock.ended` turns true in the
settle of the `advance:` that ends the clock, before the last `dawn` is raised, and stays true until
a `newRun`. The objective still completes the moment its `done` holds, at any hour before:

```lute unverified="needs the one-night clock from world.schema.yaml above; checked by hand in a scratch project"
<quest id="escape" title="Out before dawn" tier="run">
  <objective id="out" title="Leave the ward" done="visited('ward.exit')" by="clock.ended"/>
</quest>
```

A play step checks the moment with `expect: { clock: { ended: true } }` (a clock that never ends
has no `ended` to check, and saying so is a usage error).

Three spellings look close and are not the same:

- `by="clock.index >= 6"` names the last position, and the last position is not the end: the
  advance that *reaches* `h05` fails the objective, one slot before the clock ends. Use it for "by
  the last hour", not "at dawn".
- `on="dawn" until="true"` also fails at dawn, but `on=` moves the whole objective to that raise:
  its `done` is judged only at dawn too, so a player who escapes at `h01` stays `active` until
  dawn and a `questComplete` handler fires there. Use it only when that is what you want.
- `by="run.hour == 'h03'"` is a moment, not a deadline: it holds only while the clock stands at
  `h03`. An objective that becomes live at `h04` or later never sees it hold, so it never fails.
  `by="clock.index >= 4"` holds from `h03` on, and fails such an objective as soon as it is live.

`terminal: "clock.ended"` ends the game when the clock does. The `dayStart` of the day the run starts
on is not raised by the clock: an advance raises `dayStart` on each day it *enters*, and the run
starts inside day 1. If your engine raises it when a run starts, declare `raiseAtStart: true` (see
[Where the run starts](#where-the-run-starts)); otherwise put a first-morning beat on the occasion
your play starts with. Without the key, `lute calendar --axis clock` shows day 1's `dayStart` cells
as `not raised`, and a beat only day 1's `dayStart` would take as never eligible.

## Shipped alongside

Three smaller pieces arrived with the clock (dsl 0.24.0 §1). None of them needs a clock.

### Display labels for enum members

An enum member is a name, and a name is not always what a player should read. A
long-form enum may give its members display text with `labels:`:

```yaml
enums:
  weekday:
    members: [mon, tue, wed, thu, fri, sat, sun]
    labels: { mon: Monday, tue: Tuesday, wed: Wednesday, thu: Thursday, fri: Friday, sat: Saturday, sun: Sunday }
state:
  run.weekday: { type: { domain: weekday }, default: mon }
```

`{{path}}` of a state path typed against that enum (`{ domain: weekday }`) renders the label, so
the line below reads `Today is Sunday.`, not `Today is sun.`. A member without a label renders its
id. `lute play`, `lute run`, `lute trace`, and `lute test` agree, and the compiled state entry
carries `labels` for the engine. Conditions still compare ids: `run.weekday == 'sun'`.

```lute check
---
kind: scene
id: square.notice
enums:
  weekday:
    members: [mon, tue, wed, thu, fri, sat, sun]
    labels: { mon: Monday, tue: Tuesday, wed: Wednesday, thu: Thursday, fri: Friday, sat: Saturday, sun: Sunday }
state:
  run.weekday: { type: { domain: weekday }, default: mon }
---

# The square

## Shot 1.

@narrator: Today is {{run.weekday}}.
```

A label for something that is not a member is `E-ENUM-LABEL-NOT-MEMBER`, and a label that is not a
string is `E-META-VALUE`:

```lute expect="E-ENUM-LABEL-NOT-MEMBER"
---
kind: scene
id: square.notice
enums:
  weekday:
    members: [mon, tue, wed, thu, fri, sat, sun]
    labels: { mon: Monday, sunday: Sunday }
state:
  run.weekday: { type: { domain: weekday }, default: mon }
---

# The square

## Shot 1.

@narrator: Today is {{run.weekday}}.
```

Labels work in any long-form enum: a document's `enums:`, a schema's, or a plugin's `enums/`
export (see [Vocabulary](/language/vocabulary/)). A state path typed `{ domain: X }` now counts as
reading `X`, so a domain used only to type state no longer draws `W-DOMAIN-UNREAD`.

### Integer `%`

`%` is the integer remainder, and it joins the CEL profile (it used to be `E-CEL-PROFILE`). "Every
seventh day" is now a condition, and a def over it is typed `number` by inference:

```lute check
---
kind: scene
id: square.restDay
on: townVisit
when: "@restDay"
once: false
state:
  run.day: { type: number, default: 1 }
defs:
  restDay: "run.day % 7 == 0"
---

# The square

## Shot 1.

@narrator: The shutters stay down. Nobody works on the seventh day.
```

Both operands must be integers. A non-number operand (`'a' % 2`, a bool path) or a fractional
literal is `E-CEL-TYPE`:

```lute expect="E-CEL-TYPE"
---
kind: scene
id: square.restDay
on: townVisit
when: "run.day % 2.5 == 0"
state:
  run.day: { type: number, default: 1 }
---

# The square

## Shot 1.

@narrator: Half a rest.
```

A number path cannot be proven whole by the checker, so the rest of the rule is the engine's. The
result is the truncated remainder, which takes the sign of the dividend: `-7 % 3 == -1`, and
`7 % -3 == 1`. A fractional value or a zero divisor makes the result unknown, never a fractional
remainder or a crash. `lute trace`, `lute test`, `lute play`, and the condition decider all follow
that rule, so `-7 % 3 == 2` is a dead condition.

### A guarded write: `::set{… when="…"}`

A single conditional write used to need a `<match>` or `<when>` around it. `when="…"` inside the
`::set` body does it in one line: the write happens only when the condition holds.

```lute check
---
kind: scene
id: bakery.visit
state:
  run.day: { type: number, default: 1 }
  run.aff.wren: { type: number, default: 0 }
  run.warmed.wren: { type: number, default: 0 }
---

# The bakery

## Shot 1.

@wren: You again. Sit, the kettle's on.
::set{run.aff.wren += 1 when="run.warmed.wren < run.day"}
::set{run.warmed.wren = run.day}
```

Wren warms to you at most once a day, however often you visit. The guard is checked like a line's
`when=`: a bool CEL condition, no `$` (there is no match subject), and `E-ARM-DEAD` when it can
provably never hold:

```lute expect="E-ARM-DEAD"
---
kind: scene
id: bakery.visit
state:
  run.day: { type: number, default: 1 }
  run.aff.wren: { type: number, default: 0 }
---

# The bakery

## Shot 1.

@wren: Take this.
::set{run.aff.wren += 1 when="run.day > 5 && run.day < 3"}
```

A guarded write never counts as a definite assignment. A later read of a path with no `default`
stays `E-MAYBE-UNSET`, because the write may not have happened. Inside a `<track>`, a guarded
`::set` is `E-TIMELINE-CONTENT`: a conditional write is logic, and it belongs outside the
`<timeline>`. `when=` is the only attribute a `::set` body takes. Anything else in it is read as
expression text, and its `E-CEL-PARSE` names `when=` as the one attribute there is.

The write compiles to the same one-arm `match` a gated line does, so the IR is unchanged. `lute
play` prints a skipped write as `skip set run.aff.wren += 1 — when: false`.

## Tooling

- `lute play` moves the clock with `advance:` steps, splices shared steps with `include:`, and
  shows every clock move as `day 1 (Mon) night → day 2 (Tue) morning`. See
  [Playing a story](/tooling/play/).
- `lute calendar --axis clock[=d1..d2]` walks the clock slot by slot. See
  [Overviews](/tooling/overviews/).
- `lute run`, `lute play`, and `lute trace` derive `clock.index`, `clock.weekday`, and
  `clock.weekdayLabel` from the live day and slot. A trace mock that seeds `run.day: 6` renders
  `{{clock.weekdayLabel}}` as `Sat`. Seed the two clock paths: a mock or `--state` that seeds a
  `clock.*` path is `E-TRACE-MOCK-UNDECLARED` (`` `--state clock.index=…` seeds a path the clock
  derives from its day and slot, which no mock may set — seed `run.day` / `run.slot` instead ``).

The engine contract (the reserved paths, `once: day` / `once: week` / `once: slot` spending,
forward-only advances) is in
[`docs/runtime/state-lifecycle.md`](https://github.com/journeyWorker/lute/blob/main/docs/runtime/state-lifecycle.md),
and integer `%` in
[`docs/runtime/cel-and-facts.md`](https://github.com/journeyWorker/lute/blob/main/docs/runtime/cel-and-facts.md).
The normative spec is
[`0.24.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)
§1, and the draft
[`0.27.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.27.0.md)
§5 for `once: week`.
