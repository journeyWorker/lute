---
title: Core directives
description: The thirteen lute.core directives — ten staging leaves (::bg, ::music, ::sfx, ::actor, ::clear, ::camera, ::cg, ::vfx, ::video, ::sequence), the walk terminator ::end, and the forward jump pair ::jump / ::label — with their attributes, the project-declared staging vocabulary, attribute quoting, timing keys, the wait blocking model, and the when= guard.
---

A **staging directive** is a single-line leaf that stages the scene: background, music, sound,
actors, camera, event illustrations (CGs), video, cinematic sequences, and effects. Its shape is:

```
::name{attributes}
```

Directives never nest — anything with children is a logic block instead (`<branch>`, `<timeline>`,
…). Directive names and attribute meanings are **vocabulary**, extensible by plugins without any
grammar change; run `lute context <file>` to list the directives and attributes your project
accepts.

## Core vocabulary

`lute.core` declares thirteen directives — ten staging leaves, the walk terminator `::end`, and the
forward jump pair `::jump` / `::label` — with these canonical attributes:

| Directive | Attributes |
|---|---|
| `::bg` | `location`, `time`, `assetId` — a scene change: characters still on stage are hidden first (below) |
| `::music` | `playback` (domain `musicPlayback`), `mood` (domain `mood`), `volume` (domain `volume`), `assetId` |
| `::sfx` | `sound` (what is heard), `assetId` (the concrete file) |
| `::actor` | `character` (required), `anchor` (domain `anchor`), `action` (domain `action`), `emotion` (domain `emotion`), `costume` (domain `costume`) — entrance, exit, pose, expression, outfit |
| `::clear` | none — every character on stage exits; background and music stay (below) |
| `::camera` | `focus` (a cast member), `framing` (domain `framing`), `move` (domain `cameraMove`), `transition` (domain `transition`) — at least one |
| `::cg` | `assetId` (required), `display` (`show`\|`hide`, default `show`), `layout` (domain `cgLayout`) — an event illustration |
| `::vfx` | `type` (domain `vfxType`), `label`, `transition` |
| `::video` | `assetId` (required), `display` (`show`\|`hide`, default `show`) |
| `::sequence` | `name` (required, domain `sequence`) — a reference to a cinematic the engine owns; `wait` defaults to `true` |
| `::end` | `reason` (optional, free-form) — terminates the walk; control flow, not staging (below) |
| `::jump` | `to` (required) — jumps forward to a `::label`; control flow (below) |
| `::label` | `name` (required) — names a position for `::jump`; emits no record (below) |

```lute
::bg{location="family_restaurant" time="afternoon" assetId="BG.space.family_restaurant.interior.afternoon"}
::music{playback="start" mood="peaceful" assetId="sound-bgm-common-vn-mood-peaceful-0.mp3" volume="down"}
::actor{character="marina" anchor="center" action="fadeInUp"}
::camera{focus="marina" framing="close" duration="0.5"}
```

