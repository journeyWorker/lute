---
title: Core directives
description: The nine lute.core directives — eight single-line staging leaves plus the walk terminator ::end — with their attributes, timing keys, and the wait blocking model.
---

A **staging directive** is a single-line leaf that stages the scene: background, music, sound,
character entrance, camera, cut-ins, and effects. Its shape is:

```
::name{attributes}
```

Directives never nest — anything with children is a logic block instead (`<branch>`, `<timeline>`,
…). Directive names and attribute meanings are **vocabulary**, extensible by plugins without any
grammar change; run `lute context <file>` to list the directives and attributes your project
accepts.

## Core vocabulary

`lute.core` declares exactly nine directives — eight staging leaves plus the walk terminator
`::end` — with these canonical attributes:

| Directive | Attributes |
|---|---|
| `::bg` | `location`, `time`, `assetId` — a scene change: characters still on stage are hidden first (below) |
| `::music` | `action` (`start`\|`change`\|`stop`\|`resume`\|`fade-out`), `mood`, `volume` (`silent`\|`down`\|`normal`\|`up`\|`full`), `assetId`, `track` |
| `::sfx` | `sound` (description), `assetId`, `name` |
| `::auto` | `character`, `anchor` (`left`\|`center`\|`right`), `action` (a named action id such as `fade-in-up` / `pose-*`) — character entrance/exit/pose |
| `::camera` | `focus`, `zoom`, `move-x`, `move-y`, `shake`, `reset`, `duration`, `easing`, `delay`, `wait` |
| `::cut` | `assetId` (`CUT.*`), `action` (`show`\|`hide`), `full` |
| `::vfx` | `type` (e.g. `whiteOut`, `petals`), `label`, `transition` |
| `::video` | `assetId` (`VID.*`), `action` (`show`\|`hide`), `wait` |
| `::end` | `reason` (optional, free-form) — terminates the walk; control flow, not staging (below) |

```lute
::bg{location="family_restaurant" time="afternoon" assetId="BG.space.family_restaurant.interior.afternoon"}
::music{action="start" mood="peaceful" assetId="sound-bgm-common-vn-mood-peaceful-0.mp3" volume="down"}
::auto{character="marina" anchor="center" action="fade-in-up"}
::camera{focus="marina" zoom="1.1" duration="0.5"}
```

