---
title: The clock
description: "A declared day clock (dsl 0.24.0 §1) — the schema's clock: over engine-owned paths (day, optional slot and slots, raise as one occasion or a slot / dayStart / dayEnd map, week), day-granular clocks, what E-CLOCK-DECL rejects, the read-only clock.index / clock.weekday / clock.weekdayLabel, once: day and once: slot beats and entries, moving time with lute play's advance: and include: steps, the lute calendar clock axis — and the three pieces that shipped beside it: enum member display labels, integer %, and the guarded write ::set{… when=…}."
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
| `week` | optional; `length` (required inside `week`, at least 1), `first` (the weekday index of day 1, 0-based, default `0`), and `labels` (one display text per weekday, in weekday order) |

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
  `week.labels` list whose length is not `week.length`;
- a `day` or `slot` path that is not declared, not of the right type (`day` a `number`, `slot` an
  enum), the same path twice, or not `owner: engine`;
- `slots` that are not exactly the slot enum's members;
- a `raise` occasion, or any occasion of a `raise` map, that no plugin declares (with a
  did-you-mean);
- a second clock: two schemas of one project that both declare `clock:`.

Apart from a second clock, which only the project can see, each is reported on the schema, at the line of its `clock:` key, and
`lute check world.schema.yaml` finds them on its own (the `raise` occasions against the plugins of
the project the schema sits in; outside a project, any occasion name passes). A document that uses
the schema carries the same error at its line 1, naming the schema, and `check-project` folds those
copies into one record (`+N more caller`) with the schema line under it. A clock over a `run.day`
without `owner: engine`:

<!-- lute-diagnostics -->
```
world.schema.yaml:4:1: error [E-CLOCK-DECL] `clock:` `day: run.day` must be declared `owner: engine` — only the engine moves the clock (dsl 0.24.0 §1)
```

## Reading the clock

A declared clock adds three reserved, read-only paths. Content reads them anywhere a CEL condition
or an interpolation is legal: a beat's `when`, a line's `when=`, a `<match>` subject, a quest
deadline, `{{…}}`.

| Path | Type | Value |
|---|---|---|
| `clock.index` | number | `(day - 1) * len(slots) + ` the slot's position in `slots` (from 0); `day - 1` on a clock without slots. It only grows: day 1 morning is 0, day 1 night is 2, day 2 morning is 3 |
| `clock.weekday` | whole number `0..length-1` | `(week.first + day - 1) mod week.length`. Only with a `week:` |
| `clock.weekdayLabel` | enum of `week.labels` | `week.labels[clock.weekday]`, renderable in `{{…}}`. Only with `week.labels` |

The engine derives them from the live `day` and `slot`. They are not state rows, and content never
writes them: `::set{clock.index = 3}` is `E-QUEST-RESERVED-WRITE`, and the message points to the
engine as the one that moves the clock. Without a clock, `clock.index` is simply undeclared
(`E-UNDECLARED`), and so are `clock.weekday` without a `week:` and `clock.weekdayLabel` without
`week.labels`.

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
`is="Sundy"` or `== 'Sundy'` is caught the same way:

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

## Once a day, once a slot

A beat's `once` (see [Beats](/language/beats/)) gains two values on a project with a clock:

| `once` | Spent | Eligible again |
|---|---|---|
| `day` | from its presentation until the clock's day changes | on the next day |
| `slot` | from its presentation until the clock's slot changes (a new slot, or the same slot on another day) | in the next slot |

Scene beats write it in frontmatter (`once: day`), bundle beats as an attribute
(`<beat … once="day">`), and entries as `once="day"` or `once="slot"`. The engine keeps where in
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

`day` and `slot` mean nothing without a clock, so a project that declares none rejects them:

```lute expect="E-BEAT-ATTR"
---
kind: scene
id: square.bell
on: hubVisit
once: day
---

# The square

## Shot 1.

@narrator: The bell rings.
```

The message says what to do: declare a `clock:` in a schema, or use `run`, `user`, or `false`.

## Moving time

Only the engine moves the clock, and only forward. After every advance it settles the quests (a
[`by=` deadline](/language/quests-and-scenes/) the new time passes fails there), then raises the
clock's `raise` occasion, when it declares one, as an ordinary occasion. The two paths reset by
their tier like any other state, so a clock over `run.day` and `run.slot` starts over at day 1
morning with every new run.

A single `raise: slotStart` is raised once per advance, where the clock stops, never at the slots
it passes. When a day's end or start needs its own story, `raise` takes a map instead, each key
optional:

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

[`lute play`](/tooling/play/) stands in for that engine with an `advance:` step:

- `advance: slot` moves to the next slot, wrapping from the last slot into the next day;
- `advance: <n>` moves `n` slots at once. It never skips a day's close: with `dayEnd` declared it
  stops at each day's last slot on the way to raise it;
- `advance: day` moves to the first slot of the next day, whatever slot it starts from. It raises
  `dayEnd` where the clock stands; the rest of the day is skipped.

On a clock without slots, all three move whole days. The step writes the clock paths, settles, and
raises `slot`, taking the raised occasion's `pick:`, `choose:`, and selection `expect:` exactly as
an `occasion:` step would. From Monday morning, fifteen slots is Saturday morning:

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

An occasion raised once a day should be read once a day, not in every slot row.
`--occasion dayEnd@clock.day,clock.slot=night` evaluates `dayEnd` once per day, at the night slot
(`@run.day,run.slot=night`, the clock's own paths, works too), and leaves the other rows blank.

See [Overviews](/tooling/overviews/) for the rest of the calendar.

## Shipped alongside

Three smaller pieces arrived with the clock (dsl 0.24.0 §1). None of them needs a clock.

### Display labels for enum members

An enum member is an identifier, and an identifier is not always what a player should read. A
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
on: hubVisit
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
on: hubVisit
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

The engine contract (the reserved paths, `once: day` / `once: slot` spending, forward-only
advances) is in
[`docs/runtime/state-lifecycle.md`](https://github.com/journeyWorker/lute/blob/main/docs/runtime/state-lifecycle.md),
and integer `%` in
[`docs/runtime/cel-and-facts.md`](https://github.com/journeyWorker/lute/blob/main/docs/runtime/cel-and-facts.md).
The normative spec is
[`0.24.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.24.0.md)
§1.