*(From [`docs/examples/marina-s01ep02.lute`](https://github.com/journeyWorker/lute/blob/main/docs/examples/marina-s01ep02.lute).)*

Character staging lives on `::actor`; music fading out is
`::music{playback="fadeOut"}`; a character exit is `::actor{character="marina" action="fadeOutDown"}`
where `fadeOutDown` is listed in the `action` domain's `exits:`. All attribute values are strings
in double quotes (a `"` inside one is written `\"` or
[`&quot;`](#character-references-in-quoted-values); single quotes are `E-ATTR-QUOTE`), or a bare
`@ref` to a [def](/language/params/) that folds to a constant (`::camera{focus=@hero}`; a def that
reads state is `E-ATTR-DEF-DYNAMIC`). There are no inline code expressions, which keeps staging
non-Turing-complete.

A `::bg` is a **scene change**. Every character still on stage is hidden just before it by an
injected `::actor` record (`provenance.by: "stage-bookkeeping"`). A character who keeps speaking in
the new place must enter again with `::actor`; a line from one before that is `W-STAGE-ABSENT`
([below](#stage-state)), so re-enter them explicitly:

```lute
::bg{location="station" time="night"}
::actor{character="marina" action="fadeInUp"}
@marina: The last train is gone.
::bg{location="street" time="night"}
::actor{character="marina" action="fadeInUp"}
@marina: We walk, then.
```

## Staging vocabulary is the project's

The core declares the **slots**; the project declares the **members**. Every domain-typed attribute
in the table above reads a closed domain declared in `enums:` — in the document's frontmatter, or in
a schema it reaches through `uses:` (see [Content vocabulary](/language/vocabulary/)). The core ships
no members for any of them, so one project's camera says `framing="closeUp"` and another's says
`framing="twoShot"`, and the checker holds each to its own list:

| Domain | Read by |
|---|---|
| `anchor` | `::actor{anchor}` — its `default` is used when `anchor` is omitted |
| `action` | `::actor{action}` and a line's `action` — members in `exits:` take the character off stage |
| `emotion` | `::actor{emotion}` and a line's `emotion` |
| `costume` | `::actor{costume}` |
| `framing` | `::camera{framing}` |
| `cameraMove` | `::camera{move}` |
| `transition` | `::camera{transition}` |
| `cgLayout` | `::cg{layout}` |
| `musicPlayback` | `::music{playback}` |
| `sequence` | `::sequence{name}` |
| `mood`, `volume` | `::music{mood}`, `::music{volume}` |
| `vfxType` | `::vfx{type}` |
| `textStyle` | [inline style spans](/language/dialogue-and-cast/#inline-text-modifiers) in line text |

A value outside its domain is `E-BAD-ENUM`; a domain the document never declares is
`E-DOMAIN-UNKNOWN`. A project does not declare new directives this way — it fills core slots:

```lute check
---
kind: scene
id: demo.rooftop
enums:
  anchor:
    members: [left, center, right]
    default: center
  action:
    members: [fadeInUp, fadeOutDown]
    exits: [fadeOutDown]
  emotion: [neutral, angry, shy]
  costume: [school, festival]
  framing: [wide, close, closeUp]
  cameraMove: [pushIn, shake]
  transition: [cut, dissolve]
  cgLayout: [full, inset]
  sequence: [fireworks]
  musicPlayback: [start, fadeOut]
  mood: [romantic]
---

## The Rooftop {#rooftop}

::bg{location="rooftop" time="night"}
::music{playback="start" mood="romantic"}
::actor{character="marina" anchor="left" action="fadeInUp" costume="festival"}
::camera{focus="marina" framing="close" transition="dissolve" duration="0.5"}
@marina: You came.
::actor{character="marina" emotion="shy"}
::camera{move="shake" duration="0.3"}
::sequence{name="fireworks"}
::cg{assetId="CUT.rooftop.fireworks" layout="full"}
@narrator: The sky opens in colour.
::cg{assetId="CUT.rooftop.fireworks" display="hide"}
::video{assetId="VID.rooftop.ending"}
::music{playback="fadeOut" duration="2"}
::actor{character="marina" action="fadeOutDown"}
```

Each directive compiles to one staging record whose `kind` and fields carry the directive's and
attributes' own names (`lute compile`, trimmed to the staging records):

```json
{"kind": "bg", "family": "staging", "position": "001-0100", "location": "rooftop", "time": "night", "timing": {"wait": true}}
{"kind": "music", "family": "staging", "position": "001-0200", "playback": "start", "mood": "romantic"}
{"kind": "actor", "family": "staging", "position": "001-0300", "character": "marina", "anchor": "left", "action": "fadeInUp", "costume": "festival"}
{"kind": "camera", "family": "staging", "position": "001-0400", "focus": "marina", "framing": "close", "transition": "dissolve", "timing": {"wait": false, "duration": 0.5}}
{"kind": "actor", "family": "staging", "position": "001-0600", "character": "marina", "emotion": "shy"}
{"kind": "camera", "family": "staging", "position": "001-0700", "move": "shake", "timing": {"wait": false, "duration": 0.3}}
{"kind": "sequence", "family": "staging", "position": "001-0800", "name": "fireworks", "timing": {"wait": true}}
{"kind": "cg", "family": "staging", "position": "001-0900", "assetId": "CUT.rooftop.fireworks", "display": "show", "layout": "full", "timing": {"wait": false}}
{"kind": "cg", "family": "staging", "position": "001-1100", "assetId": "CUT.rooftop.fireworks", "display": "hide", "timing": {"wait": false}}
{"kind": "video", "family": "staging", "position": "001-1200", "assetId": "VID.rooftop.ending", "display": "show", "timing": {"wait": true}}
{"kind": "music", "family": "staging", "position": "001-1300", "playback": "fadeOut", "timing": {"duration": 2.0}}
{"kind": "actor", "family": "staging", "position": "001-1400", "character": "marina", "action": "fadeOutDown", "exit": true}
```

A few rules ride on these records:

- **`::actor` without a line.** An `::actor` that only sets `emotion` or `costume` changes the
  character's expression or outfit in place; an authored `emotion` updates the stage state exactly
  as a line's `emotion` does. `exit: true` appears only for an `action` listed in `exits:`.
- **Camera members are opaque.** The engine decides what `close` or `shake` looks like; Lute
  checks membership and the `focus` cast reference, and synthesizes no numeric transform. A
  `::camera` with none of `focus`, `framing`, `move`, `transition` is `E-CAMERA-EMPTY`. The camera
  is not part of the saved state.
- **`display` is always emitted.** `::cg` and `::video` resolve an omitted `display` to `show`, so
  the record always says which way it goes.
- **A sequence is a reference.** `::sequence{name}` names a cinematic the engine plays; Lute does
  not describe its contents, and it blocks (`wait: true`) unless written `wait="false"`.
  [`lute play`](/tooling/play/) records it without simulating it.

### Character references in quoted values

A double-quoted attribute value, on a directive, a content line or a tag such as `<choice>`, decodes
the XML character references `&quot;` `&apos;` `&amp;` `&lt;` `&gt;` and the numeric forms `&#NN;` /
`&#xHH;` (0.24.0). Before 0.24.0 the entity text shipped literally.

```lute check
---
kind: scene
id: demo.sign
---

## The Sign

::sfx{sound="a &quot;ding&quot; &amp; a hiss"}
<branch id="sign">
  <choice id="read" text="Read &quot;No Entry&quot; aloud">
    @narrator: You read it aloud.
  </choice>
  <choice id="fish" text="Order fish & chips, \"to go\"">
    @narrator: You order.
  </choice>
</branch>
```

The artifact carries `a "ding" & a hiss` and `Read "No Entry" aloud`. Decoding is a single pass, so
`&amp;quot;` becomes `&quot;` rather than `"`. Any other `&` stays literal: a bare `&` as in
`fish & chips`, a `&&`, or a reference outside the list such as `&nbsp;`. The `\"` escape still
works. Content text after the line's colon is not an attribute value and is never decoded.

## Stage state

The checker threads a **stage state** through the document — who is on stage, where, and in what
pose — and `lute compile` injects its staging records from the same state. A character's first
line needs no `::actor`: a speaker who has never been shown enters implicitly, and nothing warns.
What warns is staging someone the state says has **left**: after a declared exit (an `::actor`
whose `action` is in the `action` domain's `exits:` list), a `::bg` auto-hide, or a
[`::clear`](#clear--emptying-the-stage), a line by that character — or another declared exit —
before an `::actor` shows them again is `W-STAGE-ABSENT`.

Since 0.22.0 the stage state follows paths:

- It **forks** at every `<branch>` or `<hub>` choice and every `<match>` arm. Each arm starts from
  the stage as it was at the fork, so an exit in one arm never warns on a line in its sibling.
- It **joins** where the arms converge, keeping only what holds on every arm. After the
  convergence a character is on stage only if every arm left them there; one taken off on any arm
  warns when staged again without a re-show.
- A `::bg` auto-hide records the hidden characters as **exited**, so a later line by one of them
  warns until an `::actor` brings them back, and a scene change no longer forgets an earlier
  declared exit. (Before 0.22.0 both checked clean.)

```lute check
---
kind: scene
id: demo.platform
enums:
  action:
    members: [fadeInUp, fadeOutDown]
    exits: [fadeOutDown]
  anchor:
    members: [left, center, right]
    default: center
---

## The Platform

::bg{location="station" time="night"}
::actor{character="marina" action="fadeInUp"}
@marina: The last train is gone.
<branch id="wait">
  <choice id="leave" text="Let her go">
    ::actor{character="marina" action="fadeOutDown"}
    @narrator: She walks off without a word.
  </choice>
  <choice id="stay" text="Ask her to stay">
    @marina: Fine. One more minute.
  </choice>
</branch>
@marina: So, what now?
```

`@marina: Fine. One more minute.` is silent — on the `stay` path she never left. The line after
the branch is not, because the `leave` path reaches it with her gone:

<!-- lute-diagnostics -->
```
platform.lute:27:1: warning [W-STAGE-ABSENT] `marina` left the stage on an earlier declared exit (line 20) on a path that reaches here and has not been shown again, so a spoken line here stages someone who is not present. Show them again with an `::actor` before this point, or remove the earlier exit
```

An `::actor{character="marina" action="fadeInUp"}` before that line, or at the end of the `leave`
choice, puts her on stage on every path and silences it. `lute compile` stages the artifact over
the same join: after the convergence she is not on stage, so an `::actor` there is a fresh entrance
and gets its anchor again.

## `::clear` — emptying the stage

`::clear` (dsl 0.24.0 §4) takes every character off the stage at once. It has no attributes. The
background and the music stay, which is what separates it from a `::bg` scene change:

```lute check
---
kind: scene
id: demo.lastTrain
enums:
  action:
    members: [fadeInUp, fadeOutDown]
    exits: [fadeOutDown]
  anchor:
    members: [left, center, right]
    default: center
  musicPlayback: [start, stop]
  mood: [peaceful]
---

## The Last Train

::bg{location="station" time="night"}
::music{playback="start" mood="peaceful"}
::actor{character="marina" action="fadeInUp"}
::actor{character="oskar" action="fadeInUp"}
@marina: The last train is gone.
@oskar: So it is.
::clear
@narrator: The platform empties. The music plays on.
@marina: Wait for me.
```

Every character the stage state holds exits, including one who is on stage on only some of the
paths that reach the `::clear`, as at a `::bg`. The directive compiles to one `actor` exit record
per character, with no record of its own and no new IR kind, so an engine needs nothing new:

```json
{ "kind": "actor", "family": "staging", "position": "001-0900", "character": "marina", "exit": true, "provenance": { "by": "stage-clear", "explanation": "`::clear` takes `marina` off stage" } }
{ "kind": "actor", "family": "staging", "position": "001-1000", "character": "oskar", "exit": true, "provenance": { "by": "stage-clear", "explanation": "`::clear` takes `oskar` off stage" } }
```

A cleared character is gone until an `::actor` shows them again, so the last line above warns, and
the warning names the `::clear`:

<!-- lute-diagnostics -->
```
platform.lute:25:1: warning [W-STAGE-ABSENT] `marina` was taken off stage by an earlier `::clear` (line 23) on a path that reaches here and has not been shown again, so a spoken line here stages someone who is not present. Show them again with an `::actor` after the `::clear`
```

[`lute play`](/tooling/play/) prints `::clear` where it ran, and `lute trace` records it as an
exit.

## Timing & the `wait` model

`duration`, `delay`, and `wait` are reserved **staging** timing keys that may appear on any
directive but `::clear`:

- **`duration`** — the transform length in seconds (e.g. `duration="0.6"`).
- **`delay`** — an offset in seconds from the directive's own slot start.
- **`wait`** — blocking control (`true` / `false`).

`wait="true"` holds the script until that effect completes; an absent or `false` `wait` is
non-blocking, so the next line proceeds concurrently. The default is **per-directive**, not global
— for example `::bg`, `::video` and `::sequence` default to `wait="true"`, while most effects default
non-blocking. Concurrency is therefore just consecutive non-`wait` directives; there is no
`<parallel>` wrapper.

```lute
::camera{move="shake" duration="0.2"}                    /* no wait -> next line runs concurrently */
::camera{focus="elena" framing="closeUp" duration="0.5" wait="true"}  /* holds -> the following line waits for the push */
```

In the artifact the resolved values sit in the record's `timing` object —
`"timing": {"wait": true, "duration": 0.5}` — and `timing` is omitted when none is set.

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
meaning; it rides through to the artifact for the host to surface. `::end`, `::jump` and `::label`
are the entries in the table above that are not staging leaves, so none is admitted inside a
[`<track>`](/language/timeline-and-property-tracks/) clip (`E-TIMELINE-CONTENT`), which takes
staging leaves and `::set` only.

In [`lute play`](/tooling/play/), an `::end` ends only the presentation (or quest handler) it runs
in; the step still settles and the playthrough goes on with the next step. A play script stops
early with a step `end: true`. `::end` never ends the game: a game that can be over says so with a
schema's [`terminal:`](/state/schemas/#the-end-of-the-game-terminal), after which the engine raises
no occasion. [The game is over](/tooling/play/#the-game-is-over) sets the five meanings of "end"
side by side.

Anything after an `::end` **in the same straight-line body** can never run, and the checker says so
once per body, anchored at the first dead node: `W-CODE-AFTER-END` — *unreachable content after
`::end` (the walk terminates here)*. It is a warning; promote it with
`lute check --deny W-CODE-AFTER-END`.

The scope is the *immediately enclosing* sequence only — one section body, one `<choice>` body, one
`<when>` arm, one objective body, one `<on>` body. An `::end` in one choice says nothing about its
siblings, nor about content after the enclosing `<branch>`, so a per-branch ending is written the
obvious way and the shared tail below it stays live:

```lute
<branch id="ledge">
  <choice id="jump" text="Jump for it">
    @mira: Nothing to it.
    ::end{reason="fell"}
  </choice>
  <choice id="wait" text="Wait for the ladder">
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

## `::jump` and `::label` — forward jumps

`::label{name="…"}` names a position; `::jump{to="…"}` continues the walk there. A jump only goes
**forward** in document order, so the walk stays a DAG — to offer choices again, use a
[`<hub>`](/language/choices-and-hubs/). Labels share one namespace per document; a section's
`{#id}` is identity metadata, not a label, and a `::jump` cannot target it. `::jump` takes
`when=`, and a guarded jump is taken only while its condition holds:

```lute check
---
kind: scene
id: demo.jump
state:
  run.tip: { type: int, default: 0 }
---

## The Counter

@narrator: The diner is quiet.
::jump{to="outro" when="run.tip > 0"}
@narrator: You leave without a word.
::label{name="outro"}
@narrator: The bell over the door rings.
```

`::label` emits no record: the label resolves at compile time, and the jump's `target` is the
`position` of the first record after the label. The guarded jump above compiles to a one-arm
`match` around a `jump`:

```json
{"kind": "jump", "family": "control", "position": "001-0300", "target": "001-0700"}
```

A `to` that names no `::label` in the document is `E-JUMP-UNDEFINED`; a label at or above the jump
is `E-JUMP-BACKWARD`. Content after an unguarded `::jump` in the same body never runs and is
`W-CODE-AFTER-JUMP`. To jump to a line, put a `::label` immediately before it.

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
  <choice id="accept" text="I'll keep it calm">
    ::accept{quest="calmTheShed"}
    @vesna: Thank you.
  </choice>
  <choice id="decline" text="Not now">
    @vesna: Another time, then.
  </choice>
</branch>
```

Like `::set` and `::assert`, it is built in, not plugin vocabulary: it lowers to its own IR record,
`{"kind": "accept", "family": "declaration", "position": …, "quest": "calmTheShed"}`, and the engine activates the quest if it is
still `unset` and ignores the record otherwise. The target is checked twice: a missing or
malformed `quest` is `E-ACCEPT-TARGET` in `lute check`, and `check-project` reports
`E-ACCEPT-TARGET` when the id names no quest in the project or names a quest that has a `start`
predicate (such a quest activates itself; accepting it means nothing), and, since 0.24.0, a child
quest that activates with its parent (the message names the parent). See
[Quests & scenes](/language/quests-and-scenes/#quests-meet-scenes-and-occasions).

Two 0.24.0 additions sit on the same directive. A child quest declared `activate="accept"` waits
for an `::accept` and activates only while its parent is active. An accept that arrives while
the parent is not active is spent without effect, and the toolchain says so (`lute play` prints
`note: accept of quest c spent — its parent quest p is not active yet …`). `::accept{quest="…" at="nextRun"}`
queues the acceptance until just after the next `newRun` reset, so a run-tier quest taken at a hub
between runs survives that reset. `nextRun` is the only value `at` takes, and any other is
`E-ACCEPT-TARGET`. Both are described in [Quests & scenes](/language/quests-and-scenes/).

## Guarding a directive: `when=`

`::set{… when="<condition>"}` has always been skipped when its condition is false. Since dsl
0.26.0 §4, `::use`, `::accept`, `::assert`, `::retract`, and plugin passthrough and bridge
directives take the same `when=`, with the same meaning:

```lute check
---
kind: scene
id: dock.recruiter
state:
  run.pitchHeard: { type: bool, default: false }
entities:
  item: { members: [nugget] }
relations:
  hasItem: { args: [item], tier: run }
---

## The Dock

@recruiter: Think it over. The pay is good.
::assert{hasItem(nugget) when="!run.pitchHeard"}
::set{run.pitchHeard = true}
::accept{quest="recruiterJob" when="holds('hasItem', ['nugget'])"}
@narrator{when="holds('hasItem', ['nugget'])"}: The nugget is heavy in your pocket.
```

A guarded directive compiles to a one-arm `match` around it: when the condition is false, it is
skipped and the walk goes on. A guarded [`::use`](/language/components-and-extends/#guarding-a-use-when)
runs its whole expansion or none of it, and its argument reads are judged under its guard. A plugin
directive is guarded the same way, `::give{item="nugget" when="!run.pitchHeard"}`, which is how a
repeatable beat hands out a one-time gift.

For the checker a guarded directive is never a definite effect: a guarded `::assert` is not a fact
that holds on every route, a guarded `::set` is not a definite write, and a guarded `::accept` is
not a certain acceptance. That is why the last line above is not `W-FACT-GUARANTEED`. `lute trace`,
`lute test` and `lute play` apply the same writes, facts and accepts, and `lute play` prints each
skip where it happened, as it does a skipped `::set`:

```
  skip ::give{item="potion"} — when: false
  skip ::use{component="trainerBattle" who="lassMina" …} — when: false
```

A directive that lowers to a builtin record runs where it stands and refuses `when=`
(`E-UNKNOWN-ATTR`): every staging directive in the [table above](#core-vocabulary), `::end`,
`::label`, and a plugin directive with a declarative [`lower:` record](/plugins/manifests/#declarative-lowering).
Put such a directive in a `<match>` instead. A clip inside a
[`<track>`](/language/timeline-and-property-tracks/) refuses it too (`E-TIMELINE-CONTENT`), since a
conditional clip is logic:

<!-- lute-diagnostics unverified="verbatim lute check output; the message's source names E-UNKNOWN-ATTR through a constant rather than a string literal, so the scraper cannot pair quote and code" -->
```
./scenes/dock.lute:14:28: error [E-UNKNOWN-ATTR] `::bg` cannot take `when=`: it lowers to a builtin record and runs where it stands — put it in a `<match>`; `when=` guards `::use`, `::accept`, `::assert`, `::retract`, `::set` and plugin passthrough directives
```

A plugin that declares an attribute named `when` collides with the guard, and a use of that
attribute is `E-UNKNOWN-ATTR` asking to rename it in the plugin.
