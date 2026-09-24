---
title: Beats
description: "Scenes and lore entries that answer engine occasions — the scene frontmatter keys on, target, when, priority, and once, the entry attributes on= and priority=, how eligible beats are ordered, and what the checker proves about them."
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
| `target` | optional; the scene is a candidate only when the occasion is raised for this target, a dotted id in the `<entry target>` shape (`npc.achilles`, `place.lab_b2`) |
| `when` | optional CEL condition over `run` / `user` / `app` state, `quest.*`, `entry.<id>.read`, and fact queries (`holds(…)`, `count(…)`) |
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

A lore entry answers an occasion with `on=` and `priority=` beside its existing `target` and
`when`:

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

<entry id="achillesFirstMeeting" on="talk" target="npc.achilles" category="bark" priority="20" when="!entry.achillesFirstMeeting.read">
  @achilles: So you are the one who keeps coming back.
</entry>
```

Entries have no `once`. Being read again is normal for an entry: an NPC repeats a bark, a codex
page stays open. An entry that should be heard only once guards on its own `entry.<id>.read`, as
`achillesFirstMeeting` does above. `priority=` without `on=` is `E-BEAT-ATTR`.

## Which beat wins

When the engine raises occasion `O`, optionally for target `T`:

1. The **candidates** are the beats with `on: O` whose `target` is absent or equal to `T`. Scene
   and entry beats on the same occasion compete in one list.
2. A candidate is **eligible** when its `after:` and `when` hold and its `once` is not spent.
3. Eligible beats are ordered by **priority, descending, then project order**: document path,
   then declaration order within the document. `project.index.json` lists every beat in that
   order under `beats`.
4. For a `select: first` occasion the engine presents the first eligible beat. For a `select: all`
   occasion it offers the whole ordered list and the player picks.
5. If no beat is eligible, the occasion passes with no story and the engine does its default.

With the three barks above and a player on their fifth run, talking to Achilles for the first time
presents `achillesFirstMeeting` (priority 20). After that it presents `achillesBark3` (priority 10)
every time, and `achillesBark1` answers only while `user.runs` is below 3.

## Occasions

Occasions are engine vocabulary. A plugin declares them with an `occasions` export:

```yaml
occasions:
  hubVisit:  { select: first }
  talk:      { select: first, target: true }
  inbox:     { select: all }
```

Until some resolved plugin declares occasions, occasion names are **shape-only**: any identifier
is accepted, so you can write beats before the engine's plugin exists. Once any plugin declares
them, a beat naming an undeclared occasion is `E-OCCASION-UNKNOWN`, and a `target` on an occasion
declared without `target: true` is `E-BEAT-ATTR`. The declaration format, `select`, and `target`
are covered on [Playing a story](/tooling/play/#occasions).

## What the checker proves

| Code | When |
|---|---|
| `E-BEAT-ATTR` | a malformed beat key or attribute: `on` not an identifier, `target` not a dotted id, `priority` not an integer, `once` outside `run` / `user` / `false`, beat keys without `on`, or a `target` on an untargeted occasion |
| `E-OCCASION-UNKNOWN` | `on` names an occasion no resolved plugin declares (only once some plugin declares occasions) |
| `E-BEAT-UNREACHABLE` | a scene beat's `when` provably never holds. `lute check` decides scalar conditions per file; `lute check-project` also decides fact queries through the [fact envelope](/state/facts-and-datalog/). An entry beat's dead `when` stays `E-ENTRY-UNREACHABLE`. |
| `W-BEAT-SHADOWED` | `check-project` only: a `select: first` beat that can never win, because an earlier-ordered beat on the same occasion and target is always eligible (no `after:`, and a `when` that is absent or always true) and never spent (an entry, or a scene with `once: false`) |

## Tooling

- `lute play <dir> --script <play.yaml>` raises a scripted sequence of occasions through a whole
  project. It prints every candidate with its verdict, runs the winner, and advances quest
  lifecycles. See [Playing a story](/tooling/play/).
- `lute compile --all` writes `beats` into `project.index.json`. Each compiled scene beat carries
  `meta.beat`, and each entry beat carries `on` / `priority` on its `entry` record.
- `lute context` lists the project's declared occasions beside its other vocabulary.

The engine contract (candidates, eligibility, spending, presentation) is in
[`docs/runtime/beats-and-occasions.md`](https://github.com/journeyWorker/lute/blob/main/docs/runtime/beats-and-occasions.md);
the normative spec is
[`0.21.0.md`](https://github.com/journeyWorker/lute/blob/main/docs/proposals/scenario-dsl/0.21.0.md).