*(From [`docs/examples/marina-s01ep02.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/marina-s01ep02.lute).)*

Character staging lives on `::auto` with an action id (there is no `::sprite`/`::char`); music
fade-out is `::music{action="fade-out"}`; a character exit is
`::auto{action="fade-out-down"}`. All attribute values are strings in double quotes (a `"` inside
one is written `\"`; single quotes are `E-ATTR-QUOTE`), or a bare `@ref` to a
[def](/language/params/) that folds to a constant (`::camera{zoom=@closeUp}`; a def that reads state
is `E-ATTR-DEF-DYNAMIC`). There are no inline code expressions, which keeps staging
non-Turing-complete.

A `::bg` is a **scene change**. Every character still on stage is hidden just before it by an
injected `::auto` record (`provenance.by: "stage-bookkeeping"`). A character who keeps speaking in
the new place must enter again with `::auto`; a line from one before that is `W-STAGE-ABSENT`
([below](#stage-state)), so re-enter them explicitly:

```lute
::bg{location="station" time="night"}
::auto{character="marina" action="fade-in-up"}
@marina: The last train is gone.
::bg{location="street" time="night"}
::auto{character="marina" action="fade-in-up"}
@marina: We walk, then.
```

## Stage state

The checker threads a **stage state** through the document — who is on stage, where, and in what
pose — and `lute compile` injects its staging records from the same state. A character's first
line needs no `::auto`: a speaker who has never been shown enters implicitly, and nothing warns.
What warns is staging someone the state says has **left**: after a declared exit (an `::auto`
whose `action` is in the `action` domain's `exits:` list) or a `::bg` auto-hide, a line by that
character — or another declared exit — before an `::auto` shows them again is `W-STAGE-ABSENT`.

Since 0.22.0 the stage state follows paths:

- It **forks** at every `<branch>` or `<hub>` choice and every `<match>` arm. Each arm starts from
  the stage as it was at the fork, so an exit in one arm never warns on a line in its sibling.
- It **joins** where the arms converge, keeping only what holds on every arm. After the
  convergence a character is on stage only if every arm left them there; one taken off on any arm
  warns when staged again without a re-show.
- A `::bg` auto-hide records the hidden characters as **exited**, so a later line by one of them
  warns until an `::auto` brings them back, and a scene change no longer forgets an earlier
  declared exit. (Before 0.22.0 both checked clean.)

```lute check
---
kind: scene
id: demo.platform
enums:
  action:
    members: [fade-in-up, fade-out-down]
    exits: [fade-out-down]
  anchor:
    members: [left, center, right]
    default: center
---

## The Platform

::bg{location="station" time="night"}
::auto{character="marina" action="fade-in-up"}
@marina: The last train is gone.
<branch id="wait">
  <choice id="leave" label="Let her go">
    ::auto{character="marina" action="fade-out-down"}
    @narrator: She walks off without a word.
  </choice>
  <choice id="stay" label="Ask her to stay">
    @marina: Fine. One more minute.
  </choice>
</branch>
@marina: So, what now?
```

`@marina: Fine. One more minute.` is silent — on the `stay` path she never left. The line after
the branch is not, because the `leave` path reaches it with her gone:

<!-- lute-diagnostics -->
```
platform.lute:27:1: warning [W-STAGE-ABSENT] `marina` left the stage on an earlier declared exit (line 20) on a path that reaches here and has not been shown again, so a spoken line here stages someone who is not present. Show them again with an `::auto` before this point, or remove the earlier exit (dsl 0.10.0 §11.2, 0.22.0 §12)
```

An `::auto{character="marina" action="fade-in-up"}` before that line, or at the end of the `leave`
choice, puts her on stage on every path and silences it. `lute compile` stages the artifact over
the same join: after the convergence she is not on stage, so an `::auto` there is a fresh entrance
and gets its anchor again.

## Timing & the `wait` model

`duration`, `delay`, and `wait` are reserved **staging** timing keys that may appear on any
directive:

- **`duration`** — the transform length (e.g. `duration="0.6"`).
- **`delay`** — an offset from the directive's own slot start.
- **`wait`** — blocking control.

`wait="true"` holds the script until that effect completes; an absent or `false` `wait` is
non-blocking, so the next line proceeds concurrently. The default is **per-directive**, not global
— for example `::video` and background default to `wait="true"`, while most effects default
non-blocking. Concurrency is therefore just consecutive non-`wait` directives; there is no
`<parallel>` wrapper.

```lute
::camera{shake="0.3" duration="0.2"}                    /* no wait -> next line runs concurrently */
::camera{focus="elena" zoom="1.4" duration="0.5" wait="true"}  /* holds -> the following line waits for the pan */
```

The `at` key is *not* a staging timing attribute; it is a timeline-position key valid only on
clips inside a [`<timeline>`](/language/timeline-and-property-tracks/).

## `::end` — terminating the walk

`::end` stops the walk at its own record. It is exactly equivalent to falling off the end of the
command array, except that it carries a reason:

```lute
@narrator: The platform emptied out. Nothing left to catch.
::end{reason="missedTheLastTrain"}
```

`reason` is optional and free-form — `"completed"`, `"error"`, an ending id. Lute assigns it no
meaning; it rides through to the artifact for the host to surface. `::end` is the only entry in the
table above that is not a staging leaf, so it is not admitted inside a
[`<track>`](/language/timeline-and-property-tracks/) clip (`E-TIMELINE-CONTENT`), which takes
staging leaves and `::set` only.

In [`lute play`](/tooling/play/), an `::end` ends only the presentation (or quest handler) it runs
in; the step still settles and the playthrough goes on with the next step. A play script stops
early with a step `end: true`.

Anything after an `::end` **in the same straight-line body** can never run, and the checker says so
once per body, anchored at the first dead node: `W-CODE-AFTER-END` — *unreachable content after
`::end` (the walk terminates here)*. It is a warning; promote it with
`lute check --deny W-CODE-AFTER-END`.

The scope is the *immediately enclosing* sequence only — one shot body, one `<choice>` body, one
`<when>` arm, one objective body, one `<on>` body. An `::end` in one choice says nothing about its
siblings, nor about content after the enclosing `<branch>`, so a per-branch ending is written the
obvious way and the shared tail below it stays live:

```lute
<branch id="ledge">
  <choice id="jump" label="Jump for it">
    @mira: Nothing to it.
    ::end{reason="fell"}
  </choice>
  <choice id="wait" label="Wait for the ladder">
    @mira: I can be patient.
  </choice>
</branch>

@narrator: The siren faded somewhere east.
```

### Why termination is core, not a plugin directive

Termination is control flow, and control flow is the one thing plugin vocabulary does not get. A
plugin directive lowers to a record of `kind: "plugin"`, which is opaque to the checker:
reachability analysis cannot know that such a record terminates, so content after it would never be
reported dead and `W-CODE-AFTER-END` could not exist. Shipping `::end` as a new IR command kind is
also the honest version signal — an engine that does not implement the IR minor that introduced it
(`0.8.0`) must refuse the artifact rather than fall through a record it does not recognise.

## Reserved directives

Three `::`-directives are built-in rather than staging vocabulary: `::set` writes declared state (see
[State model](/state/state-model/)), `::use` expands a reusable content component (see
[Components & extends](/language/components-and-extends/)), and `::accept` takes up a quest (below).
Content additionally uses
`::assert` / `::retract` to mutate facts (see [Facts & Datalog](/state/facts-and-datalog/)) — in
scenes as well as quests. `docs/examples/haven/scenes/cryobank.lute` is `kind: scene` and carries
four `::assert` directives inside `<choice>` bodies; `lute check` on it reports
`ok … (0 warning(s))`.

### `::accept` — taking up a quest

`::accept{quest="<id>"}` (dsl 0.21.0) declares that the player accepts an **accept-driven** quest —
one with no `start` predicate — at this point in a scene. It is the scene-side form of the engine's
"accept quest" action and of `lute trace --accept`, and it usually sits in the choice where the
player agrees:

```lute
<branch id="request">
  <choice id="accept" label="I'll keep it calm">
    ::accept{quest="calmTheShed"}
    @vesna: Thank you.
  </choice>
  <choice id="decline" label="Not now">
    @vesna: Another time, then.
  </choice>
</branch>
```

Like `::set` and `::assert`, it is built in, not plugin vocabulary: it lowers to its own IR record,
`{"kind": "accept", "addr": …, "quest": "calmTheShed"}`, and the engine activates the quest if it is
still `unset` and ignores the record otherwise. The target is checked twice: a missing or
non-identifier `quest` is `E-ACCEPT-TARGET` in `lute check`, and `check-project` reports
`E-ACCEPT-TARGET` when the id names no quest in the project or names a quest that has a `start`
predicate (such a quest activates itself; accepting it means nothing). See
[Quests & scenes](/language/quests-and-scenes/#quests-meet-scenes-and-occasions).
