---
title: Beats
description: "Scenes and lore entries that answer engine occasions — the scene frontmatter keys on, target, when, priority, and once, the entry attributes on=, priority=, and once=, occasion target domains, how eligible beats are ordered, and what the checker proves about them."
---

A lot of games do not advance on a clock. The story moves when something happens: a hub visit,
entering a room, talking to an NPC, a new day, the start of a run. At that moment the game picks
one piece of story whose conditions hold, or none.

Lute calls those moments **occasions** and the pieces of story that answer them **beats** (dsl
0.21.0). The engine raises occasions. Lute declares which beats answer each one, when each is
eligible, and which eligible beat wins. A scene or a [lore entry](/language/lore-entries/) becomes
a beat by naming the occasion it answers. Everything else about the document stays the same.

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

The beat keys are scene-only and never come from project `defaults:`. A beat belongs to one scene.

`when` is an ordinary CEL slot, checked the way a quest `start` is: the profile's CEL surface,
definite assignment, and the unset-sentinel rules all apply. The one extra rule is that a `when`
may not read the scene's own `scene.*` state. That state does not exist until the scene starts.

`after:` keeps its meaning. It is the structural prerequisite over `visited` / `completed` /
`active` that [connectivity](/connectivity/scene-graph/) analyzes. A beat is eligible only when
**both** `after:` and `when` hold, so put route order in `after:` and state conditions in `when`.

A scene without `on:` is reached by explicit flow, as before. `when`, `target`, `priority`, or
`once` without `on` is an error:

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
ever. Those still work, but `once` says it directly.

## Which beat wins

When the engine raises occasion `O`, optionally for target `T`:

1. The **candidates** are the beats with `on: O` whose `target` is absent or equal to `T`. Scene
   and entry beats on the same occasion compete in one list.
2. A candidate is **eligible** when its `after:` and `when` hold and its `once` is not spent: a
   scene's by its presentation record, an entry's by its read flags.
3. Eligible beats are ordered by **priority, descending, then project order**: document path,
   then declaration order within the document. `project.index.json` lists every beat in that
   order under `beats`.
4. For a `select: first` occasion the engine presents the first eligible beat. For a `select: all`
   occasion it offers the whole ordered list and the player picks one, or closes the list without
   picking. Closing it presents nothing and spends nothing.
5. If no beat is eligible, the occasion passes with no story and the engine does its default.

Take the four barks above and a player on their fifth run. The first time they ever talk to
Achilles, the engine presents `achillesFirstMeeting` (priority 30), and `once="user"` spends it for
good. The next talk that run presents `achillesMorning` (priority 20). After that it presents
`achillesBark3` (priority 10) for the rest of the run, and `achillesBark1` answers only while
`user.runs` is below 3. Every later run opens with `achillesMorning` again, because a new run
resets its `entry.achillesMorning.read`.

## Occasions

Occasions are engine vocabulary. A plugin declares them with an `occasions` export:

```yaml
occasions:
  hubVisit:  { select: first }
  talk:      { select: first, target: { prefix: npc, entity: person } }
  examine:   { select: first, target: true }
  inbox:     { select: all }
```

Until some resolved plugin declares occasions, occasion names are **shape-only**: any identifier
is accepted, so you can write beats before the engine's plugin exists. Once any plugin declares
them, a beat naming an undeclared occasion is `E-OCCASION-UNKNOWN`, and a `target` on an untargeted
occasion (one declared with neither `target: true` nor a domain) is `E-BEAT-ATTR`. The declaration
format, `select`, and `target` are covered on [Playing a story](/tooling/play/#occasions).

An occasion can also judge quest objectives: `<objective on="runEnd" …>` evaluates its `done` only
when `runEnd` is raised while its quest is active, and its occasion is checked against the same
vocabulary. An objective takes `on=` only. It has no `target` attribute in 0.22.0
(`E-UNKNOWN-ATTR`; objective targets are deferred to 0.23), so it is judged whenever its occasion is
raised, whatever the target. See
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
is `E-BEAT-ATTR` on every target of that occasion. Scene and entry beat targets are checked alike,
and [`lute play`](/tooling/play/) refuses a step target outside the domain with the same
did-you-mean.

## What the checker proves

| Code | When |
|---|---|
| `E-BEAT-ATTR` | a malformed beat key or attribute: `on` not an identifier, `target` not a dotted id or outside its occasion's [target domain](#target-domains), `priority` not an integer, a scene's `once` outside `run` / `user` / `false` or an entry's `once` outside `run` / `user`, beat keys without `on`, or a `target` on an untargeted occasion |
| `E-OCCASION-UNKNOWN` | `on` names an occasion no resolved plugin declares (only once some plugin declares occasions) |
| `E-BEAT-UNREACHABLE` | a scene beat's `when` provably never holds. `lute check` decides what the literals alone settle — a constant `false` conjunct, or a comparison against a value outside the path's enum (`run.slot == 'afternon'`) — but treats every state path as able to hold any value, so it does not see a contradiction between two conditions on one path (`run.n == 3 && run.n == 4`, `run.flag && !run.flag`). `lute check-project` also decides fact queries through the [fact envelope](/state/facts-and-datalog/). An entry beat's dead `when` stays `E-ENTRY-UNREACHABLE`. |
| `W-BEAT-SHADOWED` | `check-project` only: a `select: first` beat that can never win, because an earlier-ordered beat on the same occasion and target is always eligible (no `after:`, and a `when` that is absent or always true) and never spent (an entry without `once`, or a scene with `once: false`) |
| `W-BEAT-PRIORITY-TIE` | `check-project` only: two beats on one `select: first` occasion, either untargeted or for the same target, with equal `priority` and `when`s that are not provably exclusive. When both are eligible, project order picks the winner, so renaming or moving a file changes it. Give one a different `priority`, or make the conditions exclusive: two comparisons that pin one path to different values (`run.slot == 'morning'` against `run.slot == 'night'`) or `holds(P)` against `!holds(P)`. A shadowed beat reports `W-BEAT-SHADOWED` instead. |
| `W-BEAT-ONCE-RUN-USER` | `check-project` only: a beat spent once per run (a scene's default `once: run`, or an entry's `once="run"`) whose `when` reads only user-tier state (`user.*`, `entry.<id>.everRead`) and no fact query or `visited()`. Once that condition holds it holds in every run, so the beat plays again each run. Use `once: user` for a beat heard once ever, or gate it on run-tier state. |

## Tooling

- `lute play <dir> --script <play.yaml>` raises a scripted sequence of occasions through a whole
  project. It prints every candidate with its verdict, runs the winner, and advances quest
  lifecycles. A step on a `select: all` occasion names the player's choice with `pick:`, or closes
  the list with `pick: none`. See [Playing a story](/tooling/play/).
- `lute compile --all` writes `beats` into `project.index.json`. Each compiled scene beat carries
  `meta.beat`, and each entry beat carries `on` / `priority` / `once` on its `entry` record.
- `lute new scene <name> --on <occasion> [--target <target>]` writes a scene beat, checking the
  occasion and target against the project first.
- `lute context` lists the project's declared occasions, with their target domains, beside its
  other vocabulary.

The engine contract (candidates, eligibility, spending, presentation) is in
[`docs/runtime/beats-and-occasions.md`](https://github.com/journeyWorker/lute/blob/main/docs/runtime/beats-and-occasions.md);
the normative specs are
[`0.21.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md)
and, for entry `once` and target domains,
[`0.22.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.22.0.md).
