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
| `::bg` | `location`, `time`, `assetId` |
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
::auto{character="bianca" anchor="center" action="fade-in-up"}
::camera{focus="bianca" zoom="1.1" duration="0.5"}
```

*(From [`docs/examples/bianca-s01ep02.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/bianca-s01ep02.lute).)*

Character staging lives on `::auto` with an action id (there is no `::sprite`/`::char`); music
fade-out is `::music{action="fade-out"}`; a character exit is
`::auto{action="fade-out-down"}`. All attribute values are strings, or a bare `@ref` to a
[def](/language/params/) (`::camera{zoom="@closeUp"}`); there are no inline code expressions, which
keeps staging non-Turing-complete.

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
::camera{focus="sofia" zoom="1.4" duration="0.5" wait="true"}  /* holds -> the following line waits for the pan */
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
also the honest version signal — an engine that does not implement 0.8.0 must refuse the artifact
rather than fall through a record it does not recognise.

## Reserved directives

Two `::`-directives are built-in rather than staging vocabulary: `::set` writes declared state (see
[State model](/state/state-model/)) and `::use` expands a reusable content component (see
[Components & extends](/language/components-and-extends/)). Quest documents additionally use
`::assert` / `::retract` to mutate facts (see [Facts & Datalog](/state/facts-and-datalog/)).
