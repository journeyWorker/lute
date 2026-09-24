---
title: Beats
description: "Scenes, lore entries, and bundled <beat> blocks that answer engine occasions — the scene frontmatter keys on, target, when, priority, once, and also, the entry attributes on=, priority=, and once=, beat bundles in a lore document, occasion target domains, select: first / all / sequence, how eligible beats are ordered and presented, and what the checker proves about them (E-BEAT-ATTR, E-OCCASION-UNKNOWN, E-BEAT-UNREACHABLE, W-BEAT-SHADOWED, W-BEAT-PRIORITY-TIE, W-BEAT-ONCE-RUN-USER)."
---

A lot of games do not advance on a clock. The story moves when something happens: a hub visit,
entering a room, talking to an NPC, a new day, the start of a run. At that moment the game picks
one piece of story whose conditions hold, or none.

Lute calls those moments **occasions** and the pieces of story that answer them **beats** (dsl
0.21.0). The engine raises occasions. Lute declares which beats answer each one, when each is
eligible, and which eligible beat wins. A scene, a [lore entry](/language/lore-entries/), or a
`<beat>` block in a lore document (a [bundle beat](#beat-bundles)) becomes a beat by naming the
occasion it answers. Everything else about the document stays the same.

## A scene beat

```lute check
---
kind: scene
id: achilles.gift
on: talk
target: npc.achilles
when: 'user.runs >= 10 && !run.giftRefused'
priority: 50
once: user
state:
  user.runs: { type: number, default: 0 }
  run.giftRefused: { type: bool, default: false }
---

# Achilles

## Shot 1.

@achilles: You have been at this a while, lad. Take this.
```

When the engine raises `talk` for `npc.achilles`, this scene is a candidate. It is eligible once
the player has started ten runs and has not refused the gift this run. It has never been shown
before, because `once: user` spends it for good the first time it plays.

## Scene beat keys

| Key | Meaning |
|---|---|
| `on` | the occasion this scene answers; it is what makes the scene a beat |
| `target` | optional; the scene is a candidate only when the occasion is raised for this target, a dotted id in the `<entry target>` shape (`npc.achilles`, `place.lab_b2`). When the occasion declares a [target domain](#target-domains), it must be `<prefix>.<member>` of that domain |
| `when` | optional CEL condition over `run` / `user` / `app` state, `quest.*`, `entry.<id>.read` / `entry.<id>.everRead`, and fact queries (`holds(…)`, `count(…)`) |
| `priority` | optional integer, default `0`; higher wins |
| `once` | `run` (the default: at most once per run), `user` (at most once ever), or `false` (repeatable) |
| `also` | optional `true` / `false`, default `false` (dsl 0.23.0). On a `select: first` occasion, an `also` beat is a side remark: it is presented after the winner instead of competing with it. See [Side remarks with `also`](#side-remarks-with-also) |

The beat keys are scene-only and never come from project `defaults:`. A beat belongs to one scene.

`when` is an ordinary CEL slot, checked the way a quest `start` is: the profile's CEL surface,
definite assignment, and the unset-sentinel rules all apply. The one extra rule is that a `when`
may not read the scene's own `scene.*` state. That state does not exist until the scene starts.

`after:` keeps its meaning. It is the structural prerequisite over `visited` / `completed` /
`active` that [connectivity](/connectivity/scene-graph/) analyzes. A beat is eligible only when
**both** `after:` and `when` hold, so put route order in `after:` and state conditions in `when`.

A scene without `on:` is reached by explicit flow, as before. `when`, `target`, `priority`, `once`,
or `also` without `on` is an error:

```lute expect="E-BEAT-ATTR"
---
kind: scene
id: courtyard.rumor
priority: 5
---

# Courtyard

## Shot 1.

@narrator: A rumor drifts across the courtyard.
```

## Entry beats

A lore entry answers an occasion with `on=`, `priority=`, and `once=` beside its existing `target`
and `when`:

```lute check
---
kind: lore
title: Achilles barks
state:
  user.runs: { type: number, default: 0 }
---

<entry id="achillesBark1" on="talk" target="npc.achilles" category="bark">
  @achilles: Keep your guard up.
</entry>

<entry id="achillesBark3" on="talk" target="npc.achilles" category="bark" priority="10" when="user.runs >= 3">
  @achilles: Back again, lad.
</entry>

<entry id="achillesMorning" on="talk" target="npc.achilles" category="bark" priority="20" once="run">
  @achilles: Up early again? Mind the stairs.
</entry>

<entry id="achillesFirstMeeting" on="talk" target="npc.achilles" category="bark" priority="30" once="user">
  @achilles: So you are the one who keeps coming back.
</entry>
```

Without `once`, an entry beat is repeatable. Being read again is normal for an entry: an NPC
repeats a bark, a codex page stays open. `once` (dsl 0.22.0) makes it spendable, and an entry is
spent by its own read flags rather than by a presentation record:

| `once` | Not eligible while | Eligible again |
|---|---|---|
| absent | never spent | — |
| `run` | `entry.<id>.read` is set: it was read this run | at the next run, which resets `read` |
| `user` | `entry.<id>.everRead` is set: it was read in any run | never |

The engine sets both flags after an entry's first read in a run, however it was presented, so an
entry the engine looked up by its `target` spends its `once` too. See
[Reading twice](/language/lore-entries/#reading-twice) for the flags themselves.

`once` takes only `run` or `user`; there is no `once="false"`, so omit the attribute for a
repeatable entry. Any other value is `E-BEAT-ATTR`, and so is `once=` or `priority=` without
`on=`: a repetition policy belongs to a beat. Before 0.22.0 the same effects were spelled as
conditions, `when="!entry.<id>.read"` for once per run and a `user.*` flag the entry set for once
ever. Those still work, but `once` says it directly. An entry never rides along another beat, so
[`also`](#side-remarks-with-also) on an `<entry>` is `E-BEAT-ATTR` too.

## Beat bundles

An interview, a round of talk at the bar, a day's worth of short NPC moments: many beats are a
few lines and a choice each, and a file per beat scatters them across the project. A lore document
can keep them together (dsl 0.23.0). A `<beat>` block is a scene beat written inside a lore
document, beside its entries, and its body is a scene body: lines, branches, hubs, `<match>`, and
directives.

```lute check
---
kind: lore
id: interviews
title: Station interviews
state:
  run.porterTrust: { type: number, default: 0 }
---

<entry id="porterNote" target="item.porter_note" category="note">
  @narrator: A note in the porter's hand: "Lamps out at ten."
</entry>

<beat id="porter" on="talk" target="npc.porter" title="The porter" priority="5">
  @porter: You again.
  <branch id="night">
    <choice id="ask" label="Ask about the night">
      @porter: I saw nothing. Nothing I'll say twice.
      ::set{run.porterTrust += 1}
    </choice>
    <choice id="leave" label="Leave him be">
      @porter: Good.
    </choice>
  </branch>
</beat>

<beat id="porterAgain" on="talk" target="npc.porter" title="The porter, again" when="visited('interviews.porter') && run.porterTrust >= 1">
  @porter: Fine. The lamps went out before ten.
</beat>
```

A `<beat>` takes the scene beat keys as attributes:

| Attribute | Meaning |
|---|---|
| `id` | required; an identifier without `-` (`[A-Za-z][A-Za-z0-9_]*`), unique in the document |
| `on` | required; the occasion the beat answers |
| `target`, `when`, `priority`, `once`, `also` | as on a [scene beat](#scene-beat-keys). `once="false"` makes the beat repeatable, and `also` may be written bare |
| `title` | optional; the label an engine shows for the beat in a `select: all` menu, localized like an entry title |

A bundle beat is a scene beat that lives in a different file:

- Its **canonical id** is `<document id>.<beat id>`, `interviews.porter` above, so the document
  must declare an `id:`. That id is the beat's row in `project.index.json` and its key in the
  presentation record. It shares the project-wide namespace of document ids, so a scene whose `id:`
  is also `interviews.porter` is `E-CONN-EPISODE-ID-DUP`.
- `once` defaults to `run` and is spent by **presentation**, as a scene's is. Read flags do not
  spend it, because it is not an entry.
- Presenting it marks the canonical id visited, so `visited('interviews.porter')` reads it in any
  condition. That is how `porterAgain` waits for the first talk. A bundle beat has no `after:`, and
  an `after:` cannot name it: `after: "visited('interviews.porter')"` is `E-CONN-UNKNOWN-NODE`,
  and the message points you to the `visited()` condition instead.
- It is checked like a scene beat. `E-OCCASION-UNKNOWN`, `E-BEAT-UNREACHABLE`, and the
  `check-project` warnings [below](#what-the-checker-proves) name it by its canonical id.
- Its lines are addressed under the canonical id. The porter's first line is
  `interviews.porter.porter_0010`, and the title's `titleLineId` is `interviews.porter.title`.

Shape faults are `E-BEAT-ATTR`: a missing, malformed, or duplicate `id`, a document with beats but
no `id:`, a missing `on`, or a bad `priority`, `once`, `also`, or `target`. Any other attribute is
`E-UNKNOWN-ATTR` (`after=` included), and a `<beat>` in a scene document is not admitted. Entries
and beats may interleave in any order. [Lore entries](/language/lore-entries/#entries-and-beats-in-one-file)
covers the entry side of the file.

## Which beat wins

When the engine raises occasion `O`, optionally for target `T`:

1. The **candidates** are the beats with `on: O` whose `target` is absent or equal to `T`. Scene,
   entry, and bundle beats on the same occasion compete in one list.
2. A candidate is **eligible** when its `after:` and `when` hold and its `once` is not spent: a
   scene's or bundle beat's by its presentation record, an entry's by its read flags.
3. Eligible beats are ordered by **priority, descending, then project order**: document path,
   then declaration order within the document. `project.index.json` lists every beat in that
   order under `beats`.
4. The occasion's `select` decides what is presented. For `select: first` the engine presents the
   first eligible beat that is not `also` (the **winner**), then every eligible
   [`also`](#side-remarks-with-also) beat. For `select: sequence` it presents every eligible beat,
   in order. For `select: all` it offers the whole ordered list and the player picks one, or closes
   the list without picking. Closing it presents nothing and spends nothing. An engine labels that
   menu with each beat's `title` (a scene's `title:`, an entry's or bundle beat's `title=`), which
   `project.index.json` carries on every beat row (dsl 0.23.0).
5. If no beat is eligible, the occasion passes with no story and the engine does its default.

Take the four barks above and a player on their fifth run. The first time they ever talk to
Achilles, the engine presents `achillesFirstMeeting` (priority 30), and `once="user"` spends it for
good. The next talk that run presents `achillesMorning` (priority 20). After that it presents
`achillesBark3` (priority 10) for the rest of the run, and `achillesBark1` answers only while
`user.runs` is below 3. Every later run opens with `achillesMorning` again, because a new run
resets its `entry.achillesMorning.read`.

## Composing an occasion

`select: first` presents one beat per occasion, and that is usually right: a single main beat keeps
"what happens when I talk to Maud" easy to explain. Two additions in dsl 0.23.0 cover the moments
where one beat is not enough.

### A routine, then the day's event

An occasion declared `select: sequence` presents **every** eligible beat, one after another, in
selection order:

```yaml
occasions:
  evening: { select: sequence }
```

Take a supper scene on `evening` with `priority: 5` and `once: false` that ends with
`::set{ run.day = run.day + 1 }`, and a letter scene on `evening` with `when: 'run.day >= 2'`. The
first evening presents supper alone. Supper moves the day to 2, but the letter's `when` was judged
when `evening` was raised, on day 1. The second evening presents supper, then the letter, because
priority orders the list. The third presents supper alone again, since the letter's default
`once: run` is spent. A sequence has nothing to pick, so a [`lute play`](/tooling/play/#composing-occasions)
step that writes `pick:` on a sequence occasion is a usage error.

### Side remarks with `also`

On a `select: first` occasion, a scene or bundle beat with `also: true` does not compete for the
win. When it is eligible, it is presented **after** the winner, in addition to it. It is still
presented when no main beat is eligible at all.

```lute check
---
kind: scene
id: hub.cat
on: hubVisit
also: true
when: 'run.day >= 2'
once: false
state:
  run.day: { type: number, default: 1 }
---

# The fountain

## Shot 1.

@narrator: The stray cat is back on the fountain's rim, watching you.
```

The winner is the first eligible beat that is not `also`, whatever the `also` beat's priority.
Several eligible `also` beats follow the winner in selection order, and each spends its own
`once`. The cat above says `once: false`, so from day two it follows whichever beat wins every
hub visit.

`also` only means something where a single beat wins. On a `select: all` or `select: sequence`
occasion every eligible beat is already offered or presented, so `also: true` there is
`E-BEAT-ATTR`:

<!-- lute-diagnostics -->
```
./scenes/supper-aside.lute:6:7: error [E-BEAT-ATTR] `also: true` applies only to a `select: first` occasion; `evening` is `select: sequence`, which already presents or offers every eligible beat — remove `also:` (dsl 0.23.0 §3)
```

So is an `also` value other than `true` or `false`. `W-BEAT-SHADOWED` and `W-BEAT-PRIORITY-TIE`
ignore `also` beats: an `also` beat never shadows another beat, is never shadowed, and never ties.

For a sequence and for `also` beats alike, eligibility is decided once, when the occasion is raised:
presenting one beat never makes a later beat of the same list eligible or ineligible. Each
presented beat spends its own `once`, and quests settle after each one, so a quest
[deadline](/language/quests-and-scenes/#deadlines) is judged between two beats of one occasion.

## Occasions

Occasions are engine vocabulary. A plugin declares them with an `occasions` export:

```yaml
occasions:
  hubVisit:  { select: first }
  talk:      { select: first, target: { prefix: npc, entity: person } }
  examine:   { select: first, target: true }
  inbox:     { select: all }
  evening:   { select: sequence }
```

Until some resolved plugin declares occasions, occasion names are **shape-only**: any identifier
is accepted, so you can write beats before the engine's plugin exists. Once any plugin declares
them, a beat naming an undeclared occasion is `E-OCCASION-UNKNOWN`, and a `target` on an untargeted
occasion (one declared with neither `target: true` nor a domain) is `E-BEAT-ATTR`. The declaration
format, `select`, and `target` are covered on [Playing a story](/tooling/play/#occasions).

An occasion can also judge quest objectives: `<objective on="runEnd" …>` evaluates its `done` only
when `runEnd` is raised while its quest is active, and its occasion is checked against the same
vocabulary. Since dsl 0.23.0 an objective can also name a target. `<objective on="talk"
target="npc.maud" …>` is judged only when `talk` is raised for `npc.maud`, and its target is checked
exactly like a beat's (`E-BEAT-ATTR`): a dotted id, only with `on`, only on an occasion that takes a
target, and inside the occasion's domain. Without `target`, an objective is judged whenever its
occasion is raised, whatever the target. See
[Quests & scenes](/language/quests-and-scenes/#quests-meet-scenes-and-occasions).

### Target domains

`target: true` keeps its 0.21.0 meaning: the occasion is raised *for* something, and a beat's
target is any dotted id. So a typo such as `npc.achilels` names a target the engine never raises,
and the beat silently never plays.

A **target domain** `{ prefix, entity }` (dsl 0.22.0) also says *which* things. Every target of the
occasion is `<prefix>.<member>`, where `<member>` is a member of the entity kind `entity`: the same
`entities:` vocabulary your facts use, read from the document's schema (its `uses:`, or the
project's `defaults:`). With the `talk` declaration above and

```yaml
entities:
  person: { members: [maud, oskar] }
```

`target: npc.oskar` answers `talk`, while `target: npc.osker` is `E-BEAT-ATTR` with a did-you-mean:

<!-- lute-diagnostics -->
```
./scenes/oskar.lute:5:9: error [E-BEAT-ATTR] target `npc.osker` is outside occasion `talk`'s domain `npc.<person>` (`npc.maud`, `npc.oskar`) — did you mean `npc.oskar`? (dsl 0.22.0 §8)
```

So is `place.oskar`, which has the wrong prefix. For an `open:` kind, whose members the engine
populates, any `<prefix>.<id>` passes. A domain naming a kind the document's schema does not declare
is `E-BEAT-ATTR` on every target of that occasion. Scene, entry, and bundle beat targets are
checked alike, and so are objective targets. [`lute play`](/tooling/play/) refuses a step target
outside the domain with the same did-you-mean.

## What the checker proves

| Code | When |
|---|---|
| `E-BEAT-ATTR` | a malformed beat key or attribute: `on` not an identifier, `target` not a dotted id or outside its occasion's [target domain](#target-domains), `priority` not an integer, a scene's or bundle beat's `once` outside `run` / `user` / `false` or an entry's `once` outside `run` / `user`, `also` not a bool, on an entry, or on a `select: all` / `sequence` occasion, beat keys without `on`, a `target` on an untargeted occasion, or a [bundle beat](#beat-bundles) shape fault (its `id`, a duplicate id, a lore document with beats but no `id:`) |
| `E-OCCASION-UNKNOWN` | `on` names an occasion no resolved plugin declares (only once some plugin declares occasions) |
| `E-BEAT-UNREACHABLE` | a scene or bundle beat's `when` provably never holds; see [How a `when` is decided](#how-a-when-is-decided). `lute check` decides what one file settles, and `lute check-project` also decides fact queries through the [fact envelope](/state/facts-and-datalog/). An entry beat's dead `when` stays `E-ENTRY-UNREACHABLE`. |
| `W-BEAT-SHADOWED` | `check-project` only: a `select: first` beat that can never win, because an earlier-ordered beat on the same occasion and target is always eligible (no `after:`, and a `when` that is absent or always true) and never spent (an entry without `once`, or a scene or bundle beat with `once: false`). `also` beats neither shadow nor are shadowed. |
| `W-BEAT-PRIORITY-TIE` | `check-project` only: two beats on one `select: first` occasion, either untargeted or for the same target, with equal `priority` and `when`s that are not provably exclusive. When both are eligible, project order picks the winner, so renaming or moving a file changes it. Give one a different `priority`, or make the conditions exclusive: conditions that cannot hold together, such as `run.slot == 'morning'` against `run.slot == 'night'`, `run.day > 5` against `run.day < 3`, or `holds(P)` against `!holds(P)`. A shadowed beat reports `W-BEAT-SHADOWED` instead, and an `also` beat never ties. |
| `W-BEAT-ONCE-RUN-USER` | `check-project` only: a beat spent once per run (a scene's or bundle beat's default `once: run`, or an entry's `once="run"`) whose `when` reads only user-tier state (`user.*`, `entry.<id>.everRead`) and no fact query or `visited()`. Once that condition holds it holds in every run, so the beat plays again each run. Use `once: user` for a beat heard once ever, or gate it on run-tier state. |

### How a `when` is decided

The checker proves what it can about a condition without running the game, and dsl 0.23.0 made it
sharper. Inside one `&&`, it collects what each operand says about each **path**: equalities,
disequalities, and numeric ranges. A path is a state path, a component param, `$` in a `<match>`,
or a relational call such as `holds(P)`, `visited(id)`, or `count(P)`, compared by its text. When
the constraints on one path cannot all hold, the whole conjunction is false. `@def`s are expanded
first. Each of these beats is `E-BEAT-UNREACHABLE` in a plain `lute check`:

| `when` | Why it never holds |
|---|---|
| `run.slot == 'morning' && run.slot == 'night'` | one path, two values |
| `run.day > 5 && run.day < 3` | an empty range |
| `run.flag && !run.flag` | a condition and its negation |
| `holds(met(maud)) && !holds(met(maud))` | the same query, both ways |
| `run.slot == 'afternon'` | a value outside the path's enum |

A condition hidden behind a def is found the same way:

```lute expect="E-BEAT-UNREACHABLE"
---
kind: scene
id: square.festival
on: hubVisit
when: "@festivalNight && run.day == 2"
state:
  run.day: { type: number, default: 1 }
defs:
  festivalNight: { type: bool, cel: "run.day == 5" }
---

# The square

## Shot 1.

@narrator: Lanterns everywhere.
```

An `||` works the other way: when its cases cover every value a path can take, it is always true.
`run.day < 3 || run.day >= 3` holds on every day, so a repeatable beat with that `when` shadows
every later beat on its occasion (`W-BEAT-SHADOWED`).

`unset` counts as a value. Reading an ordering or a bare boolean on an unset path is an error, not
`false`, so a contradiction on a path with no `default` is decided only once the same conjunction
proves the path set: `isSet(run.mood) && run.mood > 5 && run.mood < 3` is false. (Without the
`isSet`, a `when` reading that path is `E-MAYBE-UNSET` anyway.) The reasoning stays within `&&` and
`||`. It does not look inside a Datalog rule's `cel(…)` guard, so a `when` that contradicts the
scalar premise of a derived fact is not caught.

The same decider serves every condition slot. A dead `<when test>` or gated line is `E-ARM-DEAD`, a
dead entry `when` is `E-ENTRY-UNREACHABLE`, a dead objective `done` is
`E-OBJECTIVE-UNSATISFIABLE`, and `W-BEAT-PRIORITY-TIE` asks it whether two `when`s can hold
together. A project that checked clean on 0.22 may report a contradiction it used to miss. The
beat never played before either, but now the checker says so.

### Work in progress

A fact-gated beat is often dead only because the scene that asserts its fact is not written yet.
`lute check-project --wip` (dsl 0.23.0) reports `E-BEAT-UNREACHABLE`, `E-ENTRY-UNREACHABLE`, and
`E-OBJECTIVE-UNSATISFIABLE` as **warnings** when the condition is dead only because some relation
has no producer at all: no seed, no `::assert` anywhere, no rule, and not `reserved:`. The note on
the warning says why. A relation that has producers but can never match stays an error. So does
what follows downstream: a quest that is dead only because its content is unwritten still counts
for connectivity, so an `E-CONN-UNREACHABLE` it causes stays an error. See
[`lute check-project`](/tooling/cli/#check-project).

## Tooling

- `lute play <dir> --script <play.yaml>` raises a scripted sequence of occasions through a whole
  project. It prints every candidate with its verdict, presents the winner and any `also` beats
  (or every eligible beat of a `select: sequence` occasion), and advances quest lifecycles after
  each one. A step on a `select: all` occasion names the player's choice with `pick:`, or closes
  the list with `pick: none`, and a step's `expect: { presented: [ids] }` asserts the presented
  beats in order. Bundle beats play like scene beats. See [Playing a story](/tooling/play/).
- `lute beats <dir>` prints each occasion's (and each target's) beat ladder in selection order,
  with priority, `once`, `also`, `after:`, `when`, title, and the `check-project` verdicts above,
  even for a project that does not check clean yet. `lute calendar <dir> --axis …` evaluates
  eligibility over a grid of state values, such as every day and time slot, and shows what each
  cell presents: the winner, or a sequence's whole list. `lute scenario <dir> knowledge` traces
  each fact-gated beat to the facts it queries and whatever produces them. See
  [Overviews](/tooling/overviews/).
- `lute trace <lore.lute> --beat <id>` previews one bundle beat, and `lute run <artifact> --beat
  <id>` runs one from a compiled lore artifact. An id that names no bundle beat is `E-TRACE-BEAT`
  in `trace` and a usage error in `run`. See [Tracing](/tooling/tracing/#bundle-beats).
- `lute compile --all` writes `beats` into `project.index.json`: one row per scene, entry, or
  bundle beat (kind `bundle`), carrying the expanded `when` and the `title` (dsl 0.23.0). Each
  compiled scene beat carries `meta.beat`, each entry beat carries `on` / `priority` / `once` on
  its `entry` record, and each bundle beat is a `beat` record in its lore artifact, followed by its
  body.
- `lute new scene <name> --on <occasion> [--target <target>]` writes a scene beat, checking the
  occasion and target against the project first.
- `lute context` lists the project's declared occasions, with their target domains, beside its
  other vocabulary.

The engine contract (candidates, eligibility, spending, presentation) is in
[`docs/runtime/beats-and-occasions.md`](https://github.com/journeyWorker/lute/blob/main/docs/runtime/beats-and-occasions.md);
the normative specs are
[`0.21.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md),
[`0.22.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md)
for entry `once` and target domains, and
[`0.23.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.23.0.md)
for `select: sequence`, `also`, beat bundles, the sharper decider, and `--wip`.
