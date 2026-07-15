---
title: Staging directives
description: Single-line staging leaves — ::bg, ::music, ::sfx, ::auto, ::camera, ::cut, ::vfx, ::video — their attributes, timing keys, and the wait blocking model.
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

The core staging directives and their canonical attributes:

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

## Reserved directives

Two `::`-directives are built-in rather than staging vocabulary: `::set` writes declared state (see
[State model](/state/state-model/)) and `::use` expands a reusable content component (see
[Components & extends](/language/components-and-extends/)). Quest documents additionally use
`::assert` / `::retract` to mutate facts (see [Facts & Datalog](/state/facts-and-datalog/)).
